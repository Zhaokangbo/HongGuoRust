#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod player;

#[cfg(all(windows, feature = "mpv-probe"))]
mod probe;

#[cfg(all(windows, feature = "mpv-probe"))]
fn main() {
    probe::run();
}

#[cfg(not(all(windows, feature = "mpv-probe")))]
use player::state::PlayerState;

#[cfg(not(all(windows, feature = "mpv-probe")))]
fn main() {
    tauri::Builder::default()
        .manage(PlayerState::default())
        .invoke_handler(tauri::generate_handler![commands::get_snapshot])
        .run(tauri::generate_context!())
        .expect("failed to run HongGuoRust");
}
