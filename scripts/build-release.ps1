$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$previousEncodedFlags = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location $root
try {
    python scripts/fetch-icons.py
    if ($LASTEXITCODE -ne 0) { throw 'Icon preparation failed.' }
    # Encoded arguments preserve spaces in paths. These flags also retain static CRT
    # linking when overriding Cargo's target rustflags. Avoid publishing builder paths
    # in panic messages from the workspace, Cargo registry or Rust toolchain sources.
    $flags = @('-C', 'target-feature=+crt-static')
    foreach ($mapping in @(
        @($env:USERPROFILE, '/build/user'),
        @($env:CARGO_HOME, '/build/cargo'),
        @($root, '/build/workspace')
    )) {
        if ($mapping[0]) { $flags += '--remap-path-prefix=' + $mapping[0] + '=' + $mapping[1] }
    }
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f
    cargo +stable build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedFlags
    Pop-Location
}
