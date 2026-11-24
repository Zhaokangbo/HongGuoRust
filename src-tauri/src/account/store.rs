//! Windows DPAPI-encrypted persistence for HongGuo cloud account credentials.
//! Credentials stay strictly on the native Rust side and are never sent to WebView2 or logs.

use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::fmt;
use std::fs::{self, File};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

const MAX_SESSION_BYTES: u64 = 256 * 1024;
const SESSION_SCHEMA: u32 = 1;
const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

#[repr(C)]
struct CryptoApiBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(
        data_in: *const CryptoApiBlob,
        data_descr: *const u16,
        optional_entropy: *const CryptoApiBlob,
        reserved: *mut c_void,
        prompt_struct: *const c_void,
        flags: u32,
        data_out: *mut CryptoApiBlob,
    ) -> i32;
    fn CryptUnprotectData(
        data_in: *const CryptoApiBlob,
        data_descr: *mut *mut u16,
        optional_entropy: *const CryptoApiBlob,
        reserved: *mut c_void,
        prompt_struct: *const c_void,
        flags: u32,
        data_out: *mut CryptoApiBlob,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(hmem: *mut c_void) -> *mut c_void;
}

const DPAPI_ENTROPY: &[u8] = b"HongGuoRust-AccountSession-v1";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCredentials {
    pub schema_version: u32,
    pub cookie: String,
    pub device_id: String,
    pub install_id: String,
    pub user_id: String,
    pub nickname: String,
    pub signed_in_at_ms: u64,
}

impl fmt::Debug for AccountCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountCredentials")
            .field("user_id", &"[redacted]")
            .field("nickname", &self.nickname)
            .field("cookie", &"[redacted]")
            .finish()
    }
}

impl AccountCredentials {
    pub fn new(
        cookie: String,
        device_id: String,
        install_id: String,
        user_id: String,
        nickname: String,
        signed_in_at_ms: u64,
    ) -> Self {
        Self {
            schema_version: SESSION_SCHEMA,
            cookie,
            device_id,
            install_id,
            user_id,
            nickname,
            signed_in_at_ms,
        }
    }
}

pub fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    if plain.is_empty() || plain.len() > MAX_SESSION_BYTES as usize {
        return Err("account_crypto: 会话数据体积异常".into());
    }
    let input = CryptoApiBlob {
        cb_data: plain.len() as u32,
        pb_data: plain.as_ptr() as *mut u8,
    };
    let entropy = CryptoApiBlob {
        cb_data: DPAPI_ENTROPY.len() as u32,
        pb_data: DPAPI_ENTROPY.as_ptr() as *mut u8,
    };
    let mut output = CryptoApiBlob {
        cb_data: 0,
        pb_data: null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            &entropy,
            null_mut(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pb_data.is_null() {
        return Err("account_crypto: 加密账号会话失败".into());
    }
    let slice = unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize) };
    let protected = slice.to_vec();
    unsafe {
        LocalFree(output.pb_data.cast::<c_void>());
    }
    Ok(protected)
}

pub fn dpapi_unprotect(cipher: &[u8]) -> Result<Vec<u8>, String> {
    if cipher.is_empty() || cipher.len() > MAX_SESSION_BYTES as usize {
        return Err("account_crypto: 加密会话文件大小无效".into());
    }
    let input = CryptoApiBlob {
        cb_data: cipher.len() as u32,
        pb_data: cipher.as_ptr() as *mut u8,
    };
    let entropy = CryptoApiBlob {
        cb_data: DPAPI_ENTROPY.len() as u32,
        pb_data: DPAPI_ENTROPY.as_ptr() as *mut u8,
    };
    let mut output = CryptoApiBlob {
        cb_data: 0,
        pb_data: null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            &entropy,
            null_mut(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 || output.pb_data.is_null() {
        return Err("account_crypto: 解密账号会话失败".into());
    }
    let slice = unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize) };
    let plain = slice.to_vec();
    unsafe {
        LocalFree(output.pb_data.cast::<c_void>());
    }
    Ok(plain)
}

fn read_session(path: &Path) -> Option<AccountCredentials> {
    if !path.exists() {
        return None;
    }
    let meta = fs::metadata(path).ok()?;
    if meta.len() == 0 || meta.len() > MAX_SESSION_BYTES {
        return None;
    }
    let raw = fs::read(path).ok()?;
    let plain = dpapi_unprotect(&raw).ok()?;
    let creds: AccountCredentials = serde_json::from_slice(&plain).ok()?;
    if creds.schema_version != SESSION_SCHEMA
        || creds.cookie.trim().is_empty()
        || creds.device_id.trim().is_empty()
        || creds.install_id.trim().is_empty()
        || creds.user_id.trim().is_empty()
    {
        return None;
    }
    Some(creds)
}

fn write_session(path: &Path, creds: &AccountCredentials) -> Result<(), String> {
    let parent = path.parent().ok_or("account_store: 无效的账号存储目录")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let plain = serde_json::to_vec(creds).map_err(|e| e.to_string())?;
    let encrypted = dpapi_protect(&plain)?;
    let pending = path.with_extension("bin.pending");
    let mut file = File::create(&pending).map_err(|e| e.to_string())?;
    file.write_all(&encrypted).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let src: Vec<u16> = pending.as_os_str().encode_wide().chain(Some(0)).collect();
    let dst: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            src.as_ptr(),
            dst.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err("account_store: 保存加密会话失败".into());
    }
    Ok(())
}

pub struct AccountStore {
    path: PathBuf,
    creds: Mutex<Option<AccountCredentials>>,
}

impl AccountStore {
    pub fn new() -> Self {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("HongGuoRust");
        Self::from_path(dir.join("account-session.bin"))
    }

    pub fn from_path(path: PathBuf) -> Self {
        let loaded = read_session(&path);
        Self {
            path,
            creds: Mutex::new(loaded),
        }
    }

    pub fn get(&self) -> Option<AccountCredentials> {
        self.creds.lock().ok().and_then(|g| g.clone())
    }

    pub fn save(&self, creds: AccountCredentials) -> Result<(), String> {
        write_session(&self.path, &creds)?;
        if let Ok(mut g) = self.creds.lock() {
            *g = Some(creds);
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<(), String> {
        if let Ok(mut g) = self.creds.lock() {
            *g = None;
        }
        if self.path.exists() {
            let _ = fs::remove_file(&self.path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn dpapi_roundtrip_and_store_persistence_work() {
        let plain = b"synthetic-session-payload";
        let cipher = dpapi_protect(plain).expect("dpapi protect");
        assert_ne!(cipher, plain);
        let decoded = dpapi_unprotect(&cipher).expect("dpapi unprotect");
        assert_eq!(decoded, plain);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("hgr-acct-{}-{unique}", std::process::id()));
        let path = dir.join("account-session.bin");
        let store = AccountStore::from_path(path.clone());
        assert!(store.get().is_none());

        let creds = AccountCredentials::new(
            "sessionid=synthetic".into(),
            "1234567890123456".into(),
            "9876543210987654".into(),
            "888888".into(),
            "测试用户".into(),
            1_800_000_000_000,
        );
        store.save(creds).unwrap();
        let reloaded = AccountStore::from_path(path.clone()).get().unwrap();
        assert_eq!(reloaded.user_id, "888888");
        assert_eq!(reloaded.nickname, "测试用户");
        assert_eq!(reloaded.cookie, "sessionid=synthetic");
        assert!(!format!("{reloaded:?}").contains("sessionid=synthetic"));

        store.clear().unwrap();
        assert!(!path.exists());
        let _ = fs::remove_dir_all(dir);
    }
}
