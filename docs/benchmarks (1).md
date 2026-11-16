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
- `video-sync=audio` cut GPU 3D from ~20 % to ~8 % in this run. Needs confirmation (repeat
  runs, visual smoothness, other frame rates/refresh rates) before it becomes the default.
- `vo=gpu` vs `gpu-next`: difference is within the expected noise (to be confirmed with repeats).

## Open
- Repeat runs (`-Repeat 2+`) to get the noise level; correct CPU %; real VideoDecode numbers.
- Other content: 1080p30, 4K30 HEVC, AV1, large GIF.
- Smoothness check by eye for `video-sync=audio` / `desync` on this 120 Hz panel.
