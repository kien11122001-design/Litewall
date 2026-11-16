//! Grid coverage (PRD §8): split a monitor into cols×rows tiles; a tile counts as
//! covered when its centre lies inside any window rect. Pure; works with
//! negative origins because everything is in virtual-desktop coordinates.
use super::geometry::RectI32;

pub const DEFAULT_COLS: u32 = 8;
pub const DEFAULT_ROWS: u32 = 6;

/// Fraction (0.0..=1.0) of tiles covered by `windows`. Callers pass only
/// relevant windows (visible, non-minimised, non-cloaked, not ours, not shell).
pub fn covered_fraction(monitor: RectI32, windows: &[RectI32], cols: u32, rows: u32) -> f32 {
    if monitor.is_empty() || cols == 0 || rows == 0 {
        return 0.0;
    }
    let (w, h) = (monitor.width(), monitor.height());
    let (cols_i, rows_i) = (cols as i64, rows as i64);
    let mut covered = 0u32;
    for r in 0..rows_i {
        let cy = monitor.top as i64 + h * (2 * r + 1) / (2 * rows_i);
        for c in 0..cols_i {
            let cx = monitor.left as i64 + w * (2 * c + 1) / (2 * cols_i);
            if windows.iter().any(|win| win.contains_point(cx, cy)) {
                covered += 1;
            }
        }
    }
    covered as f32 / (cols * rows) as f32
}

/// `true` when the covered fraction reaches `threshold` (default 0.9).
pub fn is_covered(
    monitor: RectI32,
    windows: &[RectI32],
    cols: u32,
    rows: u32,
    threshold: f32,
) -> bool {
    covered_fraction(monitor, windows, cols, rows) >= threshold
}

/// Chromium-style shortcut: a borderless foreground window equal to (or larger
/// than) the monitor rect covers it immediately.
pub fn window_fills_monitor(monitor: RectI32, window: RectI32) -> bool {
    !monitor.is_empty()
        && window.left <= monitor.left
        && window.top <= monitor.top
        && window.right >= monitor.right
        && window.bottom >= monitor.bottom
}

/// A window whose rectangle is exactly the monitor's: borderless or exclusive fullscreen.
/// (A maximised window with a frame overhangs the monitor by a few pixels and does not count.)
pub fn window_exactly_fills(monitor: RectI32, window: RectI32) -> bool {
    !monitor.is_empty() && window == monitor
}

/// Which monitor (index into `monitors`) a true fullscreen app occupies, if any.
///
/// A fullscreen game or presentation is the foreground window, exactly fills one monitor and
/// the shell reports `QUNS_BUSY` (2) or exclusive Direct3D fullscreen (3). The window style is
/// deliberately not checked: some games keep `WS_CAPTION` set while they fill the screen
/// (seen with Touhou, style 0x94CA0000). A framed maximised window overhangs the monitor and
/// fails the exact fit, and an ordinary window leaves the state at "normal" (5).
pub fn fullscreen_app_monitor(
    monitors: &[RectI32],
    foreground: Option<RectI32>,
    state: u32,
) -> Option<usize> {
    if !matches!(state, 2 | 3) {
        return None;
    }
    let fg = foreground?;
    monitors.iter().position(|m| window_exactly_fills(*m, fg))
}

#[cfg(test)]
mod tests {
    use super::*;
    const MON: RectI32 = RectI32::new(0, 0, 1920, 1080);

    #[test]
    fn no_windows_zero() {
        assert_eq!(covered_fraction(MON, &[], 8, 6), 0.0);
    }

    #[test]
    fn full_window_is_one() {
        assert_eq!(covered_fraction(MON, &[MON], 8, 6), 1.0);
        assert!(is_covered(
            MON,
            &[RectI32::new(-10, -10, 2000, 1100)],
            8,
            6,
            0.9
        ));
    }

    #[test]
    fn left_half_is_half() {
        let half = RectI32::new(0, 0, 960, 1080);
        assert_eq!(covered_fraction(MON, &[half], 8, 6), 0.5);
        assert!(!is_covered(MON, &[half], 8, 6, 0.9));
    }

