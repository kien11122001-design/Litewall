//! Which top-level windows count as covering a monitor (PRD §8). Only called after a
//! debounced event, never on a timer.
use super::sys::*;
use crate::core::coverage::window_exactly_fills;
use crate::core::geometry::RectI32;
use std::ffi::c_void;

/// Shell and desktop windows are never "covering" (Progman/WorkerW span the desktop).
const SKIP_CLASSES: [&str; 4] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
];

unsafe extern "system" fn collect_cb(h: HWND, l: LPARAM) -> i32 {
    // SAFETY: `l` is the address of the caller's live Vec; runs only during EnumWindows.
    unsafe { (*(l as *mut Vec<HWND>)).push(h) };
    1
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: the length passed matches the buffer.
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn is_cloaked(h: HWND) -> bool {
    let mut v: u32 = 0;
    // SAFETY: valid out-pointer of the size announced.
    let hr =
        unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut v as *mut u32 as *mut c_void, 4) };
    hr >= 0 && v != 0
}

fn layered_alpha(h: HWND) -> Option<u8> {
    let (mut key, mut alpha, mut flags) = (0u32, 0u8, 0u32);
    // SAFETY: valid out-pointers.
    let ok = unsafe { GetLayeredWindowAttributes(h, &mut key, &mut alpha, &mut flags) };
    (ok != 0 && flags & LWA_ALPHA != 0).then_some(alpha)
}

fn cover_window(h: HWND, own_pid: u32) -> Option<(String, RectI32)> {
    // SAFETY: plain queries on a handle that may die meanwhile; failures are harmless.
    let (visible, iconic, ex, pid, rect) = unsafe {
        let mut pid = 0u32;
        let mut r = RECT::default();
        GetWindowThreadProcessId(h, &mut pid);
        let ok = GetWindowRect(h, &mut r);
        (
            IsWindowVisible(h) != 0,
            IsIconic(h) != 0,
            GetWindowLongPtrW(h, GWL_EXSTYLE) as u32,
            pid,
            (ok != 0).then_some(r),
        )
    };
    let r = rect?;
    if !visible || iconic || pid == own_pid {
        return None;
    }
    if ex & (WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT) != 0 {
        return None;
    }
    if ex & WS_EX_LAYERED != 0 && layered_alpha(h) == Some(0) {
        return None;
    }
    if is_cloaked(h) {
        return None;
    }
    let rect = RectI32::new(r.left, r.top, r.right, r.bottom);
    if rect.is_empty() {
        return None;
    }
    let class = class_name(h);
    if SKIP_CLASSES.contains(&class.as_str()) {
        return None;
    }
    Some((class, rect))
}

/// Visible, non-minimised, non-cloaked top-level windows (class, rect), topmost first.
pub fn visible_windows() -> Vec<(String, RectI32)> {
    let mut all: Vec<HWND> = Vec::new();
    // SAFETY: the callback runs only during this call and `all` outlives it.
    unsafe { EnumWindows(collect_cb, &mut all as *mut Vec<HWND> as LPARAM) };
    // SAFETY: no arguments, no preconditions.
    let own_pid = unsafe { GetCurrentProcessId() };
    all.into_iter()
        .filter_map(|h| cover_window(h, own_pid))
        .collect()
}

/// Class and rectangle of the foreground window (diagnostics + the "fills the monitor" shortcut).
pub fn foreground() -> Option<(String, RectI32)> {
    // SAFETY: no preconditions.
    let h = unsafe { GetForegroundWindow() };
    if h == 0 {
        return None;
    }
    let mut r = RECT::default();
    // SAFETY: valid out-pointer.
    if unsafe { GetWindowRect(h, &mut r) } == 0 {
        return None;
    }
    Some((
        class_name(h),
        RectI32::new(r.left, r.top, r.right, r.bottom),
    ))
}

/// `QUERY_USER_NOTIFICATION_STATE` (3 = exclusive D3D fullscreen, 4 = presentation mode,
/// 5 = normal); `None` when the query fails.
pub fn notification_state() -> Option<u32> {
    let mut state: i32 = 0;
    // SAFETY: valid out-pointer for the call's duration.
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    (hr >= 0).then_some(state as u32)
}

/// The foreground window as a fullscreen-app candidate: `(class, rect)`.
/// It must pass the same filters as the coverage scan (visible, not minimised, not cloaked,
/// not a tool window, not ours, not the shell), so clicking the desktop or the taskbar never
/// counts. `None` when there is no such foreground window.
pub fn foreground_candidate() -> Option<(String, RectI32)> {
    // SAFETY: no preconditions.
    let h = unsafe { GetForegroundWindow() };
    if h == 0 {
        return None;
    }
    // SAFETY: no arguments, no preconditions.
    let own_pid = unsafe { GetCurrentProcessId() };
    cover_window(h, own_pid)
}

/// Why the foreground window is (not) a fullscreen-app candidate, as one log line: class, rect,
/// styles, the screen it exactly fills (if any) and every filter that rejects it. Diagnostics
/// only, called when the shell says "busy" and an exactly-fitting window exists but nothing
/// was detected.
pub fn foreground_report(monitors: &[RectI32]) -> String {
    // SAFETY: no preconditions.
    let h = unsafe { GetForegroundWindow() };
    if h == 0 {
        return "no foreground window".to_string();
    }
    let mut r = RECT::default();
    let mut pid = 0u32;
    // SAFETY: valid out-pointers; a handle that died meanwhile just makes the calls fail.
    let (got_rect, visible, iconic, style, ex, own_pid) = unsafe {
        GetWindowThreadProcessId(h, &mut pid);
        (
            GetWindowRect(h, &mut r) != 0,
            IsWindowVisible(h) != 0,
            IsIconic(h) != 0,
            GetWindowLongPtrW(h, GWL_STYLE) as u32,
            GetWindowLongPtrW(h, GWL_EXSTYLE) as u32,
            GetCurrentProcessId(),
        )
    };
    if !got_rect {
        return "foreground window has no rectangle".to_string();
    }
    let rect = RectI32::new(r.left, r.top, r.right, r.bottom);
    let class = class_name(h);
    let fit = monitors.iter().position(|m| window_exactly_fills(*m, rect));
    let mut why: Vec<&str> = Vec::new();
    if !visible {
        why.push("not visible");
    }
    if iconic {
        why.push("minimised");
    }
    if pid == own_pid {
        why.push("our own window");
    }
    if ex & WS_EX_TOOLWINDOW != 0 {
        why.push("tool window");
    }
    if ex & WS_EX_TRANSPARENT != 0 {
        why.push("transparent");
    }
    if ex & WS_EX_LAYERED != 0 && layered_alpha(h) == Some(0) {
        why.push("layered alpha 0");
    }
    if is_cloaked(h) {
        why.push("cloaked");
    }
    if SKIP_CLASSES.contains(&class.as_str()) {
        why.push("shell class");
    }
    if fit.is_none() {
        why.push("does not exactly fill a screen");
    }
    format!(
        "foreground {class:?} rect={rect:?} style=0x{style:X} ex=0x{ex:X} \
         fits_screen={fit:?} caption={} blocked_by={why:?}",
        style & WS_CAPTION == WS_CAPTION
    )
}
