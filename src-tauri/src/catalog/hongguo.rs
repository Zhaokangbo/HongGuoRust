//! HG-4: Native Rust HongGuo catalog & real CDN playback provider.
//! Architecture adapted from N3urda/hongguoTV (GPL-3.0-only):
//! - Public SSR `_ROUTER_DATA` for category discovery & series episode lists
//! - Website JSON search API + signed v3 App search fallback
//! - Anonymous device registration (`DeviceRegister`) + Android v5 playback signing
//! - Multi-path model resolution (v5 POST -> v5 GET video_tab -> alternate hosts -> fallback_api)
//! - In-process authenticated loopback Range proxy (`HgStreamProxy`) with CENC decryption
//!   and verified primary -> backup CDN failover.
//!
//! Legacy HG-1 placeholder & mock contract types remain below for unit-test compatibility.

use super::hg_crypto::gzip_compress;
use super::hg_danmaku::{DanmakuFetcher, DanmakuItem};
use super::hg_media::{
    clean_url, parse_fallback_api, parse_json_object, parse_renditions, select_rendition, Rendition,
};
use super::hg_signer::{
    now_millis, query_encode_component, DeviceIdentity, PlaybackDeviceSession, Signer, APP_UA,
    V5_POST_UA, V5_TAB_UA, VIDEO_UA,
};
use super::hg_stream::HgStreamProxy;
use super::http_client::{request_with_options, MemoryCookies, RequestOptions};
use super::request::RequestContext;
use super::{ContentProvider, Episode, MediaTarget, QualityOption, Series, SeriesDetail};
use crate::player::source::public_https_url;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SITE: &str = "https://hongguoduanju.com";
const SITE_HOST: &str = "hongguoduanju.com";
const PRIMARY_MODEL_HOST: &str = "reading.snssdk.com";
const ALT_MODEL_HOSTS: [&str; 3] = [
    "api5-normal-sinfonlineb.fqnovel.com",
    "api5-normal-sinfonlinea.fqnovel.com",
    "api5-normal-hl.fqnovel.com",
];
const PRELOAD_UA: &str = "okhttp/4.12.0";
const APP_SEARCH_URL: &str =
    "https://api5-normal-sinfonlinea.fqnovel.com/reading/bookapi/search/tab/v";
const MAX_HTTP_BYTES: usize = 8 * 1024 * 1024;
const MAX_MODEL_BYTES: usize = 2 * 1024 * 1024;
const CATALOG_CACHE_TTL: Duration = Duration::from_secs(300);
const SEARCH_CACHE_TTL: Duration = Duration::from_secs(180);
const MAX_CACHED_DETAILS: usize = 24;
const MAX_CACHED_SEARCHES: usize = 20;
const MAX_CACHED_RENDITIONS: usize = 16;
const RENDITION_CACHE_TTL: Duration = Duration::from_secs(300);
const MAX_REAL_EPISODES: usize = 300;
const MAX_UPSTREAM_PAGES: usize = 100;
const MAX_FETCH_PAGES_PER_CALL: usize = 3;

// ---------- Helpers for SSR & JSON Parsing ----------

pub fn extract_router_data(html: &str) -> Result<Value, String> {
    let start = html
        .find("_ROUTER_DATA")
        .ok_or("hg_ssr: 页面未包含路由数据")?;
    let bytes = html.as_bytes();
    let mut first = start;
    while first < bytes.len() && bytes[first] != b'{' {
        first += 1;
    }
    if first >= bytes.len() {
        return Err("hg_ssr: 路由数据格式异常".into());
    }
    let mut depth: i32 = 0;
    let mut in_quote = false;
    let mut escaped = false;
    for i in first..bytes.len() {
        let ch = bytes[i];
        if in_quote {
            if escaped {
                escaped = false;
            } else if ch == b'\\' {
                escaped = true;
            } else if ch == b'"' {
                in_quote = false;
            }
            continue;
        }
        match ch {
            b'"' => in_quote = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    let slice = &html[first..=i];
                    return serde_json::from_str(slice)
                        .map_err(|_| "hg_ssr: 路由 JSON 解析失败".to_string());
                }
            }
            _ => {}
        }
    }
    Err("hg_ssr: 路由数据不完整".into())
}

fn is_numeric_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 30 && value.bytes().all(|b| b.is_ascii_digit())
}

fn sanitize_text(raw: &str, max_chars: usize) -> String {
    raw.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(max_chars)
        .collect()
}

fn json_str(obj: &Value, key: &str) -> String {
    match obj.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn json_usize(obj: &Value, key: &str) -> usize {
    match obj.get(key) {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0) as usize,
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn valid_remote_cover(raw: &str) -> Option<String> {
    let cleaned = clean_url(raw);
    if cleaned.is_empty()
        || cleaned.len() > 2048
        || !cleaned.starts_with("https://")
        || cleaned.contains('#')
        || cleaned.contains('\\')
        || cleaned.bytes().any(|b| b <= 0x20 || b == 0x7f)
    {
        return None;
    }
    let host = cleaned[8..].split(['/', '?']).next()?;
    if host.is_empty() || host.contains('@') || host.contains(':') {
        return None;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    let suffix = host.rsplit('.').next()?.to_ascii_lowercase();
    if suffix.len() < 2
        || !suffix.bytes().all(|b| b.is_ascii_alphabetic())
        || ["local", "localhost", "internal", "test", "invalid"].contains(&suffix.as_str())
    {
        return None;
    }
    Some(cleaned)
}

fn map_ssr_item(item: &Value) -> Option<Series> {
    let src = item
        .get("video_data")
        .filter(|v| v.is_object())
        .unwrap_or(item);
    let id = json_str(src, "series_id");
    if !is_numeric_id(&id) {
        return None;
    }
    let raw_title = [
        json_str(src, "series_name"),
        json_str(src, "series_title"),
        json_str(item, "name"),
    ]
    .into_iter()
    .find(|s| !s.is_empty())?;
    let title = sanitize_text(&raw_title, 100);
    if title.is_empty() {
        return None;
    }

    let count = {
        let c = json_usize(src, "episode_cnt");
        if c > 0 {
            c
        } else {
            src.get("series_episode_info")
                .map(|v| json_usize(v, "episode_cnt"))
                .unwrap_or(0)
        }
    };
    let episode_count = count.clamp(1, MAX_REAL_EPISODES);

    let tags: Vec<String> = src
        .get("tags")
        .or_else(|| src.get("category_list"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(str::trim)
                        .or_else(|| v.get("name").and_then(Value::as_str).map(str::trim))
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                })
                .take(3)
                .collect()
        })
        .unwrap_or_default();

    let genre = tags
        .first()
        .map(|s| sanitize_text(s, 40))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "短剧".into());

    let badge = {
        let right = sanitize_text(&json_str(src, "episode_right_text"), 32);
        if !right.is_empty() {
            right
        } else if count > 0 {
            format!("全 {count} 集")
        } else {
            "红果短剧".into()
        }
    };

    let description = sanitize_text(&json_str(src, "series_intro"), 400);
    let cover_url = valid_remote_cover(&json_str(src, "series_cover"));

    Some(Series {
        id,
        title,
        description,
        genre,
        badge,
        artwork: "studio".into(),
        cover_url,
        episode_count,
        provider_id: "hongguo".into(),
    })
}

fn dedup_series(rows: Vec<Series>) -> Vec<Series> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for item in rows {
        if seen.insert(item.id.clone()) {
            out.push(item);
        }
    }
    out
}

