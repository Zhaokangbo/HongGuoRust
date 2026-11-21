//! User-selected local MP4 library. This module never scans or deletes media files.
//! Paths and metadata remain on the owner's machine in LOCALAPPDATA.

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

const SCHEMA: u32 = 1;
const MAX_ITEMS: usize = 1000;
const MAX_DB_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredMedia {
    id: u64,
    title: String,
    path: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryData {
    schema_version: u32,
    next_id: u64,
    items: Vec<StoredMedia>,
}

impl Default for LibraryData {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            next_id: 1,
            items: Vec::new(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMedia {
    id: u64,
    title: String,
    filename: String,
    available: bool,
}

impl From<&StoredMedia> for LocalMedia {
    fn from(item: &StoredMedia) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            filename: item
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            available: crate::player::engine::local_mp4(&item.path).is_ok(),
        }
    }
}

pub struct LocalLibrary {
    file: PathBuf,
    data: Mutex<LibraryData>,
    load_error: Option<String>,
}

fn read_data(path: &Path) -> Result<LibraryData, String> {
    if !path.exists() {
        return Ok(LibraryData::default());
    }
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_DB_BYTES {
        return Err("本地媒体目录超出大小限制，请先备份后检查 library.json".into());
    }
    let json = fs::read(path).map_err(|e| e.to_string())?;
    let db: LibraryData = serde_json::from_slice(&json)
        .map_err(|e| format!("本地媒体目录无法解析，请备份 library.json：{e}"))?;
    if db.schema_version != SCHEMA || db.items.len() > MAX_ITEMS {
        return Err("本地媒体目录版本或容量不受支持，请先备份 library.json".into());
    }
    Ok(db)
}

fn store_data(path: &Path, data: &LibraryData) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid library path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DB_BYTES {
        return Err("本地媒体目录超出大小限制".into());
    }
    let pending = path.with_extension("json.pending");
    let mut file = File::create(&pending).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let src: Vec<u16> = pending.as_os_str().encode_wide().chain(Some(0)).collect();
    let dst: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
    if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), flags) } == 0 {
        return Err(format!(
            "无法安全保存媒体目录：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

impl LocalLibrary {
    pub fn new() -> Self {
        let location = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("HongGuoRust").join("library.json"));
        match location {
            Some(path) => Self::from_path(path),
            None => Self {
                file: PathBuf::new(),
                data: Mutex::new(LibraryData::default()),
                load_error: Some("找不到 LOCALAPPDATA，不能持久化本地媒体目录".into()),
            },
        }
    }

    fn from_path(path: PathBuf) -> Self {
        let (data, load_error) = match read_data(&path) {
            Ok(db) => (db, None),
            Err(error) => (LibraryData::default(), Some(error)),
        };
        Self {
            file: path,
            data: Mutex::new(data),
            load_error,
        }
    }

    fn ready(&self) -> Result<(), String> {
        match &self.load_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    pub fn list(&self) -> Result<Vec<LocalMedia>, String> {
        self.ready()?;
        let data = self.data.lock().map_err(|e| e.to_string())?;
        Ok(data.items.iter().rev().map(LocalMedia::from).collect())
    }

    pub fn add(&self, selected: PathBuf) -> Result<LocalMedia, String> {
        self.ready()?;
        // Keep the user-selected DOS path. Windows canonicalize() can return a
        // verbatim \\?\ prefix, which the existing M2 local_mp4 guard rejects.
        // Canonicalization is used only for stable duplicate identity.
        let path = selected;
        crate::player::engine::local_mp4(&path)?;
        let identity = fs::canonicalize(&path).map_err(|e| e.to_string())?;
        let title = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut data = self.data.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = data.items.iter().find(|item| {
            item.path
                .to_string_lossy()
                .eq_ignore_ascii_case(&path.to_string_lossy())
                || fs::canonicalize(&item.path)
                    .is_ok_and(|existing| existing == identity)
        }) {
            return Ok(LocalMedia::from(existing));
        }
        if data.items.len() >= MAX_ITEMS {
            return Err("媒体目录最多支持 1000 个条目".into());
        }
        let mut next = data.clone();
        let id = next.next_id;
        next.next_id = id.checked_add(1).ok_or("媒体目录 ID 已用尽")?;
        let item = StoredMedia { id, title, path };
        let summary = LocalMedia::from(&item);
        next.items.push(item);
        store_data(&self.file, &next)?;
        *data = next;
        Ok(summary)
    }

    pub fn remove(&self, id: u64) -> Result<(), String> {
        self.ready()?;
        let mut data = self.data.lock().map_err(|e| e.to_string())?;
        let mut next = data.clone();
        next.items.retain(|item| item.id != id);
        if next.items.len() == data.items.len() {
            return Err("媒体条目不存在".into());
        }
        store_data(&self.file, &next)?;
        *data = next;
        Ok(())
    }

    pub fn resolve_path(&self, id: u64) -> Result<PathBuf, String> {
        self.ready()?;
        let data = self.data.lock().map_err(|e| e.to_string())?;
        let path = data
            .items
            .iter()
            .find(|item| item.id == id)
            .ok_or("媒体条目不存在")?
            .path
            .clone();
        crate::player::engine::local_mp4(&path)
            .map_err(|_| "文件不存在、不是 MP4 或不是支持的本地盘文件".to_owned())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::LocalLibrary;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("hgr-library-{}-{unique}", std::process::id()))
    }

    #[test]
    fn add_deduplicate_restart_remove_preserves_original() {
        let dir = test_dir();
        fs::create_dir_all(&dir).unwrap();
        let media = dir.join("我的短片.MP4");
        fs::write(&media, b"test").unwrap();
        let db = dir.join("library.json");

        let library = LocalLibrary::from_path(db.clone());
        let first = library.add(media.clone()).unwrap();
        assert_eq!(first.title, "我的短片");
        assert_eq!(library.add(media.clone()).unwrap().id, first.id);
        assert_eq!(library.list().unwrap().len(), 1);
        drop(library);

        let reopened = LocalLibrary::from_path(db);
        assert_eq!(reopened.list().unwrap().len(), 1);
        // Persist the original DOS path instead of the Windows verbatim path.
        assert_eq!(reopened.resolve_path(first.id).unwrap(), media);
        reopened.remove(first.id).unwrap();
        assert!(media.exists());
        assert!(reopened.list().unwrap().is_empty());
        drop(reopened);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_media_and_corrupt_db_do_not_destroy_user_data() {
        let dir = test_dir();
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("library.json");
        let media = dir.join("example.mp4");
        fs::write(&media, b"test").unwrap();
        let library = LocalLibrary::from_path(db.clone());
        let id = library.add(media.clone()).unwrap().id;
        fs::remove_file(&media).unwrap();
        assert!(!library.list().unwrap()[0].available);
        assert!(library.resolve_path(id).is_err());
        drop(library);

        fs::write(&db, b"not-json").unwrap();
        let corrupted = LocalLibrary::from_path(db.clone());
        assert!(corrupted.list().is_err());
        assert!(corrupted.remove(id).is_err());
        assert_eq!(fs::read(&db).unwrap(), b"not-json");
        drop(corrupted);
        fs::remove_dir_all(dir).unwrap();
    }
}
