//! Native Windows WinHTTP client (zero external crates).
//! Uses system TLS trust store and automatic Windows proxy/VPN settings.
use std::collections::HashMap;
use std::ffi::c_void;
use std::io::{self, Read};
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;
const WINHTTP_ACCESS_TYPE_NO_PROXY: u32 = 1;
const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
const WINHTTP_QUERY_RAW_HEADERS_CRLF: u32 = 22;
const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;
const WINHTTP_ADDREQ_FLAG_ADD: u32 = 0x2000_0000;
const WINHTTP_ADDREQ_FLAG_REPLACE: u32 = 0x8000_0000;
const WINHTTP_OPTION_DISABLE_FEATURE: u32 = 63;
const WINHTTP_DISABLE_COOKIES: u32 = 0x0000_0001;
const WINHTTP_OPTION_DECOMPRESSION: u32 = 118;
const WINHTTP_DECOMPRESSION_FLAG_GZIP: u32 = 0x0000_0001;
const WINHTTP_DECOMPRESSION_FLAG_DEFLATE: u32 = 0x0000_0002;
const MAX_URL_LEN: usize = 8192;

type Hinternet = *mut c_void;

#[repr(C)]
struct WinHttpCurrentUserIeProxyConfig {
    auto_detect: i32,
    auto_config_url: *mut u16,
    proxy: *mut u16,
    proxy_bypass: *mut u16,
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalFree(hmem: *mut c_void) -> *mut c_void;
}

#[link(name = "winhttp")]
extern "system" {
    fn WinHttpGetIEProxyConfigForCurrentUser(
        proxy_config: *mut WinHttpCurrentUserIeProxyConfig,
    ) -> i32;
    fn WinHttpOpen(
        agent: *const u16,
        access_type: u32,
        proxy: *const u16,
        proxy_bypass: *const u16,
        flags: u32,
    ) -> Hinternet;
    fn WinHttpSetTimeouts(
        handle: Hinternet,
        resolve_timeout: i32,
        connect_timeout: i32,
        send_timeout: i32,
        receive_timeout: i32,
    ) -> i32;
    fn WinHttpSetOption(
        handle: Hinternet,
        option: u32,
        buffer: *const c_void,
        buffer_len: u32,
    ) -> i32;
    fn WinHttpConnect(
        session: Hinternet,
        server_name: *const u16,
        server_port: u16,
        reserved: u32,
    ) -> Hinternet;
    fn WinHttpOpenRequest(
        connect: Hinternet,
        verb: *const u16,
        object_name: *const u16,
        version: *const u16,
        referrer: *const u16,
        accept_types: *const *const u16,
        flags: u32,
    ) -> Hinternet;
    fn WinHttpAddRequestHeaders(
        request: Hinternet,
        headers: *const u16,
        headers_len: u32,
        modifiers: u32,
    ) -> i32;
    fn WinHttpSendRequest(
        request: Hinternet,
        headers: *const u16,
        headers_len: u32,
        optional: *const c_void,
        optional_len: u32,
        total_len: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(request: Hinternet, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        request: Hinternet,
        info_level: u32,
        name: *const u16,
        buffer: *mut c_void,
        buffer_len: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpQueryDataAvailable(request: Hinternet, bytes_available: *mut u32) -> i32;
    fn WinHttpReadData(
        request: Hinternet,
        buffer: *mut c_void,
        bytes_to_read: u32,
        bytes_read: *mut u32,
    ) -> i32;
    fn WinHttpCloseHandle(handle: Hinternet) -> i32;
}

struct Handle(Hinternet);

// WinHTTP handles are safe to transfer between threads when owned exclusively.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub struct ParsedUrl {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
}

pub fn parse_url(raw: &str) -> Result<ParsedUrl, String> {
    if raw.len() > MAX_URL_LEN || raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("http_url: URL 格式无效".into());
    }
    let (secure, rest) = if let Some(rest) = raw.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = raw.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err("http_url: 仅支持 http:// 或 https:// 协议".into());
    };
    let rest = rest.split('#').next().unwrap_or(rest);
    let (authority, path_and_query) = match rest.find(['/', '?']) {
        Some(idx) => {
            let suffix = &rest[idx..];
            let pq = if suffix.starts_with('/') {
                suffix.to_owned()
            } else {
                format!("/{suffix}")
            };
            (&rest[..idx], pq)
        }
        None => (rest, "/".to_owned()),
    };
    if authority.is_empty() || authority.contains('@') {
        return Err("http_url: 主机名无效，请勿在链接中内嵌账号密码".into());
    }
    let default_port = if secure { 443 } else { 80 };
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (h, after) = bracketed
            .split_once(']')
            .ok_or("http_url: IPv6 主机格式无效")?;
        let p = if let Some(port_str) = after.strip_prefix(':') {
            port_str
                .parse::<u16>()
                .map_err(|_| "http_url: 端口号无效")?
        } else if after.is_empty() {
            default_port
        } else {
            return Err("http_url: 主机格式无效".into());
        };
        (h.to_owned(), p)
    } else if let Some((h, port_str)) = authority.rsplit_once(':') {
        let p = port_str
            .parse::<u16>()
            .map_err(|_| "http_url: 端口号无效")?;
        (h.to_owned(), p)
    } else {
        (authority.to_owned(), default_port)
    };
    if host.is_empty() || port == 0 {
        return Err("http_url: 主机名或端口无效".into());
    }
    Ok(ParsedUrl {
        secure,
        host,
        port,
        path_and_query,
    })
}

