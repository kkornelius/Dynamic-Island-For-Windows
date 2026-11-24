# Changelog

## 0.7.3-beta.1 — 2026-11-24

- Smoother spring opening, collapse and capsule notifications, with high-resolution timing and one coalesced frame clock.
- Animated floating/anchored switching with continuous position, width and border shape.
- Direct minute/second timer input, a monochrome vector hourglass and reliable countdown redraw deadlines.
- Capsule-only alerts, improved fullscreen recovery and hiding while a browser address/search field overlaps the island.
- Sharper cached Icons8 icons; source credits live in the tray About menu.
- Ctrl+Backspace/Ctrl+Delete word deletion in Notes.
- Improved browser music/lyrics lookup and YouTube player cross-checking when Windows media metadata or timing is stale.
- Refreshed documentation, native screenshots and demo media.

## 0.7.3-preview.4 — Local testing, not published

- Redraw the timer hourglass as a minimal monochrome vector illustration.
- Click minutes/seconds to type a duration, with Enter, Tab, Escape, paste and focus-loss handling; keep the existing steppers and presets.
- Animate position, width and border attachment together when switching floating/anchored modes, including reversing an in-progress switch.
- Preserve the earliest scheduled redraw so media/status updates cannot postpone a countdown tick.

## 0.7.3-preview.3 — Local testing, not published

- Advance animation timestamps with a high-resolution monotonic clock to avoid coarse uptime-clock jumps between evenly paced frames.
- Give collapse its own softer spring while keeping the existing opening response.
- Fade panel content into a centered compact layout instead of switching layouts immediately.
- Keep the backdrop fitted to the animated body and the glass stroke independent of content fades.

## 0.7.3-preview.2 — Local testing, not published

- Resample icons to the actual display pixels before drawing, with pixel-aligned placement.
- Move source/icon credits into the tray About menu.
- Notes support Ctrl+Backspace and Ctrl+Delete word deletion with undo.
- Cross-check YouTube title and seek/play/pause controls when browser SMTC metadata or timestamps are stale.
- Retry previously cached plain/unavailable lyrics with the updated resolver.

## 0.7.3-preview.1 — Local testing, not published

- Bouncy spring transitions for closing/opening and capsule notifications, including queued alerts.
- One high-resolution, coalesced frame clock and batched rendering.
- App notifications appear only in the capsule; no duplicate Windows tray balloons.
- Restore after a destroyed/cloaked fullscreen blocker and rescan on display/resume changes.
- Step aside while a browser address/search field is focused and overlaps the capsule, without reading its value.
- Browser music metadata normalization for LRCLIB, plus playing-session selection across players.
- Icons8 SF Regular app icons with cached decoding and an attribution link.
- Preview builds leave the stable startup executable untouched.

## 0.7.2-beta.1 — First public beta

- Native media controls, optional timed lyrics and system-output visualizer.
- Weather, timer, water reminders and local quick notes.
- Eyes, hover/pin interaction, floating/anchored positions and customizable glow.
- Fullscreen/browser hiding and Bluetooth connection notices.
- Opt-in per-user startup with a stable EXE copy.
- Single-file x64 EXE with static CRT, embedded font/sound/license notices.
- Release diagnostics remap builder-specific source paths.
- Original sample-data screenshots/GIF and reproducible native media tools.
- Curated build/privacy/architecture docs, file audit and Windows CI.
- Compact weather-source info link for attribution.

Unsigned portable beta; no installer or automatic updater. Integration depends on Windows sessions, drivers and provider catalogs.
