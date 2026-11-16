//! Tray menu model (PRD §5): built from the config and the current monitors, with a
//! command attached to every selectable item. Pure; `platform/tray.rs` only draws it.
use super::app::{Target, UserCommand};
use super::audio::{OtherAudio, VOLUME_PRESETS};
use super::config::Config;
use super::monitor::MonitorInfo;
use super::pause::PauseScope;
use super::zoom::ZoomMode;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    /// `id` 0 is reserved for placeholders that can never be chosen.
    Item {
        id: u32,
        label: String,
        checked: bool,
        enabled: bool,
    },
    Submenu {
        label: String,
        entries: Vec<MenuEntry>,
    },
    Separator,
}

#[derive(Debug)]
pub struct BuiltMenu {
    pub entries: Vec<MenuEntry>,
    /// Menu item id -> what choosing it does.
    pub commands: HashMap<u32, UserCommand>,
}

struct Builder {
    next: u32,
    commands: HashMap<u32, UserCommand>,
}

impl Builder {
    fn item(&mut self, label: impl Into<String>, checked: bool, cmd: UserCommand) -> MenuEntry {
        let id = self.next;
        self.next += 1;
        self.commands.insert(id, cmd);
        MenuEntry::Item {
            id,
            label: label.into(),
            checked,
            enabled: true,
        }
    }
}

fn placeholder(label: &str, checked: bool) -> MenuEntry {
    MenuEntry::Item {
        id: 0,
        label: label.to_string(),
        checked,
        enabled: false,
    }
}

/// "Screen 2: 1080x1920" (+ " (primary)").
pub fn monitor_label(index: usize, m: &MonitorInfo) -> String {
    format!(
        "Screen {}: {}x{}{}",
        index + 1,
        m.rect_px.width(),
        m.rect_px.height(),
        if m.primary { " (primary)" } else { "" }
    )
}

/// "Audio" submenu: radio choice of the one screen that plays sound ("No audio" = silent
/// everywhere), then the volume presets. A volume that is not a preset is shown as a
/// checked placeholder.
fn audio_submenu(b: &mut Builder, cfg: &Config, monitors: &[MonitorInfo]) -> MenuEntry {
    let source = cfg.audio.source.as_ref();
    let mut sub = vec![b.item(
        "No audio",
        source.is_none(),
        UserCommand::SetAudioSource(None),
    )];
    for (i, m) in monitors.iter().enumerate() {
        sub.push(b.item(
            format!("Play audio from {}", monitor_label(i, m)),
            source == Some(&m.id),
            UserCommand::SetAudioSource(Some(m.id.clone())),
        ));
    }
    if let Some(id) = source {
        if !monitors.iter().any(|m| &m.id == id) {
            sub.push(placeholder("Audio screen not connected (silent)", true));
        }
    }
    sub.push(MenuEntry::Separator);
    let volume = cfg.audio.volume.min(100);
    for preset in VOLUME_PRESETS {
        let label = if preset == 0 {
            "Volume 0 % (mute)".to_string()
        } else {
            format!("Volume {preset} %")
        };
        sub.push(b.item(label, volume == preset, UserCommand::SetVolume(preset)));
    }
    if !VOLUME_PRESETS.contains(&volume) {
        sub.push(placeholder(&format!("Volume {volume} % (custom)"), true));
    }
    sub.push(MenuEntry::Separator);
    sub.push(placeholder("While other apps play audio:", false));
    let mode = cfg.audio.other_audio;
    sub.push(b.item(
        "Mute LiteWall",
        mode == OtherAudio::Mute,
        UserCommand::SetOtherAudio(OtherAudio::Mute),
    ));
    sub.push(b.item(
        format!("Lower LiteWall to {} %", cfg.audio.duck_percent.min(100)),
        mode == OtherAudio::Duck,
        UserCommand::SetOtherAudio(OtherAudio::Duck),
    ));
    sub.push(b.item(
        "Do nothing",
        mode == OtherAudio::Off,
        UserCommand::SetOtherAudio(OtherAudio::Off),
    ));
    MenuEntry::Submenu {
        label: "Audio".to_string(),
        entries: sub,
    }
}

