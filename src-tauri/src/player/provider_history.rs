use crate::catalog::progress::ProviderProgressStore;
use std::time::{Duration, Instant};
use tracing::warn;

/// Owned by the libmpv worker. No native session identifier is persisted.
pub struct ProviderHistorySession {
    pub session: u64,
    series_id: String,
    episode_id: String,
    source: String,
    pub resume_at: Option<f64>,
    started: bool,
    position: Option<f64>,
    duration: Option<f64>,
    last_saved: Instant,
}

impl ProviderHistorySession {
    pub fn new(
        session: u64,
        series_id: String,
        episode_id: String,
        source: String,
        resume_at: Option<f64>,
    ) -> Self {
        Self {
            session,
            series_id,
            episode_id,
            source,
            resume_at,
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
            self.save(store, false);
        }
    }

    pub fn seeked(&mut self, store: &ProviderProgressStore, sec: f64) {
        self.position = Some(sec);
        self.save(store, false);
    }

    pub fn save(&mut self, store: &ProviderProgressStore, completed: bool) {
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
        self.last_saved = Instant::now();
    }
}

pub fn finish_provider(
    active: &mut Option<ProviderHistorySession>,
    store: &ProviderProgressStore,
    completed: bool,
) {
    if let Some(mut history) = active.take() {
        history.save(store, completed);
    }
}
