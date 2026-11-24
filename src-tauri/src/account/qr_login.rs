//! Official HongGuo website QR-code login state machine.
//! Adapted from N3urda/hongguoTV (`WebLogin.kt`, GPL-3.0-only).
//! Strictly restricts confirmation redirects to `https://hongguoduanju.com` and never
//! forwards user cookies to playback/CDN endpoints.

use super::cloud::CloudAccountClient;
use super::store::AccountCredentials;
use crate::catalog::hg_crypto::base64_decode;
use crate::catalog::hg_signer::{now_millis, query_encode_component, register_anonymous_device};
use crate::catalog::http_client::{parse_url, request_no_redirects, RequestOptions};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const BASE_ORIGIN: &str = "https://hongguoduanju.com";
const ALLOWED_HOST: &str = "hongguoduanju.com";
const WEB_LOGIN_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const MAX_LOGIN_BYTES: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrLoginStart {
    pub session_id: u64,
    pub qr_data_url: String,
    pub expires_in_sec: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QrPollOutcome {
    Waiting,
    Scanned,
    Expired,
    Confirmed(AccountCredentials),
}

#[derive(Default)]
struct LoginCookieJar {
    cookies: HashMap<String, String>,
}

impl LoginCookieJar {
    fn save_from_set_cookies(&mut self, set_cookies: &[String]) {
        for raw in set_cookies {
            let Some(pair) = raw.split(';').next().map(str::trim) else {
                continue;
            };
            let Some((name, val)) = pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            let val = val.trim();
            if name.is_empty()
                || name.chars().any(char::is_control)
                || val.chars().any(char::is_control)
            {
                continue;
            }
            if val.is_empty() || raw.to_ascii_lowercase().contains("max-age=0") {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_owned(), val.to_owned());
            }
        }
    }

    fn header_value(&self) -> Option<String> {
        if self.cookies.is_empty() {
            return None;
        }
        let mut pairs: Vec<(&String, &String)> = self.cookies.iter().collect();
        pairs.sort_by_key(|(k, _)| *k);
        Some(
            pairs
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(String::as_str)
    }

    fn has_session(&self) -> bool {
        self.cookies
            .get("sessionid")
            .or_else(|| self.cookies.get("sessionid_ss"))
            .is_some_and(|v| !v.is_empty())
    }
}

struct ActiveQrSession {
    session_id: u64,
    token: String,
    jar: LoginCookieJar,
}

impl ActiveQrSession {
    fn call_passport(
        &mut self,
        path: &str,
        extra_params: &[(&str, &str)],
    ) -> Result<Value, String> {
        let mut params: Vec<(&str, &str)> = vec![
            ("aid", "513755"),
            ("passport_jssdk_version", "3.4.10"),
            ("passport_jssdk_type", "normal"),
            ("account_sdk_source", "web"),
            ("is_from_ttaccountsdk", "1"),
            ("language", "zh"),
        ];
        params.extend_from_slice(extra_params);
        let query = params
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    query_encode_component(k),
                    query_encode_component(v)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let url = format!("{BASE_ORIGIN}{path}?{query}");
        let referer = format!("{BASE_ORIGIN}/");
        let cookie_header = self.jar.header_value();
        let csrf = self.jar.get("passport_csrf_token").map(str::to_owned);

        let mut headers: Vec<(&str, &str)> = vec![
            ("User-Agent", WEB_LOGIN_UA),
            ("Referer", &referer),
            ("Origin", BASE_ORIGIN),
            ("Accept", "application/json, text/javascript"),
        ];
        if let Some(c) = cookie_header.as_deref() {
            headers.push(("Cookie", c));
        }
        if let Some(token) = csrf.as_deref() {
            headers.push(("x-tt-passport-csrf-token", token));
        }

        let resp = request_no_redirects(
            "GET",
            &url,
            &headers,
            None,
            MAX_LOGIN_BYTES,
            RequestOptions {
                user_agent: Some(WEB_LOGIN_UA),
                timeout_ms: 10_000,
                decompress: true,
                disable_cookies: true,
            },
        )?;
        self.jar.save_from_set_cookies(&resp.set_cookies());
        if !(200..300).contains(&resp.status) {
            return Err(format!(
                "account_qr_http: 登录服务返回 HTTP {}",
                resp.status
            ));
        }
        let root: Value = serde_json::from_slice(&resp.body)
            .map_err(|_| "account_qr_json: 官网登录返回格式异常".to_string())?;
        let Some(data) = root.get("data").filter(|v| v.is_object()) else {
            return Err("account_qr_json: 官网登录缺少 data 字段".into());
        };
        if root.get("message").and_then(Value::as_str) != Some("success") {
            let code = data.get("error_code").and_then(Value::as_i64).unwrap_or(-1);
            let desc = data
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("官网扫码登录失败");
            return Err(format!("account_qr_rejected: {desc}（错误码 {code}）"));
        }
        Ok(data.clone())
    }

    fn finish_redirect(&mut self, raw_redirect: &str) -> Result<(), String> {
        let mut current = resolve_official_redirect(raw_redirect)?;
        let referer = format!("{BASE_ORIGIN}/");
        for _ in 0..MAX_REDIRECTS {
            let cookie_header = self.jar.header_value();
            let mut headers: Vec<(&str, &str)> = vec![
                ("User-Agent", WEB_LOGIN_UA),
                ("Referer", &referer),
                ("Accept", "text/html,application/json"),
            ];
            if let Some(c) = cookie_header.as_deref() {
                headers.push(("Cookie", c));
            }
            let resp = request_no_redirects(
                "GET",
                &current,
                &headers,
                None,
                MAX_LOGIN_BYTES,
                RequestOptions {
                    user_agent: Some(WEB_LOGIN_UA),
                    timeout_ms: 10_000,
                    decompress: true,
                    disable_cookies: true,
                },
            )?;
            self.jar.save_from_set_cookies(&resp.set_cookies());
            if matches!(resp.status, 301 | 302 | 303 | 307 | 308) {
                let Some(loc) = resp.header("location") else {
                    return Ok(());
                };
                current = resolve_official_redirect(loc)?;
                continue;
            }
            if !(200..300).contains(&resp.status) {
                return Err(format!(
                    "account_qr_redirect: 登录确认失败（HTTP {}）",
                    resp.status
                ));
            }
            return Ok(());
        }
        Err("account_qr_redirect: 登录确认跳转次数过多".into())
    }
}

pub fn resolve_official_redirect(location: &str) -> Result<String, String> {
    let trimmed = location.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 || trimmed.chars().any(char::is_control) {
        return Err("account_qr_redirect: 登录确认地址无效".into());
    }
    let resolved = if trimmed.starts_with('/') && !trimmed.starts_with("//") {
        format!("{BASE_ORIGIN}{trimmed}")
    } else {
        trimmed.to_owned()
    };
    let parsed =
        parse_url(&resolved).map_err(|_| "account_qr_redirect: 登录确认地址无效".to_string())?;
    if !parsed.secure || !parsed.host.eq_ignore_ascii_case(ALLOWED_HOST) || parsed.port != 443 {
        return Err("account_qr_redirect: 登录确认跳转到不受支持的站点".into());
    }
    Ok(resolved)
}

pub struct QrLoginManager {
    next_id: AtomicU64,
    active: Mutex<Option<ActiveQrSession>>,
}

impl Default for QrLoginManager {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            active: Mutex::new(None),
        }
    }
}

