//! `SetWinEventHook` for foreground / minimize / show-hide / move / cloak events.
//! The callback does nothing but restart a one-shot debounce timer on the message
//! window, so idle cost is zero and a burst of events produces one `WindowsChanged`.
use super::message_window::{COVERAGE_DEBOUNCE_MS, TIMER_COVERAGE};
use super::sys::*;
use std::io;
use std::sync::atomic::{AtomicIsize, Ordering};

static TARGET: AtomicIsize = AtomicIsize::new(0);

const OBJID_WINDOW: i32 = 0;
const CHILDID_SELF: i32 = 0;

/// (min, max) event ranges: foreground; minimize start/end; object destroy..hide;
/// cloaked/uncloaked; location change.
const RANGES: [(u32, u32); 5] = [
    (0x0003, 0x0003),
    (0x0016, 0x0017),
    (0x8001, 0x8003),
    (0x8017, 0x8018),
    (0x800B, 0x800B),
];

unsafe extern "system" fn hook_proc(
    _hook: isize,
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // Only whole-window events (not carets, cursors, child objects).
    if hwnd == 0 || id_object != OBJID_WINDOW || id_child != CHILDID_SELF {
        return;
    }
    let target = TARGET.load(Ordering::Relaxed);
    if target != 0 {
        // SAFETY: runs on the message thread (out-of-context hook); re-arming the same
        // timer id restarts it, which gives a trailing debounce.
        unsafe { SetTimer(target, TIMER_COVERAGE, COVERAGE_DEBOUNCE_MS, None) };
    }
}

/// Unhooks on drop.
pub struct WinEventHooks {
    handles: Vec<isize>,
}

pub fn install(target: HWND) -> io::Result<WinEventHooks> {
    TARGET.store(target, Ordering::Relaxed);
    let mut guard = WinEventHooks {
        handles: Vec::new(),
    };
    for (min, max) in RANGES {
        // SAFETY: valid callback with the documented signature; out-of-context delivery
        // runs it on this thread through our message loop.
        let h = unsafe {
            SetWinEventHook(
                min,
                max,
                0,
                Some(hook_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if h == 0 {
            return Err(io::Error::last_os_error()); // `guard` unhooks what was installed
        }
        guard.handles.push(h);
    }
    Ok(guard)
}

impl Drop for WinEventHooks {
    fn drop(&mut self) {
        TARGET.store(0, Ordering::Relaxed);
        for &h in &self.handles {
            // SAFETY: handles came from SetWinEventHook and are released once.
            unsafe { UnhookWinEvent(h) };
        }
    }
}
