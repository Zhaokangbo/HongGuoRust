//! M10-C1: independent provider episode bookmarks. Never persist playback URLs.
//! M7's library.json and its local-file playback history are left untouched.
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

const SCHEMA: u32 = 1;
const MAX_ITEMS: usize = 2000;
const MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProgress {
    series_id: String,
    episode_id: String,
    // Change detector only; never use it as an authentication/security token.
    source_fingerprint: String,
    position_sec: f64,
    duration_sec: Option<f64>,
    last_played_ms: u64,
    completed: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgressData {
    schema_version: u32,
    items: Vec<StoredProgress>,
}

impl Default for ProgressData {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA,
            items: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEpisodeProgress {
    pub series_id: String,
    pub episode_id: String,
    pub position_sec: f64,
    pub duration_sec: Option<f64>,
    pub last_played_ms: u64,
    pub completed: bool,
}

impl From<&StoredProgress> for ProviderEpisodeProgress {
    fn from(item: &StoredProgress) -> Self {
        Self {
            series_id: item.series_id.clone(),
            episode_id: item.episode_id.clone(),
            position_sec: item.position_sec,
            duration_sec: item.duration_sec,
            last_played_ms: item.last_played_ms,
            completed: item.completed,
        }
    }
}

pub struct ProviderProgressStore {
    path: PathBuf,
    state: Mutex<ProgressData>,
    load_error: Option<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Digest-like change detector. This is deliberately not used for authorization
/// or content trust, only to invalidate stale bookmarks when an owned URL changes.
fn fingerprint(url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn valid_id(id: &str, max: usize) -> bool {
    !id.is_empty() && id.len() <= max && !id.chars().any(char::is_control)
}

fn read_data(path: &Path) -> Result<ProgressData, String> {
    if !path.exists() {
        return Ok(ProgressData::default());
    }
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("剧集历史文件超出安全大小，请先备份".into());
    }
    let data: ProgressData = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("剧集历史文件无法解析，请先备份：{e}"))?;
    if data.schema_version != SCHEMA
        || data.items.len() > MAX_ITEMS
        || data.items.iter().any(|x| {
            !valid_id(&x.series_id, 161)
                || !valid_id(&x.episode_id, 128)
                || x.source_fingerprint.len() != 16
                || !x.source_fingerprint.bytes().all(|c| c.is_ascii_hexdigit())
                || !x.position_sec.is_finite()
                || x.position_sec < 0.0
                || x.duration_sec.is_some_and(|d| !d.is_finite() || d <= 0.0)
        })
    {
        return Err("剧集历史文件格式无效，请先备份".into());
    }
    let mut identities = std::collections::HashSet::new();
    if data
        .items
        .iter()
        .any(|x| !identities.insert((&x.series_id, &x.episode_id)))
    {
        return Err("剧集历史文件包含重复条目，请先备份".into());
    }
    Ok(data)
}

fn write_data(path: &Path, data: &ProgressData) -> Result<(), String> {
    let parent = path.parent().ok_or("剧集历史目录无效")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(data).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("剧集历史文件超过容量限制".into());
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
            "保存剧集历史失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

impl ProviderProgressStore {
    pub fn new() -> Self {
        match std::env::var_os("LOCALAPPDATA") {
            Some(root) => Self::from_path(
                PathBuf::from(root)
                    .join("HongGuoRust")
                    .join("provider-progress.json"),
            ),
            None => Self {
                path: PathBuf::new(),
                state: Mutex::new(ProgressData::default()),
                load_error: Some("无法找到本机应用数据目录".into()),
            },
        }
    }

    fn from_path(path: PathBuf) -> Self {
        let (state, load_error) = match read_data(&path) {
            Ok(data) => (data, None),
            Err(e) => (ProgressData::default(), Some(e)),
        };
        Self {
            path,
            state: Mutex::new(state),
            load_error,
        }
    }

    fn ready(&self) -> Result<(), String> {
        if let Some(e) = &self.load_error {
            Err(e.clone())
        } else {
            Ok(())
        }
    }

    pub fn get(
        &self,
        series_id: &str,
        episode_id: &str,
        url: &str,
    ) -> Result<Option<ProviderEpisodeProgress>, String> {
        self.ready()?;
        let hash = fingerprint(url);
        let state = self.state.lock().map_err(|e| e.to_string())?;
        Ok(state
            .items
            .iter()
            .find(|x| {
                x.series_id == series_id
                    && x.episode_id == episode_id
                    && x.source_fingerprint == hash
            })
            .map(ProviderEpisodeProgress::from))
    }

    pub fn resume_point(
        &self,
        series_id: &str,
        episode_id: &str,
        url: &str,
    ) -> Result<Option<f64>, String> {
        Ok(self.get(series_id, episode_id, url)?.and_then(|record| {
            if record.completed {
                return None;
            }
            let sec = record.position_sec;
            if sec < 5.0 {
                return None;
            }
            if record
                .duration_sec
                .is_some_and(|duration| sec >= duration - 5.0)
            {
                return None;
            }
            Some(sec)
        }))
    }

    fn update(
        &self,
        series_id: &str,
        episode_id: &str,
        url_fingerprint: &str,
        position: Option<f64>,
        duration: Option<f64>,
        completed: bool,
        started: bool,
    ) -> Result<(), String> {
        self.ready()?;
        if !valid_id(series_id, 161) || !valid_id(episode_id, 128) {
            return Err("非法的剧集历史标识".into());
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let mut next = state.clone();
        let item = if let Some(index) = next
            .items
            .iter()
            .position(|x| x.series_id == series_id && x.episode_id == episode_id)
        {
            &mut next.items[index]
        } else {
            if next.items.len() >= MAX_ITEMS {
                let oldest = next
                    .items
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, item)| item.last_played_ms)
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                next.items.remove(oldest);
            }
            next.items.push(StoredProgress {
                series_id: series_id.into(),
                episode_id: episode_id.into(),
                source_fingerprint: url_fingerprint.into(),
                position_sec: 0.0,
                duration_sec: None,
                last_played_ms: 0,
                completed: false,
            });
            next.items.last_mut().unwrap()
        };
        if item.source_fingerprint != url_fingerprint {
            // Same manifest IDs but different media source: never seek into the old clip.
            item.source_fingerprint = url_fingerprint.into();
            item.position_sec = 0.0;
            item.duration_sec = None;
            item.completed = false;
        }
        if started {
            if item.completed {
                item.position_sec = 0.0;
            }
            item.completed = false;
        }
        if let Some(p) = position.filter(|p| p.is_finite() && *p >= 0.0) {
            item.position_sec = p;
        }
        if let Some(d) = duration.filter(|d| d.is_finite() && *d > 0.0) {
            item.duration_sec = Some(d);
        }
        if completed {
            item.completed = true;
            if let Some(d) = item.duration_sec {
                item.position_sec = d;
            }
        }
        item.last_played_ms = now_ms();
        write_data(&self.path, &next)?;
        *state = next;
        Ok(())
    }

    pub fn mark_started(
        &self,
        series_id: &str,
        episode_id: &str,
        source: &str,
    ) -> Result<(), String> {
        self.update(
            series_id,
            episode_id,
            &fingerprint(source),
            None,
            None,
            false,
            true,
        )
    }

    pub fn save_progress(
        &self,
        series_id: &str,
        episode_id: &str,
        source: &str,
        position: Option<f64>,
        duration: Option<f64>,
        completed: bool,
    ) -> Result<(), String> {
        self.update(
            series_id,
            episode_id,
            &fingerprint(source),
            position,
            duration,
            completed,
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "hgr-provider-{}-{:?}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join("provider-progress.json")
    }

    #[test]
    fn bookmarks_survive_reopen_and_are_namespaced() {
        let path = test_path();
        let store = ProviderProgressStore::from_path(path.clone());
        store
            .mark_started("demo:same", "ep-1", "https://example.com/a.mp4")
            .unwrap();
        store
            .save_progress(
                "demo:same",
                "ep-1",
                "https://example.com/a.mp4",
                Some(40.0),
                Some(100.0),
                false,
            )
            .unwrap();
        drop(store);
        let store = ProviderProgressStore::from_path(path.clone());
        assert_eq!(
            store
                .resume_point("demo:same", "ep-1", "https://example.com/a.mp4")
                .unwrap(),
            Some(40.0)
        );
        assert_eq!(
            store
                .resume_point("manifest:same", "ep-1", "https://example.com/a.mp4")
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .resume_point("demo:same", "ep-2", "https://example.com/a.mp4")
                .unwrap(),
            None
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn changed_source_and_completed_episode_never_seek_to_old_position() {
        let path = test_path();
        let store = ProviderProgressStore::from_path(path.clone());
        store
            .mark_started("manifest:series", "ep", "https://example.com/old.mp4")
            .unwrap();
        store
            .save_progress(
                "manifest:series",
                "ep",
                "https://example.com/old.mp4",
                Some(35.0),
                Some(100.0),
                false,
            )
            .unwrap();
        assert!(store
            .get("manifest:series", "ep", "https://example.com/new.mp4")
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .resume_point("manifest:series", "ep", "https://example.com/new.mp4")
                .unwrap(),
            None
        );
        store
            .mark_started("manifest:series", "ep", "https://example.com/new.mp4")
            .unwrap();
        assert_eq!(
            store
                .resume_point("manifest:series", "ep", "https://example.com/new.mp4")
                .unwrap(),
            None
        );
        store
            .save_progress(
                "manifest:series",
                "ep",
                "https://example.com/new.mp4",
                Some(95.0),
                Some(100.0),
                true,
            )
            .unwrap();
        assert!(
            store
                .get("manifest:series", "ep", "https://example.com/new.mp4")
                .unwrap()
                .unwrap()
                .completed
        );
        assert_eq!(
            store
                .resume_point("manifest:series", "ep", "https://example.com/new.mp4")
                .unwrap(),
            None
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn corrupt_file_is_not_overwritten() {
        let path = test_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"corrupt").unwrap();
        let store = ProviderProgressStore::from_path(path.clone());
        assert!(store
            .mark_started("demo:series", "ep", "https://example.com/a")
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"corrupt");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
