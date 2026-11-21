use super::controls::PlayerControl;
use super::engine::{Engine, Observation};
use super::state::{Failure, PlaybackState, PlayerSnapshot, PlayerState};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use tracing::{error, info};

enum Command {
    Open {
        id: u64,
        path: PathBuf,
        accepted: Instant,
    },
    Stop(Option<u64>),
    Control {
        id: u64,
        request: PlayerControl,
        reply: mpsc::SyncSender<Result<(), String>>,
    },
    Close,
}

#[derive(Default)]
struct Shared {
    player: PlayerState,
    closing: bool,
}

pub struct PlayerService {
    state: Arc<Mutex<Shared>>,
    tx: Sender<Command>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl PlayerService {
    pub fn new(host: isize) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Shared::default()));
        let shared = Arc::clone(&state);
        let worker = thread::Builder::new()
            .name("libmpv-player".into())
            .spawn(move || run(host, rx, shared))?;
        Ok(Self {
            state,
            tx,
            worker: Mutex::new(Some(worker)),
        })
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        self.state.lock().unwrap().player.snapshot()
    }

    pub fn open(&self, path: PathBuf) -> Result<u64, String> {
        let mut shared = self.state.lock().unwrap();
        if shared.closing {
            return Err("Window is closing".into());
        }
        let filename = path
            .file_name()
            .ok_or("Missing filename")?
            .to_string_lossy()
            .into_owned();
        let id = shared.player.begin(filename);
        let accepted = Instant::now();
        if self.tx.send(Command::Open { id, path, accepted }).is_err() {
            let failure = Failure {
                stage: "dispatch",
                code: "worker_exited".into(),
                message: "Playback worker exited".into(),
            };
            shared
                .player
                .update(id, PlaybackState::Error, Some(failure));
            return Err("Playback worker exited".into());
        }
        Ok(id)
    }

    pub fn stop(&self) -> Result<(), String> {
        let shared = self.state.lock().unwrap();
        if shared.closing {
            return Err("Window is closing".into());
        }
        self.tx
            .send(Command::Stop(shared.player.snapshot().session_id))
            .map_err(|_| "Playback worker exited".into())
    }

    /// Enqueue a validated playback control and wait for a native libmpv reply.
    /// Rust remains the only owner of the player; the Tauri command must call
    /// this on a blocking task, never from the UI/event-loop thread.
    pub fn control(&self, request: PlayerControl) -> Result<(), String> {
        let request = request.validate().map_err(str::to_owned)?;
        let shared = self.state.lock().unwrap();
        if shared.closing {
            return Err("Window is closing".into());
        }
        let id = shared.player.snapshot().session_id.ok_or("No active playback session")?;
        let (reply, receiver) = mpsc::sync_channel(1);
        self.tx.send(Command::Control { id, request, reply })
            .map_err(|_| "Playback worker exited")?;
        drop(shared);
        receiver.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "Playback control timed out".to_owned())?
    }

    pub fn begin_close(&self) -> bool {
        let mut shared = self.state.lock().unwrap();
        if shared.closing {
            return false;
        }
        shared.closing = true;
        let _ = self.tx.send(Command::Close);
        true
    }

    pub fn join(&self) -> thread::Result<()> {
        if let Some(worker) = self.worker.lock().unwrap().take() {
            worker.join()?;
        }
        Ok(())
    }
}

fn stop_targets_active_session(requested: Option<u64>, current: Option<u64>) -> bool {
    requested.is_some() && requested == current
}

fn release(engine: &mut Option<Engine>) {
    if let Some(current) = engine.take() {
        let id = current.session;
        let started = Instant::now();
        drop(current);
        info!(
            session_id = id,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "libmpv released"
        );
    }
}

fn failed(state: &Mutex<Shared>, id: u64, failure: Failure) {
    error!(session_id=id, stage=failure.stage, code=%failure.code, message=%failure.message, "playback failure");
    state
        .lock()
        .unwrap()
        .player
        .update(id, PlaybackState::Error, Some(failure));
}