pub fn encode_basic_auth(username: &str, password: &str) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = format!("{username}:{password}");
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4 + 6);
    out.push_str("Basic ");
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        out.push(CHARS[(a >> 2) as usize] as char);
        out.push(CHARS[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub struct HttpResponse {
    pub status: u32,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    #[allow(dead_code)]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn set_cookies(&self) -> Vec<String> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .map(|(_, v)| v.clone())
            .collect()
    }
}

#[derive(Clone, Copy)]
pub struct RequestOptions<'a> {
    pub user_agent: Option<&'a str>,
    pub timeout_ms: i32,
    pub decompress: bool,
    pub disable_cookies: bool,
}

impl Default for RequestOptions<'_> {
    fn default() -> Self {
        Self {
            user_agent: None,
            timeout_ms: 15_000,
            decompress: false,
            disable_cookies: true,
        }
    }
}

/// In-memory per-host cookie jar (used for website SSR `ttwid` cookies, never for
/// anonymous playback model endpoints).
#[derive(Default)]
pub struct MemoryCookies {
    store: Mutex<HashMap<String, Vec<(String, String)>>>,
}

impl MemoryCookies {
    pub fn save_from_headers(&self, host: &str, set_cookies: &[String]) {
        if set_cookies.is_empty() {
            return;
        }
        let host_key = host.to_ascii_lowercase();
        let Ok(mut map) = self.store.lock() else {
            return;
        };
        let entry = map.entry(host_key).or_default();
        for raw in set_cookies {
            let Some(pair) = raw.split(';').next().map(str::trim) else {
                continue;
            };
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            let value = value.trim();
            if name.is_empty()
                || name.chars().any(char::is_control)
                || value.chars().any(char::is_control)
            {
                continue;
            }
            if let Some(existing) = entry.iter_mut().find(|(k, _)| k == name) {
                existing.1 = value.to_owned();
            } else if entry.len() < 32 {
                entry.push((name.to_owned(), value.to_owned()));
            }
        }
    }

    pub fn cookie_header(&self, host: &str) -> Option<String> {
        let host_key = host.to_ascii_lowercase();
        let map = self.store.lock().ok()?;
        let items = map.get(&host_key)?;
        if items.is_empty() {
            return None;
        }
        Some(
            items
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

pub struct HttpStream {
    pub status: u32,
    pub headers: Vec<(String, String)>,
    req: Handle,
    _conn: Handle,
    _session: Handle,
}

impl HttpStream {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let to_read = buf.len().min(64 * 1024) as u32;
        let mut read_bytes: u32 = 0;
        let ok = unsafe {
            WinHttpReadData(
                self.req.0,
                buf.as_mut_ptr().cast::<c_void>(),
                to_read,
                &mut read_bytes,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(read_bytes as usize)
    }
}

fn query_raw_headers(req: Hinternet) -> Vec<(String, String)> {
    let mut bytes_needed: u32 = 0;
    unsafe {
        WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            null(),
            null_mut(),
            &mut bytes_needed,
            null_mut(),
        );
    }
    if bytes_needed < 4 || bytes_needed > 128 * 1024 {
        return Vec::new();
    }
    let mut wide_buf = vec![0u16; (bytes_needed as usize).div_ceil(2)];
    let mut buf_len = (wide_buf.len() * 2) as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            null(),
            wide_buf.as_mut_ptr().cast::<c_void>(),
            &mut buf_len,
            null_mut(),
        )
    };
    if ok == 0 {
        return Vec::new();
    }
    let units = (buf_len as usize) / 2;
    let text = String::from_utf16_lossy(&wide_buf[..units.min(wide_buf.len())]);
    let mut out = Vec::new();
    for line in text.split("\r\n").skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            if !key.is_empty() {
                out.push((key.to_owned(), val.to_owned()));
            }
        }
    }
    out
}

