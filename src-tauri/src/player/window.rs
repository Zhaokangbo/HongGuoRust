use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::sync::{Condvar, Mutex};
use std::time::Duration;
use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetClientRect, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SW_HIDE,
    SW_SHOW, WS_CHILD,
    WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
};

/// The UI thread owns this STATIC child. Windows destroys it with its parent
/// after the playback worker has dropped its last libmpv handle.
pub fn create(window: &WebviewWindow) -> Result<isize, String> {
    let parent = window.hwnd().map_err(|e| e.to_string())?.0;
    let host = unsafe {
        CreateWindowExW(
            0,
            windows_sys::w!("STATIC"),
            windows_sys::w!("HongGuoRust video"),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            1,
            1,
            parent,
            null_mut(),
            null_mut(),
            null_mut(),
        )
    };
    if host.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    layout(
        parent,
        host as isize,
        window.scale_factor().map_err(|e| e.to_string())?,
        window.is_fullscreen().map_err(|e| e.to_string())?,
    );
    Ok(host as isize)
}

pub fn layout(parent: HWND, host: isize, scale: f64, fullscreen: bool) {
    let mut rect = RECT::default();
    if unsafe { GetClientRect(parent, &mut rect) } == 0 {
        return;
    }
    let px = |v: f64| (v * scale).round() as i32;
    let left = if fullscreen { 0 } else { px(18.) };
    let top = if fullscreen { 0 } else { px(66.) };
    let right = if fullscreen { 0 } else { px(18.) };
    let bottom = if fullscreen { 0 } else { px(96.) };
    unsafe {
        SetWindowPos(
            host as HWND,
            HWND_TOP,
            left,
            top,
            (rect.right - left - right).max(1),
            (rect.bottom - top - bottom).max(1),
            SWP_NOACTIVATE,
        );
    }
}

/// Hide the native child HWND when the HTML catalog occupies the same region.
/// The normal player layout and native HWND lifetime remain unchanged.
pub fn set_video_visible(host: isize, visible: bool) {
    unsafe { ShowWindow(host as HWND, if visible { SW_SHOW } else { SW_HIDE }) };
}

#[derive(Default)]
struct PickerState {
    active: bool,
    closing: bool,
}

#[derive(Default)]
pub struct Picker {
    state: Mutex<PickerState>,
    returned: Condvar,
}

impl Picker {
    pub fn choose(&self, parent: isize) -> Result<Option<PathBuf>, String> {
        {
            let mut state = self.state.lock().unwrap();
            if state.closing || state.active {
                return Err("File picker unavailable".into());
            }
            state.active = true;
        }
        let result = pick(parent);
        self.state.lock().unwrap().active = false;
        self.returned.notify_all();
        result
    }

    // The blocking close task keeps the owner HWND alive until the modal call returns.
    pub fn shutdown(&self, parent: isize) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetClassNameW, GetLastActivePopup, GetWindow, PostMessageW, GW_OWNER, WM_CLOSE,
        };
        let parent = parent as HWND;
        let mut state = self.state.lock().unwrap();
        state.closing = true;
        while state.active {
            unsafe {
                let dialog = GetLastActivePopup(parent);
                let mut class = [0u16; 32];
                let len = GetClassNameW(dialog, class.as_mut_ptr(), class.len() as i32);
                if dialog != parent
                    && GetWindow(dialog, GW_OWNER) == parent
                    && String::from_utf16_lossy(&class[..len as usize]) == "#32770"
                {
                    PostMessageW(dialog, WM_CLOSE, 0, 0);
                }
            }
            state = self
                .returned
                .wait_timeout(state, Duration::from_millis(40))
                .unwrap()
                .0;
        }
    }
}

fn pick(parent: isize) -> Result<Option<PathBuf>, String> {
    let mut buffer = vec![0u16; 32768];
    let filter: Vec<_> = "MP4 video\0*.mp4\0\0".encode_utf16().collect();
    let mut dialog = OPENFILENAMEW::default();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = parent as HWND;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrFile = buffer.as_mut_ptr();
    dialog.nMaxFile = buffer.len() as u32;
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        let code = unsafe { CommDlgExtendedError() };
        return if code == 0 {
            Ok(None)
        } else {
            Err(format!("File dialog error {code}"))
        };
    }
    let len = buffer
        .iter()
        .position(|&c| c == 0)
        .ok_or("Unterminated file path")?;
    Ok(Some(OsString::from_wide(&buffer[..len]).into()))
}
