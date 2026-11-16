# IMPLEMENTATION_PLAN.md — LiteWall

Companion to `CLAUDE.md` (the PRD). Tick boxes as work lands: `[ ]` → `[x]`.
Requirement IDs (F-xx) refer to the PRD. Work top to bottom; each milestone ends with a **Gate** that must pass before the next begins.

**Order logic:** de-risk the hardest unknown first (desktop embedding on Windows 11 24H2), then build outward: one monitor → many → tray/config → pause → audio → hardening → release.

**Scope (decided 2026-11-09, D2):** Windows 10 only for now (personal use; dev machine = Windows 10 LTSC 21H2, Classic WorkerW layout). Windows 11 support, including the 24H2/25H2 Raised layout, is **extended scope**: items marked *(extended)* are deferred and do not block any gate. This overrides the PRD's §10 priority table until the PRD is updated.

---

## Progress dashboard

| Milestone | Scope | Status |
|---|---|---|
| M0 | Feasibility spike | [x] **Gate passed 2026-11-10** (Win10 Classic, Option A; see `docs/m0-findings.md`). Open: libmpv build identity, `--vo`/`--hwdec` benchmarks |
| M1 | Project scaffolding | [x] **Gate passed 2026-11-10** (Windows build, 42 tests, clippy/fmt clean; idle WS ~5 MB, CPU 0). Open: CI first green run, README/LICENSE (D1), rest of module skeleton (arrives with M2-M5) |
| M2 | Desktop host (attach/detach) | [x] **Gate passed 2026-11-10** (Classic layout, Win10) |
| M3 | Single-monitor playback + zoom | [ ] (all zoom modes, hwdec and benchmarks done; open: GIF/WebP check, explicit "no stray mpv window" confirmation) |
| M4 | Multi-monitor | [x] **Gate passed 2026-11-11** (3 monitors, mixed DPI/refresh, negative origins; 20-cycle hot-plug soak; ids stable across a reboot; known issue D15) |
| M5 | Tray + config + persistence | [x] **Gate passed 2026-11-12** (launch -> tray -> pick files -> per-screen zoom -> quit -> relaunch restores everything; autostart works). Closed 2026-11-14: step 4 (reload behaviour D16, Advanced submenu, unthrottled click feedback) and the missing-file / failed-player notice verified by the owner |
| M6 | Pause engine | [x] **Gate passed 2026-11-15** (per-screen cover pause, lock/unlock and screen-off, presentation mode, exclusive fullscreen (state 3) and borderless games pause all screens, tray toggle for the fullscreen scope, paused GPU 0 %). Carried to M8: real sleep/wake, remote desktop, fast user switching. Known limitation D18: fullscreen on a non-primary screen is only a per-screen cover (future idea) |
| M7 | Audio | [ ] **in progress** (step 1 verified 2026-11-15: audio source switching, volume presets, persistence, disable, cover, missing-screen notice; open: **non-primary screens play faster (B1)**, step 2 = auto-mute / duck when another app plays audio (F-44 pulled forward)) |
| M8 | Robustness / self-healing | [ ] |
| M9 | Performance pass | [ ] |
| M10 | Packaging & first release | [ ] |
| P1 | Fast-follow features | [ ] |
| P2 | Later features | [ ] |

### Execution log

- 2026-11-09: Pure logic in `core/` (`zoom`, `pause`, `coverage`, `geometry`) written and tested on Linux: 42 tests pass, `clippy -D warnings` and `fmt --check` clean. Win32/mpv code is not started because M0 (needs Windows) gates it. Items below are ticked only where the pure part is fully done; the Win32 part of the same line is noted as pending.

- 2026-11-10: The `core/` sources from the 2026-11-09 entry were not present in this workspace, so they were **re-created from the PRD** and re-verified: `geometry`, `zoom`, `pause` (+`PauseTracker`, `Hysteresis`), `coverage`, `monitor` (new), `config` (new). Result: **39 tests pass** (rustc/cargo 1.75 in the sandbox), `cargo fmt --check` and `cargo clippy --all-targets -D warnings` clean. Not yet run on the owner's Windows toolchain. Added `spike/` (M0), CI workflow file, `docs/` placeholders. Note: sandbox cargo is 1.75, so `Cargo.lock` is not shipped; generate it on the owner's machine.

- 2026-11-10 (later): **M0 gate passed** on the owner's Windows 10 machine (details: `docs/m0-findings.md`). Found and fixed a spike bug: `SetTimer(NULL, id, ..)` ignores `id`, so timers must be matched by the returned id. D3 = A, D4 = hand-written FFI.

- 2026-11-10 (M1): added `logger.rs` (rotating file sink, 3 tests), `platform/{sys,instance,message_window}.rs`, `main.rs` wiring (`--quit` flag), manifest + `build.rs` (`embed-resource` 3). Pure suite: **42 tests pass**, `clippy --all-targets -D warnings` clean (Win32 layer checked with `cfg(windows)` stripped). Not yet built on Windows.