impl QrLoginManager {
    pub fn start(&self) -> Result<QrLoginStart, String> {
        let session_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut session = ActiveQrSession {
            session_id,
            token: String::new(),
            jar: LoginCookieJar::default(),
        };
        let next_url = format!("{BASE_ORIGIN}/");
        let data = session.call_passport("/passport/web/get_qrcode/", &[("next", &next_url)])?;
        let token = data
            .get("token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= 512)
            .ok_or("account_qr_token: 官网未下发二维码令牌")?
            .to_owned();
        let raw_qr = data
            .get("qrcode")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("account_qr_image: 官网未下发二维码图片")?;
        let b64 = raw_qr
            .split_once("base64,")
            .map(|(_, rest)| rest)
            .unwrap_or(raw_qr)
            .trim();
        let decoded =
            base64_decode(b64).map_err(|_| "account_qr_image: 二维码图片编码无效".to_string())?;
        if decoded.len() < 16 || decoded.len() > 512 * 1024 {
            return Err("account_qr_image: 二维码图片体积异常".into());
        }
        let qr_data_url = format!("data:image/png;base64,{b64}");

        let raw_exp = data
            .get("expire_time")
            .and_then(Value::as_i64)
            .unwrap_or(180);
        let now_sec = (now_millis() / 1000) as i64;
        let expires_in_sec = if raw_exp > 1_000_000_000 {
            (raw_exp - now_sec).clamp(30, 600) as u32
        } else {
            raw_exp.clamp(30, 600) as u32
        };

        session.token = token;
        if let Ok(mut guard) = self.active.lock() {
            *guard = Some(session);
        }
        Ok(QrLoginStart {
            session_id,
            qr_data_url,
            expires_in_sec,
        })
    }

    pub fn cancel(&self) {
        if let Ok(mut guard) = self.active.lock() {
            *guard = None;
        }
    }

    pub fn poll(
        &self,
        session_id: u64,
        cloud: &CloudAccountClient,
    ) -> Result<QrPollOutcome, String> {
        // Take session out briefly while polling so we never hold the mutex across network I/O.
        let mut session = {
            let mut guard = self
                .active
                .lock()
                .map_err(|_| "account_qr_lock: 内部状态锁异常".to_string())?;
            match guard.take() {
                Some(s) if s.session_id == session_id => s,
                other => {
                    *guard = other;
                    return Ok(QrPollOutcome::Expired);
                }
            }
        };

        let next_url = format!("{BASE_ORIGIN}/");
        let token = session.token.clone();
        let poll_res = session.call_passport(
            "/passport/web/check_qrconnect/",
            &[("next", &next_url), ("token", &token)],
        );
        let data = match poll_res {
            Ok(d) => d,
            Err(err) => {
                // Restore session on transient failure unless another session started.
                if let Ok(mut guard) = self.active.lock() {
                    if guard.is_none() {
                        *guard = Some(session);
                    }
                }
                return Err(err);
            }
        };

        let status = data
            .get("status")
            .and_then(|v| {
                v.as_str()
                    .map(|s| s.to_ascii_lowercase())
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .unwrap_or_default();

        if matches!(status.as_str(), "5" | "expired" | "4" | "refused") {
            return Ok(QrPollOutcome::Expired);
        }

        if matches!(status.as_str(), "3" | "confirmed") {
            if let Some(redirect) = data
                .get("redirect_url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                session.finish_redirect(redirect)?;
            }
            if !session.jar.has_session() {
                return Err("account_qr_session: 官网尚未下发登录态，请在手机端确认后重试".into());
            }
            // Confirm official web account info endpoint.
            let _ = session.call_passport("/passport/account/info/v2/", &[])?;
            let cookie = session
                .jar
                .header_value()
                .ok_or("account_qr_session: 登录会话 Cookie 缺失")?;

            // Bind a fresh anonymous device identity and verify `biz_user_id` on HongGuo cloud API.
            let identity = register_anonymous_device()?;
            let temp_creds = AccountCredentials::new(
                cookie.clone(),
                identity.device_id.clone(),
                identity.install_id.clone(),
                String::new(),
                String::new(),
                now_millis(),
            );
            let profile = cloud.profile(&temp_creds)?;
            let final_creds = AccountCredentials::new(
                cookie,
                identity.device_id,
                identity.install_id,
                profile.user_id,
                profile.nickname,
                now_millis(),
            );
            return Ok(QrPollOutcome::Confirmed(final_creds));
        }

        let outcome = if matches!(status.as_str(), "2" | "scanned") {
            QrPollOutcome::Scanned
        } else {
            QrPollOutcome::Waiting
        };

        if let Ok(mut guard) = self.active.lock() {
            if guard.is_none() {
                *guard = Some(session);
            }
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_validator_allows_only_official_https_host() {
        assert_eq!(
            resolve_official_redirect("/auth/callback?code=1").unwrap(),
            "https://hongguoduanju.com/auth/callback?code=1"
        );
        assert!(resolve_official_redirect("https://hongguoduanju.com/ok").is_ok());
        assert!(resolve_official_redirect("http://hongguoduanju.com/ok").is_err());
        assert!(resolve_official_redirect("https://evil.example.com/ok").is_err());
        assert!(resolve_official_redirect("https://hongguoduanju.com.evil.com/ok").is_err());
        assert!(resolve_official_redirect("//evil.example.com/ok").is_err());
    }

    #[test]
    #[ignore]
    fn live_probe_qr_start() {
        let mgr = QrLoginManager::default();
        let started = mgr.start().expect("get_qrcode should succeed live");
        assert!(started.qr_data_url.starts_with("data:image/png;base64,"));
        assert!(started.qr_data_url.len() > 200);
        assert!(started.expires_in_sec >= 30);
        let cloud = CloudAccountClient::new();
        let polled = mgr
            .poll(started.session_id, &cloud)
            .expect("check_qrconnect should succeed live");
        assert_eq!(polled, QrPollOutcome::Waiting);
        mgr.cancel();
    }
}
