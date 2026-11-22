use super::controls::PlayerControl;
use super::engine::{Engine, Observation};
use super::history::{finish, HistorySession};
use super::preview::{PreviewFrame, PreviewService};
use super::provider_history::{finish_provider, ProviderHistorySession};
use super::source::MediaSource;
use super::state::{Failure, PlaybackState, PlayerSnapshot, PlayerState};
use crate::catalog::progress::ProviderProgressStore;
use crate::local_library::LocalLibrary;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use tracing::{error, info};

struct ProviderPlayback {
    series_id: String,
    episode_id: String,
    source: String,
    resume_at: Option<f64>,
}

enum Command {
    Open {
        id: u64,
        source: MediaSource,
        accepted: Instant,
        library_id: Option<u64>,
        resume_at: Option<f64>,
        provider: Option<ProviderPlayback>,
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
    preview_source: Option<(u64, MediaSource)>,
}

pub struct PlayerService {
    state: Arc<Mutex<Shared>>,
    tx: Sender<Command>,
    worker: Mutex<Option<JoinHandle<()>>>,
    library: Arc<LocalLibrary>,
    provider_progress: Arc<ProviderProgressStore>,
    preview: PreviewService,
}

impl PlayerService {
    pub fn new(
        host: isize,
        library: Arc<LocalLibrary>,
        provider_progress: Arc<ProviderProgressStore>,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Shared::default()));
        let shared = Arc::clone(&state);
        let worker_library = Arc::clone(&library);
        let worker_progress = Arc::clone(&provider_progress);
        let worker = thread::Builder::new()
            .name("libmpv-player".into())
            .spawn(move || run(host, rx, shared, worker_library, worker_progress))?;
        Ok(Self {
            state,
            tx,
            worker: Mutex::new(Some(worker)),
            library,
            provider_progress,
            preview: PreviewService::default(),
        })
    }

    pub fn snapshot(&self) -> PlayerSnapshot {
        self.state.lock().unwrap().player.snapshot()
    }

    pub fn open(&self, path: PathBuf) -> Result<u64, String> {
        self.open_source(MediaSource::Local(path), None, None, None)
    }

    pub fn open_library(&self, id: u64) -> Result<u64, String> {
        let path = self.library.resolve_path(id)?;
        let resume_at = self.library.resume_point(id)?;
        self.open_source(MediaSource::Local(path), Some(id), resume_at, None)
    }

    pub fn open_https(&self, url: String) -> Result<u64, String> {
        self.open_source(MediaSource::https(url)?, None, None, None)
    }

    /// Called only after CatalogService validated the selected episode and URL.
    pub fn open_catalog_https(
        &self,
        url: String,
        series_id: String,
        episode_id: String,
    ) -> Result<u64, String> {
        let source = MediaSource::https(url.clone())?;
        let resume_at = self
            .provider_progress
            .resume_point(&series_id, &episode_id, &url)?;
        self.open_source(
            source,
            None,
            None,
            Some(ProviderPlayback {
                series_id,
                episode_id,
                source: url,
                resume_at,
            }),
        )
    }

    fn open_source(
        &self,
        source: MediaSource,
        library_id: Option<u64>,
        resume_at: Option<f64>,
        provider: Option<ProviderPlayback>,
    ) -> Result<u64, String> {
        let mut shared = self.state.lock().unwrap();
        if shared.closing {
            return Err("Window is closing".into());
        }
        let filename = source.label();
        let kind = source.kind();
        let id = shared.player.begin(filename);
        self.preview.cancel();
        shared.preview_source = Some((id, source.clone()));
        let accepted = Instant::now();
        if self
            .tx
            .send(Command::Open {
                id,
                source,
                accepted,
                library_id,
                resume_at,
                provider,
            })
            .is_err()
        {
            shared.preview_source = None;
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
        info!(session_id = id, source_kind = kind, "open source accepted");
        Ok(id)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.preview.cancel();
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
        let id = shared
            .player
            .snapshot()
            .session_id
            .ok_or("No active playback session")?;
        let (reply, receiver) = mpsc::sync_channel(1);
        self.tx
            .send(Command::Control { id, request, reply })
            .map_err(|_| "Playback worker exited")?;
        drop(shared);
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "Playback control timed out".to_owned())?
    }

    /// The preview uses its own Mpv image-output handle on a blocking task.
    /// Only the active native session can request it; source URLs never enter IPC.
    pub fn preview_frame(
        &self,
        session_id: u64,
        seconds: f64,
    ) -> Result<Option<PreviewFrame>, String> {
        let source = {
            let shared = self.state.lock().unwrap();
            if shared.closing {
                return Ok(None);
            }
            let snapshot = shared.player.snapshot();
            if snapshot.session_id != Some(session_id)
                || !matches!(
                    snapshot.state,
                    PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Buffering
                )
            {
                return Ok(None);
            }
            match shared.preview_source.as_ref() {
                Some((id, source)) if *id == session_id => source.clone(),
                _ => return Ok(None),
            }
        };
        let frame = self.preview.frame(session_id, seconds, &source)?;
        let current = self.state.lock().unwrap();
        if current.closing
            || current.player.snapshot().session_id != Some(session_id)
            || !matches!(
                current.player.snapshot().state,
                PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Buffering
            )
        {
            return Ok(None);
        }
        Ok(frame)
    }

    pub fn begin_close(&self) -> bool {
        self.preview.cancel();
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

fn end_cache_interval(since: &mut Option<(u64, Instant)>, reason: &'static str) {
    if let Some((session_id, started)) = since.take() {
        info!(
            session_id,
            elapsed_ms = started.elapsed().as_millis() as u64,
            reason,
            "cache-wait interval ended"
        );
    }
}

fn run(
    host: isize,
    rx: Receiver<Command>,
    state: Arc<Mutex<Shared>>,
    library: Arc<LocalLibrary>,
    provider_progress: Arc<ProviderProgressStore>,
) {
    let mut engine: Option<Engine> = None;
    let mut cache_since: Option<(u64, Instant)> = None;
    let mut history: Option<HistorySession> = None;
    let mut provider_history: Option<ProviderHistorySession> = None;
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
            Some(Command::Open {
                id,
                source,
                accepted,
                library_id,
                resume_at,
                provider,
            }) => {
                {
                    let shared = state.lock().unwrap();
                    if shared.closing || !shared.player.current(id) {
                        continue;
                    }
                }
                end_cache_interval(&mut cache_since, "replaced");
                finish(&mut history, &library, false);
                finish_provider(&mut provider_history, &provider_progress, false);
                release(&mut engine);
                match Engine::load(host, &source, id, accepted) {
                    Ok(player) => {
                        info!(
                            session_id = id,
                            elapsed_ms = accepted.elapsed().as_millis() as u64,
                            "loadfile accepted"
                        );
                        engine = Some(player);
                        history = library_id.map(|key| HistorySession::new(id, key, resume_at));
                        provider_history = provider.map(|key| {
                            ProviderHistorySession::new(
                                id,
                                key.series_id,
                                key.episode_id,
                                key.source,
                                key.resume_at,
                            )
                        });
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
                end_cache_interval(&mut cache_since, "stopped");
                finish(&mut history, &library, false);
                finish_provider(&mut provider_history, &provider_progress, false);
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
                            shared.player.set_user_paused(id, true);
                        }
                        PlayerControl::Pause(false) => {
                            shared.player.set_user_paused(id, false);
                        }
                        _ => {}
                    }
                }
                if result.is_ok() {
                    if let Some(active) = history.as_mut().filter(|h| h.session == id) {
                        match request {
                            PlayerControl::SeekAbsolute(sec) => active.seeked(&library, sec),
                            PlayerControl::Pause(true) => active.save(&library, false),
                            _ => {}
                        }
                    }
                    if let Some(active) = provider_history.as_mut().filter(|h| h.session == id) {
                        match request {
                            PlayerControl::SeekAbsolute(sec) => {
                                active.seeked(&provider_progress, sec)
                            }
                            PlayerControl::Pause(true) => active.save(&provider_progress, false),
                            _ => {}
                        }
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
                        info!(session_id = id, elapsed_ms = ms, "file-loaded");
                        if let Some(active) = history.as_mut().filter(|h| h.session == id) {
                            if let Some(sec) = active.resume_at.take() {
                                if let Err(e) = current.control(PlayerControl::SeekAbsolute(sec)) {
                                    tracing::warn!(session_id = id, error = %e.message, "resume seek failed");
                                } else {
                                    active.seeked(&library, sec);
                                    info!(
                                        session_id = id,
                                        position_sec = sec,
                                        "resume seek accepted"
                                    );
                                }
                            }
                        }
                        if let Some(active) = provider_history.as_mut().filter(|h| h.session == id)
                        {
                            if let Some(sec) = active.resume_at.take() {
                                if let Err(e) = current.control(PlayerControl::SeekAbsolute(sec)) {
                                    tracing::warn!(session_id = id, error = %e.message, "provider resume seek failed");
                                } else {
                                    active.seeked(&provider_progress, sec);
                                    info!(
                                        session_id = id,
                                        position_sec = sec,
                                        "provider resume seek accepted"
                                    );
                                }
                            }
                        }
                    }
                    Ok(Some(Observation::Restart)) => {
                        info!(session_id = id, elapsed_ms = ms, "playback-restart");
                        state.lock().unwrap().player.mark_started(id);
                        if let Some(active) = history.as_mut().filter(|h| h.session == id) {
                            active.started(&library);
                        }
                        if let Some(active) = provider_history.as_mut().filter(|h| h.session == id)
                        {
                            active.started(&provider_progress);
                        }
                    }
                    Ok(Some(Observation::CacheWaiting(waiting))) => {
                        if waiting {
                            if cache_since.is_none() {
                                cache_since = Some((id, Instant::now()));
                                info!(session_id = id, "cache-wait interval started");
                            }
                        } else {
                            end_cache_interval(&mut cache_since, "recovered");
                        }
                        let mut shared = state.lock().unwrap();
                        let old = shared.player.snapshot().state;
                        shared.player.set_cache_waiting(id, waiting);
                        let new_state = shared.player.snapshot().state;
                        if old != new_state {
                            info!(
                                session_id = id,
                                waiting_for_cache = waiting,
                                old_state = ?old,
                                new_state = ?new_state,
                                "cache-driven playback transition"
                            );
                        }
                    }
                    Ok(Some(Observation::End)) => {
                        end_cache_interval(&mut cache_since, "ended");
                        finish(&mut history, &library, true);
                        finish_provider(&mut provider_history, &provider_progress, true);
                        release(&mut engine);
                        state
                            .lock()
                            .unwrap()
                            .player
                            .update(id, PlaybackState::Ended, None);
                    }
                    Err(e) => {
                        end_cache_interval(&mut cache_since, "error");
                        finish(&mut history, &library, false);
                        finish_provider(&mut provider_history, &provider_progress, false);
                        release(&mut engine);
                        failed(&state, id, e);
                    }
                    Ok(Some(Observation::Progress { position, duration })) => {
                        state
                            .lock()
                            .unwrap()
                            .player
                            .progress(id, position, duration);
                        if let Some(active) = history.as_mut().filter(|h| h.session == id) {
                            active.progress(&library, position, duration);
                        }
                        if let Some(active) = provider_history.as_mut().filter(|h| h.session == id)
                        {
                            active.progress(&provider_progress, position, duration);
                        }
                    }
                    Ok(None) => {}
                }
            }
        }
    }
    let started = Instant::now();
    end_cache_interval(&mut cache_since, "closed");
    finish(&mut history, &library, false);
    finish_provider(&mut provider_history, &provider_progress, false);
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
