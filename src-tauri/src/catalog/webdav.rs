//! Standard WebDAV media library provider (RFC 4918 PROPFIND).
//! Supports Alist, Synology/NAS, Nextcloud, rclone serve webdav, etc.
//! Automatically groups folder contents into multi-episode drama series with
//! natural episode ordering. Credentials stay inside Rust and are never sent to WebView.
use super::http_client::{encode_basic_auth, parse_url, request};
use super::{ContentProvider, Episode, MediaTarget, Series, SeriesDetail};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
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

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_FOLDERS: usize = 48;
const MAX_SERIES: usize = 160;
const MAX_EPISODES_TOTAL: usize = 2000;
const MAX_EPISODES_PER_SERIES: usize = 200;

const PROPFIND_BODY: &[u8] = br#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:resourcetype/>
    <d:displayname/>
    <d:getcontentlength/>
  </d:prop>
</d:propfind>"#;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebDavConfig {
    name: String,
    base_url: String,
    username: String,
    password: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedEpisode {
    id: String,
    number: usize,
    title: String,
    url: String,
    runtime_label: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedSeries {
    id: String,
    title: String,
    genre: String,
    description: String,
    episodes: Vec<CachedEpisode>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebDavIndex {
    last_synced_ms: Option<u64>,
    series: Vec<CachedSeries>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavStatus {
    pub configured: bool,
    pub name: String,
    pub base_url: String,
    pub username: String,
    pub has_password: bool,
    pub series_count: usize,
    pub episode_count: usize,
    pub last_synced_ms: Option<u64>,
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or("webdav_storage: 无法确定存储目录")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
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
            "webdav_storage: 保存配置失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn fnv1a_id(input: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in input.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("d{hash:016x}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let parsed = parse_url(trimmed)?;
    let _ = parsed;
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let clean = without_fragment.trim_end_matches('/');
    Ok(format!("{clean}/"))
}

fn origin_of(url: &str) -> Option<String> {
    let parsed = parse_url(url).ok()?;
    let scheme = if parsed.secure { "https" } else { "http" };
    let default_port = if parsed.secure { 443 } else { 80 };
    if parsed.port == default_port {
        Some(format!("{scheme}://{}", parsed.host))
    } else {
        Some(format!("{scheme}://{}:{}", parsed.host, parsed.port))
    }
}

fn xml_unescape(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(val) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(val);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Namespace-agnostic XML tag inner-text extractor for WebDAV multistatus payloads.
fn extract_blocks<'a>(xml: &'a str, local_tag: &str) -> Vec<&'a str> {
    let mut results = Vec::new();
    let bytes = xml.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        let Some(rel_lt) = xml[pos..].find('<') else {
            break;
        };
        let tag_start = pos + rel_lt;
        let Some(rel_gt) = xml[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + rel_gt;
        let open_inner = &xml[tag_start + 1..tag_end];
        if open_inner.starts_with('/') || open_inner.starts_with('?') || open_inner.starts_with('!')
        {
            pos = tag_end + 1;
            continue;
        }
        let self_closing = open_inner.ends_with('/');
        let tag_token = open_inner
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let local = tag_token.rsplit(':').next().unwrap_or(tag_token);
        if local.eq_ignore_ascii_case(local_tag) {
            if self_closing {
                results.push("");
                pos = tag_end + 1;
                continue;
            }
            // Find matching closing tag `</...local_tag>`
            let content_start = tag_end + 1;
            let mut search = content_start;
            let mut found_end = None;
            while search < bytes.len() {
                let Some(rel_close) = xml[search..].find("</") else {
                    break;
                };
                let c_start = search + rel_close;
                let Some(rel_cgt) = xml[c_start..].find('>') else {
                    break;
                };
                let c_end = c_start + rel_cgt;
                let close_token = xml[c_start + 2..c_end].trim();
                let close_local = close_token.rsplit(':').next().unwrap_or(close_token);
                if close_local.eq_ignore_ascii_case(local_tag) {
                    found_end = Some((c_start, c_end + 1));
                    break;
                }
                search = c_end + 1;
            }
            if let Some((content_end, next_pos)) = found_end {
                results.push(&xml[content_start..content_end]);
                pos = next_pos;
                continue;
            }
        }
        pos = tag_end + 1;
    }
    results
}

#[derive(Debug, Clone)]
struct DavEntry {
    url: String,
    name: String,
    is_dir: bool,
}

fn is_video_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".mp4", ".mkv", ".mov", ".m4v", ".webm"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

fn strip_video_extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.trim().is_empty() => stem.trim().to_owned(),
        _ => name.trim().to_owned(),
    }
}

