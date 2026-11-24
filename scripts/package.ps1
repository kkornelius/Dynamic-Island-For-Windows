$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$stage = $null
Push-Location $root
try {
    python scripts/generate-notices.py
    if ($LASTEXITCODE -ne 0) { throw 'License generation failed.' }
    & (Join-Path $PSScriptRoot 'build-release.ps1')
    $binary = Join-Path $root 'target\release\dynamic-island-for-windows.exe'
    python scripts/verify-portable.py $binary
    if ($LASTEXITCODE -ne 0) { throw 'Portable binary checks failed.' }
    $metadata = cargo metadata --offline --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the package version.' }
    $version = ($metadata.packages | Where-Object name -EQ 'dynamic-island-for-windows').version
    if ($version -notmatch '^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$') { throw 'Invalid release version.' }
    $name = 'DynamicIslandForWindows-' + $version + '-windows-x64'
    $dist = Join-Path $root 'dist'
    # A fresh directory prevents old/unreviewed files entering the ZIP.
    $stage = Join-Path $root ('artifacts\public-release\package-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stage, $dist -Force | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $stage 'dynamic-island-for-windows.exe')
    Copy-Item -LiteralPath 'README.md', 'LICENSE', 'Cargo.lock', 'CHANGELOG.md', 'CONTRIBUTING.md', 'SECURITY.md' -Destination $stage
    Copy-Item -LiteralPath 'assets\licenses\NOTICES.txt' -Destination (Join-Path $stage 'THIRD_PARTY_NOTICES.txt')
    $docs = Join-Path $stage 'docs'
    $media = Join-Path $docs 'media'
    New-Item -ItemType Directory -Path $docs, $media -Force | Out-Null
    $publicDocs = @('ARCHITECTURE', 'BUILDING', 'USAGE', 'PRIVACY', 'PERFORMANCE', 'RELEASING', 'PUBLIC-FILES')
    foreach ($document in $publicDocs) {
        Copy-Item -LiteralPath ('docs\' + $document + '.md') -Destination $docs
    }
    $publicMedia = @('overview.png', 'demo.gif', 'idle.png', 'compact.png', 'media.png', 'lyrics.png', 'weather.png', 'timer.png', 'water.png', 'notes.png', 'settings.png')
    foreach ($picture in $publicMedia) {
        Copy-Item -LiteralPath (Join-Path 'docs\media' $picture) -Destination $media
    }
    # Keep relative credit links usable in the extracted package.
    foreach ($relative in @('assets\fonts\OFL.txt', 'assets\sounds\README.md', 'assets\licenses\NOTICES.txt', 'assets\icons8\README.md', 'assets\icons8\manifest.json')) {
        $destination = Join-Path $stage $relative
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        Copy-Item -LiteralPath $relative -Destination $destination
    }
    $internalHash = Get-FileHash -LiteralPath (Join-Path $stage 'dynamic-island-for-windows.exe') -Algorithm SHA256
    ($internalHash.Hash.ToLowerInvariant() + '  dynamic-island-for-windows.exe') | Set-Content -LiteralPath (Join-Path $stage 'SHA256SUMS.txt') -Encoding ascii
    $standalone = Join-Path $dist ($name + '.exe')
    $archive = Join-Path $dist ($name + '.zip')
    Copy-Item -LiteralPath $binary -Destination $standalone -Force
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -Force
    $hashes = foreach ($path in @($standalone, $archive)) {
        $digest = Get-FileHash -LiteralPath $path -Algorithm SHA256
        $digest.Hash.ToLowerInvariant() + '  ' + (Split-Path -Leaf $path)
    }
    $hashes | Set-Content -LiteralPath (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii
    Get-Item -LiteralPath $standalone, $archive, (Join-Path $dist 'SHA256SUMS.txt') | Select-Object Name, Length
} finally {
    if ($stage -and (Test-Path -LiteralPath $stage)) {
        $absolute = (Resolve-Path -LiteralPath $stage).ProviderPath
        $allowed = [IO.Path]::GetFullPath((Join-Path $root 'artifacts\public-release')).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
        if (-not $absolute.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove a package stage outside its generated directory.'
        }
        Remove-Item -LiteralPath $absolute -Recurse -Force
    }
    Pop-Location
}
