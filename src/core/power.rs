//! Maps Windows session and power notifications to pause reasons (pure, unit-tested).
use super::pause::PauseReasons;

/// `wParam` values of `WM_WTSSESSION_CHANGE`.
pub const WTS_CONSOLE_CONNECT: u32 = 1;
pub const WTS_CONSOLE_DISCONNECT: u32 = 2;
pub const WTS_REMOTE_CONNECT: u32 = 3;
pub const WTS_REMOTE_DISCONNECT: u32 = 4;
pub const WTS_SESSION_LOCK: u32 = 7;
pub const WTS_SESSION_UNLOCK: u32 = 8;

/// `wParam` values of `WM_POWERBROADCAST`.
pub const PBT_APMSUSPEND: u32 = 0x4;
pub const PBT_APMRESUMESUSPEND: u32 = 0x7;
pub const PBT_APMRESUMEAUTOMATIC: u32 = 0x12;

/// Reason changes (reason, on) for a session notification. `remote_now` is whether this
/// session is a remote-desktop session right now (`GetSystemMetrics(SM_REMOTESESSION)`).
pub fn session_changes(code: u32, remote_now: bool) -> Vec<(PauseReasons, bool)> {
    match code {
        WTS_SESSION_LOCK => vec![(PauseReasons::LOCKED, true)],
        WTS_SESSION_UNLOCK => vec![(PauseReasons::LOCKED, false)],
        // Fast user switch away / a remote user taking over the console.
        WTS_CONSOLE_DISCONNECT => vec![
            (PauseReasons::LOCKED, true),
            (PauseReasons::REMOTE, remote_now),
        ],
        WTS_CONSOLE_CONNECT => vec![
            (PauseReasons::LOCKED, false),
            (PauseReasons::REMOTE, remote_now),
        ],
        WTS_REMOTE_CONNECT | WTS_REMOTE_DISCONNECT => vec![(PauseReasons::REMOTE, remote_now)],
        _ => Vec::new(),
    }
}

/// `GUID_CONSOLE_DISPLAY_STATE` values: 0 = off, 1 = on, 2 = dimmed.
/// Only "off" pauses: a dimmed screen is still visible.
pub fn display_state_change(state: u32) -> (PauseReasons, bool) {
    (PauseReasons::DISPLAY_OFF, state == 0)
}

/// `QUERY_USER_NOTIFICATION_STATE`: 3 = exclusive Direct3D fullscreen, 4 = presentation mode.
/// Only these pause every screen. `QUNS_BUSY` (2) is deliberately ignored: an ordinary
/// fullscreen window on one screen is already handled by the per-screen coverage check.
pub fn fullscreen_state_change(state: u32) -> (PauseReasons, bool) {
    (PauseReasons::FULLSCREEN, matches!(state, 3 | 4))
}

/// Reason change for a power broadcast (`None` = not relevant).
pub fn power_change(code: u32) -> Option<(PauseReasons, bool)> {
    match code {
        PBT_APMSUSPEND => Some((PauseReasons::SLEEP, true)),
        PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC => Some((PauseReasons::SLEEP, false)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_and_unlock_toggle_locked() {
        assert_eq!(
            session_changes(WTS_SESSION_LOCK, false),
            vec![(PauseReasons::LOCKED, true)]
        );
        assert_eq!(
            session_changes(WTS_SESSION_UNLOCK, false),
            vec![(PauseReasons::LOCKED, false)]
        );
    }

    #[test]
    fn remote_follows_the_session_metric() {
        assert_eq!(
            session_changes(WTS_REMOTE_CONNECT, true),
            vec![(PauseReasons::REMOTE, true)]
        );
        assert_eq!(
            session_changes(WTS_REMOTE_DISCONNECT, false),
            vec![(PauseReasons::REMOTE, false)]
        );
    }

    #[test]
    fn console_disconnect_locks_and_connect_unlocks() {
        let d = session_changes(WTS_CONSOLE_DISCONNECT, false);
        assert!(d.contains(&(PauseReasons::LOCKED, true)));
        let c = session_changes(WTS_CONSOLE_CONNECT, false);
        assert!(c.contains(&(PauseReasons::LOCKED, false)));
    }

    #[test]
    fn sleep_and_both_resume_kinds() {
        assert_eq!(
            power_change(PBT_APMSUSPEND),
            Some((PauseReasons::SLEEP, true))
        );
        assert_eq!(
            power_change(PBT_APMRESUMESUSPEND),
            Some((PauseReasons::SLEEP, false))
        );
        assert_eq!(
            power_change(PBT_APMRESUMEAUTOMATIC),
            Some((PauseReasons::SLEEP, false))
        );
    }

    #[test]
    fn only_display_off_pauses() {
        assert_eq!(display_state_change(0), (PauseReasons::DISPLAY_OFF, true));
        assert_eq!(display_state_change(1), (PauseReasons::DISPLAY_OFF, false));
        assert_eq!(display_state_change(2), (PauseReasons::DISPLAY_OFF, false));
    }

    #[test]
    fn only_exclusive_fullscreen_and_presentation_pause_everything() {
        assert_eq!(fullscreen_state_change(3), (PauseReasons::FULLSCREEN, true));
        assert_eq!(fullscreen_state_change(4), (PauseReasons::FULLSCREEN, true));
        for s in [1, 2, 5, 6, 7] {
            assert_eq!(
                fullscreen_state_change(s),
                (PauseReasons::FULLSCREEN, false),
                "{s}"
            );
        }
    }

    #[test]
    fn unrelated_codes_change_nothing() {
        assert!(session_changes(5, true).is_empty());
        assert!(session_changes(6, true).is_empty());
        assert_eq!(power_change(0xA), None);
    }
}
