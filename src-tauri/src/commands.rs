use crate::player::{service::PlayerService, state::PlayerSnapshot, window};
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
