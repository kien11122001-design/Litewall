//! Small shell helpers.
use super::sys::*;
use std::io;
use std::path::Path;
use std::ptr::null;

/// Open `path` (a folder) in Explorer.
pub fn open_folder(path: &Path) -> io::Result<()> {
    let target = wide(&path.to_string_lossy());
    let verb = wide("open");
    // SAFETY: valid NUL-terminated UTF-16 strings for the duration of the call.
    let r = unsafe {
        ShellExecuteW(
            0,
            verb.as_ptr(),
            target.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    if r > 32 {
        Ok(())
    } else {
        Err(io::Error::other(format!("ShellExecuteW failed ({r})")))
    }
}
