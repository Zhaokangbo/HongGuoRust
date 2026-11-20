use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr::null_mut;
use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetClientRect, SetWindowPos, HWND_TOP, SWP_NOACTIVATE,
    WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
};

/// The UI thread owns this STATIC child. Windows destroys it with its parent
/// after the playback worker has dropped its last libmpv handle.
pub fn create(window: &WebviewWindow) -> Result<isize, String> {
    let parent = window.hwnd().map_err(|e| e.to_string())?.0;
    let host = unsafe {
        CreateWindowExW(0, windows_sys::w!("STATIC"), windows_sys::w!("HongGuoRust video"),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0, 0, 1, 1, parent, null_mut(), null_mut(), null_mut())
    };
    if host.is_null() { return Err(std::io::Error::last_os_error().to_string()); }
    layout(parent, host as isize, window.scale_factor().map_err(|e| e.to_string())?);
    Ok(host as isize)
}

pub fn layout(parent: HWND, host: isize, scale: f64) {
    let mut rect = RECT::default();
    if unsafe { GetClientRect(parent, &mut rect) } == 0 { return; }
    let px = |v: f64| (v * scale).round() as i32;
    unsafe { SetWindowPos(host as HWND, HWND_TOP, px(18.), px(66.),
        (rect.right - 2 * px(18.)).max(1), (rect.bottom - px(66.) - px(96.)).max(1),
        SWP_NOACTIVATE); }
}

pub fn pick(parent: isize) -> Result<Option<PathBuf>, String> {
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
        return if code == 0 { Ok(None) } else { Err(format!("File dialog error {code}")) };
    }
    let len = buffer.iter().position(|&c| c == 0).ok_or("Unterminated file path")?;
    Ok(Some(OsString::from_wide(&buffer[..len]).into()))
}
