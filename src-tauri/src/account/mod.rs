//! HG-5: Official HongGuo QR-code login & bidirectional cloud account sync.
//! Keeps credentials encrypted on disk via Windows DPAPI and strictly isolated from
//! WebView2 IPC and anonymous playback/CDN endpoints.

pub mod cloud;
pub mod qr_login;
pub mod store;

use crate::catalog::hg_signer::now_millis;
use crate::catalog::progress::ProviderProgressStore;
use cloud::{CloudAccountClient, CloudFavorite, CloudHistoryEntry, CloudReservation};
use qr_login::{QrLoginManager, QrLoginStart, QrPollOutcome};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use store::AccountStore;

const UPLOAD_MIN_INTERVAL: Duration = Duration::from_secs(20);
const UPLOAD_MIN_DELTA_SEC: f64 = 5.0;

/// Sanitized account state safe to expose over Tauri IPC (never contains cookies or device IDs).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatusSnapshot {
    pub signed_in: bool,
    pub nickname: String,
    pub last_synced_ms: Option<u64>,
    pub favorites_count: usize,
    pub history_count: usize,
    pub reservations_count: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrPollSnapshot {
    pub status: String,
    pub account: Option<AccountStatusSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudLibrarySnapshot {
    pub signed_in: bool,
    pub nickname: String,
    pub last_synced_ms: Option<u64>,
    pub favorites: Vec<CloudFavorite>,
    pub history: Vec<CloudHistoryEntry>,
    pub online_reservations: Vec<CloudReservation>,
    pub upcoming_reservations: Vec<CloudReservation>,
    pub last_error: Option<String>,
}

#[derive(Default)]
struct CachedCloudState {
    favorites: Vec<CloudFavorite>,
    history: Vec<CloudHistoryEntry>,
    online_reservations: Vec<CloudReservation>,
    upcoming_reservations: Vec<CloudReservation>,
    last_synced_ms: Option<u64>,
    last_error: Option<String>,
}

struct UploadMark {
    at: Instant,
    position_sec: f64,
}

pub struct AccountService {
    store: AccountStore,
    qr: QrLoginManager,
    cloud: Arc<CloudAccountClient>,
    cache: Mutex<CachedCloudState>,
    upload_marks: Mutex<HashMap<String, UploadMark>>,
}

impl AccountService {
    pub fn new() -> Self {
        Self {
            store: AccountStore::new(),
            qr: QrLoginManager::default(),
            cloud: Arc::new(CloudAccountClient::new()),
            cache: Mutex::new(CachedCloudState::default()),
            upload_marks: Mutex::new(HashMap::new()),
        }
    }

    pub fn status(&self) -> AccountStatusSnapshot {
        let creds = self.store.get();
        let cache = self.cache.lock().ok();
        match creds {
            Some(c) => AccountStatusSnapshot {
                signed_in: true,
                nickname: c.nickname,
                last_synced_ms: cache.as_ref().and_then(|g| g.last_synced_ms),
                favorites_count: cache.as_ref().map(|g| g.favorites.len()).unwrap_or(0),
                history_count: cache.as_ref().map(|g| g.history.len()).unwrap_or(0),
                reservations_count: cache
                    .as_ref()
                    .map(|g| g.online_reservations.len() + g.upcoming_reservations.len())
                    .unwrap_or(0),
                last_error: cache.as_ref().and_then(|g| g.last_error.clone()),
            },
            None => AccountStatusSnapshot {
                signed_in: false,
                nickname: String::new(),
                last_synced_ms: None,
                favorites_count: 0,
                history_count: 0,
                reservations_count: 0,
                last_error: None,
            },
        }
    }

    pub fn cloud_library(&self) -> CloudLibrarySnapshot {
        let creds = self.store.get();
        let cache = self.cache.lock().ok();
        match creds {
            Some(c) => CloudLibrarySnapshot {
                signed_in: true,
                nickname: c.nickname,
                last_synced_ms: cache.as_ref().and_then(|g| g.last_synced_ms),
                favorites: cache
                    .as_ref()
                    .map(|g| g.favorites.clone())
                    .unwrap_or_default(),
                history: cache
                    .as_ref()
                    .map(|g| g.history.clone())
                    .unwrap_or_default(),
                online_reservations: cache
                    .as_ref()
                    .map(|g| g.online_reservations.clone())
                    .unwrap_or_default(),
                upcoming_reservations: cache
                    .as_ref()
                    .map(|g| g.upcoming_reservations.clone())
                    .unwrap_or_default(),
                last_error: cache.as_ref().and_then(|g| g.last_error.clone()),
            },
            None => CloudLibrarySnapshot::default(),
        }
    }

    pub fn start_qr(&self) -> Result<QrLoginStart, String> {
        self.qr.start()
    }

    pub fn cancel_qr(&self) {
        self.qr.cancel();
    }

    pub fn poll_qr(&self, session_id: u64) -> Result<QrPollSnapshot, String> {
        match self.qr.poll(session_id, &self.cloud)? {
            QrPollOutcome::Waiting => Ok(QrPollSnapshot {
                status: "waiting".into(),
                account: None,
            }),
            QrPollOutcome::Scanned => Ok(QrPollSnapshot {
                status: "scanned".into(),
                account: None,
            }),
            QrPollOutcome::Expired => Ok(QrPollSnapshot {
                status: "expired".into(),
                account: None,
            }),
            QrPollOutcome::Confirmed(creds) => {
                // Isolate account state cleanly when switching accounts.
                if let Ok(mut g) = self.cache.lock() {
                    *g = CachedCloudState::default();
                }
                if let Ok(mut m) = self.upload_marks.lock() {
                    m.clear();
                }
                self.store.save(creds)?;
                Ok(QrPollSnapshot {
                    status: "confirmed".into(),
                    account: Some(self.status()),
                })
            }
        }
    }

    pub fn logout(&self) -> Result<AccountStatusSnapshot, String> {
        self.qr.cancel();
        self.store.clear()?;
        if let Ok(mut g) = self.cache.lock() {
            *g = CachedCloudState::default();
        }
        if let Ok(mut m) = self.upload_marks.lock() {
            m.clear();
        }
        Ok(self.status())
    }

    /// Synchronize cloud favorites, watch history, and reservations (online & upcoming),
    /// and merge newer cloud watch records into local `ProviderProgressStore`.
    pub fn sync_now(
        &self,
        progress_store: &ProviderProgressStore,
    ) -> Result<CloudLibrarySnapshot, String> {
        let Some(creds) = self.store.get() else {
            return Err("account_signed_out: 请先扫码登录红果账号".into());
        };

        let mut errors: Vec<String> = Vec::new();
        let favorites = match self.cloud.favorites(&creds) {
            Ok(rows) => Some(rows),
            Err(err) => {
                errors.push(err);
                None
            }
        };
        let history = match self.cloud.history(&creds) {
            Ok(rows) => Some(rows),
            Err(err) => {
                errors.push(err);
                None
            }
        };
        let online_res = match self.cloud.reservations(&creds, true) {
            Ok(rows) => Some(rows),
            Err(err) => {
                tracing::warn!(error = %err, "cloud online reservations sync failed");
                None
            }
        };
        let upcoming_res = match self.cloud.reservations(&creds, false) {
            Ok(rows) => Some(rows),
            Err(err) => {
                tracing::warn!(error = %err, "cloud upcoming reservations sync failed");
                None
            }
        };

        if favorites.is_none() && history.is_none() {
            let msg = errors
                .into_iter()
                .next()
                .unwrap_or_else(|| "account_sync: 同步云端片单失败，请稍后重试".into());
            if let Ok(mut g) = self.cache.lock() {
                g.last_error = Some(msg.clone());
            }
            return Err(msg);
        }

        // Merge newer cloud watch progress into local `ProviderProgressStore` by timestamp.
        if let Some(ref hist) = history {
            for item in hist {
                let Some(raw_series) = item.series.id.strip_prefix("hongguo:") else {
                    continue;
                };
                let stable_key = format!("hongguo:{raw_series}:{}", item.episode_id);
                let completed = item
                    .duration_sec
                    .is_some_and(|d| d > 5.0 && item.position_sec >= d - 5.0);
                let _ = progress_store.merge_cloud_bookmark(
                    &item.series.id,
                    &item.episode_id,
                    &stable_key,
                    item.position_sec,
                    item.duration_sec,
                    item.updated_at_ms,
                    completed,
                );
            }
        }

        if let Ok(mut g) = self.cache.lock() {
            if let Some(rows) = favorites {
                g.favorites = rows;
            }
            if let Some(rows) = history {
                g.history = rows;
            }
            if let Some(rows) = online_res {
                g.online_reservations = rows;
            }
            if let Some(rows) = upcoming_res {
                g.upcoming_reservations = rows;
            }
            g.last_synced_ms = Some(now_millis());
            g.last_error = errors.into_iter().next();
        }
        Ok(self.cloud_library())
    }

    /// Bidirectional cloud favorite write with read-back confirmation.
    pub fn set_cloud_favorite(&self, scoped_series_id: &str, favorite: bool) -> Result<(), String> {
        let Some(raw_id) = scoped_series_id.strip_prefix("hongguo:") else {
            return Ok(());
        };
        let Some(creds) = self.store.get() else {
            return Ok(());
        };
        self.cloud.set_favorite(&creds, raw_id, favorite)?;
        // Refresh cached cloud favorites after write so UI reflects verified server state.
        if let Ok(updated) = self.cloud.favorites(&creds) {
            if let Ok(mut g) = self.cache.lock() {
                g.favorites = updated;
                g.last_synced_ms = Some(now_millis());
            }
        }
        Ok(())
    }

    /// Fire-and-forget debounced background upload of HongGuo watch progress.
    /// Never blocks the libmpv playback worker; errors are logged only.
    #[allow(clippy::too_many_arguments)]
    pub fn schedule_progress_upload(
        self: &Arc<Self>,
        scoped_series_id: &str,
        episode_id: &str,
        episode_number: usize,
        episode_count: usize,
        position_sec: f64,
        duration_sec: Option<f64>,
        force: bool,
    ) {
        let Some(raw_series_id) = scoped_series_id.strip_prefix("hongguo:").map(str::to_owned)
        else {
            return;
        };
        let Some(creds) = self.store.get() else {
            return;
        };
        if !position_sec.is_finite() || position_sec < 1.0 {
            return;
        }
        let key = format!("{raw_series_id}:{episode_id}");
        if let Ok(mut marks) = self.upload_marks.lock() {
            if let Some(prev) = marks.get(&key) {
                if !force
                    && (prev.at.elapsed() < UPLOAD_MIN_INTERVAL
                        || (position_sec - prev.position_sec).abs() < UPLOAD_MIN_DELTA_SEC)
                {
                    return;
                }
            }
            marks.insert(
                key,
                UploadMark {
                    at: Instant::now(),
                    position_sec,
                },
            );
        }
        let cloud = Arc::clone(&self.cloud);
        let episode_id = episode_id.to_owned();
        let position_ms = (position_sec * 1000.0).round().max(0.0) as u64;
        let duration_ms = duration_sec
            .filter(|d| d.is_finite() && *d > 0.0)
            .map(|d| (d * 1000.0).round() as u64)
            .unwrap_or(0);
        let updated_at_ms = now_millis();

        std::thread::spawn(move || {
            if let Err(err) = cloud.upload_progress(
                &creds,
                &raw_series_id,
                &episode_id,
                episode_number.max(1),
                position_ms,
                duration_ms,
                episode_count,
                updated_at_ms,
            ) {
                tracing::warn!(error = %err, "background cloud progress upload failed");
            }
        });
    }
}
