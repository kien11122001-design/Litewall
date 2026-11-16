//! One wallpaper window + one mpv player per monitor, kept in line with the config and the
//! current monitor layout. Safe Rust only: the decisions come from `core::reconcile`, the
//! Win32 and libmpv calls from `platform` and `mpv`.
use crate::core::audio::{self, AudioState, OtherAudio, OtherAudioTracker, SourceStatus};
use crate::core::config::Config;
use crate::core::coverage::{self, DEFAULT_COLS, DEFAULT_ROWS};
use crate::core::geometry::RectI32;
use crate::core::media;
use crate::core::menu::monitor_label;
use crate::core::monitor::{self, MonitorId, MonitorInfo};
use crate::core::pause::{Hysteresis, PauseReasons, PauseScope, PauseTracker, PlayState};
use crate::core::power;
use crate::core::reconcile::{plan, Action, Running, Wanted};
use crate::core::vsync;
use crate::core::zoom::ZoomMode;
use crate::mpv::ffi::Api;
use crate::mpv::player::{Player, PlayerEvent, PlayerOptions};
use crate::platform::audio_sessions;
use crate::platform::desktop::{self, Host};
use crate::platform::fullscreen;
use crate::platform::message_window;
use crate::platform::monitors;
use crate::platform::sys::HWND;
use crate::platform::window::{self, WallpaperWindow};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

struct Instance {
    // Field order matters: fields drop in declaration order, and mpv's own child window
    // lives inside the wallpaper window, so the player must go first.
    player: Player,
    window: WallpaperWindow,
    token: usize,
    running: Running,
    /// Whether mpv is currently paused (so only changes are sent to libmpv).
    paused: bool,
    /// Audio state last sent to the player (so only changes are sent to libmpv).
    audio: AudioState,
    /// Wall-clock time and playback position of the last "Log playback stats" click
    /// (for the speed probe).
    stats_mark: Option<(Instant, f64)>,
    /// `video-sync` mode last sent to the player (see `core::vsync`).
    video_sync: &'static str,
}

pub struct Runtime {
    api: Rc<Api>,
    host: Host,
    message_hwnd: HWND,
    extra: Vec<(String, String)>,
    fallback_zoom: ZoomMode,
    config: Config,
    /// Pause reasons: global USER plus per-monitor COVERED (debounced by `hyst`).
    tracker: PauseTracker,
    hyst: HashMap<MonitorId, Hysteresis>,
    started: Instant,
    /// Last `QUERY_USER_NOTIFICATION_STATE` seen (5 = normal), to log changes only.
    fs_state: u32,
    /// Exclusive D3D fullscreen / presentation mode (always pauses every screen).
    fs_sys: bool,
    /// Raw reading: a true fullscreen app (foreground, exact fit, no caption, BUSY state)
    /// is on a screen and `fullscreen_app_scope` is "all screens".
    fs_raw: bool,
    /// The same reading after hysteresis (pause after 250 ms, resume after 100 ms).
    fs_hyst: Hysteresis,
    fs_app_on: bool,
    /// What the last raw reading saw (class, screen, state), for the log.
    fs_desc: String,
    /// Whether the 1 s fallback timer is currently being re-armed (logged on change).
    fs_timer_on: bool,
    /// Last "fullscreen app not detected" report logged (so the same line is not repeated).
    fs_reject: String,
    monitors: Vec<MonitorInfo>,
    instances: HashMap<MonitorId, Instance>,
    next_token: usize,
    notices: Vec<String>,
    /// Audio-related notices (shown under their own title).
    audio_notices: Vec<String>,
    /// The configured audio screen is not connected (notice already raised).
    audio_missing: bool,
    /// Stable "another app is playing audio" flag (muting is immediate, unmuting is delayed).
    other: OtherAudioTracker,
    /// The one-shot audio-check timer is armed.
    audio_watch_armed: bool,
    /// The session check failed and was already reported (no log flood).
    audio_check_failed: bool,
    /// Number of other-audio checks so far, and the last session summary that was logged.
    audio_checks: u32,
    last_audio_summary: String,
    covered: HashMap<MonitorId, bool>,
}

