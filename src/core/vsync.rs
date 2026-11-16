//! Which `video-sync` mode a screen's player should use (pure, unit-tested).
//!
//! Measured on the owner's machine (2026-11-15): Windows composes every screen on the clock of
//! the fastest one, so on a 60 Hz screen next to a 120 Hz screen mpv measures about 119 Hz
//! (`estimated-display-fps`) while the screen is configured for 60 Hz. `display-desync` times
//! frames against the measured rate and the video then plays about 2x too fast (1.29x-1.97x
//! seen). `desync` times frames against the system clock and played at 1.00x on every screen,
//! with the audio in step (`avsync` about -0.01 s).
//!
//! So the fastest screen keeps `display-desync` (its measurement is right, and it is the mode
//! chosen in D12/D14), and every screen that is slower than the fastest uses `desync`.

/// The default mode (benchmarked in M3, D12).
pub const DISPLAY_SYNC: &str = "display-desync";
/// Clock-paced mode for screens whose refresh rate is not the compositor's rate.
pub const CLOCK_SYNC: &str = "desync";

/// `refresh_hz` is the screen's rate as Windows reports it (0.0 = unknown), `max_hz` the
/// highest rate among the screens that are playing (0.0 = unknown). Unknown rates keep the
/// default, and a difference below 1 Hz counts as equal (59.94 vs 60).
pub fn video_sync_for(refresh_hz: f32, max_hz: f32) -> &'static str {
    if refresh_hz > 0.0 && max_hz > 0.0 && refresh_hz + 1.0 < max_hz {
        CLOCK_SYNC
    } else {
        DISPLAY_SYNC
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fastest_screen_keeps_display_sync_and_slower_ones_use_the_clock() {
        assert_eq!(video_sync_for(120.0, 120.0), DISPLAY_SYNC);
        assert_eq!(video_sync_for(60.0, 120.0), CLOCK_SYNC);
        assert_eq!(video_sync_for(144.0, 144.0), DISPLAY_SYNC);
        assert_eq!(video_sync_for(60.0, 144.0), CLOCK_SYNC);
    }

    #[test]
    fn equal_screens_all_keep_display_sync() {
        assert_eq!(video_sync_for(60.0, 60.0), DISPLAY_SYNC);
        assert_eq!(video_sync_for(59.94, 60.0), DISPLAY_SYNC);
    }

    #[test]
    fn unknown_rates_keep_the_default() {
        assert_eq!(video_sync_for(0.0, 120.0), DISPLAY_SYNC);
        assert_eq!(video_sync_for(60.0, 0.0), DISPLAY_SYNC);
    }
}
