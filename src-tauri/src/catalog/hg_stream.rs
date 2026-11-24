//! In-process authenticated loopback HTTP/1.1 Range streamer for HongGuo CDN playback.
//! Adapted from N3urda/hongguoTV (GPL-3.0-only): RemoteVideo.kt + server/http.mjs.
//!
//! - Binds strictly to `127.0.0.1:0` (ephemeral OS-assigned port, zero port conflicts).
//! - Requires a per-process random 256-bit `X-HGR-Stream-Token` header (passed only
//!   to libmpv via `http-header-fields`, never exposed to WebView or persisted).
//! - Handles primary -> backup CDN failover with header integrity verification.
//! - Performs on-the-fly MP4 CENC AES-128-CTR decryption for arbitrary Range seeks.

use super::hg_crypto::{fill_random_bytes, hex_encode};
use super::hg_media::{decrypt_range, header_end, prepare_cenc_header, PreparedCenc, Rendition};
use super::hg_signer::VIDEO_UA;
use super::http_client::{open_stream, RequestOptions};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const REFERER_HEADER: &str = "https://novel.snssdk.com/";
const INITIAL_PROBE_BYTES: u64 = 256 * 1024;
const STREAM_CHUNK_BYTES: u64 = 256 * 1024;
const MAX_ACTIVE_STREAMS: usize = 8;
const MAX_CONCURRENT_CONNS: usize = 6;
const STREAM_TTL: Duration = Duration::from_secs(600);
const MAX_REQUEST_HEADER: usize = 8192;

#[derive(Debug)]
struct RangeError {
    message: String,
    retryable: bool,
}

impl RangeError {
    fn fatal(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            retryable: false,
        }
    }

    fn retryable(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            retryable: true,
        }
    }
}

struct StreamUrls {
    primary: String,
    backup: Option<String>,
}

pub struct RemoteVideoSession {
    urls: Mutex<StreamUrls>,
    key: Option<[u8; 16]>,
    pub total: u64,
    prepared: PreparedCenc,
    prefix: Vec<u8>,
    created_at: Instant,
}

fn parse_content_range(
    header: &str,
    start: u64,
    end: u64,
    expected_total: u64,
) -> Result<(usize, u64), RangeError> {
    let rest = header
        .trim()
        .strip_prefix("bytes ")
        .ok_or_else(|| RangeError::fatal("视频分段响应格式异常"))?;
    let (span, total_str) = rest
        .split_once('/')
        .ok_or_else(|| RangeError::fatal("视频分段总长度缺失"))?;
    let (begin_str, finish_str) = span
        .split_once('-')
        .ok_or_else(|| RangeError::fatal("视频分段范围格式异常"))?;
    let begin: u64 = begin_str
        .parse()
        .map_err(|_| RangeError::fatal("视频分段起点无效"))?;
    let finish: u64 = finish_str
        .parse()
        .map_err(|_| RangeError::fatal("视频分段终点无效"))?;
    let full: u64 = total_str
        .parse()
        .map_err(|_| RangeError::fatal("视频总大小无效"))?;
    if full <= start {
        return Err(RangeError::fatal("视频分段超出资源大小"));
    }
    let last = end.min(full - 1);
    if begin != start || finish != last {
        return Err(RangeError::fatal("视频分段范围不匹配"));
    }
    if expected_total != 0 && full != expected_total {
        return Err(RangeError::fatal("视频资源已变化，请重试"));
    }
    let count = (last - start + 1) as usize;
    Ok((count, full))
}

