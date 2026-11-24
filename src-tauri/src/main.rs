#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod commands;
mod local_library;
mod player;

use catalog::progress::ProviderProgressStore;
use catalog::{CatalogService, FavoritesStore, OwnedManifestStore, WebDavStore};
use local_library::LocalLibrary;
use player::{
    overlay, preferences::PreferencesStore, preview::PosterCache, service::PlayerService, window,
};
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{Manager, WindowEvent};

struct VideoHost(isize);
pub struct StartupMedia(Mutex<Option<PathBuf>>);

/// Extract a valid local video file passed via Windows "Open with" or command-line args.
fn extract_startup_media() -> Option<PathBuf> {
    for raw in std::env::args_os().skip(1) {
        let text = raw.to_string_lossy();
        let trimmed = text.trim().trim_matches('"');
        if trimmed.is_empty() || trimmed.starts_with('-') {
            continue;
        }
        // Strip Windows verbatim prefix `\\?\C:\...` while still rejecting UNC `\\?\UNC\...`.
        let normalized = if let Some(rest) = trimmed.strip_prefix(r"\\?\") {
            if rest.as_bytes().get(1) == Some(&b':') {
                rest
            } else {
                continue;
            }
        } else {
            trimmed
        };
        let candidate = PathBuf::from(normalized);
        let resolved = if candidate.is_absolute() {
            candidate
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(candidate)
        } else {
            continue;
        };
        if player::engine::local_video(&resolved).is_ok() {
            return Some(resolved);
        }
    }
    None
}

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
    let webdav = Arc::new(WebDavStore::new());
    let catalog = Arc::new(CatalogService::with_webdav(
        Arc::clone(&library),
        Arc::clone(&manifest),
        Arc::clone(&webdav),
    ));
    let progress = Arc::new(ProviderProgressStore::new());
    let favorites = Arc::new(FavoritesStore::new());
    let preferences = Arc::new(PreferencesStore::new());
    let posters = Arc::new(PosterCache::new());
    let startup_media = StartupMedia(Mutex::new(extract_startup_media()));
    tauri::Builder::default()
        .manage(catalog)
        .manage(library)
        .manage(progress)
        .manage(favorites)
        .manage(preferences)
        .manage(posters)
        .manage(manifest)
        .manage(webdav)
        .manage(startup_media)
        .manage(window::Picker::default())
        .manage(overlay::PlayerUi::default())
        .setup(|app| {
            let win = app
                .get_webview_window(overlay::MAIN)
                .ok_or("Missing main window")?;
            window::apply_dark_titlebar(&win);
            let host = window::create(&win)?;
            // Start hidden on the browse surface so there is no native HWND flash.
            window::set_video_visible(host, false);
            app.manage(VideoHost(host));
            overlay::create(app.handle(), &win)?;
            let library = Arc::clone(app.state::<Arc<LocalLibrary>>().inner());
            let progress = Arc::clone(app.state::<Arc<ProviderProgressStore>>().inner());
            let preferences = Arc::clone(app.state::<Arc<PreferencesStore>>().inner());
            app.manage(PlayerService::new(host, library, progress, preferences)?);
            tracing::info!("native host, overlay and service ready");
            Ok(())
        })
        .on_window_event(|win, event| {
            if win.label() == overlay::OVERLAY {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    // Never destroy the overlay independently; forward to main window close.
                    api.prevent_close();
                    if let Some(main) = win.get_webview_window(overlay::MAIN) {
                        let _ = main.close();
                    }
                }
                return;
            }
            if win.label() != overlay::MAIN {
                return;
            }
            match event {
                WindowEvent::Moved(_) | WindowEvent::Focused(true) => {
                    if let (Some(ui), Some(main)) = (
                        win.try_state::<overlay::PlayerUi>(),
                        win.get_webview_window(overlay::MAIN),
                    ) {
                        if ui.is_active() {
                            overlay::sync(&main);
                        }
                    }
                }
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    if let (Some(host), Ok(hwnd)) = (win.try_state::<VideoHost>(), win.hwnd()) {
                        window::layout(hwnd.0, host.0);
                    }
                    if let (Some(ui), Some(main)) = (
                        win.try_state::<overlay::PlayerUi>(),
                        win.get_webview_window(overlay::MAIN),
                    ) {
                        if ui.is_active() {
                            overlay::sync(&main);
                        }
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    if let Some(ov) = win.get_webview_window(overlay::OVERLAY) {
                        let _ = ov.hide();
                    }
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
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_runtime_health,
            commands::catalog_search,
            commands::catalog_sources,
            commands::catalog_search_page,
            commands::catalog_cancel_request,
            commands::catalog_import_manifest,
            commands::catalog_clear_manifest,
            commands::catalog_subscribe_manifest,
            commands::catalog_refresh_manifest,
            commands::catalog_manifest_info,
            commands::webdav_get_status,
            commands::webdav_configure,
            commands::webdav_sync,
            commands::webdav_clear,
            commands::catalog_favorites_list,
            commands::catalog_favorite_set,
            commands::catalog_detail,
            commands::catalog_play_episode,
            commands::catalog_episode_qualities,
            commands::catalog_episode_danmaku,
            commands::catalog_episode_progress,
            commands::catalog_recently_watched,
            commands::library_list,
            commands::library_add_local,
            commands::library_remove,
            commands::library_play,
            commands::library_clear_history,
            commands::library_set_favorite,
            commands::local_poster_frame,
            commands::catalog_poster_frame,
            commands::set_catalog_visible,
            commands::player_present,
            commands::player_update_context,
            commands::player_dismiss,
            commands::get_player_context,
            commands::open_startup_media,
            commands::open_local,
            commands::stop,
            commands::set_paused,
            commands::seek_to,
            commands::player_preview_frame,
            commands::player_tracks,
            commands::player_select_track,
            commands::set_rate,
            commands::set_volume,
            commands::get_player_preferences,
            commands::set_player_auto_next,
            commands::open_https,
            commands::get_fullscreen,
            commands::set_fullscreen
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HongGuoRust");
}
