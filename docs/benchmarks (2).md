# Benchmarks

Machine: HP, Ryzen 8845HS, integrated Radeon (driver 32.0.32015.7029), Windows 10 LTSC 21H2.
Output: one monitor, 2560x1600 @ 120 Hz. Build: release. Defaults from `src/mpv/player.rs`
(`vo=gpu-next`, `hwdec=auto-safe`, `video-sync=display-desync`) unless a variant says otherwise.
Tool: `scripts/bench.ps1` (8 s warm-up, then counter samples; per-process GPU Engine counters).

## Run 1 (2026-11-10): 1080p60 H.264, ~5 MB, 10 s loop, 15 samples, single run per variant

| Variant | hwdec active | CPU % (all cores)* | GPU 3D % | Private MB | Working set MB |
|---|---|---|---|---|---|
| baseline | d3d11va | 0.4 | 19.8 | 334 | 111 |
| `vo=gpu` | d3d11va | 0.5 | 17.7 | 305 | 111 |
| `hwdec=d3d11va-copy` | d3d11va-copy | 1.4 | 22.0 | 381 | 154 |
| `hwdec=no` | no | 4.6 | 19.8 | 304 | 237 |
| `video-sync=audio` | d3d11va | 0.3 | **8.2** | 305 | 113 |
| `video-sync=display-resample` | d3d11va | 0.6 | 20.0 | 336 | 114 |

\* In run 1 the CPU figure divided CPU time by the nominal sample count, not the measured
elapsed time, so absolute values may be too low (up to ~2x). The ranking between variants is
unaffected. Fixed in the current `bench.ps1`.
The GPU video-decode column was 0 for every variant: the AMD driver names that engine
`video codec 0`, which the script did not match yet. Fixed in the current `bench.ps1`.

### Reading
- `d3d11va-copy` is strictly worse than zero-copy `d3d11va` (more CPU, more memory, no GPU gain).
- Software decode costs only ~4.6 % CPU for this clip, a usable fallback; the 3D cost is
  identical, so most of the GPU load is rendering/presenting, not decoding.
- `video-sync=audio` cut GPU 3D from ~20 % to ~8 % in this run, **but the owner sees real judder
  with it (2026-11-10), so it is rejected as the default** (D12). Keep `display-desync`.
  Possible opt-in "low GPU" mode later.
- `vo=gpu` vs `gpu-next`: difference is within the expected noise (to be confirmed with repeats).

## Run 2 (2026-11-10): same clip, 3 runs per variant, 10 samples each (means)

Noise is small (GPU 3D within about 0.5 points, memory within 1 MB), so the differences below are real.
CPU % is now computed over measured elapsed time (share of all 16 logical cores).

| Variant | CPU % | GPU 3D % | GPU video-decode % | Private MB | Working set MB |
|---|---|---|---|---|---|
| baseline (gpu-next, display-desync) | 0.27 | 19.9 | 12.3 | 336 | 113 |
| `vo=gpu` | 0.22 | **17.5** | 12.1 | **307** | 113 |
| `hwdec=d3d11va-copy` | 0.77 | 22.1 | 12.2 | 384 | 156 |
| `hwdec=no` (software) | 2.29 | 19.8 | 0 | 300 | 227 |
| `video-sync=audio` | 0.17 | 8.2 | 12.4 | 304 | 113 |
| `video-sync=desync` | 0.17 | 8.3 | 12.6 | 304 | 113 |
| `video-sync=display-resample` | 0.21 | 19.8 | 12.3 | 336 | 113 |
| `audio` + `vo=gpu` | 0.23 | 11.5 | 12.8 | 307 | 112 |
| `audio` + `swapchain-depth=1` | 0.16 | 7.6 | 12.0 | 304 | 113 |

### Reading
- `vo=gpu` beats `gpu-next` with the default `display-desync` by ~2.4 points of GPU 3D (-12 %) and ~29 MB. Adopted as the default (D14). With `video-sync=audio` the order flips (`gpu-next` is cheaper), which is irrelevant while `audio` is rejected.
- Hardware decode uses ~12 % of the iGPU video engine for this clip; software decode costs ~2.3 % CPU (about 0.37 of one core) and uses no video engine. 3D load is identical, so the 3D cost is rendering/presenting, not decoding. `hwdec=auto-safe` (zero-copy `d3d11va`) stays the default; software is a fine fallback.
- `d3d11va-copy` is worse on every axis.
- Every variant sits at ~300-385 MB private memory regardless of decode path: the base cost is mpv + D3D11 + driver, not the demuxer cache. Investigate in M9 (per-monitor cost would be ~1 GB on three screens).
- The ~8 % GPU of `audio`/`desync` comes from presenting frames when they are due instead of every display refresh, and that is also what produces the judder (D12). Ideas for M9 that keep display sync but cost less: `display-vdrop`, `display-adrop`, `display-tempo`, `swapchain-depth=1` together with display sync.

## Open
- M9 candidates listed under Run 2.
- Other content: 1080p30, 4K30 HEVC, AV1, large GIF.
- Smoothness check by eye for `video-sync=audio` / `desync` on this 120 Hz panel.