fn video_ext_label(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_uppercase())
        .filter(|ext| ext.len() <= 5)
        .unwrap_or_else(|| "VIDEO".into())
}

/// Natural numeric sort so "第2集" / "EP2" comes before "第10集" / "EP10".
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ia = a.chars().peekable();
    let mut ib = b.chars().peekable();
    loop {
        match (ia.peek(), ib.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&ca), Some(&cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(&c) = ia.peek() {
                    if c.is_ascii_digit() {
                        na.push(c);
                        ia.next();
                    } else {
                        break;
                    }
                }
                let mut nb = String::new();
                while let Some(&c) = ib.peek() {
                    if c.is_ascii_digit() {
                        nb.push(c);
                        ib.next();
                    } else {
                        break;
                    }
                }
                let ta = na.trim_start_matches('0');
                let tb = nb.trim_start_matches('0');
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(&ca), Some(&cb)) => {
                let ord = ca.to_ascii_lowercase().cmp(&cb.to_ascii_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                ia.next();
                ib.next();
            }
        }
    }
}

fn resolve_href(dir_url: &str, href: &str) -> Option<String> {
    let href = xml_unescape(href.trim());
    if href.is_empty() {
        return None;
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href);
    }
    let origin = origin_of(dir_url)?;
    if href.starts_with('/') {
        Some(format!("{origin}{href}"))
    } else {
        let base = if dir_url.ends_with('/') {
            dir_url.to_owned()
        } else {
            format!("{dir_url}/")
        };
        Some(format!("{base}{href}"))
    }
}

fn canonical_path(url: &str) -> String {
    parse_url(url)
        .map(|p| {
            percent_decode(p.path_and_query.split('?').next().unwrap_or("/"))
                .trim_end_matches('/')
                .to_owned()
        })
        .unwrap_or_default()
}

fn parse_multistatus(dir_url: &str, xml: &str) -> Vec<DavEntry> {
    let self_path = canonical_path(dir_url);
    let mut entries = Vec::new();
    let mut seen = HashSet::new();

    for block in extract_blocks(xml, "response") {
        let Some(href_raw) = extract_blocks(block, "href").into_iter().next() else {
            continue;
        };
        let Some(full_url) = resolve_href(dir_url, href_raw) else {
            continue;
        };
        let item_path = canonical_path(&full_url);
        if item_path == self_path || !seen.insert(item_path.clone()) {
            continue;
        }
        let is_dir = !extract_blocks(block, "collection").is_empty() || full_url.ends_with('/');
        let display_name = extract_blocks(block, "displayname")
            .into_iter()
            .next()
            .map(xml_unescape)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let fallback_name = item_path.rsplit('/').next().unwrap_or("").trim().to_owned();
        let name = display_name.unwrap_or(fallback_name);
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let clean_url = if is_dir && !full_url.ends_with('/') {
            format!("{full_url}/")
        } else {
            full_url
        };
        entries.push(DavEntry {
            url: clean_url,
            name,
            is_dir,
        });
    }
    entries
}

fn propfind_dir(dir_url: &str, auth_header: Option<&str>) -> Result<Vec<DavEntry>, String> {
    let mut headers = vec![
        ("Depth", "1"),
        ("Content-Type", "application/xml; charset=utf-8"),
    ];
    if let Some(auth) = auth_header {
        headers.push(("Authorization", auth));
    }
    let resp = request(
        "PROPFIND",
        dir_url,
        &headers,
        Some(PROPFIND_BODY),
        MAX_RESPONSE_BYTES,
    )?;
    if resp.status == 401 || resp.status == 403 {
        return Err(format!(
            "webdav_auth: 认证失败（HTTP {}），请检查 WebDAV 用户名与密码",
            resp.status
        ));
    }
    if resp.status == 404 {
        return Err("webdav_not_found: 指定的 WebDAV 目录不存在（HTTP 404）".into());
    }
    if resp.status != 207 && !(200..300).contains(&resp.status) {
        return Err(format!(
            "webdav_http: 服务器不支持 WebDAV 或返回异常状态 HTTP {}",
            resp.status
        ));
    }
    let xml = String::from_utf8_lossy(&resp.body);
    Ok(parse_multistatus(dir_url, &xml))
}

