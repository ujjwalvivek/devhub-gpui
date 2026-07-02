use gpui::Window;
use std::path::Path;

#[cfg(target_os = "windows")]
fn windows_hwnd(window: &Window) -> Option<windows::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;

    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(HWND(handle.hwnd.get() as *mut std::ffi::c_void))
}

#[cfg(target_os = "windows")]
pub(crate) fn configure_windows_surface(window: &Window) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE,
        DWMWCP_ROUNDSMALL,
    };

    let Some(hwnd) = windows_hwnd(window) else {
        return;
    };
    let corner_preference = DWMWCP_ROUNDSMALL;
    let border_color = 0x000e0e0e_u32;

    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_preference as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&corner_preference) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_color as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&border_color) as u32,
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn configure_windows_surface(_: &Window) {}

#[cfg(target_os = "windows")]
pub(crate) fn begin_window_drag(window: &Window) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, HTCAPTION, WM_NCLBUTTONDOWN};

    let Some(hwnd) = windows_hwnd(window) else {
        return;
    };

    unsafe {
        let _ = ReleaseCapture();
        let _ = PostMessageW(
            Some(hwnd),
            WM_NCLBUTTONDOWN,
            WPARAM(HTCAPTION as usize),
            LPARAM(0),
        );
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn toggle_window_zoom(window: &Window) {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsZoomed, ShowWindowAsync, SW_MAXIMIZE, SW_RESTORE,
    };

    let Some(hwnd) = windows_hwnd(window) else {
        return;
    };
    unsafe {
        let command = if IsZoomed(hwnd).as_bool() {
            SW_RESTORE
        } else {
            SW_MAXIMIZE
        };
        let _ = ShowWindowAsync(hwnd, command);
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn toggle_window_zoom(window: &Window) {
    window.zoom_window();
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn begin_window_drag(window: &Window) {
    window.start_window_move();
}

#[cfg(target_os = "windows")]
pub(crate) fn open_with_picker(path: &Path, window: &Window) {
    open_with_target(path.as_os_str().to_os_string(), window);
}

#[cfg(target_os = "windows")]
pub(crate) fn open_uri_with_picker(uri: &str, window: &Window) {
    open_with_target(uri.into(), window);
}

#[cfg(target_os = "windows")]
fn open_with_target(target: std::ffi::OsString, window: &Window) {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::{
        SHOpenWithDialog, OAIF_ALLOW_REGISTRATION, OAIF_EXEC, OPENASINFO,
    };

    let hwnd_raw = windows_hwnd(window).map(|h| h.0 as isize);
    let path_wide: Vec<u16> = target.encode_wide().chain(Some(0)).collect();

    std::thread::spawn(move || {
        let hwnd = hwnd_raw.map(|raw| HWND(raw as *mut std::ffi::c_void));
        let info = OPENASINFO {
            pcszFile: PCWSTR(path_wide.as_ptr()),
            pcszClass: PCWSTR::null(),
            oaifInFlags: OAIF_EXEC | OAIF_ALLOW_REGISTRATION,
        };
        unsafe {
            let _ = SHOpenWithDialog(hwnd, &info);
        }
    });
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub(crate) fn open_with_picker(path: &Path, _window: &Window) {
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub(crate) fn open_uri_with_picker(uri: &str, _window: &Window) {
    let _ = std::process::Command::new("xdg-open").arg(uri).spawn();
}

#[cfg(target_os = "macos")]
pub(crate) fn open_with_picker(path: &Path, _window: &Window) {
    let _ = std::process::Command::new("open").arg(path).spawn();
}

#[cfg(target_os = "macos")]
pub(crate) fn open_uri_with_picker(uri: &str, _window: &Window) {
    let _ = std::process::Command::new("open").arg(uri).spawn();
}