fn prefer_direct_slot() -> &'static Mutex<Option<Instant>> {
    static SLOT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn should_prefer_direct() -> bool {
    prefer_direct_slot()
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_some_and(|until| Instant::now() < until)
}

fn mark_prefer_direct() {
    if let Ok(mut g) = prefer_direct_slot().lock() {
        *g = Some(Instant::now() + Duration::from_secs(120));
    }
}

fn clear_prefer_direct() {
    if let Ok(mut g) = prefer_direct_slot().lock() {
        *g = None;
    }
}

pub fn open_stream(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    options: RequestOptions<'_>,
) -> Result<HttpStream, String> {
    if should_prefer_direct() {
        return match open_stream_with_proxy(method, url, headers, body, options, false) {
            Ok(stream) => Ok(stream),
            Err((err, _)) => {
                clear_prefer_direct();
                open_stream_with_proxy(method, url, headers, body, options, true).map_err(|_| err)
            }
        };
    }
    match open_stream_with_proxy(method, url, headers, body, options, true) {
        Ok(stream) => Ok(stream),
        Err((err, retry_direct)) if retry_direct => {
            match open_stream_with_proxy(method, url, headers, body, options, false) {
                Ok(stream) => {
                    mark_prefer_direct();
                    Ok(stream)
                }
                Err(_) => Err(err),
            }
        }
        Err((err, _)) => Err(err),
    }
}

