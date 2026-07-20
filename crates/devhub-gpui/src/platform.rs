use gpui::Window;

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