fn parse_app_search_items(root: &Value, tab_type: i64) -> Vec<Series> {
    let tabs = root
        .get("search_tabs")
        .or_else(|| root.get("data").and_then(|d| d.get("search_tabs")))
        .and_then(Value::as_array);
    let Some(tabs) = tabs else {
        return Vec::new();
    };
    let tab = tabs
        .iter()
        .find(|t| t.get("tab_type").and_then(Value::as_i64) == Some(tab_type))
        .or_else(|| tabs.first());
    let Some(cells) = tab.and_then(|t| t.get("data")).and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for cell in cells {
        let mut candidates: Vec<Value> = vec![cell.clone()];
        if let Some(lynx_str) = cell.get("lynx_data").and_then(Value::as_str) {
            if let Ok(parsed) = serde_json::from_str::<Value>(lynx_str) {
                if let Some(arr) = parsed.get("cell_data").and_then(Value::as_array) {
                    candidates.extend(arr.iter().cloned());
                }
            }
        }
        for row in candidates {
            let Some(video) = row
                .get("video_data")
                .and_then(Value::as_array)
                .and_then(|arr| arr.iter().find(|v| v.is_object()))
            else {
                continue;
            };
            let id = [
                json_str(video, "series_id"),
                json_str(&row, "book_id"),
                json_str(&row, "search_result_id"),
            ]
            .into_iter()
            .find(|s| is_numeric_id(s));
            let Some(id) = id else { continue };
            let raw_title = [
                row.get("search_high_light")
                    .and_then(|h| h.get("title"))
                    .and_then(|t| t.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_owned(),
                json_str(video, "title"),
                json_str(&row, "cell_name"),
            ]
            .into_iter()
            .find(|s| !s.is_empty());
            let Some(raw_title) = raw_title else { continue };
            let title = sanitize_text(&raw_title, 100);
            let count = json_usize(video, "episode_cnt").clamp(1, MAX_REAL_EPISODES);
            let cover = valid_remote_cover(
                &[json_str(video, "cover"), json_str(video, "cover_url")]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or_default(),
            );
            let desc = sanitize_text(&json_str(video, "video_desc"), 400);
            let genre = sanitize_text(&json_str(video, "sub_title"), 40);
            out.push(Series {
                id,
                title,
                description: desc,
                genre: if genre.is_empty() {
                    "短剧".into()
                } else {
                    genre
                },
                badge: format!("全 {count} 集"),
                artwork: "studio".into(),
                cover_url: cover,
                episode_count: count,
                provider_id: "hongguo".into(),
            });
        }
    }
    dedup_series(out)
}

// ---------- Multi-Path v5 Playback Model Fetcher ----------

enum ModelOutcome {
    Renditions(Vec<Rendition>),
    EmptyResponse,
    NoCompatibleRenditions,
    Failure(String),
}

fn set_query_param(url: &str, key: &str, value: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, q),
        None => return format!("{url}?{key}={value}"),
    };
    let mut parts: Vec<String> = Vec::new();
    let mut replaced = false;
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let pair_key = pair.split_once('=').map(|(k, _)| k).unwrap_or(pair);
        if pair_key == key {
            if !replaced {
                parts.push(format!("{key}={value}"));
                replaced = true;
            }
        } else {
            parts.push(pair.to_owned());
        }
    }
    if !replaced {
        parts.push(format!("{key}={value}"));
    }
    format!("{base}?{}", parts.join("&"))
}

fn extract_renditions_from_model(
    model: &Value,
    fetch_fallback: &dyn Fn(&str) -> Result<String, String>,
) -> ModelOutcome {
    let direct = parse_renditions(model);
    if !direct.is_empty() {
        return ModelOutcome::Renditions(direct);
    }
    let Some(fallback_url) = parse_fallback_api(model) else {
        return ModelOutcome::NoCompatibleRenditions;
    };
    // App-provided VOD codec negotiation (Issue #102 / MobilePlayback.compatibleLadder):
    // try codec_type=1 (AVC) then codec_type=3 (HEVC), then raw fallback_url.
    let candidates = [
        set_query_param(&fallback_url, "codec_type", "1"),
        set_query_param(&fallback_url, "codec_type", "3"),
        fallback_url,
    ];
    let mut last_failure: Option<String> = None;
    for candidate in &candidates {
        let raw = match fetch_fallback(candidate) {
            Ok(r) => r,
            Err(_) => {
                last_failure = Some("hg_play: fallback_api 请求失败".into());
                continue;
            }
        };
        let Ok(parsed) = serde_json::from_str::<Value>(&raw) else {
            last_failure = Some("hg_play: fallback_api 响应格式异常".into());
            continue;
        };
        let Some(info) = parsed
            .get("video_info")
            .and_then(|v| v.get("data"))
            .filter(|v| v.is_object())
        else {
            continue;
        };
        let rows = parse_renditions(info);
        if !rows.is_empty() {
            return ModelOutcome::Renditions(rows);
        }
    }
    match last_failure {
        Some(err) => ModelOutcome::Failure(err),
        None => ModelOutcome::NoCompatibleRenditions,
    }
}

fn check_upstream_code(root: &Value) -> Result<(), String> {
    let code = root
        .get("Code")
        .or_else(|| root.get("code"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if code != 0 {
        return Err(format!(
            "hg_play_rejected: 上游拒绝播放请求（错误码 {code}）"
        ));
    }
    Ok(())
}

// ---------- Cached Provider State ----------

#[derive(Clone)]
struct ProgressiveFeedCache {
    items: Vec<Series>,
    seen_ids: HashSet<String>,
    next_page: usize,
    has_more: bool,
    saved_at: Instant,
}

impl ProgressiveFeedCache {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            seen_ids: HashSet::new(),
            next_page: 1,
            has_more: true,
            saved_at: Instant::now(),
        }
    }

    fn append_page(&mut self, page_items: Vec<Series>, has_more: bool) {
        let mut added = 0usize;
        for item in page_items {
            if self.seen_ids.insert(item.id.clone()) {
                self.items.push(item);
                added += 1;
            }
        }
        self.next_page = self.next_page.saturating_add(1);
        self.has_more = has_more && added > 0 && self.next_page <= MAX_UPSTREAM_PAGES;
        self.saved_at = Instant::now();
    }
}

struct CachedDetail {
    detail: SeriesDetail,
    saved_at: Instant,
}

#[derive(Clone)]
struct CachedRenditions {
    rows: Vec<Rendition>,
    active_quality: u32,
    saved_at: Instant,
}

pub struct HongGuoRealProvider {
    cookies: MemoryCookies,
    device_session: PlaybackDeviceSession,
    v5_signer: Signer,
    v3_signer: Signer,
    stream_proxy: HgStreamProxy,
    danmaku: DanmakuFetcher,
    /// `0` means "highest available compatible quality"; positive values (e.g. 1080, 720, 540)
    /// remember the user's explicit session quality selection.
    preferred_quality: AtomicU32,
    home_cache: Mutex<Option<ProgressiveFeedCache>>,
    search_cache: Mutex<HashMap<String, ProgressiveFeedCache>>,
    detail_cache: Mutex<HashMap<String, CachedDetail>>,
    rendition_cache: Mutex<HashMap<String, CachedRenditions>>,
}

impl Default for HongGuoRealProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl HongGuoRealProvider {
    pub fn new() -> Self {
        Self {
            cookies: MemoryCookies::default(),
            device_session: PlaybackDeviceSession::default(),
            v5_signer: Signer::new(true),
            v3_signer: Signer::new(false),
            stream_proxy: HgStreamProxy::default(),
            danmaku: DanmakuFetcher::new(),
            preferred_quality: AtomicU32::new(0),
            home_cache: Mutex::new(None),
            search_cache: Mutex::new(HashMap::new()),
            detail_cache: Mutex::new(HashMap::new()),
            rendition_cache: Mutex::new(HashMap::new()),
        }
    }

