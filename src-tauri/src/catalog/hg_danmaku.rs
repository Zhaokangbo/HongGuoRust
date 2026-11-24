//! Official Hongguo timed comments (danmaku) signer and paginated fetcher.
//! Adapted from N3urda/hongguoTV (`CommentSignature.kt`, `TimedComments.kt`) and
//! jackotom/hongguo-drama (`danmaku.js`, `danmaku-sign.js`), GPL-3.0-only.
//! Anonymous read-only endpoint — never sends account cookies or credentials.

use super::hg_crypto::{
    aes128_cbc_encrypt_pkcs7, base64_encode, fill_random_bytes, hex_decode, hex_encode, md5, sm3,
};
use super::hg_signer::now_millis;
use super::http_client::{request_with_options, RequestOptions};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const COMMENT_HOST: &str = "https://api5-normal-sinfonlineb.fqnovel.com";
const COMMENT_UA: &str = "com.phoenix.read/73532 (Linux; U; Android 16; zh_CN; 25053RT47C; Build/BP2A.250605.031.A3; Cronet/TTNetVersion:04657795 2026-01-23 QuicVersion:c67e9834 2025-09-08)";
const ARGUS_SECRET_HEX: &str = "ac1adaae95a7af94a5114ab3b3a97dd80050aa0a39314c40528caec95256c28c";

const PAGE_COUNT: usize = 90;
const MAX_PAGES: usize = 15;
const MAX_ITEMS: usize = 1000;
const MAX_EMPTY_PAGES: usize = 2;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_DURATION_MS: u64 = 3_600_000;
const MAX_DURATION_MS: u64 = 86_400_000;
const CACHE_TTL: Duration = Duration::from_secs(300);
const MAX_CACHED_EPISODES: usize = 64;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DanmakuItem {
    pub id: String,
    pub text: String,
    pub time_sec: f64,
    pub color: String,
}

fn new_comment_device_id() -> String {
    let mut buf = [0u8; 8];
    fill_random_bytes(&mut buf);
    let raw = u64::from_be_bytes(buf);
    let val = 1_000_000_000_000_000_000u64 + (raw % 8_000_000_000_000_000_000u64);
    val.to_string()
}

/// Python `urllib.parse.quote_plus` encoding (keeps only A-Za-z0-9_.-~, space -> '+').
fn quote_plus(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else if byte == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn build_comment_query(stamp_ms: u64, device_id: &str, install_id: &str) -> String {
    let ticket = stamp_ms.to_string();
    let mut pairs: [(&str, &str); 21] = [
        ("_rticket", &ticket),
        ("ac", "wifi"),
        ("aid", "8662"),
        ("app_name", "novelread"),
        ("channel", "update_64"),
        ("device_brand", "Redmi"),
        ("device_id", device_id),
        ("device_platform", "android"),
        ("device_type", "25053RT47C"),
        ("dpi", "520"),
        ("iid", install_id),
        ("language", "zh"),
        ("manifest_version_code", "73532"),
        ("os", "android"),
        ("os_api", "36"),
        ("os_version", "16"),
        ("resolution", "1280*2772"),
        ("ssmix", "a"),
        ("update_version_code", "73532"),
        ("version_code", "73532"),
        ("version_name", "7.3.5.32"),
    ];
    pairs.sort_by_key(|(k, _)| *k);
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", quote_plus(k), quote_plus(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn push_uvarint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            out.push(byte | 0x80);
        } else {
            out.push(byte);
            break;
        }
    }
}

