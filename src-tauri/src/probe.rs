//! M2-A build only. Each invocation owns a single libmpv session at a time.
use crate::player::{engine, window};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tauri::{Manager, State, WebviewWindow, WindowEvent};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    probe: bool,
    state: &'static str,
    revision: u64,
    session_id: Option<u64>,
    app_version: &'static str,
    filename: Option<String>,
    error: Option<String>,
}

struct Probe {
    host: isize,
    snapshot: Arc<Mutex<Snapshot>>,
    cancelled: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    closing: AtomicBool,
}

impl Probe {
    fn start(&self, file: PathBuf) -> Result<(), String> {
        engine::local_mp4(&file)?;
        let mut worker = self.worker.lock().unwrap();
        if self.closing.load(Ordering::Acquire) {
            return Err("Window is closing".into());
        }
        self.cancelled.store(true, Ordering::Release);
        if let Some(old) = worker.take() {
            old.join().map_err(|_| "Playback worker panicked")?;
        }
        self.cancelled.store(false, Ordering::Release);
        let snapshot = Arc::clone(&self.snapshot);
        let cancelled = Arc::clone(&self.cancelled);
        let host = self.host;
        {
            let mut s = snapshot.lock().unwrap();
            s.revision += 1;
            s.session_id = Some(s.revision);
            s.state = "Loading";
            s.filename = file.file_name().map(|f| f.to_string_lossy().into_owned());
            s.error = None;
        }
        *worker = Some(std::thread::spawn(move || {
            let result = engine::play(host, &file, &cancelled, || {
                let mut s = snapshot.lock().unwrap();
                s.state = "Playing";
                s.revision += 1;
            });
            let mut s = snapshot.lock().unwrap();
            s.revision += 1;
            if let Err(error) = result {
                eprintln!("Playback error: {error}");
                s.state = "Error";
                s.error = Some(error);
            } else if cancelled.load(Ordering::Acquire) {
                s.state = "Idle";
                s.session_id = None;
                s.filename = None;
            } else {
                s.state = "Ended";
            }
        }));
        Ok(())
    }

    fn halt(&self) -> Result<(), String> {
        let mut worker = self.worker.lock().unwrap();
        self.cancelled.store(true, Ordering::Release);
        if let Some(w) = worker.take() {
            w.join().map_err(|_| "Playback worker panicked")?;
        }
        let mut s = self.snapshot.lock().unwrap();
        s.state = "Idle";
        s.revision += 1;
        s.session_id = None;
        s.filename = None;
        s.error = None;
        Ok(())
    }
}

#[tauri::command]
fn get_snapshot(probe: State<'_, Probe>) -> Snapshot {
    probe.snapshot.lock().unwrap().clone()
}

#[tauri::command]
async fn open_local(window: WebviewWindow) -> Result<(), String> {
    let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(path) = window::pick(owner)? {
            window.state::<Probe>().start(path)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stop(window: WebviewWindow) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || window.state::<Probe>().halt())
        .await
        .map_err(|e| e.to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let main = app
                .get_webview_window("main")
                .ok_or("Missing main window")?;
            let host = window::create(&main)?;
            app.manage(Probe {
                host,
                snapshot: Arc::new(Mutex::new(Snapshot {
                    probe: true,
                    state: "Idle",
                    revision: 0,
                    session_id: None,
                    app_version: env!("CARGO_PKG_VERSION"),
                    filename: None,
                    error: None,
                })),
                cancelled: Arc::new(AtomicBool::new(false)),
                worker: Mutex::new(None),
                closing: AtomicBool::new(false),
            });
            // An explicit desktop command line lets the pixel test select its generated fixture.
            let mut args = std::env::args_os().skip(1);
            if args.next().as_deref() == Some(std::ffi::OsStr::new("--probe-local")) {
                let file = args.next().ok_or("Missing probe file")?;
                main.state::<Probe>().start(file.into())?;
            }
            Ok(())
        })
        .on_window_event(|win, event| {
            let Some(probe) = win.try_state::<Probe>() else {
                return;
            };
            match event {
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    if let (Ok(hwnd), Ok(scale)) = (win.hwnd(), win.scale_factor()) {
                        window::layout(hwnd.0, probe.host, scale);
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    if !probe.closing.swap(true, Ordering::AcqRel) {
                        let win = win.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            if let Err(error) = win.state::<Probe>().halt() {
                                eprintln!("{error}");
                            }
                            if let Err(error) = win.destroy() {
                                eprintln!("{error}");
                            }
                        });
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![get_snapshot, open_local, stop])
        .run(tauri::generate_context!())
        .expect("failed to run HongGuoRust playback probe");
}
