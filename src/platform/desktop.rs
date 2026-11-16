//! Desktop host discovery (PRD §2.1; approach proven in M0, see `docs/m0-findings.md`).
//!
//! Sends the undocumented `0x052C` message to `Progman` so Explorer creates the
//! wallpaper `WorkerW`, then finds it: the top-level window that hosts the icons
//! (`SHELLDLL_DefView`) has the wallpaper `WorkerW` as its next sibling (Classic layout).
//! The Windows 11 24H2+ "Raised" layout (icons under `Progman`) is detected and reported
//! but not supported yet (extended scope).
use super::sys::*;
use std::fmt;
use std::ptr::null;
use std::sync::atomic::{AtomicBool, Ordering};

/// The window tree is dumped on the first lookup and whenever no host is found, not on
/// every display change (it is ~40 log lines and made the log rotate within minutes).
static TREE_LOGGED: AtomicBool = AtomicBool::new(false);
static FAILURE_LOGGED: AtomicBool = AtomicBool::new(false);

/// Undocumented `Progman` message that spawns the wallpaper `WorkerW`.
const MSG_SPAWN_WORKERW: u32 = 0x052C;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Icons live in a `WorkerW`; the wallpaper `WorkerW` is its next sibling.
    Classic,
    /// Icons are a child of `Progman` (Windows 11 24H2+).
    Raised,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HostError {
    NoProgman,
    /// Neither the Classic pair nor the Raised layout was recognised.
    NoWorkerW,
    /// Raised layout detected: not supported yet (extended scope).
    RaisedUnsupported,
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            HostError::NoProgman => "Progman window not found (is Explorer running?)",
            HostError::NoWorkerW => "wallpaper WorkerW not found (see the window tree in the log)",
            HostError::RaisedUnsupported => "Windows 11 24H2+ desktop layout is not supported yet",
        })
    }
}
impl std::error::Error for HostError {}

/// The window wallpaper windows are parented to.
#[derive(Clone, Copy, Debug)]
pub struct Host {
    pub layout: Layout,
    pub hwnd: HWND,
}

fn find_top(class: &str) -> HWND {
    let c = wide(class);
    // SAFETY: `c` is a valid NUL-terminated UTF-16 string for the call's duration.
    unsafe { FindWindowW(c.as_ptr(), null()) }
}

fn find_child(parent: HWND, class: &str) -> HWND {
    let c = wide(class);
    // SAFETY: as above; a stale `parent` just makes the call return 0.
    unsafe { FindWindowExW(parent, 0, c.as_ptr(), null()) }
}

struct Search {
    /// `Progman` itself is never the icon host in the Classic layout.
    skip: HWND,
    found: HWND,
}

unsafe extern "system" fn icon_host_cb(h: HWND, l: LPARAM) -> i32 {
    // SAFETY: `l` is the address of the caller's live `Search` (callback runs only
    // during `EnumWindows`).
    let s = unsafe { &mut *(l as *mut Search) };
    if h == s.skip || find_child(h, "SHELLDLL_DefView") == 0 {
        return 1;
    }
    let w = wide("WorkerW");
    // SAFETY: `w` is a valid NUL-terminated UTF-16 string for the call's duration.
    let next = unsafe { FindWindowExW(0, h, w.as_ptr(), null()) };
    if next != 0 {
        s.found = next;
        return 0; // stop enumerating
    }
    1
}

/// Spawn (if needed) and locate the wallpaper host, logging every step.
pub fn find_host() -> Result<Host, HostError> {
    let progman = find_top("Progman");
    log::debug!("Progman = 0x{progman:X}");
    if progman == 0 {
        return Err(HostError::NoProgman);
    }

    let mut res = 0usize;
    // SAFETY: valid window handle, valid out-pointer, bounded 1 s timeout.
    let ret = unsafe { SendMessageTimeoutW(progman, MSG_SPAWN_WORKERW, 0xD, 1, 0, 1000, &mut res) };
    log::debug!("0x052C(wParam=0xD, lParam=1) -> ret={ret} res={res}");
    if ret == 0 {
        log::warn!("0x052C failed or timed out; continuing with the current tree");
    }
    if !TREE_LOGGED.swap(true, Ordering::Relaxed) {
        dump_tree("after 0x052C (first lookup)");
    }

    let mut search = Search {
        skip: progman,
        found: 0,
    };
    // SAFETY: the callback runs only during this call and `search` outlives it.
    unsafe { EnumWindows(icon_host_cb, &mut search as *mut Search as LPARAM) };
    if search.found != 0 {
        log::info!(
            "desktop layout Classic, wallpaper WorkerW = 0x{:X}",
            search.found
        );
        return Ok(Host {
            layout: Layout::Classic,
            hwnd: search.found,
        });
    }
    if !FAILURE_LOGGED.swap(true, Ordering::Relaxed) {
        dump_tree("no Classic host found");
    }
    if find_child(progman, "SHELLDLL_DefView") != 0 {
        log::error!("desktop layout Raised (SHELLDLL_DefView under Progman): not supported yet");
        return Err(HostError::RaisedUnsupported);
    }
    log::error!("no wallpaper WorkerW found; see the window tree above");
    Err(HostError::NoWorkerW)
}

// ------------------------------------------------------------------ diagnostics

unsafe extern "system" fn collect_cb(h: HWND, l: LPARAM) -> i32 {
    // SAFETY: `l` is the address of a live `Vec<HWND>` owned by the caller.
    unsafe { (*(l as *mut Vec<HWND>)).push(h) };
    1
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: the length passed matches the buffer.
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn describe(h: HWND, indent: &str) -> String {
    let mut r = RECT::default();
    // SAFETY: a stale handle makes these calls fail harmlessly.
    let (visible, style, ex) = unsafe {
        GetWindowRect(h, &mut r);
        (
            IsWindowVisible(h) != 0,
            GetWindowLongPtrW(h, GWL_STYLE) as u32,
            GetWindowLongPtrW(h, GWL_EXSTYLE) as u32,
        )
    };
    format!(
        "{indent}hwnd=0x{h:X} class={} visible={visible} rect=({},{})-({},{}) style=0x{style:X} ex=0x{ex:X}",
        class_name(h),
        r.left,
        r.top,
        r.right,
        r.bottom
    )
}

/// Log the `Progman` / `WorkerW` top-level windows and all their descendants (debug level).
pub fn dump_tree(title: &str) {
    log::debug!("--- window tree: {title} ---");
    let mut tops: Vec<HWND> = Vec::new();
    // SAFETY: the callback runs only during this call and `tops` outlives it.
    unsafe { EnumWindows(collect_cb, &mut tops as *mut Vec<HWND> as LPARAM) };
    for &top in &tops {
        let class = class_name(top);
        if class != "Progman" && class != "WorkerW" {
            continue;
        }
        log::debug!("{}", describe(top, ""));
        let mut kids: Vec<HWND> = Vec::new();
        // SAFETY: as above, for `kids`.
        unsafe { EnumChildWindows(top, collect_cb, &mut kids as *mut Vec<HWND> as LPARAM) };
        for kid in kids {
            log::debug!("{}", describe(kid, "    "));
        }
    }
    log::debug!("--- end tree ---");
}

/// Ask Explorer to repaint the static wallpaper after our window is gone.
/// NULL path = "reload the current wallpaper"; the user's setting is not changed.
pub fn refresh_wallpaper() {
    // SAFETY: NULL pvParam is the documented "reload current wallpaper" form.
    let ok = unsafe { SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, std::ptr::null_mut(), 0) };
    log::debug!("refresh_wallpaper -> {ok}");
}
