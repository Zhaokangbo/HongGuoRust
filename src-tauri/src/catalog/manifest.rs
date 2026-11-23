//! Explicitly user-selected, locally persisted catalog of direct public HTTPS MP4s.
//! No directory traversal, remote search, scraping or arbitrary file paths.
//! Only manifest metadata is exposed via the catalog API, never playback URLs.
use super::http_client;
use super::{ContentProvider, Episode, MediaTarget, Series, SeriesDetail};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

const SCHEMA: u32 = 1;
const MAX_BYTES: u64 = 512 * 1024;
const MAX_SERIES: usize = 100;
const MAX_EPISODES: usize = 500;
const MAX_EPISODES_PER_SERIES: usize = 100;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionMeta {
    url: Option<String>,
    last_synced_ms: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSubscriptionInfo {
    pub url: Option<String>,
    pub series_count: usize,
    pub last_synced_ms: Option<u64>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestData {
    schema_version: u32,
    series: Vec<ManifestSeries>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestSeries {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    genre: String,
    #[serde(default)]
    cover_url: Option<String>,
    episodes: Vec<ManifestEpisode>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestEpisode {
    id: String,
    title: String,
    url: String,
}

impl ManifestData {
    fn empty() -> Self {
        Self {
            schema_version: SCHEMA,
            series: Vec::new(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA {
            return Err("manifest_schema: 仅支持 schemaVersion = 1".into());
        }
        if self.series.len() > MAX_SERIES {
            return Err("manifest_limit: 剧集条目超过 100 个".into());
        }
        let mut series_ids = HashSet::new();
        let mut episode_total = 0usize;
        for series in &self.series {
            validate_id(&series.id)?;
            validate_text(&series.title, 128, true)?;
            validate_text(&series.description, 1024, false)?;
            validate_text(&series.genre, 64, false)?;
            if let Some(cover) = series.cover_url.as_deref() {
                validate_public_https_resource(cover, "封面链接")?;
            }
            if !series_ids.insert(series.id.as_str()) {
                return Err("manifest_duplicate: 重复的系列 ID".into());
            }
            if series.episodes.is_empty() || series.episodes.len() > MAX_EPISODES_PER_SERIES {
                return Err("manifest_limit: 每部作品需要 1–100 集".into());
            }
            episode_total += series.episodes.len();
            if episode_total > MAX_EPISODES {
                return Err("manifest_limit: 总集数超过 500 集".into());
            }
            let mut episode_ids = HashSet::new();
            for episode in &series.episodes {
                validate_id(&episode.id)?;
                validate_text(&episode.title, 128, true)?;
                if !episode_ids.insert(episode.id.as_str()) {
                    return Err("manifest_duplicate: 同一作品存在重复集 ID".into());
                }
                crate::player::source::public_https_url(&episode.url)
                    .map_err(|e| format!("manifest_url: {} / {}：{e}", series.id, episode.id))?;
            }
        }
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("manifest_id: ID 只能包含 1–64 个英数字、短横线、下划线".into());
    }
    Ok(())
}

fn validate_public_https_resource(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 2048
        || !trimmed.starts_with("https://")
        || trimmed
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
        || trimmed.contains('#')
    {
        return Err(format!("manifest_url: {label}必须是有效的公开 HTTPS 链接"));
    }
    let parsed = http_client::parse_url(trimmed)?;
    let host = parsed.host.to_ascii_lowercase();
    let labels: Vec<_> = host.split('.').collect();
    if !parsed.secure
        || parsed.port != 443
        || labels.len() < 2
        || labels
            .last()
            .is_none_or(|tld| tld.len() < 2 || !tld.bytes().all(|b| b.is_ascii_alphabetic()))
        || ["local", "internal", "localhost", "test", "invalid"]
            .contains(labels.last().unwrap_or(&""))
    {
        return Err(format!("manifest_url: {label}必须指向公开 HTTPS 域名"));
    }
    Ok(trimmed.to_owned())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn subscription_meta_path(manifest_path: &Path) -> PathBuf {
    manifest_path.with_file_name("manifest-subscription.json")
}

fn read_subscription_meta(manifest_path: &Path) -> SubscriptionMeta {
    let path = subscription_meta_path(manifest_path);
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SubscriptionMeta>(&bytes).ok())
        .unwrap_or_default()
}

fn persist_subscription_meta(manifest_path: &Path, meta: &SubscriptionMeta) {
    if manifest_path.as_os_str().is_empty() {
        return;
    }
    let path = subscription_meta_path(manifest_path);
    if let Ok(bytes) = serde_json::to_vec_pretty(meta) {
        let _ = fs::write(path, bytes);
    }
}

fn validate_text(value: &str, max: usize, required: bool) -> Result<(), String> {
    if (required && value.trim().is_empty())
        || value.len() > max
        || value.chars().any(char::is_control)
    {
        return Err("manifest_text: 标题或说明无效或过长".into());
    }
    Ok(())
}

fn parse_manifest(bytes: &[u8]) -> Result<ManifestData, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("manifest_limit: 清单不得超过 256 KiB".into());
    }
    let data: ManifestData = serde_json::from_slice(bytes)
        .map_err(|error| format!("manifest_json: JSON 格式错误：{error}"))?;
    data.validate()?;
    Ok(data)
}

fn read_manifest(path: &Path) -> Result<ManifestData, String> {
    if !path.exists() {
        return Ok(ManifestData::empty());
    }
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_BYTES {
        return Err("manifest_limit: 清单不得超过 256 KiB".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    parse_manifest(&bytes)
}

fn persist_manifest(path: &Path, data: &ManifestData) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("manifest_unavailable: 无法确定数据目录")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("manifest_limit: 清单超过大小限制".into());
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
            "manifest_write: 无法保存清单：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

struct ManifestState {
    data: ManifestData,
    load_error: Option<String>,
    subscription: SubscriptionMeta,
}

pub struct OwnedManifestStore {
    path: PathBuf,
    state: Mutex<ManifestState>,
}

impl OwnedManifestStore {
    pub fn new() -> Self {
        match std::env::var_os("LOCALAPPDATA") {
            Some(root) => Self::from_path(
                PathBuf::from(root)
                    .join("HongGuoRust")
                    .join("owned-manifest.json"),
            ),
            None => Self {
                path: PathBuf::new(),
                state: Mutex::new(ManifestState {
                    data: ManifestData::empty(),
                    load_error: Some("manifest_unavailable: 缺少 LOCALAPPDATA".into()),
                    subscription: SubscriptionMeta::default(),
                }),
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn from_test_path(path: PathBuf) -> Self {
        Self::from_path(path)
    }

    fn from_path(path: PathBuf) -> Self {
        let (data, load_error) = match read_manifest(&path) {
            Ok(data) => (data, None),
            Err(e) => (ManifestData::empty(), Some(e)),
        };
        let subscription = read_subscription_meta(&path);
        Self {
            path,
            state: Mutex::new(ManifestState {
                data,
                load_error,
                subscription,
            }),
        }
    }

    fn snapshot(&self) -> Result<ManifestData, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        if let Some(err) = &state.load_error {
            return Err(err.clone());
        }
        Ok(state.data.clone())
    }

    pub fn subscription_info(&self) -> ManifestSubscriptionInfo {
        let state = self.state.lock().unwrap();
        ManifestSubscriptionInfo {
            url: state.subscription.url.clone(),
            series_count: state.data.series.len(),
            last_synced_ms: state.subscription.last_synced_ms,
        }
    }

    /// The selected JSON file remains in place. Only validated metadata is
    /// copied into LOCALAPPDATA, not the videos it references.
    pub fn import_file(&self, source: &Path) -> Result<usize, String> {
        if !source
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            return Err("manifest_file: 请选择 .json 清单文件".into());
        }
        if !source.is_file() {
            return Err("manifest_file: 所选清单文件不存在".into());
        }
        let data = read_manifest(source)?;
        self.replace_with_meta(data, SubscriptionMeta::default())
    }

    /// Fetch and subscribe to a remote public HTTPS JSON manifest.
    pub fn import_remote_url(&self, raw_url: &str) -> Result<usize, String> {
        let url = validate_public_https_resource(raw_url, "订阅链接")?;
        let bytes = http_client::get_bytes(&url, MAX_BYTES as usize)?;
        let data = parse_manifest(&bytes)?;
        let meta = SubscriptionMeta {
            url: Some(url),
            last_synced_ms: Some(now_ms()),
        };
        self.replace_with_meta(data, meta)
    }

    /// Re-fetch the currently subscribed remote HTTPS JSON manifest.
    pub fn refresh_subscription(&self) -> Result<usize, String> {
        let url = {
            let state = self.state.lock().map_err(|e| e.to_string())?;
            state
                .subscription
                .url
                .clone()
                .ok_or("manifest_subscription: 尚未配置远程订阅链接")?
        };
        self.import_remote_url(&url)
    }

    fn replace_with_meta(
        &self,
        data: ManifestData,
        meta: SubscriptionMeta,
    ) -> Result<usize, String> {
        data.validate()?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if self.path.as_os_str().is_empty() {
            return Err("manifest_unavailable: 缺少应用数据目录".into());
        }
        persist_manifest(&self.path, &data)?;
        persist_subscription_meta(&self.path, &meta);
        let count = data.series.len();
        state.data = data;
        state.load_error = None;
        state.subscription = meta;
        Ok(count)
    }

    pub fn clear(&self) -> Result<(), String> {
        self.replace_with_meta(ManifestData::empty(), SubscriptionMeta::default())
            .map(|_| ())
    }
}

pub struct OwnedManifestProvider {
    store: Arc<OwnedManifestStore>,
}

impl OwnedManifestProvider {
    pub fn new(store: Arc<OwnedManifestStore>) -> Self {
        Self { store }
    }

    fn series(value: &ManifestSeries) -> Series {
        Series {
            id: value.id.clone(),
            title: value.title.clone(),
            description: value.description.clone(),
            genre: if value.genre.is_empty() {
                "剧集清单".into()
            } else {
                value.genre.clone()
            },
            badge: "剧集清单".into(),
            artwork: "studio".into(),
            cover_url: value.cover_url.clone(),
            episode_count: value.episodes.len(),
            provider_id: "manifest".into(),
        }
    }
}

impl ContentProvider for OwnedManifestProvider {
    fn id(&self) -> &'static str {
        "manifest"
    }

    fn name(&self) -> &'static str {
        "自有媒体清单"
    }

    fn search(&self, query: &str) -> Result<Vec<Series>, String> {
        let term = query.trim().to_lowercase();
        Ok(self
            .store
            .snapshot()?
            .series
            .iter()
            .filter(|item| {
                term.is_empty()
                    || item.title.to_lowercase().contains(&term)
                    || item.description.to_lowercase().contains(&term)
                    || item.genre.to_lowercase().contains(&term)
            })
            .map(Self::series)
            .collect())
    }

    fn detail(&self, series_id: &str) -> Result<Option<SeriesDetail>, String> {
        Ok(self
            .store
            .snapshot()?
            .series
            .into_iter()
            .find(|item| item.id == series_id)
            .map(|item| SeriesDetail {
                series: Self::series(&item),
                episodes: item
                    .episodes
                    .iter()
                    .enumerate()
                    .map(|(index, episode)| Episode {
                        id: episode.id.clone(),
                        number: index + 1,
                        title: episode.title.clone(),
                        runtime_label: "MP4".into(),
                        playable: true,
                    })
                    .collect(),
            }))
    }

    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<MediaTarget, String> {
        let data = self.store.snapshot()?;
        let series = data
            .series
            .iter()
            .find(|item| item.id == series_id)
            .ok_or("series_not_found: 清单作品不存在")?;
        let episode = series
            .episodes
            .iter()
            .find(|item| item.id == episode_id)
            .ok_or("episode_not_found: 清单选集不存在")?;
        crate::player::source::public_https_url(&episode.url).map(MediaTarget::Https)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("hgr-owned-manifest-{}-{id}", std::process::id()))
    }

    fn fixture(url: &str) -> String {
        format!(
            "{{\"schemaVersion\":1,\"series\":[{{\"id\":\"my-series\",\"title\":\"个人短片\",\
             \"description\":\"测试\", \"genre\":\"自有内容\", \"episodes\":[{{\
             \"id\":\"ep-01\",\"title\":\"第 1 集\",\"url\":\"{url}\"}}]}}]}}"
        )
    }

    #[test]
    fn import_search_resolve_and_restart_are_persistent() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let selected = dir.join("user-import.json");
        fs::write(
            &selected,
            fixture("https://samplelib.com/lib/preview/mp4/sample-20s.mp4"),
        )
        .unwrap();
        let database = dir.join("owned-manifest.json");
        let store = Arc::new(OwnedManifestStore::from_test_path(database.clone()));
        assert_eq!(store.import_file(&selected).unwrap(), 1);
        let provider = OwnedManifestProvider::new(Arc::clone(&store));
        assert_eq!(provider.search("个人").unwrap().len(), 1);
        assert_eq!(
            provider
                .detail("my-series")
                .unwrap()
                .unwrap()
                .episodes
                .len(),
            1
        );
        assert_eq!(
            provider.resolve_episode("my-series", "ep-01").unwrap(),
            MediaTarget::Https("https://samplelib.com/lib/preview/mp4/sample-20s.mp4".into())
        );
        drop(store);
        let restarted = OwnedManifestStore::from_test_path(database);
        assert_eq!(restarted.snapshot().unwrap().series.len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_replacement_preserves_the_previous_manifest() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("user-import.json");
        fs::write(
            &source,
            fixture("https://samplelib.com/lib/preview/mp4/sample-20s.mp4"),
        )
        .unwrap();
        let store = OwnedManifestStore::from_test_path(dir.join("owned-manifest.json"));
        store.import_file(&source).unwrap();
        for payload in [
            fixture("http://bad.example.com/media.mp4"),
            fixture("https://localhost/media.mp4"),
            fixture("https://example.com/media.m3u8"),
            "{\"schemaVersion\":99,\"series\":[]}".into(),
            "{\"schemaVersion\":1,\"series\":[{\"id\":\"x\",\"title\":\"x\",\"episodes\":[]}]}"
                .into(),
            "{broken json".into(),
        ] {
            fs::write(&source, payload).unwrap();
            assert!(store.import_file(&source).is_err());
            assert_eq!(store.snapshot().unwrap().series.len(), 1);
        }
        store.clear().unwrap();
        assert!(store.snapshot().unwrap().series.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