enum CProto<'a> {
    Uint(u64),
    Bytes(&'a [u8]),
    Str(&'a str),
}

fn encode_comment_proto(fields: &[(u32, CProto<'_>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (tag, val) in fields {
        match val {
            CProto::Uint(n) => {
                push_uvarint(&mut out, u64::from(*tag) << 3);
                push_uvarint(&mut out, *n);
            }
            CProto::Bytes(b) => {
                push_uvarint(&mut out, (u64::from(*tag) << 3) | 2);
                push_uvarint(&mut out, b.len() as u64);
                out.extend_from_slice(b);
            }
            CProto::Str(s) => {
                let b = s.as_bytes();
                push_uvarint(&mut out, (u64::from(*tag) << 3) | 2);
                push_uvarint(&mut out, b.len() as u64);
                out.extend_from_slice(b);
            }
        }
    }
    out
}

fn pkcs7_pad(data: &[u8]) -> Vec<u8> {
    let pad = 16 - (data.len() % 16);
    let mut out = Vec::with_capacity(data.len() + pad);
    out.extend_from_slice(data);
    out.resize(data.len() + pad, pad as u8);
    out
}

pub fn comment_x_gorgon(raw_query: &str, stamp: u32, nonce: &[u8; 11]) -> String {
    let first = ((u16::from(nonce[0]) % 7) + 1) << 5;
    let second = 0xe0u8 | (nonce[1] & 0x0f);

    let mut input = [0u8; 20];
    input[0..4].copy_from_slice(&md5(raw_query.as_bytes())[0..4]);
    input[12..16].copy_from_slice(&[0x00, 0x01, 0x07, 0x04]);
    input[16..20].copy_from_slice(&stamp.to_be_bytes());

    let key = [0x4au8, 0x00, 0x16, second, 0x47, 0x6c, 0x00, first as u8];
    let mut s = [0u8; 256];
    for (i, slot) in s.iter_mut().enumerate() {
        *slot = i as u8;
    }
    let mut j: usize = 0;
    for i in 0..256 {
        j = (j + usize::from(s[i]) + usize::from(key[i & 7])) & 255;
        // Upstream protocol intentionally assigns (not swaps) s[i] = s[j].
        s[i] = s[j];
    }
    j = 0;
    for n in 0..20 {
        let i = n + 1;
        j = (j + usize::from(s[i])) & 255;
        let y = s[j];
        s[i] = y;
        let idx = (usize::from(y) + usize::from(y)) & 255;
        input[n] ^= s[idx];
    }
    for i in 0..20 {
        let swapped = input[i].rotate_left(4);
        let next = input[(i + 1) % 20];
        input[i] = (swapped ^ next).reverse_bits() ^ 0xeb;
    }

    let mut out = Vec::with_capacity(26);
    out.extend_from_slice(&[0x84, 0x04, first as u8, second, 0x00, 0x00]);
    out.extend_from_slice(&input);
    hex_encode(&out)
}

pub fn comment_ladon(stamp: u32, salt: &[u8; 4]) -> String {
    let mut seed = Vec::with_capacity(8);
    seed.extend_from_slice(salt);
    seed.extend_from_slice(b"8662");
    let ascii = hex_encode(&md5(&seed)).into_bytes();

    let mut queue = std::collections::VecDeque::with_capacity(36);
    queue.push_back(u64::from_le_bytes(ascii[16..24].try_into().unwrap()));
    queue.push_back(u64::from_le_bytes(ascii[24..32].try_into().unwrap()));
    let mut left = u64::from_le_bytes(ascii[0..8].try_into().unwrap());
    let mut right = u64::from_le_bytes(ascii[8..16].try_into().unwrap());

    let mut keys = [0u64; 34];
    for (i, slot) in keys.iter_mut().enumerate() {
        *slot = left;
        let next = right.rotate_right(8).wrapping_add(left) ^ (i as u64);
        queue.push_back(next);
        left = next ^ left.rotate_left(3);
        right = queue.pop_front().unwrap();
    }

    let text = format!("{stamp}-1611921764-3019");
    let data = pkcs7_pad(text.as_bytes());
    let mut out = Vec::with_capacity(4 + data.len());
    out.extend_from_slice(salt);
    for chunk in data.chunks_exact(16) {
        let mut a = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
        let mut b = u64::from_le_bytes(chunk[8..16].try_into().unwrap());
        for &k in &keys {
            b = k ^ a.wrapping_add(b.rotate_right(8));
            a = b ^ a.rotate_left(3);
        }
        out.extend_from_slice(&a.to_le_bytes());
        out.extend_from_slice(&b.to_le_bytes());
    }
    base64_encode(&out)
}

pub fn comment_argus(raw_query: &str, stamp: u32, nonce: &[u8; 11]) -> Result<String, String> {
    let random = u32::from_le_bytes(nonce[6..10].try_into().unwrap());
    let nested = encode_comment_proto(&[
        (1, CProto::Uint(2 + u64::from(nonce[10] % 49) * 2)),
        (2, CProto::Uint(1_388_734)),
        (3, CProto::Uint(1_388_734)),
        (4, CProto::Uint(1_388_734)),
    ]);
    let zero_flags = [0u8; 8];
    let body_sm3 = sm3(&[0u8; 16]);
    let query_sm3 = sm3(raw_query.as_bytes());
    let message = encode_comment_proto(&[
        (1, CProto::Uint(0x4040_1252)),
        (2, CProto::Uint(2)),
        (3, CProto::Uint(u64::from(random))),
        (4, CProto::Str("3019")),
        (6, CProto::Str("1611921764")),
        (7, CProto::Str("6.8.1.32")),
        (8, CProto::Str("v04.07.01-ml-android")),
        (9, CProto::Uint(135_135_744)),
        (10, CProto::Bytes(&zero_flags)),
        (12, CProto::Uint(u64::from(stamp) * 2)),
        (13, CProto::Bytes(&body_sm3[..6])),
        (14, CProto::Bytes(&query_sm3[..6])),
        (15, CProto::Bytes(&nested)),
        (20, CProto::Str("none")),
        (21, CProto::Uint(738)),
    ]);

    let secret = hex_decode(ARGUS_SECRET_HEX)?;
    let mut sm3_in = Vec::with_capacity(secret.len() * 2 + 4);
    sm3_in.extend_from_slice(&secret);
    sm3_in.extend_from_slice(&[0x3c, 0xcc, 0x56, 0x7b]);
    sm3_in.extend_from_slice(&secret);
    let derived = sm3(&sm3_in);

    let mut keys = [0u64; 72];
    for i in 0..4 {
        keys[i] = u64::from_le_bytes(derived[i * 8..i * 8 + 8].try_into().unwrap());
    }
    const SIMON_Z: u64 = 0x3dc9_4c3a_046d_678b;
    for i in 4..72 {
        let x = keys[i - 1].rotate_right(3) ^ keys[i - 3];
        let z_bit = (SIMON_Z >> ((i - 4) % 62)) & 1;
        keys[i] = (!keys[i - 4]) ^ x ^ x.rotate_right(1) ^ z_bit ^ 3;
    }

    let blocks = pkcs7_pad(&message);
    let mut encrypted = Vec::with_capacity(blocks.len());
    for chunk in blocks.chunks_exact(16) {
        let mut a = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
        let mut b = u64::from_le_bytes(chunk[8..16].try_into().unwrap());
        for &k in &keys {
            let next = a ^ (b.rotate_left(1) & b.rotate_left(8)) ^ b.rotate_left(2) ^ k;
            a = b;
            b = next;
        }
        encrypted.extend_from_slice(&a.to_le_bytes());
        encrypted.extend_from_slice(&b.to_le_bytes());
    }

    let mask: [u8; 8] = [0xd0, 0x4f, 0xfd, 0xff, 0xd0, 0x4f, 0xfd, 0xff];
    let mut rev_payload = Vec::with_capacity(8 + encrypted.len());
    rev_payload.extend_from_slice(&mask);
    for (i, byte) in encrypted.iter().enumerate() {
        rev_payload.push(byte ^ mask[i & 7]);
    }
    rev_payload.reverse();

    let mut container = Vec::with_capacity(9 + rev_payload.len() + 2);
    container.extend_from_slice(&[0xa6, 0xe7, 0x83, 0xee, 0x70, 0x01, 0x10, 0x09, 0x18]);
    container.extend_from_slice(&rev_payload);
    container.extend_from_slice(&[0x56, 0x7b]);

    let aes_key = md5(&secret[0..16]);
    let aes_iv = md5(&secret[16..32]);
    let cbc = aes128_cbc_encrypt_pkcs7(&aes_key, &aes_iv, &container);

    let mut final_bytes = Vec::with_capacity(2 + cbc.len());
    final_bytes.extend_from_slice(&[0x3c, 0xcc]);
    final_bytes.extend_from_slice(&cbc);
    Ok(base64_encode(&final_bytes))
}

pub fn sign_comment_headers(
    raw_query: &str,
    stamp_ms: u64,
    nonce: &[u8; 11],
) -> Result<Vec<(String, String)>, String> {
    let stamp = (stamp_ms / 1000) as u32;
    let salt: [u8; 4] = nonce[2..6].try_into().unwrap();
    Ok(vec![
        ("X-Gorgon".into(), comment_x_gorgon(raw_query, stamp, nonce)),
        ("X-Khronos".into(), stamp.to_string()),
        ("X-SS-Req-Ticket".into(), stamp_ms.to_string()),
        ("X-SS-STUB".into(), String::new()),
        ("X-Argus".into(), comment_argus(raw_query, stamp, nonce)?),
        ("X-Ladon".into(), comment_ladon(stamp, &salt)),
    ])
}

fn clean_danmaku_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(180));
    let mut last_space = true;
    for ch in raw.chars() {
        if ch.is_control() || ch == '\u{2028}' || ch == '\u{2029}' {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
            continue;
        }
        if out.chars().count() >= 180 {
            break;
        }
        last_space = ch.is_whitespace();
        out.push(ch);
    }
    out.trim().to_owned()
}