fn fetch_cdn_range(
    url: &str,
    start: u64,
    end: u64,
    expected_total: u64,
    timeout_ms: i32,
) -> Result<(Vec<u8>, u64), RangeError> {
    if end < start || end - start >= 8 * 1024 * 1024 {
        return Err(RangeError::fatal("非法媒体分段大小"));
    }
    let range_header = format!("bytes={start}-{end}");
    let headers = [
        ("User-Agent", VIDEO_UA),
        ("Referer", REFERER_HEADER),
        ("Range", range_header.as_str()),
        ("Accept-Encoding", "identity"),
    ];
    let mut stream = open_stream(
        "GET",
        url,
        &headers,
        None,
        RequestOptions {
            user_agent: Some(VIDEO_UA),
            timeout_ms,
            decompress: false,
            disable_cookies: true,
        },
    )
    .map_err(|e| {
        // Network send/receive timeouts are retryable against the backup CDN.
        let retryable = e.starts_with("http_send:") || e.starts_with("http_receive:");
        if retryable {
            RangeError::retryable(e)
        } else {
            RangeError::fatal(e)
        }
    })?;

    if stream.status == 206 {
        let cr = stream
            .header("content-range")
            .ok_or_else(|| RangeError::fatal("视频分段响应缺少 Content-Range"))?;
        let (expected_len, full) = parse_content_range(cr, start, end, expected_total)?;
        let mut buf = vec![0u8; expected_len];
        let mut filled = 0usize;
        while filled < expected_len {
            match stream.read(&mut buf[filled..]) {
                Ok(0) => {
                    return Err(RangeError::retryable("视频分段提前结束"));
                }
                Ok(n) => filled += n,
                Err(_) => {
                    return Err(RangeError::retryable("视频分段读取中断"));
                }
            }
        }
        return Ok((buf, full));
    }

    if stream.status == 200 && start == 0 {
        let declared = stream
            .header("content-length")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        if declared > 0 && declared <= end - start + 1 {
            let expected_len = declared as usize;
            let mut buf = vec![0u8; expected_len];
            let mut filled = 0usize;
            while filled < expected_len {
                match stream.read(&mut buf[filled..]) {
                    Ok(0) => break,
                    Ok(n) => filled += n,
                    Err(_) => return Err(RangeError::retryable("视频读取中断")),
                }
            }
            if filled == expected_len {
                return Ok((buf, declared));
            }
        }
    }

    let retryable = (500..=599).contains(&stream.status)
        && stream
            .header("retry-after")
            .map(str::trim)
            .unwrap_or("")
            .is_empty();
    Err(RangeError {
        message: format!("视频服务器不支持分段读取（HTTP {}）", stream.status),
        retryable,
    })
}

fn fetch_index_from_url(url: &str, timeout_ms: i32) -> Result<(Vec<u8>, Vec<u8>, u64), RangeError> {
    let (first, total) = fetch_cdn_range(url, 0, INITIAL_PROBE_BYTES - 1, 0, timeout_ms)?;
    let moov_end = header_end(&first, total).map_err(RangeError::fatal)?;
    let (prefix, header) = if moov_end > first.len() {
        let (extra, _) = fetch_cdn_range(
            url,
            first.len() as u64,
            (moov_end - 1) as u64,
            total,
            timeout_ms,
        )?;
        let mut combined = Vec::with_capacity(moov_end);
        combined.extend_from_slice(&first);
        combined.extend_from_slice(&extra);
        let h = combined[..moov_end].to_vec();
        (combined, h)
    } else {
        let h = first[..moov_end].to_vec();
        (first, h)
    };
    Ok((prefix, header, total))
}

impl RemoteVideoSession {
    pub fn prepare(rendition: &Rendition) -> Result<Arc<Self>, String> {
        let has_backup = rendition.backup_url.is_some();
        let initial_timeout = if has_backup { 5_000 } else { 10_000 };

        let mut active_primary = rendition.url.clone();
        let mut active_backup = rendition.backup_url.clone();

        let (prefix, header, total) = match fetch_index_from_url(&active_primary, initial_timeout) {
            Ok(ok) => ok,
            Err(err) if err.retryable => {
                if let Some(backup) = active_backup.take() {
                    active_primary = backup.clone();
                    fetch_index_from_url(&backup, 10_000).map_err(|e| e.message)?
                } else {
                    return Err(err.message);
                }
            }
            Err(err) => return Err(err.message),
        };

        let prepared = prepare_cenc_header(&header, rendition.key)?;
        if prepared.samples.iter().any(|s| s.end > total) {
            return Err("hg_mp4: 视频样本超出资源总大小".into());
        }

        Ok(Arc::new(Self {
            urls: Mutex::new(StreamUrls {
                primary: active_primary,
                backup: active_backup,
            }),
            key: rendition.key,
            total,
            prepared,
            prefix,
            created_at: Instant::now(),
        }))
    }

    fn current_url(&self) -> String {
        self.urls
            .lock()
            .map(|g| g.primary.clone())
            .unwrap_or_default()
    }