- 2026-11-10 (M1 verified): first Windows build OK (embed-resource + SDK 26100 `rc.exe`), `cargo test --lib` 42 pass, `clippy --all-targets -D warnings` and `fmt --check` clean on Windows. Verified from logs: mutex, `--quit` (0x8001), second-launch refresh (0x8002), exit code 0. Idle: one process after the second launch exits, WS ~5.3 MB, private ~1.1 MB, 4 threads, CPU 0 over 10 s. Fixed on the way: `logger.rs` was missing, `platform/mod.rs` held a copy of `core/mod.rs`, log lines from two processes interleaved (now one `write_all` per record), rustfmt pass. `Cargo.lock` now exists (generated on the owner's machine): commit it.

- 2026-11-10 (M2): **M2 gate passed** for Classic layout on Windows 10. Implemented desktop host discovery, wallpaper window attachment/detachment, coordinate mapping, and GDI test pattern rendering. *Note on quit:* `RedrawWindow` on the host didn’t clear leftover pixels; calling `SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, NULL, 0)` after destroying the window fixed it while leaving the user's wallpaper setting untouched.

- 2026-11-10 (M3, partial): `mpv/ffi.rs`, `mpv/player.rs`, `core/media.rs` in place; libmpv client API 2.6 loads and `Player::new(wid)` works on the owner's machine. F-08 failure paths verified on Windows: missing file (clean exit) and corrupt file (`mpv error -17` -> wallpaper window hidden, process stays alive). Log showed a window + player being created and torn down before the missing-file check (possible flicker): fixed by `Player::preflight()` running before window creation (to re-verify on Windows). Added repeatable `--opt name=value` (mpv overrides applied last; `wid` reserved; `parse_opt` unit-tested) and `scripts/bench.ps1` (untested) for the benchmark item. Remaining in M3: live zoom check per mode, GIF/WebP check, hwdec confirmation, benchmark table.

- 2026-11-10 (M3, benchmarks run 1): 49 tests pass; `preflight` and `--opt` verified on Windows (missing file logs only the error; rejected option warns; `--opt wid=` refused). First real benchmark (1080p60 H.264, one monitor, 2560x1600@120): hwdec=d3d11va confirmed, `d3d11va-copy` worse, software decode ~4.6 % CPU, **`video-sync=audio` cut GPU 3D from ~20 % to ~8 %** (single run, to be confirmed). Numbers in `docs/benchmarks.md`. `bench.ps1` revised (repeat runs, elapsed-time CPU %, `video*` engine match for AMD `video codec 0`, more variants).

- 2026-11-10 (M3 -> M4): repeat benchmark run done by the owner (engine list confirmed: AMD `video codec 0`). Owner saw real judder with `video-sync=audio`: **D12** keeps `display-desync`. Loop-seam smoothing triaged as later scope (**D13**, P2 F-12b). M4 started: `core/monitor.rs` (`choose_id`, `dedupe_ids`, 2 tests), `platform/sys.rs` (+`MONITORINFOEXW`, `DISPLAY_DEVICEW`, `DEVMODEW`, size-asserted), `platform/monitors.rs` (`enumerate()`), `--list-monitors`. Not yet built or run.

- 2026-11-10 (M4 step 1 verified, step 2 written): `--list-monitors` works on the owner's machine (3 monitors, mixed DPI/refresh, negative origins). Step 2: `core/reconcile.rs` (pure `plan()` -> create/remove/move/load/zoom, 9 tests), `runtime.rs` (HashMap of window+player per monitor), `--config <file>`, `--play` now covers all monitors, `AppEvent` + debounced display-change timer in `message_window.rs`, `WallpaperWindow::move_to`, second launch reloads the config. `docs/config.example.toml` has the owner's real ids. Not yet built or run.

- 2026-11-11 (M4 step 2 verified, step 3 written): 60 tests pass, clippy clean, 3 monitors with 2 videos work; per-monitor zoom, enable/disable via config reload, DPI/scaling change and Win+P cycles worked. Owner reports: (a) portrait monitor with `other.mp4` (2160x3840, 25 fps, 208 Mbps) looks cropped/odd; (b) on a repeat run nothing loaded: **cause found**, the earlier idle instance (started without `--config`) swallowed the new launch because a second launch only forwards a refresh message and ignores its options; (c) the taskbar disappeared once and needed `restartExplorer.bat` (during that run LiteWall had no windows, so probably not LiteWall; evidence requested: Event Viewer + litewall.log). Step 3: `--config`/`--play` launch now replaces a running instance; `TaskbarCreated` -> debounced `Runtime::on_explorer_restart` (re-attach after an Explorer restart, an M8 item); mpv properties `video-codec`, `width`, `height`, `osd-width`, `osd-height` are logged per player (`mpv#N`); default `vo=gpu` (D14) in `player.rs`. Not yet built or run.

- 2026-11-11 (M4 step 3 verified): owner confirms everything works, including a relaunch with `--config` (replaces the running instance) and `restartExplorer.bat` (wallpapers re-attach by themselves). Log check: mpv's window size equals each monitor (1920x1080 / 1080x1920 / 2560x1600), `d3d11va` on all three players, so the portrait "cropped" look is not a sizing bug (9:16 clip on a 9:16 monitor, Fit = Fill). Event Viewer showed no Explorer/LiteWall/mpv crash and no display-driver reset in the 6 h window: the earlier vanished taskbar is unexplained. Next: `scripts/hotplug.ps1` soak (20 x `DisplaySwitch /internal` <-> `/extend`), reboot check of the monitor ids, then M5.

- 2026-11-11 (hot-plug soak, 3 cycles): process handles flat (2941-2942), private bytes flat (1426-1439 MB, no growth). But the script saw "3 wallpaper window(s) running" after every `/internal` switch too (expected 1), so it proved nothing about the rebuild: either its 6 s wait was too short or the switch did not reach the handler. `hotplug.ps1` now waits for a new log line and reports Handled/Seconds per switch. Memory note: private bytes (commit) for 3 players incl. a 4K 208 Mbps clip are ~1.4 GB, while Task Manager showed < 450 MB (a different counter): measure both per player in M9.

- 2026-11-11 (hot-plug, manual Win+P): log proves the display-change path works end to end, twice: `display message 0x7E` -> 400 ms debounce -> `monitors: 0 added, 2 removed` -> `1 wallpaper window(s) running` (0.52 s after the message), then `2 added` -> `3 wallpaper window(s) running`. The scripted `DisplaySwitch.exe` calls had produced NO events (nothing logged between the instance start and the manual switches), so `hotplug.ps1` was rewritten to call `SetDisplayConfig` directly and report its return code; its first rewrite failed to parse because `Switch` is a PowerShell keyword (function renamed `InvokeStep`).

- 2026-11-11 (hot-plug soak, 3 scripted cycles via SetDisplayConfig, Rc=0): every switch handled in ~3 s (1 window after internal, 3 after extend); memory back to ~1.44 GB after each extend (no growth). One row said Handled=False after 27.5 s although the log showed the right state: the log file had rotated mid-run, which reset the script's line count (script now compares the last log line instead). Handles drifted up a little: extend rows 2937 -> 2941 -> 2942 -> 2952, internal rows 1155 -> 1160 -> 1168 (about +5 per cycle): needs the 20-cycle run to tell noise from a leak. `find_host` now dumps the window tree only on the first lookup and on failure (it was ~40 log lines per display change).

- 2026-11-11 (hot-plug soak, 20 cycles = 40 switches): all 40 handled in ~3.3 s each, `Rc=0`, window counts always right (1 after internal, 3 after extend). Memory flat (private 1430-1455 MB after extend, working set ~300-316 MB, no trend). **Handle count grows linearly: +113 over 19 cycles on both the 1-window and the 3-window rows, i.e. ~6 handles per cycle** (a small leak: ~290 per 50 cycles, within the M8 soak target to fix). Added `--probe-handles N [--play file]` (in-process loop of `find_host`, `enumerate`, and window+player create/destroy, logging the handle count) to tell our code from mpv/driver. M4 remaining: handle-leak triage, monitor-id reboot check.

- 2026-11-11 (handle-leak probe, run 1): `find_host` x20 and `enumerate` x20 do not leak (72 -> 72 handles). The window + mpv player create/destroy loop does: 146 -> 374 over 20 rounds, i.e. ~108 once-off in the first rounds, then a steady **+8 handles per round**. So the leak is in the wallpaper window or in mpv/D3D11/driver, not in our monitor/desktop code. Next: probe splits window-only vs player, and `--opt hwdec=no` / `--opt vo=null` variants to find the layer. Workaround candidate if it is in mpv/driver: keep players alive across display changes (re-parent/hide) instead of destroy + create.

- 2026-11-13 (M5 step 3 verified, step 4 written): owner ran step 3 (80 tests): the 4K30 file runs smoothly on the primary once one of the other two screens is disabled (two 4K decodes fine, three overload the iGPU), and asked for a lighter-copy feature (now the first P1 item) and a GUI settings panel instead of config editing (F-51, raised). `questions.txt` reload tests, explained by the log: cutting the `[audio]` block or its header gave a valid file, so nothing to warn about, and reload only read the file and never wrote it back (hence "cut-out content not restored"); with the file deleted every `ReloadConfig` logged `cannot read ... config.toml`, but the notification was rate-limited to once per 30 s, so repeated attempts showed nothing (the one pop-up the owner saw was the first); no `PickFile` was attempted in the log while the file was missing, so "change video does not work" is not reproduced (a pick recreates the file). The "very heavy video" notice fired for a 2160x3840 30 fps file at only ~26 Mbps with wording that blamed the bitrate: reworded per reason. Step 4 (not yet built or run): D16 reload behaviour, "Advanced" submenu, unthrottled click feedback, `encoder-list` logging, new wording. Next: M6 (pause engine).

- 2026-11-14 (M5 closed): owner ran the five-point step 4 pass and the missing-file notice test; everything passed (81 tests). `Runtime` now collects deduplicated notices (`take_notices`) from `create` and `Action::Load` failures and `Controller::flush_notices` shows them after every command/event and once at startup. Encoder check from "Log playback stats": libx264 no, libx265 no, h264_amf yes, hevc_amf yes, h264_mf yes, h264_nvenc yes, h264_qsv yes (see the P1 lighter-copy item).

- 2026-11-14 (M6 step 1 verified): `platform/events.rs` (`SetWinEventHook`, five ranges, trailing 200 ms debounce on a one-shot timer) and `platform/fullscreen.rs` (window filter) feed `coverage.rs`; observe-only. The owner's nine checks passed. Log findings: a maximized window logs `COVERED 100 %` with `fg_fills=true`; the primary screen at 69 % with windows partly on it is correctly not covered (threshold 0.9); several overlapping windows add up to 100 % through the grid; Alt-Tab produces covered/uncovered flapping about every 0.5 s, which is real (hysteresis alone does not hide that, pausing and resuming is cheap).

- 2026-11-14 (M6 step 2 verified): `PauseTracker` + per-screen `Hysteresis` + mpv `pause`, one-shot timer armed only while a change is pending, manual pause is the sticky `USER` reason. The first run used a stale exe: the zip stores file times without a timezone, Windows read them as local time (UTC+7), the files looked older than the last build and cargo skipped the rebuild (the log still had step 1 strings). Fix: every apply script now sets `LastWriteTime` on `src\*.rs` before `cargo build`, and the zips carry later dates. After a real rebuild: `PAUSED (reasons 0x2)` about 0.3 s after a window covers a screen, `RESUMED` on uncover; litewall.exe GPU 0 % with all screens covered, about 13 % with one video visible, about 19 % with two (Desktop Window Manager only costs GPU while a screen is uncovered); manual pause stays through cover/uncover; the window peek shows a paused frame. RAM stays about 300 MB while paused (memory is an M9 item; the earlier ~35 MB reading after a manual pause did not reproduce).

- 2026-11-14 (M6 step 3 verified in part, step 3b written): `core/power.rs` (pure session/power mapping, 5 tests), `WTSRegisterSessionNotification`, `WM_POWERBROADCAST`; 86 tests pass. Verified: Win+L and unlock (`session event 7/8`, all screens `PAUSED (reasons 0x10)`, resume on unlock), a screen that was covered stays paused after unlock (reason falls back to 0x2), manual pause survives lock and unlock. Not verified: remote desktop and fast user switching (skipped by the owner). Sleep: no `power event` line was logged at all, so `PBT_APMSUSPEND` is not delivered on this laptop (probably Modern Standby); after waking, litewall.exe GPU shows a brief 0 -> 15 % before the animation appears, which means the video was not paused while asleep. Step 3b (not yet built or run): `RegisterPowerSettingNotification(GUID_CONSOLE_DISPLAY_STATE)`, new reason `DISPLAY_OFF` (0x100), `AppEvent::DisplayPower`, one more unit test (87 expected); D17.

- 2026-11-14 (M6 step 3b verified in part, step 4 written): 87 tests pass. Screen-off verified: `display power state 0: screens off` pauses the screens that are still playing (`PAUSED (reasons 0x100)`), `state 1` plus the unlock resume them, and a screen that was already covered or locked stays paused until its own reason clears. Real sleep: the owner reports litewall.exe GPU still jumps 0 -> 15 % after the animation appears, but the log has no gap and no `display power state` / `power event` around any sleep, so the sleep itself is not captured; open until a log with the sleep inside is available (`powercfg /a` and the Kernel-Power events 42/107/506/507 tell which kind of sleep this laptop uses). Step 4 (not yet built or run): `SHQueryUserNotificationState` on every debounced window event; states 3 (exclusive D3D fullscreen) and 4 (presentation mode) set the global FULLSCREEN reason (all screens), `QUNS_BUSY` is ignored on purpose (ordinary fullscreen windows are covered per screen); the 1 s one-shot fallback timer runs only while a window exactly fills a screen (a framed maximised window does not count) or FULLSCREEN is set. `foreground:` log lines moved to trace level. 89 tests expected.

- 2026-11-14 (M6 step 4 run, owner): 89 tests pass, clippy clean. Results from the log: **presentation mode** (`presentationsettings.exe`) gives `notification state 5 -> 4`, `PAUSED (reasons 0x4)` on every screen that was not already paused and `RESUMED` on leaving; a screen that was already `COVERED` (0x2) correctly stays paused until its own reason clears; lock gives state `1` (NOT_PRESENT), ignored by design (the session event pauses with 0x10). **Games**: a fullscreen game and a borderless game on one screen gave `PAUSED (reasons 0x2)` on that screen only (the game even changed Screen 3 to 1920x1200), and the notification state was only ever `2` (BUSY), never 3, so exclusive-fullscreen detection (state 3) is **unverified**: on Windows 10 most fullscreen games are flip-model borderless. A browser in F11 and YouTube fullscreen gave `COVERED` and no state change (state stayed 5); the game-like windows (classes `FDKAppClass`, `Application`, `Artemis`) gave state 2 about 0.2-1 s after the cover. The other screens kept playing, which is the per-screen design but not what the owner wants for games on a shared iGPU. The state also lags a window by up to ~1 s and returns to 5 about 1 s after it closes (this is why the 1 s timer exists).

- 2026-11-15 (M6 step 4b written, not yet built or run): owner decision: a lifestyle wallpaper must not compete with GPU-heavy programs, but pausing every screen for any covered screen (`scope = all_monitors`) is too aggressive. New rule (D18): the **foreground** window, passing the existing window filters, with **no caption** (`WS_CAPTION`), **exactly** filling one screen (a framed maximised window overhangs by ~8 px) and notification state **2 or 3** is a "true fullscreen app" and sets the global FULLSCREEN reason (all screens), debounced with a second `Hysteresis` (250 ms / 100 ms). Ordinary maximised (framed) windows still only pause their own screen. (Corrected after the run, see the verified entry: a browser in F11 / YouTube fullscreen on the **primary** screen is also state 2 and pauses all screens.) New config key `pause.fullscreen_app_scope` (`all_monitors` default, `this_monitor`) and tray item "Pause all screens for fullscreen apps" (checked = all screens). States 3 (exclusive) and 4 (presentation) keep pausing all screens regardless of the setting. The 1 s timer now also runs while a fullscreen app is detected, and logs one debug line when it is armed and one when it stops (`fullscreen timer armed (...)` / `fullscreen timer stopped`), so it can be checked from the log. Log lines: `fullscreen app: <class> on Screen N (notification state S): pausing all screens` and `fullscreen app ended: resuming the screens it paused`. Files: `core/{coverage,config,app,menu}.rs`, `runtime.rs`, `platform/{fullscreen,sys}.rs`. 4 new unit tests (93 expected on Windows; 92 pass in the Linux sandbox, where the Windows-only player test does not run); the Windows-only files could not be compiled in the sandbox.

- 2026-11-15 (M6 step 4b verified, owner): 93 tests pass, clippy clean, release build OK. Games and borderless games that report state 2 pause every screen (several titles, see the checklist); an Unreal game gave state 3 (exclusive fullscreen is now seen) and the global 0x4 pause; presentation mode unchanged; tray toggle off restores per-screen pausing (verified twice); a framed maximised window pauses only its own screen with no timer lines. **Correction to the 4b design note:** F11 / YouTube fullscreen on the primary screen also gives state 2 and pauses all screens, on a non-primary screen the state stays 5 and only that screen pauses, because the notification state only describes the primary monitor (D18 limitation). Not detected: Touhou games (the log shows exact fit and state 2 but no `fullscreen app` line, probably the caption check) and a few niche titles; the owner accepts per-app rules (F-35) for these outliers. A fourth display (`UGD1302`) appeared in the pause lines at 17:28 UTC. Next: optional step 4c (log why a candidate is rejected), then real sleep, remote desktop and fast user switching.

- 2026-11-15 (M6 step 4c written, not yet built or run): diagnostics only, no behaviour change. When `fullscreen_app_scope` is all screens, the state is 2 or 3, an exactly-fitting window exists and no fullscreen app was detected, the runtime logs once per distinct report: `fullscreen app not detected (state S): foreground "<class>" rect=... style=0x.. ex=0x.. fits_screen=<screen or None> blocked_by=[...]` (reasons: not visible, minimised, our own window, tool window, transparent, layered alpha 0, cloaked, shell class, has caption, does not exactly fill a screen). Purpose: find out why Touhou-style games are missed and whether the no-caption check should be relaxed. Files: `runtime.rs`, `platform/fullscreen.rs`. Test count unchanged (93). Owner also confirms litewall.exe shows 0 % GPU while the wallpapers are paused during a game.

- 2026-11-15 (M6 step 4c run, step 4d written): 4c worked: Touhou in borderless mode logged `fullscreen app not detected (state 2): foreground "Touhou ..." rect 0,0-2560,1600 style=0x94CA0000 fits_screen=Some(2) blocked_by=["has caption"]`, so the game keeps `WS_CAPTION` while it fills the screen. Its exclusive mode gave state 3 and paused all screens (`fullscreen app: ... (notification state 3)`), borderless only paused its own screen. Oneechanbara Origin still is not detected in any mode (accepted, per-app rules F-35). F11 / YouTube fullscreen on a non-primary screen still pauses only that screen (D18 limitation). **Step 4d (not yet built or run)**: the no-caption requirement is removed from the fullscreen-app rule (`coverage::fullscreen_app_monitor(monitors, foreground, state)`, `fullscreen::foreground_candidate() -> (class, rect)`); the state 2/3 gate plus the exact fit already separate games from framed maximised windows (an ordinary maximised window overhangs by ~8 px and leaves the state at 5). `foreground_report` now prints `caption=<bool>` as information. Files: `core/coverage.rs`, `platform/fullscreen.rs`, `runtime.rs`. Test count unchanged (93).

- 2026-11-15 (M6 step 4d verified, M6 closed): 93 tests pass, clippy clean. Touhou in borderless mode: `fullscreen app: Touhou ... (notification state 2): pausing all screens`, both screens resumed in the same millisecond when it ended, and the cover/uncover pauses afterwards behave as before. Owner skips fullscreen on non-primary screens (Future ideas, open-ended). **Gate M6 passed.** Carried to M8: real sleep/wake, remote desktop, fast user switching. Next: M7 (audio).

- 2026-11-15 (M7 step 1 written, not yet built or run): `core/audio.rs` (new: `AudioState`, `state_for`, `source_status`, `VOLUME_PRESETS`, 5 tests), `UserCommand::{SetAudioSource, SetVolume}` (2 tests), tray "Audio" submenu in `core/menu.rs` (4 tests), `Player::set_audio` (`volume`, `aid=auto|no`, `mute` in a blip-free order), `Runtime::apply_audio` (called at the end of every `sync`, so config changes, display changes and Explorer restarts all re-apply it; players are created with the right audio state from the start), `Runtime::take_audio_notices` + controller title "Audio". 11 new unit tests: 103 pass in the Linux sandbox, 104 expected on Windows. The Windows-only files could not be compiled in the sandbox. Test needs a video that has an audio track.

- 2026-11-15 (M7 step 1 verified, owner): 104 tests pass, clippy clean. Log and owner: "No audio" is silent; picking a screen plays that screen's audio (log: one `audio ON`, the previous one `audio off`, ~3 ms apart); volume presets change `audio ON (volume N)`; relaunch restores source and volume; covering the source screen stops its audio and uncovering resumes it; disabling the source screen silences it; the 4th display (`UGD1302`) vanished while it was the source: `the audio screen is not connected: everything stays silent` and exactly one notice. mpv logs `Audio/Video desynchronisation detected` every ~20-60 s on the audio screen (that screen's video runs ahead of its audio). **Bug B1** (non-primary screens play 1.25x-1.5x too fast) is open, see the M7 checklist. New requests: auto-mute / duck when another app plays audio (step 2), ambient audio passthrough (P1 + extended).