fn build_episodes(mut files: Vec<DavEntry>) -> Vec<CachedEpisode> {
    files.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    files
        .into_iter()
        .take(MAX_EPISODES_PER_SERIES)
        .enumerate()
        .map(|(idx, file)| CachedEpisode {
            id: format!("ep-{:03}", idx + 1),
            number: idx + 1,
            title: strip_video_extension(&file.name),
            url: file.url,
            runtime_label: video_ext_label(&file.name),
        })
        .collect()
}

fn scan_webdav_library(config: &WebDavConfig) -> Result<WebDavIndex, String> {
    let auth = if !config.username.is_empty() || !config.password.is_empty() {
        Some(encode_basic_auth(&config.username, &config.password))
    } else {
        None
    };
    let root_entries = propfind_dir(&config.base_url, auth.as_deref())?;
    let mut series = Vec::new();
    let mut total_episodes = 0usize;
    let mut folders_visited = 0usize;

    // 1. Loose videos directly in the root folder become standalone items.
    let mut root_videos: Vec<DavEntry> = root_entries
        .iter()
        .filter(|e| !e.is_dir && is_video_filename(&e.name))
        .cloned()
        .collect();
    root_videos.sort_by(|a, b| natural_cmp(&a.name, &b.name));

    let mut root_dirs: Vec<DavEntry> = root_entries.into_iter().filter(|e| e.is_dir).collect();
    root_dirs.sort_by(|a, b| natural_cmp(&a.name, &b.name));

    // 2. Scan first-level folders: if a folder contains videos -> Series;
    //    if it contains subfolders -> Genre containing Series (depth 2).
    for dir in root_dirs {
        if folders_visited >= MAX_FOLDERS
            || series.len() >= MAX_SERIES
            || total_episodes >= MAX_EPISODES_TOTAL
        {
            break;
        }
        folders_visited += 1;
        let Ok(children) = propfind_dir(&dir.url, auth.as_deref()) else {
            continue;
        };
        let videos: Vec<DavEntry> = children
            .iter()
            .filter(|e| !e.is_dir && is_video_filename(&e.name))
            .cloned()
            .collect();
        if !videos.is_empty() {
            let episodes = build_episodes(videos);
            total_episodes += episodes.len();
            series.push(CachedSeries {
                id: fnv1a_id(&dir.url),
                title: dir.name.clone(),
                genre: if config.name.is_empty() {
                    "WebDAV 片库".into()
                } else {
                    config.name.clone()
                },
                description: format!(
                    "来自 WebDAV 目录「{}」，共 {} 集。",
                    dir.name,
                    episodes.len()
                ),
                episodes,
            });
            continue;
        }

        // Depth 2: treat `dir.name` as genre, each child folder as a series.
        let mut subdirs: Vec<DavEntry> = children.into_iter().filter(|e| e.is_dir).collect();
        subdirs.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        for sub in subdirs {
            if folders_visited >= MAX_FOLDERS
                || series.len() >= MAX_SERIES
                || total_episodes >= MAX_EPISODES_TOTAL
            {
                break;
            }
            folders_visited += 1;
            let Ok(sub_children) = propfind_dir(&sub.url, auth.as_deref()) else {
                continue;
            };
            let sub_videos: Vec<DavEntry> = sub_children
                .into_iter()
                .filter(|e| !e.is_dir && is_video_filename(&e.name))
                .collect();
            if sub_videos.is_empty() {
                continue;
            }
            let episodes = build_episodes(sub_videos);
            total_episodes += episodes.len();
            series.push(CachedSeries {
                id: fnv1a_id(&sub.url),
                title: sub.name.clone(),
                genre: dir.name.clone(),
                description: format!("{} · 共 {} 集。", dir.name, episodes.len()),
                episodes,
            });
        }
    }

    // Add standalone root videos at the end (or if all videos are at root, group nicely).
    for video in root_videos {
        if series.len() >= MAX_SERIES || total_episodes >= MAX_EPISODES_TOTAL {
            break;
        }
        let title = strip_video_extension(&video.name);
        let label = video_ext_label(&video.name);
        total_episodes += 1;
        series.push(CachedSeries {
            id: fnv1a_id(&video.url),
            title: title.clone(),
            genre: if config.name.is_empty() {
                "WebDAV 影片".into()
            } else {
                config.name.clone()
            },
            description: format!("来自 WebDAV 的流式视频（{label}）。"),
            episodes: vec![CachedEpisode {
                id: "ep-001".into(),
                number: 1,
                title,
                url: video.url,
                runtime_label: label,
            }],
        });
    }

    Ok(WebDavIndex {
        last_synced_ms: Some(now_ms()),
        series,
    })
}

