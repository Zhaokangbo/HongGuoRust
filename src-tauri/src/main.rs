#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod player;

use player::{service::PlayerService, window};
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Mutex;
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
    tauri::Builder::default()
        .manage(window::Picker::default())
        .setup(|app| {
            let win = app
                .get_webview_window("main")
                .ok_or("Missing main window")?;
            let host = window::create(&win)?;
            app.manage(VideoHost(host));
            app.manage(PlayerService::new(host)?);
            tracing::info!("native host and service ready");
            Ok(())
        })
        .on_window_event(|win, event| match event {
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let (Some(host), Ok(hwnd), Ok(scale)) =
                    (win.try_state::<VideoHost>(), win.hwnd(), win.scale_factor())
                {
                    window::layout(hwnd.0, host.0, scale);
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
            commands::open_local,
            commands::stop
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HongGuoRust");
}
