//! ZoomMode -> mpv property set (pure). Every mode sets *every* property so that
//! switching modes can never leave stale state (PRD §7).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ZoomMode {
    /// Keep aspect, crop overflow (`panscan=1`).
    #[default]
    Fill,
    /// Keep aspect, letterbox (`panscan=0`).
    Fit,
    /// Ignore aspect (`keepaspect=no`).
    Stretch,
    /// 1:1 pixels (`video-unscaled=yes`).
    Original,
    /// Manual adjustment on top of Fill. `zoom_log2`: size × 2^zoom.
    Custom {
        #[serde(default)]
        zoom_log2: f32,
        #[serde(default)]
        pan_x: f32,
        #[serde(default)]
        pan_y: f32,
        #[serde(default)]
        align_x: f32,
        #[serde(default)]
        align_y: f32,
    },
}

/// The exact property names this module always sets, in order.
pub const PROPERTY_NAMES: [&str; 8] = [
    "keepaspect",
    "panscan",
    "video-unscaled",
    "video-zoom",
    "video-pan-x",
    "video-pan-y",
    "video-align-x",
    "video-align-y",
];

fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if !v.is_finite() {
        return 0.0;
    }
    let c = v.clamp(lo, hi);
    if c == 0.0 {
        0.0 // normalise -0.0
    } else {
        c
    }
}

fn num(v: f32) -> String {
    format!("{v}")
}

/// Properties to apply (in order) for `mode`. Ranges follow mpv: zoom −20..20,
/// pan −3..3, align −1..1.
pub fn properties(mode: &ZoomMode) -> Vec<(&'static str, String)> {
    // (keepaspect, panscan, unscaled, zoom, pan_x, pan_y, align_x, align_y)
    let (keep, panscan, unscaled, zoom, px, py, ax, ay) = match *mode {
        ZoomMode::Fill => (true, 1.0, false, 0.0, 0.0, 0.0, 0.0, 0.0),
        ZoomMode::Fit => (true, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0),
        ZoomMode::Stretch => (false, 0.0, false, 0.0, 0.0, 0.0, 0.0, 0.0),
        ZoomMode::Original => (true, 0.0, true, 0.0, 0.0, 0.0, 0.0, 0.0),
        ZoomMode::Custom {
            zoom_log2,
            pan_x,
            pan_y,
            align_x,
            align_y,
        } => (
            true,
            1.0,
            false,
            clamp(zoom_log2, -20.0, 20.0),
            clamp(pan_x, -3.0, 3.0),
            clamp(pan_y, -3.0, 3.0),
            clamp(align_x, -1.0, 1.0),
            clamp(align_y, -1.0, 1.0),
        ),
    };
    let yn = |b: bool| if b { "yes" } else { "no" }.to_string();
    vec![
        ("keepaspect", yn(keep)),
        ("panscan", num(panscan)),
        ("video-unscaled", yn(unscaled)),
        ("video-zoom", num(zoom)),
        ("video-pan-x", num(px)),
        ("video-pan-y", num(py)),
        ("video-align-x", num(ax)),
        ("video-align-y", num(ay)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(p: &'a [(&'static str, String)], k: &str) -> &'a str {
        &p.iter().find(|(n, _)| *n == k).expect("property present").1
    }

    #[test]
    fn every_mode_sets_every_property() {
        let modes = [
            ZoomMode::Fill,
            ZoomMode::Fit,
            ZoomMode::Stretch,
            ZoomMode::Original,
            ZoomMode::Custom {
                zoom_log2: 0.5,
                pan_x: 0.1,
                pan_y: -0.1,
                align_x: 1.0,
                align_y: -1.0,
            },
        ];
        for m in modes {
            let names: Vec<_> = properties(&m).iter().map(|(n, _)| *n).collect();
            assert_eq!(names, PROPERTY_NAMES, "{m:?}");
        }
    }

    #[test]
    fn basic_modes() {
        let p = properties(&ZoomMode::Fill);
        assert_eq!((get(&p, "keepaspect"), get(&p, "panscan")), ("yes", "1"));
        let p = properties(&ZoomMode::Fit);
        assert_eq!((get(&p, "keepaspect"), get(&p, "panscan")), ("yes", "0"));
        let p = properties(&ZoomMode::Stretch);
        assert_eq!(get(&p, "keepaspect"), "no");
        let p = properties(&ZoomMode::Original);
        assert_eq!(get(&p, "video-unscaled"), "yes");
    }

    #[test]
    fn switching_from_custom_resets_everything() {
        let custom = properties(&ZoomMode::Custom {
            zoom_log2: 1.5,
            pan_x: 0.3,
            pan_y: 0.2,
            align_x: 1.0,
            align_y: 1.0,
        });
        assert_eq!(get(&custom, "video-zoom"), "1.5");
        let fit = properties(&ZoomMode::Fit);
        for k in [
            "video-zoom",
            "video-pan-x",
            "video-pan-y",
            "video-align-x",
            "video-align-y",
        ] {
            assert_eq!(get(&fit, k), "0", "{k} not reset");
        }
        assert_eq!(get(&fit, "video-unscaled"), "no");
        // Original -> Fill must clear video-unscaled.
        assert_eq!(get(&properties(&ZoomMode::Fill), "video-unscaled"), "no");
    }

    #[test]
    fn custom_values_are_clamped_and_sanitised() {
        let p = properties(&ZoomMode::Custom {
            zoom_log2: 99.0,
            pan_x: -50.0,
            pan_y: f32::NAN,
            align_x: 7.0,
            align_y: -0.0,
        });
        assert_eq!(get(&p, "video-zoom"), "20");
        assert_eq!(get(&p, "video-pan-x"), "-3");
        assert_eq!(get(&p, "video-pan-y"), "0");
        assert_eq!(get(&p, "video-align-x"), "1");
        assert_eq!(get(&p, "video-align-y"), "0");
        assert_eq!(get(&p, "panscan"), "1");
    }
}