- 2026-11-12 (M5 gate passed, step 3 written): owner verified the file dialog (all screens and per screen), zoom, autostart (Run entry present; after sign-out/in the wallpapers are back in 2-3 s) and calls the app "good enough to use daily". The reload-safety test did not fail: the owner deleted the `[audio]` header, which leaves `volume = 40` as an unknown key under `[general]`, still valid TOML; unknown keys are ignored by design, so no warning (idea: log/notify unknown keys, P1). Performance report: a 4K30 ~200 Mbps video on all three screens stutters on the primary (2560x1600 @ 120 Hz VRR) but not on the two 60 Hz screens: the same heavy file is decoded three times on one iGPU video engine and the primary also has the largest and fastest-presented surface. Step 3 (not yet built or run): a "very heavy video" notification (4K at 30 fps or more, or an average bitrate of 80 Mbps or more; `core/media.rs::heavy_video_warning`, raised when the video output is configured via the new `PlayerEvent::VideoReady`), and a tray item "Log playback stats (diagnostics)" that logs each screen's fps and dropped frames. Ideas recorded: shared decode when several screens show the same file (needs the libmpv render API, large), pulling the import optimizer (F-12) earlier.

- 2026-11-12 (M5 step 1 verified, step 2 written): owner confirms the tray icon and every menu item that exists, per-screen enable/disable and zoom, pause/resume, open-config-folder, persistence across quit/start and an Explorer restart (per-screen file and zoom restored), and that menu clicks do not disturb playback. Findings: (a) "Reload config" could wipe the settings when the file was missing or invalid (it re-applied defaults): now non-destructive, a bad file keeps the current settings and shows a notification, label "Reload config from file"; (b) `coverage_threshold` was saved as 0.8999999761581421 (f32): now f64 and rounded; (c) taskbar flicker on launch, startup fade-in, per-monitor pause, image wallpapers and detaching a screen came up as ideas, see M6/M9/P2 and F-14. Step 2 (not yet built or run): file dialog on a worker thread (`GetOpenFileNameW`, filters for videos, GIF/WebP, images), images via `image-display-duration=inf`, "Start with Windows" through the Run key (synced with the config at every start), startup waits up to 60 s for Explorer instead of exiting, tray notifications (rate-limited) for config errors and large GIF/WebP.