    fn fetch_site_text(&self, url: &str, use_video_ua: bool) -> Result<String, String> {
        let ua = if use_video_ua {
            VIDEO_UA
        } else {
            "Mozilla/5.0"
        };
        let referer = format!("{SITE}/");
        let cookie = self.cookies.cookie_header(SITE_HOST);
        let mut headers: Vec<(&str, &str)> = vec![
            ("User-Agent", ua),
            ("Accept-Language", "zh-CN,zh;q=0.9"),
            ("Referer", &referer),
        ];
        if let Some(c) = cookie.as_deref() {
            headers.push(("Cookie", c));
        }
        let resp = request_with_options(
            "GET",
            url,
            &headers,
            None,
            MAX_HTTP_BYTES,
            RequestOptions {
                user_agent: Some(ua),
                timeout_ms: 10_000,
                decompress: true,
                disable_cookies: false,
            },
        )?;
        self.cookies
            .save_from_headers(SITE_HOST, &resp.set_cookies());
        if !(200..300).contains(&resp.status) {
            return Err(format!("hg_http: 红果站点返回 HTTP {}", resp.status));
        }
        String::from_utf8(resp.body).map_err(|_| "hg_http: 响应不是有效 UTF-8".to_string())
    }

    fn fetch_fallback_text(&self, url: &str) -> Result<String, String> {
        let headers = [
            ("Accept", "application/json"),
            ("Accept-Language", "zh-CN,zh;q=0.9"),
        ];
        let resp = request_with_options(
            "GET",
            url,
            &headers,
            None,
            MAX_MODEL_BYTES,
            RequestOptions {
                user_agent: Some(PRELOAD_UA),
                timeout_ms: 8_000,
                decompress: true,
                disable_cookies: true,
            },
        )?;
        if !(200..300).contains(&resp.status) {
            return Err(format!("hg_fallback_http_{}", resp.status));
        }
        String::from_utf8(resp.body).map_err(|_| "hg_fallback_utf8".to_string())
    }

    fn fetch_home_page(&self, page: usize) -> Result<(Vec<Series>, bool), String> {
        let url = if page <= 1 {
            format!("{SITE}/category?tab=1&sort_type=1")
        } else {
            format!("{SITE}/category?tab=1&sort_type=1&page={page}")
        };
        let html = self.fetch_site_text(&url, false)?;
        let router = extract_router_data(&html)?;
        let loader = router
            .get("loaderData")
            .ok_or("hg_ssr: 首页数据结构缺失 loaderData")?;
        let section = loader
            .get("category_page")
            .or_else(|| loader.get("category_$"))
            .ok_or("hg_ssr: 首页数据结构已变化")?;
        let rows = section
            .get("recommendList")
            .or_else(|| {
                section
                    .get("categoryData")
                    .and_then(|c| c.get("recommendList"))
            })
            .and_then(Value::as_array)
            .ok_or("hg_ssr: 首页推荐列表暂不可用")?;
        let items = dedup_series(rows.iter().filter_map(map_ssr_item).collect());
        let total_pages = section
            .get("pagination")
            .and_then(|p| p.get("totalPages"))
            .and_then(Value::as_u64)
            .map(|n| (n as usize).min(MAX_UPSTREAM_PAGES))
            .unwrap_or(MAX_UPSTREAM_PAGES);
        let has_more = !items.is_empty() && page < total_pages;
        Ok((items, has_more))
    }

    fn fetch_home_until(
        &self,
        needed: usize,
        request: Option<&RequestContext>,
    ) -> Result<(Vec<Series>, bool), String> {
        if let Some(ctx) = request {
            ctx.check()?;
        }
        let target = needed.max(1);
        let mut cache = self
            .home_cache
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .filter(|c| c.saved_at.elapsed() < CATALOG_CACHE_TTL)
            .unwrap_or_else(ProgressiveFeedCache::new);

        let mut fetched = 0usize;
        while cache.items.len() < target && cache.has_more && fetched < MAX_FETCH_PAGES_PER_CALL {
            if let Some(ctx) = request {
                ctx.check()?;
            }
            let page_num = cache.next_page;
            match self.fetch_home_page(page_num) {
                Ok((page_items, has_more)) => {
                    cache.append_page(page_items, has_more);
                    fetched += 1;
                }
                Err(err) => {
                    if !cache.items.is_empty()
                        && !err.starts_with("provider_cancelled:")
                        && !err.starts_with("provider_timeout:")
                    {
                        tracing::warn!(page = page_num, error = %err, "hongguo catalog next page fetch failed; returning partial results");
                        break;
                    }
                    return Err(err);
                }
            }
        }
        if !cache.items.is_empty() {
            if let Ok(mut guard) = self.home_cache.lock() {
                *guard = Some(cache.clone());
            }
        }
        Ok((cache.items, cache.has_more))
    }

    fn search_website_api_page(
        &self,
        keyword: &str,
        page: usize,
    ) -> Result<(Vec<Series>, bool), String> {
        let encoded = query_encode_component(keyword);
        let url = format!("{SITE}/api/search/result?query={encoded}&page={page}");
        let text = self.fetch_site_text(&url, true)?;
        let root: Value =
            serde_json::from_str(&text).map_err(|_| "hg_search: 搜索响应格式异常".to_string())?;
        let rows = root
            .get("searchList")
            .and_then(Value::as_array)
            .ok_or("hg_search: 搜索列表字段缺失")?;
        let filtered = rows
            .iter()
            .filter(|item| {
                let genre = item
                    .get("video_data")
                    .map(|v| json_str(v, "genre"))
                    .unwrap_or_default();
                genre.is_empty() || genre == "203" || genre == "205"
            })
            .filter_map(map_ssr_item)
            .collect();
        let items = dedup_series(filtered);
        let more_flag = if let Some(b) = root.get("hasMore").and_then(Value::as_bool) {
            b
        } else if let Some(tp) = root.get("totalPages").and_then(Value::as_u64) {
            page < (tp as usize).min(MAX_UPSTREAM_PAGES)
        } else {
            false
        };
        let has_more = !rows.is_empty() && page < MAX_UPSTREAM_PAGES && more_flag;
        Ok((items, has_more))
    }

    fn search_signed_app_api(&self, keyword: &str) -> Result<Vec<Series>, String> {
        let params: Vec<(String, String)> = vec![
            ("iid".into(), "1905892595382586".into()),
            ("device_id".into(), "1905892595378490".into()),
            ("ac".into(), "wifi".into()),
            ("channel".into(), "update_64".into()),
            ("aid".into(), "8662".into()),
            ("app_name".into(), "novelread".into()),
            ("version_code".into(), "72232".into()),
            ("version_name".into(), "7.2.2.32".into()),
            ("device_platform".into(), "android".into()),
            ("os".into(), "android".into()),
            ("ssmix".into(), "a".into()),
            ("device_type".into(), "25053RT47C".into()),
            ("device_brand".into(), "Redmi".into()),
            ("language".into(), "zh".into()),
            ("os_api".into(), "36".into()),
            ("os_version".into(), "16".into()),
            ("manifest_version_code".into(), "72232".into()),
            ("resolution".into(), "1280*2772".into()),
            ("dpi".into(), "520".into()),
            ("update_version_code".into(), "72232".into()),
            ("host_abi".into(), "arm64-v8a".into()),
            ("dragon_device_type".into(), "phone".into()),
            ("pv_player".into(), "72232".into()),
            ("compliance_status".into(), "0".into()),
            ("need_personal_recommend".into(), "1".into()),
            ("player_so_load".into(), "1".into()),
            ("is_android_pad_screen".into(), "0".into()),
            ("query".into(), keyword.to_owned()),
            ("count".into(), "20".into()),
            ("offset".into(), "0".into()),
            ("tab_type".into(), "11".into()),
            ("bookshelf_search_plan".into(), "4".into()),
            ("use_correct".into(), "true".into()),
            ("user_is_login".into(), "0".into()),
        ];
        let signed = self
            .v3_signer
            .sign_v3_get(APP_SEARCH_URL, &params, now_millis())?;
        let hdr_refs: Vec<(&str, &str)> = signed
            .headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let resp = request_with_options(
            "GET",
            &signed.url,
            &hdr_refs,
            None,
            MAX_MODEL_BYTES,
            RequestOptions {
                user_agent: Some(APP_UA),
                timeout_ms: 8_000,
                decompress: true,
                disable_cookies: true,
            },
        )?;
        if !(200..300).contains(&resp.status) {
            return Err(format!("hg_app_search_http_{}", resp.status));
        }
        let root: Value = serde_json::from_slice(&resp.body)
            .map_err(|_| "hg_app_search: 搜索响应格式异常".to_string())?;
        check_upstream_code(&root)?;
        Ok(parse_app_search_items(&root, 11))
    }

