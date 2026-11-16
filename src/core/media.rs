//! Media-file helpers that need no Win32 and no libmpv (pure, unit-tested).
//!
//! * `parse_zoom`: CLI / test-harness spelling of a [`ZoomMode`].
//! * `large_animation_warning`: F-02, GIF/WebP are decoded in software and get
//!   heavy quickly, so large ones deserve a warning.
use super::zoom::ZoomMode;
use std::path::Path;

/// Animated-image files above this size trigger a warning (20 MB).
pub const LARGE_ANIMATION_BYTES: u64 = 20 * 1024 * 1024;

/// Extensions the MVP accepts (PRD F-01, F-02). Informational: libmpv decides in the end.
pub const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "mkv", "webm", "mov", "m4v", "avi"];
/// Still images (libmpv shows them for as long as the wallpaper is set). `webp` and `gif`
/// are listed under animations because they may move.
pub const IMAGE_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "bmp", "tif", "tiff", "avif", "jxl"];
pub const ANIMATION_EXTENSIONS: [&str; 2] = ["gif", "webp"];

fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

pub fn is_animation(path: &Path) -> bool {
    extension_lower(path).is_some_and(|e| ANIMATION_EXTENSIONS.contains(&e.as_str()))
}

pub fn is_image(path: &Path) -> bool {
    extension_lower(path).is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
}

pub fn is_known_extension(path: &Path) -> bool {
    extension_lower(path).is_some_and(|e| {
        VIDEO_EXTENSIONS.contains(&e.as_str())
            || ANIMATION_EXTENSIONS.contains(&e.as_str())
            || IMAGE_EXTENSIONS.contains(&e.as_str())
    })
}