- 2026-11-11 (M4 gate passed, M5 step 1 written): the monitor ids matched after a reboot (`check-ids.ps1`: OK x3), which closed M4. M5 step 1 (not yet built or run): `core/app.rs` (pure `App::update(UserCommand) -> Vec<Effect>`, 8 tests), `core/menu.rs` (pure tray menu model, 9 tests), `platform/tray.rs` (raw `Shell_NotifyIconW` + popup menu), `platform/shell.rs` (open folder), `controller.rs` (events -> commands -> effects), `Runtime::{apply_config, set_paused, monitors}`. A launch without arguments is now the real app: it loads `%LOCALAPPDATA%\LiteWall\config.toml`, restores the wallpapers and shows the tray icon; `--play` is a session override that is never saved; a plain second launch reloads the config. The event handler is now `Fn` (re-entrant) so mpv and display events still arrive while a tray menu is open. Not yet wired: file dialog, autostart registry entry (both log a warning), notifications.

- 2026-11-11 (handle-leak triage closed): `vo=gpu-next` leaks 8.0/round and `gpu-api=opengl` 9.0/round (vs `vo=gpu` 8.0, `hwdec=no` 8.0, `vo=null` 0), so the leak is common to mpv's GPU outputs / the driver: **D15 known issue**, no further digging. M4 left: monitor-id reboot check (`scripts/check-ids.ps1`).

- 2026-11-10 (M3 zoom verified): owner confirmed Fill, Fit, Stretch, Original and Custom all look right on the primary monitor. The first `--list-monitors` attempt hung because the exe was still the pre-M4 build: it ignored the unknown flag and started an idle instance, so `Start-Process -Wait` never returned. `m4/main.rs` now rejects unknown `--` options. Still open in M3: GIF/WebP check, explicit "no stray mpv window" confirmation.

- 2026-11-10 (benchmarks run 2, 3 runs per variant): noise < 1 %. `vo=gpu` wins with display sync (-12 % GPU 3D, -29 MB) -> **D14**. Video engine ~12 % with `d3d11va`; software decode 2.3 % CPU. `d3d11va-copy` worse everywhere. Base memory ~300 MB regardless of decode path (M9 item). Full table in `docs/benchmarks.md`.

---

## Environment (dev machine) and pre-flight

Recorded 2026-11-09. Update this section whenever the machine or toolchain changes.

### Existing, verified

| Item | Value | Source |
|---|---|---|
| OS | Windows 10 IoT Enterprise LTSC 2021, 21H2, build 19044.7807 | registry query |
| Desktop layout on this machine | **Classic (WorkerW) only** | build 19044 predates the 24H2 change |
|Kei's Windows Rust dev machine — toolchain versions, installed cargo tools, Windows SDK, git: 
	OS is Windows 10 IoT Enterprise LTSC 2021 (21H2, build 19044); smoke-test cargo build/run works with MSVC
	Rust stable 1.98.1 on x86_64-pc-windows-msvc, cargo 1.98.1, with aarch64-pc-windows-msvc target
	rustup components installed: clippy, rustfmt, rust-src, rust-docs
	cargo tools installed: cargo-audit 0.22.2, cargo-bloat 0.12.1, cargo-deny 0.20.2
	Windows SDK 10.0.26100.0 present; git 2.51.0
	HP workstation with AMD Ryzen 8845HS, integrated AMD Radeon graphics only (driver 32.0.32015.7029); display reports 2560x1600 at 120 Hz|
| Rust | stable 1.98.1 (2026-09-01), host `x86_64-pc-windows-msvc`, LLVM 22.1.8 | `rustc -vV` |
| Cargo | 1.98.1 | `cargo --version` |
| Extra targets | `aarch64-pc-windows-msvc` (only needed for P2 ARM64) | `rustup show` |
| Components | clippy, rustfmt, rust-src, rust-docs | `rustup component list` |
| Cargo tools | `cargo-audit` 0.22.2, `cargo-bloat` 0.12.1, `cargo-deny` 0.20.2 | `cargo install --list` |
| Windows SDK | 10.0.26100.0 (provides `rc.exe` for `embed-resource`) | `Windows Kits\10\Lib` |
| Git | 2.51.0.windows.1 | `git --version` |
| MSVC linker | works: a fresh `cargo new` + `cargo run` built and ran | smoke test |
| Prior experience | Glide (Rust, `windows` crate, hook, tray, autostart, TOML config) is finished and working | owner |

Notes:
- `vswhere` printed nothing, but the smoke test proves the MSVC linker is usable. No action needed.
- The SDK version (26100) is newer than the OS. Do not infer the desktop layout from the SDK.
- Win10 build 19044 supports everything planned so far: PerMonitorV2 DPI, high-resolution waitable timers, `SetWinEventHook`, `WTSRegisterSessionNotification`, `SHQueryUserNotificationState`. Avoid any Win11-only API.
- As far as I know, IoT Enterprise LTSC 2021 has a much longer support window than regular Windows 10. Verify before quoting a date in the README.

### Still to obtain or install