pub fn build(cfg: &Config, monitors: &[MonitorInfo]) -> BuiltMenu {
    let mut b = Builder {
        next: 1,
        commands: HashMap::new(),
    };
    let mut entries = Vec::new();

    entries.push(b.item(
        "Choose a video for all screens\u{2026}",
        false,
        UserCommand::PickFile(Target::AllMonitors),
    ));
    entries.push(MenuEntry::Separator);

    if monitors.is_empty() {
        entries.push(placeholder("No screens detected", false));
    }
    for (i, m) in monitors.iter().enumerate() {
        let enabled = cfg.is_enabled(&m.id);
        let profile = cfg.profile(&m.id);
        let zoom = profile.map_or(ZoomMode::default(), |p| p.zoom);
        let has_own_file = profile.is_some_and(|p| p.file.is_some());

        let mut sub = vec![
            b.item(
                "Enabled",
                enabled,
                UserCommand::SetEnabled {
                    id: m.id.clone(),
                    enabled: !enabled,
                },
            ),
            b.item(
                "Choose a video for this screen\u{2026}",
                false,
                UserCommand::PickFile(Target::Monitor(m.id.clone())),
            ),
        ];
        if has_own_file {
            sub.push(b.item(
                "Use the default video",
                false,
                UserCommand::ClearMonitorFile(m.id.clone()),
            ));
        }
        sub.push(MenuEntry::Separator);
        for (label, preset) in [
            ("Fill (crop to cover)", ZoomMode::Fill),
            ("Fit (letterbox)", ZoomMode::Fit),
            ("Stretch", ZoomMode::Stretch),
            ("Original size", ZoomMode::Original),
        ] {
            sub.push(b.item(
                label,
                zoom == preset,
                UserCommand::SetZoom {
                    id: m.id.clone(),
                    zoom: preset,
                },
            ));
        }
        if matches!(zoom, ZoomMode::Custom { .. }) {
            sub.push(placeholder("Custom (edit config.toml)", true));
        }
        entries.push(MenuEntry::Submenu {
            label: monitor_label(i, m),
            entries: sub,
        });
    }

    entries.push(MenuEntry::Separator);
    entries.push(audio_submenu(&mut b, cfg, monitors));
    entries.push(MenuEntry::Separator);
    let pause_label = if cfg.general.paused_by_user {
        "Resume wallpapers"
    } else {
        "Pause wallpapers"
    };
    entries.push(b.item(pause_label, false, UserCommand::TogglePause));
    entries.push(b.item(
        "Pause all screens for fullscreen apps",
        cfg.pause.fullscreen_app_scope == PauseScope::AllMonitors,
        UserCommand::ToggleFullscreenScope,
    ));
    entries.push(b.item(
        "Start with Windows",
        cfg.general.autostart,
        UserCommand::ToggleAutostart,
    ));
    entries.push(MenuEntry::Separator);
    let advanced = vec![
        b.item("Open config folder", false, UserCommand::OpenConfigFolder),
        b.item("Reload config from file", false, UserCommand::ReloadConfig),
        b.item(
            "Log playback stats (diagnostics)",
            false,
            UserCommand::LogStats,
        ),
    ];
    entries.push(MenuEntry::Submenu {
        label: "Advanced".to_string(),
        entries: advanced,
    });
    entries.push(MenuEntry::Separator);
    entries.push(b.item("Quit", false, UserCommand::Quit));

    BuiltMenu {
        entries,
        commands: b.commands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::MonitorProfile;
    use crate::core::geometry::RectI32;
    use crate::core::monitor::MonitorId;
    use std::path::PathBuf;

    fn mon(id: &str, w: i32, h: i32, primary: bool) -> MonitorInfo {
        MonitorInfo {
            id: MonitorId::new(id),
            rect_px: RectI32::new(0, 0, w, h),
            dpi: 96,
            refresh_hz: 60.0,
            primary,
        }
    }
    fn two() -> Vec<MonitorInfo> {
        vec![mon("A", 1920, 1080, false), mon("B", 2560, 1600, true)]
    }
    fn flatten(entries: &[MenuEntry]) -> Vec<&MenuEntry> {
        let mut out = Vec::new();
        for e in entries {
            out.push(e);
            if let MenuEntry::Submenu { entries, .. } = e {
                out.extend(flatten(entries));
            }
        }
        out
    }
    fn label_of(e: &MenuEntry) -> Option<&str> {
        match e {
            MenuEntry::Item { label, .. } | MenuEntry::Submenu { label, .. } => Some(label),
            MenuEntry::Separator => None,
        }
    }
    fn find<'a>(entries: &'a [MenuEntry], label: &str) -> &'a MenuEntry {
        flatten(entries)
            .into_iter()
            .find(|e| label_of(e) == Some(label))
            .unwrap_or_else(|| panic!("no entry {label:?}"))
    }
    fn is_checked(e: &MenuEntry) -> bool {
        matches!(e, MenuEntry::Item { checked: true, .. })
    }

    #[test]
    fn top_level_has_pick_pause_autostart_and_quit() {
        let m = build(&Config::default(), &two());
        for label in [
            "Choose a video for all screens\u{2026}",
            "Pause wallpapers",
            "Pause all screens for fullscreen apps",
            "Start with Windows",
            "Open config folder",
            "Reload config from file",
            "Quit",
        ] {
            find(&m.entries, label);
        }
    }

    #[test]
    fn one_submenu_per_monitor_with_labels() {
        let m = build(&Config::default(), &two());
        let subs: Vec<_> = m
            .entries
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Submenu { label, .. } if label.starts_with("Screen ") => {
                    Some(label.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            subs,
            ["Screen 1: 1920x1080", "Screen 2: 2560x1600 (primary)"]
        );
    }

    #[test]
    fn maintenance_items_live_in_the_advanced_submenu() {
        let m = build(&Config::default(), &two());
        let advanced = m
            .entries
            .iter()
            .find_map(|e| match e {
                MenuEntry::Submenu { label, entries } if label == "Advanced" => Some(entries),
                _ => None,
            })
            .expect("Advanced submenu");
        for label in [
            "Open config folder",
            "Reload config from file",
            "Log playback stats (diagnostics)",
        ] {
            find(advanced, label);
        }
        // ... and not at the top level.
        assert!(!m
            .entries
            .iter()
            .any(|e| label_of(e) == Some("Reload config from file")));
    }

    #[test]
    fn zoom_checkmark_follows_the_profile() {
        let cfg = Config {
            monitors: vec![MonitorProfile {
                id: MonitorId::new("B"),
                enabled: true,
                file: None,
                zoom: ZoomMode::Fit,
            }],
            ..Config::default()
        };
        let m = build(&cfg, &two());
        let subs: Vec<_> = m
            .entries
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Submenu { entries, .. } => Some(entries),
                _ => None,
            })
            .collect();
        // Screen 1 has no profile: Fill. Screen 2: Fit.
        assert!(is_checked(find(subs[0], "Fill (crop to cover)")));
        assert!(!is_checked(find(subs[0], "Fit (letterbox)")));
        assert!(is_checked(find(subs[1], "Fit (letterbox)")));
        assert!(!is_checked(find(subs[1], "Fill (crop to cover)")));
    }

    #[test]
    fn own_file_adds_the_use_default_entry_only_then() {
        let none = build(&Config::default(), &two());
        assert!(!flatten(&none.entries)
            .iter()
            .any(|e| label_of(e) == Some("Use the default video")));
        let cfg = Config {
            monitors: vec![MonitorProfile {
                id: MonitorId::new("A"),
                enabled: true,
                file: Some(PathBuf::from("a.mp4")),
                zoom: ZoomMode::Fill,
            }],
            ..Config::default()
        };
        let with = build(&cfg, &two());
        let entry = find(&with.entries, "Use the default video");
        if let MenuEntry::Item { id, .. } = entry {
            assert_eq!(
                with.commands[id],
                UserCommand::ClearMonitorFile(MonitorId::new("A"))
            );
        } else {
            panic!("not an item");
        }
    }

    #[test]
    fn custom_zoom_is_shown_checked_and_disabled() {
        let cfg = Config {
            monitors: vec![MonitorProfile {
                id: MonitorId::new("A"),
                enabled: true,
                file: None,
                zoom: ZoomMode::Custom {
                    zoom_log2: 0.3,
                    pan_x: 0.0,
                    pan_y: 0.0,
                    align_x: 0.0,
                    align_y: 0.0,
                },
            }],
            ..Config::default()
        };
        let m = build(&cfg, &two());
        match find(&m.entries, "Custom (edit config.toml)") {
            MenuEntry::Item {
                id,
                checked,
                enabled,
                ..
            } => assert_eq!((*id, *checked, *enabled), (0, true, false)),
            _ => panic!("not an item"),
        }
    }

    fn audio_entries(m: &BuiltMenu) -> &Vec<MenuEntry> {
        m.entries
            .iter()
            .find_map(|e| match e {
                MenuEntry::Submenu { label, entries } if label == "Audio" => Some(entries),
                _ => None,
            })
            .expect("Audio submenu")
    }

    #[test]
    fn audio_defaults_to_no_audio_with_one_radio_item_per_screen() {
        let m = build(&Config::default(), &two());
        let a = audio_entries(&m);
        assert!(is_checked(find(a, "No audio")));
        assert!(!is_checked(find(a, "Play audio from Screen 1: 1920x1080")));
        assert!(!is_checked(find(
            a,
            "Play audio from Screen 2: 2560x1600 (primary)"
        )));
    }

    #[test]
    fn audio_source_is_the_only_checked_screen_and_sends_its_id() {
        let mut cfg = Config::default();
        cfg.audio.source = Some(MonitorId::new("B"));
        let m = build(&cfg, &two());
        let a = audio_entries(&m);
        assert!(!is_checked(find(a, "No audio")));
        assert!(!is_checked(find(a, "Play audio from Screen 1: 1920x1080")));
        let item = find(a, "Play audio from Screen 2: 2560x1600 (primary)");
        assert!(is_checked(item));
        if let MenuEntry::Item { id, .. } = item {
            assert_eq!(
                m.commands[id],
                UserCommand::SetAudioSource(Some(MonitorId::new("B")))
            );
        }
    }

    #[test]
    fn missing_audio_screen_is_flagged_and_volume_presets_work() {
        let mut cfg = Config::default();
        cfg.audio.source = Some(MonitorId::new("gone"));
        cfg.audio.volume = 50;
        let m = build(&cfg, &two());
        let a = audio_entries(&m);
        find(a, "Audio screen not connected (silent)");
        assert!(is_checked(find(a, "Volume 50 %")));
        assert!(!is_checked(find(a, "Volume 25 %")));
        if let MenuEntry::Item { id, .. } = find(a, "Volume 75 %") {
            assert_eq!(m.commands[id], UserCommand::SetVolume(75));
        }
        find(a, "Volume 0 % (mute)");
    }

    #[test]
    fn other_audio_choice_defaults_to_mute_and_follows_the_config() {
        let m = build(&Config::default(), &two());
        let a = audio_entries(&m);
        assert!(is_checked(find(a, "Mute LiteWall")));
        assert!(!is_checked(find(a, "Lower LiteWall to 20 %")));
        assert!(!is_checked(find(a, "Do nothing")));
        let mut cfg = Config::default();
        cfg.audio.other_audio = OtherAudio::Duck;
        cfg.audio.duck_percent = 35;
        let m = build(&cfg, &two());
        let a = audio_entries(&m);
        let item = find(a, "Lower LiteWall to 35 %");
        assert!(is_checked(item));
        if let MenuEntry::Item { id, .. } = item {
            assert_eq!(m.commands[id], UserCommand::SetOtherAudio(OtherAudio::Duck));
        }
    }

    #[test]
    fn non_preset_volume_shows_a_checked_custom_placeholder() {
        let m = build(&Config::default(), &two()); // default volume is 40
        let a = audio_entries(&m);
        match find(a, "Volume 40 % (custom)") {
            MenuEntry::Item {
                id,
                checked,
                enabled,
                ..
            } => assert_eq!((*id, *checked, *enabled), (0, true, false)),
            _ => panic!("not an item"),
        }
    }

    #[test]
    fn fullscreen_scope_item_is_checked_by_default_and_unchecked_for_this_screen() {
        let m = build(&Config::default(), &two());
        let item = find(&m.entries, "Pause all screens for fullscreen apps");
        assert!(is_checked(item));
        if let MenuEntry::Item { id, .. } = item {
            assert_eq!(m.commands[id], UserCommand::ToggleFullscreenScope);
        }
        let mut cfg = Config::default();
        cfg.pause.fullscreen_app_scope = PauseScope::ThisMonitor;
        let m = build(&cfg, &two());
        assert!(!is_checked(find(
            &m.entries,
            "Pause all screens for fullscreen apps"
        )));
    }

    #[test]
    fn pause_and_autostart_labels_follow_the_state() {
        let mut cfg = Config::default();
        cfg.general.paused_by_user = true;
        cfg.general.autostart = true;
        let m = build(&cfg, &two());
        find(&m.entries, "Resume wallpapers");
        assert!(is_checked(find(&m.entries, "Start with Windows")));
    }

    #[test]
    fn disabled_screen_offers_to_enable_it() {
        let cfg = Config {
            monitors: vec![MonitorProfile {
                id: MonitorId::new("A"),
                enabled: false,
                file: None,
                zoom: ZoomMode::Fill,
            }],
            ..Config::default()
        };
        let m = build(&cfg, &two());
        let subs: Vec<_> = m
            .entries
            .iter()
            .filter_map(|e| match e {
                MenuEntry::Submenu { entries, .. } => Some(entries),
                _ => None,
            })
            .collect();
        let enabled = find(subs[0], "Enabled");
        assert!(!is_checked(enabled));
        if let MenuEntry::Item { id, .. } = enabled {
            assert_eq!(
                m.commands[id],
                UserCommand::SetEnabled {
                    id: MonitorId::new("A"),
                    enabled: true
                }
            );
        }
    }

    #[test]
    fn every_selectable_item_has_a_unique_command_and_placeholders_have_none() {
        let m = build(&Config::default(), &two());
        let mut seen = std::collections::HashSet::new();
        for e in flatten(&m.entries) {
            if let MenuEntry::Item { id, enabled, .. } = e {
                if *id != 0 {
                    assert!(seen.insert(*id), "duplicate id {id}");
                    assert!(m.commands.contains_key(id));
                    assert!(*enabled);
                } else {
                    assert!(!*enabled);
                }
            }
        }
        assert_eq!(seen.len(), m.commands.len());
    }

    #[test]
    fn no_monitors_shows_a_disabled_placeholder() {
        let m = build(&Config::default(), &[]);
        match find(&m.entries, "No screens detected") {
            MenuEntry::Item { id, enabled, .. } => assert_eq!((*id, *enabled), (0, false)),
            _ => panic!("not an item"),
        }
    }
}