    fn fetch_search_until(
        &self,
        keyword: &str,
        needed: usize,
        request: Option<&RequestContext>,
    ) -> Result<(Vec<Series>, bool), String> {
        if let Some(ctx) = request {
            ctx.check()?;
        }
        let key = keyword.trim().to_owned();
        let target = needed.max(1);
        let mut cache = self
            .search_cache
            .lock()
            .ok()
            .and_then(|g| g.get(&key).cloned())
            .filter(|c| c.saved_at.elapsed() < SEARCH_CACHE_TTL)
            .unwrap_or_else(ProgressiveFeedCache::new);

        let mut fetched = 0usize;
        while cache.items.len() < target && cache.has_more && fetched < MAX_FETCH_PAGES_PER_CALL {
            if let Some(ctx) = request {
                ctx.check()?;
            }
            let page_num = cache.next_page;
            match self.search_website_api_page(&key, page_num) {
                Ok((page_items, has_more)) => {
                    if page_num == 1 && page_items.is_empty() {
                        if let Some(ctx) = request {
                            ctx.check()?;
                        }
                        if let Ok(app_items) = self.search_signed_app_api(&key) {
                            if !app_items.is_empty() {
                                cache.append_page(app_items, false);
                                break;
                            }
                        }
                    }
                    cache.append_page(page_items, has_more);
                    fetched += 1;
                }
                Err(web_err) => {
                    if page_num == 1 && cache.items.is_empty() {
                        if let Some(ctx) = request {
                            ctx.check()?;
                        }
                        match self.search_signed_app_api(&key) {
                            Ok(app_items) if !app_items.is_empty() => {
                                cache.append_page(app_items, false);
                                break;
                            }
                            _ => return Err(web_err),
                        }
                    }
                    if !cache.items.is_empty()
                        && !web_err.starts_with("provider_cancelled:")
                        && !web_err.starts_with("provider_timeout:")
                    {
                        tracing::warn!(page = page_num, error = %web_err, "hongguo search next page fetch failed; returning partial results");
                        break;
                    }
                    return Err(web_err);
                }
            }
        }
        if let Ok(mut guard) = self.search_cache.lock() {
            if guard.len() >= MAX_CACHED_SEARCHES {
                guard.retain(|_, v| v.saved_at.elapsed() < SEARCH_CACHE_TTL);
                if guard.len() >= MAX_CACHED_SEARCHES {
                    if let Some(oldest_key) = guard
                        .iter()
                        .min_by_key(|(_, v)| v.saved_at)
                        .map(|(k, _)| k.clone())
                    {
                        guard.remove(&oldest_key);
                    }
                }
            }
            guard.insert(key, cache.clone());
        }
        Ok((cache.items, cache.has_more))
    }

    fn fetch_series_detail(
        &self,
        series_id: &str,
        request: Option<&RequestContext>,
    ) -> Result<Option<SeriesDetail>, String> {
        if !is_numeric_id(series_id) {
            return Err("hg_invalid_series: 剧集 ID 格式无效".into());
        }
        if let Some(ctx) = request {
            ctx.check()?;
        }
        if let Ok(guard) = self.detail_cache.lock() {
            if let Some(cached) = guard.get(series_id) {
                if cached.saved_at.elapsed() < CATALOG_CACHE_TTL {
                    return Ok(Some(cached.detail.clone()));
                }
            }
        }
        let html = self.fetch_site_text(&format!("{SITE}/detail?series_id={series_id}"), false)?;
        if let Some(ctx) = request {
            ctx.check()?;
        }
        let router = extract_router_data(&html)?;
        let Some(series_json) = router
            .get("loaderData")
            .and_then(|l| l.get("detail_page"))
            .and_then(|d| d.get("seriesDetail"))
            .filter(|v| v.is_object())
        else {
            return Ok(None);
        };
        let Some(vid_list) = series_json.get("vid_list").and_then(Value::as_array) else {
            return Err("hg_detail: 该剧集暂无可用分集".into());
        };

        let mut series = map_ssr_item(series_json).ok_or("hg_detail: 剧集元数据解析失败")?;
        series.id = series_id.to_owned();

        // Parse optional per-episode metadata if upstream ever supplies it alongside `vid_list`.
        let mut ep_meta: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
        for key in ["episode_list", "item_list", "video_list"] {
            if let Some(arr) = series_json.get(key).and_then(Value::as_array) {
                for item in arr {
                    let Some(vid) = ["vid", "video_id", "item_id"]
                        .iter()
                        .map(|k| json_str(item, k))
                        .find(|v| is_numeric_id(v))
                    else {
                        continue;
                    };
                    let title = ["episode_title", "item_title", "title", "name"]
                        .iter()
                        .map(|k| sanitize_text(&json_str(item, k), 80))
                        .find(|s| !s.is_empty());
                    let dur_sec = item
                        .get("duration")
                        .or_else(|| item.get("video_duration"))
                        .and_then(Value::as_f64)
                        .filter(|d| d.is_finite() && *d > 0.0)
                        .map(|d| {
                            let secs = if d > 36_000.0 {
                                (d / 1000.0).round() as u64
                            } else {
                                d.round() as u64
                            };
                            format!("{:02}:{:02}", secs / 60, secs % 60)
                        });
                    ep_meta.insert(vid, (title, dur_sec));
                }
            }
        }

        let mut seen = HashSet::new();
        let mut episodes = Vec::new();
        for (idx, vid_val) in vid_list.iter().take(MAX_REAL_EPISODES).enumerate() {
            let vid = match vid_val {
                Value::String(s) => s.trim().to_owned(),
                Value::Number(n) => n.to_string(),
                _ => continue,
            };
            if !is_numeric_id(&vid) || !seen.insert(vid.clone()) {
                continue;
            }
            let num = idx + 1;
            let (raw_ep_title, dur) = ep_meta.remove(&vid).unwrap_or((None, None));
            let title = match raw_ep_title {
                Some(t)
                    if t.contains(&format!("第{num}集")) || t.contains(&format!("第 {num} 集")) =>
                {
                    t
                }
                Some(t) => format!("第 {num} 集 · {t}"),
                None => format!("{} · 第 {num} 集", series.title),
            };
            episodes.push(Episode {
                id: vid,
                number: num,
                title,
                runtime_label: dur.unwrap_or_default(),
                playable: true,
            });
        }
        if episodes.is_empty() {
            return Err("hg_detail: 分集列表为空".into());
        }

        series.episode_count = episodes.len();
        let detail = SeriesDetail { series, episodes };

        if let Ok(mut guard) = self.detail_cache.lock() {
            if guard.len() >= MAX_CACHED_DETAILS {
                guard.retain(|_, v| v.saved_at.elapsed() < CATALOG_CACHE_TTL);
                if guard.len() >= MAX_CACHED_DETAILS {
                    if let Some(oldest) = guard
                        .iter()
                        .min_by_key(|(_, v)| v.saved_at)
                        .map(|(k, _)| k.clone())
                    {
                        guard.remove(&oldest);
                    }
                }
            }
            guard.insert(
                series_id.to_owned(),
                CachedDetail {
                    detail: detail.clone(),
                    saved_at: Instant::now(),
                },
            );
        }
        Ok(Some(detail))
    }

