param(
    [string]$LibraryPath = '.tools/client/library.json',
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [ValidateRange(1, 20)][int]$Runs = 5,
    [ValidateRange(2, 60)][int]$IdleSeconds = 5
)
$ErrorActionPreference = 'Stop'
$emuRoot = Split-Path $PSScriptRoot -Parent
$emuExe = Join-Path $emuRoot 'target/release/emucore-client.exe'
if (-not (Test-Path -LiteralPath $emuExe -PathType Leaf)) { throw 'Build the release client first.' }
$emuLibrary = [IO.Path]::GetFullPath($LibraryPath, $emuRoot)
if ($emuLibrary.Contains('"')) { throw 'Library path must not contain a quote.' }
$emuOutput = [IO.Path]::GetFullPath($OutputDirectory, $emuRoot)
if (Test-Path -LiteralPath $emuOutput) { throw 'Use a new measurement output directory.' }
New-Item -ItemType Directory -Path $emuOutput | Out-Null
$emuPorts = @(5039, 5580, 5581)
function Assert-NoReferenceListener {
    $emuListeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.LocalPort -in $emuPorts })
    if ($emuListeners.Count -gt 0) { throw 'A reference listener exists; idle client measurements require Android to remain stopped.' }
}
Assert-NoReferenceListener
$emuResults = @()
for ($emuRun = 0; $emuRun -lt $Runs; $emuRun++) {
    $emuProcess = $null
    $emuWatch = [Diagnostics.Stopwatch]::StartNew()
    try {
        # Hidden process measurement only. No UI input or game-launch requests.
        $emuProcess = Start-Process -FilePath $emuExe -ArgumentList ('"' + $emuLibrary + '"') -WorkingDirectory $emuRoot -WindowStyle Hidden -PassThru
        if (-not $emuProcess.WaitForInputIdle(10000)) { throw 'Client did not reach message-queue idle within ten seconds.' }
        $emuQueueIdleMs = $emuWatch.Elapsed.TotalMilliseconds
        $emuProcess.Refresh()
        if ($emuProcess.HasExited) { throw 'Client exited before idle measurement; inspect startup-error.txt.' }
        $emuCpuStart = $emuProcess.TotalProcessorTime.TotalMilliseconds
        $emuIdleWatch = [Diagnostics.Stopwatch]::StartNew()
        for ($emuSecond = 0; $emuSecond -lt $IdleSeconds; $emuSecond++) {
            Start-Sleep -Seconds 1
            Assert-NoReferenceListener
            $emuProcess.Refresh()
            if ($emuProcess.HasExited) { throw 'Client exited during idle measurement.' }
        }
        $emuResults += [pscustomobject]@{
            run = $emuRun
            launch_to_message_queue_idle_ms = $emuQueueIdleMs
            idle_observation_ms = $emuIdleWatch.Elapsed.TotalMilliseconds
            process_cpu_delta_ms = $emuProcess.TotalProcessorTime.TotalMilliseconds - $emuCpuStart
            private_bytes_snapshot = $emuProcess.PrivateMemorySize64
            working_set_bytes_snapshot = $emuProcess.WorkingSet64
            peak_working_set_bytes = $emuProcess.PeakWorkingSet64
            sdk_reference_listeners = 0
        }
    } finally {
        if ($null -ne $emuProcess) {
            if (-not $emuProcess.HasExited) { $emuProcess.Kill($true); $emuProcess.WaitForExit() }
            $emuProcess.Dispose()
        }
    }
}
Assert-NoReferenceListener
$emuReport = [pscustomobject]@{
    measured_at_utc = [DateTime]::UtcNow.ToString('o')
    executable_sha256 = (Get-FileHash -LiteralPath $emuExe -Algorithm SHA256).Hash
    library_sha256 = if (Test-Path -LiteralPath $emuLibrary) { (Get-FileHash -LiteralPath $emuLibrary -Algorithm SHA256).Hash } else { $null }
    boundary = 'Hidden release process, launch to WaitForInputIdle and observed idle snapshots. Includes process creation. No UI input, Android launch, visual verification, cold-cache guarantee, or low-end inference. Process forcibly ended after each sample.'
    runs = $emuResults
}
$emuReport | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $emuOutput 'report.json') -Encoding utf8
$emuResults | Format-Table
exit 0
