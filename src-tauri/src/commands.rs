use crate::player::state::{PlayerSnapshot, PlayerState};
use tauri::State;

#[tauri::command]
pub fn get_snapshot(player: State<'_, PlayerState>) -> PlayerSnapshot {
    player.snapshot(env!("CARGO_PKG_VERSION").to_owned())
}