    /// Primary Android v5 POST (`preload/v1` for 7.4.5.32 Issue #102, or `player/v1` legacy) with gzip body.
    fn fetch_v5_post_model(
        &self,
        video_id: &str,
        identity: &DeviceIdentity,
        host: &str,
        current_app: bool,
    ) -> ModelOutcome {
        let device_level = if current_app { 2 } else { 1 };
        let dr_scene = if current_app { "preload" } else { "player" };
        let payload = serde_json::json!({
            "biz_param": {
                "detail_page_version": 0,
                "device_level": device_level,
                "disable_digg_stat": false,
                "disable_video_relate_book": false,
                "need_all_video_definition": true,
                "need_mp4_align": false,
                "use_os_player": false,
                "use_server_dns": false,
                "video_platform": 1024
            },
            "mixed_video_id_map": { "1004": [video_id] },
            "dr_scene": dr_scene
        });
        let gz_bytes = gzip_compress(payload.to_string().as_bytes());
        let endpoint = if current_app {
            format!("https://{host}/novel/player/multi_video_model/preload/v1")
        } else {
            format!("https://{host}/novel/player/multi_video_model/player/v1")
        };
        let params = if current_app {
            identity.v5_current_app_params()
        } else {
            identity.v5_query_params()
        };
        let ua = if current_app { PRELOAD_UA } else { V5_POST_UA };
        let signed =
            match self
                .v5_signer
                .sign_v5_playback(&endpoint, &params, Some(&gz_bytes), now_millis())
            {
                Ok(s) => s,
                Err(e) => return ModelOutcome::Failure(e),
            };
        let mut headers: Vec<(&str, &str)> = vec![
            ("User-Agent", ua),
            ("Accept", "application/json"),
            ("Content-Encoding", "gzip"),
            ("Content-Type", "application/json; charset=utf-8"),
        ];
        for (k, v) in &signed.headers {
            headers.push((k.as_str(), v.as_str()));
        }
        let resp = match request_with_options(
            "POST",
            &signed.url,
            &headers,
            Some(&gz_bytes),
            MAX_MODEL_BYTES,
            RequestOptions {
                user_agent: Some(ua),
                timeout_ms: 8_000,
                decompress: true,
                disable_cookies: true,
            },
        ) {
            Ok(r) => r,
            Err(e) => return ModelOutcome::Failure(e),
        };
        if !(200..300).contains(&resp.status) {
            return ModelOutcome::Failure(format!(
                "hg_play: 播放模型请求被拒绝（HTTP {}）",
                resp.status
            ));
        }
        let text = String::from_utf8_lossy(&resp.body);
        if text.trim().is_empty() {
            return ModelOutcome::EmptyResponse;
        }
        let Ok(root) = serde_json::from_str::<Value>(text.trim()) else {
            return ModelOutcome::Failure("hg_play: 播放模型响应格式异常".into());
        };
        if let Err(e) = check_upstream_code(&root) {
            return ModelOutcome::Failure(e);
        }
        let Some(data) = root.get("data").filter(|v| v.is_object()) else {
            return ModelOutcome::Failure("hg_play: 播放模型缺少 data 字段".into());
        };
        let entries = data.get("1004").filter(|v| v.is_object()).unwrap_or(data);
        let Some(model) =
            parse_json_object(entries.get(video_id).and_then(|e| e.get("video_model")))
        else {
            return ModelOutcome::NoCompatibleRenditions;
        };
        extract_renditions_from_model(&model, &|url| self.fetch_fallback_text(url))
    }

    /// Issue #83 recovery: Android v5 GET `/reading/bookapi/video_tab/multi_video_model/v1/`
    /// signed with MD5(empty bytes), sending no body, Content-Type, Content-Encoding or x-ss-stub.
    fn fetch_v5_video_tab_model(
        &self,
        video_id: &str,
        identity: &DeviceIdentity,
        host: &str,
    ) -> ModelOutcome {
        let mut params = identity.v5_current_app_params();
        params.extend([
            ("video_id".into(), video_id.to_owned()),
            ("video_id_type".into(), "0".into()),
            ("device_level".into(), "1".into()),
            ("need_all_video_definition".into(), "1".into()),
            ("need_mp4_align".into(), "0".into()),
            ("use_os_player".into(), "0".into()),
            ("video_platform".into(), "1024".into()),
            ("source".into(), "0".into()),
            ("from_video_id".into(), String::new()),
        ]);
        let base = format!("https://{host}/reading/bookapi/video_tab/multi_video_model/v1/");
        let signed = match self
            .v5_signer
            .sign_v5_playback(&base, &params, Some(&[]), now_millis())
        {
            Ok(s) => s,
            Err(e) => return ModelOutcome::Failure(e),
        };
        let mut headers: Vec<(&str, &str)> = vec![
            ("User-Agent", V5_TAB_UA),
            ("Accept", "application/json; charset=utf-8"),
            ("Accept-Language", "zh-CN,zh;q=0.9"),
        ];
        for (k, v) in &signed.headers {
            if !k.eq_ignore_ascii_case("x-ss-stub")
                && !k.eq_ignore_ascii_case("content-type")
                && !k.eq_ignore_ascii_case("content-encoding")
            {
                headers.push((k.as_str(), v.as_str()));
            }
        }
        let resp = match request_with_options(
            "GET",
            &signed.url,
            &headers,
            None,
            MAX_MODEL_BYTES,
            RequestOptions {
                user_agent: Some(V5_TAB_UA),
                timeout_ms: 8_000,
                decompress: true,
                disable_cookies: true,
            },
        ) {
            Ok(r) => r,
            Err(e) => return ModelOutcome::Failure(e),
        };
        if !(200..300).contains(&resp.status) {
            return ModelOutcome::Failure(format!(
                "hg_play: 备用播放请求被拒绝（HTTP {}）",
                resp.status
            ));
        }
        let text = String::from_utf8_lossy(&resp.body);
        if text.trim().is_empty() {
            return ModelOutcome::EmptyResponse;
        }
        let Ok(root) = serde_json::from_str::<Value>(text.trim()) else {
            return ModelOutcome::Failure("hg_play: 备用播放响应格式异常".into());
        };
        if let Err(e) = check_upstream_code(&root) {
            return ModelOutcome::Failure(e);
        }
        let Some(data) = root.get("data").filter(|v| v.is_object()) else {
            return ModelOutcome::Failure("hg_play: 备用播放响应缺少 data".into());
        };
        let rows = data
            .get("video_model_data")
            .filter(|v| v.is_object())
            .unwrap_or(data);
        let Some(model) = parse_json_object(rows.get(video_id).and_then(|e| e.get("video_model")))
        else {
            return ModelOutcome::NoCompatibleRenditions;
        };
        extract_renditions_from_model(&model, &|url| self.fetch_fallback_text(url))
    }

    fn resolve_renditions_with_identity(
        &self,
        video_id: &str,
        identity: &DeviceIdentity,
    ) -> Result<Vec<Rendition>, (String, bool)> {
        let mut all_empty = true;
        let mut last_err = "hg_play: 播放接口暂时返回空响应，请稍后重试".to_string();

        // 1. Primary v5 POST (Issue #102 / 0.16.14: reading.snssdk.com preload/v1, 7.4.5.32 profile)
        match self.fetch_v5_post_model(video_id, identity, PRIMARY_MODEL_HOST, true) {
            ModelOutcome::Renditions(rows) => return Ok(rows),
            ModelOutcome::EmptyResponse => {}
            ModelOutcome::NoCompatibleRenditions => {
                all_empty = false;
                last_err = "hg_play: 当前集暂无兼容清晰度".into();
            }
            ModelOutcome::Failure(err) => {
                all_empty = false;
                last_err = err;
            }
        }

        // 2. Issue #83 recovery: v5 GET video_tab on sinfonlineb
        match self.fetch_v5_video_tab_model(video_id, identity, ALT_MODEL_HOSTS[0]) {
            ModelOutcome::Renditions(rows) => return Ok(rows),
            ModelOutcome::EmptyResponse => {}
            ModelOutcome::NoCompatibleRenditions => {
                all_empty = false;
                last_err = "hg_play: 备用接口仅返回不兼容编码".into();
            }
            ModelOutcome::Failure(err) => {
                all_empty = false;
                last_err = err;
            }
        }

        // 3. Alternate host v5 POST (sinfonlineb, then sinfonlinea)
        for &host in &ALT_MODEL_HOSTS[..2] {
            match self.fetch_v5_post_model(video_id, identity, host, true) {
                ModelOutcome::Renditions(rows) => return Ok(rows),
                ModelOutcome::EmptyResponse => {}
                ModelOutcome::NoCompatibleRenditions => {
                    all_empty = false;
                    last_err = "hg_play: 没有可播放的兼容视频资源".into();
                }
                ModelOutcome::Failure(err) => {
                    all_empty = false;
                    last_err = err;
                }
            }
        }

        Err((last_err, all_empty))
    }

