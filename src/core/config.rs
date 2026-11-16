//! Versioned TOML config with migration scaffold, atomic save and corrupt-file
//! recovery (PRD §6). Pure apart from std::fs in `load_or_default`/`save_atomic`.
use super::audio::OtherAudio;
use super::monitor::MonitorId;
use super::pause::PauseScope;
use super::zoom::ZoomMode;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

pub const CURRENT_VERSION: u32 = 1;

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub general: General,
    pub audio: AudioConfig,
    pub pause: PauseConfig,
    /// "Same on all screens" source; monitors without their own `file` use it.
    pub default_profile: Option<WallpaperSpec>,
    pub monitors: Vec<MonitorProfile>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub autostart: bool,
    /// Manual pause survives restarts (USER reason is sticky).
    pub paused_by_user: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// `None` = silent everywhere.
    pub source: Option<MonitorId>,
    /// 0..=100.
    pub volume: u8,
    /// What to do while another app plays audio (default: mute LiteWall).
    pub other_audio: OtherAudio,
    /// Volume used by `other_audio = "duck"`, 0..=100.
    pub duck_percent: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PauseConfig {
    pub on_fullscreen: bool,
    pub on_battery: bool,
    pub on_lock_or_remote: bool,
    pub scope: PauseScope,
    /// What a true fullscreen app (game, presentation) pauses: every screen (default, the
    /// GPU is contended) or only the screen it is on.
    pub fullscreen_app_scope: PauseScope,
    /// Fraction of grid tiles that must be covered (0.0..=1.0).
    pub coverage_threshold: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallpaperSpec {
    pub file: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonitorProfile {
    pub id: MonitorId,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Per-monitor override; `None` = use `default_profile`.
    #[serde(default)]
    pub file: Option<PathBuf>,
    #[serde(default)]
    pub zoom: ZoomMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            general: General::default(),
            audio: AudioConfig::default(),
            pause: PauseConfig::default(),
            default_profile: None,
            monitors: Vec::new(),
        }
    }
}
impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            source: None,
            volume: 40,
            other_audio: OtherAudio::Mute,
            duck_percent: 20,
        }
    }
}
impl Default for PauseConfig {
    fn default() -> Self {
        Self {
            on_fullscreen: true,
            on_battery: false,
            on_lock_or_remote: true,
            scope: PauseScope::ThisMonitor,
            fullscreen_app_scope: PauseScope::AllMonitors,
            coverage_threshold: 0.9,
        }
    }
}

impl Config {
    pub fn profile(&self, id: &MonitorId) -> Option<&MonitorProfile> {
        self.monitors.iter().find(|m| &m.id == id)
    }

    /// File this monitor should play: its own override, else the default profile.
    pub fn resolved_file(&self, id: &MonitorId) -> Option<&Path> {
        self.profile(id)
            .and_then(|p| p.file.as_deref())
            .or_else(|| self.default_profile.as_ref().map(|d| d.file.as_path()))
    }

    /// Whether a monitor shows wallpaper. Unknown monitors default to enabled.
    pub fn is_enabled(&self, id: &MonitorId) -> bool {
        self.profile(id).map_or(true, |p| p.enabled)
    }

    /// Clamp out-of-range values (hand-edited files).
    pub fn sanitize(&mut self) {
        self.audio.volume = self.audio.volume.min(100);
        self.audio.duck_percent = self.audio.duck_percent.min(100);
        let t = self.pause.coverage_threshold;
        self.pause.coverage_threshold = if t.is_finite() {
            // Rounded so the saved file shows 0.9, not 0.8999999761581421.
            (t.clamp(0.1, 1.0) * 10_000.0).round() / 10_000.0
        } else {
            0.9
        };
    }
}

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    Parse(String),
    /// File was written by a newer LiteWall.
    UnsupportedVersion(u32),
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Parse(e) => write!(f, "config parse error: {e}"),
            ConfigError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "config version {v} is newer than supported ({CURRENT_VERSION})"
                )
            }
        }
    }
}
impl std::error::Error for ConfigError {}

/// `MIGRATIONS[i]` upgrades schema version `i+1` to `i+2` on the raw TOML value.
/// Empty while `CURRENT_VERSION == 1`; add a step when bumping the version.
const MIGRATIONS: &[fn(&mut toml::Value)] = &[];

/// Apply `steps[from-1] .. steps[to-2]` in order, stamping the version after each.
pub fn run_migrations(value: &mut toml::Value, from: u32, to: u32, steps: &[fn(&mut toml::Value)]) {
    for v in from..to {
        if let Some(step) = steps.get((v - 1) as usize) {
            step(value);
        }
        if let Some(t) = value.as_table_mut() {
            t.insert("version".into(), toml::Value::Integer(v as i64 + 1));
        }
    }
}

