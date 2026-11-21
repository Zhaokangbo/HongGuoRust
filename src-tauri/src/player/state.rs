use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum PlaybackState {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
    Ended,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct Failure {
    pub stage: &'static str,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub state: PlaybackState,
    pub revision: u64,
    pub session_id: Option<u64>,
    pub app_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Failure>,
}

#[derive(Default)]
pub struct PlayerState {
    sequence: u64,
    revision: u64,
    session: Option<u64>,
    state: PlaybackState,
    filename: Option<String>,
    error: Option<Failure>,
}

impl PlayerState {
    pub fn snapshot(&self) -> PlayerSnapshot {
        PlayerSnapshot {
            state: self.state,
            revision: self.revision,
            session_id: self.session,
            app_version: env!("CARGO_PKG_VERSION"),
            filename: self.filename.clone(),
            error: self.error.clone(),
        }
    }

    pub fn begin(&mut self, filename: String) -> u64 {
        self.sequence += 1;
        self.session = Some(self.sequence);
        self.filename = Some(filename);
        self.error = None;
        self.state = PlaybackState::Loading;
        self.revision += 1;
        self.sequence
    }

    pub fn current(&self, id: u64) -> bool {
        self.session == Some(id)
    }

    pub fn update(&mut self, id: u64, next: PlaybackState, error: Option<Failure>) {
        if self.current(id)
            && matches!(
                self.state,
                PlaybackState::Loading | PlaybackState::Playing | PlaybackState::Paused
            )
            && self.state != next
        {
            self.state = next;
            self.error = error;
            self.revision += 1;
        }
    }

    pub fn clear(&mut self, id: Option<u64>) {
        if self.session.is_some() && self.session == id {
            self.state = PlaybackState::Idle;
            self.session = None;
            self.filename = None;
            self.error = None;
            self.revision += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_preserves_the_m1_ipc_contract() {
        assert_eq!(
            serde_json::to_value(PlayerState::default().snapshot()).unwrap(),
            serde_json::json!({"state":"Idle","revision":0,"sessionId":null,"appVersion":env!("CARGO_PKG_VERSION")})
        );
    }

    #[test]
    fn replacement_rejects_late_restart_end_error_and_stop() {
        let mut s = PlayerState::default();
        let a = s.begin("A.mp4".into());
        let b = s.begin("B.mp4".into());
        assert!(b > a);
        for state in [
            PlaybackState::Playing,
            PlaybackState::Ended,
            PlaybackState::Error,
        ] {
            s.update(a, state, None);
        }
        s.clear(Some(a));
        assert_eq!(s.snapshot().revision, 2);
        assert_eq!(s.snapshot().state, PlaybackState::Loading);
        s.update(b, PlaybackState::Playing, None);
        s.update(b, PlaybackState::Ended, None);
        s.update(b, PlaybackState::Playing, None);
        assert_eq!(s.snapshot().state, PlaybackState::Ended);
        s.clear(Some(b));
        assert_eq!(s.snapshot().state, PlaybackState::Idle);
        assert!(s.begin("C.mp4".into()) > b);
    }
}
