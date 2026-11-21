#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod commands;
mod local_library;
mod player;

use catalog::{CatalogService, OwnedManifestStore};
use local_library::LocalLibrary;
use player::{service::PlayerService, window};
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Manager, WindowEvent};

struct VideoHost(isize);

fn main() {
    let dir = std::env::var_os("HONGGUORUST_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("HongGuoRust")
                .join("logs")
        });
    let logger = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false);
    // A per-process log keeps simultaneous launches independent.
    match fs::create_dir_all(&dir)
        .and_then(|_| File::create(dir.join(format!("player-{}.log", std::process::id()))))
    {
        Ok(file) => logger.with_writer(Mutex::new(file)).init(),
        Err(_) => logger.with_writer(std::io::stderr).init(),
    }
    let library = Arc::new(LocalLibrary::new());
    let manifest = Arc::new(OwnedManifestStore::new());
    let catalog = CatalogService::new(Arc::clone(&library), Arc::clone(&manifest));
    tauri::Builder::default()
        .manage(catalog)
        .manage(library)
        .manage(manifest)
        .manage(window::Picker::default())
        .setup(|app| {
            let win = app
                .get_webview_window("main")
                .ok_or("Missing main window")?;
            let host = window::create(&win)?;
            app.manage(VideoHost(host));
            let library = Arc::clone(app.state::<Arc<LocalLibrary>>().inner());
            app.manage(PlayerService::new(host, library)?);
            tracing::info!("native host and service ready");
            Ok(())
        })
        .on_window_event(|win, event| match event {
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let (Some(host), Ok(hwnd), Ok(scale)) =
                    (win.try_state::<VideoHost>(), win.hwnd(), win.scale_factor())
                {
                    window::layout(hwnd.0, host.0, scale, win.is_fullscreen().unwrap_or(false));
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                if let Some(service) = win.try_state::<PlayerService>() {
                    api.prevent_close();
                    if service.begin_close() {
                        let requested = Instant::now();
                        let win = win.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            if let Ok(hwnd) = win.hwnd() {
                                win.state::<window::Picker>().shutdown(hwnd.0 as isize);
                            }
                            if win.state::<PlayerService>().join().is_err() {
                                tracing::error!("player worker panicked");
                            }
                            tracing::info!(
                                elapsed_ms = requested.elapsed().as_millis() as u64,
                                "worker joined before window destroy"
                            );
                            if let Err(e) = win.destroy() {
                                tracing::error!(error=%e, "window destroy failed");
                            }
                        });
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::catalog_search,
            commands::catalog_sources,
            commands::catalog_search_page,
            commands::catalog_cancel_request,
            commands::catalog_import_manifest,
            commands::catalog_clear_manifest,
            commands::catalog_detail,
            commands::catalog_play_episode,
            commands::library_list,
            commands::library_add_local,
            commands::library_remove,
            commands::library_play,
            commands::library_clear_history,
            commands::library_set_favorite,
            commands::set_catalog_visible,
            commands::open_local,
            commands::stop,
            commands::set_paused,
            commands::seek_to,
            commands::set_rate,
            commands::set_volume,
            commands::open_https,
            commands::get_fullscreen,
            commands::set_fullscreen
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HongGuoRust");
}
