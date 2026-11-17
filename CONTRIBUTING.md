# Contributing

Focused bug reports, reproducible examples and pull requests are welcome. Include app/Windows version, display scale, panel and reproduction steps. Remove private Notes, credentials and sensitive media/device data from attachments.

Build on Windows with stable Rust/MSVC and C++ Build Tools. Run formatting, Clippy, unit tests and release build; see [BUILDING.md](docs/BUILDING.md). Native tests need an interactive desktop and an isolated profile. Playback-control probes can affect the active media session.

Use the neutral palette/vector icon system. Keep blocking work off the UI thread. Document new data flows, dependencies, license requirements and limits. Include relevant validation with PRs.

Generated captures, profiles, outputs and credentials do not belong in commits. Run `python scripts/audit-public.py --staged` and review the diff before publishing.
