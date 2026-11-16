//! Glue between the tray, the pure `App` state and the `Runtime`: turns events into
//! commands and executes the effects the app asks for. Safe Rust only.
use crate::core::app::{App, Effect, UserCommand};
use crate::core::config;
use crate::core::media;
use crate::core::menu::{self, BuiltMenu};
use crate::platform::events::{self, WinEventHooks};
use crate::platform::message_window::{self, AppEvent};
use crate::platform::sys::{HWND, WM_CONTEXTMENU, WM_LBUTTONUP, WM_RBUTTONUP};
use crate::platform::tray::Tray;
use crate::platform::{autostart, dialog, shell};
use crate::runtime::Runtime;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Mouse messages on the tray icon that open the menu.
pub fn is_menu_click(msg: u32) -> bool {
    matches!(msg, WM_RBUTTONUP | WM_LBUTTONUP | WM_CONTEXTMENU)
}

pub struct Controller {
    app: App,
    runtime: Runtime,
    tray: Rc<Tray>,
    config_path: PathBuf,
    message_hwnd: HWND,
    /// `false` for throw-away sessions (`--play`): nothing is written to the config file.
    persist: bool,
    /// Last notification shown, to keep the same message from repeating.
    last_notice: Option<(String, Instant)>,
    /// Window event hooks (M6); dropped (unhooked) with the controller.
    hooks: Option<WinEventHooks>,
}

impl Controller {
    pub fn new(
        app: App,
        runtime: Runtime,
        tray: Rc<Tray>,
        config_path: PathBuf,
        message_hwnd: HWND,
        persist: bool,
    ) -> Self {
        Self {
            app,
            runtime,
            tray,
            config_path,
            message_hwnd,
            persist,
            last_notice: None,
            hooks: None,
        }
    }

    /// The menu for the current config and monitors. The caller shows it *without* holding
    /// a borrow of the controller, because the popup runs a nested message loop.
    pub fn build_menu(&self) -> BuiltMenu {
        menu::build(self.app.config(), self.runtime.monitors())
    }