    fn promote_backup(&self, failed_url: &str) -> Result<String, RangeError> {
        let mut guard = self
            .urls
            .lock()
            .map_err(|_| RangeError::fatal("流状态锁异常"))?;
        if guard.primary != failed_url {
            return Ok(guard.primary.clone());
        }
        let Some(backup) = guard.backup.take() else {
            return Err(RangeError::fatal("无可用备用 CDN"));
        };
        drop(guard);

        // Verify backup index matches original header before promoting (RemoteVideo.kt).
        let header_size = self.prepared.header.len();
        let (backup_head, _) =
            fetch_cdn_range(&backup, 0, (header_size - 1) as u64, self.total, 5_000)?;
        if backup_head.len() != header_size || backup_head != self.prefix[..header_size] {
            return Err(RangeError::fatal("备用视频资源的索引与当前集不一致"));
        }
        if let Ok(mut g) = self.urls.lock() {
            g.primary = backup.clone();
        }
        Ok(backup)
    }

    fn fetch_raw_range(&self, start: u64, end: u64) -> Result<Vec<u8>, String> {
        let url = self.current_url();
        match fetch_cdn_range(&url, start, end, self.total, 12_000) {
            Ok((bytes, _)) => Ok(bytes),
            Err(err) if err.retryable => {
                let backup = self.promote_backup(&url).map_err(|_| err.message.clone())?;
                fetch_cdn_range(&backup, start, end, self.total, 12_000)
                    .map(|(bytes, _)| bytes)
                    .map_err(|e| e.message)
            }
            Err(err) => Err(err.message),
        }
    }

    /// Read decrypted bytes for inclusive range `[start, end]`.
    pub fn read_decrypted(&self, start: u64, end: u64) -> Result<Vec<u8>, String> {
        if start >= self.total || end < start {
            return Ok(Vec::new());
        }
        let clamped_end = end.min(self.total - 1);
        let len = (clamped_end - start + 1) as usize;
        let cached_len = self.prefix.len() as u64;
        let raw = if clamped_end < cached_len {
            self.prefix[start as usize..=clamped_end as usize].to_vec()
        } else if start < cached_len {
            let mut combined = Vec::with_capacity(len);
            combined.extend_from_slice(&self.prefix[start as usize..]);
            let rest = self.fetch_raw_range(cached_len, clamped_end)?;
            combined.extend_from_slice(&rest);
            combined
        } else {
            self.fetch_raw_range(start, clamped_end)?
        };
        Ok(decrypt_range(&raw, start, self.key, &self.prepared))
    }
}

// ---------- HTTP Range Parsing & Constant-Time Token Check ----------