fn open_stream_with_proxy(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    options: RequestOptions<'_>,
    use_system_proxy: bool,
) -> Result<HttpStream, (String, bool)> {
    let parsed = parse_url(url).map_err(|e| (e, false))?;
    let ua_from_headers = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
        .map(|(_, v)| *v);
    let agent = wide(
        options
            .user_agent
            .or(ua_from_headers)
            .unwrap_or("HongGuoRust/1.0"),
    );
    let session = unsafe {
        if use_system_proxy {
            let h = WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            );
            if h.is_null() {
                WinHttpOpen(
                    agent.as_ptr(),
                    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                    null(),
                    null(),
                    0,
                )
            } else {
                h
            }
        } else {
            WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_NO_PROXY,
                null(),
                null(),
                0,
            )
        }
    };
    if session.is_null() {
        return Err(("http_init: 无法初始化网络会话".into(), false));
    }
    let session = Handle(session);
    let timeout = options.timeout_ms.clamp(1_000, 60_000);
    let connect_timeout = timeout.min(8_000);
    unsafe {
        WinHttpSetTimeouts(
            session.0,
            connect_timeout,
            connect_timeout,
            timeout,
            timeout,
        );
    }

    let host_w = wide(&parsed.host);
    let conn = unsafe { WinHttpConnect(session.0, host_w.as_ptr(), parsed.port, 0) };
    if conn.is_null() {
        return Err(("http_connect: 无法连接目标服务器".into(), use_system_proxy));
    }
    let conn = Handle(conn);

    let verb_w = wide(method);
    let path_w = wide(&parsed.path_and_query);
    let flags = if parsed.secure {
        WINHTTP_FLAG_SECURE
    } else {
        0
    };
    let req = unsafe {
        WinHttpOpenRequest(
            conn.0,
            verb_w.as_ptr(),
            path_w.as_ptr(),
            null(),
            null(),
            null(),
            flags,
        )
    };
    if req.is_null() {
        return Err(("http_request: 创建网络请求失败".into(), false));
    }
    let req = Handle(req);

    if options.decompress {
        let flags: u32 = WINHTTP_DECOMPRESSION_FLAG_GZIP | WINHTTP_DECOMPRESSION_FLAG_DEFLATE;
        unsafe {
            let _ = WinHttpSetOption(
                req.0,
                WINHTTP_OPTION_DECOMPRESSION,
                (&flags as *const u32).cast::<c_void>(),
                std::mem::size_of::<u32>() as u32,
            );
        }
    }

    if options.disable_cookies {
        let disable: u32 = WINHTTP_DISABLE_COOKIES;
        unsafe {
            let _ = WinHttpSetOption(
                req.0,
                WINHTTP_OPTION_DISABLE_FEATURE,
                (&disable as *const u32).cast::<c_void>(),
                std::mem::size_of::<u32>() as u32,
            );
        }
    }

    for (name, value) in headers {
        if name.chars().any(char::is_control) || value.chars().any(char::is_control) {
            return Err(("http_header: 请求头包含非法字符".into(), false));
        }
        // WinHTTP treats "Header:\r\n" with ADDREQ_FLAG_REPLACE as a delete operation and fails
        // with ERROR_WINHTTP_HEADER_NOT_FOUND when the header is not yet present.
        if value.is_empty() {
            continue;
        }
        let line = wide(&format!("{name}: {value}\r\n"));
        let ok = unsafe {
            WinHttpAddRequestHeaders(
                req.0,
                line.as_ptr(),
                (line.len() - 1) as u32,
                WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
            )
        };
        if ok == 0 {
            return Err(("http_header: 设置请求头失败".into(), false));
        }
    }

    let (body_ptr, body_len) = match body {
        Some(bytes) if !bytes.is_empty() => (bytes.as_ptr().cast::<c_void>(), bytes.len() as u32),
        _ => (null(), 0),
    };
    if unsafe { WinHttpSendRequest(req.0, null(), 0, body_ptr, body_len, body_len, 0) } == 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        let retry_direct =
            use_system_proxy && matches!(code, 12002 | 12029 | 12030 | 12152 | 12175);
        return Err((
            format!("http_send: 请求发送失败（请检查地址、代理或网络，错误码 {code}）"),
            retry_direct,
        ));
    }
    if unsafe { WinHttpReceiveResponse(req.0, null_mut()) } == 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        let retry_direct =
            use_system_proxy && matches!(code, 12002 | 12029 | 12030 | 12152 | 12175);
        return Err((
            format!("http_receive: 服务器无响应或连接超时（错误码 {code}）"),
            retry_direct,
        ));
    }

    let mut status: u32 = 0;
    let mut status_len = std::mem::size_of::<u32>() as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast::<c_void>(),
            &mut status_len,
            null_mut(),
        )
    };
    if ok == 0 {
        return Err(("http_status: 无法读取 HTTP 状态码".into(), false));
    }

    let resp_headers = query_raw_headers(req.0);
    Ok(HttpStream {
        status,
        headers: resp_headers,
        req,
        _conn: conn,
        _session: session,
    })
}

pub fn request_with_options(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    max_response_bytes: usize,
    options: RequestOptions<'_>,
) -> Result<HttpResponse, String> {
    let stream = open_stream(method, url, headers, body, options)?;
    let mut out = Vec::new();
    loop {
        let mut available: u32 = 0;
        if unsafe { WinHttpQueryDataAvailable(stream.req.0, &mut available) } == 0 {
            return Err("http_read: 读取响应数据失败".into());
        }
        if available == 0 {
            break;
        }
        let chunk_len = (available as usize).min(64 * 1024);
        if out.len().saturating_add(chunk_len) > max_response_bytes {
            return Err("http_limit: 响应体积超过安全上限".into());
        }
        let start = out.len();
        out.resize(start + chunk_len, 0);
        let mut read: u32 = 0;
        if unsafe {
            WinHttpReadData(
                stream.req.0,
                out[start..].as_mut_ptr().cast::<c_void>(),
                chunk_len as u32,
                &mut read,
            )
        } == 0
        {
            return Err("http_read: 读取响应流中断".into());
        }
        out.truncate(start + read as usize);
        if read == 0 {
            break;
        }
    }
    Ok(HttpResponse {
        status: stream.status,
        headers: stream.headers,
        body: out,
    })
}

