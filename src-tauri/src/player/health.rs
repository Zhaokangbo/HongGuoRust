//! M12-A: bounded, in-memory native playback lifecycle diagnostics.
//! Never store or return media URLs, filesystem paths, titles or raw errors.
use serde::Serialize;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealth {
    pub schema_version: u32,
    pub active_session_id: Option<u64>,
    pub sessions_opened: u64,
    pub files_loaded: u64,
    pub playback_restarts: u64,
    pub playback_ended: u64,
    pub playback_failed: u64,
    pub playback_stopped: u64,
    pub buffer_events: u64,
    pub buffer_total_ms: u64,
    pub buffer_current_ms: Option<u64>,
    pub last_load_ms: Option<u64>,
    pub last_playback_restart_ms: Option<u64>,
    pub libmpv_releases: u64,
    pub last_release_ms: Option<u64>,
    pub max_release_ms: u64,
    pub last_outcome: Option<&'static str>,
}

impl Default for RuntimeHealth {
    fn default() -> Self {
        Self {
            schema_version: 1,
            active_session_id: None,
            sessions_opened: 0,
            files_loaded: 0,
            playback_restarts: 0,
            playback_ended: 0,
            playback_failed: 0,
            playback_stopped: 0,
            buffer_events: 0,
            buffer_total_ms: 0,
            buffer_current_ms: None,
            last_load_ms: None,
            last_playback_restart_ms: None,
            libmpv_releases: 0,
            last_release_ms: None,
            max_release_ms: 0,
            last_outcome: None,
        }
    }
}

pub struct RuntimeHealthMeter {
    current: RuntimeHealth,
    accepted_at: Option<(u64, Instant)>,
    buffer_at: Option<(u64, Instant)>,
    first_restart_id: Option<u64>,
}

impl Default for RuntimeHealthMeter {
    fn default() -> Self {
        Self {
            current: RuntimeHealth::default(),
            accepted_at: None,
            buffer_at: None,
            first_restart_id: None,
        }
    }
}

fn duration_ms(start: Instant, now: Instant) -> u64 {
    now.saturating_duration_since(start)
        .as_millis()
        .min(u64::MAX as u128) as u64
}

impl RuntimeHealthMeter {
    /// Snapshot only scalar, non-identifying counters. No filesystem writes.
    pub fn snapshot(&self, now: Instant) -> RuntimeHealth {
        let mut result = self.current;
        result.buffer_current_ms = self
            .buffer_at
            .filter(|(id, _)| Some(*id) == result.active_session_id)
            .map(|(_, since)| duration_ms(since, now));
        result
    }

    fn finish_buffer(&mut self, now: Instant) {
        if let Some((_, since)) = self.buffer_at.take() {
            self.current.buffer_total_ms = self
                .current
                .buffer_total_ms
                .saturating_add(duration_ms(since, now));
        }
    }

    pub fn open(&mut self, session: u64, accepted: Instant, now: Instant) {
        self.finish_buffer(now);
        self.current.active_session_id = Some(session);
        self.current.sessions_opened = self.current.sessions_opened.saturating_add(1);
        self.current.last_load_ms = None;
        self.current.last_playback_restart_ms = None;
        self.current.last_outcome = None;
        self.accepted_at = Some((session, accepted));
        self.first_restart_id = None;
    }

    pub fn loaded(&mut self, session: u64, now: Instant) {
        if self.current.active_session_id != Some(session) {
            return;
        }
        self.current.files_loaded = self.current.files_loaded.saturating_add(1);
        if let Some((id, accepted)) = self.accepted_at {
            if id == session {
                self.current.last_load_ms = Some(duration_ms(accepted, now));
            }
        }
    }

    /// A playback-restart event marks mpv's playback start/restart signal;
    /// it is NOT a measured frame presentation or audiovisual latency metric.
    pub fn restart(&mut self, session: u64, now: Instant) {
        if self.current.active_session_id != Some(session) {
            return;
        }
        self.current.playback_restarts = self.current.playback_restarts.saturating_add(1);
        if self.first_restart_id != Some(session) {
            if let Some((id, accepted)) = self.accepted_at {
                if id == session {
                    self.current.last_playback_restart_ms = Some(duration_ms(accepted, now));
                }
            }
            self.first_restart_id = Some(session);
        }
    }

    pub fn buffer(&mut self, session: u64, waiting: bool, now: Instant) {
        if self.current.active_session_id != Some(session) {
            return;
        }
        if waiting {
            if self.buffer_at.is_none() {
                self.buffer_at = Some((session, now));
                self.current.buffer_events = self.current.buffer_events.saturating_add(1);
            }
        } else if self
            .buffer_at
            .as_ref()
            .is_some_and(|(id, _)| *id == session)
        {
            self.finish_buffer(now);
        }
    }

    fn finish_session(&mut self, session: u64, outcome: &'static str, now: Instant) -> bool {
        if self.current.active_session_id != Some(session) {
            return false;
        }
        self.finish_buffer(now);
        self.current.active_session_id = None;
        self.current.last_outcome = Some(outcome);
        self.accepted_at = None;
        self.first_restart_id = None;
        true
    }

