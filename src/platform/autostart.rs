//! "Start with Windows": a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! (no admin rights, easy for the user to see and remove).
use super::sys::*;
use std::io;
use std::path::Path;
use std::ptr::{null, null_mut};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "LiteWall";

/// Add (`enabled`) or remove the Run entry. The command is the quoted path of `exe`.
pub fn set(enabled: bool, exe: &Path) -> io::Result<()> {
    let key = wide(RUN_KEY);
    let name = wide(VALUE_NAME);
    let mut hkey: isize = 0;
    // SAFETY: valid NUL-terminated strings and out-pointers for the calls' duration; the key
    // handle is closed below on every path.
    unsafe {
        let rc = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            null(),
            0,
            KEY_SET_VALUE,
            null(),
            &mut hkey,
            null_mut(),
        );
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        let rc = if enabled {
            let command = wide(&format!("\"{}\"", exe.display()));
            RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr() as *const u8,
                (command.len() * 2) as u32, // bytes, including the terminating NUL
            )
        } else {
            match RegDeleteValueW(hkey, name.as_ptr()) {
                ERROR_FILE_NOT_FOUND => 0, // already gone
                other => other,
            }
        };
        RegCloseKey(hkey);
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
    }
    Ok(())
}
