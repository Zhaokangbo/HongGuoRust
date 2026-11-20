#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod player;

use player::state::PlayerState;

fn main() {
    tauri::Builder::default()
        .manage(PlayerState::default())
        .invoke_handler(tauri::generate_handler![commands::get_snapshot])
        .run(tauri::generate_context!())
        .expect("failed to run HongGuoRust");
}
