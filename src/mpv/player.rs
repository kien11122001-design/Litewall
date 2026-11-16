//! One mpv instance rendering into one wallpaper window (`wid`). Safe Rust only:
//! every libmpv call goes through [`super::ffi::Handle`].
//!
//! The option profile is `CLAUDE.md` §7. Everything except `wid` is best-effort: an
//! option the pinned libmpv rejects is logged and skipped, so a newer or older build
//! degrades instead of failing to start.
use super::ffi::{Api, Event, Handle, MpvError, Wake};
use crate::core::media;
use crate::core::zoom::{self, ZoomMode};
use crate::platform::sys::HWND;
use crate::platform::WM_APP_MPV_EVENT;
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Options applied before `mpv_initialize`, in order. `wid` is added separately.
const BASE_OPTIONS: &[(&str, &str)] = &[
    // We are a wallpaper, not a player.
    ("config", "no"),
    ("load-scripts", "no"),
    ("ytdl", "no"),
    ("osc", "no"),
    ("osd-level", "0"),
    ("input-default-bindings", "no"),
    ("input-vo-keyboard", "no"),
    ("input-cursor", "no"),
    ("cursor-autohide", "always"),
    ("terminal", "no"),
    ("idle", "yes"),
    ("force-window", "yes"),
    ("stop-screensaver", "no"),
    ("keep-open", "no"),
    ("loop-file", "inf"),
    // Still images: show them for as long as the wallpaper is set.
    ("image-display-duration", "inf"),
    // Decode / render: cheapest path that still looks good (benchmarked in M3/M9).
    ("hwdec", "auto-safe"),
    ("vo", "gpu"),
    ("gpu-api", "d3d11"),
    ("gpu-context", "d3d11"),
    ("video-sync", "display-desync"),
    ("framedrop", "decoder"),
    ("interpolation", "no"),
    ("deband", "no"),
    ("dither-depth", "no"),
    ("scale", "bilinear"),
    ("cscale", "bilinear"),
    ("dscale", "bilinear"),
    ("correct-downscaling", "no"),
    ("gpu-shader-cache", "no"),
    // Memory.
    ("cache", "yes"),
    ("demuxer-max-bytes", "32MiB"),
    ("demuxer-max-back-bytes", "8MiB"),
];

#[derive(Debug)]
pub enum PlayerError {
    Mpv(MpvError),
    /// The file does not exist. `loadfile` would report success for it, so it is
    /// checked up front.
    FileMissing(PathBuf),
    /// Path is not valid UTF-8 (mpv takes UTF-8 strings).
    BadPath(PathBuf),
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlayerError::Mpv(e) => write!(f, "{e}"),
            PlayerError::FileMissing(p) => write!(f, "file not found: {}", p.display()),
            PlayerError::BadPath(p) => write!(f, "path is not valid UTF-8: {}", p.display()),
        }
    }
}
impl std::error::Error for PlayerError {}

impl From<MpvError> for PlayerError {
    fn from(e: MpvError) -> Self {
        PlayerError::Mpv(e)
    }
}

/// What the owner of a [`Player`] needs to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    /// A file started playing.
    Loaded,
    /// Playback of the current file failed (corrupt file, unsupported codec, ...).
    LoadFailed(String),
    /// mpv shut down on its own; the player is unusable.
    Shutdown,
    /// The video output is configured for the loaded file (first frame about to show).
    VideoReady,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerOptions {
    /// `false` = no audio at all (`aid=no`, no audio decode). M7 turns this on for
    /// the audio-source monitor only.
    pub audio: bool,
    /// 0..=100.
    pub volume: u8,
    /// mpv option overrides (`--opt name=value`), applied last so they win. For
    /// benchmarks and diagnostics; a rejected option is logged and skipped.
    pub extra: Vec<(String, String)>,
    /// `video-sync` mode for this screen (see `core::vsync`); `--opt` can still override it.
    pub video_sync: String,
}

impl Default for PlayerOptions {
    fn default() -> Self {
        Self {
            audio: false,
            volume: 40,
            extra: Vec::new(),
            video_sync: crate::core::vsync::DISPLAY_SYNC.to_string(),
        }
    }
}

pub struct Player {
    h: Handle,
    token: usize,
    current: Option<PathBuf>,
    /// `VideoReady` already reported for the current file.
    ready_sent: bool,
}