fn run(host: isize, rx: Receiver<Command>, state: Arc<Mutex<Shared>>) {
    let mut engine: Option<Engine> = None;
    loop {
        let cmd = if engine.is_some() {
            match rx.try_recv() {
                Ok(v) => Some(v),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(v) => Some(v),
                Err(_) => break,
            }
        };
        match cmd {
            Some(Command::Open { id, path, accepted }) => {
                {
                    let shared = state.lock().unwrap();
                    if shared.closing || !shared.player.current(id) {
                        continue;
                    }
                }
                release(&mut engine);
                match Engine::load(host, &path, id, accepted) {
                    Ok(player) => {
                        info!(
                            session_id = id,
                            elapsed_ms = accepted.elapsed().as_millis() as u64,
                            "loadfile accepted"
                        );
                        engine = Some(player);
                    }
                    Err(e) => failed(&state, id, e),
                }
            }
            Some(Command::Stop(id)) => {
                // A Stop can be queued before a subsequent Open. Never release
                // the newly selected session's engine for a stale Stop.
                let current_id = state.lock().unwrap().player.snapshot().session_id;
                if !stop_targets_active_session(id, current_id) {
                    info!(session_id=?id, current_session_id=?current_id, "ignored stale stop");
                    continue;
                }
                release(&mut engine);
                state.lock().unwrap().player.clear(id);
                info!(session_id=?id, "stopped");
            }
            Some(Command::Control { id, request, reply }) => {
                let current_id = state.lock().unwrap().player.snapshot().session_id;
                if current_id != Some(id) || engine.as_ref().map(|e| e.session) != Some(id) {
                    let _ = reply.send(Err("Playback session changed or is not ready".into()));
                    continue;
                }
                let result = engine
                    .as_mut()
                    .unwrap()
                    .control(request)
                    .map_err(|failure| failure.message);
                if result.is_ok() {
                    let mut shared = state.lock().unwrap();
                    match request {
                        PlayerControl::Pause(true) => {
                            shared.player.update(id, PlaybackState::Paused, None);
                        }
                        PlayerControl::Pause(false) => {
                            shared.player.update(id, PlaybackState::Playing, None);
                        }
                        _ => {}
                    }
                }
                let _ = reply.send(result);
            }
            Some(Command::Close) => break,
            None => {
                let current = engine.as_mut().unwrap();
                let id = current.session;
                let event = current.poll();
                let ms = current.opened.elapsed().as_millis() as u64;
                match event {
                    Ok(Some(Observation::Loaded)) => {
                        info!(session_id = id, elapsed_ms = ms, "file-loaded")
                    }
                    Ok(Some(Observation::Restart)) => {
                        info!(session_id = id, elapsed_ms = ms, "playback-restart");
                        let mut shared = state.lock().unwrap();
                        if shared.player.snapshot().state != PlaybackState::Paused {
                            shared.player.update(id, PlaybackState::Playing, None);
                        }
                    }
                    Ok(Some(Observation::End)) => {
                        release(&mut engine);
                        state
                            .lock()
                            .unwrap()
                            .player
                            .update(id, PlaybackState::Ended, None);
                    }
                    Err(e) => {
                        release(&mut engine);
                        failed(&state, id, e);
                    }
                    Ok(None) => {}
                }
            }
        }
    }
    let started = Instant::now();
    release(&mut engine);
    info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        "player worker exited"
    );
}

#[cfg(test)]
mod tests {
    use super::stop_targets_active_session;

    #[test]
    fn late_stop_never_targets_replacement_session() {
        assert!(stop_targets_active_session(Some(2), Some(2)));
        assert!(!stop_targets_active_session(Some(1), Some(2)));
        assert!(!stop_targets_active_session(Some(1), None));
        assert!(!stop_targets_active_session(None, Some(2)));
        assert!(!stop_targets_active_session(None, None));
    }
}