/// Parse + migrate + sanitize. Missing `version` is treated as 1.
pub fn parse(text: &str) -> Result<Config, ConfigError> {
    let mut value: toml::Value =
        toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
    let version = value
        .get("version")
        .and_then(|v| v.as_integer())
        .map_or(1, |v| v.clamp(1, u32::MAX as i64) as u32);
    if version > CURRENT_VERSION {
        return Err(ConfigError::UnsupportedVersion(version));
    }
    run_migrations(&mut value, version, CURRENT_VERSION, MIGRATIONS);
    let mut cfg: Config = value
        .try_into()
        .map_err(|e: toml::de::Error| ConfigError::Parse(e.to_string()))?;
    cfg.version = CURRENT_VERSION;
    cfg.sanitize();
    Ok(cfg)
}

pub fn to_toml(cfg: &Config) -> String {
    toml::to_string_pretty(cfg).expect("Config is always serializable")
}

#[derive(Debug, PartialEq)]
pub enum LoadOutcome {
    Loaded,
    /// No file yet; defaults returned (first run).
    Missing,
    /// File was unreadable/invalid; moved aside to `backup`, defaults returned.
    Recovered {
        backup: PathBuf,
        reason: String,
    },
}

pub fn load_or_default(path: &Path) -> (Config, LoadOutcome) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (Config::default(), LoadOutcome::Missing)
        }
        Err(e) => return recover(path, e.to_string()),
    };
    match parse(&text) {
        Ok(c) => (c, LoadOutcome::Loaded),
        Err(e) => recover(path, e.to_string()),
    }
}

fn recover(path: &Path, reason: String) -> (Config, LoadOutcome) {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bad");
    let backup = PathBuf::from(backup);
    let _ = std::fs::remove_file(&backup);
    let _ = std::fs::rename(path, &backup);
    (Config::default(), LoadOutcome::Recovered { backup, reason })
}

