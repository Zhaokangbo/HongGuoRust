//! Persistent user favorites for catalog series (WebDAV, remote/local manifests, etc.).
//! Stores metadata only; never stores playback URLs or credentials.
use super::Series;
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
const MAX_FAVORITES: usize = 500;
const MAX_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteSeries {
    pub id: String,
    pub title: String,
    pub description: String,
    pub genre: String,
    pub badge: String,
    pub artwork: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    pub episode_count: usize,
    pub provider_id: String,
    pub added_at_ms: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoritesFile {
    schema_version: u32,
    items: Vec<FavoriteSeries>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn read_file(path: &Path) -> Vec<FavoriteSeries> {
    if !path.exists() {
        return Vec::new();
    }
    let Ok(meta) = fs::metadata(path) else {
        return Vec::new();
    };
    if meta.len() > MAX_BYTES {
        return Vec::new();
    }
    let Ok(bytes) = fs::read(path) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_slice::<FavoritesFile>(&bytes) else {
        return Vec::new();
    };
    if parsed.schema_version != SCHEMA {
        return Vec::new();
    }
    let mut items: Vec<FavoriteSeries> = parsed
        .items
        .into_iter()
        .filter(|item| !item.id.is_empty() && !item.title.is_empty())
        .take(MAX_FAVORITES)
        .collect();
    items.sort_by(|a, b| b.added_at_ms.cmp(&a.added_at_ms));
    items
}

fn persist_file(path: &Path, items: &[FavoriteSeries]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("favorites_storage: 无法确定收藏数据目录")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let payload = FavoritesFile {
        schema_version: SCHEMA,
        items: items.to_vec(),
    };
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("favorites_limit: 收藏数据超过大小限制".into());
    }
    let pending = path.with_extension("json.pending");
    let mut file = File::create(&pending).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
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
        return Err(format!(
            "favorites_write: 保存收藏失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

pub struct FavoritesStore {
    path: PathBuf,
    items: Mutex<Vec<FavoriteSeries>>,
}

impl FavoritesStore {
    pub fn new() -> Self {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("HongGuoRust");
        Self::from_path(dir.join("favorites.json"))
    }

    fn from_path(path: PathBuf) -> Self {
        let items = read_file(&path);
        Self {
            path,
            items: Mutex::new(items),
        }
    }

    pub fn list(&self) -> Result<Vec<FavoriteSeries>, String> {
        self.items
            .lock()
            .map(|guard| guard.clone())
            .map_err(|e| e.to_string())
    }

    pub fn add(&self, series: Series) -> Result<Vec<FavoriteSeries>, String> {
        let mut guard = self.items.lock().map_err(|e| e.to_string())?;
        guard.retain(|item| item.id != series.id);
        if guard.len() >= MAX_FAVORITES {
            return Err("favorites_limit: 收藏数量已达上限（500 部）".into());
        }
        guard.insert(
            0,
            FavoriteSeries {
                id: series.id,
                title: series.title,
                description: series.description,
                genre: series.genre,
                badge: series.badge,
                artwork: series.artwork,
                cover_url: series.cover_url,
                episode_count: series.episode_count,
                provider_id: series.provider_id,
                added_at_ms: now_ms(),
            },
        );
        persist_file(&self.path, &guard)?;
        Ok(guard.clone())
    }

    pub fn remove(&self, series_id: &str) -> Result<Vec<FavoriteSeries>, String> {
        let mut guard = self.items.lock().map_err(|e| e.to_string())?;
        let before = guard.len();
        guard.retain(|item| item.id != series_id);
        if guard.len() != before {
            persist_file(&self.path, &guard)?;
        }
        Ok(guard.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_persist_and_deduplicate() {
        let dir =
            std::env::temp_dir().join(format!("hgr-favs-{}-{}", std::process::id(), now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("favorites.json");
        let store = FavoritesStore::from_path(path.clone());
        let sample = Series {
            id: "manifest:s1".into(),
            title: "测试短剧".into(),
            description: "简介".into(),
            genre: "悬疑".into(),
            badge: "剧集清单".into(),
            artwork: "studio".into(),
            cover_url: None,
            episode_count: 10,
            provider_id: "manifest".into(),
        };
        assert_eq!(store.add(sample.clone()).unwrap().len(), 1);
        assert_eq!(store.add(sample).unwrap().len(), 1);

        let reloaded = FavoritesStore::from_path(path);
        assert_eq!(reloaded.list().unwrap().len(), 1);
        assert_eq!(reloaded.remove("manifest:s1").unwrap().len(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
