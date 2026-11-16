//! Process diagnostics used by the leak probe (`--probe-handles`).
use super::sys::*;

/// Number of kernel handles currently open in this process.
pub fn process_handle_count() -> u32 {
    let mut n = 0u32;
    // SAFETY: the pseudo-handle from GetCurrentProcess is always valid and `n` is a valid
    // out-pointer; on failure `n` stays 0.
    unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut n) };
    n
}
