# Public-file policy

The repository/release use an explicit allowlist.

| Allowed | Purpose |
|---|---|
| Rust source under `src/` | App and tests |
| Cargo files and `.cargo/config.toml` | Locked dependencies and static CRT configuration |
| Selected `assets/` | Inter/OFL, CC0 gulp/provenance, embedded notices, Icons8 credits/download manifest; raw Icons8 PNGs stay excluded |
| Curated docs and `docs/media/` | Guides and reviewed fictional-data native previews |
| Reviewed scripts and `.github/` | Build, audit, demo, packaging and CI |
| Root README/license/changelog/contribution/security files | Project information |

Excluded from Git and packages:

- `artifacts/`: raw frames, internal audits, working profiles and private publication helpers.
- `target/`: compiler outputs/local paths.
- `dist/`: generated binaries, uploaded separately as selected Release assets.
- Historical internal docs outside the curated list.
- Notes/settings/caches/logs, `.env`, credentials, certificates and private keys.

Public images come from the real renderer with fictional data. No personal desktop recording, live Spotify session or private Notes are used. Images are reviewed in addition to file/content checks. Required third-party license notices remain included.

An automated scan is a supplementary check, not proof that every secret pattern or legal concern has been identified.
