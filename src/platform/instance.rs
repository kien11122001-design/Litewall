//! Single-instance guard (named mutex) and forwarding to the running instance.
use super::message_window::CLASS_NAME;
use super::sys::*;
use std::io;
use std::ptr::null;
use std::time::Duration;

const MUTEX_NAME: &str = "Local\\LiteWall.SingleInstance";

/// Holds the mutex for the process lifetime; releases on drop.
pub struct Guard(HANDLE);

impl Drop for Guard {
    fn drop(&mut self) {
        // SAFETY: handle came from CreateMutexW and is closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

pub enum Instance {
    /// We are the first instance.
    First(Guard),
    /// Another instance already runs.
    Existing,
}

pub fn acquire() -> io::Result<Instance> {
    let name = wide(MUTEX_NAME);
    // SAFETY: `name` is a valid NUL-terminated UTF-16 string for the call's duration.
    let (h, err) = unsafe {
        let h = CreateMutexW(null(), 1, name.as_ptr());
        (h, GetLastError())
    };
    if h == 0 {
        return Err(io::Error::last_os_error());
    }
    if err == ERROR_ALREADY_EXISTS {
        // SAFETY: valid handle; we do not keep it.
        unsafe { CloseHandle(h) };
        Ok(Instance::Existing)
    } else {
        Ok(Instance::First(Guard(h)))
    }
}

/// Post `msg` to the running instance's hidden window. Retries briefly because the
/// first instance may still be creating its window. Returns whether it was delivered.
/// Call from a short-lived launch path only (it sleeps).
pub fn signal_existing(msg: u32) -> bool {
    let class = wide(CLASS_NAME);
    for _ in 0..20 {
        // SAFETY: `class` is a valid NUL-terminated UTF-16 string.
        let hwnd = unsafe { FindWindowW(class.as_ptr(), null()) };
        if hwnd != 0 {
            // SAFETY: posting a message to a foreign window handle is safe; failure is reported.
            return unsafe { PostMessageW(hwnd, msg, 0, 0) } != 0;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}
