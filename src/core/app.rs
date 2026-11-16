//! Application state machine (PRD §5): user commands in, effects out. Pure: the platform
//! layer executes the effects, so every config edit made from the tray is unit-tested here.
use super::audio::OtherAudio;
use super::config::{Config, MonitorProfile, WallpaperSpec};
use super::monitor::MonitorId;
use super::pause::PauseScope;
use super::zoom::ZoomMode;
use std::path::PathBuf;

/// Which screens a picked file applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    AllMonitors,
    Monitor(MonitorId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum UserCommand {
    /// Open the file dialog for `Target`.
    PickFile(Target),
    /// The dialog returned a file.
    FilePicked {
        target: Target,
        path: PathBuf,
    },
    /// Go back to the default video on one screen.
    ClearMonitorFile(MonitorId),
    SetEnabled {
        id: MonitorId,
        enabled: bool,
    },
    SetZoom {
        id: MonitorId,
        zoom: ZoomMode,
    },
    TogglePause,
    ToggleAutostart,
    /// Play audio from this screen only (`None` = silent everywhere).
    SetAudioSource(Option<MonitorId>),
    /// Volume of the audio source, 0..=100.
    SetVolume(u8),
    /// What to do while another app plays audio.
    SetOtherAudio(OtherAudio),
    /// Flip what a fullscreen app pauses: all screens <-> only its own screen.
    ToggleFullscreenScope,
    OpenConfigFolder,
    /// Write per-screen playback statistics (dropped frames, fps) to the log.
    LogStats,
    /// Ask for a re-read of the config file.
    ReloadConfig,
    /// The config file was re-read (result of `Effect::ReloadConfigFromDisk`).
    ConfigReloaded(Config),
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    SaveConfig,
    /// Re-sync the running wallpapers with the config.
    ApplyConfig,
    ShowFileDialog(Target),
    SetAutostart(bool),
    SetPaused(bool),
    OpenConfigFolder,
    LogStats,
    ReloadConfigFromDisk,
    Quit,
}

pub struct App {
    config: Config,
}

/// The monitor's profile, created (enabled, default zoom, no own file) when it has none.
fn profile_mut<'a>(cfg: &'a mut Config, id: &MonitorId) -> &'a mut MonitorProfile {
    let idx = match cfg.monitors.iter().position(|p| &p.id == id) {
        Some(i) => i,
        None => {
            cfg.monitors.push(MonitorProfile {
                id: id.clone(),
                enabled: true,
                file: None,
                zoom: ZoomMode::default(),
            });
            cfg.monitors.len() - 1
        }
    };
    &mut cfg.monitors[idx]
}

fn save_and_apply() -> Vec<Effect> {
    vec![Effect::SaveConfig, Effect::ApplyConfig]
}