fn patterns(exts: &[&str]) -> String {
    exts.iter()
        .map(|e| format!("*.{e}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// (name, pattern list) pairs for the file dialog; the first one is the default.
pub fn dialog_filters() -> Vec<(String, String)> {
    let all: Vec<&str> = VIDEO_EXTENSIONS
        .iter()
        .chain(ANIMATION_EXTENSIONS.iter())
        .chain(IMAGE_EXTENSIONS.iter())
        .copied()
        .collect();
    vec![
        ("All supported wallpapers".to_string(), patterns(&all)),
        ("Videos".to_string(), patterns(&VIDEO_EXTENSIONS)),
        (
            "Animations (GIF, WebP)".to_string(),
            patterns(&ANIMATION_EXTENSIONS),
        ),
        ("Images".to_string(), patterns(&IMAGE_EXTENSIONS)),
        ("All files".to_string(), "*.*".to_string()),
    ]
}

/// `Some(text)` when `path` is a GIF/WebP larger than [`LARGE_ANIMATION_BYTES`].
pub fn large_animation_warning(path: &Path, size_bytes: u64) -> Option<&'static str> {
    if is_animation(path) && size_bytes > LARGE_ANIMATION_BYTES {
        Some("Large GIF/WebP animations use a lot of CPU and memory; converting to MP4 is much lighter.")
    } else {
        None
    }
}

/// `Some(text)` when a video is unusually heavy to decode as a wallpaper: 4K at 30 fps or
/// more, or an average bitrate of 80 Mbps or more. Decoding it once per screen can overload
/// the GPU's video engine (an iGPU in particular) and show up as stutter.
pub fn heavy_video_warning(width: u32, height: u32, fps: f64, bitrate_bps: f64) -> Option<String> {
    let pixels = u64::from(width) * u64::from(height);
    let mbps = bitrate_bps / 1_000_000.0;
    if mbps >= 80.0 {
        return Some(format!(
            "An average bitrate of about {mbps:.0} Mbps ({width}x{height}, {fps:.0} fps) is very \
             heavy for a wallpaper. 10-30 Mbps looks the same and plays smoother."
        ));
    }
    if pixels >= 3840 * 2160 && fps >= 29.0 {
        return Some(format!(
            "{width}x{height} at {fps:.0} fps is heavy to decode, and every screen decodes its own \
             copy, so several screens can stutter. A screen-sized copy (1080p or 1440p) looks the \
             same and runs smoother."
        ));
    }
    None
}

/// `fill | fit | stretch | original | custom[:zoom_log2[,pan_x[,pan_y]]]` (case-insensitive).
/// `None` for anything unrecognised or a non-numeric custom value.
pub fn parse_zoom(s: &str) -> Option<ZoomMode> {
    let s = s.trim().to_ascii_lowercase();
    let (name, args) = match s.split_once(':') {
        Some((n, a)) => (n, Some(a)),
        None => (s.as_str(), None),
    };
    match (name, args) {
        ("fill", None) => Some(ZoomMode::Fill),
        ("fit", None) => Some(ZoomMode::Fit),
        ("stretch", None) => Some(ZoomMode::Stretch),
        ("original", None) => Some(ZoomMode::Original),
        ("custom", args) => {
            let mut v = [0.0f32; 3];
            if let Some(a) = args {
                for (slot, part) in v.iter_mut().zip(a.split(',')) {
                    *slot = part.trim().parse().ok().filter(|x: &f32| x.is_finite())?;
                }
                if a.split(',').count() > 3 {
                    return None;
                }
            }
            Some(ZoomMode::Custom {
                zoom_log2: v[0],
                pan_x: v[1],
                pan_y: v[2],
                align_x: 0.0,
                align_y: 0.0,
            })
        }
        _ => None,
    }
}

/// `name=value` for the `--opt` CLI flag (mpv option override, used for benchmarks).
/// `None` for a missing `=`, an empty or whitespace-containing name, or `wid`
/// (the window id is owned by LiteWall and must not be overridden).
pub fn parse_opt(s: &str) -> Option<(String, String)> {
    let (name, value) = s.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || name.contains(char::is_whitespace) || name.eq_ignore_ascii_case("wid") {
        return None;
    }
    Some((name.to_string(), value.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn opt_parsing() {
        let ok = |n: &str, v: &str| Some((n.to_string(), v.to_string()));
        assert_eq!(parse_opt("vo=gpu"), ok("vo", "gpu"));
        assert_eq!(
            parse_opt(" hwdec = d3d11va-copy "),
            ok("hwdec", "d3d11va-copy")
        );
        assert_eq!(parse_opt("gpu-shader-cache="), ok("gpu-shader-cache", ""));
        assert_eq!(parse_opt("a=b=c"), ok("a", "b=c"));
        for bad in ["", "vo", "=gpu", "wid=1", "WID=1", "two words=1"] {
            assert_eq!(parse_opt(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn simple_modes_parse_case_insensitively() {
        assert_eq!(parse_zoom("fill"), Some(ZoomMode::Fill));
        assert_eq!(parse_zoom(" FIT "), Some(ZoomMode::Fit));
        assert_eq!(parse_zoom("Stretch"), Some(ZoomMode::Stretch));
        assert_eq!(parse_zoom("original"), Some(ZoomMode::Original));
    }

    #[test]
    fn custom_with_and_without_values() {
        let c = |z, x, y| {
            Some(ZoomMode::Custom {
                zoom_log2: z,
                pan_x: x,
                pan_y: y,
                align_x: 0.0,
                align_y: 0.0,
            })
        };
        assert_eq!(parse_zoom("custom"), c(0.0, 0.0, 0.0));
        assert_eq!(parse_zoom("custom:0.5"), c(0.5, 0.0, 0.0));
        assert_eq!(parse_zoom("custom:0.15,0,-0.05"), c(0.15, 0.0, -0.05));
        assert_eq!(parse_zoom("custom: 1 , 0.1 "), c(1.0, 0.1, 0.0));
    }

    #[test]
    fn bad_input_is_rejected() {
        for bad in [
            "",
            "zoom",
            "fill:1",
            "custom:abc",
            "custom:1,2,3,4",
            "custom:nan",
            "custom:1,,2",
        ] {
            assert_eq!(parse_zoom(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn extension_checks() {
        assert!(is_animation(&PathBuf::from("a/b/Loop.GIF")));
        assert!(is_animation(&PathBuf::from("x.webp")));
        assert!(!is_animation(&PathBuf::from("x.mp4")));
        assert!(is_known_extension(&PathBuf::from("x.MKV")));
        assert!(!is_known_extension(&PathBuf::from("x.exe")));
        assert!(!is_known_extension(&PathBuf::from("noext")));
    }

    #[test]
    fn heavy_videos_are_flagged_and_normal_ones_are_not() {
        let w = heavy_video_warning(3840, 2160, 30.0, 200_000_000.0).unwrap();
        assert!(w.contains("3840x2160") && w.contains("200 Mbps"), "{w}");
        let size_only = heavy_video_warning(3840, 2160, 29.97, 20_000_000.0).unwrap(); // 4K30
        assert!(
            size_only.contains("3840x2160") && !size_only.contains("Mbps"),
            "{size_only}"
        );
        assert!(heavy_video_warning(1920, 1080, 30.0, 100_000_000.0).is_some()); // bitrate
        assert!(heavy_video_warning(1920, 1080, 60.0, 15_000_000.0).is_none());
        assert!(heavy_video_warning(3840, 2160, 24.0, 20_000_000.0).is_none());
    }

    #[test]
    fn images_are_known_and_distinct_from_animations() {
        assert!(is_image(&PathBuf::from("a/b/Photo.JPG")));
        assert!(is_image(&PathBuf::from("x.png")));
        assert!(!is_image(&PathBuf::from("x.gif")));
        assert!(!is_image(&PathBuf::from("x.mp4")));
        assert!(is_known_extension(&PathBuf::from("x.png")));
        assert!(is_known_extension(&PathBuf::from("x.avi")));
    }

    #[test]
    fn dialog_filters_list_everything_first_and_all_files_last() {
        let f = dialog_filters();
        assert_eq!(f[0].0, "All supported wallpapers");
        for p in ["*.mp4", "*.gif", "*.webp", "*.png", "*.jpg"] {
            assert!(f[0].1.contains(p), "{p} missing from {}", f[0].1);
        }
        assert!(!f[0].1.contains(' '));
        assert_eq!(f.last().unwrap().1, "*.*");
    }

    #[test]
    fn warns_only_for_large_animations() {
        let gif = PathBuf::from("big.gif");
        assert!(large_animation_warning(&gif, LARGE_ANIMATION_BYTES + 1).is_some());
        assert!(large_animation_warning(&gif, LARGE_ANIMATION_BYTES).is_none());
        assert!(large_animation_warning(&gif, 1024).is_none());
        let mp4 = PathBuf::from("big.mp4");
        assert!(large_animation_warning(&mp4, 10 * LARGE_ANIMATION_BYTES).is_none());
    }
}
