param([int]$SampleSeconds = 15, [string]$OutputName = 'performance')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $root 'target\release\dynamic-island-for-windows.exe'
if (-not (Test-Path -LiteralPath $executable)) { throw 'Build the release executable first.' }
if (Get-Process -Name dynamic-island-for-windows -ErrorAction SilentlyContinue) { throw 'Close the running island before measuring.' }
$output = Join-Path (Join-Path $root 'artifacts') $OutputName
New-Item -ItemType Directory -Path $output -Force | Out-Null
$logicalProcessors = [Environment]::ProcessorCount
$results = @()
foreach ($scenario in @('idle', 'audio', 'audio-expanded', 'timer', 'all', 'water')) {
    $data = Join-Path $output $scenario
    $arguments = @('--benchmark-scenario', $scenario, '--run-for', ($SampleSeconds + 7), '--data-dir', ('"' + $data + '"'))
    $process = Start-Process -FilePath $executable -ArgumentList $arguments -WorkingDirectory $root -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 3
    $process.Refresh()
    $previousCpu = $process.TotalProcessorTime.TotalSeconds
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $previousTime = 0.0
    $samples = @()
    for ($index = 0; $index -lt $SampleSeconds; $index++) {
        Start-Sleep -Seconds 1
        $process.Refresh()
        if ($process.HasExited) { throw "Scenario $scenario exited before sampling finished." }
        $elapsed = $clock.Elapsed.TotalSeconds
        $cpu = $process.TotalProcessorTime.TotalSeconds
        $corePercent = 100 * ($cpu - $previousCpu) / ($elapsed - $previousTime)
        $samples += [pscustomobject]@{
            seconds = [math]::Round($elapsed, 3)
            cpu_one_core_percent = [math]::Round($corePercent, 4)
            cpu_machine_percent = [math]::Round($corePercent / $logicalProcessors, 4)
            working_set_mib = [math]::Round($process.WorkingSet64 / 1MB, 3)
            private_mib = [math]::Round($process.PrivateMemorySize64 / 1MB, 3)
            handles = $process.HandleCount
        }
        $previousCpu = $cpu
        $previousTime = $elapsed
    }
    # Keep the launching STA responsive while the native graphics client exits.
    $exitWatch = [Diagnostics.Stopwatch]::StartNew()
    while (-not $process.HasExited -and $exitWatch.Elapsed.TotalSeconds -lt 15) {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
    }
    if (-not $process.HasExited) { throw "Scenario $scenario did not shut down on schedule." }
    $result = [pscustomobject]@{
        scenario = $scenario
        samples = $samples
        mean_cpu_machine_percent = [math]::Round(($samples | Measure-Object cpu_machine_percent -Average).Average, 4)
        mean_cpu_one_core_percent = [math]::Round(($samples | Measure-Object cpu_one_core_percent -Average).Average, 4)
        peak_working_set_mib = ($samples | Measure-Object working_set_mib -Maximum).Maximum
        peak_private_mib = ($samples | Measure-Object private_mib -Maximum).Maximum
        first_handles = $samples[0].handles
        last_handles = $samples[-1].handles
        exit_code = $process.ExitCode
    }
    $results += $result
    Write-Output ($result | Select-Object scenario, mean_cpu_machine_percent, mean_cpu_one_core_percent, peak_working_set_mib, peak_private_mib, first_handles, last_handles)
}
[pscustomobject]@{logical_processors=$logicalProcessors;sample_seconds=$SampleSeconds;note='Idle: visualizer disabled. Audio scenarios: actual current Windows output. All: expanded media, capture, timer, water scheduler and Bluetooth watcher.';results=$results} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output 'results.json') -Encoding utf8
