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
| M1 | Project scaffolding | [ ] (code written + type-checked + 42 pure tests; **awaiting first Windows run** of manifest, mutex, message loop; CI unverified) |
| M2 | Desktop host (attach/detach) | [ ] (pure coordinate math done) |
| M3 | Single-monitor playback + zoom | [ ] (zoom.rs + tests done) |
| M4 | Multi-monitor | [ ] (`MonitorId`/`MonitorInfo`/diff done) |
| M5 | Tray + config + persistence | [ ] (`core/config.rs`: structs, migration scaffold, atomic save, `.bad` recovery done) |
| M6 | Pause engine | [ ] (pause.rs, coverage.rs, hysteresis logic done) |
| M7 | Audio | [ ] |
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
- [ ] D6. MVP UI: tray-only / tray + minimal settings window *(proposed: tray-only)*
- [ ] D7. GIF handling: direct playback (MVP) / transcode on import (P2) *(proposed: direct)*
- [ ] D8. Tray implementation: `tray-icon` / raw `Shell_NotifyIcon` *(proposed: raw, reuse Glide patterns)*
- [ ] D9. File dialog: `rfd` / native `IFileOpenDialog` *(proposed: native, smaller binary)*
- [ ] D10. Final product name and icon
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
- [x] **Classic layout:** find the correct `WorkerW`, `SetParent`, set `WS_CHILD`, size to a monitor → confirm video visible behind icons
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
- [ ] Module skeleton: `core/{config,zoom,pause,coverage,app}.rs`, `platform/{desktop,monitors,window,events,fullscreen,tray,autostart}.rs`, `mpv/{ffi,player}.rs` *(core/{zoom,pause,coverage,geometry,monitor,config} done; core/app, platform/, mpv/ pending)*
- [x] `#![windows_subsystem = "windows"]` and a log sink (file under `%LOCALAPPDATA%\LiteWall\logs`, size-capped rotation) *(`src/logger.rs`, rotation unit-tested; first Windows run pending)*
- [x] Release profile: `lto="fat"`, `codegen-units=1`, `strip=true`, `opt-level="s"`
- [ ] Application manifest via `embed-resource`: PerMonitorV2 DPI awareness, `asInvoker`, long paths, Windows 10/11 compat GUIDs *(written: `litewall.manifest`, `litewall.rc`, `build.rs`; not yet built on Windows)*
- [ ] Single-instance guard (named mutex); second launch forwards a "show/refresh" message and exits *(written in `platform/instance.rs`, type-checked, untested; `--quit` also forwards to the running instance)*
- [ ] Message-only hidden window + `GetMessageW` loop skeleton; custom `WM_APP_*` message constants *(written in `platform/message_window.rs`, untested. Deviation: a hidden **top-level** window instead of message-only, because message-only windows do not receive broadcasts such as `WM_DISPLAYCHANGE` / `TaskbarCreated` needed in M4/M6/M8)*
- [ ] CI (GitHub Actions): `fmt --check`, `clippy -D warnings`, `test` on Windows; run `core/` tests on Linux too *(`.github/workflows/ci.yml` written; tick after the first green run)*
- [x] `docs/` folder: `benchmarks.md`, `m0-findings.md` placeholders
- [ ] README stub + LICENSE (after D1) + THIRD_PARTY_LICENSES placeholder

**Gate M1:** `cargo build --release` produces an exe that starts, holds the single-instance lock, idles at ~0% CPU, and exits cleanly.

---

## M2 — Desktop host (attach / detach)

*Implements F-04. All Win32 `unsafe` lives in `platform/`.*