fn normalize_color(val: Option<&Value>) -> String {
    let Some(v) = val else {
        return "#FFFFFF".into();
    };
    if let Some(n) = v.as_i64().filter(|&n| n >= 0) {
        return format!("#{:06X}", (n as u32) & 0x00ff_ffff);
    }
    if let Some(s) = v.as_str() {
        let t = s.trim();
        if t.len() == 7 && t.starts_with('#') && t[1..].bytes().all(|b| b.is_ascii_hexdigit()) {
            return t.to_ascii_uppercase();
        }
        if let Ok(n) = t.parse::<u32>() {
            return format!("#{:06X}", n & 0x00ff_ffff);
        }
    }
    "#FFFFFF".into()
}

fn parse_cursor_next_ms(cursor_val: Option<&Value>) -> Option<u64> {
    let raw = cursor_val?.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(obj) = serde_json::from_str::<Value>(raw) {
        for key in ["end_offset_time", "next_offset_time", "offset_time"] {
            if let Some(n) = obj.get(key).and_then(Value::as_u64) {
                return Some(n);
            }
        }
    }
    None
}

struct CachedDanmaku {
    items: Vec<DanmakuItem>,
    saved_at: Instant,
}

pub struct DanmakuFetcher {
    device_id: String,
    install_id: String,
    cache: Mutex<HashMap<String, CachedDanmaku>>,
}