pub fn request(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    max_response_bytes: usize,
) -> Result<HttpResponse, String> {
    request_with_options(
        method,
        url,
        headers,
        body,
        max_response_bytes,
        RequestOptions::default(),
    )
}

pub fn get_bytes(url: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let resp = request(
        "GET",
        url,
        &[("Accept", "application/json, */*")],
        None,
        max_bytes,
    )?;
    if !(200..300).contains(&resp.status) {
        return Err(format!("http_status: 服务器返回 HTTP {}", resp.status));
    }
    Ok(resp.body)
}

unsafe fn wide_ptr_to_string(ptr: *mut u16) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 && len < 4096 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    Some(String::from_utf16_lossy(slice))
}

fn is_local_or_lan_host(host: &str) -> bool {
    let h = host.trim().to_ascii_lowercase();
    if h == "localhost" || h == "::1" || h.ends_with(".local") || !h.contains('.') {
        return true;
    }
    if let Ok(ip) = h.parse::<std::net::Ipv4Addr>() {
        return ip.is_loopback() || ip.is_private() || ip.is_link_local();
    }
    false
}

fn normalize_proxy_entry(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }
    // Handle Windows semicolon-separated per-protocol proxy strings:
    // e.g. "http=127.0.0.1:7897;https=127.0.0.1:7897;socks=127.0.0.1:7898"
    let chosen = if trimmed.contains('=') {
        let mut https_val = None;
        let mut http_val = None;
        for part in trimmed.split(';') {
            if let Some((k, v)) = part.trim().split_once('=') {
                let k = k.trim().to_ascii_lowercase();
                let v = v.trim();
                if k == "https" && !v.is_empty() {
                    https_val = Some(v);
                } else if k == "http" && !v.is_empty() {
                    http_val = Some(v);
                }
            }
        }
        https_val.or(http_val)?
    } else {
        trimmed
    };
    if chosen.starts_with("http://") || chosen.starts_with("https://") {
        Some(chosen.to_owned())
    } else if chosen.contains("://") {
        // libmpv's http-proxy option expects an HTTP proxy URL.
        None
    } else {
        Some(format!("http://{chosen}"))
    }
}

/// Detect active HTTP proxy for `libmpv` (process env first, then Windows IE/system proxy).
/// Automatically bypasses proxy for loopback & private LAN hosts (local Alist / NAS WebDAV).
pub fn system_http_proxy(target_url: &str) -> Option<String> {
    let parsed = parse_url(target_url).ok()?;
    if is_local_or_lan_host(&parsed.host) {
        return None;
    }
    for var in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(value) = std::env::var(var) {
            if let Some(proxy) = normalize_proxy_entry(&value) {
                return Some(proxy);
            }
        }
    }
    let mut cfg = WinHttpCurrentUserIeProxyConfig {
        auto_detect: 0,
        auto_config_url: null_mut(),
        proxy: null_mut(),
        proxy_bypass: null_mut(),
    };
    let ok = unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut cfg) };
    if ok == 0 {
        return None;
    }
    let proxy_raw = unsafe { wide_ptr_to_string(cfg.proxy) };
    unsafe {
        if !cfg.auto_config_url.is_null() {
            GlobalFree(cfg.auto_config_url.cast());
        }
        if !cfg.proxy.is_null() {
            GlobalFree(cfg.proxy.cast());
        }
        if !cfg.proxy_bypass.is_null() {
            GlobalFree(cfg.proxy_bypass.cast());
        }
    }
    proxy_raw.and_then(|s| normalize_proxy_entry(&s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_https_ports_and_rejects_embedded_credentials() {
        let a = parse_url("https://example.com/list.json?v=1").unwrap();
        assert!(
            a.secure
                && a.host == "example.com"
                && a.port == 443
                && a.path_and_query == "/list.json?v=1"
        );

        let b = parse_url("http://127.0.0.1:5244/dav/drama/").unwrap();
        assert!(
            !b.secure
                && b.host == "127.0.0.1"
                && b.port == 5244
                && b.path_and_query == "/dav/drama/"
        );

        assert!(parse_url("https://user:pass@example.com/dav").is_err());
        assert!(parse_url("ftp://example.com/a").is_err());
    }

    #[test]
    fn encodes_basic_auth_correctly() {
        assert_eq!(
            encode_basic_auth("admin", "secret"),
            "Basic YWRtaW46c2VjcmV0"
        );
    }
}
