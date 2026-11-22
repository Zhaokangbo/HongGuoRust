//! HG-2A optional community SDK provider, powered by a separate opt-in local
//! Node bridge. This is NOT an official HongGuo API or a licensed content source.
//! All traffic from the Tauri client stays on fixed 127.0.0.1:19091; the
//! optional Node SDK process handles its own upstream API requests.
//! Production release builds never register this experimental adapter.
use super::request::RequestContext;
use super::{ContentProvider, Episode, MediaTarget, Series, SeriesDetail};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

const MAX_BYTES: usize = 100 * 1024;
const MAX_HEADER: usize = 4096;
const PORT: u16 = 19091;

pub struct HgCommunityProvider {
    token: String,
    hosts: HashSet<String>,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-' || ch == b'_')
}

fn is_host(value: &str) -> bool {
    value.len() >= 4
        && value.len() <= 253
        && value.contains('.')
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'.' || ch == b'-')
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
}

/// Only display public HTTPS cover images provided by the opt-in SDK.
 /// Never accept credentials, private IP literals, fragments or unsafe schemes.
 /// This is syntactic validation, not a network DNS/redirect pinning guarantee.
fn valid_cover_url(value: &str) -> Option<&str> {
    if value.len() > 2048
        || !value.starts_with("https://")
        || value.contains('#')
        || value.bytes().any(|byte| byte <= 0x20 || byte == 0x7f)
    {
        return None;
    }
    let host = value[8..].split(['/', '?']).next()?;
    let suffix = host.rsplit('.').next()?;
    if !is_host(host)
        || host.contains('@')
        || host.contains(':')
        || suffix.len() < 2
        || !suffix.bytes().all(|byte| byte.is_ascii_alphabetic())
        || ["local", "localhost", "internal", "test", "invalid"].contains(&suffix)
        || host.parse::<std::net::IpAddr>().is_ok()
    {
        return None;
    }
    Some(value)
}

