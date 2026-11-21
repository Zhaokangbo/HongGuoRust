use crate::player::{
    controls::PlayerControl, service::PlayerService, state::PlayerSnapshot, window,
};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, State, WebviewWindow};

#[tauri::command]
pub fn get_snapshot(player: State<'_, PlayerService>) -> PlayerSnapshot {
    static FIRST: AtomicBool = AtomicBool::new(true);
    let snapshot = player.snapshot();
    if FIRST.swap(false, Ordering::Relaxed) {
        tracing::info!(revision=snapshot.revision, state=?snapshot.state, "first snapshot IPC");
    }
    snapshot
}

#[tauri::command]
pub async fn open_local(window: WebviewWindow) -> Result<Option<u64>, String> {
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || {
        window
            .state::<window::Picker>()
            .choose(owner)?
            .map(|path| window.state::<PlayerService>().open(path))
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn stop(player: State<'_, PlayerService>) -> Result<(), String> {
    player.stop()
}

async fn dispatch_control(window: WebviewWindow, request: PlayerControl) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || window.state::<PlayerService>().control(request))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn set_paused(window: WebviewWindow, paused: bool) -> Result<(), String> {
    dispatch_control(window, PlayerControl::Pause(paused)).await
}

#[tauri::command]
pub async fn seek_to(window: WebviewWindow, seconds: f64) -> Result<(), String> {
    dispatch_control(window, PlayerControl::SeekAbsolute(seconds)).await
}

#[tauri::command]
pub async fn set_rate(window: WebviewWindow, rate: f64) -> Result<(), String> {
    dispatch_control(window, PlayerControl::SetRate(rate)).await
}

#[tauri::command]
pub async fn set_volume(window: WebviewWindow, volume: f64) -> Result<(), String> {
    dispatch_control(window, PlayerControl::SetVolume(volume)).await
}