- [ ] **libmpv x64**: `libmpv-2.dll` (LGPL variant per the PRD's licensing note). Record version, source URL and SHA-256 here. Place next to the exe in the spike crate.
- [ ] Optional: a second, standard (GPL) libmpv build for comparison in M0.
- [ ] Test media: 1080p30 H.264 MP4 (about 10–30 s, loopable), later 1080p60 H.264, 4K30 HEVC, 1080p AV1 and a mid-size GIF for M3/M9.
- [ ] *(extended)* **Windows 11 24H2/25H2 test environment** (VM or second PC). Only needed once Windows 11 support is taken up; not required for the M0 gate. Hyper-V (Gen 2 VM with vTPM) or VirtualBox/VMware on this host are options; check that the VM feature is available on this edition. A VM is fine for window parenting and visibility checks but not for judging GPU decode.
- [ ] *(extended)* Optional: a Windows 11 ≤23H2 environment, to cover the second Classic variant listed in the PRD test matrix. Windows 10 on the dev machine covers Classic only on Win10.
- [ ] Optional: a code-signing certificate decision (M10, not needed earlier).

Not needed: extra Rust components, Visual Studio changes, nightly toolchain (Miri is not in this plan), ARM64 tooling until P2.

### Information still needed from the owner

- [ ] GPU model(s) and driver version: `Get-CimInstance Win32_VideoController | Select Name, DriverVersion, CurrentRefreshRate, CurrentHorizontalResolution, CurrentVerticalResolution`
- [ ] Monitor count, sizes, refresh rates, DPI scaling, and which sits left of or above the primary (the PRD mentions three monitors)
- [ ] *(extended)* Whether Hyper-V or another VM tool is available for a Windows 11 test environment
- [ ] Decision D1 below (D2 is decided: Windows 10 only)
- [ ] Optional: Glide's `tray.rs`, `autostart.rs` and single-instance code, so LiteWall matches existing patterns

### How this build is run

- Claude cannot run Windows-specific code (desktop embedding, mpv, tray). Win32 and mpv files are delivered untested, each with a short manual test checklist, and the owner is the test bench.
- Pure logic in `core/` (zoom, pause, coverage, config, monitor diff) is unit-tested on Claude's side where a toolchain is available, and again on the owner's machine with `cargo test`.
- Files are delivered one by one after each task is finished. Revisions come from files the owner sends back; changes are targeted edits, not rewrites.
- M0 is throwaway spike code. M1 and later do not start until the M0 gate passes.

---

## Decision log (fill in as decided)

- [ ] D1. Project license: ______ (decides GPL vs LGPL libmpv build)
- [x] D2. Minimum OS: **Windows 10 only** (decided 2026-11-09; personal use, tested on Windows 10 LTSC 21H2). Windows 11, including 24H2/25H2, is extended scope.
- [x] D3. Rendering path: **A (`wid` into Classic WorkerW)**, decided by M0 (2026-11-10). B/C only if Windows 11 support is taken up
- [x] D4. libmpv binding: **hand-written FFI + `libloading`** (proven in the spike, 2026-11-10)
- [ ] D5. Default behavior: same video on all monitors + per-monitor override (assumed; confirmed by owner: pending)
- [x] D6. MVP UI: **tray-only** (decided 2026-11-11, default accepted)
- [ ] D7. GIF handling: direct playback (MVP) / transcode on import (P2) *(proposed: direct)*
- [x] D8. Tray implementation: **raw `Shell_NotifyIconW`** (decided 2026-11-11)
- [x] D9. File dialog: **native**; amended 2026-11-12 to the classic `GetOpenFileNameW` (same Explorer-style window, far less unsafe code than `IFileOpenDialog`)
- [ ] D10. Final product name and icon
- [x] D12. `video-sync` default stays **`display-desync`**. `audio` (GPU 3D ~20 % -> ~8 %) shows visible judder on the owner's panel (2026-11-10), so it is rejected as a default. Maybe an opt-in "low GPU" mode later (P1).
- [x] D16. "Reload config from file" (decided 2026-11-13, owner testing): read-only on the running state unless the file is valid. Valid file -> applied and written back (restores cut-out blocks); invalid file -> current settings kept, file untouched, warning notification; missing file -> recreated from the current settings. Feedback for something the user just clicked is never rate-limited (that was why a repeated reload sometimes showed nothing); only automatic notices (heavy video) are. The maintenance items moved into an "Advanced" submenu.
- [x] D17. Sleep and screen-off detection uses `GUID_CONSOLE_DISPLAY_STATE` (own reason `DISPLAY_OFF`, 0x100) next to `WM_POWERBROADCAST` (decided 2026-11-14): `PBT_APMSUSPEND` was never delivered on the owner's laptop (log shows no `power event`; probably Modern Standby), while screen on/off notifications also pause the wallpapers on a plain idle screen-off. Pausing uses mpv `pause=yes` (GPU/CPU 0 %), not `vid=no`; RAM is not released by pausing.
- [x] D19. Audio design (decided 2026-11-15, M7 step 1): one tray submenu "Audio" instead of an audio item inside every screen submenu (a radio group is clearer and cannot select two sources); the source screen is the only one with `aid=auto` and `mute=no`, every other player has `aid=no` (no audio decode) and `mute=yes`; audio is switched with mpv properties at run time, no player is rebuilt; if the source screen disappears nothing else takes over (the config keeps the id, so sound returns with the screen) and one notice is shown; audio follows the video pause state (a covered source screen is silent too, P1 F-43 changes that later); the default is no source (first launch is silent).
- [x] D18. Fullscreen apps pause **all** screens by default (decided 2026-11-15, owner): the wallpaper must stay out of the way of GPU-heavy programs (one iGPU shared by the wallpaper decode and the game). Detection: foreground window + passes the window filters + exactly fills one screen + notification state 2 (BUSY) or 3. `scope` (covered windows) stays `this_monitor`; the new `fullscreen_app_scope` (`all_monitors` default) is separate, with a tray toggle. **Limitation found 2026-11-15:** `SHQueryUserNotificationState` only reports fullscreen on the **primary** monitor, so the same app pauses all screens when fullscreen on the primary screen (this includes a browser in F11 / YouTube fullscreen) but only its own screen (`COVERED`, 0x2) on a non-primary screen. Accepted; outliers are for per-app rules (F-35). Rejected: per-process GPU counters (needs polling), D3D swapchain detection (no reliable public API), process-name lists (maintenance; per-app rules are P2 F-35). States 3 and 4 always pause all screens. *Update 2026-11-15 (step 4d):* the no-caption requirement was dropped because some games (Touhou, style 0x94CA0000) keep `WS_CAPTION` while they fill the screen; the state 2/3 gate plus the exact fit is enough. Fullscreen on a non-primary screen stays a per-screen cover by owner decision (see Future ideas).
- [x] D15. Known issue, not a blocker: every mpv GPU video output leaks ~8-9 kernel handles per player create/destroy (`vo=gpu` 8, `vo=gpu-next` 8, `gpu-api=opengl` 9; `hwdec=no` identical; only `vo=null` is flat; our window code and `find_host`/`enumerate` do not leak). So it sits in mpv's GPU path or the AMD driver, not in LiteWall. Cost: ~300 handles per 50 rebuilds. Workaround if it ever matters: keep players alive across display changes (hide/re-parent) instead of destroy + create. Re-run `--probe-handles` with the pinned libmpv build (M10).
- [x] D14. Default `vo=gpu` (was `gpu-next`): about -12 % GPU 3D and -29 MB private memory with `display-desync`, noise < 1 % (3 runs, 2026-11-10). One-line change in `src/mpv/player.rs` `BASE_OPTIONS`; revisit in M9.
- [x] D13. Loop-seam smoothing (fade / ping-pong) is **not** a runtime feature in the MVP. Baked into the file at import time (ffmpeg), it belongs to the P2 import optimizer (F-12); a runtime fade-through-black is a small optional idea; a true in-player crossfade is large (needs the render API, Option B).
- [x] D11. `ZoomMode::Custom` is applied on top of **Fill** (`panscan=1`), so small zoom/pan values adjust a cropped fill (matches the PRD config example). PRD §7 table says `panscan=<base>`; update when confirmed.

---

## M0 — Feasibility spike (throwaway code is fine)

**Goal:** prove a looping video can be shown behind the icons on *both* desktop layouts, and choose the rendering path.

**Status 2026-11-10: GATE PASSED (Windows 10 scope).** Run on the owner's machine; details and numbers in `docs/m0-findings.md`. The four manual checks (behind icons, click-through, no taskbar/Alt-Tab, Win+D) are owner-reported, not in the logs. Still open: libmpv build identity, GPU decode-engine view, benchmark variants.

- [x] Pre-flight: dev machine = Windows 10 LTSC 21H2 (Classic only, see Environment); a Windows 11 VM is *(extended)*
- [x] Order of work: Classic on the dev machine; the Raised layout is deferred *(extended)*
- [ ] Download libmpv: both the standard and the LGPL `libmpv-2.dll` builds; record version and source URL *(one ~97 MB build in use; version/URL/SHA-256 not recorded; LGPL build untested)*
- [x] Write a ~100-line spike (separate `spike/` crate): borderless window + libmpv playing a test MP4 in a normal window (sanity check)
- [x] Implement layout detection: send `0x052C` to `Progman`; dump the window tree (Progman / WorkerW / SHELLDLL_DefView) to a log; classify as `Classic` or `Raised`
- [x] **Classic layout:** find the correct `WorkerW`, `SetParent`, size to a monitor → confirm video visible behind icons
- [ ] *(extended)* **Raised layout:** `WS_POPUP`→`WS_CHILD`, `SetParent` into `Progman`, `SetWindowPos` just below `SHELLDLL_DefView` → confirm whether mpv output is visible (Option A)
- [ ] *(extended)* If invisible on Raised: test Option C (mpv d3d11 composition mode) and note mpv version required
- [ ] *(extended)* If still invisible: prototype Option B (libmpv render API + D3D11 swapchain + DirectComposition visual) at minimum for a solid-color + one video frame
- [x] Verify mouse input passes through to desktop icons (no click stealing); set `input-cursor=no`, `WS_EX_TRANSPARENT` if needed
- [x] Verify the window is absent from taskbar and Alt-Tab (`WS_EX_TOOLWINDOW` / no taskbar button)
- [x] Verify runtime `wid` change in the pinned libmpv (useful for re-attach)
- [ ] Measure baseline: CPU, GPU decode engine, private memory for 1080p H.264 loop with `hwdec=auto-safe` *(partial: hwdec=d3d11va, CPU ~4%, GPU 3D ~28%, private 313-345 MB; GPU Video Decode graph not yet captured)*
- [x] Write findings into `docs/m0-findings.md` (layouts tested, what worked, versions, screenshots, window-tree dumps)
- [x] Record decisions D3, D4 in the log above

**Gate M0 (Windows 10 scope):** Option A (`wid` into the Classic WorkerW) shows a looping video behind icons on Windows 10, with click-through and no taskbar/Alt-Tab entry. If it fails, stop and re-plan before building anything else. Raised-layout verification (Options B/C) is *(extended)*.

---

## M1 — Project scaffolding

- [x] `cargo new litewall`; set MSVC target; add `rust-toolchain.toml` (stable) *(crate + toolchain file done; MSVC target is the owner's host default)*
- [x] Module skeleton: `core/{config,zoom,pause,coverage,app}.rs`, `platform/{desktop,monitors,window,events,fullscreen,tray,autostart}.rs`, `mpv/{ffi,player}.rs` *(all present as of 2026-11-14; `events`/`fullscreen` arrived with M6, plus `core/{power,reconcile,menu,media,geometry,monitor}.rs`, `controller.rs`, `runtime.rs`)*
- [x] `#![windows_subsystem = "windows"]` and a log sink (file under `%LOCALAPPDATA%\LiteWall\logs`, size-capped rotation) *(`src/logger.rs`, rotation unit-tested; verified on Windows 2026-11-10: log file written, one write per record so two processes do not interleave lines)*
- [x] Release profile: `lto="fat"`, `codegen-units=1`, `strip=true`, `opt-level="s"`
- [x] Application manifest via `embed-resource`: PerMonitorV2 DPI awareness, `asInvoker`, long paths, Windows 10/11 compat GUIDs *(`litewall.manifest`, `litewall.rc`, `build.rs`: builds and embeds on Windows, exe runs; the DPI-awareness effect itself is checked in M2 via real window rects)*
- [x] Single-instance guard (named mutex); second launch forwards a "show/refresh" message and exits *(`platform/instance.rs`; verified 2026-11-10: second launch forwards `0x8002`, `--quit` forwards `0x8001`, only one process remains)*
- [x] Message-only hidden window + `GetMessageW` loop skeleton; custom `WM_APP_*` message constants *(`platform/message_window.rs`, verified on Windows 2026-11-10. Deviation: a hidden **top-level** window instead of message-only, because message-only windows do not receive broadcasts such as `WM_DISPLAYCHANGE` / `TaskbarCreated` needed in M4/M6/M8)*
- [ ] CI (GitHub Actions): `fmt --check`, `clippy -D warnings`, `test` on Windows; run `core/` tests on Linux too *(`.github/workflows/ci.yml` written; `fmt`/`clippy`/`test` pass locally on Windows; tick after the first green run on GitHub)*
- [x] `docs/` folder: `benchmarks.md`, `m0-findings.md` placeholders
- [ ] README stub + LICENSE (after D1) + THIRD_PARTY_LICENSES placeholder

**Gate M1:** `cargo build --release` produces an exe that starts, holds the single-instance lock, idles at ~0% CPU, and exits cleanly.

**Status 2026-11-10: GATE PASSED.** Evidence in the execution log (M1 verified). Idle working set ~5.3 MB is inside the PRD §4 idle budget (< 15 MB). Items still open on the checklist above (CI green run, README/LICENSE after D1, rest of the skeleton) do not block M2.

---

## M2 — Desktop host (attach / detach)

*Implements F-04. All Win32 `unsafe` lives in `platform/`.*

- [x] `platform/desktop.rs`: `detect_layout() -> Layout { Classic, Raised }` (build number is a hint only; confirm from the window tree)
- [x] `find_host(layout) -> HWND` with logged diagnostics for every step; clear error enum when parts are missing
- [x] `platform/window.rs`: register window class; create borderless per-monitor window with correct styles (`WS_CHILD`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`; never `WS_POPUP|WS_CHILD` together)
- [x] `attach(hwnd, layout)` for Classic (WorkerW parent) and Raised *(extended)* (Progman parent, z-order below `SHELLDLL_DefView`)
- [x] `detach(hwnd)` and window destruction; verify no stray artifacts on desktop
- [x] Coordinate conversion helper: virtual-desktop rect → parent-relative via `MapWindowPoints`; unit-test the pure math *(pure math + tests in `core/geometry.rs`; the `MapWindowPoints` call that supplies the parent origin is pending in `platform/`)*
- [x] Paint a solid color / test pattern into the window to verify placement before video exists
- [x] Click-through: icons remain clickable; right-click desktop menu still works
- [x] Manual check on each layout in the matrix

**Gate M2:** a colored window sits behind icons on one monitor on the Classic layout (Raised is *(extended)*); icons and desktop context menu still work; quitting leaves the desktop unchanged.

---

## M3 — Single-monitor playback + zoom

*Implements F-01, F-02, F-03 (single monitor), F-08.*

- [x] `mpv/ffi.rs`: load `libmpv-2.dll` next to the exe via `libloading`; resolve the needed symbols; clear error + tray notice if missing (or wrap `libmpv2` per D4)
- [x] `mpv/player.rs`: `Player::new(wid)`; set the option profile from `CLAUDE.md §7` before `mpv_initialize`
- [x] `load(path)`, `loop-file=inf`; handle load errors (event wait via wakeup callback → `PostMessage`)
- [ ] Never start mpv's own window: confirm `wid` is set before init and `force-window` behaves
- [x] `core/zoom.rs`: `ZoomMode → Vec<(property, value)>` pure function; **always reset all properties** per mode
- [x] Unit tests for all five modes and for switching between them
- [x] Apply zoom modes live; verify Fill crops, Fit letterboxes, Stretch distorts, Original is 1:1, Custom pans/zooms (log2 scale)
- [ ] GIF/WebP: verify loop and measure cost; show a size warning for large files (F-02)
- [x] Error handling: missing file, unsupported codec, corrupt file → window stays empty (static wallpaper visible) + tray notification placeholder *(verified 2026-11-10: missing file and corrupt file; pre-check now runs before window creation, re-verify; tray notice arrives in M5)*
- [x] Verify hardware decode is active (log mpv's `hwdec-current`)
- [x] Benchmark table into `docs/benchmarks.md`: `vo=gpu` vs `gpu-next`, `hwdec` variants, `video-sync` variants (pick defaults from data) *(done 2026-11-10: three runs per variant, noise < 1 %, D12 and D14 follow from it; full table in `docs/benchmarks.md`)*

**Gate M3:** a chosen video loops on one monitor behind icons with all zoom modes working, HW decode confirmed, no stray mpv window.

---

## M4 — Multi-monitor

*Implements F-05, F-20 – F-23.*

- [x] `platform/monitors.rs`: enumerate monitors *(verified 2026-11-10 on the owner's 3 monitors: negative origins, DPI 96/96/144, refresh 60/60/120 Hz, unique interface-path ids)* → `MonitorInfo` (rect in physical px, DPI, refresh rate, primary)
- [ ] Stable `MonitorId` from the display device interface path (DisplayConfig API); fallback `\\.\DISPLAYn`; tests for id stability logic *(`choose_id` + `dedupe_ids` + tests done; the path comes from `EnumDisplayDevicesW(EDD_GET_DEVICE_INTERFACE_NAME)` instead of DisplayConfig (same path, less FFI); verify stability across reboot/replug)*
- [x] `diff(old, new) -> {added, removed, changed}` pure function + unit tests (including negative origins)
- [x] One wallpaper window + one `Player` per enabled monitor *(verified 2026-11-11 on 3 monitors, 2 videos: `runtime.rs` + pure planner `core/reconcile.rs` (9 tests); owner-reported RAM under 450 MB total, to be measured in M9)* (`HashMap<MonitorId, MonitorRuntime>`)
- [x] Per-monitor zoom and per-monitor file override *(verified 2026-11-11 via `--config`; portrait monitor with a 2160x3840 clip looked cropped/odd: under investigation, see log)*; "same on all" default
- [x] Handle `WM_DISPLAYCHANGE` / `WM_DPICHANGED` *(verified 2026-11-11: Win+P cycles and a scaling change worked; one later run ended with the taskbar gone, cause unknown, see log)*: apply diff, recreate/resize windows, keep playback state
- [x] Disabled monitor → no window *(planner-tested; verified 2026-11-11, then enabled via config reload)*, no mpv instance, static wallpaper visible
- [x] Verify mixed DPI, mixed refresh rates (each monitor presents at its own rate), and a monitor left of/above primary (negative coordinates)
- [x] Hot-plug test: unplug/replug monitor 20×, no leaks (watch handle and memory counts) *(2026-11-11: 20 cycles, every switch handled in ~3.3 s, memory flat; handles +~6 per cycle = the known GPU-VO leak, D15)*

**Gate M4:** three-monitor setup shows correct, independently zoomed video per monitor; hot-plug and resolution changes recover without restart.

---

## M5 — Tray, config, persistence

*Implements F-06, F-07, F-50.*

- [x] `core/config.rs`: structs from `CLAUDE.md §6`, serde+TOML, `version` field, defaults
- [x] Atomic save (temp file + rename); corrupt config → back up as `.bad`, load defaults, notify *(notify = `LoadOutcome::Recovered`; tray notification wiring pending)*
- [x] Migration scaffold (`v1 → v2` stub + test) so future schema changes are painless
- [x] Pure `App::update(event) -> Vec<Effect>` with `UserCommand`s; effects executor in the platform layer *(verified 2026-11-12: `core/app.rs`, 8 tests)*
- [x] Tray icon + menu (per D8): Pick file… / per-monitor submenu (enable, zoom mode, audio source placeholder) / Pause-Resume / Start with Windows / Open config folder / Quit *(verified 2026-11-12 on the owner's machine, except the file dialog (step 2))*
- [x] File picker off the message thread (per D9) → result returned via `PostMessage` *(written 2026-11-12 (step 2, `GetOpenFileNameW` on a worker thread); needs the owner's run)* *(verified 2026-11-12: picks for all screens and per screen, videos, images)*
- [x] Autostart via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (toggle); quoted path *(written 2026-11-12 (step 2); also synced with the config at every start; needs the owner's run)* *(verified 2026-11-12: Run entry written, wallpapers back 2-3 s after sign-in)*
- [x] Restore wallpapers on launch from config; missing file → skip with notification *(verified 2026-11-12: per-screen files and zoom come back after quit/start and an Explorer restart)*
- [x] Custom zoom editing: initial approach = edit `config.toml` + "Reload config" menu item (UI for it is P1) *(verified; reload is now non-destructive (a bad config.toml keeps the current settings and shows a notification))*
- [x] Tray notifications (balloon/toast) for errors, rate-limited *(verified 2026-11-14: config-reload errors, large GIF/WebP, "very heavy video", and a missing or unplayable wallpaper file (once per problem, not once per screen); feedback for a user click is never throttled, D16)*

**Gate M5 (MVP functional):** launch → tray → pick a file → plays on chosen monitors with chosen modes → quit → relaunch restores everything.

---

## M6 — Pause engine

*Implements F-30, F-31, F-33. (F-32, F-34 in P1.)*

- [x] `core/pause.rs`: `PauseReasons` bitflags + reducer + `PlayState`; unit tests for overlapping reasons and sticky `USER` *(also `PauseTracker` with scope handling and `Hysteresis`; std-only, no `bitflags` crate)*
- [x] `core/coverage.rs`: N×M grid coverage math (rects in, covered fraction out); tests incl. partial overlap, multiple windows, negative origins
- [x] `platform/events.rs`: `SetWinEventHook` for foreground change, minimize start/end, location change; debounce 150–250 ms; marshal into `AppEvent` on the main thread *(verified 2026-11-14: five hook ranges, 200 ms trailing debounce on one one-shot timer, zero idle activity)*
- [x] `platform/fullscreen.rs`: window enumeration for coverage (skip own windows, shell/taskbar, tool windows, cloaked windows) + `SHQueryUserNotificationState` *(window enumeration verified 2026-11-14: skips own process, tool/transparent/cloaked/zero-alpha/minimised windows and shell classes; `SHQueryUserNotificationState` verified 2026-11-14 for presentation mode (state 4) and for logging BUSY (2) with games; state 3 (exclusive) never seen on Windows 10; `foreground_candidate()` added 2026-11-15 in step 4b, not yet run, so this stays open)* *(closed 2026-11-15: `SHQueryUserNotificationState` states 3 and 4 verified, `foreground_candidate()` and `foreground_report()` verified in step 4b-4d)*
- [x] Hysteresis: pause after ~250 ms covered, resume after ~100 ms uncovered *(verified 2026-11-14: pure `Hysteresis` driven by one one-shot timer, armed only while a change is pending)*
- [x] Per-monitor `COVERED` handling; apply with mpv `pause` *(verified 2026-11-14: only the covered screen pauses; `PauseScope::AllMonitors` is wired but not exercised yet)*
- [x] Global reasons: session lock/unlock + remote (`WTSRegisterSessionNotification`), sleep/resume (`WM_POWERBROADCAST`) *(lock/unlock verified 2026-11-14: `session event 7/8`, all screens `PAUSED (reasons 0x10)`, a covered screen stays paused after unlock, manual pause survives lock. Remote desktop and fast user switching: skipped, untested. Sleep: `PBT_APMSUSPEND` was never delivered (no `power event` in the log), probably Modern Standby; step 3b adds the screen on/off notification `GUID_CONSOLE_DISPLAY_STATE` (reason `DISPLAY_OFF` 0x100): screen-off verified 2026-11-14 (`display power state 0/1`, `PAUSED (reasons 0x100)`, resume on screen-on). A real suspend/resume is still not visible in the log (no gap, no `display power state` around it), see the 2026-11-14 step 3b entry)* *(closed 2026-11-15, remaining parts carried to M8 where they are already listed: real sleep/wake, remote desktop, fast user switching)*
- [x] Manual pause (`USER`) from tray; never auto-cleared *(verified 2026-11-14: stays paused through cover/uncover and lock/unlock; resuming releases only screens that are not covered)*
- [x] 1 s fallback timer active **only** while a fullscreen-sized window exists; no timer otherwise *(step 4: one-shot 1 s timer armed only while a window exactly fills a screen, a fullscreen app is detected or FULLSCREEN is set; step 4b logs `fullscreen timer armed` / `stopped` at debug level. Verified 2026-11-15: armed and stopped in matching pairs, never armed with nothing fullscreen or for a framed maximised window)* *(verified 2026-11-15: armed only while a fullscreen window exists, stops after)*
- [x] Step 4b: a true fullscreen app (foreground, no caption, exact fit, state 2 or 3) pauses all screens; `fullscreen_app_scope` setting + tray toggle; hysteresis *(verified 2026-11-15: Denshattack, an emulator, a visual novel, Astral Chain, an SDL app, borderless Trails in the Sky and Chrome F11 on the primary screen log `fullscreen app: ... pausing all screens` and resume ~0.1 s after leaving; tray toggle off = own screen only; presentation mode unchanged. Not detected: Touhou games (exact fit + state 2 logged, no `fullscreen app` line; probably the no-caption check) and a few niche titles. Non-primary screens never report state 2, so there only `COVERED`. 93 tests pass)* *(step 4c added the rejection report, step 4d removed the caption check after Touhou showed `style=0x94CA0000`; verified 2026-11-15: Touhou borderless now pauses all screens at state 2 and resumes both screens within the same second)*
- [x] Verify with: exclusive-fullscreen game, borderless-fullscreen game, maximized browser, maximized-but-unfocused window, video player fullscreen, Alt-Tab animation (no flapping) *(done 2026-11-14: maximized browser, maximized-but-unfocused, Alt-Tab (no stutter; the log flaps about every 0.5 s while switching, which is real), Win+D, snapped half window (not covered), window peek shows the paused frame. Step 4 results 2026-11-14: presentation mode pauses all screens and resumes; a fullscreen game, a borderless game, F11 and YouTube fullscreen each paused only their own screen (0x2) and the other screens kept playing, which step 4b changes for games. Under step 4b (2026-11-15): exclusive fullscreen **seen**: an Unreal game gave state 3 and the global 0x4 pause; borderless games and F11 on the primary screen pause all screens; F11 on a non-primary screen pauses only that screen. Still open: fullscreen video player, games that are not detected (Touhou))* *(closed 2026-11-15: exclusive game (state 3), borderless games, F11 on the primary screen, maximised (un)focused windows, Alt-Tab, presentation mode all verified; not detected: Oneechanbara Origin (accepted, per-app rules F-35); F11 on a non-primary screen: see Future ideas)*
- [x] Verify ~0% CPU/GPU while paused (Task Manager GPU engines) *(2026-11-14, two enabled screens with a 4K30 clip: every screen covered = litewall.exe 0 % GPU; one covered about 13 %; both visible about 19 % (30 % total with Desktop Window Manager). Paused RAM stays ~300 MB: pause frees GPU/CPU only, memory is an M9 item)*
- [x] Log every reason transition to make field bugs diagnosable *(info level: `COVERED n % by [...]` / `uncovered n %`, `monitor ...: PAUSED|RESUMED (reasons 0x..)`, `session event`, `power event`, `display power state`)*

**Gate M6:** covering one monitor pauses only that monitor; locking/sleeping pauses all; resume is immediate and glitch-free; idle-while-paused cost is ~0.

**Status 2026-11-15: GATE PASSED.** Open items carried to M8: real sleep/wake (the log has no event around a real suspend, see D17), remote desktop, fast user switching. Everything else on this list is verified.

---

## M7 — Audio

*Implements F-40 – F-42.*

- [x] Default: all monitors silent; non-source monitors use `aid=no` (no audio decode at all) *(step 1: `core/audio.rs::state_for`, players start with `aid=no`, `mute=yes`; not yet run)* *(verified 2026-11-15)*
- [x] Single `audio.source: Option<MonitorId>`; setting a new source silences the previous one *(step 1: `SetAudioSource`, `Runtime::apply_audio` switches `aid`/`mute`/`volume` at run time; not yet run)* *(verified 2026-11-15: log shows exactly one `audio ON`, the previous one `audio off`)*
- [x] Tray: "Play audio from this screen" per monitor (radio behavior) + "No audio" *(step 1: one "Audio" submenu with "No audio", one radio item per screen and the volume presets, D19; not yet run)* *(verified 2026-11-15)*
- [x] Volume presets 0/25/50/75/100 → mpv `volume`; persisted *(step 1: in the Audio submenu; a non-preset volume such as the default 40 shows as a checked "custom" placeholder; not yet run)* *(verified 2026-11-15: presets and persistence)*
- [x] Audio follows pause state (paused video = paused audio in MVP) *(verified 2026-11-15: covering the source screen stops its audio, uncovering resumes it)*
- [x] Removing the source monitor → silent (never auto-moves); notify *(step 1: `source_status` Missing -> one "Audio" notice, the source stays in the config so sound returns when the screen returns; not yet run)* *(verified 2026-11-15: the 4th display disappeared, one notice, silence)*
- [x] Verify the same video on 3 monitors yields exactly one audio stream *(owner 2026-11-15: one source audible at a time; the log shows one `audio ON`)*
- [ ] First launch never plays sound; check muted state before the first frame renders
- [ ] Verify loop-point gap on a few real files; document the behavior

- [ ] **B1 (open bug, owner 2026-11-15):** with an audio source set, the video on the primary screen (2560x1600 @ 120 Hz) plays at the right speed but the videos on the other screens play about 1.25x-1.5x too fast. Not explained by the log (the only warnings are mpv's `Audio/Video desynchronisation detected` on the audio screen). To find out: does it also happen with "No audio" (all silent)? `Log playback stats` in the bad state (compare `fps` with `container`); a run with `--opt video-sync=desync`. Suspects: `video-sync=display-desync` on screens whose refresh rate mpv mis-detects under the WorkerW, or a decoder that cannot keep up (`framedrop=decoder`).
- [ ] **Step 2 (F-44 pulled forward, owner request 2026-11-15): auto-mute / duck when another app plays audio.** While an audio source is playing, detect other audio sessions on the default output (Spotify, browser, DAW, a game) and mute or lower LiteWall. Proposed config: `audio.other_audio = "mute" | "duck" | "off"` and `audio.duck_percent` (volume while another app plays, e.g. 20), plus a tray toggle. Detection: Core Audio session enumeration (`IAudioSessionManager2`), ignoring our own process and system sounds; a 2 s one-shot re-check only while a source is audible (same pattern as the fullscreen timer, no polling when silent or paused); pure reducer in `core/audio.rs` with tests; it also covers a game that is playing sound in a window that does not cover the screen.
- [ ] *(P1/extended, owner idea)* **Ambient audio passthrough**: audio keeps playing while the video is paused or covered (`vid=no`, this is F-43, moderate: the reducer already has the `AudioOnly` branch), and the audio can come from a screen that is covered, disabled or even unplugged. The first part is planned (P1); the second part needs a window-less player instance (`vo=null`) per audio source and is **extended scope** (open-ended). It interacts with step 2: ambience pauses/ducks when another app plays audio.
- [ ] Verify loop-point gap on a few real files (kept from above)

**Gate M7:** one chosen monitor plays audio at the chosen volume; others are silent and don't decode audio; settings persist.

---

## M8 — Robustness / self-healing

- [x] Explorer restart: handle the `TaskbarCreated` registered message *(first version, owner-verified 2026-11-11 with restartExplorer.bat: 1.5 s debounce, rebuild all windows; no retry yet if the host is not ready; the soak in the M8 gate still applies)*; also investigate: taskbar vanished once after repeated Win+P / scaling tests → re-detect layout, re-attach all windows (use runtime `wid` change if supported)
- [ ] Watchdog: detect when wallpaper windows were hidden/de-parented/z-order-lost (on `WM_DISPLAYCHANGE`, foreground events, and a low-rate check only while playing); re-attach with exponential backoff
- [ ] Sleep/wake and GPU reset (`DXGI_ERROR_DEVICE_REMOVED` → mpv recovery): recreate player if mpv reports fatal error
- [ ] Remote desktop connect/disconnect, fast user switching, UAC secure desktop
- [ ] mpv crash/error isolation: failure on one monitor doesn't affect others; auto-retry once, then notify
- [ ] Config hot-reload (watch or menu) without restart
- [ ] Panic hook: log, best-effort cleanup (`panic="abort"` decision documented; the OS removes our windows regardless)
- [ ] Orphan check on launch: stale wallpaper window from a previous crashed run does not block start
- [ ] Soak test: 24 h run, 20 Explorer restarts, 20 sleep/wake, 50 hot-plug cycles; no crashes, handle/memory growth flat

**Gate M8:** survives the soak list unattended with a clear log.

---

## M9 — Performance pass

- [ ] Benchmark suite documented and scripted (PowerShell + `typeperf`/Task Manager screenshots): 1080p30 H.264, 1080p60 H.264, 4K30 HEVC, 1080p AV1, large GIF
- [ ] Compare `hwdec=auto-safe` vs `d3d11va-copy` vs `no`, `vo=gpu` vs `gpu-next`, `video-sync` variants; update defaults from data
- [ ] Minimize memory: cache sizes, shader cache off/on, scalers; record per-monitor memory
- [ ] Confirm zero timer activity when idle (ETW / Process Explorer thread view)
- [ ] Binary size review (`cargo bloat`); drop unneeded features/crates
- [ ] Startup time profile (< 1.5 s to first frame)
- [ ] Startup polish (owner feedback 2026-11-12): show each wallpaper window only after its first frame (no black flash), stagger the players instead of starting all at once, optional short fade-in from black (`brightness` ramp from one-shot timers; a true crossfade from the static wallpaper needs the render API); investigate the taskbar flicker on launch with an A/B test (`--test-pattern` vs one enabled monitor vs three)
- [ ] Optional: free the player of a monitor that has stayed covered for N minutes (~0.3-0.8 GB each) and recreate it when it is uncovered (note the D15 handle cost per recreate)
- [ ] Verify against budgets in `CLAUDE.md §4`; update the PRD with real numbers

**Gate M9:** budgets met or deviations documented with reasons.

---

## M10 — Packaging & first release

- [ ] Re-run `--probe-handles 20 --play <clip>` with the pinned libmpv build and compare with D15 (8 handles/round on the dev build)
- [ ] Pin the libmpv build (version + hash), choose standard vs LGPL per D1; document how to swap the DLL
- [ ] `THIRD_PARTY_LICENSES` (mpv/LGPL text, FFmpeg, libplacebo, Rust crate licenses via `cargo-about`/`cargo-deny`)
- [ ] Portable zip: exe + `libmpv-2.dll` + licenses + README
- [ ] Code signing (certificate decision) and SmartScreen/antivirus check (VirusTotal scan, document false-positive handling)
- [ ] Icon, app metadata (version resource)
- [ ] Release automation: tag → GitHub Actions → zip artifact + checksums
- [ ] README: install, usage, supported OS, known limitations (24H2 notes, antivirus note), how to report bugs with a log
- [ ] Clean-machine install test (fresh VM, no Rust, no VC++ redist issues; static CRT if needed: `-C target-feature=+crt-static`)
- [ ] Uninstall/cleanup notes: remove autostart entry, config folder

**Gate M10:** a stranger can download the zip on a clean Windows 10 PC and have a wallpaper running in under a minute.

---

## P1 — Fast-follow (after MVP release)

- [ ] F-32 Pause on battery (toggle) via power-setting notification
- [ ] F-34 Pause scope: this monitor vs all monitors (config + tray) *(pure scope logic already in `PauseTracker`; `pause.scope` for covered windows is config-only; the separate `fullscreen_app_scope` has a tray toggle since step 4b; a tray item for `scope` is pending)*
- [ ] F-43 Audio continues while video pauses (owner 2026-11-15: wants this as an "ambience" mode; a variant that plays from any screen even when it is covered/disabled is extended scope, see M7) (`vid=no` when covered; restore on resume); verify zero video cost *(pure `AudioOnly` reducer branch done)*
- [ ] Opt-in "low GPU" mode: `video-sync=audio` (~60 % less GPU 3D in the 1080p60 benchmark, but judder on mismatched rates); label the trade-off in the UI
- [ ] F-09 Playback speed (0.25x–2x)
- [ ] F-11 Span mode (one canvas across all monitors; per-monitor crop; mixed-size/bezel design note first) *(`union_rect` helper exists)*
- [ ] F-51 Minimal settings window: monitor layout preview, per-monitor file/zoom, custom zoom/pan sliders with live preview *(owner 2026-11-13: hand-editing `config.toml` is not acceptable for users, wants a GUI panel opened from the tray; D6 keeps the MVP tray-only, so this is the first P1 UI item, after M6/M7 add audio and pause options that need a place to live; "Reload config from file" then stays only as an advanced item)*
- [ ] Interactive zoom/pan editor (drag to pan, scroll to zoom) writing back to the profile
- [ ] Per-monitor "reset to default" and "copy settings to…" actions
- [ ] Warn (log + notification) about unknown keys in `config.toml` (a typo such as `enabeld` is silently ignored today)
- [ ] **Lighter copy of a heavy video (owner request 2026-11-13, promoted from F-12; first item after M6/M7)**: tray / picker action "Make a lighter copy" that re-encodes the chosen video to the size of the screen(s) showing it (e.g. 3840x2160 -> 2560x1440 or 1080p, 10-30 Mbps, audio stripped, loop-friendly) into `%LOCALAPPDATA%\LiteWall\optimized\`, plays that copy, and keeps the original path so it can be switched back; offered from the "very heavy video" notification. Evidence: a 4K30 file on all three screens stutters on the primary, but is smooth with one other screen disabled. Step 0 is a feasibility spike: the tray item "Log playback stats" now also logs the `encoder-list` of the pinned libmpv (libx264 / h264_amf / h264_mf yes or no). If libmpv can encode (`--o=`) no ffmpeg needs bundling; otherwise bundle ffmpeg (licence, ~80 MB) or use hardware encoders *(Step 0 result 2026-11-14: libx264 no, libx265 no; h264_amf, hevc_amf, h264_mf, h264_nvenc, h264_qsv yes on the owner's AMD machine, so libmpv's own encode mode may be enough without bundling ffmpeg; `h264_mf` is the vendor-neutral fallback, nvenc/qsv only work with that hardware. libmpv `--o=` encoding itself is still untried.)*
- [ ] Long-term: share one decode between screens that show the same file (needs the libmpv render API / Option B)
- [ ] Auto-update check (opt-in; no telemetry)

## P2 — Later

- [ ] F-10 Playlist / folder / shuffle with interval
- [ ] F-12b Loop-seam smoothing at import time (ffmpeg): ping-pong (`reverse` + concat) or crossfade the last N frames into the first; zero runtime cost. Offered for clips whose last frame does not match the first (see D13)
- [ ] F-12 Import optimizer (bundled FFmpeg): target-monitor downscale, fps cap, codec pick by GPU support, audio strip, loop-friendly keyframes, before/after size report, GIF→MP4 transcode
- [ ] F-35 Per-app pause rules *(motivated by 2026-11-15 testing: a few games (e.g. Oneechanbara Origin) never reach state 2/3 or an exact fit in any mode; an "always pause while this exe is foreground" rule is the planned answer)*
- [ ] F-44 Auto-duck/mute on other-app audio (Windows audio session APIs) *(pulled forward into M7 step 2 on 2026-11-15)*
- [ ] F-45 Volume slider, per-monitor volume, output device selection
- [ ] F-52 CLI control (`--set`, `--pause`, `--quit`) via named pipe / second-instance message
- [ ] ARM64 build
- [ ] Detach/attach a screen from the tray via `SetDisplayConfig` (Windows display topology, not just LiteWall): experimental, with a confirmation, restores the previous layout on failure. Today unticking "Enabled" already removes that screen's window, player and decode cost
- [ ] "Reset to defaults" tray item (with a confirmation) and an option to set the current setup as the default profile
- [ ] Localization scaffold

## Future ideas (unscheduled)

- [ ] *(extended, open-ended)* Fullscreen on a **non-primary** screen pauses all screens like on the primary one. `SHQueryUserNotificationState` only describes the primary monitor, so on other screens a browser in F11 or a game only pauses its own screen. A possible rule for those screens only: foreground + exact fit + no caption, without the state check (accepts some false positives, e.g. a borderless overlay; could be an opt-in). Owner decision 2026-11-15: skipped for now, probably rarely needed; pick up if users ask for it (collect votes/feedback after release). The `foreground_report` log line is the tool to evaluate real cases.
- [ ] F-13 URL import via yt-dlp (user-initiated download, local file result)
- [ ] F-14 Static wallpapers (JXL/PNG/JPG), instant fallback frame while loading, thumbnails for the picker *(images themselves pulled forward into M5 step 2 on 2026-11-12: dialog filter + `image-display-duration=inf`; thumbnails and the fallback frame stay later)*
- [ ] F-36 Pause on high CPU/GPU load
- [ ] Time-of-day / scheduled wallpapers
- [ ] Lock-screen integration (revisit once Microsoft's native video wallpaper behavior is known)
- [ ] Runtime fade-through-black at the loop point (`brightness` ramp from one-shot timers, no idle timer); true crossfade only via the render API (Option B), large
- [ ] Re-evaluate vs Windows native video wallpaper when it reaches general availability

---

## Manual regression checklist (run before each release)

**Layouts:** [ ] Win10 (LTSC 21H2) [ ] *(extended)* Win11 ≤23H2 [ ] *(extended)* Win11 24H2/25H2

**Display:** [ ] 1 monitor [ ] 3 monitors mixed size [ ] mixed DPI [ ] mixed refresh [ ] monitor left/above primary [ ] hot-plug [ ] resolution change [ ] rotate a monitor

**Zoom:** [ ] Fill [ ] Fit [ ] Stretch [ ] Original [ ] Custom (zoom + pan) [ ] mode switching leaves no stale state

**Pause:** [ ] exclusive fullscreen game (all screens) [ ] borderless game (all screens) [ ] F11 browser / YouTube fullscreen (all screens on the primary screen, own screen on others) [ ] presentation mode (all screens) [ ] tray toggle "Pause all screens for fullscreen apps" off = own screen only [ ] maximized window (focused) [ ] maximized (unfocused) [ ] lock/unlock [ ] sleep/wake [ ] RDP [ ] manual pause persists through auto events

**Audio:** [ ] default silent [ ] one source only [ ] volume presets [ ] source monitor removed → silent

**Shell:** [ ] Explorer kill/restart [ ] desktop icons clickable [ ] desktop right-click menu [ ] Win+D / Show desktop [ ] not in taskbar/Alt-Tab [ ] quit leaves static wallpaper intact

**Lifecycle:** [ ] autostart on/off [ ] second instance behavior [ ] config corrupt → recovered [ ] missing video file → graceful [ ] missing `libmpv-2.dll` → clear message

---

## Definition of done (every task)

- [ ] Code compiles with `clippy -D warnings`, formatted with `rustfmt`
- [ ] New pure logic has unit tests; new Win32 behavior is on the manual checklist
- [ ] No new idle timers/polling; no blocking on the message thread
- [ ] `unsafe` confined to `platform/` or `mpv/ffi.rs`, with a safety comment
- [ ] `CLAUDE.md` and this plan updated if scope, decisions, or numbers changed
