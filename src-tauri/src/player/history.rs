use crate::local_library::LocalLibrary;
use std::time::{Duration, Instant};
use tracing::warn;

/// Owned only by the libmpv worker; one session is always tied to one local item.
pub struct HistorySession {
    pub session: u64,
    library_id: u64,
    pub resume_at: Option<f64>,
    started: bool,
    position: Option<f64>,
    duration: Option<f64>,
    last_saved: Instant,
}

impl HistorySession {
    pub fn new(session: u64, library_id: u64, resume_at: Option<f64>) -> Self {
        Self {
            session,
            library_id,
            resume_at,
            started: false,
            position: None,
            duration: None,
            last_saved: Instant::now(),
        }
    }

    pub fn started(&mut self, library: &LocalLibrary) {
        if !self.started {
            self.started = true;
            if let Err(e) = library.mark_started(self.library_id) {
                warn!(session_id = self.session, error = %e, "history start save failed");
            }
        }
    }

    pub fn progress(
        &mut self,
        library: &LocalLibrary,
        position: Option<f64>,
        duration: Option<f64>,
    ) {
        if let Some(sec) = position.filter(|v| v.is_finite() && *v >= 0.0) {
            self.position = Some(sec);
        }
        if let Some(sec) = duration.filter(|v| v.is_finite() && *v > 0.0) {
            self.duration = Some(sec);
        }
        if self.started
            && self.position.is_some()
            && self.last_saved.elapsed() >= Duration::from_secs(5)
        {
            self.save(library, false);
        }
    }

    pub fn seeked(&mut self, library: &LocalLibrary, sec: f64) {
        self.position = Some(sec);
        self.save(library, false);
    }

    pub fn save(&mut self, library: &LocalLibrary, completed: bool) {
        if !self.started {
            return;
        }
        if let Err(e) = library.record_progress(
            self.library_id,
            self.position,
            self.duration,
            completed,
        ) {
            warn!(session_id = self.session, error = %e, "history checkpoint save failed");
        }
        self.last_saved = Instant::now();
    }
}

pub fn finish(
    active: &mut Option<HistorySession>,
    library: &LocalLibrary,
    completed: bool,
) {
    if let Some(mut history) = active.take() {
        history.save(library, completed);
    }
}