- [ ] `platform/desktop.rs`: `detect_layout() -> Layout { Classic, Raised }` (build number is a hint only; confirm from the window tree)
- [ ] `find_host(layout) -> HWND` with logged diagnostics for every step; clear error enum when parts are missing
- [ ] `platform/window.rs`: register window class; create borderless per-monitor window with correct styles (`WS_CHILD`, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`; never `WS_POPUP|WS_CHILD` together)
- [ ] `attach(hwnd, layout)` for Classic (WorkerW parent) and Raised *(extended)* (Progman parent, z-order below `SHELLDLL_DefView`)
- [ ] `detach(hwnd)` and window destruction; verify no stray artifacts on desktop
- [x] Coordinate conversion helper: virtual-desktop rect → parent-relative via `MapWindowPoints`; unit-test the pure math *(pure math + tests in `core/geometry.rs`; the `MapWindowPoints` call that supplies the parent origin is pending in `platform/`)*
- [ ] Paint a solid color / test pattern into the window to verify placement before video exists
- [ ] Click-through: icons remain clickable; right-click desktop menu still works
- [ ] Manual check on each layout in the matrix

**Gate M2:** a colored window sits behind icons on one monitor on the Classic layout (Raised is *(extended)*); icons and desktop context menu still work; quitting leaves the desktop unchanged.

---

## M3 — Single-monitor playback + zoom

*Implements F-01, F-02, F-03 (single monitor), F-08.*

- [ ] `mpv/ffi.rs`: load `libmpv-2.dll` next to the exe via `libloading`; resolve the needed symbols; clear error + tray notice if missing (or wrap `libmpv2` per D4)
- [ ] `mpv/player.rs`: `Player::new(wid)`; set the option profile from `CLAUDE.md §7` before `mpv_initialize`
- [ ] `load(path)`, `loop-file=inf`; handle load errors (event wait via wakeup callback → `PostMessage`)
- [ ] Never start mpv's own window: confirm `wid` is set before init and `force-window` behaves
- [x] `core/zoom.rs`: `ZoomMode → Vec<(property, value)>` pure function; **always reset all properties** per mode
- [x] Unit tests for all five modes and for switching between them
- [ ] Apply zoom modes live; verify Fill crops, Fit letterboxes, Stretch distorts, Original is 1:1, Custom pans/zooms (log2 scale)
- [ ] GIF/WebP: verify loop and measure cost; show a size warning for large files (F-02)
- [ ] Error handling: missing file, unsupported codec, corrupt file → window stays empty (static wallpaper visible) + tray notification placeholder
- [ ] Verify hardware decode is active (log mpv's `hwdec-current`)
- [ ] Benchmark table into `docs/benchmarks.md`: `vo=gpu` vs `gpu-next`, `hwdec` variants, `video-sync` variants (pick defaults from data)

**Gate M3:** a chosen video loops on one monitor behind icons with all zoom modes working, HW decode confirmed, no stray mpv window.

---

## M4 — Multi-monitor

*Implements F-05, F-20 – F-23.*

- [ ] `platform/monitors.rs`: enumerate monitors → `MonitorInfo` (rect in physical px, DPI, refresh rate, primary)
- [ ] Stable `MonitorId` from the display device interface path (DisplayConfig API); fallback `\\.\DISPLAYn`; tests for id stability logic *(`MonitorId` type + serde done in `core/monitor.rs`; DisplayConfig lookup pending)*
- [x] `diff(old, new) -> {added, removed, changed}` pure function + unit tests (including negative origins)
- [ ] One wallpaper window + one `Player` per enabled monitor (`HashMap<MonitorId, MonitorRuntime>`)
- [ ] Per-monitor zoom and per-monitor file override; "same on all" default
- [ ] Handle `WM_DISPLAYCHANGE` / `WM_DPICHANGED`: apply diff, recreate/resize windows, keep playback state
- [ ] Disabled monitor → no window, no mpv instance, static wallpaper visible
- [ ] Verify mixed DPI, mixed refresh rates (each monitor presents at its own rate), and a monitor left of/above primary (negative coordinates)
- [ ] Hot-plug test: unplug/replug monitor 20×, no leaks (watch handle and memory counts)

**Gate M4:** three-monitor setup shows correct, independently zoomed video per monitor; hot-plug and resolution changes recover without restart.

---

## M5 — Tray, config, persistence

*Implements F-06, F-07, F-50.*

- [x] `core/config.rs`: structs from `CLAUDE.md §6`, serde+TOML, `version` field, defaults
- [x] Atomic save (temp file + rename); corrupt config → back up as `.bad`, load defaults, notify *(notify = `LoadOutcome::Recovered`; tray notification wiring pending)*
- [x] Migration scaffold (`v1 → v2` stub + test) so future schema changes are painless
- [ ] Pure `App::update(event) -> Vec<Effect>` with `UserCommand`s; effects executor in the platform layer
- [ ] Tray icon + menu (per D8): Pick file… / per-monitor submenu (enable, zoom mode, audio source placeholder) / Pause-Resume / Start with Windows / Open config folder / Quit
- [ ] File picker off the message thread (per D9) → result returned via `PostMessage`
- [ ] Autostart via `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (toggle); quoted path
- [ ] Restore wallpapers on launch from config; missing file → skip with notification
- [ ] Custom zoom editing: initial approach = edit `config.toml` + "Reload config" menu item (UI for it is P1)
- [ ] Tray notifications (balloon/toast) for errors, rate-limited

**Gate M5 (MVP functional):** launch → tray → pick a file → plays on chosen monitors with chosen modes → quit → relaunch restores everything.

---

## M6 — Pause engine

*Implements F-30, F-31, F-33. (F-32, F-34 in P1.)*

- [x] `core/pause.rs`: `PauseReasons` bitflags + reducer + `PlayState`; unit tests for overlapping reasons and sticky `USER` *(also `PauseTracker` with scope handling and `Hysteresis`; std-only, no `bitflags` crate)*
- [x] `core/coverage.rs`: N×M grid coverage math (rects in, covered fraction out); tests incl. partial overlap, multiple windows, negative origins
- [ ] `platform/events.rs`: `SetWinEventHook` for foreground change, minimize start/end, location change; debounce 150–250 ms; marshal into `AppEvent` on the main thread
- [ ] `platform/fullscreen.rs`: window enumeration for coverage (skip own windows, shell/taskbar, tool windows, cloaked windows) + `SHQueryUserNotificationState`
- [ ] Hysteresis: pause after ~250 ms covered, resume after ~100 ms uncovered *(pure `Hysteresis` + tests done; wiring to one-shot timers pending)*
- [ ] Per-monitor `COVERED` handling; apply with mpv `pause`
- [ ] Global reasons: session lock/unlock + remote (`WTSRegisterSessionNotification`), sleep/resume (`WM_POWERBROADCAST`)
- [ ] Manual pause (`USER`) from tray; never auto-cleared
- [ ] 1 s fallback timer active **only** while a fullscreen-sized window exists; no timer otherwise
- [ ] Verify with: exclusive-fullscreen game, borderless-fullscreen game, maximized browser, maximized-but-unfocused window, video player fullscreen, Alt-Tab animation (no flapping)
- [ ] Verify ~0% CPU/GPU while paused (Task Manager GPU engines)
- [ ] Log every reason transition (debug level) to make field bugs diagnosable

**Gate M6:** covering one monitor pauses only that monitor; locking/sleeping pauses all; resume is immediate and glitch-free; idle-while-paused cost is ~0.

---

## M7 — Audio

*Implements F-40 – F-42.*

- [ ] Default: all monitors silent; non-source monitors use `aid=no` (no audio decode at all)
- [ ] Single `audio.source: Option<MonitorId>`; setting a new source silences the previous one
- [ ] Tray: "Play audio from this screen" per monitor (radio behavior) + "No audio"
- [ ] Volume presets 0/25/50/75/100 → mpv `volume`; persisted
- [ ] Audio follows pause state (paused video = paused audio in MVP)
- [ ] Removing the source monitor → silent (never auto-moves); notify
- [ ] Verify the same video on 3 monitors yields exactly one audio stream
- [ ] First launch never plays sound; check muted state before the first frame renders
- [ ] Verify loop-point gap on a few real files; document the behavior

**Gate M7:** one chosen monitor plays audio at the chosen volume; others are silent and don't decode audio; settings persist.

---

## M8 — Robustness / self-healing

- [ ] Explorer restart: handle the `TaskbarCreated` registered message → re-detect layout, re-attach all windows (use runtime `wid` change if supported)
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
- [ ] Verify against budgets in `CLAUDE.md §4`; update the PRD with real numbers

**Gate M9:** budgets met or deviations documented with reasons.

---

## M10 — Packaging & first release

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
- [ ] F-34 Pause scope: this monitor vs all monitors (config + tray) *(pure scope logic already in `PauseTracker`; config + tray pending)*
- [ ] F-43 Audio continues while video pauses (`vid=no` when covered; restore on resume); verify zero video cost *(pure `AudioOnly` reducer branch done)*
- [ ] F-09 Playback speed (0.25x–2x)
- [ ] F-11 Span mode (one canvas across all monitors; per-monitor crop; mixed-size/bezel design note first) *(`union_rect` helper exists)*
- [ ] F-51 Minimal settings window: monitor layout preview, per-monitor file/zoom, custom zoom/pan sliders with live preview
- [ ] Interactive zoom/pan editor (drag to pan, scroll to zoom) writing back to the profile
- [ ] Per-monitor "reset to default" and "copy settings to…" actions
- [ ] Auto-update check (opt-in; no telemetry)

## P2 — Later

- [ ] F-10 Playlist / folder / shuffle with interval
- [ ] F-12 Import optimizer (bundled FFmpeg): target-monitor downscale, fps cap, codec pick by GPU support, audio strip, loop-friendly keyframes, before/after size report, GIF→MP4 transcode
- [ ] F-35 Per-app pause rules
- [ ] F-44 Auto-duck/mute on other-app audio (Windows audio session APIs)
- [ ] F-45 Volume slider, per-monitor volume, output device selection
- [ ] F-52 CLI control (`--set`, `--pause`, `--quit`) via named pipe / second-instance message
- [ ] ARM64 build
- [ ] Localization scaffold

## Future ideas (unscheduled)

- [ ] F-13 URL import via yt-dlp (user-initiated download, local file result)
- [ ] F-14 Static wallpapers (JXL/PNG/JPG), instant fallback frame while loading, thumbnails for the picker
- [ ] F-36 Pause on high CPU/GPU load
- [ ] Time-of-day / scheduled wallpapers
- [ ] Lock-screen integration (revisit once Microsoft's native video wallpaper behavior is known)
- [ ] Re-evaluate vs Windows native video wallpaper when it reaches general availability

---

## Manual regression checklist (run before each release)

**Layouts:** [ ] Win10 (LTSC 21H2) [ ] *(extended)* Win11 ≤23H2 [ ] *(extended)* Win11 24H2/25H2

**Display:** [ ] 1 monitor [ ] 3 monitors mixed size [ ] mixed DPI [ ] mixed refresh [ ] monitor left/above primary [ ] hot-plug [ ] resolution change [ ] rotate a monitor

**Zoom:** [ ] Fill [ ] Fit [ ] Stretch [ ] Original [ ] Custom (zoom + pan) [ ] mode switching leaves no stale state

**Pause:** [ ] exclusive fullscreen game [ ] borderless game [ ] maximized window (focused) [ ] maximized (unfocused) [ ] lock/unlock [ ] sleep/wake [ ] RDP [ ] manual pause persists through auto events

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
