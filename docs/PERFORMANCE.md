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

The v0.7.2-beta.1 release candidate passed 58 unit tests plus 396 partial-render, 96 surface-reuse and 180 recolor comparisons on the development machine. Maximum per-channel difference from fresh rendering was 1/255. Eye-return cleanup and idle audio exclusion matched exactly. These are correctness checks, not FPS measurements or live service/device tests.

- Unit checks for deadlines, persistence, Unicode editing, lyrics matching and startup parsing.
- Native click/hover/pin, save/reopen, layout, visibility and worker tests.
- Partial/full, retained/fresh and recolor/fresh pixel comparisons at three scales and both positions.
- Portable PE import and embedded-notice checks.
- Explicit session/driver preconditions; unavailable integration is not treated as verified.

See [BUILDING.md](BUILDING.md). Inspect generated native captures before sharing them.
