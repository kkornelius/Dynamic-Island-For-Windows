[CmdletBinding(SupportsShouldProcess = $true, ConfirmImpact = 'Low')]
param(
    [switch]$BuildCache,
    [switch]$Diagnostics,
    [switch]$OldDistributions
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).ProviderPath
$rootPrefix = $root.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
if (-not ($BuildCache -or $Diagnostics -or $OldDistributions)) {
    throw 'Choose -BuildCache, -Diagnostics and/or -OldDistributions. Add -WhatIf to preview.'
}
$version = [regex]::Match((Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value
if ($version -notmatch '^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$') { throw 'Cannot determine the current version.' }
$keep = @("DynamicIslandForWindows-$version-windows-x64.exe", "DynamicIslandForWindows-$version-windows-x64.zip", 'SHA256SUMS.txt')
$targets = @()
if ($BuildCache) {
    $targets += Join-Path $root 'target'
    $targets += Join-Path $PSScriptRoot '__pycache__'
}
if ($Diagnostics -and (Test-Path -LiteralPath (Join-Path $root 'artifacts'))) {
    $targets += Get-ChildItem -LiteralPath (Join-Path $root 'artifacts') -Force |
        Where-Object Name -NE 'release-validation' | Select-Object -ExpandProperty FullName
}
if ($OldDistributions -and (Test-Path -LiteralPath (Join-Path $root 'dist'))) {
    $targets += Get-ChildItem -LiteralPath (Join-Path $root 'dist') -Force |
        Where-Object { $_.Name -notin $keep } | Select-Object -ExpandProperty FullName
}
$removed = 0
foreach ($target in $targets) {
    if (-not (Test-Path -LiteralPath $target)) { continue }
    $item = Get-Item -LiteralPath $target -Force
    $absolute = [IO.Path]::GetFullPath($item.FullName)
    if (-not $absolute.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Cleanup target escapes the project: $absolute"
    }
    $relative = $absolute.Substring($rootPrefix.Length)
    if ($relative -ne 'target' -and $relative -ne 'scripts\__pycache__' -and
        -not $relative.StartsWith('artifacts\', [StringComparison]::OrdinalIgnoreCase) -and
        -not $relative.StartsWith('dist\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Cleanup target is not a generated output: $relative"
    }
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Refusing linked target: $relative" }
    if ($item.PSIsContainer) {
        $links = @(Get-ChildItem -LiteralPath $absolute -Force -Directory -Recurse |
            Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint })
        if ($links.Count) { throw "Refusing directory containing links: $relative" }
    }
    if ($PSCmdlet.ShouldProcess($absolute, 'Remove generated project output')) {
        Remove-Item -LiteralPath $absolute -Recurse -Force
        $removed++
    }
}
Write-Output "Removed $removed generated items. Current release files and artifacts/release-validation were retained."