struct WebDavState {
    config: Option<WebDavConfig>,
    index: WebDavIndex,
}

pub struct WebDavStore {
    config_path: PathBuf,
    index_path: PathBuf,
    state: Mutex<WebDavState>,
}

impl WebDavStore {
    pub fn new() -> Self {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("HongGuoRust");
        Self::from_dir(dir)
    }

    fn from_dir(dir: PathBuf) -> Self {
        let config_path = dir.join("webdav-config.json");
        let index_path = dir.join("webdav-index.json");
        let config = fs::read(&config_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<WebDavConfig>(&bytes).ok())
            .filter(|c| !c.base_url.trim().is_empty());
        let index = if config.is_some() {
            fs::read(&index_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<WebDavIndex>(&bytes).ok())
                .unwrap_or_default()
        } else {
            WebDavIndex::default()
        };
        Self {
            config_path,
            index_path,
            state: Mutex::new(WebDavState { config, index }),
        }
    }

    pub fn status(&self) -> WebDavStatus {
        let state = self.state.lock().unwrap();
        let episode_count = state.index.series.iter().map(|s| s.episodes.len()).sum();
        match &state.config {
            Some(cfg) => WebDavStatus {
                configured: true,
                name: cfg.name.clone(),
                base_url: cfg.base_url.clone(),
                username: cfg.username.clone(),
                has_password: !cfg.password.is_empty(),
                series_count: state.index.series.len(),
                episode_count,
                last_synced_ms: state.index.last_synced_ms,
            },
            None => WebDavStatus {
                configured: false,
                name: String::new(),
                base_url: String::new(),
                username: String::new(),
                has_password: false,
                series_count: 0,
                episode_count: 0,
                last_synced_ms: None,
            },
        }
    }

    pub fn configure_and_sync(
        &self,
        name: String,
        base_url: String,
        username: String,
        password: String,
    ) -> Result<WebDavStatus, String> {
        let normalized_url = normalize_base_url(&base_url)?;
        let clean_name = name.trim();
        let display_name = if clean_name.is_empty() {
            "WebDAV 片库".to_owned()
        } else if clean_name.len() > 64 || clean_name.chars().any(char::is_control) {
            return Err("webdav_name: 名称不能超过 64 个字符".into());
        } else {
            clean_name.to_owned()
        };
        // If user left password blank while updating the same URL/username, preserve stored password.
        let effective_password = if password.is_empty() {
            let state = self.state.lock().map_err(|e| e.to_string())?;
            state
                .config
                .as_ref()
                .filter(|c| c.base_url == normalized_url && c.username == username.trim())
                .map(|c| c.password.clone())
                .unwrap_or_default()
        } else {
            password
        };
        let config = WebDavConfig {
            name: display_name,
            base_url: normalized_url,
            username: username.trim().to_owned(),
            password: effective_password,
        };
        // Verify connectivity & scan before committing!
        let index = scan_webdav_library(&config)?;
        atomic_write_json(&self.config_path, &config)?;
        atomic_write_json(&self.index_path, &index)?;
        {
            let mut state = self.state.lock().map_err(|e| e.to_string())?;
            state.config = Some(config);
            state.index = index;
        }
        Ok(self.status())
    }

    pub fn sync(&self) -> Result<WebDavStatus, String> {
        let config = {
            let state = self.state.lock().map_err(|e| e.to_string())?;
            state
                .config
                .clone()
                .ok_or("webdav_not_configured: 尚未配置 WebDAV 连接")?
        };
        let index = scan_webdav_library(&config)?;
        atomic_write_json(&self.index_path, &index)?;
        {
            let mut state = self.state.lock().map_err(|e| e.to_string())?;
            state.index = index;
        }
        Ok(self.status())
    }