    fn fetch_or_cached_renditions(&self, video_id: &str) -> Result<Vec<Rendition>, String> {
        if let Ok(guard) = self.rendition_cache.lock() {
            if let Some(cached) = guard.get(video_id) {
                if cached.saved_at.elapsed() < RENDITION_CACHE_TTL && !cached.rows.is_empty() {
                    return Ok(cached.rows.clone());
                }
            }
        }
        let identity = self.device_session.obtain()?;
        let rows = match self.resolve_renditions_with_identity(video_id, &identity) {
            Ok(rows) => rows,
            Err((err, true)) => {
                // All endpoints returned empty HTTP 200: rotate anonymous device once (cooldown-guarded).
                if let Ok(Some(fresh)) = self.device_session.rotate_if_allowed(&identity.device_id)
                {
                    self.resolve_renditions_with_identity(video_id, &fresh)
                        .map_err(|(e, _)| e)?
                } else {
                    return Err(err);
                }
            }
            Err((err, false)) => return Err(err),
        };
        if rows.is_empty() {
            return Err("hg_play: 没有可播放的视频清晰度，请稍后重试".into());
        }
        Ok(rows)
    }

    fn save_renditions(&self, video_id: &str, rows: Vec<Rendition>, active_quality: u32) {
        if let Ok(mut guard) = self.rendition_cache.lock() {
            if guard.len() >= MAX_CACHED_RENDITIONS {
                guard.retain(|_, v| v.saved_at.elapsed() < RENDITION_CACHE_TTL);
                if guard.len() >= MAX_CACHED_RENDITIONS {
                    if let Some(oldest) = guard
                        .iter()
                        .min_by_key(|(_, v)| v.saved_at)
                        .map(|(k, _)| k.clone())
                    {
                        guard.remove(&oldest);
                    }
                }
            }
            guard.insert(
                video_id.to_owned(),
                CachedRenditions {
                    rows,
                    active_quality,
                    saved_at: Instant::now(),
                },
            );
        }
    }

    fn resolve_rendition_for_quality(
        &self,
        video_id: &str,
        requested_quality: Option<u32>,
    ) -> Result<Rendition, String> {
        let rows = self.fetch_or_cached_renditions(video_id)?;
        let cap = match requested_quality {
            Some(0) => {
                self.preferred_quality.store(0, Ordering::Relaxed);
                u32::MAX
            }
            Some(q) => {
                self.preferred_quality.store(q, Ordering::Relaxed);
                q
            }
            None => {
                let saved = self.preferred_quality.load(Ordering::Relaxed);
                if saved == 0 {
                    u32::MAX
                } else {
                    saved
                }
            }
        };
        let picked = select_rendition(&rows, cap)
            .ok_or_else(|| "hg_play: 没有可播放的视频清晰度，请稍后重试".to_string())?;
        self.save_renditions(video_id, rows, picked.quality);
        Ok(picked)
    }

    #[cfg(test)]
    fn resolve_best_rendition(&self, video_id: &str) -> Result<Rendition, String> {
        self.resolve_rendition_for_quality(video_id, None)
    }

    fn quality_options_for(&self, video_id: &str) -> Result<Vec<QualityOption>, String> {
        let cached = self
            .rendition_cache
            .lock()
            .ok()
            .and_then(|g| g.get(video_id).cloned())
            .filter(|c| c.saved_at.elapsed() < RENDITION_CACHE_TTL && !c.rows.is_empty());
        let (rows, active_quality) = match cached {
            Some(c) => (c.rows, c.active_quality),
            None => {
                let rows = self.fetch_or_cached_renditions(video_id)?;
                let pref = self.preferred_quality.load(Ordering::Relaxed);
                let cap = if pref == 0 { u32::MAX } else { pref };
                let active = select_rendition(&rows, cap).map(|r| r.quality).unwrap_or(0);
                self.save_renditions(video_id, rows.clone(), active);
                (rows, active)
            }
        };
        // Deduplicate by quality tier, preferring AVC over HEVC at the same resolution.
        let mut by_tier: BTreeMap<u32, String> = BTreeMap::new();
        for r in &rows {
            let codec_label = match r.codec.to_ascii_lowercase().as_str() {
                "h264" | "avc" | "avc1" | "avc3" => "H.264",
                "bytevc1" | "hevc" | "h265" | "hvc1" | "hev1" => "HEVC",
                _ => "MP4",
            };
            let entry = by_tier
                .entry(r.quality)
                .or_insert_with(|| codec_label.to_owned());
            if codec_label == "H.264" {
                *entry = "H.264".into();
            }
        }
        let mut out: Vec<QualityOption> = by_tier
            .into_iter()
            .rev()
            .map(|(quality, codec)| QualityOption {
                quality,
                label: if quality > 0 {
                    format!("{quality}P")
                } else {
                    "自动".into()
                },
                codec,
                selected: quality == active_quality,
            })
            .collect();
        if !out.is_empty() && !out.iter().any(|o| o.selected) {
            out[0].selected = true;
        }
        Ok(out)
    }
}

impl ContentProvider for HongGuoRealProvider {
    fn id(&self) -> &'static str {
        "hongguo"
    }

    fn name(&self) -> &'static str {
        "红果短剧"
    }

    fn supports_search(&self) -> bool {
        true
    }

    fn supports_detail(&self) -> bool {
        true
    }

    fn supports_play(&self) -> bool {
        true
    }

    fn stable_progress_key(&self, series_id: &str, episode_id: &str) -> Option<String> {
        if is_numeric_id(series_id) && is_numeric_id(episode_id) {
            Some(format!("hongguo:{series_id}:{episode_id}"))
        } else {
            None
        }
    }

    fn supports_poster_extract(&self) -> bool {
        false
    }

    fn search(&self, query: &str) -> Result<Vec<Series>, String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            Ok(self.fetch_home_until(24, None)?.0)
        } else {
            Ok(self.fetch_search_until(trimmed, 20, None)?.0)
        }
    }

    fn search_with_request(
        &self,
        query: &str,
        request: &RequestContext,
    ) -> Result<Vec<Series>, String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            Ok(self.fetch_home_until(24, Some(request))?.0)
        } else {
            Ok(self.fetch_search_until(trimmed, 20, Some(request))?.0)
        }
    }

    fn search_until_with_request(
        &self,
        query: &str,
        needed: usize,
        request: Option<&RequestContext>,
    ) -> Result<(Vec<Series>, bool), String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.fetch_home_until(needed, request)
        } else {
            self.fetch_search_until(trimmed, needed, request)
        }
    }

    fn detail(&self, series_id: &str) -> Result<Option<SeriesDetail>, String> {
        self.fetch_series_detail(series_id, None)
    }

    fn detail_with_request(
        &self,
        series_id: &str,
        request: &RequestContext,
    ) -> Result<Option<SeriesDetail>, String> {
        self.fetch_series_detail(series_id, Some(request))
    }

    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<MediaTarget, String> {
        self.resolve_episode_with_quality(series_id, episode_id, None)
    }

    fn resolve_episode_with_quality(
        &self,
        series_id: &str,
        episode_id: &str,
        quality: Option<u32>,
    ) -> Result<MediaTarget, String> {
        if !is_numeric_id(series_id) || !is_numeric_id(episode_id) {
            return Err("hg_invalid_episode: 分集 ID 无效".into());
        }
        let rendition = self.resolve_rendition_for_quality(episode_id, quality)?;
        let (loopback_url, token_header) = self.stream_proxy.open_rendition(&rendition)?;
        Ok(MediaTarget::LocalStream {
            url: loopback_url,
            token_header,
            stable_key: format!("hongguo:{series_id}:{episode_id}"),
        })
    }

    fn episode_qualities(
        &self,
        series_id: &str,
        episode_id: &str,
    ) -> Result<Vec<QualityOption>, String> {
        if !is_numeric_id(series_id) || !is_numeric_id(episode_id) {
            return Ok(Vec::new());
        }
        self.quality_options_for(episode_id)
    }

    fn episode_danmaku(
        &self,
        series_id: &str,
        episode_id: &str,
        duration_ms: Option<u64>,
    ) -> Result<Vec<DanmakuItem>, String> {
        if !is_numeric_id(series_id) || !is_numeric_id(episode_id) {
            return Ok(Vec::new());
        }
        self.danmaku
            .fetch_episode(series_id, episode_id, duration_ms)
    }
}