pub fn parse_http_range(value: Option<&str>, total: u64) -> Option<(u64, u64)> {
    if total == 0 {
        return None;
    }
    let Some(raw) = value else {
        return Some((0, total - 1));
    };
    let rest = raw.trim().strip_prefix("bytes=")?;
    // Reject multi-range requests (comma-separated).
    if rest.contains(',') {
        return None;
    }
    let (start_str, end_str) = rest.split_once('-')?;
    if start_str.is_empty() && end_str.is_empty() {
        return None;
    }
    if start_str.is_empty() {
        let suffix: u64 = end_str.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = total.saturating_sub(suffix);
        return Some((start, total - 1));
    }
    let start: u64 = start_str.parse().ok()?;
    if start >= total {
        return None;
    }
    let end = if end_str.is_empty() {
        total - 1
    } else {
        end_str.parse::<u64>().ok()?.min(total - 1)
    };
    (start <= end).then_some((start, end))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------- In-Process Loopback Media Proxy ----------

struct StreamEntry {
    id: String,
    session: Arc<RemoteVideoSession>,
}

struct ProxyInner {
    port: u16,
    token: String,
    streams: Mutex<VecDeque<StreamEntry>>,
    active_conns: AtomicUsize,
}

impl ProxyInner {
    fn register(&self, session: Arc<RemoteVideoSession>) -> (String, String) {
        let mut id_bytes = [0u8; 16];
        fill_random_bytes(&mut id_bytes);
        let id = hex_encode(&id_bytes);
        if let Ok(mut list) = self.streams.lock() {
            let now = Instant::now();
            list.retain(|entry| now.duration_since(entry.session.created_at) < STREAM_TTL);
            while list.len() >= MAX_ACTIVE_STREAMS {
                list.pop_front();
            }
            list.push_back(StreamEntry {
                id: id.clone(),
                session,
            });
        }
        let url = format!("http://127.0.0.1:{}/stream/{id}.mp4", self.port);
        (url, self.token.clone())
    }

    fn resolve(&self, id: &str) -> Option<Arc<RemoteVideoSession>> {
        let mut list = self.streams.lock().ok()?;
        let now = Instant::now();
        list.retain(|entry| now.duration_since(entry.session.created_at) < STREAM_TTL);
        list.iter()
            .find(|entry| entry.id == id)
            .map(|entry| Arc::clone(&entry.session))
    }
}

fn send_simple_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    extra_header: Option<&str>,
) {
    let extra = extra_header.unwrap_or("");
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nCache-Control: no-store\r\n{extra}Connection: close\r\n\r\n"
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn handle_client(mut conn: TcpStream, proxy: Arc<ProxyInner>) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = conn.set_write_timeout(Some(Duration::from_secs(20)));
    let _ = conn.set_nodelay(true);

    let mut raw = Vec::with_capacity(1024);
    let mut buf = [0u8; 2048];
    let header_end_pos = loop {
        match conn.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos;
                }
                if raw.len() > MAX_REQUEST_HEADER {
                    send_simple_response(&mut conn, 431, "Request Header Fields Too Large", None);
                    return;
                }
            }
            Err(_) => return,
        }
    };

    let Ok(header_text) = std::str::from_utf8(&raw[..header_end_pos]) else {
        send_simple_response(&mut conn, 400, "Bad Request", None);
        return;
    };
    let mut lines = header_text.split("\r\n");
    let Some(req_line) = lines.next() else {
        send_simple_response(&mut conn, 400, "Bad Request", None);
        return;
    };
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    if method != "GET" && method != "HEAD" {
        send_simple_response(&mut conn, 405, "Method Not Allowed", None);
        return;
    }

    let mut token_header: Option<&str> = None;
    let mut range_header: Option<&str> = None;
    let mut origin_present = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let val = v.trim();
            if k.eq_ignore_ascii_case("x-hgr-stream-token") {
                token_header = Some(val);
            } else if k.eq_ignore_ascii_case("range") {
                range_header = Some(val);
            } else if k.eq_ignore_ascii_case("origin") {
                origin_present = true;
            }
        }
    }

    if origin_present
        || !token_header.is_some_and(|t| constant_time_eq(t.as_bytes(), proxy.token.as_bytes()))
    {
        send_simple_response(&mut conn, 403, "Forbidden", None);
        return;
    }

    let path = target.split('?').next().unwrap_or("");
    let Some(stream_id) = path
        .strip_prefix("/stream/")
        .and_then(|s| s.strip_suffix(".mp4"))
        .filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
    else {
        send_simple_response(&mut conn, 404, "Not Found", None);
        return;
    };

    let Some(session) = proxy.resolve(stream_id) else {
        send_simple_response(&mut conn, 404, "Not Found", None);
        return;
    };

    let Some((start, end)) = parse_http_range(range_header, session.total) else {
        let extra = format!("Content-Range: bytes */{}\r\n", session.total);
        send_simple_response(&mut conn, 416, "Range Not Satisfiable", Some(&extra));
        return;
    };

    let content_length = end - start + 1;
    let first_end = (start + STREAM_CHUNK_BYTES - 1).min(end);
    let first_chunk = if method == "HEAD" {
        None
    } else {
        match session.read_decrypted(start, first_end) {
            Ok(bytes) => Some(bytes),
            Err(_) => {
                send_simple_response(&mut conn, 502, "Bad Gateway", None);
                return;
            }
        }
    };

    let status_line = if range_header.is_some() {
        "HTTP/1.1 206 Partial Content"
    } else {
        "HTTP/1.1 200 OK"
    };
    let mut resp_head = format!(
        "{status_line}\r\nContent-Type: video/mp4\r\nAccept-Ranges: bytes\r\nContent-Length: {content_length}\r\nCache-Control: no-store\r\nConnection: close\r\n"
    );
    if range_header.is_some() {
        resp_head.push_str(&format!(
            "Content-Range: bytes {start}-{end}/{}\r\n",
            session.total
        ));
    }
    resp_head.push_str("\r\n");

    if conn.write_all(resp_head.as_bytes()).is_err() {
        return;
    }
    if method == "HEAD" {
        let _ = conn.flush();
        return;
    }

    if let Some(first) = first_chunk {
        if conn.write_all(&first).is_err() {
            return;
        }
    }

    let mut offset = first_end + 1;
    while offset <= end {
        let chunk_end = (offset + STREAM_CHUNK_BYTES - 1).min(end);
        let Ok(chunk) = session.read_decrypted(offset, chunk_end) else {
            return;
        };
        if conn.write_all(&chunk).is_err() {
            return;
        }
        offset = chunk_end + 1;
    }
    let _ = conn.flush();
}

