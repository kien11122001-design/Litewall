//! Hidden top-level window + message loop.
//!
//! Deliberately NOT a message-only window: broadcast messages (`WM_DISPLAYCHANGE`,
//! `TaskbarCreated`, `WM_POWERBROADCAST`, `WM_DPICHANGED`) are not delivered to
//! message-only windows, and M4/M6/M8 need them. It is never shown.
//!
//! Everything the application reacts to arrives as an [`AppEvent`] on the message thread.
use super::sys::*;
use super::{WM_APP_FILE_PICKED, WM_APP_MPV_EVENT, WM_APP_QUIT, WM_APP_REFRESH, WM_APP_TRAY};
use std::cell::RefCell;
use std::io;
use std::ptr::{null, null_mut};
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

pub const CLASS_NAME: &str = "LiteWall.MessageWindow";

/// One-shot timer that collapses a burst of display messages into a single event.
const TIMER_DISPLAY: usize = 1;
const DISPLAY_DEBOUNCE_MS: u32 = 400;
/// After `TaskbarCreated` Explorer is still building the desktop; wait before re-attaching.
const TIMER_EXPLORER: usize = 2;
const EXPLORER_DELAY_MS: u32 = 1500;

/// Debounce for window events (foreground, minimize, move, ...): scan once things settle.
pub const TIMER_COVERAGE: usize = 3;
pub const COVERAGE_DEBOUNCE_MS: u32 = 200;
/// One-shot timer for the pause hysteresis (armed only while a change is pending).
const TIMER_PAUSE: usize = 4;
/// One-shot timer for the "is another app playing audio?" check (armed only while LiteWall plays sound).
const TIMER_AUDIO: usize = 6;
/// 1 s fallback re-check, armed only while a fullscreen-sized window exists.
const TIMER_FULLSCREEN: usize = 5;

// Every one-shot timer needs its own id: two timers with the same id share one Windows timer
// (re-arming one replaces the other, and the first match arm swallows both).
const _: () = {
    let ids = [
        TIMER_DISPLAY,
        TIMER_EXPLORER,
        TIMER_COVERAGE,
        TIMER_PAUSE,
        TIMER_FULLSCREEN,
        TIMER_AUDIO,
    ];
    let mut i = 0;
    while i < ids.len() {
        let mut j = i + 1;
        while j < ids.len() {
            assert!(ids[i] != ids[j], "duplicate timer id");
            j += 1;
        }
        i += 1;
    }
};

/// Handle of the screen on/off registration (0 = none).
static POWER_NOTIFY: AtomicIsize = AtomicIsize::new(0);

/// The new display state (0 off, 1 on, 2 dimmed) from a `PBT_POWERSETTINGCHANGE` `lParam`.
fn display_state(lp: LPARAM) -> Option<u32> {
    if lp == 0 {
        return None;
    }
    // SAFETY: for PBT_POWERSETTINGCHANGE, `lParam` points to a POWERBROADCAST_SETTING that is
    // valid for the duration of the message; the DWORD payload may be unaligned.
    unsafe {
        let s = &*(lp as *const POWERBROADCAST_SETTING);
        if s.PowerSetting != GUID_CONSOLE_DISPLAY_STATE || s.DataLength < 4 {
            return None;
        }
        Some(std::ptr::read_unaligned(s.Data.as_ptr() as *const u32))
    }
}

/// Registered message id of `TaskbarCreated` (0 until the window exists).
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// What the application has to react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    /// libmpv has events waiting for the player with this token.
    Mpv(usize),
    /// Monitors, resolution, DPI or desktop layout changed (debounced).
    DisplayChanged,
    /// A second instance was launched.
    Refresh,
    /// Explorer was restarted (`TaskbarCreated`), debounced; the desktop host is gone.
    ExplorerRestarted,
    /// Mouse message on the notification-area icon (lParam of `WM_APP_TRAY`).
    Tray(u32),
    /// The file dialog returned a choice (read it with `dialog::take_picked`).
    FilePicked,
    /// Windows moved, minimised, shown/hidden or changed foreground (debounced).
    WindowsChanged,
    /// A pause/resume hysteresis delay has elapsed.
    PauseTick,
    /// Time to look for other apps' audio again.
    AudioCheck,
    /// Session change (lock, unlock, remote connect, ...): the `wParam` of `WM_WTSSESSION_CHANGE`.
    Session(u32),
    /// Power broadcast (suspend, resume): the `wParam` of `WM_POWERBROADCAST`.
    Power(u32),
    /// Screens switched off / on / dimmed (0 / 1 / 2).
    DisplayPower(u32),
    /// The 1 s fullscreen fallback timer fired.
    FullscreenTick,
    /// The message window is about to be destroyed: last chance to remove the tray icon.
    Quitting,
}