impl App {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Apply a command to the state and say what the outside world has to do about it.
    /// A command that changes nothing returns no effects.
    pub fn update(&mut self, cmd: UserCommand) -> Vec<Effect> {
        match cmd {
            UserCommand::PickFile(target) => vec![Effect::ShowFileDialog(target)],
            UserCommand::FilePicked { target, path } => {
                match target {
                    Target::AllMonitors => {
                        self.config.default_profile = Some(WallpaperSpec { file: path });
                        // "All screens" means all: drop the per-screen overrides.
                        for p in &mut self.config.monitors {
                            p.file = None;
                        }
                    }
                    Target::Monitor(id) => profile_mut(&mut self.config, &id).file = Some(path),
                }
                save_and_apply()
            }
            UserCommand::ClearMonitorFile(id) => {
                if !self.config.profile(&id).is_some_and(|p| p.file.is_some()) {
                    return Vec::new();
                }
                profile_mut(&mut self.config, &id).file = None;
                save_and_apply()
            }
            UserCommand::SetEnabled { id, enabled } => {
                if self.config.is_enabled(&id) == enabled {
                    return Vec::new();
                }
                profile_mut(&mut self.config, &id).enabled = enabled;
                save_and_apply()
            }
            UserCommand::SetZoom { id, zoom } => {
                let current = self
                    .config
                    .profile(&id)
                    .map_or(ZoomMode::default(), |p| p.zoom);
                if current == zoom {
                    return Vec::new();
                }
                profile_mut(&mut self.config, &id).zoom = zoom;
                save_and_apply()
            }
            UserCommand::TogglePause => {
                let paused = !self.config.general.paused_by_user;
                self.config.general.paused_by_user = paused;
                vec![Effect::SaveConfig, Effect::SetPaused(paused)]
            }
            UserCommand::ToggleAutostart => {
                let on = !self.config.general.autostart;
                self.config.general.autostart = on;
                vec![Effect::SaveConfig, Effect::SetAutostart(on)]
            }
            UserCommand::SetAudioSource(source) => {
                if self.config.audio.source == source {
                    return Vec::new();
                }
                self.config.audio.source = source;
                save_and_apply()
            }
            UserCommand::SetVolume(volume) => {
                let volume = volume.min(100);
                if self.config.audio.volume == volume {
                    return Vec::new();
                }
                self.config.audio.volume = volume;
                save_and_apply()
            }
            UserCommand::SetOtherAudio(mode) => {
                if self.config.audio.other_audio == mode {
                    return Vec::new();
                }
                self.config.audio.other_audio = mode;
                save_and_apply()
            }
            UserCommand::ToggleFullscreenScope => {
                let p = &mut self.config.pause;
                p.fullscreen_app_scope = match p.fullscreen_app_scope {
                    PauseScope::AllMonitors => PauseScope::ThisMonitor,
                    PauseScope::ThisMonitor => PauseScope::AllMonitors,
                };
                save_and_apply()
            }
            UserCommand::OpenConfigFolder => vec![Effect::OpenConfigFolder],
            UserCommand::LogStats => vec![Effect::LogStats],
            UserCommand::ReloadConfig => vec![Effect::ReloadConfigFromDisk],
            UserCommand::ConfigReloaded(cfg) => {
                self.config = cfg;
                vec![
                    Effect::ApplyConfig,
                    Effect::SetPaused(self.config.general.paused_by_user),
                ]
            }
            UserCommand::Quit => vec![Effect::Quit],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> MonitorId {
        MonitorId::new(s)
    }
    fn app() -> App {
        App::new(Config::default())
    }

    #[test]
    fn disabling_a_screen_creates_its_profile_and_is_idempotent() {
        let mut a = app();
        let fx = a.update(UserCommand::SetEnabled {
            id: id("A"),
            enabled: false,
        });
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        assert!(!a.config().is_enabled(&id("A")));
        // Same again: nothing to do. Enabling an unknown screen: already the default.
        assert!(a
            .update(UserCommand::SetEnabled {
                id: id("A"),
                enabled: false
            })
            .is_empty());
        assert!(a
            .update(UserCommand::SetEnabled {
                id: id("B"),
                enabled: true
            })
            .is_empty());
        assert_eq!(a.config().monitors.len(), 1);
    }

    #[test]
    fn zoom_changes_are_saved_and_unchanged_zoom_is_ignored() {
        let mut a = app();
        let fx = a.update(UserCommand::SetZoom {
            id: id("A"),
            zoom: ZoomMode::Fit,
        });
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        assert_eq!(a.config().profile(&id("A")).unwrap().zoom, ZoomMode::Fit);
        assert!(a
            .update(UserCommand::SetZoom {
                id: id("A"),
                zoom: ZoomMode::Fit
            })
            .is_empty());
        // Fill is the default for a screen without a profile: no change, no profile.
        assert!(a
            .update(UserCommand::SetZoom {
                id: id("B"),
                zoom: ZoomMode::Fill
            })
            .is_empty());
        assert!(a.config().profile(&id("B")).is_none());
    }

    #[test]
    fn picking_for_all_sets_the_default_and_clears_overrides() {
        let mut a = app();
        a.update(UserCommand::FilePicked {
            target: Target::Monitor(id("A")),
            path: PathBuf::from("own.mp4"),
        });
        a.update(UserCommand::SetZoom {
            id: id("A"),
            zoom: ZoomMode::Fit,
        });
        let fx = a.update(UserCommand::FilePicked {
            target: Target::AllMonitors,
            path: PathBuf::from("all.mp4"),
        });
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        let c = a.config();
        assert_eq!(
            c.default_profile.as_ref().unwrap().file,
            PathBuf::from("all.mp4")
        );
        assert_eq!(
            c.resolved_file(&id("A")),
            Some(std::path::Path::new("all.mp4"))
        );
        // The zoom choice survives.
        assert_eq!(c.profile(&id("A")).unwrap().zoom, ZoomMode::Fit);
    }

    #[test]
    fn picking_for_one_screen_leaves_the_others_alone() {
        let mut a = app();
        a.update(UserCommand::FilePicked {
            target: Target::AllMonitors,
            path: PathBuf::from("all.mp4"),
        });
        a.update(UserCommand::FilePicked {
            target: Target::Monitor(id("B")),
            path: PathBuf::from("b.mp4"),
        });
        let c = a.config();
        assert_eq!(
            c.resolved_file(&id("A")),
            Some(std::path::Path::new("all.mp4"))
        );
        assert_eq!(
            c.resolved_file(&id("B")),
            Some(std::path::Path::new("b.mp4"))
        );
    }

    #[test]
    fn clearing_an_override_returns_to_the_default() {
        let mut a = app();
        a.update(UserCommand::FilePicked {
            target: Target::AllMonitors,
            path: PathBuf::from("all.mp4"),
        });
        // Nothing to clear yet.
        assert!(a.update(UserCommand::ClearMonitorFile(id("B"))).is_empty());
        a.update(UserCommand::FilePicked {
            target: Target::Monitor(id("B")),
            path: PathBuf::from("b.mp4"),
        });
        let fx = a.update(UserCommand::ClearMonitorFile(id("B")));
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        assert_eq!(
            a.config().resolved_file(&id("B")),
            Some(std::path::Path::new("all.mp4"))
        );
    }

    #[test]
    fn pause_and_autostart_toggle_and_say_so() {
        let mut a = app();
        assert_eq!(
            a.update(UserCommand::TogglePause),
            vec![Effect::SaveConfig, Effect::SetPaused(true)]
        );
        assert!(a.config().general.paused_by_user);
        assert_eq!(
            a.update(UserCommand::TogglePause),
            vec![Effect::SaveConfig, Effect::SetPaused(false)]
        );
        assert_eq!(
            a.update(UserCommand::ToggleAutostart),
            vec![Effect::SaveConfig, Effect::SetAutostart(true)]
        );
        assert!(a.config().general.autostart);
    }

    #[test]
    fn audio_source_is_exclusive_and_changes_are_saved_once() {
        let mut a = app();
        assert!(a.config().audio.source.is_none());
        let fx = a.update(UserCommand::SetAudioSource(Some(id("A"))));
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        // Choosing another screen replaces the source (no second source can exist).
        a.update(UserCommand::SetAudioSource(Some(id("B"))));
        assert_eq!(a.config().audio.source, Some(id("B")));
        // Same again: nothing to do.
        assert!(a
            .update(UserCommand::SetAudioSource(Some(id("B"))))
            .is_empty());
        // "No audio".
        let fx = a.update(UserCommand::SetAudioSource(None));
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        assert!(a.config().audio.source.is_none());
        assert!(a.update(UserCommand::SetAudioSource(None)).is_empty());
    }

    #[test]
    fn other_audio_mode_is_saved_once() {
        let mut a = app();
        assert_eq!(a.config().audio.other_audio, OtherAudio::Mute);
        let fx = a.update(UserCommand::SetOtherAudio(OtherAudio::Duck));
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        assert_eq!(a.config().audio.other_audio, OtherAudio::Duck);
        assert!(a
            .update(UserCommand::SetOtherAudio(OtherAudio::Duck))
            .is_empty());
    }

    #[test]
    fn volume_is_clamped_saved_and_idempotent() {
        let mut a = app();
        let fx = a.update(UserCommand::SetVolume(75));
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        assert_eq!(a.config().audio.volume, 75);
        assert!(a.update(UserCommand::SetVolume(75)).is_empty());
        a.update(UserCommand::SetVolume(250));
        assert_eq!(a.config().audio.volume, 100);
    }

    #[test]
    fn fullscreen_scope_toggles_between_all_screens_and_this_screen() {
        let mut a = app();
        assert_eq!(
            a.config().pause.fullscreen_app_scope,
            PauseScope::AllMonitors
        );
        let fx = a.update(UserCommand::ToggleFullscreenScope);
        assert_eq!(fx, vec![Effect::SaveConfig, Effect::ApplyConfig]);
        assert_eq!(
            a.config().pause.fullscreen_app_scope,
            PauseScope::ThisMonitor
        );
        a.update(UserCommand::ToggleFullscreenScope);
        assert_eq!(
            a.config().pause.fullscreen_app_scope,
            PauseScope::AllMonitors
        );
    }

    #[test]
    fn pick_open_reload_and_quit_only_emit_their_effect() {
        let mut a = app();
        assert_eq!(
            a.update(UserCommand::PickFile(Target::AllMonitors)),
            vec![Effect::ShowFileDialog(Target::AllMonitors)]
        );
        assert_eq!(
            a.update(UserCommand::OpenConfigFolder),
            vec![Effect::OpenConfigFolder]
        );
        assert_eq!(
            a.update(UserCommand::ReloadConfig),
            vec![Effect::ReloadConfigFromDisk]
        );
        assert_eq!(a.update(UserCommand::Quit), vec![Effect::Quit]);
        assert_eq!(a.update(UserCommand::LogStats), vec![Effect::LogStats]);
    }

    #[test]
    fn a_reloaded_config_replaces_the_state_without_saving() {
        let mut a = app();
        let mut fresh = Config::default();
        fresh.general.paused_by_user = true;
        fresh.default_profile = Some(WallpaperSpec {
            file: PathBuf::from("x.mp4"),
        });
        let fx = a.update(UserCommand::ConfigReloaded(fresh.clone()));
        assert_eq!(fx, vec![Effect::ApplyConfig, Effect::SetPaused(true)]);
        assert_eq!(a.config(), &fresh);
    }
}
