//! User-selected local MP4 library. This module never scans or deletes media files.
//! Paths and metadata remain on the owner's machine in LOCALAPPDATA.

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
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
    #[serde(default)]
    history: PlaybackHistory,
    #[serde(default)]
    favorite: bool,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaybackHistory {
    position_sec: f64,
    duration_sec: Option<f64>,
    last_played_ms: Option<u64>,
    completed: bool,
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
    position_sec: f64,
    duration_sec: Option<f64>,
    last_played_ms: Option<u64>,
    completed: bool,
    favorite: bool,
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
            position_sec: item.history.position_sec,
            duration_sec: item.history.duration_sec,
            last_played_ms: item.history.last_played_ms,
            completed: item.history.completed,
            favorite: item.favorite,
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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
                || fs::canonicalize(&item.path).is_ok_and(|existing| existing == identity)
        }) {
            return Ok(LocalMedia::from(existing));
        }
        if data.items.len() >= MAX_ITEMS {
            return Err("媒体目录最多支持 1000 个条目".into());
        }
        let mut next = data.clone();
        let id = next.next_id;
        next.next_id = id.checked_add(1).ok_or("媒体目录 ID 已用尽")?;
        let item = StoredMedia {
            id,
            title,
            path,
            history: PlaybackHistory::default(),
            favorite: false,
        };
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

    pub fn resume_point(&self, id: u64) -> Result<Option<f64>, String> {
        self.ready()?;
        let data = self.data.lock().map_err(|e| e.to_string())?;
        let item = data.items.iter().find(|item| item.id == id);
        let item = item.ok_or("媒体条目不存在")?;
        let history = &item.history;
        if history.completed || !history.position_sec.is_finite() {
            return Ok(None);
        }
        let sec = history.position_sec;
        if sec < 5.0
            || history
                .duration_sec
                .is_some_and(|d| !d.is_finite() || d < 10.0 || sec >= d - 5.0 || sec >= d * 0.95)
        {
            return Ok(None);
        }
        Ok(Some(sec))
    }

    pub fn mark_started(&self, id: u64) -> Result<(), String> {
        self.ready()?;
        let mut data = self.data.lock().map_err(|e| e.to_string())?;
        let mut next = data.clone();
        let item = next
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("媒体条目不存在")?;
        if item.history.completed {
            item.history.position_sec = 0.0;
        }
        item.history.completed = false;
        item.history.last_played_ms = Some(now_ms());
        store_data(&self.file, &next)?;
        *data = next;
        Ok(())
    }

    pub fn record_progress(
        &self,
        id: u64,
        position: Option<f64>,
        duration: Option<f64>,
        completed: bool,
    ) -> Result<(), String> {
        self.ready()?;
        let mut data = self.data.lock().map_err(|e| e.to_string())?;
        let mut next = data.clone();
        let item = next
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("媒体条目不存在")?;
        if let Some(sec) = position.filter(|v| v.is_finite() && *v >= 0.0) {
            item.history.position_sec = sec;
        }
        if let Some(sec) = duration.filter(|v| v.is_finite() && *v > 0.0) {
            item.history.duration_sec = Some(sec);
        }
        if completed {
            item.history.completed = true;
            if let Some(sec) = item.history.duration_sec {
                item.history.position_sec = sec;
            }
        }
        item.history.last_played_ms = Some(now_ms());
        store_data(&self.file, &next)?;
        *data = next;
        Ok(())
    }

    pub fn set_favorite(&self, id: u64, favorite: bool) -> Result<(), String> {
        self.ready()?;
        let mut data = self.data.lock().map_err(|e| e.to_string())?;
        let item = data
            .items
            .iter()
            .find(|item| item.id == id)
            .ok_or("媒体条目不存在")?;
        if item.favorite == favorite {
            return Ok(());
        }
        let mut next = data.clone();
        let item = next
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("媒体条目不存在")?;
        item.favorite = favorite;
        store_data(&self.file, &next)?;
        *data = next;
        Ok(())
    }

    pub fn clear_history(&self, id: u64) -> Result<(), String> {
        self.ready()?;
        let mut data = self.data.lock().map_err(|e| e.to_string())?;
        let mut next = data.clone();
        let item = next
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("媒体条目不存在")?;
        item.history = PlaybackHistory::default();
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
    fn legacy_library_schema_and_resume_bookmark() {
        let dir = test_dir();
        fs::create_dir_all(&dir).unwrap();
        let media = dir.join("legacy.mp4");
        fs::write(&media, b"test").unwrap();
        let file = dir.join("library.json");
        let legacy = serde_json::json!({
            "schemaVersion": 1,
            "nextId": 2,
            "items": [{"id": 1, "title": "legacy", "path": media}]
        });
        fs::write(&file, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let library = LocalLibrary::from_path(file.clone());
        assert_eq!(library.resume_point(1).unwrap(), None);
        library.mark_started(1).unwrap();
        library
            .record_progress(1, Some(40.0), Some(120.0), false)
            .unwrap();
        drop(library);

        let reopened = LocalLibrary::from_path(file);
        assert_eq!(reopened.resume_point(1).unwrap(), Some(40.0));
        reopened
            .record_progress(1, Some(117.0), Some(120.0), true)
            .unwrap();
        assert!(reopened.list().unwrap()[0].completed);
        assert!(reopened.resume_point(1).unwrap().is_none());
        reopened.mark_started(1).unwrap();
        assert_eq!(reopened.list().unwrap()[0].position_sec, 0.0);
        reopened.record_progress(1, Some(4.0), None, false).unwrap();
        assert!(reopened.resume_point(1).unwrap().is_none());
        reopened.record_progress(1, Some(8.0), None, false).unwrap();
        assert_eq!(reopened.resume_point(1).unwrap(), Some(8.0));
        reopened.clear_history(1).unwrap();
        assert!(reopened.list().unwrap()[0].last_played_ms.is_none());
        assert!(media.exists());
        drop(reopened);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn favorites_survive_restart_and_do_not_clear_playback_history() {
        let dir = test_dir();
        fs::create_dir_all(&dir).unwrap();
        let media = dir.join("favorite.mp4");
        fs::write(&media, b"test").unwrap();
        let file = dir.join("library.json");
        let library = LocalLibrary::from_path(file.clone());
        let id = library.add(media.clone()).unwrap().id;
        library
            .record_progress(id, Some(18.0), Some(120.0), false)
            .unwrap();
        library.set_favorite(id, true).unwrap();
        assert!(library.list().unwrap()[0].favorite);
        drop(library);

        let reopened = LocalLibrary::from_path(file);
        assert!(reopened.list().unwrap()[0].favorite);
        assert_eq!(reopened.resume_point(id).unwrap(), Some(18.0));
        reopened.clear_history(id).unwrap();
        assert!(reopened.list().unwrap()[0].favorite);
        reopened.set_favorite(id, false).unwrap();
        assert!(!reopened.list().unwrap()[0].favorite);
        assert!(media.exists());
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