    #[test]
    fn multiple_windows_combine() {
        let l = RectI32::new(0, 0, 960, 1080);
        let r = RectI32::new(960, 0, 1920, 1080);
        assert_eq!(covered_fraction(MON, &[l, r], 8, 6), 1.0);
        // Overlap must not double count.
        assert_eq!(covered_fraction(MON, &[l, l, l], 8, 6), 0.5);
    }

    #[test]
    fn partial_overlap_and_outside() {
        let outside = RectI32::new(2000, 0, 3000, 1080);
        assert_eq!(covered_fraction(MON, &[outside], 8, 6), 0.0);
        // Window covering the top third: 2 of 6 rows.
        let top = RectI32::new(0, 0, 1920, 360);
        let f = covered_fraction(MON, &[top], 8, 6);
        assert!((f - 2.0 / 6.0).abs() < 1e-6, "{f}");
    }

    #[test]
    fn negative_origin_monitor() {
        let left_mon = RectI32::new(-1920, 0, 0, 1080);
        let win_on_main = RectI32::new(0, 0, 1920, 1080);
        assert_eq!(covered_fraction(left_mon, &[win_on_main], 8, 6), 0.0);
        let win_on_left = RectI32::new(-1920, 0, 0, 1080);
        assert_eq!(covered_fraction(left_mon, &[win_on_left], 8, 6), 1.0);
        // Straddling window covers the right half of the left monitor.
        let straddle = RectI32::new(-960, 0, 960, 1080);
        assert_eq!(covered_fraction(left_mon, &[straddle], 8, 6), 0.5);
        // Monitor above primary (negative y).
        let above = RectI32::new(0, -1080, 1920, 0);
        assert_eq!(covered_fraction(above, &[above], 8, 6), 1.0);
    }

    #[test]
    fn degenerate_inputs() {
        assert_eq!(
            covered_fraction(RectI32::new(0, 0, 0, 0), &[MON], 8, 6),
            0.0
        );
        assert_eq!(covered_fraction(MON, &[MON], 0, 6), 0.0);
    }

    #[test]
    fn exact_fullscreen_is_stricter_than_fills() {
        assert!(window_exactly_fills(MON, MON));
        assert!(!window_exactly_fills(MON, RectI32::new(-8, -8, 1928, 1088))); // maximised frame
        assert!(!window_exactly_fills(
            RectI32::new(0, 0, 0, 0),
            RectI32::new(0, 0, 0, 0)
        ));
    }

    #[test]
    fn fills_monitor_shortcut() {
        assert!(window_fills_monitor(MON, MON));
        assert!(window_fills_monitor(MON, RectI32::new(-8, -8, 1928, 1088)));
        assert!(!window_fills_monitor(MON, RectI32::new(0, 0, 1900, 1080)));
    }

    #[test]
    fn fullscreen_app_needs_an_exact_fit_and_a_busy_state() {
        let left = RectI32::new(-1920, 0, 0, 1080);
        let mons = [left, MON];
        assert_eq!(fullscreen_app_monitor(&mons, Some(MON), 2), Some(1));
        assert_eq!(fullscreen_app_monitor(&mons, Some(left), 3), Some(0));
        // Normal state (an ordinary window): not a fullscreen app.
        assert_eq!(fullscreen_app_monitor(&mons, Some(MON), 5), None);
        // Presentation mode (4) is handled separately and always pauses everything.
        assert_eq!(fullscreen_app_monitor(&mons, Some(MON), 4), None);
        // A maximised framed window overhangs the monitor.
        let framed = RectI32::new(-8, -8, 1928, 1088);
        assert_eq!(fullscreen_app_monitor(&mons, Some(framed), 2), None);
        // No foreground window, or one that fits no monitor.
        assert_eq!(fullscreen_app_monitor(&mons, None, 2), None);
        let half = RectI32::new(0, 0, 960, 1080);
        assert_eq!(fullscreen_app_monitor(&mons, Some(half), 2), None);
        assert_eq!(fullscreen_app_monitor(&[], Some(MON), 2), None);
    }
}
