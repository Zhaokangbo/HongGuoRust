use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum PlaybackState {
    #[default]
    Idle,
}

#[derive(Default)]
pub struct PlayerState {
    state: PlaybackState,
    revision: u64,
    session_id: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    state: PlaybackState,
    revision: u64,
    session_id: Option<u64>,
    app_version: String,
}

impl PlayerState {
    pub fn snapshot(&self, app_version: String) -> PlayerSnapshot {
        PlayerSnapshot {
            state: self.state,
            revision: self.revision,
            session_id: self.session_id,
            app_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_player_exposes_idle_without_a_session() {
        let player = PlayerState::default();
        let expected = json!({
            "state": "Idle",
            "revision": 0,
            "sessionId": null,
            "appVersion": "0.1.0"
        });
        for _ in 0..2 {
            let snapshot = player.snapshot("0.1.0".into());
            assert_eq!(serde_json::to_value(snapshot).unwrap(), expected);
        }
    }
}
