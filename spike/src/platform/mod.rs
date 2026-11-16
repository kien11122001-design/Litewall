//! All Win32 code. `unsafe` is confined to this module tree (PRD working agreement 3).
pub mod instance;
pub mod message_window;
pub mod sys;

/// Custom window messages (WM_APP + n).
pub const WM_APP_QUIT: u32 = sys::WM_APP + 1;
/// Another instance was launched: re-read config / refresh.
pub const WM_APP_REFRESH: u32 = sys::WM_APP + 2;
/// Reserved for M3: libmpv wakeup callback -> main thread.
pub const WM_APP_MPV_EVENT: u32 = sys::WM_APP + 3;