    pub fn clear(&self) -> Result<(), String> {
        let _ = fs::remove_file(&self.config_path);
        let _ = fs::remove_file(&self.index_path);
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        state.config = None;
        state.index = WebDavIndex::default();
        Ok(())
    }

    fn auth_header(&self) -> Option<String> {
        let state = self.state.lock().ok()?;
        let cfg = state.config.as_ref()?;
        if cfg.username.is_empty() && cfg.password.is_empty() {
            None
        } else {
            Some(encode_basic_auth(&cfg.username, &cfg.password))
        }
    }

    fn snapshot(&self) -> Vec<CachedSeries> {
        self.state
            .lock()
            .map(|s| s.index.series.clone())
            .unwrap_or_default()
    }
}

pub struct WebDavProvider {
    store: Arc<WebDavStore>,
}

impl WebDavProvider {
    pub fn new(store: Arc<WebDavStore>) -> Self {
        Self { store }
    }

    fn to_series(item: &CachedSeries) -> Series {
        Series {
            id: item.id.clone(),
            title: item.title.clone(),
            description: item.description.clone(),
            genre: item.genre.clone(),
            badge: "WebDAV".into(),
            artwork: "city".into(),
            cover_url: None,
            episode_count: item.episodes.len(),
            provider_id: "webdav".into(),
        }
    }
}

impl ContentProvider for WebDavProvider {
    fn id(&self) -> &'static str {
        "webdav"
    }

    fn name(&self) -> &'static str {
        "WebDAV 片库"
    }

    fn search(&self, query: &str) -> Result<Vec<Series>, String> {
        let term = query.trim().to_lowercase();
        Ok(self
            .store
            .snapshot()
            .iter()
            .filter(|item| {
                term.is_empty()
                    || item.title.to_lowercase().contains(&term)
                    || item.genre.to_lowercase().contains(&term)
                    || item.description.to_lowercase().contains(&term)
            })
            .map(Self::to_series)
            .collect())
    }

    fn detail(&self, series_id: &str) -> Result<Option<SeriesDetail>, String> {
        Ok(self
            .store
            .snapshot()
            .into_iter()
            .find(|item| item.id == series_id)
            .map(|item| SeriesDetail {
                series: Self::to_series(&item),
                episodes: item
                    .episodes
                    .into_iter()
                    .map(|ep| Episode {
                        id: ep.id,
                        number: ep.number,
                        title: ep.title,
                        runtime_label: ep.runtime_label,
                        playable: true,
                    })
                    .collect(),
            }))
    }

    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<MediaTarget, String> {
        let items = self.store.snapshot();
        let series = items
            .iter()
            .find(|item| item.id == series_id)
            .ok_or("series_not_found: WebDAV 作品不存在，请刷新同步")?;
        let ep = series
            .episodes
            .iter()
            .find(|e| e.id == episode_id)
            .ok_or("episode_not_found: WebDAV 选集不存在")?;
        let url = crate::player::source::webdav_media_url(&ep.url)?;
        Ok(MediaTarget::WebDav {
            url,
            auth_header: self.store.auth_header(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multistatus_and_sorts_episodes_naturally() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/dav/drama/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/drama/%E7%AC%AC10%E9%9B%86.mp4</D:href>
    <D:propstat><D:prop><D:resourcetype/></D:prop></D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/drama/%E7%AC%AC2%E9%9B%86.mp4</D:href>
    <D:propstat><D:prop><D:resourcetype/></D:prop></D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav/drama/%E7%AC%AC1%E9%9B%86.mp4</D:href>
    <D:propstat><D:prop><D:resourcetype/></D:prop></D:propstat>
  </D:response>
</D:multistatus>"#;

        let entries = parse_multistatus("http://127.0.0.1:5244/dav/drama/", xml);
        assert_eq!(entries.len(), 3);
        let eps = build_episodes(entries);
        assert_eq!(eps[0].title, "第1集");
        assert_eq!(eps[1].title, "第2集");
        assert_eq!(eps[2].title, "第10集");
    }
}
