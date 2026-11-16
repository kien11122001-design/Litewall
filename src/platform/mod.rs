//! All Win32 code. `unsafe` is confined to this module tree (PRD working agreement 3).
pub mod audio_sessions;
pub mod autostart;
pub mod desktop;
pub mod diag;
pub mod dialog;
pub mod events;
pub mod fullscreen;
pub mod instance;
pub mod message_window;
pub mod monitors;
pub mod shell;
pub mod sys;
pub mod tray;
pub mod window;

/// Custom window messages (WM_APP + n).
pub const WM_APP_QUIT: u32 = sys::WM_APP + 1;
/// Another instance was launched: re-read config / refresh.
pub const WM_APP_REFRESH: u32 = sys::WM_APP + 2;
/// libmpv wakeup callback -> main thread.
pub const WM_APP_MPV_EVENT: u32 = sys::WM_APP + 3;
/// Notification-area icon callback (lParam = the mouse message).
pub const WM_APP_TRAY: u32 = sys::WM_APP + 4;
/// The file dialog thread has a choice waiting in `dialog::take_picked`.
pub const WM_APP_FILE_PICKED: u32 = sys::WM_APP + 5;
