//! Netflix-style player chrome.
//!
//! libmpv renders into a native child HWND, and an HTML element can never be
//! composited on top of a sibling HWND ("airspace" problem). The player
//! controls therefore live in a second, transparent, undecorated WebView window
//! that is *owned* by the main window:
//!
//! - an owned window is always above its owner in the z-order;
//! - it is hidden automatically while the owner is minimized;
//! - it is destroyed automatically together with its owner.
//!
//! Rust keeps the overlay glued to the main window's client area on every
//! move / resize / fullscreen change and stores the opaque UI context (series
//! title, episode list, local playlist) so the overlay can recover after a
//! reload. The context never contains media URLs or file paths.
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const MAIN: &str = "main";
pub const OVERLAY: &str = "overlay";
pub const EVENT_CONTEXT: &str = "player://context";
pub const EVENT_DISMISSED: &str = "player://dismissed";

/// Upper bound for the serialized UI context (episode lists are capped at 200
/// episodes by the providers, so this is generous).
const MAX_CONTEXT_BYTES: usize = 256 * 1024;

#[derive(Default)]
pub struct PlayerUi {
    active: AtomicBool,
    context: Mutex<Option<Value>>,
}

impl PlayerUi {
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn context(&self) -> Option<Value> {
        self.context.lock().ok().and_then(|value| value.clone())
    }

    pub fn set_context(&self, context: Value) -> Result<(), String> {
        validate_context(&context)?;
        *self.context.lock().map_err(|_| "player context poisoned")? = Some(context);
        Ok(())
    }

    fn clear(&self) {
        self.active.store(false, Ordering::SeqCst);
        if let Ok(mut value) = self.context.lock() {
            *value = None;
        }
    }
}

fn validate_context(context: &Value) -> Result<(), String> {
    if !context.is_object() {
        return Err("invalid_player_context: context must be an object".into());
    }
    let size = serde_json::to_vec(context)
        .map_err(|e| e.to_string())?
        .len();
    if size > MAX_CONTEXT_BYTES {
        return Err("invalid_player_context: context is too large".into());
    }
    Ok(())
}

/// Create the hidden overlay window once, at startup.
pub fn create(app: &AppHandle, main: &WebviewWindow) -> tauri::Result<WebviewWindow> {
    let overlay = WebviewWindowBuilder::new(app, OVERLAY, WebviewUrl::App("overlay.html".into()))
        .title("HongGuoRust Player")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .owner(main)?
        .build()?;
    sync(main);
    Ok(overlay)
}

/// Glue the overlay to the main window's client area (physical pixels).
pub fn sync(main: &WebviewWindow) {
    let Some(overlay) = main.get_webview_window(OVERLAY) else {
        return;
    };
    if main.is_minimized().unwrap_or(false) {
        return;
    }
    if let (Ok(position), Ok(size)) = (main.inner_position(), main.inner_size()) {
        let _ = overlay.set_position(PhysicalPosition::new(position.x, position.y));
        let _ = overlay.set_size(PhysicalSize::new(size.width.max(1), size.height.max(1)));
    }
}

/// Show the native video surface and the overlay for a freshly opened session.
pub fn present(app: &AppHandle, host: isize, ui: &PlayerUi, context: Value) -> Result<(), String> {
    ui.set_context(context.clone())?;
    ui.active.store(true, Ordering::SeqCst);
    let main = app.get_webview_window(MAIN).ok_or("Missing main window")?;
    let overlay = app
        .get_webview_window(OVERLAY)
        .ok_or("Missing player overlay window")?;
    if let Ok(hwnd) = main.hwnd() {
        super::window::layout(hwnd.0, host);
    }
    super::window::set_video_visible(host, true);
    sync(&main);
    overlay.show().map_err(|e| e.to_string())?;
    let _ = overlay.set_focus();
    overlay
        .emit_to(OVERLAY, EVENT_CONTEXT, &context)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Replace the context after an in-player episode change.
pub fn update(app: &AppHandle, ui: &PlayerUi, context: Value) -> Result<(), String> {
    ui.set_context(context.clone())?;
    app.emit_to(OVERLAY, EVENT_CONTEXT, &context)
        .map_err(|e| e.to_string())
}

/// Hide the player chrome and video surface and hand control back to the
/// main window. Playback itself is stopped by the caller.
pub fn dismiss(app: &AppHandle, host: isize, ui: &PlayerUi) -> Result<(), String> {
    ui.clear();
    let main = app.get_webview_window(MAIN).ok_or("Missing main window")?;
    if main.is_fullscreen().unwrap_or(false) {
        let _ = main.set_fullscreen(false);
    }
    if let Some(overlay) = app.get_webview_window(OVERLAY) {
        let _ = overlay.hide();
        let _ = overlay.emit_to(OVERLAY, EVENT_CONTEXT, Value::Null);
    }
    super::window::set_video_visible(host, false);
    let _ = main.set_focus();
    main.emit_to(MAIN, EVENT_DISMISSED, ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_must_be_a_bounded_object() {
        let ui = PlayerUi::default();
        assert!(ui
            .set_context(json!({ "kind": "single", "title": "x" }))
            .is_ok());
        assert!(ui.context().is_some());
        assert!(ui.set_context(json!("not an object")).is_err());
        assert!(ui.set_context(json!([1, 2, 3])).is_err());
        let huge = "x".repeat(MAX_CONTEXT_BYTES + 1);
        assert!(ui.set_context(json!({ "title": huge })).is_err());
        ui.clear();
        assert!(ui.context().is_none());
        assert!(!ui.is_active());
    }
}
