#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("LiteWall runs on Windows only. (`cargo test --lib` runs the pure logic anywhere.)");
}

#[cfg(windows)]
fn main() {
    std::process::exit(win::run());
}

#[cfg(windows)]
mod win {
    use litewall::controller::{is_menu_click, Controller};
    use litewall::core::app::App;
    use litewall::core::config::{self, LoadOutcome, WallpaperSpec};
    use litewall::core::geometry::RectI32;
    use litewall::core::media::{parse_opt, parse_zoom};
    use litewall::core::monitor::MonitorInfo;
    use litewall::core::zoom::ZoomMode;
    use litewall::mpv::ffi::Api;
    use litewall::mpv::player::{Player, PlayerOptions};
    use litewall::platform::instance::{self, Instance};
    use litewall::platform::message_window::{self, AppEvent, MessageWindow};
    use litewall::platform::tray::Tray;
    use litewall::platform::window::WallpaperWindow;
    use litewall::platform::{autostart, desktop::HostError};
    use litewall::platform::{desktop, diag, monitors, WM_APP_QUIT, WM_APP_REFRESH};
    use litewall::runtime::Runtime;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    /// `%LOCALAPPDATA%\LiteWall`: config file and logs.
    fn data_dir() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("LiteWall")
    }

    fn log_dir() -> PathBuf {
        data_dir().join("logs")
    }

    /// `--name value` or `--name=value`.
    fn arg_value(name: &str) -> Option<String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let prefix = format!("{name}=");
        args.iter().enumerate().find_map(|(i, a)| {
            if a == name {
                args.get(i + 1).cloned()
            } else {
                a.strip_prefix(&prefix).map(str::to_string)
            }
        })
    }

    /// Every occurrence of `--name value` / `--name=value` (repeatable flags).
    fn arg_values(name: &str) -> Vec<String> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let prefix = format!("{name}=");
        let mut out = Vec::new();
        for (i, a) in args.iter().enumerate() {
            if a == name {
                if let Some(v) = args.get(i + 1) {
                    out.push(v.clone());
                }
            } else if let Some(v) = a.strip_prefix(&prefix) {
                out.push(v.to_string());
            }
        }
        out
    }

    /// After asking a running instance to quit: wait (up to 5 s) until its lock is free.
    fn wait_for_lock() -> Option<instance::Guard> {
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            match instance::acquire() {
                Ok(Instance::First(g)) => return Some(g),
                Ok(Instance::Existing) => {}
                Err(e) => {
                    log::error!("single-instance mutex failed: {e}");
                    return None;
                }
            }
        }
        None
    }

    /// Leak probe: does the process handle count grow when our own code runs in a loop?
    /// A1 = `find_host`, A2 = `enumerate`, A3 = wallpaper window create/destroy, B (needs a
    /// file) = window + mpv player lifecycle, honouring `--opt name=value` so variants can be
    /// compared (e.g. `--opt hwdec=no`, `--opt vo=null`).
    fn probe_handles(rounds: u32, file: Option<PathBuf>, extra: Vec<(String, String)>) -> i32 {
        use std::time::Duration;
        log::info!(
            "probe: {rounds} round(s) per phase, baseline handles = {}, mpv options {extra:?}",
            diag::process_handle_count()
        );
        let before = diag::process_handle_count();
        for _ in 0..rounds {
            let _ = desktop::find_host();
        }
        log::info!(
            "probe A1 find_host x{rounds}: {before} -> {} handles",
            diag::process_handle_count()
        );
        let before = diag::process_handle_count();
        for _ in 0..rounds {
            let _ = monitors::enumerate();
        }
        log::info!(
            "probe A2 enumerate x{rounds}: {before} -> {} handles",
            diag::process_handle_count()
        );
        let setup = || -> Result<_, Box<dyn std::error::Error>> {
            Ok((desktop::find_host()?, monitors::primary_rect()?))
        };
        let (host, rect): (desktop::Host, RectI32) = match setup() {
            Ok(t) => t,
            Err(e) => {
                log::error!("probe setup failed: {e}");
                return 1;
            }
        };
        let before = diag::process_handle_count();
        for _ in 0..rounds {
            match WallpaperWindow::create(&host, rect) {
                Ok(w) => {
                    std::thread::sleep(Duration::from_millis(100));
                    drop(w);
                }
                Err(e) => {
                    log::error!("probe A3 window failed: {e}");
                    return 1;
                }
            }
        }
        log::info!(
            "probe A3 window create/destroy x{rounds}: {before} -> {} handles",
            diag::process_handle_count()
        );
        let Some(file) = file else {
            log::info!("probe B skipped (add --play <file> to test the player lifecycle)");
            return 0;
        };
        let api: Rc<Api> = match Api::load() {
            Ok(a) => a,
            Err(e) => {
                log::error!("probe B: {e}");
                return 1;
            }
        };
        let warm_round = rounds.min(5);
        let mut warm = 0u32;
        let before = diag::process_handle_count();
        for i in 1..=rounds {
            let step = || -> Result<(), Box<dyn std::error::Error>> {
                // Locals drop in reverse order: the player first, then its window.
                let w = WallpaperWindow::create(&host, rect)?;
                let opts = PlayerOptions {
                    extra: extra.clone(),
                    ..PlayerOptions::default()
                };
                let mut p = Player::new(&api, w.hwnd(), 0, 0, opts)?;
                p.apply_zoom(&ZoomMode::Fill);
                p.load(&file)?;
                std::thread::sleep(Duration::from_millis(400));
                Ok(())
            };
            if let Err(e) = step() {
                log::error!("probe B round {i} failed: {e}");
                return 1;
            }
            if i == warm_round {
                warm = diag::process_handle_count();
            }
            if i % 5 == 0 {
                log::info!(
                    "probe B player round {i}: {} handles",
                    diag::process_handle_count()
                );
            }
        }
        let after = diag::process_handle_count();
        log::info!("probe B player x{rounds}: {before} -> {after} handles");
        if rounds > warm_round {
            log::info!(
                "probe B steady growth after round {warm_round}: {:.1} handles per round",
                (after as f64 - warm as f64) / (rounds - warm_round) as f64
            );
        }
        0
    }

    fn has_flag(name: &str) -> bool {
        std::env::args().skip(1).any(|a| a == name)
    }

    fn describe_monitor(i: usize, m: &MonitorInfo) -> String {
        format!(
            "monitor[{i}] id={} rect={:?} size={}x{} dpi={} ({}%) refresh={:.0} Hz primary={}",
            m.id.as_str(),
            m.rect_px,
            m.rect_px.width(),
            m.rect_px.height(),
            m.dpi,
            m.dpi * 100 / 96,
            m.refresh_hz,
            m.primary
        )
    }

    /// M2 manual test: attach a GDI test pattern behind the icons on the primary monitor.
    fn start_test_pattern() -> Result<WallpaperWindow, Box<dyn std::error::Error>> {
        let host = desktop::find_host()?;
        let monitor = monitors::primary_rect()?;
        log::info!(
            "test pattern: host=0x{:X} layout={:?} monitor={monitor:?}",
            host.hwnd,
            host.layout
        );
        Ok(WallpaperWindow::create(&host, monitor)?)
    }

    /// At login Explorer may not have built the desktop yet: wait (up to `limit`) instead of
    /// giving up on the first failed lookup.
    fn wait_for_host(limit: std::time::Duration) -> Result<desktop::Host, HostError> {
        let start = std::time::Instant::now();
        let mut attempts = 0u32;
        loop {
            match desktop::find_host() {
                Ok(h) => return Ok(h),
                Err(e @ (HostError::NoProgman | HostError::NoWorkerW))
                    if start.elapsed() < limit =>
                {
                    if attempts % 10 == 0 {
                        log::warn!("desktop not ready ({e}); waiting for Explorer");
                    }
                    attempts += 1;
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// The whole application: config, wallpapers (one per enabled monitor), tray icon.
    /// `play` overrides the default video for this session only (nothing is saved).
    fn start_app(
        play: Option<String>,
        config_path: PathBuf,
        fallback_zoom: ZoomMode,
        extra: Vec<(String, String)>,
        message_hwnd: isize,
    ) -> Result<Rc<RefCell<Controller>>, Box<dyn std::error::Error>> {
        let persist = play.is_none();
        let (mut cfg, outcome) = config::load_or_default(&config_path);
        match outcome {
            LoadOutcome::Loaded => log::info!("config loaded from {}", config_path.display()),
            LoadOutcome::Missing => log::info!(
                "no config at {} yet: starting empty (pick a video from the tray menu)",
                config_path.display()
            ),
            LoadOutcome::Recovered { backup, reason } => log::error!(
                "config {} is invalid ({reason}); moved to {}, starting empty",
                config_path.display(),
                backup.display()
            ),
        }
        if let Some(f) = play {
            cfg.default_profile = Some(WallpaperSpec {
                file: PathBuf::from(f),
            });
        }

        if persist {
            // Keep the Run entry in line with the config (also refreshes a moved exe path).
            match std::env::current_exe() {
                Ok(exe) => {
                    if let Err(e) = autostart::set(cfg.general.autostart, &exe) {
                        log::warn!("cannot sync the Run entry: {e}");
                    }
                }
                Err(e) => log::warn!("cannot find our own exe path: {e}"),
            }
        }

        let api = Api::load()?;
        let host = wait_for_host(std::time::Duration::from_secs(60))?;
        let list = monitors::enumerate()?;
        for (i, m) in list.iter().enumerate() {
            log::info!("{}", describe_monitor(i, m));
        }

        let mut rt = Runtime::new(api, host, message_hwnd, cfg.clone(), extra, fallback_zoom);
        rt.set_paused(cfg.general.paused_by_user);
        rt.sync(list, false);
        log::info!("{} wallpaper window(s) running", rt.count());

        let tray = Rc::new(Tray::new(message_hwnd, "LiteWall"));
        if let Err(e) = tray.add() {
            log::error!("cannot add the tray icon: {e}");
        }
        let controller = Rc::new(RefCell::new(Controller::new(
            App::new(cfg),
            rt,
            Rc::clone(&tray),
            config_path,
            message_hwnd,
            persist,
        )));

        controller.borrow_mut().flush_notices();
        controller.borrow_mut().start_observing();

        let handle = Rc::clone(&controller);
        message_window::set_event_handler(move |ev| {
            if let AppEvent::Tray(msg) = ev {
                if is_menu_click(msg) {
                    // Build under a short borrow, show WITHOUT holding it (the popup runs a
                    // nested message loop that must still be able to deliver events), then
                    // dispatch the choice.
                    let Ok(built) = handle.try_borrow().map(|c| c.build_menu()) else {
                        return;
                    };
                    if let Some(id) = tray.show_menu(&built.entries) {
                        if let Some(cmd) = built.commands.get(&id).cloned() {
                            match handle.try_borrow_mut() {
                                Ok(mut c) => c.dispatch(cmd),
                                Err(_) => log::warn!("menu choice ignored: the controller is busy"),
                            }
                        }
                    }
                }
                return;
            }
            match handle.try_borrow_mut() {
                Ok(mut c) => c.on_event(ev),
                Err(_) => log::warn!("{ev:?} ignored: the controller is busy"),
            }
        });
        Ok(controller)
    }

    pub fn run() -> i32 {
        let quit = has_flag("--quit");
        let test_pattern = has_flag("--test-pattern");
        let play = arg_value("--play");
        let config_arg = arg_value("--config");
        let mode_arg = arg_value("--mode");
        let _ = litewall::logger::init(&log_dir(), log::LevelFilter::Debug);

        // A typo or a flag from a newer build must fail loudly instead of silently starting an
        // idle instance (which also makes `Start-Process -Wait` hang).
        const KNOWN_FLAGS: [&str; 8] = [
            "--quit",
            "--test-pattern",
            "--play",
            "--config",
            "--mode",
            "--opt",
            "--list-monitors",
            "--probe-handles",
        ];
        for a in std::env::args().skip(1) {
            if let Some(rest) = a.strip_prefix("--") {
                let name = format!("--{}", rest.split('=').next().unwrap_or(""));
                if !KNOWN_FLAGS.contains(&name.as_str()) {
                    log::error!("unknown option {a:?} (known: {})", KNOWN_FLAGS.join(" "));
                    return 1;
                }
            }
        }

        // Diagnostic: log every monitor (id, rect, DPI, refresh) and exit. Works while
        // LiteWall is running; read the result in the log.
        if has_flag("--list-monitors") {
            log::info!(
                "DPI awareness PerMonitorV2: {}",
                monitors::is_per_monitor_v2()
            );
            return match monitors::enumerate() {
                Ok(list) => {
                    for (i, m) in list.iter().enumerate() {
                        log::info!("{}", describe_monitor(i, m));
                    }
                    0
                }
                Err(e) => {
                    log::error!("monitor enumeration failed: {e}");
                    1
                }
            };
        }

        // A launch with `--play`/`--config` replaces a running instance (its options would
        // otherwise be ignored); a plain second launch only asks it to reload its config.
        let takeover = play.is_some() || config_arg.is_some();
        // Diagnostic: `--probe-handles N [--play file]` (does not touch a running instance).
        if let Some(n) = arg_value("--probe-handles") {
            return match n.parse::<u32>() {
                Ok(n) if (1..=500).contains(&n) => {
                    let mut extra = Vec::new();
                    for raw in arg_values("--opt") {
                        match parse_opt(&raw) {
                            Some(kv) => extra.push(kv),
                            None => {
                                log::error!("bad --opt {raw:?}");
                                return 1;
                            }
                        }
                    }
                    probe_handles(n, play.clone().map(PathBuf::from), extra)
                }
                _ => {
                    log::error!("--probe-handles needs a number from 1 to 500");
                    1
                }
            };
        }

        let guard = match instance::acquire() {
            Ok(Instance::First(g)) => g,
            Ok(Instance::Existing) if takeover && !quit => {
                log::info!("another instance is running: asking it to quit so these options apply");
                instance::signal_existing(WM_APP_QUIT);
                match wait_for_lock() {
                    Some(g) => g,
                    None => {
                        log::error!("the running instance did not exit within 5 s");
                        return 1;
                    }
                }
            }
            Ok(Instance::Existing) => {
                let msg = if quit { WM_APP_QUIT } else { WM_APP_REFRESH };
                let ok = instance::signal_existing(msg);
                log::info!("second launch: forwarded msg=0x{msg:X} delivered={ok}");
                return 0;
            }
            Err(e) => {
                log::error!("single-instance mutex failed: {e}");
                return 1;
            }
        };
        if quit {
            log::info!("--quit: nothing is running");
            return 0;
        }

        let mode = match mode_arg.as_deref().map(parse_zoom) {
            None => ZoomMode::Fill,
            Some(Some(m)) => m,
            Some(None) => {
                log::error!(
                    "unknown --mode {:?} (use fill, fit, stretch, original, custom[:zoom,panx,pany])",
                    mode_arg.unwrap_or_default()
                );
                return 1;
            }
        };

        let mut extra_opts = Vec::new();
        for raw in arg_values("--opt") {
            match parse_opt(&raw) {
                Some(kv) => extra_opts.push(kv),
                None => {
                    log::error!("bad --opt {raw:?} (use --opt name=value; `wid` is reserved)");
                    return 1;
                }
            }
        }

        log::info!("LiteWall {} starting", env!("CARGO_PKG_VERSION"));
        log::info!(
            "DPI awareness PerMonitorV2: {}",
            monitors::is_per_monitor_v2()
        );
        let msg_window = match MessageWindow::create() {
            Ok(w) => w,
            Err(e) => {
                log::error!("cannot create message window: {e}");
                return 1;
            }
        };
        log::debug!("message window 0x{:X}", msg_window.hwnd());

        // Dropped in this order on every exit path: the controller (wallpaper players, then
        // windows, then the tray icon), then the test-pattern window, then the instance lock.
        let mut controller: Option<Rc<RefCell<Controller>>> = None;
        let mut pattern: Option<WallpaperWindow> = None;
        if test_pattern {
            match start_test_pattern() {
                Ok(w) => pattern = Some(w),
                Err(e) => {
                    log::error!("--test-pattern failed: {e}");
                    return 1;
                }
            }
        } else {
            let config_path = config_arg
                .map(PathBuf::from)
                .unwrap_or_else(|| data_dir().join("config.toml"));
            match start_app(play, config_path, mode, extra_opts, msg_window.hwnd()) {
                Ok(c) => controller = Some(c),
                Err(e) => {
                    log::error!("startup failed: {e}");
                    return 1;
                }
            }
        }

        let code = msg_window.run();
        message_window::clear_event_handler(); // releases the handler's Rc clones
        drop(controller);
        drop(pattern);
        log::info!("exiting with code {code}");
        drop(guard);
        code
    }
}