impl Runtime {
    /// `fallback_zoom` applies to monitors that have no profile in the config.
    pub fn new(
        api: Rc<Api>,
        host: Host,
        message_hwnd: HWND,
        config: Config,
        extra: Vec<(String, String)>,
        fallback_zoom: ZoomMode,
    ) -> Self {
        let tracker = PauseTracker::new(config.pause.scope);
        Self {
            api,
            host,
            message_hwnd,
            extra,
            fallback_zoom,
            config,
            tracker,
            hyst: HashMap::new(),
            started: Instant::now(),
            fs_state: 5,
            fs_sys: false,
            fs_raw: false,
            fs_hyst: Hysteresis::with_defaults(),
            fs_app_on: false,
            fs_desc: String::new(),
            fs_timer_on: false,
            fs_reject: String::new(),
            monitors: Vec::new(),
            instances: HashMap::new(),
            next_token: 0,
            notices: Vec::new(),
            audio_notices: Vec::new(),
            audio_missing: false,
            other: OtherAudioTracker::default(),
            audio_watch_armed: false,
            audio_check_failed: false,
            audio_checks: 0,
            last_audio_summary: String::new(),
            covered: HashMap::new(),
        }
    }

    fn push_notice(&mut self, text: String) {
        if !self.notices.contains(&text) {
            self.notices.push(text);
        }
    }