    pub fn ended(&mut self, session: u64, now: Instant) {
        if self.finish_session(session, "ended", now) {
            self.current.playback_ended = self.current.playback_ended.saturating_add(1);
        }
    }

    pub fn failed(&mut self, session: u64, now: Instant) {
        if self.finish_session(session, "error", now) {
            self.current.playback_failed = self.current.playback_failed.saturating_add(1);
        }
    }

    pub fn stopped(&mut self, session: u64, now: Instant) {
        if self.finish_session(session, "stopped", now) {
            self.current.playback_stopped = self.current.playback_stopped.saturating_add(1);
        }
    }

    pub fn closed(&mut self, now: Instant) {
        if let Some(id) = self.current.active_session_id {
            self.finish_session(id, "closed", now);
        }
    }

    pub fn released(&mut self, duration_ms: u64) {
        self.current.libmpv_releases = self.current.libmpv_releases.saturating_add(1);
        self.current.last_release_ms = Some(duration_ms);
        self.current.max_release_ms = self.current.max_release_ms.max(duration_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn in_memory_counters_track_one_session_and_total_buffering() {
        let now = Instant::now();
        let mut meter = RuntimeHealthMeter::default();
        meter.open(1, now, now);
        meter.loaded(1, now + Duration::from_millis(200));
        meter.restart(1, now + Duration::from_millis(350));
        meter.buffer(1, true, now + Duration::from_millis(400));
        meter.buffer(1, true, now + Duration::from_millis(401));
        let active = meter.snapshot(now + Duration::from_millis(520));
        assert_eq!(active.buffer_events, 1);
        assert_eq!(active.buffer_current_ms, Some(120));
        assert_eq!(active.buffer_total_ms, 0);
        meter.buffer(1, false, now + Duration::from_millis(650));
        meter.ended(1, now + Duration::from_millis(900));
        meter.released(12);
        let final_state = meter.snapshot(now + Duration::from_millis(910));
        assert_eq!(final_state.last_load_ms, Some(200));
        assert_eq!(final_state.last_playback_restart_ms, Some(350));
        assert_eq!(final_state.buffer_total_ms, 250);
        assert_eq!(final_state.buffer_current_ms, None);
        assert_eq!(final_state.playback_ended, 1);
        assert_eq!(final_state.libmpv_releases, 1);
        assert_eq!(final_state.last_release_ms, Some(12));
        assert_eq!(final_state.active_session_id, None);
    }

    #[test]
    fn old_session_events_do_not_contaminate_new_session() {
        let now = Instant::now();
        let mut meter = RuntimeHealthMeter::default();
        meter.open(1, now, now);
        meter.buffer(1, true, now);
        meter.open(
            2,
            now + Duration::from_millis(100),
            now + Duration::from_millis(100),
        );
        meter.loaded(1, now + Duration::from_millis(110));
        meter.buffer(1, true, now + Duration::from_millis(130));
        meter.ended(1, now + Duration::from_millis(200));
        meter.failed(1, now + Duration::from_millis(300));
        assert_eq!(
            meter
                .snapshot(now + Duration::from_millis(350))
                .active_session_id,
            Some(2)
        );
        assert_eq!(
            meter
                .snapshot(now + Duration::from_millis(350))
                .buffer_total_ms,
            100
        );
        assert_eq!(
            meter
                .snapshot(now + Duration::from_millis(350))
                .playback_failed,
            0
        );
        meter.stopped(2, now + Duration::from_millis(400));
        meter.stopped(2, now + Duration::from_millis(500));
        assert_eq!(
            meter
                .snapshot(now + Duration::from_millis(550))
                .playback_stopped,
            1
        );
    }

    #[test]
    fn diagnostics_do_not_expose_urls_paths_or_error_details() {
        let now = Instant::now();
        let mut meter = RuntimeHealthMeter::default();
        meter.open(1, now, now);
        meter.failed(1, now);
        let json = serde_json::to_string(&meter.snapshot(now)).unwrap();
        for forbidden in [
            "filename",
            "filePath",
            "source",
            "url",
            "errorMessage",
            "token",
        ] {
            assert!(!json.contains(forbidden));
        }
        assert!(json.contains("\"playbackFailed\":1"));
        assert!(json.contains("\"lastOutcome\":\"error\""));
    }

    #[test]
    fn snapshot_counters_are_saturating_and_no_double_finish() {
        let now = Instant::now();
        let mut meter = RuntimeHealthMeter::default();
        meter.open(1, now, now);
        meter.buffer(1, true, now);
        meter.closed(now + Duration::from_millis(400));
        meter.closed(now + Duration::from_millis(500));
        assert_eq!(meter.snapshot(now).buffer_total_ms, 400);
        assert_eq!(meter.snapshot(now).last_outcome, Some("closed"));
        assert_eq!(meter.snapshot(now).buffer_current_ms, None);
    }
}
