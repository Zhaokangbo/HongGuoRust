//! M11-C2: bounded local user preferences, separate from library/history.
//! Corrupt or unsupported settings are never overwritten automatically.
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
const MAX_SETTINGS_BYTES: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayerPreferences {
    schema_version: u32,
    pub volume: f64,
    pub last_audible_volume: f64,
    pub playback_rate: f64,
    pub auto_next: bool,
}

impl Default for PlayerPreferences {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            volume: 100.0,
            last_audible_volume: 100.0,
            playback_rate: 1.0,
            auto_next: false,
        }
    }
}

impl PlayerPreferences {
    fn valid(self) -> bool {
        self.schema_version == SCHEMA
            && self.volume.is_finite()
            && (0.0..=100.0).contains(&self.volume)
            && self.last_audible_volume.is_finite()
            && (0.0..=100.0).contains(&self.last_audible_volume)
            && self.last_audible_volume > 0.0
            && self.playback_rate.is_finite()
            && (0.5..=2.0).contains(&self.playback_rate)
    }
}

fn read_preferences(path: &Path) -> Result<PlayerPreferences, String> {
    if !path.exists() {
        return Ok(PlayerPreferences::default());
    }
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > MAX_SETTINGS_BYTES {
        return Err("播放器设置文件过大，请先备份 player-settings.json".into());
    }
    let content = fs::read(path).map_err(|e| e.to_string())?;
    let data: PlayerPreferences = serde_json::from_slice(&content)
        .map_err(|e| format!("播放器设置文件无法解析，请先备份 player-settings.json：{e}"))?;
    if !data.valid() {
        return Err("播放器设置版本或参数不受支持，请先备份 player-settings.json".into());
    }
    Ok(data)
}

fn write_preferences(path: &Path, preferences: PlayerPreferences) -> Result<(), String> {
    if !preferences.valid() {
        return Err("播放器设置参数不合法".into());
    }
    let parent = path.parent().ok_or("Invalid preferences path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&preferences).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return Err("播放器设置文件超出大小限制".into());
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
            "无法安全保存播放器设置：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub struct PreferencesStore {
    path: PathBuf,
    value: Mutex<PlayerPreferences>,
    load_error: Option<String>,
}

impl PreferencesStore {
    pub fn new() -> Self {
        let location = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("HongGuoRust").join("player-settings.json"));
        match location {
            Some(path) => Self::from_path(path),
            None => Self {
                path: PathBuf::new(),
                value: Mutex::new(PlayerPreferences::default()),
                load_error: Some("找不到 LOCALAPPDATA，无法保存播放器设置".into()),
            },
        }
    }

    pub(crate) fn from_path(path: PathBuf) -> Self {
        let (value, load_error) = match read_preferences(&path) {
            Ok(data) => (data, None),
            Err(error) => (PlayerPreferences::default(), Some(error)),
        };
        Self {
            path,
            value: Mutex::new(value),
            load_error,
        }
    }

    pub fn snapshot(&self) -> Result<PlayerPreferences, String> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        Ok(*self.value.lock().map_err(|e| e.to_string())?)
    }

    /// Atomic copy-on-write update. Persist first, commit the in-memory value
    /// only after MoveFileExW succeeds; old data is not overwritten on errors.
    fn update(&self, apply: impl FnOnce(&mut PlayerPreferences)) -> Result<PlayerPreferences, String> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        let mut locked = self.value.lock().map_err(|e| e.to_string())?;
        let mut next = *locked;
        apply(&mut next);
        if !next.valid() {
            return Err("播放器设置参数不合法".into());
        }
        if next != *locked {
            write_preferences(&self.path, next)?;
            *locked = next;
        }
        Ok(next)
    }

    pub fn set_volume(&self, volume: f64) -> Result<PlayerPreferences, String> {
        self.update(|data| {
            data.volume = volume;
            if volume > 0.0 {
                data.last_audible_volume = volume;
            }
        })
    }

    pub fn set_rate(&self, rate: f64) -> Result<PlayerPreferences, String> {
        self.update(|data| data.playback_rate = rate)
    }

    pub fn set_auto_next(&self, value: bool) -> Result<PlayerPreferences, String> {
        self.update(|data| data.auto_next = value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("hongguorust-settings-{}-{unique}", std::process::id()))
    }

    #[test]
    fn default_and_restart_persist_every_field() {
        let dir = fixture();
        let path = dir.join("player-settings.json");
        let store = PreferencesStore::from_path(path.clone());
        assert_eq!(store.snapshot().unwrap(), PlayerPreferences::default());
        store.set_rate(1.5).unwrap();
        store.set_volume(50.0).unwrap();
        store.set_volume(0.0).unwrap();
        store.set_auto_next(true).unwrap();
        drop(store);

        let opened = PreferencesStore::from_path(path.clone());
        let p = opened.snapshot().unwrap();
        assert_eq!(p.volume, 0.0);
        assert_eq!(p.last_audible_volume, 50.0);
        assert_eq!(p.playback_rate, 1.5);
        assert!(p.auto_next);
        assert_eq!(p.schema_version, SCHEMA);
        assert!(!path.with_extension("json.pending").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_input_never_changes_file_or_memory() {
        let dir = fixture();
        let path = dir.join("player-settings.json");
        let store = PreferencesStore::from_path(path.clone());
        store.set_volume(25.0).unwrap();
        let original = fs::read(&path).unwrap();
        for n in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
            assert!(store.set_volume(n).is_err());
        }
        for n in [f64::NAN, f64::INFINITY, 0.49, 2.01] {
            assert!(store.set_rate(n).is_err());
        }
        assert_eq!(store.snapshot().unwrap().volume, 25.0);
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupted_or_future_schema_is_not_overwritten() {
        let dir = fixture();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("player-settings.json");
        for invalid in [
            b"not JSON".to_vec(),
            br#"{"schemaVersion":2,"volume":25,"lastAudibleVolume":25,"playbackRate":1,"autoNext":false}"#.to_vec(),
            br#"{"schemaVersion":1,"volume":25,"lastAudibleVolume":25,"playbackRate":4,"autoNext":false}"#.to_vec(),
        ] {
            fs::write(&path, &invalid).unwrap();
            let store = PreferencesStore::from_path(path.clone());
            assert!(store.snapshot().is_err());
            assert!(store.set_rate(1.0).is_err());
            assert_eq!(fs::read(&path).unwrap(), invalid);
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
