# Performance notes

Results depend on hardware, scale, media source and desktop composition; these are not universal CPU/RAM or FPS guarantees.

## Historical local UI measurements

The Notes implementation in v0.7.0 was measured on an Intel Core i5-12400F (12 logical processors), approximately 32 GB RAM, Windows 10 Pro and 100% display scale. Short cases used four seconds of warmup and twelve one-second samples. Mixed menus used fifteen seconds of warmup and ninety seconds of samples.

The instrumented fixture used synthetic media/audio, offline data and logical rendering to an unshown HWND. Real draw/mask/upload calls ran, but live capture, pointer/auto-hide monitoring and physical desktop composition were excluded. CPU is normalized to all twelve logical processors; memory is resident working set including shared pages.

| Workload | Average CPU | Peak working set | Input P95 |
|---|---:|---:|---:|
| Short-note typing | below 0.1% | 39.9 MiB | 3.3 ms |
| Editing 19,964 Latin characters | 0.28% | 41.6 MiB | 12.1 ms |
| Editing 20,000 emoji | 0.11% | 43.8 MiB | 9.0 ms |
| Mixed menus, 90 seconds | 0.20% | 49.9 MiB | n/a |

Almost 20,000 empty lines reached 18.7 ms input P95 and 22.2 ms maximum, exceeding a 60 Hz frame budget. A short soak does not prove freedom from multi-hour leaks. In the mixed run, GDI stayed at four and handles decreased from 286 to 279; memory varied with panels rather than rising monotonically in that interval.

These are historical observations, not a fresh benchmark of every public-beta feature. Private profiles/raw diagnostics are excluded from this repository. Public validation includes unit tests and native rendering comparisons.

## Current verification approach

The v0.7.3 candidate passed 72 unit tests, 82 native interaction checks and 732 render comparisons: 396 partial-render, 96 surface-reuse, 180 recolor, 48 collapse endpoint and 12 dock endpoint cases. Maximum per-channel difference from fresh rendering was 1/255; dock endpoints matched exactly. These are correctness checks, not live service/device coverage.

An offline native fixture measured eight floating/anchored switches at 100% DPI, 3 px stroke and 100% glow. Across 184 animated frames, mean rendering including native upload was 9.16 ms and P95 was 10.20 ms; none exceeded 16.7 ms. This measures app render work on the development machine, not end-to-end display latency or guaranteed FPS on other hardware/scales. The shared frame clock coalesces animation work; redraw scheduling preserves an already-due countdown tick when media/status updates arrive.

- Unit checks for deadlines, persistence, Unicode editing, lyrics matching and startup parsing.
- Native click/hover/pin, save/reopen, layout, visibility and worker tests.
- Partial/full, retained/fresh and recolor/fresh pixel comparisons at three scales and both positions.
- Portable PE import and embedded-notice checks.
- Explicit session/driver preconditions; unavailable integration is not treated as verified.

See [BUILDING.md](BUILDING.md). Inspect generated native captures before sharing them.
