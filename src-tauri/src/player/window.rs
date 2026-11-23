use std::ffi::{c_void, OsString};
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
    CreateWindowExW, GetClientRect, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
};

const SS_BLACKRECT: u32 = 0x0000_0004;

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: HWND,
        dw_attribute: u32,
        pv_attribute: *const c_void,
        cb_attribute: u32,
    ) -> i32;
}

/// Match the Windows 10/11 native titlebar & border to the `#141414` cinema UI.
pub fn apply_dark_titlebar(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = hwnd.0;
    let dark_mode: u32 = 1;
    // Win32 COLORREF is 0x00BBGGRR: #141414 caption, #D8D8D8 text, #242424 border.
    let caption_color: u32 = 0x0014_1414;
    let text_color: u32 = 0x00D8_D8D8;
    let border_color: u32 = 0x0024_2424;
    unsafe {
        // 20 = DWMWA_USE_IMMERSIVE_DARK_MODE (Win10 20H1+/Win11), 19 = earlier Win10 builds
        let _ = DwmSetWindowAttribute(
            hwnd,
            20,
            (&dark_mode as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            19,
            (&dark_mode as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        // 35 = DWMWA_CAPTION_COLOR, 36 = DWMWA_TEXT_COLOR, 34 = DWMWA_BORDER_COLOR (Win11)
        let _ = DwmSetWindowAttribute(
            hwnd,
            35,
            (&caption_color as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            36,
            (&text_color as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            34,
            (&border_color as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// The UI thread owns this STATIC child. Windows destroys it with its parent
/// after the playback worker has dropped its last libmpv handle.
pub fn create(window: &WebviewWindow) -> Result<isize, String> {
    let parent = window.hwnd().map_err(|e| e.to_string())?.0;
    let host = unsafe {
        CreateWindowExW(
            0,
            windows_sys::w!("STATIC"),
            windows_sys::w!("HongGuoRust video"),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | SS_BLACKRECT,
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
    layout(parent, host as isize);
    Ok(host as isize)
}

/// The video surface always fills the whole client area. Player controls are
/// drawn by the transparent, owned overlay window (see `overlay.rs`), so the
/// native HWND no longer reserves space for an HTML header or footer.
pub fn layout(parent: HWND, host: isize) {
    let mut rect = RECT::default();
    if unsafe { GetClientRect(parent, &mut rect) } == 0 {
        return;
    }
    unsafe {
        SetWindowPos(
            host as HWND,
            HWND_TOP,
            0,
            0,
            (rect.right - rect.left).max(1),
            (rect.bottom - rect.top).max(1),
            SWP_NOACTIVATE,
        );
    }
}

/// Hide the native child HWND when the HTML catalog occupies the same region.
/// The normal player layout and native HWND lifetime remain unchanged.
pub fn set_video_visible(host: isize, visible: bool) {
    unsafe {
        if visible {
            SetWindowPos(
                host as HWND,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        } else {
            ShowWindow(host as HWND, SW_HIDE);
        }
    }
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
        self.choose_filtered(parent, PickKind::Mp4)
    }

    pub fn choose_video(&self, parent: isize) -> Result<Option<PathBuf>, String> {
        self.choose_filtered(parent, PickKind::Video)
    }

    pub fn choose_manifest(&self, parent: isize) -> Result<Option<PathBuf>, String> {
        self.choose_filtered(parent, PickKind::Manifest)
    }

    fn choose_filtered(&self, parent: isize, kind: PickKind) -> Result<Option<PathBuf>, String> {
        {
            let mut state = self.state.lock().unwrap();
            if state.closing || state.active {
                return Err("File picker unavailable".into());
            }
            state.active = true;
        }
        let result = pick(parent, kind);
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

#[derive(Clone, Copy)]
enum PickKind {
    Mp4,
    Video,
    Manifest,
}

fn pick(parent: isize, kind: PickKind) -> Result<Option<PathBuf>, String> {
    let mut buffer = vec![0u16; 32768];
    let filter: Vec<_> = match kind {
        PickKind::Manifest => "JSON manifest\0*.json\0\0".encode_utf16().collect(),
        PickKind::Mp4 => "MP4 video\0*.mp4\0\0".encode_utf16().collect(),
        PickKind::Video => "Video files\0*.mp4;*.mkv;*.mov;*.m4v;*.webm;*.avi;*.flv;*.ts\0\0"
            .encode_utf16()
            .collect(),
    };
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