// ============================================================================
// Legacy HG-1 Contract Types (kept intact for existing unit tests)
// ============================================================================

#[allow(dead_code)]
const MAX_ITEM_COUNT: usize = 50;
#[allow(dead_code)]
const MAX_EPISODES: usize = 200;
#[allow(dead_code)]
const MAX_ID_BYTES: usize = 96;
#[allow(dead_code)]
const MAX_TITLE_CHARS: usize = 100;

#[allow(dead_code)]
pub trait ApprovedCatalogTransport: Send + Sync {
    fn search(&self, query: &str) -> Result<Vec<ApprovedSeries>, String>;
    fn detail(&self, id: &str) -> Result<Option<ApprovedSeriesDetail>, String>;
    fn play(&self, id: &str, episode_id: &str) -> Result<String, String>;
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ApprovedSeries {
    pub id: String,
    pub title: String,
    pub description: String,
    pub genre: String,
    pub episodes: usize,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ApprovedSeriesDetail {
    pub series: ApprovedSeries,
    pub episodes: Vec<ApprovedEpisode>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ApprovedEpisode {
    pub id: String,
    pub number: usize,
    pub title: String,
}

#[allow(dead_code)]
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

#[allow(dead_code)]
fn valid_text(text: &str, max_chars: usize) -> bool {
    !text.is_empty() && text.chars().count() <= max_chars && !text.chars().any(char::is_control)
}

#[allow(dead_code)]
fn check_series(item: &ApprovedSeries) -> Result<(), String> {
    if !valid_id(&item.id)
        || !valid_text(&item.title, MAX_TITLE_CHARS)
        || !valid_text(&item.description, 500)
        || !valid_text(&item.genre, 40)
        || item.episodes == 0
        || item.episodes > MAX_EPISODES
    {
        return Err("hg_invalid_catalog: Provider data is not valid".into());
    }
    Ok(())
}

#[allow(dead_code)]
fn safe_series(item: ApprovedSeries) -> Result<Series, String> {
    check_series(&item)?;
    Ok(Series {
        id: item.id,
        title: item.title,
        description: item.description,
        genre: item.genre,
        badge: "授权样例（非红果真实内容）".into(),
        artwork: "studio".into(),
        cover_url: None,
        episode_count: item.episodes,
        provider_id: "hongguo".into(),
    })
}

#[allow(dead_code)]
pub fn allowed_media_url(value: &str, approved_host: &str) -> Result<String, String> {
    let verified = public_https_url(value).map_err(|_| "hg_invalid_media_url")?;
    if !valid_public_host(approved_host) {
        return Err("hg_unapproved_host: Invalid approved hostname".into());
    }
    let authority = verified[8..].split(['/', '?']).next().unwrap_or("");
    let origin = authority
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if origin != approved_host.to_ascii_lowercase() {
        return Err("hg_unapproved_host: Media domain does not match agreement".into());
    }
    Ok(verified)
}

fn valid_public_host(host: &str) -> bool {
    if host.len() > 253 || host.is_empty() || host.contains(':') {
        return false;
    }
    let labels: Vec<_> = host.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
        && labels.last().is_some_and(|suffix| {
            suffix.len() >= 2 && suffix.bytes().all(|c| c.is_ascii_alphabetic())
        })
        && !["local", "internal", "localhost", "test", "invalid"]
            .contains(&labels.last().unwrap().to_ascii_lowercase().as_str())
}

#[allow(dead_code)]
#[derive(Default)]
pub struct HongGuoUnavailableProvider;

impl ContentProvider for HongGuoUnavailableProvider {
    fn id(&self) -> &'static str {
        "hongguo"
    }

    fn name(&self) -> &'static str {
        "红果接入（待授权）"
    }

    fn supports_search(&self) -> bool {
        false
    }

    fn supports_detail(&self) -> bool {
        false
    }

    fn supports_play(&self) -> bool {
        false
    }

    fn search(&self, _query: &str) -> Result<Vec<Series>, String> {
        Ok(Vec::new())
    }

    fn detail(&self, _series_id: &str) -> Result<Option<SeriesDetail>, String> {
        Ok(None)
    }

    fn resolve_episode(&self, _series_id: &str, _episode_id: &str) -> Result<MediaTarget, String> {
        Err("hg_not_authorized: 红果播放接口尚未授权".into())
    }
}

#[allow(dead_code)]
pub struct HongGuoAuthorizedProvider {
    transport: Arc<dyn ApprovedCatalogTransport>,
    media_host: String,
}

#[allow(dead_code)]
impl HongGuoAuthorizedProvider {
    pub fn for_approved_transport(
        transport: Arc<dyn ApprovedCatalogTransport>,
        media_host: String,
    ) -> Result<Self, String> {
        if !valid_public_host(&media_host) {
            return Err("hg_unapproved_host: Invalid approved hostname".into());
        }
        Ok(Self {
            transport,
            media_host,
        })
    }
}

impl ContentProvider for HongGuoAuthorizedProvider {
    fn id(&self) -> &'static str {
        "hongguo"
    }