pub struct HgStreamProxy {
    inner: OnceLock<Result<Arc<ProxyInner>, String>>,
}

impl Default for HgStreamProxy {
    fn default() -> Self {
        Self {
            inner: OnceLock::new(),
        }
    }
}

impl HgStreamProxy {
    fn ensure_started(&self) -> Result<Arc<ProxyInner>, String> {
        let res = self.inner.get_or_init(|| {
            let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
                .map_err(|_| "hg_proxy: 无法启动本机流媒体代理".to_string())?;
            let port = listener
                .local_addr()
                .map_err(|_| "hg_proxy: 无法读取代理端口".to_string())?
                .port();
            let mut token_bytes = [0u8; 32];
            fill_random_bytes(&mut token_bytes);
            let inner = Arc::new(ProxyInner {
                port,
                token: hex_encode(&token_bytes),
                streams: Mutex::new(VecDeque::new()),
                active_conns: AtomicUsize::new(0),
            });
            let accept_inner = Arc::clone(&inner);
            thread::Builder::new()
                .name("hg-stream-proxy".into())
                .spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let peer_ok = stream.peer_addr().is_ok_and(|addr| addr.ip().is_loopback());
                        if !peer_ok {
                            continue;
                        }
                        let prev = accept_inner.active_conns.fetch_add(1, Ordering::AcqRel);
                        if prev >= MAX_CONCURRENT_CONNS {
                            accept_inner.active_conns.fetch_sub(1, Ordering::AcqRel);
                            let mut conn = stream;
                            send_simple_response(&mut conn, 503, "Service Unavailable", None);
                            continue;
                        }
                        let worker_proxy = Arc::clone(&accept_inner);
                        let spawned =
                            thread::Builder::new()
                                .name("hg-stream-conn".into())
                                .spawn(move || {
                                    handle_client(stream, Arc::clone(&worker_proxy));
                                    worker_proxy.active_conns.fetch_sub(1, Ordering::AcqRel);
                                });
                        if spawned.is_err() {
                            accept_inner.active_conns.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                })
                .map_err(|_| "hg_proxy: 无法创建代理线程".to_string())?;
            tracing::info!(port, "hongguo loopback media proxy ready");
            Ok(inner)
        });
        res.clone()
    }

    /// Prepare a CDN rendition and return `(loopback_url, x_hgr_stream_token)`.
    pub fn open_rendition(&self, rendition: &Rendition) -> Result<(String, String), String> {
        let proxy = self.ensure_started()?;
        let session = RemoteVideoSession::prepare(rendition)?;
        Ok(proxy.register(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_range_parser_handles_open_closed_suffix_and_invalid_ranges() {
        assert_eq!(parse_http_range(None, 1000), Some((0, 999)));
        assert_eq!(parse_http_range(Some("bytes=0-99"), 1000), Some((0, 99)));
        assert_eq!(parse_http_range(Some("bytes=500-"), 1000), Some((500, 999)));
        assert_eq!(
            parse_http_range(Some("bytes=500-5000"), 1000),
            Some((500, 999))
        );
        assert_eq!(parse_http_range(Some("bytes=-200"), 1000), Some((800, 999)));
        assert_eq!(parse_http_range(Some("bytes=-2000"), 1000), Some((0, 999)));
        assert_eq!(parse_http_range(Some("bytes=1000-"), 1000), None);
        assert_eq!(parse_http_range(Some("bytes=500-400"), 1000), None);
        assert_eq!(parse_http_range(Some("bytes=0-10, 20-30"), 1000), None);
        assert_eq!(parse_http_range(Some("bytes=-"), 1000), None);
        assert_eq!(parse_http_range(Some("bytes=-0"), 1000), None);
    }

    #[test]
    fn content_range_parser_validates_exact_window_and_total() {
        assert_eq!(
            parse_content_range("bytes 0-1023/5000", 0, 1023, 0).unwrap(),
            (1024, 5000)
        );
        assert_eq!(
            parse_content_range("bytes 4096-4999/5000", 4096, 8191, 5000).unwrap(),
            (904, 5000)
        );
        assert!(parse_content_range("bytes 0-1023/5001", 0, 1023, 5000).is_err());
        assert!(parse_content_range("bytes 1-1023/5000", 0, 1023, 5000).is_err());
    }
}