    /// User-facing problems found while syncing (deduplicated); the controller shows them.
    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    /// Audio problems found while syncing (shown by the controller under "Audio").
    pub fn take_audio_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.audio_notices)
    }

    /// Make every player's audio match the config: only the source screen plays, all others
    /// are silent with no audio track decoded. A source screen that is not connected leaves
    /// everything silent (it never moves to another screen) and raises one notice.
    fn apply_audio(&mut self) {
        self.apply_audio_players();
        self.update_audio_watch();
    }

    /// Whether LiteWall is playing sound that another app's audio would have to yield to.
    fn audio_watch_needed(&self) -> bool {
        self.config.audio.other_audio != OtherAudio::Off
            && self
                .instances
                .iter()
                .any(|(id, i)| !i.paused && audio::state_for(&self.config.audio, id).on)
    }

    /// Start the check timer when it is needed and not running. With nothing audible, or the
    /// source paused, there is no timer at all.
    fn update_audio_watch(&mut self) {
        if self.audio_watch_needed() {
            if !self.audio_watch_armed {
                self.audio_watch_armed = true;
                message_window::arm_audio_timer(self.message_hwnd, 100);
            }
        } else {
            self.audio_watch_armed = false;
            self.other.reset();
        }
    }

    /// `AppEvent::AudioCheck`: look for another app's audio, apply the result, and look again
    /// in 2 s while LiteWall is still playing sound.
    pub fn on_audio_check(&mut self) {
        self.audio_watch_armed = false;
        if !self.audio_watch_needed() {
            self.other.reset();
            self.apply_audio_players();
            return;
        }
        let own_pid = std::process::id();
        self.audio_checks += 1;
        let found = match audio_sessions::list_sessions() {
            Ok(list) => {
                self.audio_check_failed = false;
                let found = audio::first_other_audio(&list, own_pid);
                // Logged when it changes (and on the first check), so the log shows that the
                // check runs and what it sees without a line every 2 s.
                let summary = format!(
                    "{} -> {}",
                    audio::describe_sessions(&list, own_pid),
                    found.map_or("no other audio".to_string(), |p| format!(
                        "other audio (pid {p})"
                    ))
                );
                if summary != self.last_audio_summary {
                    log::info!("audio sessions: {summary}");
                    self.last_audio_summary = summary;
                }
                found
            }
            Err(e) => {
                if !self.audio_check_failed {
                    log::warn!("cannot check other apps' audio: {e}");
                }
                self.audio_check_failed = true;
                None
            }
        };
        let before = self.other.active();
        let now = self.other.observe(found.is_some());
        if now != before {
            match found {
                Some(pid) if now => log::info!(
                    "another app is playing audio (pid {pid}): LiteWall audio {}",
                    match self.config.audio.other_audio {
                        OtherAudio::Duck =>
                            format!("lowered to {} %", self.config.audio.duck_percent),
                        _ => "muted".to_string(),
                    }
                ),
                _ => log::info!("other audio stopped: LiteWall audio restored"),
            }
        }
        self.apply_audio_players();
        if self.audio_watch_needed() {
            self.audio_watch_armed = true;
            message_window::arm_audio_timer(self.message_hwnd, 2000);
        }
    }

    fn apply_audio_players(&mut self) {
        let other = self.other.active();
        let mode = self.config.audio.other_audio;
        let duck = self.config.audio.duck_percent;
        for (id, inst) in self.instances.iter_mut() {
            let want =
                audio::effective_state(audio::state_for(&self.config.audio, id), other, mode, duck);
            if want == inst.audio {
                continue;
            }
            match inst.player.set_audio(want.on, want.volume) {
                Ok(()) => {
                    log::info!(
                        "monitor {}: audio {} (volume {})",
                        id.as_str(),
                        if want.on { "ON" } else { "off" },
                        want.volume
                    );
                    inst.audio = want;
                }
                Err(e) => log::warn!("monitor {}: cannot change audio: {e}", id.as_str()),
            }
        }
        match audio::source_status(&self.config.audio, &self.monitors) {
            SourceStatus::Missing => {
                if !self.audio_missing {
                    self.audio_missing = true;
                    log::warn!("the audio screen is not connected: everything stays silent");
                    self.audio_notices.push(
                        "The screen that played audio is not connected, so everything is silent. \
                         It plays again when that screen returns, or choose another one in the \
                         Audio menu."
                            .to_string(),
                    );
                }
            }
            _ => self.audio_missing = false,
        }
    }

    /// M6 step 1 (observe only): which screens are covered by windows? Logged, nothing paused.
    pub fn on_windows_changed(&mut self) {
        let windows = fullscreen::visible_windows();
        let rects: Vec<RectI32> = windows.iter().map(|(_, r)| *r).collect();
        let fg = fullscreen::foreground();
        if let Some((class, rect)) = &fg {
            log::trace!("foreground: {class} {rect:?}");
        }
        let threshold = self.config.pause.coverage_threshold as f32;
        for (i, m) in self.monitors.iter().enumerate() {
            let frac = coverage::covered_fraction(m.rect_px, &rects, DEFAULT_COLS, DEFAULT_ROWS);
            let covered = frac >= threshold && self.config.pause.on_fullscreen;
            let fg_fills = fg
                .as_ref()
                .is_some_and(|(_, r)| coverage::window_fills_monitor(m.rect_px, *r));
            let label = monitor_label(i, m);
            log::trace!("cover {label}: {:.0}% fg_fills={fg_fills}", frac * 100.0);
            if self.covered.insert(m.id.clone(), covered) != Some(covered) {
                if covered {
                    let big: Vec<&str> = windows
                        .iter()
                        .filter(|(_, r)| {
                            r.intersect(&m.rect_px)
                                .is_some_and(|x| x.area() * 4 >= m.rect_px.area())
                        })
                        .map(|(c, _)| c.as_str())
                        .collect();
                    log::info!("cover {label}: COVERED {:.0}% by {big:?}", frac * 100.0);
                } else {
                    log::info!("cover {label}: uncovered {:.0}%", frac * 100.0);
                }
            }
        }
        self.check_fullscreen(&windows);
        self.update_pause();
    }

    /// Fullscreen detection for the global FULLSCREEN reason (pauses every screen):
    /// * exclusive Direct3D fullscreen / presentation mode (notification state 3 or 4), always;
    /// * a true fullscreen app: the foreground window has no caption, exactly fills a screen
    ///   and the shell reports a busy state (games, presentations), when `fullscreen_app_scope`
    ///   is "all screens". A normal maximised window or a browser in F11 is not one; it only
    ///   pauses its own screen through the coverage check.
    ///
    /// The notification state can lag a second behind the window and change without a window
    /// event, so while an exactly-fitting window exists (or FULLSCREEN is set) a 1 s one-shot
    /// timer looks again. With none of those no timer is armed. The raw readings are stored
    /// here and turned into the reason (with hysteresis) in `update_pause`.
    fn check_fullscreen(&mut self, windows: &[(String, RectI32)]) {
        let rects: Vec<RectI32> = self.monitors.iter().map(|m| m.rect_px).collect();
        let exact = windows
            .iter()
            .any(|(_, r)| rects.iter().any(|m| coverage::window_exactly_fills(*m, *r)));
        let state = fullscreen::notification_state().unwrap_or(5);
        if state != self.fs_state {
            log::info!("notification state {} -> {state}", self.fs_state);
            self.fs_state = state;
        }
        let enabled = self.config.pause.on_fullscreen;
        let (_, sys_on) = power::fullscreen_state_change(state);
        self.fs_sys = sys_on && enabled;

        let mut app_raw = false;
        if enabled && self.config.pause.fullscreen_app_scope == PauseScope::AllMonitors {
            if let Some((class, rect)) = fullscreen::foreground_candidate() {
                if let Some(i) = coverage::fullscreen_app_monitor(&rects, Some(rect), state) {
                    app_raw = true;
                    self.fs_desc = format!(
                        "{class} on {} (notification state {state})",
                        monitor_label(i, &self.monitors[i])
                    );
                }
            }
        }
        self.fs_raw = app_raw;

        // Step 4c diagnostics: the shell says "busy" and a window fits a screen exactly, but no
        // fullscreen app was found. Say why (once per distinct report).
        let watching = enabled && self.config.pause.fullscreen_app_scope == PauseScope::AllMonitors;
        if watching && exact && matches!(state, 2 | 3) && !app_raw {
            let report = fullscreen::foreground_report(&rects);
            if report != self.fs_reject {
                log::info!("fullscreen app not detected (state {state}): {report}");
                self.fs_reject = report;
            }
        } else if !exact || app_raw {
            self.fs_reject.clear();
        }

        let need = exact || self.fs_sys || app_raw || self.fs_app_on;
        if need {
            message_window::arm_fullscreen_timer(self.message_hwnd, 1000);
        }
        if need != self.fs_timer_on {
            self.fs_timer_on = need;
            if need {
                log::debug!(
                    "fullscreen timer armed (exact-fit window {exact}, system fullscreen {}, \
                     fullscreen app {})",
                    self.fs_sys,
                    app_raw || self.fs_app_on
                );
            } else {
                log::debug!("fullscreen timer stopped: nothing is fullscreen");
            }
        }
    }

    /// The 1 s fallback timer fired: look again.
    pub fn on_fullscreen_tick(&mut self) {
        self.on_windows_changed();
    }

    /// At startup: a remote-desktop session pauses right away.
    pub fn init_session_state(&mut self) {
        if self.config.pause.on_lock_or_remote && message_window::is_remote_session() {
            log::info!("running in a remote-desktop session: wallpapers paused");
            self.tracker.set_global(PauseReasons::REMOTE, true);
            self.apply_pause_states();
        }
    }

    /// Lock / unlock / remote connect / console connect (`WM_WTSSESSION_CHANGE`).
    pub fn on_session_event(&mut self, code: u32) {
        let changes = power::session_changes(code, message_window::is_remote_session());
        log::info!("session event {code}: {changes:?}");
        if !self.config.pause.on_lock_or_remote {
            return;
        }
        for (flag, on) in changes {
            self.tracker.set_global(flag, on);
        }
        self.apply_pause_states();
    }

    /// Suspend / resume (`WM_POWERBROADCAST`).
    pub fn on_power_event(&mut self, code: u32) {
        if let Some((flag, on)) = power::power_change(code) {
            log::info!(
                "power event 0x{code:X}: {}",
                if on { "sleep" } else { "resume" }
            );
            self.tracker.set_global(flag, on);
            self.apply_pause_states();
        }
    }

    /// Screens off / on / dimmed (`GUID_CONSOLE_DISPLAY_STATE`).
    pub fn on_display_power(&mut self, state: u32) {
        let (flag, on) = power::display_state_change(state);
        log::info!(
            "display power state {state}: {}",
            if on { "screens off" } else { "screens on" }
        );
        self.tracker.set_global(flag, on);
        self.apply_pause_states();
    }

    /// A hysteresis delay elapsed (`AppEvent::PauseTick`): re-evaluate with the last raw readings.
    pub fn on_pause_tick(&mut self) {
        self.update_pause();
    }

    /// Feed the raw coverage readings through the per-monitor hysteresis (pause after 250 ms
    /// covered, resume after 100 ms uncovered), update the COVERED reason and apply it.
    fn update_pause(&mut self) {
        let now = self.started.elapsed();
        // Global FULLSCREEN reason: system fullscreen at once, a fullscreen app debounced.
        let app_on = self.fs_hyst.observe(self.fs_raw, now);
        if app_on != self.fs_app_on {
            self.fs_app_on = app_on;
            if app_on {
                log::info!("fullscreen app: {}: pausing all screens", self.fs_desc);
            } else {
                log::info!("fullscreen app ended: resuming the screens it paused");
            }
        }
        self.tracker
            .set_global(PauseReasons::FULLSCREEN, self.fs_sys || app_on);
        for m in &self.monitors {
            let raw = self.covered.get(&m.id).copied().unwrap_or(false);
            let stable = self
                .hyst
                .entry(m.id.clone())
                .or_insert_with(Hysteresis::with_defaults)
                .observe(raw, now);
            self.tracker
                .set_monitor(m.id.as_str(), PauseReasons::COVERED, stable);
        }
        let gone: Vec<MonitorId> = self
            .hyst
            .keys()
            .filter(|id| !self.monitors.iter().any(|m| &m.id == *id))
            .cloned()
            .collect();
        for id in gone {
            self.hyst.remove(&id);
            self.covered.remove(&id);
            self.tracker.remove_monitor(id.as_str());
        }
        self.apply_pause_states();
        self.rearm_pause_timer(now);
    }

    /// One-shot timer for the earliest pending hysteresis deadline (none pending = no timer).
    fn rearm_pause_timer(&self, now: Duration) {
        let next = self
            .hyst
            .values()
            .chain(std::iter::once(&self.fs_hyst))
            .filter_map(|h| h.next_deadline())
            .min();
        if let Some(deadline) = next {
            let ms = deadline.saturating_sub(now).as_millis().min(60_000) as u32;
            message_window::arm_pause_timer(self.message_hwnd, ms + 5);
        }
    }

    /// Send `pause` to every player whose wanted state differs from its current one.
    fn apply_pause_states(&mut self) {
        self.apply_pause_to_players();
        self.update_audio_watch();
    }

    fn apply_pause_to_players(&mut self) {
        for (id, inst) in self.instances.iter_mut() {
            let want = self.tracker.state(id.as_str(), false) == PlayState::Paused;
            if want == inst.paused {
                continue;
            }
            match inst.player.set_pause(want) {
                Ok(()) => {
                    inst.paused = want;
                    log::info!(
                        "monitor {}: {} (reasons 0x{:X})",
                        id.as_str(),
                        if want { "PAUSED" } else { "RESUMED" },
                        self.tracker.effective(id.as_str()).bits()
                    );
                }
                Err(e) => log::warn!("monitor {}: cannot change pause: {e}", id.as_str()),
            }
        }
    }

    /// Number of monitors that currently have a wallpaper window.
    pub fn count(&self) -> usize {
        self.instances.len()
    }

    /// Bring the running wallpapers in line with the config and `monitors`.
    pub fn sync(&mut self, monitors: Vec<MonitorInfo>, reposition_all: bool) {
        let running: HashMap<MonitorId, Running> = self
            .instances
            .iter()
            .map(|(id, i)| (id.clone(), i.running.clone()))
            .collect();
        let actions = plan(
            &monitors,
            &self.config,
            self.fallback_zoom,
            &running,
            reposition_all,
        );
        log::debug!("sync: {} action(s)", actions.len());
        self.monitors = monitors;
        for a in actions {
            self.apply(a);
        }
        // Rects or windows may have changed: re-read coverage (also updates pause states).
        self.on_windows_changed();
        self.apply_audio();
        self.apply_video_sync();
    }

    /// The fastest refresh rate among the screens that have a wallpaper window.
    fn max_playing_hz(&self) -> f32 {
        self.monitors
            .iter()
            .filter(|m| self.instances.contains_key(&m.id))
            .map(|m| m.refresh_hz)
            .fold(0.0, f32::max)
    }

    /// `core::vsync` decision for screen `id` with the current monitor list. A screen that is
    /// not playing yet (being created) counts towards the maximum too.
    fn video_sync_for(&self, id: &MonitorId) -> &'static str {
        let hz = self
            .monitors
            .iter()
            .find(|m| &m.id == id)
            .map_or(0.0, |m| m.refresh_hz);
        vsync::video_sync_for(hz, self.max_playing_hz().max(hz))
    }

    /// Keep every player's `video-sync` mode in line with the current screens.
    fn apply_video_sync(&mut self) {
        // `--opt video-sync=...` is an explicit override for tests: leave it alone.
        if self.extra.iter().any(|(name, _)| name == "video-sync") {
            return;
        }
        let max = self.max_playing_hz();
        for m in &self.monitors {
            let Some(inst) = self.instances.get_mut(&m.id) else {
                continue;
            };
            let want = vsync::video_sync_for(m.refresh_hz, max);
            if want == inst.video_sync {
                continue;
            }
            match inst.player.set_video_sync(want) {
                Ok(()) => {
                    log::info!(
                        "monitor {}: video-sync {want} ({:.0} Hz, fastest screen {max:.0} Hz)",
                        m.id.as_str(),
                        m.refresh_hz
                    );
                    inst.video_sync = want;
                }
                Err(e) => log::warn!("monitor {}: cannot set video-sync: {e}", m.id.as_str()),
            }
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Create { id, rect, wanted } => self.create(id, rect, wanted),
            Action::Remove { id } => {
                if self.instances.remove(&id).is_some() {
                    log::info!("monitor {}: wallpaper removed", id.as_str());
                }
            }
            Action::Move { id, rect } => {
                let host = self.host;
                if let Some(inst) = self.instances.get_mut(&id) {
                    match inst.window.move_to(&host, rect) {
                        Ok(()) => inst.running.rect = rect,
                        Err(e) => log::warn!("monitor {}: move failed: {e}", id.as_str()),
                    }
                }
            }
            Action::Load { id, file } => {
                let mut failure: Option<String> = None;
                if let Some(inst) = self.instances.get_mut(&id) {
                    match inst.player.load(&file) {
                        Ok(()) => {
                            log::info!("monitor {}: now playing {}", id.as_str(), file.display());
                            inst.running.file = file;
                        }
                        Err(e) => {
                            log::error!("monitor {}: cannot load: {e}", id.as_str());
                            failure = Some(format!("Cannot play {}: {e}", file.display()));
                        }
                    }
                }
                if let Some(msg) = failure {
                    self.instances.remove(&id);
                    self.push_notice(msg);
                }
            }
            Action::Zoom { id, zoom } => {
                if let Some(inst) = self.instances.get_mut(&id) {
                    inst.player.apply_zoom(&zoom);
                    log::info!("monitor {}: zoom {zoom:?}", id.as_str());
                    inst.running.zoom = zoom;
                }
            }
        }
    }

    fn create(&mut self, id: MonitorId, rect: RectI32, wanted: Wanted) {
        // F-08: a bad file must not leave an empty window over the static wallpaper.
        if let Err(e) = Player::preflight(&wanted.file) {
            log::error!("monitor {}: {e}; leaving the static wallpaper", id.as_str());
            self.push_notice(format!("{e}. This screen keeps the normal wallpaper."));
            return;
        }
        let window = match WallpaperWindow::create(&self.host, rect) {
            Ok(w) => w,
            Err(e) => {
                log::error!("monitor {}: cannot create window: {e}", id.as_str());
                return;
            }
        };
        let token = self.next_token;
        self.next_token += 1;
        // Audio is decided before the player exists, so a silent screen never decodes audio
        // and nothing can play before the first frame.
        let audio_state = audio::state_for(&self.config.audio, &id);
        let video_sync = self.video_sync_for(&id);
        let opts = PlayerOptions {
            video_sync: video_sync.to_string(),
            audio: audio_state.on,
            volume: audio_state.volume,
            extra: self.extra.clone(),
        };
        // Locals drop in reverse order, so on an early return the player goes before the window.
        let mut player = match Player::new(&self.api, window.hwnd(), self.message_hwnd, token, opts)
        {
            Ok(p) => p,
            Err(e) => {
                log::error!("monitor {}: cannot start mpv: {e}", id.as_str());
                self.push_notice(format!("Cannot start the video player: {e}"));
                return;
            }
        };
        player.apply_zoom(&wanted.zoom);
        if let Err(e) = player.load(&wanted.file) {
            log::error!("monitor {}: cannot load: {e}", id.as_str());
            self.push_notice(format!("Cannot play {}: {e}", wanted.file.display()));
            return;
        }
        let paused = self.tracker.state(id.as_str(), false) == PlayState::Paused;
        if paused {
            if let Err(e) = player.set_pause(true) {
                log::warn!("monitor {}: cannot pause: {e}", id.as_str());
            }
        }
        log::info!(
            "monitor {}: playing {} ({:?}) on {rect:?} (player mpv#{token})",
            id.as_str(),
            wanted.file.display(),
            wanted.zoom
        );
        self.instances.insert(
            id,
            Instance {
                player,
                window,
                token,
                running: Running {
                    file: wanted.file,
                    zoom: wanted.zoom,
                    rect,
                },
                paused,
                audio: audio_state,
                stats_mark: None,
                video_sync,
            },
        );
    }

    /// libmpv has events waiting for the player with `token`.
    /// Returns user-facing notices (for the tray) raised by those events.
    pub fn on_mpv_event(&mut self, token: usize) -> Vec<String> {
        let mut notices = Vec::new();
        let Some((id, inst)) = self.instances.iter_mut().find(|(_, i)| i.token == token) else {
            log::debug!("mpv event for unknown player token {token}");
            return notices;
        };
        for ev in inst.player.drain_events() {
            match ev {
                PlayerEvent::Loaded => window::set_visible(inst.window.hwnd(), true),
                PlayerEvent::LoadFailed(why) => {
                    // F-08: leave the static wallpaper visible instead of a black window.
                    log::error!(
                        "monitor {}: {why}; hiding its wallpaper window",
                        id.as_str()
                    );
                    window::set_visible(inst.window.hwnd(), false);
                }
                PlayerEvent::Shutdown => {
                    log::error!("monitor {}: mpv shut down unexpectedly", id.as_str());
                    window::set_visible(inst.window.hwnd(), false);
                }
                PlayerEvent::VideoReady => {
                    if let Some(msg) = heavy_video_notice(&inst.player, &inst.running.file) {
                        log::warn!("monitor {}: {msg}", id.as_str());
                        notices.push(msg);
                    }
                }
            }
        }
        notices
    }

    /// Write every player's playback statistics to the log (tray: "Log playback stats").
    /// Also a speed probe: from the second click on, how far the video advanced compared
    /// with the wall clock since the previous click (1.00 = real time). Click twice, 5-10 s
    /// apart, on a clip that is longer than that (a loop that wraps in between is reported).
    pub fn log_stats(&mut self) {
        let now = Instant::now();
        for (id, inst) in self.instances.iter_mut() {
            let screen = self
                .monitors
                .iter()
                .find(|m| &m.id == id)
                .map_or("?".to_string(), |m| {
                    format!(
                        "{}x{} @ {:.0} Hz per Windows",
                        m.rect_px.width(),
                        m.rect_px.height(),
                        m.refresh_hz
                    )
                });
            log::info!(
                "stats {} ({screen}, paused {}, audio {}): {}",
                id.as_str(),
                inst.paused,
                inst.audio.on,
                inst.player.stats()
            );
            let pos = inst
                .player
                .property("time-pos")
                .and_then(|v| v.parse::<f64>().ok());
            match (inst.stats_mark, pos) {
                (Some((t0, p0)), Some(p1)) => {
                    let wall = now.duration_since(t0).as_secs_f64();
                    let advanced = p1 - p0;
                    if wall < 1.0 {
                        log::info!("speed probe {}: clicks too close together", id.as_str());
                    } else if advanced < 0.0 {
                        log::info!(
                            "speed probe {}: the loop wrapped between the two readings, click \
                             again within a few seconds",
                            id.as_str()
                        );
                    } else {
                        log::info!(
                            "speed probe {}: video advanced {advanced:.2} s in {wall:.2} s of \
                             wall time = {:.2}x",
                            id.as_str(),
                            advanced / wall
                        );
                    }
                }
                _ => log::info!(
                    "speed probe {}: first reading, click again in 5-10 s",
                    id.as_str()
                ),
            }
            inst.stats_mark = pos.map(|p| (now, p));
        }
        log::info!(
            "audio watch: mode {:?}, timer armed {}, needed {}, checks so far {}, other app audio {}",
            self.config.audio.other_audio,
            self.audio_watch_armed,
            self.audio_watch_needed(),
            self.audio_checks,
            self.other.active()
        );
        if let Some(inst) = self.instances.values().next() {
            log::info!("encoders: {}", inst.player.encoders_summary());
        }
    }

    /// Monitors, resolution or DPI changed (already debounced).
    pub fn on_display_change(&mut self) {
        log::info!("display change: re-detecting the desktop host and the monitors");
        match desktop::find_host() {
            Ok(h) if h.hwnd == self.host.hwnd => self.host = h,
            Ok(h) => {
                log::warn!(
                    "desktop host changed (0x{:X} -> 0x{:X}); rebuilding every wallpaper window",
                    self.host.hwnd,
                    h.hwnd
                );
                self.instances.clear();
                self.host = h;
            }
            Err(e) => {
                log::error!("cannot find the desktop host after a display change: {e}");
                return;
            }
        }
        match monitors::enumerate() {
            Ok(list) => {
                let d = monitor::diff(&self.monitors, &list);
                log::info!(
                    "monitors: {} added, {} removed, {} changed",
                    d.added.len(),
                    d.removed.len(),
                    d.changed.len()
                );
                self.sync(list, true);
                log::info!("{} wallpaper window(s) running", self.count());
            }
            Err(e) => log::error!("monitor enumeration failed: {e}"),
        }
    }

    /// Explorer restarted: its desktop windows (our parent) are gone, so rebuild everything.
    pub fn on_explorer_restart(&mut self) {
        log::warn!("Explorer restarted: rebuilding every wallpaper window");
        self.instances.clear();
        match desktop::find_host() {
            Ok(h) => self.host = h,
            Err(e) => {
                log::error!("cannot find the desktop host after the Explorer restart: {e}");
                return;
            }
        }
        match monitors::enumerate() {
            Ok(list) => {
                self.sync(list, false);
                log::info!("{} wallpaper window(s) running", self.count());
            }
            Err(e) => log::error!("monitor enumeration failed: {e}"),
        }
    }

    /// The monitors found by the last enumeration (left to right).
    pub fn monitors(&self) -> &[MonitorInfo] {
        &self.monitors
    }

    /// Replace the config and bring the wallpapers in line with it.
    pub fn apply_config(&mut self, cfg: Config) {
        self.config = cfg;
        self.tracker.set_scope(self.config.pause.scope);
        let list = self.monitors.clone();
        self.sync(list, false);
        log::info!("{} wallpaper window(s) running", self.count());
    }

    /// Manual pause (the sticky USER reason): pause or resume every player; players created
    /// later start in the same state.
    pub fn set_paused(&mut self, paused: bool) {
        self.tracker.set_global(PauseReasons::USER, paused);
        self.apply_pause_states();
        log::info!(
            "wallpapers {} by the user",
            if paused { "paused" } else { "resumed" }
        );
    }
}

/// Warning text when the file in `player` is unusually heavy (size, fps or bitrate).
fn heavy_video_notice(player: &Player, file: &Path) -> Option<String> {
    let num = |name: &str| player.property(name).and_then(|v| v.parse::<f64>().ok());
    let (w, h) = (num("width")?, num("height")?);
    let (fps, duration) = (num("container-fps")?, num("duration")?);
    if fps <= 0.0 || duration <= 0.0 {
        return None; // still image or unknown
    }
    let bytes = std::fs::metadata(file).ok()?.len() as f64;
    media::heavy_video_warning(w as u32, h as u32, fps, bytes * 8.0 / duration)
}