/// Write to a temp file in the same directory, then rename over the target.
pub fn save_atomic(cfg: &Config, path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, to_toml(cfg))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("litewall-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const PRD_EXAMPLE: &str = r#"
version = 1

[general]
autostart = true

[audio]
source = "MONITOR\\DEL4177\\{x}"
volume = 40

[pause]
on_fullscreen = true
on_battery = false
on_lock_or_remote = true
scope = "this_monitor"

[default_profile]
file = "D:\\Videos\\rain.mp4"

[[monitors]]
id = "MONITOR\\DEL4177\\{x}"
enabled = true
zoom = { mode = "fill" }

[[monitors]]
id = "MONITOR\\GSM5B09\\{y}"
enabled = true
file = "D:\\Videos\\forest_1080.mp4"
zoom = { mode = "custom", zoom_log2 = 0.15, pan_x = 0.0, pan_y = -0.05 }
"#;

    #[test]
    fn parses_the_prd_example() {
        let c = parse(PRD_EXAMPLE).unwrap();
        assert!(c.general.autostart);
        assert_eq!(c.audio.volume, 40);
        assert_eq!(c.monitors.len(), 2);
        assert_eq!(c.monitors[0].zoom, ZoomMode::Fill);
        assert_eq!(
            c.monitors[1].zoom,
            ZoomMode::Custom {
                zoom_log2: 0.15,
                pan_x: 0.0,
                pan_y: -0.05,
                align_x: 0.0,
                align_y: 0.0
            }
        );
        let a = MonitorId::new("MONITOR\\DEL4177\\{x}");
        let b = MonitorId::new("MONITOR\\GSM5B09\\{y}");
        assert_eq!(c.resolved_file(&a), Some(Path::new("D:\\Videos\\rain.mp4")));
        assert_eq!(
            c.resolved_file(&b),
            Some(Path::new("D:\\Videos\\forest_1080.mp4"))
        );
        assert_eq!(
            c.resolved_file(&MonitorId::new("other")),
            Some(Path::new("D:\\Videos\\rain.mp4"))
        );
    }

    #[test]
    fn round_trip() {
        let mut c = parse(PRD_EXAMPLE).unwrap();
        c.general.paused_by_user = true;
        c.monitors[0].enabled = false;
        let again = parse(&to_toml(&c)).unwrap();
        assert_eq!(c, again);
        let d = Config::default();
        assert_eq!(parse(&to_toml(&d)).unwrap(), d);
    }

    #[test]
    fn empty_file_gives_defaults_and_missing_version_is_v1() {
        assert_eq!(parse("").unwrap(), Config::default());
        assert!(
            parse("[general]\nautostart = true\n")
                .unwrap()
                .general
                .autostart
        );
    }

    #[test]
    fn unknown_keys_are_ignored_and_values_sanitised() {
        let c = parse("mystery = 1\n[audio]\nvolume = 250\n[pause]\ncoverage_threshold = 5.0\n")
            .unwrap();
        assert_eq!(c.audio.volume, 100);
        assert_eq!(c.pause.coverage_threshold, 1.0);
    }

    #[test]
    fn newer_version_is_rejected_and_garbage_is_a_parse_error() {
        assert_eq!(
            parse("version = 99\n"),
            Err(ConfigError::UnsupportedVersion(99))
        );
        assert!(matches!(
            parse("this is [not toml"),
            Err(ConfigError::Parse(_))
        ));
        assert!(matches!(
            parse("[audio]\nvolume = \"loud\"\n"),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn migration_scaffold_runs_steps_in_order() {
        fn v1_to_v2(v: &mut toml::Value) {
            let t = v.as_table_mut().unwrap();
            if let Some(old) = t.remove("old_name") {
                t.insert("new_name".into(), old);
            }
        }
        fn v2_to_v3(v: &mut toml::Value) {
            v.as_table_mut()
                .unwrap()
                .insert("added_in_v3".into(), toml::Value::Boolean(true));
        }
        let mut val: toml::Value = toml::from_str("version = 1\nold_name = 7\n").unwrap();
        run_migrations(&mut val, 1, 3, &[v1_to_v2, v2_to_v3]);
        assert_eq!(val["version"].as_integer(), Some(3));
        assert_eq!(val["new_name"].as_integer(), Some(7));
        assert!(val.get("old_name").is_none());
        assert_eq!(val["added_in_v3"].as_bool(), Some(true));
        // from == to is a no-op.
        let mut same: toml::Value = toml::from_str("version = 3\n").unwrap();
        run_migrations(&mut same, 3, 3, &[v1_to_v2, v2_to_v3]);
        assert_eq!(same["version"].as_integer(), Some(3));
    }

    #[test]
    fn atomic_save_and_load() {
        let d = tmp_dir("save");
        let p = d.join("sub").join("config.toml");
        let mut c = Config::default();
        c.audio.volume = 75;
        save_atomic(&c, &p).unwrap();
        assert!(!d.join("sub").join("config.toml.tmp").exists());
        assert_eq!(load_or_default(&p), (c.clone(), LoadOutcome::Loaded));
        c.audio.volume = 10;
        save_atomic(&c, &p).unwrap(); // overwrite existing
        assert_eq!(load_or_default(&p).0.audio.volume, 10);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn missing_file_and_corrupt_file() {
        let d = tmp_dir("load");
        let p = d.join("config.toml");
        assert_eq!(
            load_or_default(&p),
            (Config::default(), LoadOutcome::Missing)
        );
        std::fs::write(&p, "garbage [[[").unwrap();
        let (c, o) = load_or_default(&p);
        assert_eq!(c, Config::default());
        match o {
            LoadOutcome::Recovered { backup, .. } => {
                assert_eq!(backup, d.join("config.toml.bad"));
                assert!(backup.exists());
                assert!(!p.exists());
            }
            other => panic!("unexpected {other:?}"),
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn fullscreen_app_scope_defaults_to_all_screens_and_round_trips() {
        assert_eq!(
            Config::default().pause.fullscreen_app_scope,
            PauseScope::AllMonitors
        );
        // An older file without the key gets the default.
        let old = parse("[pause]\nscope = \"this_monitor\"\n").unwrap();
        assert_eq!(old.pause.fullscreen_app_scope, PauseScope::AllMonitors);
        let mut c = Config::default();
        c.pause.fullscreen_app_scope = PauseScope::ThisMonitor;
        let again = parse(&to_toml(&c)).unwrap();
        assert_eq!(again.pause.fullscreen_app_scope, PauseScope::ThisMonitor);
    }

    #[test]
    fn other_audio_settings_default_to_mute_and_parse() {
        let d = Config::default();
        assert_eq!(d.audio.other_audio, OtherAudio::Mute);
        assert_eq!(d.audio.duck_percent, 20);
        let c = parse("[audio]\nother_audio = \"duck\"\nduck_percent = 35\n").unwrap();
        assert_eq!(c.audio.other_audio, OtherAudio::Duck);
        assert_eq!(c.audio.duck_percent, 35);
        let off = parse("[audio]\nother_audio = \"off\"\nduck_percent = 250\n").unwrap();
        assert_eq!(off.audio.other_audio, OtherAudio::Off);
        assert_eq!(off.audio.duck_percent, 100);
        assert_eq!(parse(&to_toml(&c)).unwrap(), c);
    }

    #[test]
    fn enabled_defaults() {
        let c = Config::default();
        assert!(c.is_enabled(&MonitorId::new("x")));
        let c = parse("[[monitors]]\nid = \"m\"\nenabled = false\n").unwrap();
        assert!(!c.is_enabled(&MonitorId::new("m")));
        assert_eq!(c.monitors[0].zoom, ZoomMode::Fill);
    }
}
