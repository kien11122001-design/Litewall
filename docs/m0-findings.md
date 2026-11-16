# M0 findings (Windows 10 IoT LTSC 21H2 build 19044, Classic WorkerW)

Date: 2026-11-10. Machine: HP, Ryzen 8845HS, integrated Radeon (driver 32.0.32015.7029).
Monitors: [0] 2560x1600 primary @(0,0); [1] 1080x1920 portrait @(-1080,-314); [2] 1920x1080 @(-3000,854).

## Result: GATE M0 PASSED (Windows 10 scope, Option A)

| Check | Result | Evidence |
|---|---|---|
| libmpv loads via `libloading`, hand-written FFI works | PASS | `mpv_initialize rc=0`, all PRD §7 options accepted (no `FAILED` lines) |
| Sanity: video in a normal window (`--normal`) | PASS | screenshot; an earlier black window was a missing `clip.mp4` (`loadfile` returns 0 even for missing files) |
| Layout detection | Classic | after `0x052C(0xD,1)`: icons' `SHELLDLL_DefView` moved to WorkerW `0x409C0`; wallpaper WorkerW = `0x1A09E8` (sibling after it); `defview_in_progman=false` |
| Spawn variant | `0xD, 1` works | `--spawn-variant 1` not needed |
| Option A: `wid` into WorkerW, video behind icons | PASS | spike.log (SetParent ok, rect correct) + owner report |
| Parent-relative coords, negative origin | PASS | parent origin (-3000,-314); primary child=(3000,314); portrait monitor child=(1920,0) with size 1080x1920 |
| Click-through, desktop menu, not in taskbar / Alt-Tab, Win+D | PASS (owner-reported, not in logs) | |
| Runtime `wid` change | PASS | `runtime wid change -> rc=0` at +8 s (`--rewid`) |
| HW decode | d3d11va | `hwdec-current=d3d11va`, `vo=gpu-next`, `dropped=0` |

Spawn/attach details to carry into M2:
- Enumerate top-level windows; the one that has a `SHELLDLL_DefView` child gives the icon host; the next `WorkerW` sibling is the wallpaper host.
- Style flow that worked: create `WS_POPUP`, then clear POPUP, set `WS_CHILD|WS_VISIBLE`, `SetParent(workerw)`, `SetWindowPos(HWND_BOTTOM, rel_x, rel_y, w, h, NOACTIVATE|SHOWWINDOW)`. Ex-style `WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE`.
- Parent origin = `GetWindowRect(workerw)` top-left (here (-3000,-314), the virtual-desktop origin).

## Baseline numbers (1080p60 H.264 `Big_Buck_Bunny_1080_10s_5MB.mp4`, one monitor, 2560x1600 @120 Hz, `vo=gpu-next`, `hwdec=auto-safe`)

- Process `private` = 313-345 MB, `working_set` = 109-130 MB (whole process incl. mpv). **Above the PRD hypothesis (small cache); x3 monitors would be ~1 GB private.** To investigate in M3/M9.
- Task Manager (whole system): CPU ~4 %, GPU 3D ~28 %, Copy ~2 %, dedicated GPU mem 1.4 GB. The "Video Decode" engine graph was not in the screenshot, so decode-engine use is **not yet visually confirmed** (mpv reports d3d11va).
- 28 % GPU 3D for one 1080p60 wallpaper on the iGPU is far from the "cheap" goal; candidates to benchmark: `--vo gpu`, `--hwdec d3d11va-copy`, `video-sync`, upscale cost 1080p -> 2560x1600.

## Open items
- libmpv build identity not recorded: the DLL in use is ~97 MB (looks like a full/GPL build). Record version, source URL, SHA-256 (`Get-FileHash`, `(Get-Item libmpv-2.dll).VersionInfo`), and test an LGPL build (needed for D1).
- Benchmarks #6 (`--vo gpu`, `--hwdec d3d11va-copy|no`) not run yet.
- Monitor [2] not tested (monitors [0] and [1] pass).
- Raised layout (Win11 24H2+): *(extended)*, untouched.

## Decisions
- **D3 = A** (child window + `wid` into the Classic WorkerW). B/C only if Windows 11 support is taken up.
- **D4 = hand-written FFI + `libloading`** (proven here; ~10 functions needed).