    fn name(&self) -> &'static str {
        "红果适配接口（尚未启用）"
    }

    fn search(&self, query: &str) -> Result<Vec<Series>, String> {
        if query.len() > 128 || query.chars().any(char::is_control) {
            return Err("hg_invalid_query: Invalid query".into());
        }
        let items = self
            .transport
            .search(query)
            .map_err(|_| "hg_search_unavailable")?;
        if items.len() > MAX_ITEM_COUNT {
            return Err("hg_invalid_catalog: Too many results".into());
        }
        let mut seen = HashSet::new();
        items
            .into_iter()
            .map(|item| {
                if !seen.insert(item.id.clone()) {
                    return Err("hg_invalid_catalog: Duplicate series ID".into());
                }
                safe_series(item)
            })
            .collect()
    }

    fn detail(&self, series_id: &str) -> Result<Option<SeriesDetail>, String> {
        if !valid_id(series_id) {
            return Err("hg_invalid_series: Invalid series ID".into());
        }
        let details = self
            .transport
            .detail(series_id)
            .map_err(|_| "hg_detail_unavailable")?;
        let Some(details) = details else {
            return Ok(None);
        };
        if details.series.id != series_id || details.episodes.len() != details.series.episodes {
            return Err("hg_invalid_catalog: Episode count mismatch".into());
        }
        let series = safe_series(details.series)?;
        let mut ids = HashSet::new();
        let mut numbers = HashSet::new();
        let mut episodes = Vec::with_capacity(details.episodes.len());
        for item in details.episodes {
            if !valid_id(&item.id)
                || item.number == 0
                || item.number > MAX_EPISODES
                || !valid_text(&item.title, MAX_TITLE_CHARS)
                || !ids.insert(item.id.clone())
                || !numbers.insert(item.number)
            {
                return Err("hg_invalid_catalog: Invalid episode".into());
            }
            episodes.push(Episode {
                id: item.id,
                number: item.number,
                title: item.title,
                runtime_label: "—".into(),
                playable: true,
            });
        }
        episodes.sort_by_key(|episode| episode.number);
        Ok(Some(SeriesDetail { series, episodes }))
    }

    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<MediaTarget, String> {
        if !valid_id(series_id) || !valid_id(episode_id) {
            return Err("hg_invalid_episode: Invalid episode ID".into());
        }
        let details = self
            .detail(series_id)?
            .ok_or("hg_unknown_series: Series not found")?;
        if !details.episodes.iter().any(|item| item.id == episode_id) {
            return Err("hg_unknown_episode: Episode not found".into());
        }
        let url = self
            .transport
            .play(series_id, episode_id)
            .map_err(|_| "hg_play_unavailable")?;
        allowed_media_url(&url, &self.media_host).map(MediaTarget::Https)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_data_parser_handles_escaped_quotes_and_nested_braces() {
        let html =
            r#"<script>window._ROUTER_DATA = {"loaderData":{"x":"a}\"b","list":[1,2]}};</script>"#;
        let data = extract_router_data(html).unwrap();
        assert_eq!(data["loaderData"]["x"], "a}\"b");
    }

    #[test]
    fn ssr_item_mapper_extracts_series_metadata_and_sanitizes_cover() {
        let raw = serde_json::json!({
            "video_data": {
                "series_id": "7703065438208412697",
                "series_name": "测试短剧",
                "series_cover": "https:\\/\\/p3-novel.byteimg.com\\/poster.jpg?x=1\\u0026y=2",
                "series_intro": "精彩剧情",
                "episode_cnt": 80,
                "tags": ["古装", "逆袭"]
            }
        });
        let mapped = map_ssr_item(&raw).unwrap();
        assert_eq!(mapped.id, "7703065438208412697");
        assert_eq!(mapped.title, "测试短剧");
        assert_eq!(mapped.genre, "古装");
        assert_eq!(mapped.episode_count, 80);
        assert_eq!(
            mapped.cover_url.as_deref(),
            Some("https://p3-novel.byteimg.com/poster.jpg?x=1&y=2")
        );
    }

    #[test]
    fn production_placeholder_never_exposes_authorized_mock_or_a_playable_target() {
        let slot = HongGuoUnavailableProvider;
        assert_eq!(slot.id(), "hongguo");
        assert!(slot.name().contains("待授权"));
        assert!(!slot.supports_search());
        assert!(!slot.supports_detail());
        assert!(!slot.supports_play());
        assert!(slot.search("").unwrap().is_empty());
        assert!(slot.detail("synthetic-1").unwrap().is_none());
        assert!(slot.resolve_episode("synthetic-1", "ep-1").is_err());
    }

    #[test]
    fn approved_hostname_blocks_cross_domain_and_private_media() {
        for url in [
            "http://media.example.org/a.mp4",
            "https://media.example.org.evil.net/a.mp4",
            "https://127.0.0.1/a.mp4",
            "https://user:pass@media.example.org/a.mp4",
            "https://media.example.org:8443/a.mp4",
            "https://media.example.org/a.m3u8",
            "https://media.example.org/a.mp4#frag",
        ] {
            assert!(allowed_media_url(url, "media.example.org").is_err());
        }
        assert!(allowed_media_url("https://media.example.org/a.mp4", "localhost").is_err());
        assert!(allowed_media_url("https://media.example.org/a.mp4", "media.example.org").is_ok());
    }

    #[test]
    fn progressive_feed_cache_deduplicates_across_pages_and_stops_when_exhausted() {
        let mk = |id: &str| Series {
            id: id.into(),
            title: format!("Drama {id}"),
            description: String::new(),
            genre: "都市".into(),
            badge: "全 60 集".into(),
            artwork: "studio".into(),
            cover_url: None,
            episode_count: 60,
            provider_id: "hongguo".into(),
        };
        let mut cache = ProgressiveFeedCache::new();
        cache.append_page(vec![mk("1"), mk("2")], true);
        assert_eq!(cache.items.len(), 2);
        assert_eq!(cache.next_page, 2);
        assert!(cache.has_more);

        // Page 2 has one duplicate ("2") and one new ("3")
        cache.append_page(vec![mk("2"), mk("3")], true);
        assert_eq!(cache.items.len(), 3);
        assert_eq!(cache.next_page, 3);
        assert!(cache.has_more);

        // Page 3 repeats only already-seen items -> has_more flips false to avoid infinite loop
        cache.append_page(vec![mk("1"), mk("3")], true);
        assert_eq!(cache.items.len(), 3);
        assert!(!cache.has_more);
    }

    /// Manual real-network verification probe (excluded from default offline `cargo test`).
    /// Run with: `cargo test --locked live_probe_hongguo_end_to_end -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_probe_hongguo_end_to_end() {
        let provider = HongGuoRealProvider::new();
        let home = provider.search("").expect("home SSR fetch must succeed");
        assert!(!home.is_empty(), "home catalog should not be empty");
        let first = &home[0];
        println!(
            "HOME ok: count={}, first_id={}, title={}, genre={}, cover_present={}",
            home.len(),
            first.id,
            first.title,
            first.genre,
            first.cover_url.is_some()
        );
        if let Some(cover) = &first.cover_url {
            if let Ok(resp) = request_with_options(
                "HEAD",
                cover,
                &[],
                None,
                64 * 1024,
                RequestOptions {
                    user_agent: Some("Mozilla/5.0"),
                    timeout_ms: 6_000,
                    decompress: false,
                    disable_cookies: true,
                },
            ) {
                println!(
                    "COVER probe: status={}, content_type={:?}",
                    resp.status,
                    resp.header("content-type")
                );
            }
        }
        let search = provider.search("重生").expect("search must succeed");
        println!("SEARCH ok: count={}", search.len());
        assert!(!search.is_empty(), "search should return results");

        let detail = provider
            .detail(&first.id)
            .expect("detail must succeed")
            .expect("series should exist");
        println!(
            "DETAIL ok: series={}, episodes={}",
            detail.series.title,
            detail.episodes.len()
        );
        assert!(!detail.episodes.is_empty());

        let ep = &detail.episodes[0];
        let rendition = provider
            .resolve_best_rendition(&ep.id)
            .expect("v5 playback rendition resolution must succeed");
        let qualities = provider
            .episode_qualities(&first.id, &ep.id)
            .expect("episode qualities must resolve");
        let danmaku = provider
            .episode_danmaku(&first.id, &ep.id, None)
            .expect("official timed danmaku fetch must succeed");
        println!(
            "RENDITION ok: quality={}P, codec={}, encrypted={}, has_backup={}, tiers={:?}, danmaku_count={}",
            rendition.quality,
            rendition.codec,
            rendition.key.is_some(),
            rendition.backup_url.is_some(),
            qualities.iter().map(|q| format!("{}({})", q.label, q.codec)).collect::<Vec<_>>(),
            danmaku.len()
        );
        assert!(
            !qualities.is_empty(),
            "should expose at least one quality tier"
        );

        let session = crate::catalog::hg_stream::RemoteVideoSession::prepare(&rendition)
            .expect("CDN header prepare & CENC parse must succeed");
        let head = session
            .read_decrypted(0, 65535)
            .expect("decrypted 64KiB read must succeed");
        assert!(head.len() >= 16);
        assert_eq!(&head[4..8], b"ftyp");
        assert!(!head.windows(4).any(|w| w == b"encv"));
        assert!(!head.windows(4).any(|w| w == b"sinf"));
        println!(
            "STREAM ok: total_bytes={}, decrypted_head_bytes={}, ftyp_valid=true",
            session.total,
            head.len()
        );
    }
}