fn url_encode(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

impl HgCommunityProvider {
    pub fn from_env() -> Option<Self> {
        if !cfg!(debug_assertions)
            || std::env::var("HONGGUORUST_HG2A_EXPERIMENT").ok().as_deref() != Some("1")
        {
            return None;
        }
        let token = std::env::var("HONGGUORUST_HG2A_TOKEN").ok()?;
        if !(48..=128).contains(&token.len()) || !token.bytes().all(|v| v.is_ascii_hexdigit()) {
            return None;
        }
        let hosts = std::env::var("HONGGUORUST_HG2A_MEDIA_HOSTS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| is_host(s))
            .collect();
        Some(Self { token, hosts })
    }

    fn get_json(&self, path: &str, context: Option<&RequestContext>) -> Result<Value, String> {
        if path.len() > 1024
            || !path.starts_with("/v1/")
            || path.bytes().any(|b| b <= 32 || b >= 127)
        {
            return Err("hgcommunity_invalid_route".into());
        }
        if let Some(ctx) = context {
            ctx.check()?;
        }
        let started = Instant::now();
        let mut stream = TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, PORT)),
            Duration::from_millis(250),
        )
        .map_err(|_| "hgcommunity_offline: Start the opt-in local SDK bridge")?;
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|_| "hgcommunity_io_error")?;
        stream
            .set_write_timeout(Some(Duration::from_millis(300)))
            .map_err(|_| "hgcommunity_io_error")?;
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nX-HG2A-Token: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
            self.token
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|_| "hgcommunity_io_error")?;
        let mut raw = Vec::new();
        let mut temp = [0u8; 4096];
        loop {
            if let Some(ctx) = context {
                ctx.check()?;
            }
            if started.elapsed() > Duration::from_secs(8) {
                return Err("hgcommunity_timeout".into());
            }
            match stream.read(&mut temp) {
                Ok(0) => break,
                Ok(bytes) => {
                    if raw.len() + bytes > MAX_BYTES {
                        return Err("hgcommunity_response_too_large".into());
                    }
                    raw.extend_from_slice(&temp[..bytes]);
                }
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(_) => return Err("hgcommunity_io_error".into()),
            }
        }
        let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
            return Err("hgcommunity_invalid_http".into());
        };
        if split > MAX_HEADER {
            return Err("hgcommunity_header_too_large".into());
        }
        let header = std::str::from_utf8(&raw[..split]).map_err(|_| "hgcommunity_invalid_http")?;
        let status = header.lines().next().unwrap_or("");
        if !status.starts_with("HTTP/1.1 ") || status.len() < 12 {
            return Err("hgcommunity_invalid_http".into());
        }
        let status_code: u16 = status
            .get(9..12)
            .ok_or("hgcommunity_invalid_http")?
            .parse()
            .map_err(|_| "hgcommunity_invalid_http")?;
        let mut length = None;
        let mut is_json = false;
        for line in header.split("\r\n").skip(1) {
            if let Some((key, value)) = line.split_once(':') {
                if key.eq_ignore_ascii_case("content-length") {
                    if length.is_some() {
                        return Err("hgcommunity_invalid_http".into());
                    }
                    length = Some(
                        value
                            .trim()
                            .parse::<usize>()
                            .map_err(|_| "hgcommunity_invalid_http")?,
                    );
                } else if key.eq_ignore_ascii_case("content-type")
                    && value.trim().starts_with("application/json")
                {
                    is_json = true;
                } else if key.eq_ignore_ascii_case("transfer-encoding") {
                    return Err("hgcommunity_unsupported_encoding".into());
                }
            }
        }
        let body = &raw[split + 4..];
        if !is_json || length != Some(body.len()) || body.len() > MAX_BYTES - MAX_HEADER {
            return Err("hgcommunity_invalid_http".into());
        }
        if status_code != 200 {
            return Err(format!("hgcommunity_http_{status_code}"));
        }
        let parsed: Value = serde_json::from_slice(body).map_err(|_| "hgcommunity_invalid_json")?;
        if parsed.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
            return Err("hgcommunity_schema_mismatch".into());
        }
        Ok(parsed)
    }

    fn map_series(item: &Value) -> Result<Series, String> {
        let id = item["id"].as_str().unwrap_or("");
        let title = item["title"].as_str().unwrap_or("");
        let description = item["description"].as_str().unwrap_or("");
        let genre = item["genre"].as_str().unwrap_or("");
        let count = item["episodes"].as_u64().unwrap_or(0);
        if !valid_id(id)
            || title.is_empty()
            || title.chars().count() > 100
            || description.chars().count() > 300
            || genre.chars().count() > 40
            || count == 0
            || count > 200
        {
            return Err("hgcommunity_invalid_catalog".into());
        }
        Ok(Series {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            genre: genre.into(),
            badge: "非官方社区数据（实验）".into(),
            artwork: "studio".into(),
            cover_url: valid_cover_url(item["coverUrl"].as_str()).map(str::to_owned),
            episode_count: count as usize,
            provider_id: "hgcommunity".into(),
        })
    }

    fn search_inner(
        &self,
        query: &str,
        context: Option<&RequestContext>,
    ) -> Result<Vec<Series>, String> {
        if query.len() > 128 || query.chars().any(char::is_control) {
            return Err("hgcommunity_invalid_query".into());
        }
        let value = self.get_json(&format!("/v1/search?q={}", url_encode(query)), context)?;
        let items = value["items"]
            .as_array()
            .ok_or("hgcommunity_invalid_catalog")?;
        if items.len() > 50 {
            return Err("hgcommunity_invalid_catalog".into());
        }
        let mut ids = HashSet::new();
        items
            .iter()
            .map(|raw| {
                let item = Self::map_series(raw)?;
                if !ids.insert(item.id.clone()) {
                    return Err("hgcommunity_duplicate_series".into());
                }
                Ok(item)
            })
            .collect()
    }

    fn detail_inner(
        &self,
        series_id: &str,
        context: Option<&RequestContext>,
    ) -> Result<Option<SeriesDetail>, String> {
        if !valid_id(series_id) {
            return Err("hgcommunity_invalid_series".into());
        }
        let value = match self.get_json(&format!("/v1/series/{series_id}"), context) {
            Ok(value) => value,
            Err(err) if err == "hgcommunity_http_404" => return Ok(None),
            Err(err) => return Err(err),
        };
        let raw = &value["detail"];
        let series = Self::map_series(&raw["series"])?;
        if series.id != series_id {
            return Err("hgcommunity_invalid_catalog".into());
        }
        let entries = raw["episodes"]
            .as_array()
            .ok_or("hgcommunity_invalid_catalog")?;
        if entries.is_empty() || entries.len() != series.episode_count {
            return Err("hgcommunity_invalid_catalog".into());
        }
        let mut seen_ids = HashSet::new();
        let mut seen_numbers = HashSet::new();
        let mut episodes = Vec::new();
        for raw_ep in entries {
            let id = raw_ep["id"].as_str().unwrap_or("");
            let title = raw_ep["title"].as_str().unwrap_or("");
            let number = raw_ep["number"].as_u64().unwrap_or(0);
            if !valid_id(id)
                || title.is_empty()
                || title.chars().count() > 100
                || number == 0
                || number > 200
                || !seen_ids.insert(id.to_owned())
                || !seen_numbers.insert(number)
            {
                return Err("hgcommunity_invalid_episode".into());
            }
            episodes.push(Episode {
                id: id.into(),
                number: number as usize,
                title: title.into(),
                runtime_label: "—".into(),
                playable: raw_ep["playable"].as_bool().unwrap_or(false),
            });
        }
        episodes.sort_by_key(|ep| ep.number);
        Ok(Some(SeriesDetail { series, episodes }))
    }
}

