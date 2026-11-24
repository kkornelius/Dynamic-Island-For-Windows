# Building and verification

Requirements: Windows x64, current stable Rust/MSVC, Visual Studio C++ Build Tools and a Windows SDK. Optional release scripts use Python 3.11+. Pillow/NumPy are only needed to generate demo media.

The application runs without Rust or Python. `.cargo/config.toml` enables static CRT linking for Windows x64. `scripts/build-release.ps1` also remaps builder-specific paths in diagnostics. The PE verifier checks for extra VC++ runtime imports, personal Windows paths and embedded license notices.

```powershell
python scripts/fetch-icons.py
cargo build --release --locked
.\target\release\dynamic-island-for-windows.exe
```

## Source checks

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
.\scripts\build-release.ps1
python scripts/verify-portable.py target/release/dynamic-island-for-windows.exe
```

CI runs build/unit checks on Windows. Native desktop tests are separate; headless CI does not verify real Spotify, Bluetooth or browser behavior.

## Native tests

Quit other instances and use isolated data:

```powershell
.\target\release\dynamic-island-for-windows.exe --self-test --offline --data-dir artifacts\native-test
.\target\release\dynamic-island-for-windows.exe --verify-partial-render --data-dir artifacts\render-test
.\target\release\dynamic-island-for-windows.exe --verify-canvas-reuse --data-dir artifacts\render-test
.\target\release\dynamic-island-for-windows.exe --verify-color-reuse --data-dir artifacts\render-test
```

Profiles use the isolated directory; rendering comparisons write reports under `artifacts/` (some use fixed filenames). Session-dependent checks need a compatible session. Native integration captures can contain current media/artwork; inspect them before sharing. `artifacts/` is excluded from Git and packages.

## Reproduce the screenshots and GIF

```powershell
python -m pip install Pillow numpy
cargo build --release --locked
python scripts/render-demo.py
```

The script supplies an isolated profile to `--render-demo`. It exports original sample text/artwork and example weather/Notes. Reviewed output goes to `docs/media/`; raw frames/QA reports remain in `artifacts/`. It does not capture the desktop or a live account.

## Package

```powershell
python scripts/generate-notices.py
.\scripts\package.ps1
```

The script builds/checks the EXE, assembles curated docs/licenses, and writes the standalone EXE, ZIP and checksums under `dist/`. Regenerate embedded notices when dependencies/assets change.

## Clean generated files

After closing old builds, preview cleanup and then run it:

```powershell
.\scripts\clean.ps1 -BuildCache -Diagnostics -OldDistributions -WhatIf
.\scripts\clean.ps1 -BuildCache -Diagnostics -OldDistributions
```

This removes Cargo build output, Python bytecode, temporary diagnostic profiles/captures and older distribution files. It keeps the version named in Cargo.toml in `dist/`, compact reports in `artifacts/release-validation/`, source, assets and public docs. App data under `%LOCALAPPDATA%` is outside its scope. The next build recreates its build cache.