/// `Fn`, not `FnMut`: the handler may be re-entered while a popup menu runs its nested
/// message loop, so it keeps its mutable state behind `RefCell`s and uses `try_borrow_mut`.
type EventHandler = Rc<dyn Fn(AppEvent)>;

thread_local! {
    static HANDLER: RefCell<Option<EventHandler>> = const { RefCell::new(None) };
}

/// Install the event handler (message thread only).
pub fn set_event_handler(f: impl Fn(AppEvent) + 'static) {
    HANDLER.with(|h| *h.borrow_mut() = Some(Rc::new(f)));
}

pub fn clear_event_handler() {
    HANDLER.with(|h| *h.borrow_mut() = None);
}

fn dispatch(ev: AppEvent) {
    // Clone the Rc out first: the call below must not hold the slot borrowed.
    let handler = HANDLER.with(|h| h.borrow().clone());
    if let Some(f) = handler {
        f(ev);
    }
}

/// Arm the one-shot pause-hysteresis timer (re-arming restarts it). Message thread only.
pub fn arm_pause_timer(hwnd: HWND, ms: u32) {
    // SAFETY: own window handle on its own thread; a stale handle just fails.
    unsafe { SetTimer(hwnd, TIMER_PAUSE, ms.max(1), None) };
}

/// Arm the one-shot 1 s fullscreen fallback timer. Message thread only.
pub fn arm_fullscreen_timer(hwnd: HWND, ms: u32) {
    // SAFETY: own window handle on its own thread; a stale handle just fails.
    unsafe { SetTimer(hwnd, TIMER_FULLSCREEN, ms.max(1), None) };
}

/// Arm the one-shot audio-check timer (re-arming restarts it). Message thread only.
pub fn arm_audio_timer(hwnd: HWND, ms: u32) {
    // SAFETY: own window handle on its own thread; a stale handle just fails.
    unsafe { SetTimer(hwnd, TIMER_AUDIO, ms.max(1), None) };
}

/// Whether this process runs in a remote-desktop session.
pub fn is_remote_session() -> bool {
    // SAFETY: no preconditions.
    unsafe { GetSystemMetrics(SM_REMOTESESSION) != 0 }
}

/// Ask the message window to quit (what `--quit` and the tray's Quit item do).
pub fn request_quit(hwnd: HWND) {
    // SAFETY: posting to a window handle is safe; a stale handle just fails.
    unsafe { PostMessageW(hwnd, WM_APP_QUIT, 0, 0) };
}