impl Default for DanmakuFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DanmakuFetcher {
    pub fn new() -> Self {
        Self {
            device_id: new_comment_device_id(),
            install_id: new_comment_device_id(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn fetch_episode(
        &self,
        series_id: &str,
        episode_id: &str,
        duration_ms: Option<u64>,
    ) -> Result<Vec<DanmakuItem>, String> {
        let valid_id =
            |s: &str| s.len() >= 6 && s.len() <= 30 && s.bytes().all(|b| b.is_ascii_digit());
        if !valid_id(series_id) || !valid_id(episode_id) {
            return Err("hg_danmaku: 无效的剧集或分集编号".into());
        }
        let cache_key = format!("{series_id}:{episode_id}");
        if let Ok(guard) = self.cache.lock() {
            if let Some(cached) = guard.get(&cache_key) {
                if cached.saved_at.elapsed() < CACHE_TTL {
                    return Ok(cached.items.clone());
                }
            }
        }

        let effective_duration = duration_ms
            .filter(|&d| d > 0)
            .unwrap_or(DEFAULT_DURATION_MS)
            .min(MAX_DURATION_MS);

        let mut items: Vec<DanmakuItem> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut start_ms: u64 = 0;
        let mut cursor = String::new();
        let mut empty_streak = 0usize;

        for page in 0..MAX_PAGES {
            if items.len() >= MAX_ITEMS || start_ms >= effective_duration {
                break;
            }
            if page > 0 {
                std::thread::sleep(Duration::from_millis(80));
            }
            let (page_items, next_ms, next_cursor, has_more) =
                match self.fetch_page(series_id, episode_id, start_ms, effective_duration, &cursor)
                {
                    Ok(res) => res,
                    Err(err) => {
                        if !items.is_empty() {
                            break;
                        }
                        return Err(err);
                    }
                };

            let mut added = 0usize;
            for item in page_items {
                if seen.insert(item.id.clone()) {
                    items.push(item);
                    added += 1;
                    if items.len() >= MAX_ITEMS {
                        break;
                    }
                }
            }
            if added == 0 {
                empty_streak += 1;
                if empty_streak >= MAX_EMPTY_PAGES {
                    break;
                }
            } else {
                empty_streak = 0;
            }

            if has_more == Some(false) {
                break;
            }
            if let Some(next_time) = next_ms {
                if next_time > start_ms && next_time < effective_duration {
                    start_ms = next_time;
                    cursor.clear();
                    continue;
                }
            }
            if let Some(c) = next_cursor.filter(|c| !c.is_empty() && c != &cursor) {
                cursor = c;
                continue;
            }
            break;
        }

        items.sort_by(|a, b| {
            a.time_sec
                .partial_cmp(&b.time_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Ok(mut guard) = self.cache.lock() {
            if guard.len() >= MAX_CACHED_EPISODES {
                guard.retain(|_, v| v.saved_at.elapsed() < CACHE_TTL);
                if guard.len() >= MAX_CACHED_EPISODES {
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
                cache_key,
                CachedDanmaku {
                    items: items.clone(),
                    saved_at: Instant::now(),
                },
            );
        }

        Ok(items)
    }

    fn fetch_page(
        &self,
        series_id: &str,
        episode_id: &str,
        start_ms: u64,
        duration_ms: u64,
        cursor: &str,
    ) -> Result<(Vec<DanmakuItem>, Option<u64>, Option<String>, Option<bool>), String> {
        let stamp_ms = now_millis();
        let query = build_comment_query(stamp_ms, &self.device_id, &self.install_id);
        let mut nonce = [0u8; 11];
        fill_random_bytes(&mut nonce);
        let sig_headers = sign_comment_headers(&query, stamp_ms, &nonce)?;

        let payload = serde_json::json!({
            "comment_source": 601,
            "server_channel": 1000,
            "group_id": episode_id,
            "group_type": 30,
            "comment_type": 20,
            "sort": 1,
            "count": PAGE_COUNT,
            "cursor": cursor,
            "aid": 8662,
            "compliance_status": 0,
            "business_param": {
                "book_id": series_id,
                "start_offset_time": start_ms,
                "playlet_item_duration": duration_ms,
                "need_danmaku_guide_type": [1, 3, 4, 2]
            }
        });
        let body = payload.to_string().into_bytes();
        let url = format!("{COMMENT_HOST}/novel/commentapi/comment/list/{episode_id}/v1/?{query}");

        let mut headers: Vec<(&str, &str)> = vec![
            ("User-Agent", COMMENT_UA),
            ("Accept", "application/json"),
            ("Content-Type", "application/json; charset=utf-8"),
            ("X-XS-From-Web", "0"),
            ("Sdk-Version", "2"),
            ("Comment-Source", "601"),
            ("Server-Channel", "1000"),
        ];
        for (k, v) in &sig_headers {
            headers.push((k.as_str(), v.as_str()));
        }

        let resp = request_with_options(
            "POST",
            &url,
            &headers,
            Some(&body),
            MAX_RESPONSE_BYTES,
            RequestOptions {
                user_agent: Some(COMMENT_UA),
                timeout_ms: 8_000,
                decompress: true,
                disable_cookies: true,
            },
        )?;
        if !(200..300).contains(&resp.status) {
            return Err(format!("hg_danmaku: 弹幕服务返回 HTTP {}", resp.status));
        }
        let root: Value = serde_json::from_slice(&resp.body)
            .map_err(|_| "hg_danmaku: 弹幕响应格式异常".to_string())?;
        if root.get("code").and_then(Value::as_i64) != Some(0) {
            return Err("hg_danmaku: 弹幕接口暂不可用".into());
        }
        let Some(data) = root.get("data").filter(|v| v.is_object()) else {
            return Ok((Vec::new(), None, None, Some(false)));
        };

        let extra_next = data
            .get("extra")
            .and_then(|e| e.get("next_query_danmaku_list_time"))
            .and_then(Value::as_u64);
        let list_info = data.get("common_list_info");
        let has_more = list_info
            .and_then(|i| i.get("has_more"))
            .and_then(Value::as_bool);
        let next_cursor = list_info
            .and_then(|i| i.get("cursor"))
            .and_then(Value::as_str)
            .map(|s| s.to_owned());
        let cursor_next = parse_cursor_next_ms(list_info.and_then(|i| i.get("cursor")));
        let next_ms = extra_next.or(cursor_next);

        let mut items = Vec::new();
        if let Some(rows) = data.get("data_list").and_then(Value::as_array) {
            for row in rows.iter().take(300) {
                let Some(comment) = row.get("comment").filter(|v| v.is_object()) else {
                    continue;
                };
                let Some(common) = comment.get("common").filter(|v| v.is_object()) else {
                    continue;
                };
                let Some(expand) = comment.get("expand").filter(|v| v.is_object()) else {
                    continue;
                };
                if common.get("group_id").and_then(Value::as_str) != Some(episode_id) {
                    continue;
                }
                if common.get("status").and_then(Value::as_i64) != Some(1) {
                    continue;
                }
                let Some(offset_ms) = expand.get("offset_time").and_then(Value::as_u64) else {
                    continue;
                };
                if offset_ms > MAX_DURATION_MS {
                    continue;
                }
                let id = comment
                    .get("comment_id")
                    .and_then(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .or_else(|| v.as_u64().map(|n| n.to_string()))
                    })
                    .unwrap_or_default();
                let raw_text = common
                    .get("content")
                    .and_then(|c| c.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let text = clean_danmaku_text(raw_text);
                if id.is_empty() || text.is_empty() {
                    continue;
                }
                let color = normalize_color(
                    expand
                        .get("color")
                        .or_else(|| common.get("content").and_then(|c| c.get("color"))),
                );
                items.push(DanmakuItem {
                    id,
                    text,
                    time_sec: (offset_ms as f64) / 1000.0,
                    color,
                });
            }
        }

        Ok((items, next_ms, next_cursor, has_more))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_signature_matches_reference_golden_vector_byte_for_byte() {
        let query = build_comment_query(
            1_800_000_000_000,
            "1234567890123456789",
            "2234567890123456789",
        );
        assert_eq!(
            query,
            "_rticket=1800000000000&ac=wifi&aid=8662&app_name=novelread&channel=update_64&device_brand=Redmi&device_id=1234567890123456789&device_platform=android&device_type=25053RT47C&dpi=520&iid=2234567890123456789&language=zh&manifest_version_code=73532&os=android&os_api=36&os_version=16&resolution=1280%2A2772&ssmix=a&update_version_code=73532&version_code=73532&version_name=7.3.5.32"
        );
        let nonce: [u8; 11] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        let headers = sign_comment_headers(&query, 1_800_000_000_000, &nonce).unwrap();
        let get = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
                .unwrap()
        };
        assert_eq!(
            get("X-Gorgon"),
            "840440e200002f99a710ff447dece3898d8546b77f144b8256aa"
        );
        assert_eq!(get("X-Khronos"), "1800000000");
        assert_eq!(get("X-SS-Req-Ticket"), "1800000000000");
        assert_eq!(get("X-SS-STUB"), "");
        assert_eq!(
            get("X-Ladon"),
            "AwQFBg5eKAmsDiJ0N5kGlDr9uRVnfrwAzFAlv0Cqi9J16ndL"
        );
        assert_eq!(
            get("X-Argus"),
            "PMxPj+JImxTKEfQuAsVmWpqWQ4BCXEhjteOlvQDjp/6AjKGsWAKooon1gSc6pbc1mDwPN+9frV6F49pezP4qLJlqDgjZO4gVv7oV3o+dNR38QVxlcOThRhFfBGKzG6Fl7BWfcvxr3NMBrAAqTbNQXvst0ELn7djXkv9TuhmX97PM6dl/k92SJ7MIJML4fycRZJzxyYNdFBRtVM2mxOMj6WMX"
        );
    }
}