impl Player {
    /// Create and initialise an mpv instance that renders into `wid`.
    /// `message_hwnd` receives `WM_APP_MPV_EVENT` (wParam = `token`) whenever events
    /// are waiting; the handler must then call [`Player::drain_events`].
    pub fn new(
        api: &Rc<Api>,
        wid: HWND,
        message_hwnd: HWND,
        token: usize,
        opts: PlayerOptions,
    ) -> Result<Player, PlayerError> {
        let mut h = Handle::create(api)?;
        // `wid` must be set before init, otherwise mpv opens its own top-level window.
        h.set_option("wid", &(wid as i64).to_string())?;
        for (name, value) in BASE_OPTIONS {
            soft_option(&mut h, name, value);
        }
        soft_option(&mut h, "video-sync", &opts.video_sync);
        if opts.audio {
            soft_option(&mut h, "mute", "no");
            soft_option(&mut h, "volume", &opts.volume.min(100).to_string());
        } else {
            soft_option(&mut h, "aid", "no");
            soft_option(&mut h, "mute", "yes");
        }
        for (name, value) in &opts.extra {
            log::info!("mpv override: {name}={value}");
            soft_option(&mut h, name, value);
        }
        h.set_wake(Wake::new(message_hwnd, WM_APP_MPV_EVENT, token));
        h.initialize()?;
        if let Err(e) = h.request_log_messages("warn") {
            log::debug!("mpv log messages unavailable: {e}");
        }
        // Logged on every change: the active decoder, the video codec/size and the size of
        // mpv's own window (osd-width/height), to diagnose cropping or a wrong size.
        for name in [
            "hwdec-current",
            "video-codec",
            "width",
            "height",
            "osd-width",
            "osd-height",
        ] {
            if let Err(e) = h.observe_property(name) {
                log::debug!("cannot observe {name}: {e}");
            }
        }
        let (major, minor) = api.client_api_version();
        log::info!("mpv player created (client API {major}.{minor}, wid=0x{wid:X})");
        Ok(Player {
            h,
            token,
            current: None,
            ready_sent: false,
        })
    }

    pub fn token(&self) -> usize {
        self.token
    }

    pub fn current(&self) -> Option<&Path> {
        self.current.as_deref()
    }

    /// Cheap checks that need no mpv and no window: the file exists, is a regular file
    /// and has a UTF-8 path. Call it *before* creating a wallpaper window so a bad path
    /// never causes a window to flash up and disappear. Returns the size in bytes.
    pub fn preflight(path: &Path) -> Result<u64, PlayerError> {
        let meta = std::fs::metadata(path).map_err(|_| PlayerError::FileMissing(path.into()))?;
        if !meta.is_file() {
            return Err(PlayerError::FileMissing(path.into()));
        }
        if path.to_str().is_none() {
            return Err(PlayerError::BadPath(path.into()));
        }
        Ok(meta.len())
    }

    /// Replace whatever is playing with `path` (looping).
    pub fn load(&mut self, path: &Path) -> Result<(), PlayerError> {
        let size = Self::preflight(path)?;
        let text = path
            .to_str()
            .ok_or_else(|| PlayerError::BadPath(path.into()))?;
        if let Some(w) = media::large_animation_warning(path, size) {
            log::warn!("{}: {w}", path.display());
        }
        if !media::is_known_extension(path) {
            log::warn!("{}: unusual extension, trying anyway", path.display());
        }
        self.h.command(&["loadfile", text, "replace"])?;
        self.current = Some(path.to_path_buf());
        self.ready_sent = false;
        Ok(())
    }

    /// Apply a zoom mode. Every property is set every time, so no state goes stale.
    pub fn apply_zoom(&mut self, mode: &ZoomMode) {
        for (name, value) in zoom::properties(mode) {
            if let Err(e) = self.h.set_property(name, &value) {
                log::warn!("zoom: {name}={value} rejected: {e}");
            }
        }
    }

    pub fn set_pause(&mut self, paused: bool) -> Result<(), PlayerError> {
        Ok(self
            .h
            .set_property("pause", if paused { "yes" } else { "no" })?)
    }

    /// Switch this player's audio on or off at run time (no rebuild needed). Off means no audio
    /// track at all (`aid=no`, nothing is decoded) and muted; on selects the default track at
    /// `volume`. The order avoids a blip: mute first when going silent, unmute last when
    /// starting.
    pub fn set_audio(&mut self, on: bool, volume: u8) -> Result<(), PlayerError> {
        if on {
            self.h
                .set_property("volume", &volume.min(100).to_string())?;
            self.h.set_property("aid", "auto")?;
            self.h.set_property("mute", "no")?;
        } else {
            self.h.set_property("mute", "yes")?;
            self.h.set_property("aid", "no")?;
        }
        Ok(())
    }

    /// Change the `video-sync` mode at run time (screens can change refresh rate or become
    /// the fastest/slowest one when monitors come and go).
    pub fn set_video_sync(&mut self, mode: &str) -> Result<(), PlayerError> {
        Ok(self.h.set_property("video-sync", mode)?)
    }

