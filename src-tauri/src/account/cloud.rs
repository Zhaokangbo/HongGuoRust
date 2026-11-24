//! Signed HongGuo cloud account API client.
//! Adapted from N3urda/hongguoTV (`CloudAccount.kt`, GPL-3.0-only).
//!
//! Security boundary:
//! - User session cookies are ONLY attached to signed `api5-normal-sinfonlinea.fqnovel.com`
//!   account endpoints and are NEVER forwarded to playback, danmaku, or CDN hosts.
//! - Uses `request_no_redirects` so upstream redirects can never leak the `Cookie` header.

use super::store::AccountCredentials;
use crate::catalog::hg_media::clean_url;
use crate::catalog::hg_signer::{now_millis, Signer, APP_UA};
use crate::catalog::http_client::{request_no_redirects, RequestOptions};
use crate::catalog::Series;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

const CLOUD_API_ORIGIN: &str = "https://api5-normal-sinfonlinea.fqnovel.com";
const MAX_CLOUD_BYTES: usize = 2 * 1024 * 1024;
const CLOUD_TIMEOUT_MS: i32 = 12_000;

#[derive(Debug, Clone)]
pub struct CloudProfile {
    pub user_id: String,
    pub nickname: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudFavorite {
    pub series: Series,
    pub added_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudHistoryEntry {
    pub series: Series,
    pub episode_id: String,
    pub episode_number: usize,
    pub episode_title: String,
    pub position_sec: f64,
    pub duration_sec: Option<f64>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudReservation {
    pub series: Series,
    pub online: bool,
    pub publish_at_ms: u64,
    pub publish_text: String,
}

pub struct CloudAccountClient {
    signer: Signer,
}

impl Default for CloudAccountClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudAccountClient {
    pub fn new() -> Self {
        Self {
            signer: Signer::new(false),
        }
    }

    fn device_params(creds: &AccountCredentials, is_post: bool) -> Vec<(String, String)> {
        let ver_code = if is_post { "73932" } else { "72232" };
        let ver_name = if is_post { "7.3.9.32" } else { "7.2.2.32" };
        let iid = if creds.install_id.is_empty() {
            "1905892595382586"
        } else {
            &creds.install_id
        };
        let did = if creds.device_id.is_empty() {
            "1905892595378490"
        } else {
            &creds.device_id
        };
        vec![
            ("iid".into(), iid.into()),
            ("device_id".into(), did.into()),
            ("ac".into(), "wifi".into()),
            ("channel".into(), "update_64".into()),
            ("aid".into(), "8662".into()),
            ("app_name".into(), "novelread".into()),
            ("version_code".into(), ver_code.into()),
            ("version_name".into(), ver_name.into()),
            ("device_platform".into(), "android".into()),
            ("os".into(), "android".into()),
            ("ssmix".into(), "a".into()),
            ("device_type".into(), "25053RT47C".into()),
            ("device_brand".into(), "Redmi".into()),
            ("language".into(), "zh".into()),
            ("os_api".into(), "36".into()),
            ("os_version".into(), "16".into()),
            ("manifest_version_code".into(), ver_code.into()),
            ("resolution".into(), "1280*2772".into()),
            ("dpi".into(), "520".into()),
            ("update_version_code".into(), ver_code.into()),
            ("host_abi".into(), "arm64-v8a".into()),
            ("dragon_device_type".into(), "phone".into()),
            ("pv_player".into(), ver_code.into()),
            ("compliance_status".into(), "0".into()),
            ("need_personal_recommend".into(), "1".into()),
            ("player_so_load".into(), "1".into()),
            ("is_android_pad_screen".into(), "0".into()),
        ]
    }

    fn request_json(
        &self,
        creds: &AccountCredentials,
        path: &str,
        extra_params: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<Value, String> {
        let mut params = Self::device_params(creds, body.is_some());
        for &(k, v) in extra_params {
            params.push((k.to_owned(), v.to_owned()));
        }
        let base_url = format!("{CLOUD_API_ORIGIN}{path}");
        let signed = self
            .signer
            .sign_v3_request(&base_url, &params, body, now_millis())?;

        let method = if body.is_some() { "POST" } else { "GET" };
        let mut headers: Vec<(&str, &str)> = Vec::with_capacity(signed.headers.len() + 2);
        headers.push(("Cookie", creds.cookie.as_str()));
        if body.is_some() {
            headers.push(("Content-Type", "application/json; charset=utf-8"));
        }
        for (k, v) in &signed.headers {
            headers.push((k.as_str(), v.as_str()));
        }

        let resp = request_no_redirects(
            method,
            &signed.url,
            &headers,
            body,
            MAX_CLOUD_BYTES,
            RequestOptions {
                user_agent: Some(APP_UA),
                timeout_ms: CLOUD_TIMEOUT_MS,
                decompress: true,
                disable_cookies: true,
            },
        )?;

        if !(200..300).contains(&resp.status) {
            return Err(format!(
                "account_http: 账号请求失败（HTTP {}）",
                resp.status
            ));
        }
        let text = String::from_utf8_lossy(&resp.body);
        if text.trim().is_empty() {
            let stage = match path {
                "/reading/user/info/v1/" => "账号资料",
                "/reading/bookapi/read_progress/list/v1/" => "云端历史列表",
                "/reading/bookapi/multi-detail/v1/" => "云端剧集资料",
                "/reading/bookapi/bookshelf/video/info/v1/" => "云端收藏",
                "/reading/user/subscribe/list/v0/" => "云端预约",
                _ => "账号",
            };
            return Err(format!(
                "account_empty: {stage}接口返回空响应，请稍后重试或重新登录"
            ));
        }
        let root: Value = serde_json::from_str(text.trim())
            .map_err(|_| "account_parse: 账号接口返回格式异常".to_string())?;
        let code = root.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 {
            return Err(format!(
                "account_api: 账号接口不可用（错误码 {code}），请重新登录或稍后重试"
            ));
        }
        Ok(root)
    }

    pub fn profile(&self, creds: &AccountCredentials) -> Result<CloudProfile, String> {
        let root = self.request_json(creds, "/reading/user/info/v1/", &[], None)?;
        let data = root
            .get("data")
            .and_then(Value::as_object)
            .ok_or("account_profile: 账号资料为空")?;
        let user_id = obj_str(data, "biz_user_id");
        if user_id.is_empty() {
            return Err(
                "account_not_provisioned: 该账号还没有红果阅读业务资料，请先在手机红果 App 登录一次后再扫码"
                    .into(),
            );
        }
        let nickname = {
            let raw = obj_str(data, "user_name");
            if raw.is_empty() {
                "红果用户".to_owned()
            } else {
                sanitize_text(&raw, 40)
            }
        };
        Ok(CloudProfile { user_id, nickname })
    }

    pub fn favorites(&self, creds: &AccountCredentials) -> Result<Vec<CloudFavorite>, String> {
        let root = self.request_json(
            creds,
            "/reading/bookapi/bookshelf/video/info/v1/",
            &[],
            None,
        )?;
        parse_favorites(&root)
    }

    pub fn set_favorite(
        &self,
        creds: &AccountCredentials,
        raw_series_id: &str,
        add: bool,
    ) -> Result<(), String> {
        if !is_numeric_id(raw_series_id) {
            return Err("account_favorite: 剧集编号无效".into());
        }
        let mut entry = json!({
            "book_id": raw_series_id,
            "book_type": 2,
            "modify_time": now_millis(),
            "video_shelf_operate_type": if add { 0 } else { 1 },
        });
        if !add {
            entry["group_name"] = Value::String(String::new());
        }
        let mut body = json!({
            "is_cancelled": false,
            "update_bookshelf_video_list": [entry],
        });
        if add {
            body["shark_extra"] = json!({
                "enter_from": "MainFragmentActivity",
                "previous_page": "",
                "page_list": "MainFragmentActivity",
                "is_active_behavior": "true",
                "inactive_type": "0",
            });
        }
        let payload = body.to_string();
        self.request_json(
            creds,
            "/reading/bookapi/bookshelf/video/update/v1/",
            &[],
            Some(payload.as_bytes()),
        )?;
        Ok(())
    }

    pub fn history(&self, creds: &AccountCredentials) -> Result<Vec<CloudHistoryEntry>, String> {
        let root = self.request_json(
            creds,
            "/reading/bookapi/read_progress/list/v1/",
            &[("book_types", "2"), ("limit", "50")],
            None,
        )?;
        let rows = root
            .get("video_data")
            .or_else(|| root.get("data").and_then(|d| d.get("video_data")))
            .and_then(Value::as_array)
            .ok_or("account_history: 云端历史数据格式已变化")?;
        let mut entries = parse_history_rows(rows);
        if entries.is_empty() {
            return Ok(entries);
        }

        let book_ids = entries
            .iter()
            .filter_map(|e| e.series.id.strip_prefix("hongguo:"))
            .collect::<Vec<_>>()
            .join(",");
        if !book_ids.is_empty() {
            if let Ok(detail_root) = self.request_json(
                creds,
                "/reading/bookapi/multi-detail/v1/",
                &[("book_id", &book_ids), ("book_type", "2")],
                None,
            ) {
                if let Some(arr) = detail_root.get("data").and_then(Value::as_array) {
                    let meta_map = parse_multi_detail_map(arr);
                    for entry in &mut entries {
                        if let Some(raw_id) = entry.series.id.strip_prefix("hongguo:") {
                            if let Some(meta) = meta_map.get(raw_id) {
                                let count = meta.episode_count.max(entry.series.episode_count);
                                entry.series = meta.clone();
                                entry.series.episode_count = count;
                                if count > 0 && entry.series.badge.is_empty() {
                                    entry.series.badge = format!("全 {count} 集");
                                }
                                entry.episode_title = format!(
                                    "{} · 第 {} 集",
                                    entry.series.title, entry.episode_number
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(entries)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upload_progress(
        &self,
        creds: &AccountCredentials,
        raw_series_id: &str,
        episode_id: &str,
        episode_number: usize,
        position_ms: u64,
        duration_ms: u64,
        episode_count: usize,
        updated_at_ms: u64,
    ) -> Result<(), String> {
        if !is_numeric_id(raw_series_id) || !is_numeric_id(episode_id) {
            return Err("account_upload: 剧集或分集编号无效".into());
        }
        let progress_rate = if duration_ms > 0 {
            let ratio = (position_ms as f64 / duration_ms as f64).clamp(0.0, 1.0);
            format!("{ratio:.4}")
        } else {
            "0".to_owned()
        };
        let body = json!({
            "books": [{
                "book_id": raw_series_id,
                "item_id": episode_id,
                "vid_index": episode_number.max(1),
                "current_play_time": position_ms,
                "current_total_time": duration_ms,
                "episode_count": episode_count,
                "read_timestamp_ms": updated_at_ms,
                "book_type": 2,
                "progress_rate": progress_rate,
                "player_cumulative_total_duration": position_ms,
            }],
            "write_mode": 1,
        });
        let payload = body.to_string();
        self.request_json(
            creds,
            "/reading/bookapi/read_progress/upload/v1/",
            &[],
            Some(payload.as_bytes()),
        )?;
        Ok(())
    }

    pub fn reservations(
        &self,
        creds: &AccountCredentials,
        online: bool,
    ) -> Result<Vec<CloudReservation>, String> {
        let online_flag = if online { "true" } else { "false" };
        let root = self.request_json(
            creds,
            "/reading/user/subscribe/list/v0/",
            &[
                ("offset", "0"),
                ("limit", "20"),
                ("is_online", online_flag),
                ("tab_type", "13"),
                ("subscribe_order_type", "0"),
                ("swipe_type", "0"),
                ("need_calendar_schema", "false"),
                ("subscribe_offset", "0"),
            ],
            None,
        )?;
        let mut rows = parse_reservations(&root, online)?;
        if !online {
            return Ok(rows);
        }
        let missing: Vec<String> = rows
            .iter()
            .filter(|r| r.series.episode_count == 0)
            .filter_map(|r| r.series.id.strip_prefix("hongguo:").map(str::to_owned))
            .collect();
        if missing.is_empty() {
            return Ok(rows);
        }
        // Optional metadata enrichment: failure must not hide valid reservations.
        if let Ok(detail_root) = self.request_json(
            creds,
            "/reading/bookapi/multi-detail/v1/",
            &[("book_id", &missing.join(",")), ("book_type", "2")],
            None,
        ) {
            if let Some(arr) = detail_root.get("data").and_then(Value::as_array) {
                let meta_map = parse_multi_detail_map(arr);
                for row in &mut rows {
                    if let Some(raw_id) = row.series.id.strip_prefix("hongguo:") {
                        if let Some(meta) = meta_map.get(raw_id) {
                            if meta.episode_count > 0 {
                                row.series.episode_count = meta.episode_count;
                                row.series.badge = format!("全 {} 集", meta.episode_count);
                            }
                            if row.series.cover_url.is_none() {
                                row.series.cover_url = meta.cover_url.clone();
                            }
                            if row.series.description.is_empty() {
                                row.series.description = meta.description.clone();
                            }
                        }
                    }
                }
            }
        }
        Ok(rows)
    }
}

// ---------- Parsers & Helpers ----------

fn is_numeric_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 30 && value.bytes().all(|b| b.is_ascii_digit())
}

fn sanitize_text(raw: &str, max_chars: usize) -> String {
    raw.chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .collect::<String>()
        .trim()
        .chars()
        .take(max_chars)
        .collect()
}

fn val_str(val: &Value, key: &str) -> String {
    match val.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn obj_str(map: &serde_json::Map<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn val_u64(val: &Value, key: &str) -> u64 {
    match val.get(key) {
        Some(Value::Number(n)) => n
            .as_u64()
            .or_else(|| n.as_i64().map(|v| v.max(0) as u64))
            .unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<u64>().unwrap_or(0),
        _ => 0,
    }
}

fn normalize_timestamp_ms(raw: u64) -> u64 {
    if raw > 0 && raw <= 9_999_999_999 {
        raw.saturating_mul(1000)
    } else {
        raw
    }
}

fn extract_episode_count(val: &Value) -> usize {
    for key in ["episode_cnt", "episode_count", "drama_chapter_number"] {
        let count = val_u64(val, key) as usize;
        if count > 0 {
            return count;
        }
    }
    0
}

/// Convert signed/HEIC ByteDance `novel-pic` cover URLs (which WebView2 on Windows cannot
/// decode as `.heic`, and whose `x-signature` breaks if only the extension is altered)
/// into the public, WebView2-compatible JPEG URL on `p3-novel.byteimg.com`.
pub fn normalize_cover_url(raw: &str) -> Option<String> {
    let mut cleaned = clean_url(raw);
    if let Some(rest) = cleaned.strip_prefix("http://") {
        cleaned = format!("https://{rest}");
    }
    if cleaned.is_empty()
        || cleaned.len() > 2048
        || !cleaned.starts_with("https://")
        || cleaned.contains('#')
        || cleaned.contains('\\')
        || cleaned.bytes().any(|b| b <= 0x20 || b == 0x7f)
    {
        return None;
    }
    let after_scheme = &cleaned["https://".len()..];
    let (host, path_and_query) = match after_scheme.find('/') {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx..]),
        None => return Some(cleaned),
    };
    let host_lower = host.to_ascii_lowercase();
    let is_bytedance_pic = host_lower.ends_with(".fqnovelpic.com")
        || host_lower == "fqnovelpic.com"
        || host_lower.ends_with(".byteimg.com")
        || host_lower == "byteimg.com";
    let path_only = path_and_query.split('?').next().unwrap_or(path_and_query);
    if is_bytedance_pic
        && (host_lower.contains("sign") || path_only.to_ascii_lowercase().ends_with(".heic"))
    {
        if let Some(idx) = path_only.find("/novel-pic/") {
            let after = &path_only[idx + "/novel-pic/".len()..];
            let key = after.split(['~', '/', '.']).next().unwrap_or("").trim();
            if key.len() >= 8
                && key.len() <= 128
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Some(format!(
                    "https://p3-novel.byteimg.com/novel-pic/{key}~tplv-shrink:640:0.image"
                ));
            }
        }
    }
    Some(cleaned)
}

fn parse_multi_detail_map(rows: &[Value]) -> HashMap<String, Series> {
    let mut map = HashMap::new();
    for row in rows {
        let raw_id = val_str(row, "book_id");
        let title = sanitize_text(&val_str(row, "book_name"), 100);
        if !is_numeric_id(&raw_id) || title.is_empty() {
            continue;
        }
        let episode_count = extract_episode_count(row);
        let badge = if episode_count > 0 {
            format!("全 {episode_count} 集")
        } else {
            "红果短剧".into()
        };
        let description = sanitize_text(&val_str(row, "abstract"), 280);
        let cover_url = normalize_cover_url(&val_str(row, "thumb_url"));
        map.insert(
            raw_id.clone(),
            Series {
                id: format!("hongguo:{raw_id}"),
                title,
                description,
                genre: "红果短剧".into(),
                badge,
                artwork: "studio".into(),
                cover_url,
                episode_count,
                provider_id: "hongguo".into(),
            },
        );
    }
    map
}

pub fn parse_favorites(root: &Value) -> Result<Vec<CloudFavorite>, String> {
    let data = root
        .get("data")
        .and_then(Value::as_object)
        .ok_or("account_favorites: 账户收藏数据格式已变化")?;
    let Some(shelf_val) = data.get("video_shelf_info") else {
        return Err("account_favorites: 账户收藏列表格式已变化".into());
    };
    if shelf_val.is_null() {
        return Ok(Vec::new());
    }
    let rows = shelf_val
        .as_array()
        .ok_or("account_favorites: 账户收藏列表格式已变化")?;

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let video = row
            .get("video_data")
            .filter(|v| v.is_object())
            .unwrap_or(row);
        let raw_id = [
            val_str(video, "series_id"),
            val_str(row, "series_id"),
            val_str(row, "book_id"),
        ]
        .into_iter()
        .find(|s| is_numeric_id(s));
        let Some(raw_id) = raw_id else { continue };
        let title = [
            val_str(row, "series_title"),
            val_str(video, "title"),
            val_str(row, "book_name"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(|s| sanitize_text(&s, 100))
        .unwrap_or_default();
        if title.is_empty() || !seen.insert(raw_id.clone()) {
            continue;
        }

        let cover_raw = [
            val_str(row, "series_cover_url"),
            val_str(row, "series_cover"),
            val_str(video, "series_cover_url"),
            val_str(video, "series_cover"),
            val_str(video, "cover"),
            val_str(video, "cover_url"),
            val_str(video, "thumb_url"),
            val_str(row, "thumb_url"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default();
        let cover_url = normalize_cover_url(&cover_raw);

        let episode_count = extract_episode_count(video).max(extract_episode_count(row));
        let badge = if episode_count > 0 {
            format!("全 {episode_count} 集")
        } else {
            "云端收藏".into()
        };
        let description = [
            val_str(row, "series_intro"),
            val_str(video, "video_desc"),
            val_str(row, "abstract"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(|s| sanitize_text(&s, 280))
        .unwrap_or_default();

        let genre = parse_category_tags(&val_str(row, "category_schema"))
            .or_else(|| {
                let c = sanitize_text(&val_str(video, "category"), 40);
                (!c.is_empty()).then_some(c)
            })
            .unwrap_or_else(|| "红果短剧".into());

        let added_at_ms = normalize_timestamp_ms(val_u64(row, "collect_time"));
        out.push(CloudFavorite {
            series: Series {
                id: format!("hongguo:{raw_id}"),
                title,
                description,
                genre,
                badge,
                artwork: "studio".into(),
                cover_url,
                episode_count,
                provider_id: "hongguo".into(),
            },
            added_at_ms,
        });
    }

    if !rows.is_empty() && out.is_empty() {
        return Err("account_favorites: 账户收藏条目格式已变化".into());
    }
    out.sort_by(|a, b| b.added_at_ms.cmp(&a.added_at_ms));
    Ok(out)
}

fn parse_category_tags(schema_json: &str) -> Option<String> {
    if schema_json.is_empty() {
        return None;
    }
    let arr = serde_json::from_str::<Value>(schema_json).ok()?;
    let items = arr.as_array()?;
    let names: Vec<String> = items
        .iter()
        .map(|item| sanitize_text(&val_str(item, "name"), 20))
        .filter(|s| !s.is_empty())
        .take(3)
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names.join(" · "))
    }
}

pub fn parse_history_rows(rows: &[Value]) -> Vec<CloudHistoryEntry> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let raw_id = {
            let s = val_str(row, "series_id");
            if !s.is_empty() {
                s
            } else {
                val_str(row, "book_id")
            }
        };
        let episode_id = val_str(row, "vid");
        if !is_numeric_id(&raw_id) || !is_numeric_id(&episode_id) || !seen.insert(raw_id.clone()) {
            continue;
        }
        let episode_number = (val_u64(row, "vid_index") as usize).max(1);
        let pos_ms = val_u64(row, "current_play_time");
        let dur_ms = val_u64(row, "current_total_time");
        let updated_at_ms = normalize_timestamp_ms(val_u64(row, "last_watch_time"));
        let episode_count = val_u64(row, "episode_count") as usize;
        let fallback_title = format!("红果短剧 {raw_id}");
        let badge = if episode_count > 0 {
            format!("全 {episode_count} 集")
        } else {
            "云端记录".into()
        };
        out.push(CloudHistoryEntry {
            series: Series {
                id: format!("hongguo:{raw_id}"),
                title: fallback_title.clone(),
                description: String::new(),
                genre: "红果短剧".into(),
                badge,
                artwork: "studio".into(),
                cover_url: None,
                episode_count,
                provider_id: "hongguo".into(),
            },
            episode_id,
            episode_number,
            episode_title: format!("{fallback_title} · 第 {episode_number} 集"),
            position_sec: pos_ms as f64 / 1000.0,
            duration_sec: (dur_ms > 0).then_some(dur_ms as f64 / 1000.0),
            updated_at_ms,
        });
    }
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    out
}

pub fn parse_reservations(root: &Value, online: bool) -> Result<Vec<CloudReservation>, String> {
    let data = root
        .get("data")
        .and_then(Value::as_object)
        .ok_or("account_reservations: 预约数据格式已变化，请稍后重试")?;
    let Some(items_val) = data.get("subscribe_items") else {
        return Err("account_reservations: 预约列表格式已变化，请稍后重试".into());
    };
    if items_val.is_null() {
        return Ok(Vec::new());
    }
    let rows = items_val
        .as_array()
        .ok_or("account_reservations: 预约列表格式已变化，请稍后重试")?;

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        if let Some(flag) = row.get("is_online").and_then(Value::as_bool) {
            if flag != online {
                continue;
            }
        }
        let nested = row
            .get("subscribe_data")
            .filter(|v| v.is_object())
            .or_else(|| row.get("video_data").filter(|v| v.is_object()));
        let video = nested.unwrap_or(row);
        let raw_series_id = [
            val_str(video, "series_id"),
            val_str(row, "series_id"),
            val_str(row, "book_id"),
        ]
        .into_iter()
        .find(|s| is_numeric_id(s));

        let scoped_id = if let Some(sid) = raw_series_id {
            if online {
                format!("hongguo:{sid}")
            } else {
                format!("reservation:{sid}")
            }
        } else if !online && nested.is_none() {
            let item_id = val_str(row, "item_id");
            if is_numeric_id(&item_id) {
                format!("reservation:{item_id}")
            } else {
                continue;
            }
        } else {
            continue;
        };

        if !seen.insert(scoped_id.clone()) {
            continue;
        }

        let title = [
            val_str(row, "series_title"),
            val_str(video, "title"),
            val_str(video, "book_name"),
            val_str(row, "name"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(|s| sanitize_text(&s, 100))
        .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let cover_raw = [
            val_str(row, "series_cover_url"),
            val_str(row, "series_cover"),
            val_str(video, "series_cover_url"),
            val_str(video, "series_cover"),
            val_str(video, "cover"),
            val_str(video, "cover_url"),
            val_str(video, "thumb_url"),
            val_str(row, "thumb_url"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default();
        let cover_url = normalize_cover_url(&cover_raw);

        let episode_count = if online {
            extract_episode_count(video).max(extract_episode_count(row))
        } else {
            0
        };
        let badge = if online {
            if episode_count > 0 {
                format!("全 {episode_count} 集")
            } else {
                "已上线".into()
            }
        } else {
            "待上线".into()
        };
        let description = [
            val_str(video, "video_desc"),
            val_str(row, "abstract"),
            val_str(row, "item_desc"),
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(|s| sanitize_text(&s, 280))
        .unwrap_or_default();
        let genre = {
            let c = sanitize_text(&val_str(video, "category"), 40);
            if c.is_empty() {
                "预约短剧".into()
            } else {
                c
            }
        };
        let publish_at_ms = normalize_timestamp_ms(val_u64(row, "schedule_publish_time"));
        let publish_text = sanitize_text(&val_str(row, "expected_rec_text"), 60);

        out.push(CloudReservation {
            series: Series {
                id: scoped_id,
                title,
                description,
                genre,
                badge,
                artwork: "studio".into(),
                cover_url,
                episode_count,
                provider_id: "hongguo".into(),
            },
            online,
            publish_at_ms,
            publish_text,
        });
    }

    if !rows.is_empty() && out.is_empty() {
        return Err("account_reservations: 预约条目格式已变化，请稍后重试".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cloud_favorites_and_converts_heic_covers() {
        let raw = json!({
            "code": 0,
            "data": {
                "video_shelf_info": [
                    {
                        "series_id": "752291100123",
                        "series_title": "重生之红果风云",
                        "series_cover_url": "https://p3-reading-sign.fqnovelpic.com/novel-pic/a316a85007de06165e04feb098bfad41~tplv-noop.heic?lk3s=45961287&x-expires=1800687117&x-signature=FN3ivFqzYoXkXMOAHgp2vbjfZio%3D",
                        "series_intro": "一段逆袭故事",
                        "collect_time": 1730000000,
                        "category_schema": "[{\"name\":\"都市\"},{\"name\":\"逆袭\"}]",
                        "video_data": {
                            "episode_cnt": 80
                        }
                    }
                ]
            }
        });
        let favs = parse_favorites(&raw).unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].series.id, "hongguo:752291100123");
        assert_eq!(favs[0].series.title, "重生之红果风云");
        assert_eq!(favs[0].series.episode_count, 80);
        assert_eq!(favs[0].series.genre, "都市 · 逆袭");
        assert_eq!(favs[0].added_at_ms, 1_730_000_000_000);
        assert_eq!(
            favs[0].series.cover_url.as_deref(),
            Some("https://p3-novel.byteimg.com/novel-pic/a316a85007de06165e04feb098bfad41~tplv-shrink:640:0.image")
        );
    }

    #[test]
    fn parses_cloud_history_and_reservations() {
        let hist_rows = vec![json!({
            "series_id": "752291100123",
            "vid": "752291100999",
            "vid_index": 5,
            "current_play_time": 45000,
            "current_total_time": 90000,
            "last_watch_time": 1730000500,
            "episode_count": 60
        })];
        let history = parse_history_rows(&hist_rows);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].series.id, "hongguo:752291100123");
        assert_eq!(history[0].episode_id, "752291100999");
        assert_eq!(history[0].episode_number, 5);
        assert!((history[0].position_sec - 45.0).abs() < 0.01);
        assert_eq!(history[0].duration_sec, Some(90.0));

        let res_upcoming = json!({
            "code": 0,
            "data": {
                "subscribe_items": [
                    {
                        "item_id": "99887766",
                        "name": "即将上线大剧",
                        "is_online": false,
                        "expected_rec_text": "预计明日 10:00 上线"
                    }
                ]
            }
        });
        let upcoming = parse_reservations(&res_upcoming, false).unwrap();
        assert_eq!(upcoming.len(), 1);
        assert_eq!(upcoming[0].series.id, "reservation:99887766");
        assert!(!upcoming[0].online);
        assert_eq!(upcoming[0].publish_text, "预计明日 10:00 上线");
    }
}