pub struct MessageWindow {
    hwnd: HWND,
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_APP_QUIT | WM_CLOSE => {
            log::info!("quit requested");
            dispatch(AppEvent::Quitting);
            // SAFETY: called on the window's own thread from its wndproc.
            unsafe {
                let power = POWER_NOTIFY.swap(0, Ordering::Relaxed);
                if power != 0 {
                    UnregisterPowerSettingNotification(power);
                }
                WTSUnRegisterSessionNotification(h);
                DestroyWindow(h)
            };
            0
        }
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            0
        }
        WM_APP_REFRESH => {
            log::info!("refresh requested (second launch)");
            dispatch(AppEvent::Refresh);
            0
        }
        WM_APP_MPV_EVENT => {
            dispatch(AppEvent::Mpv(wp));
            0
        }
        WM_APP_TRAY => {
            dispatch(AppEvent::Tray(lp as u32));
            0
        }
        WM_APP_FILE_PICKED => {
            dispatch(AppEvent::FilePicked);
            0
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            log::debug!("display message 0x{m:X}: debouncing {DISPLAY_DEBOUNCE_MS} ms");
            // SAFETY: own window, own thread. Re-arming the same timer id restarts it, so
            // a burst of messages produces one event.
            unsafe { SetTimer(h, TIMER_DISPLAY, DISPLAY_DEBOUNCE_MS, None) };
            0
        }
        WM_TIMER if wp == TIMER_DISPLAY => {
            // SAFETY: as above; one-shot, so no timer is left running.
            unsafe { KillTimer(h, TIMER_DISPLAY) };
            dispatch(AppEvent::DisplayChanged);
            0
        }
        WM_TIMER if wp == TIMER_EXPLORER => {
            // SAFETY: as above.
            unsafe { KillTimer(h, TIMER_EXPLORER) };
            dispatch(AppEvent::ExplorerRestarted);
            0
        }
        WM_TIMER if wp == TIMER_COVERAGE => {
            // SAFETY: own window, own thread; one-shot.
            unsafe { KillTimer(h, TIMER_COVERAGE) };
            dispatch(AppEvent::WindowsChanged);
            0
        }
        WM_TIMER if wp == TIMER_PAUSE => {
            // SAFETY: own window, own thread; one-shot.
            unsafe { KillTimer(h, TIMER_PAUSE) };
            dispatch(AppEvent::PauseTick);
            0
        }
        WM_TIMER if wp == TIMER_FULLSCREEN => {
            // SAFETY: own window, own thread; one-shot.
            unsafe { KillTimer(h, TIMER_FULLSCREEN) };
            dispatch(AppEvent::FullscreenTick);
            0
        }
        WM_TIMER if wp == TIMER_AUDIO => {
            // SAFETY: own window, own thread; one-shot.
            unsafe { KillTimer(h, TIMER_AUDIO) };
            dispatch(AppEvent::AudioCheck);
            0
        }
        WM_WTSSESSION_CHANGE => {
            dispatch(AppEvent::Session(wp as u32));
            0
        }
        WM_POWERBROADCAST => {
            if wp == PBT_POWERSETTINGCHANGE {
                if let Some(state) = display_state(lp) {
                    dispatch(AppEvent::DisplayPower(state));
                }
            } else {
                dispatch(AppEvent::Power(wp as u32));
            }
            1 // TRUE: the broadcast is acknowledged
        }
        _ if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            log::warn!(
                "Explorer restarted (TaskbarCreated); re-attaching in {EXPLORER_DELAY_MS} ms"
            );
            // SAFETY: own window, own thread; restarts the one-shot timer on repeated broadcasts.
            unsafe { SetTimer(h, TIMER_EXPLORER, EXPLORER_DELAY_MS, None) };
            0
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

impl MessageWindow {
    pub fn create() -> io::Result<Self> {
        let class = wide(CLASS_NAME);
        // SAFETY: all pointers are valid for the duration of the calls; class/title outlive them.
        unsafe {
            let hinst = GetModuleHandleW(null());
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: 0,
                hCursor: 0,
                hbrBackground: 0,
                lpszMenuName: null(),
                lpszClassName: class.as_ptr(),
                hIconSm: 0,
            };
            if RegisterClassExW(&wc) == 0 {
                return Err(io::Error::last_os_error());
            }
            TASKBAR_CREATED.store(
                RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
                Ordering::Relaxed,
            );
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                0,
                0,
                hinst,
                null_mut(),
            );
            if hwnd == 0 {
                return Err(io::Error::last_os_error());
            }
            // Screen on/off (also fires once at registration with the current state).
            let power = RegisterPowerSettingNotification(
                hwnd,
                GUID_CONSOLE_DISPLAY_STATE.as_ptr(),
                DEVICE_NOTIFY_WINDOW_HANDLE,
            );
            if power == 0 {
                log::warn!(
                    "cannot register for screen on/off notifications: {}",
                    io::Error::last_os_error()
                );
            }
            POWER_NOTIFY.store(power, Ordering::Relaxed);
            // Lock / unlock / remote-desktop notifications for this session.
            if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
                log::warn!(
                    "cannot register for session notifications: {}",
                    io::Error::last_os_error()
                );
            }
            Ok(Self { hwnd })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Blocks in `GetMessageW` (zero idle cost) until the window is destroyed.
    /// Returns the process exit code.
    pub fn run(&self) -> i32 {
        // SAFETY: standard message loop on the thread that created the window.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            loop {
                let r = GetMessageW(&mut msg, 0, 0, 0);
                if r == 0 {
                    return msg.wParam as i32;
                }
                if r < 0 {
                    return 1;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