    pub fn set_mute(&mut self, muted: bool) -> Result<(), PlayerError> {
        Ok(self
            .h
            .set_property("mute", if muted { "yes" } else { "no" })?)
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), PlayerError> {
        Ok(self
            .h
            .set_property("volume", &volume.min(100).to_string())?)
    }

    /// Any mpv property as text (diagnostics).
    pub fn property(&self, name: &str) -> Option<String> {
        self.h.get_property(name)
    }

    /// Which video encoders this libmpv build has (decides how a lighter copy of a heavy
    /// video can be made: libmpv's own encoding mode, or a bundled ffmpeg).
    pub fn encoders_summary(&self) -> String {
        let list = self.property("encoder-list").unwrap_or_default();
        if list.is_empty() {
            return "encoder-list unavailable".to_string();
        }
        let has = |name: &str| {
            if list.contains(&format!("\"{name}\"")) {
                "yes"
            } else {
                "no"
            }
        };
        format!(
            "libx264 {}, libx265 {}, h264_amf {}, hevc_amf {}, h264_mf {}, h264_nvenc {}, h264_qsv {}",
            has("libx264"),
            has("libx265"),
            has("h264_amf"),
            has("hevc_amf"),
            has("h264_mf"),
            has("h264_nvenc"),
            has("h264_qsv")
        )
    }

    /// One-line playback statistics for the log.
    pub fn stats(&self) -> String {
        let p = |n: &str| self.property(n).unwrap_or_else(|| "?".into());
        format!(
            "fps {} (container {}), speed {}, display {} Hz (estimated {}), video-sync {}, \
             avsync {}, vsync-ratio {}, vsync-jitter {}, dropped: vo {} decoder {}, delayed {}, \
             hwdec {}",
            p("estimated-vf-fps"),
            p("container-fps"),
            p("speed"),
            p("display-fps"),
            p("estimated-display-fps"),
            p("video-sync"),
            p("avsync"),
            p("vsync-ratio"),
            p("vsync-jitter"),
            p("frame-drop-count"),
            p("decoder-frame-drop-count"),
            p("vo-delayed-frame-count"),
            p("hwdec-current")
        )
    }

    /// Active hardware decoder (`d3d11va`, ...) or `no`/`None` when software decoding.
    pub fn hwdec_current(&self) -> Option<String> {
        self.h.get_property("hwdec-current")
    }

    /// Handle everything mpv has queued. Call from the `WM_APP_MPV_EVENT` handler.
    pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
        self.h.rearm_wake();
        let mut out = Vec::new();
        loop {
            match self.h.poll_event() {
                Event::None => break,
                Event::Shutdown => out.push(PlayerEvent::Shutdown),
                Event::Log { level, text } => match level.as_str() {
                    "fatal" | "error" => log::error!("mpv: {text}"),
                    _ => log::warn!("mpv: {text}"),
                },
                Event::StartFile => log::debug!("mpv: start-file"),
                Event::FileLoaded => {
                    log::info!(
                        "mpv#{} file loaded: {} (hwdec-current={})",
                        self.token,
                        self.current()
                            .map_or("?".into(), |p| p.display().to_string()),
                        self.hwdec_current().unwrap_or_else(|| "n/a".into())
                    );
                    out.push(PlayerEvent::Loaded);
                }
                Event::EndFile { error: Some(code) } => {
                    log::error!("playback failed (mpv error {code})");
                    out.push(PlayerEvent::LoadFailed(format!("mpv error {code}")));
                }
                Event::EndFile { error: None } => log::debug!("mpv: end-file"),
                Event::Property { name, value } => {
                    log::info!(
                        "mpv#{} property {name} = {}",
                        self.token,
                        value.as_deref().unwrap_or("(unset)")
                    );
                    if name == "osd-width"
                        && !self.ready_sent
                        && value.as_deref().is_some_and(|v| v != "0")
                    {
                        self.ready_sent = true;
                        out.push(PlayerEvent::VideoReady);
                    }
                }
                Event::Other(id) => log::trace!("mpv: event {id}"),
            }
        }
        out
    }
}

fn soft_option(h: &mut Handle, name: &str, value: &str) {
    if let Err(e) = h.set_option(name, value) {
        log::warn!("mpv option {name}={value} rejected: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_rejects_missing_and_directories() {
        let dir = std::env::temp_dir().join(format!("litewall-preflight-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(matches!(
            Player::preflight(&dir.join("nope.mp4")),
            Err(PlayerError::FileMissing(_))
        ));
        assert!(matches!(
            Player::preflight(&dir),
            Err(PlayerError::FileMissing(_))
        ));
        let f = dir.join("a.mp4");
        std::fs::write(&f, b"12345").unwrap();
        assert_eq!(Player::preflight(&f).unwrap(), 5);
        let _ = std::fs::remove_dir_all(dir);
    }
}