    pub fn on_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Mpv(token) => {
                for notice in self.runtime.on_mpv_event(token) {
                    self.notify("Very heavy video", &notice, true);
                }
            }
            AppEvent::DisplayChanged => self.runtime.on_display_change(),
            AppEvent::Refresh => self.dispatch(UserCommand::ReloadConfig),
            AppEvent::ExplorerRestarted => {
                if let Err(e) = self.tray.add() {
                    log::warn!("cannot re-add the tray icon: {e}");
                }
                self.runtime.on_explorer_restart();
            }
            AppEvent::FilePicked => {
                for (target, path) in dialog::take_picked() {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        if let Some(warning) = media::large_animation_warning(&path, meta.len()) {
                            self.notify_now("Large animation", warning, true);
                        }
                    }
                    self.dispatch(UserCommand::FilePicked { target, path });
                }
            }
            AppEvent::WindowsChanged => self.runtime.on_windows_changed(),
            AppEvent::PauseTick => self.runtime.on_pause_tick(),
            AppEvent::AudioCheck => self.runtime.on_audio_check(),
            AppEvent::Session(code) => self.runtime.on_session_event(code),
            AppEvent::Power(code) => self.runtime.on_power_event(code),
            AppEvent::DisplayPower(state) => self.runtime.on_display_power(state),
            AppEvent::FullscreenTick => self.runtime.on_fullscreen_tick(),
            AppEvent::Quitting => self.tray.remove(),
            // Menu clicks are handled by the event closure (no borrow while the popup runs).
            AppEvent::Tray(_) => {}
        }
        self.flush_notices();
    }

    pub fn dispatch(&mut self, cmd: UserCommand) {
        log::info!("command: {cmd:?}");
        for effect in self.app.update(cmd) {
            self.execute(effect);
        }
        self.flush_notices();
    }

    /// Show the problems the runtime collected while syncing (missing file, player failure).
    pub fn flush_notices(&mut self) {
        for text in self.runtime.take_notices() {
            log::warn!("notice: {text}");
            self.notify("Wallpaper not loaded", &text, true);
        }
        for text in self.runtime.take_audio_notices() {
            log::warn!("notice: {text}");
            self.notify("Audio", &text, true);
        }
    }

    /// Install the window event hooks and take a first reading.
    pub fn start_observing(&mut self) {
        match events::install(self.message_hwnd) {
            Ok(h) => self.hooks = Some(h),
            Err(e) => log::error!("cannot install the window event hooks: {e}"),
        }
        self.runtime.init_session_state();
        self.runtime.on_windows_changed();
    }

    /// Feedback for something the user just did: always shown.
    fn notify_now(&mut self, title: &str, text: &str, warning: bool) {
        self.tray.notify(title, text, warning);
    }

    /// Automatic notice (not the answer to a click): the same message is not repeated within
    /// 30 seconds.
    fn notify(&mut self, title: &str, text: &str, warning: bool) {
        let key = format!("{title}|{text}");
        if let Some((last, at)) = &self.last_notice {
            if *last == key && at.elapsed() < Duration::from_secs(30) {
                return;
            }
        }
        self.tray.notify(title, text, warning);
        self.last_notice = Some((key, Instant::now()));
    }

    fn execute(&mut self, effect: Effect) {
        match effect {
            Effect::SaveConfig => {
                if !self.persist {
                    log::info!("config not saved (--play session)");
                } else if let Err(e) = config::save_atomic(self.app.config(), &self.config_path) {
                    log::error!("cannot save {}: {e}", self.config_path.display());
                }
            }
            Effect::ApplyConfig => self.runtime.apply_config(self.app.config().clone()),
            Effect::SetPaused(paused) => self.runtime.set_paused(paused),
            Effect::LogStats => self.runtime.log_stats(),
            Effect::OpenConfigFolder => {
                if let Some(dir) = self.config_path.parent() {
                    if let Err(e) = shell::open_folder(dir) {
                        log::error!("cannot open {}: {e}", dir.display());
                    }
                }
            }
            Effect::ReloadConfigFromDisk => match std::fs::read_to_string(&self.config_path) {
                Ok(text) => match config::parse(&text) {
                    Ok(cfg) => {
                        log::info!("config reloaded from {}", self.config_path.display());
                        self.dispatch(UserCommand::ConfigReloaded(cfg));
                        // Write the effective settings back: restores blocks the user cut
                        // out and tidies the file.
                        self.execute(Effect::SaveConfig);
                        self.notify_now(
                            "Config reloaded",
                            "The settings were read from config.toml.",
                            false,
                        );
                    }
                    Err(e) => {
                        // Keep what is running and leave the broken file alone so it can be fixed.
                        log::error!("config.toml has an error: {e}");
                        self.notify_now(
                            "Config not reloaded",
                            "config.toml has an error. The current settings are kept and the file was not changed.",
                            true,
                        );
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    log::warn!("config.toml is missing: recreating it from the current settings");
                    self.execute(Effect::SaveConfig);
                    self.notify_now(
                        "Config recreated",
                        "config.toml was missing. It was recreated from the current settings.",
                        false,
                    );
                }
                Err(e) => {
                    log::error!("cannot read {}: {e}", self.config_path.display());
                    self.notify_now(
                        "Config not reloaded",
                        "config.toml could not be read. The current settings are kept.",
                        true,
                    );
                }
            },
            Effect::ShowFileDialog(target) => {
                dialog::pick_file_async(self.message_hwnd, target, media::dialog_filters())
            }
            Effect::SetAutostart(on) => match std::env::current_exe() {
                Ok(exe) => match autostart::set(on, &exe) {
                    Ok(()) => log::info!("start with Windows: {on}"),
                    Err(e) => {
                        log::error!("cannot change the Run entry: {e}");
                        self.notify_now(
                            "Start with Windows",
                            "The setting could not be changed.",
                            true,
                        );
                    }
                },
                Err(e) => log::error!("cannot find our own exe path: {e}"),
            },
            Effect::Quit => message_window::request_quit(self.message_hwnd),
        }
    }
}
