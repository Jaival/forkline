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