impl ContentProvider for HgCommunityProvider {
    fn id(&self) -> &'static str {
        "hgcommunity"
    }

    fn name(&self) -> &'static str {
        "红果社区 SDK（非官方实验）"
    }

    fn supports_play(&self) -> bool {
        !self.hosts.is_empty()
    }

    fn search(&self, query: &str) -> Result<Vec<Series>, String> {
        self.search_inner(query, None)
    }

    fn search_with_request(
        &self,
        query: &str,
        request: &RequestContext,
    ) -> Result<Vec<Series>, String> {
        request.check()?;
        self.search_inner(query, Some(request))
    }

    fn detail(&self, series_id: &str) -> Result<Option<SeriesDetail>, String> {
        self.detail_inner(series_id, None)
    }

    fn detail_with_request(
        &self,
        series_id: &str,
        request: &RequestContext,
    ) -> Result<Option<SeriesDetail>, String> {
        request.check()?;
        self.detail_inner(series_id, Some(request))
    }

    fn resolve_episode(&self, series_id: &str, episode_id: &str) -> Result<MediaTarget, String> {
        if !self.supports_play() {
            return Err("hgcommunity_play_disabled: Media host not explicitly approved".into());
        }
        if !valid_id(episode_id) {
            return Err("hgcommunity_invalid_episode".into());
        }
        let detail = self
            .detail_inner(series_id, None)?
            .ok_or("hgcommunity_unknown_series")?;
        if !detail.episodes.iter().any(|ep| ep.id == episode_id && ep.playable) {
            return Err("hgcommunity_unknown_episode".into());
        }
        let value = self.get_json(
            &format!("/v1/series/{series_id}/episodes/{episode_id}/play"),
            None,
        )?;
        let url = value["mediaUrl"]
            .as_str()
            .ok_or("hgcommunity_invalid_media")?;
        let verified = crate::player::source::public_https_url(url)
            .map_err(|_| "hgcommunity_invalid_media")?;
        let hostname = verified[8..]
            .split(['/', '?'])
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if !self.hosts.contains(&hostname) {
            return Err("hgcommunity_unapproved_media_host".into());
        }
        Ok(MediaTarget::Https(verified))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_is_percent_encoded_and_id_guards_reject_paths() {
        assert_eq!(
            url_encode("短剧 测试"),
            "%E7%9F%AD%E5%89%A7%20%E6%B5%8B%E8%AF%95"
        );
        assert!(valid_id("7697622096326708286"));
        assert!(!valid_id("../video.mp4"));
        assert!(!valid_id("http://example.com"));
    }

    #[test]
    fn public_cover_validation_preserves_signed_https_queries_and_rejects_private_urls() {
        let valid = "https://covers.example.org/signed.webp?token=a%2Fb&expires=1892345678";
        assert_eq!(valid_cover_url(valid), Some(valid));
        for url in [
            "http://covers.example.org/poster.webp",
            "file:///C:/secrets/poster.jpg",
            "https://127.0.0.1/a.jpg",
            "https://192.168.1.2/a.jpg",
            "https://localhost/a.jpg",
            "https://private.internal/a.webp",
            "https://user:password@covers.example.org/a.jpg",
            "https://covers.example.org:8443/a.jpg",
            "https://covers.example.org/a.jpg#frag",
            "https://covers.example.org/a.jpg\\nHeader:secret",
        ] {
            assert!(valid_cover_url(url).is_none(), "unexpected allowed cover URL");
        }
        let mapped = HgCommunityProvider::map_series(&serde_json::json!({
            "id": "real-123",
            "title": "合成元数据",
            "description": "合法封面字段",
            "genre": "测试",
            "episodes": 2,
            "coverUrl": valid
        }))
        .unwrap();
        assert_eq!(mapped.cover_url.as_deref(), Some(valid));
    }

    #[test]
    fn no_opt_in_means_no_source_registration() {
        if std::env::var("HONGGUORUST_HG2A_EXPERIMENT").ok().as_deref() != Some("1") {
            assert!(HgCommunityProvider::from_env().is_none());
        }
    }

    #[test]
    fn rejects_invalid_source_fields_and_untrusted_ids() {
        assert!(HgCommunityProvider::map_series(&serde_json::json!({
            "id": "../wrong", "title": "bad", "description": "", "genre": "a", "episodes": 1
        }))
        .is_err());
        assert!(HgCommunityProvider::map_series(&serde_json::json!({
            "id": "valid", "title": "ok", "description": "", "genre": "a", "episodes": 900
        }))
        .is_err());
    }
}
