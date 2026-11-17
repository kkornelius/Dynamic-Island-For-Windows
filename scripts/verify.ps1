$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed.' }
    cargo check --locked
    if ($LASTEXITCODE -ne 0) { throw 'Cargo check failed.' }
    cargo clippy --all-targets --locked -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw 'Unit tests failed.' }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
} finally { Pop-Location }
