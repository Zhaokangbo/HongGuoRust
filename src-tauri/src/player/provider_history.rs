use crate::account::AccountService;
use crate::catalog::progress::ProviderProgressStore;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::warn;

/// Owned by the libmpv worker. No native session identifier is persisted.
pub struct ProviderHistorySession {
    pub session: u64,
    series_id: String,
    episode_id: String,
    episode_number: usize,
    episode_count: usize,
    source: String,
    pub resume_at: Option<f64>,
    account: Option<Arc<AccountService>>,
    started: bool,
    position: Option<f64>,
    duration: Option<f64>,
    last_saved: Instant,
}

impl ProviderHistorySession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: u64,
        series_id: String,
        episode_id: String,
        episode_number: usize,
        episode_count: usize,
        source: String,
        resume_at: Option<f64>,
        account: Option<Arc<AccountService>>,
    ) -> Self {
        Self {
            session,
            series_id,
            episode_id,
            episode_number,
            episode_count,
            source,
            resume_at,
            account,
            started: false,
            position: None,
            duration: None,
            last_saved: Instant::now(),
        }
    }

    pub fn started(&mut self, store: &ProviderProgressStore) {
        if self.started {
            return;
        }
        self.started = true;
        if let Err(e) = store.mark_started(&self.series_id, &self.episode_id, &self.source) {
            warn!(session_id = self.session, error = %e, "provider history start failed");
        }
    }

    pub fn progress(
        &mut self,
        store: &ProviderProgressStore,
        position: Option<f64>,
        duration: Option<f64>,
    ) {
        if let Some(p) = position.filter(|v| v.is_finite() && *v >= 0.0) {
            self.position = Some(p);
        }
        if let Some(d) = duration.filter(|v| v.is_finite() && *v > 0.0) {
            self.duration = Some(d);
        }
        if self.started
            && self.position.is_some()
            && self.last_saved.elapsed() >= Duration::from_secs(5)
        {
            self.save(store, false, false);
        }
    }

    pub fn seeked(&mut self, store: &ProviderProgressStore, sec: f64) {
        self.position = Some(sec);
        self.save(store, false, false);
    }

    pub fn save(&mut self, store: &ProviderProgressStore, completed: bool, force_cloud: bool) {
        if !self.started {
            return;
        }
        if let Err(e) = store.save_progress(
            &self.series_id,
            &self.episode_id,
            &self.source,
            self.position,
            self.duration,
            completed,
        ) {
            warn!(session_id = self.session, error = %e, "provider checkpoint failed");
        }
        if let Some(account) = &self.account {
            let pos = if completed {
                self.duration.or(self.position).unwrap_or(0.0)
            } else {
                self.position.unwrap_or(0.0)
            };
            account.schedule_progress_upload(
                &self.series_id,
                &self.episode_id,
                self.episode_number,
                self.episode_count,
                pos,
                self.duration,
                force_cloud || completed,
            );
        }
        self.last_saved = Instant::now();
    }
}

pub fn finish_provider(
    active: &mut Option<ProviderHistorySession>,
    store: &ProviderProgressStore,
    completed: bool,
) {
    if let Some(mut history) = active.take() {
        history.save(store, completed, true);
    }
}
