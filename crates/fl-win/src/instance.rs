//! One Forkline per data directory. Two processes on one database would run
//! two sync engines and two action workers (a queued comment could be sent
//! twice). The first process holds a named mutex and listens on a named
//! pipe; a later launch with the same key writes "show" to the pipe and exits.

/// The result of [`single_instance`].
pub enum Instance {
    /// This process is the first. Call [`Listener::listen`] once the window
    /// can be shown.
    First(Listener),
    /// Another process has the key; it was asked to show its window.
    Other,
}

/// The first process's pipe, not yet listening.
pub struct Listener {
    #[cfg_attr(not(windows), allow(dead_code))]
    pipe: String,
}

/// Mutex and pipe name from `key` (for example the data directory): FNV-1a,
/// stable across builds, so two Forkline versions on one directory also see
/// each other.
fn name(key: &str) -> String {
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    format!("Forkline-{hash:016x}")
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Take the single-instance lock for `key`, or ask its owner to show itself.
#[cfg(windows)]
pub fn single_instance(key: &str) -> Instance {
    use std::io::Write as _;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
    use windows::core::PCWSTR;

    let id = name(key);
    let pipe = format!(r"\\.\pipe\{id}");
    let mutex_name = wide(&format!(r"Local\{id}"));
    // SAFETY: the name is a live NUL-terminated UTF-16 buffer; no security
    // attributes. GetLastError is read right after, on the same thread. The
    // handle is never closed: the lock lasts as long as the process.
    let exists = unsafe {
        let created = CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr()));
        created.is_ok() && GetLastError() == ERROR_ALREADY_EXISTS
    };
    if !exists {
        return Instance::First(Listener { pipe });
    }
    // Let the first process take the foreground when it shows its window.
    // SAFETY: takes a plain id; failure only means it may not come to front.
    let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    // The first process may still be starting; its pipe appears shortly.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(&pipe) {
            let _ = f.write_all(b"show");
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Instance::Other
}

/// Other platforms: always the first.
#[cfg(not(windows))]
pub fn single_instance(key: &str) -> Instance {
    Instance::First(Listener { pipe: name(key) })
}

impl Listener {
    /// Call `on_show` (on a background thread) for each later launch.
    #[cfg(windows)]
    pub fn listen(self, on_show: impl Fn() + Send + 'static) {
        use std::io::Read as _;
        use std::os::windows::io::FromRawHandle as _;
        use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, GetLastError};
        use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
        use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_TYPE_BYTE, PIPE_WAIT};
        use windows::core::PCWSTR;

        let name = wide(&self.pipe);
        std::thread::Builder::new()
            .name("forkline-instance".into())
            .spawn(move || {
                loop {
                    // SAFETY: the name is a live NUL-terminated UTF-16 buffer;
                    // default security (other users cannot write to it). The
                    // first-instance flag fails if another process holds the
                    // name.
                    let handle = unsafe {
                        CreateNamedPipeW(
                            PCWSTR(name.as_ptr()),
                            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                            PIPE_TYPE_BYTE | PIPE_WAIT,
                            1,
                            0,
                            64,
                            0,
                            None,
                        )
                    };
                    if handle.is_invalid() {
                        return;
                    }
                    // SAFETY: `handle` is the pipe just created; this blocks
                    // until a client connects. A client that connected first
                    // reports ERROR_PIPE_CONNECTED, which is a success.
                    let connected = unsafe {
                        ConnectNamedPipe(handle, None).is_ok() || GetLastError() == ERROR_PIPE_CONNECTED
                    };
                    // SAFETY: `handle` is a valid pipe handle that nothing
                    // else owns; the File closes it when dropped.
                    let mut file = unsafe { std::fs::File::from_raw_handle(handle.0) };
                    let mut buf = [0u8; 16];
                    if connected && file.read(&mut buf).is_ok_and(|n| &buf[..n] == b"show") {
                        on_show();
                    }
                }
            })
            .expect("cannot start the single-instance thread");
    }

    #[cfg(not(windows))]
    pub fn listen(self, _on_show: impl Fn() + Send + 'static) {}
}

#[cfg(all(test, windows))]
mod tests {
    use super::{Instance, single_instance};

    #[test]
    fn a_second_launch_asks_the_first_to_show() {
        let key = format!("test-{}-{:?}", std::process::id(), std::time::SystemTime::now());
        let Instance::First(listener) = single_instance(&key) else { panic!("a fresh key is taken") };
        let (tx, rx) = std::sync::mpsc::channel();
        listener.listen(move || tx.send(()).unwrap());
        assert!(matches!(single_instance(&key), Instance::Other));
        rx.recv_timeout(std::time::Duration::from_secs(3)).expect("the first did not hear the second");
        assert!(matches!(single_instance(&format!("{key}-other")), Instance::First(_)));
    }
}
