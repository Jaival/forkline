//! Safe wrappers over the few Win32 calls Forkline needs and the GPUI stack
//! does not expose. This is the only crate that may use `unsafe`; keep each
//! call small and justify it with a `SAFETY:` comment.

/// Draw the window's native title bar dark or light. GPUI sets it only from
/// the Windows app mode; this makes it follow Forkline's theme instead.
///
/// `hwnd` is the window's handle (from `raw-window-handle`). A stale or
/// invalid handle makes the call fail; the error is ignored, the title bar
/// keeps its colour.
#[cfg(windows)]
pub fn set_dark_title_bar(hwnd: isize, dark: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
    use windows::core::BOOL;

    let value = BOOL::from(dark);
    // SAFETY: DwmSetWindowAttribute reads `size_of::<BOOL>()` bytes from the
    // pointer, which points at `value`, a live BOOL on this stack frame. The
    // handle is only passed to the OS, which validates it and returns an
    // error for one that is not a window.
    let _ = unsafe {
        DwmSetWindowAttribute(
            HWND(hwnd as *mut _),
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const value).cast(),
            size_of::<BOOL>() as u32,
        )
    };
}

/// Other platforms draw their own title bars.
#[cfg(not(windows))]
pub fn set_dark_title_bar(_hwnd: isize, _dark: bool) {}

mod instance;
pub use instance::{Instance, Listener, single_instance};

/// Hide (`true`) or show the window on screen while the OS still treats it as
/// visible, so GPUI keeps drawing frames into it. The main window opens
/// cloaked and is uncloaked after its first frame, so no blank frame shows.
#[cfg(windows)]
pub fn set_cloaked(hwnd: isize, cloaked: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
    use windows::core::BOOL;

    let value = BOOL::from(cloaked);
    // SAFETY: as in `set_dark_title_bar`: the pointer is to `value`, a live
    // BOOL of the size passed; the OS validates the handle.
    let _ = unsafe {
        DwmSetWindowAttribute(HWND(hwnd as *mut _), DWMWA_CLOAK, (&raw const value).cast(), size_of::<BOOL>() as u32)
    };
}

#[cfg(not(windows))]
pub fn set_cloaked(_hwnd: isize, _cloaked: bool) {}

/// Show the window without activating it (it stays cloaked if it was).
#[cfg(windows)]
pub fn show_inactive(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SW_SHOWNA, ShowWindow};
    // SAFETY: ShowWindow takes the handle by value and validates it; the
    // returned previous visibility is not needed.
    let _ = unsafe { ShowWindow(HWND(hwnd as *mut _), SW_SHOWNA) };
}

#[cfg(not(windows))]
pub fn show_inactive(_hwnd: isize) {}

/// Restore the window if it is minimized (a maximized or normal window keeps
/// its size).
#[cfg(windows)]
pub fn restore_if_minimized(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{IsIconic, SW_RESTORE, ShowWindow};
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: both calls take the handle by value and validate it.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
    }
}

#[cfg(not(windows))]
pub fn restore_if_minimized(_hwnd: isize) {}
