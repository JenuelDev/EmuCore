param(
    [Parameter(Mandatory = $true)][string]$Elf,
    [Parameter(Mandatory = $true)][string]$Output,
    [ValidateRange(1, 1000)][int]$Runs = 20,
    [ValidateRange(0, 100)][int]$Warmup = 3,
    [string[]]$GuestArguments = @()
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$emuRoot = Split-Path $PSScriptRoot -Parent
$emuExecutable = Join-Path $emuRoot 'target/release/emucore.exe'
if (-not (Test-Path -LiteralPath $emuExecutable -PathType Leaf)) {
    throw 'Build the release executable with scripts/check.ps1 first.'
}
$emuElf = (Resolve-Path -LiteralPath $Elf).Path
$emuOutput = [IO.Path]::GetFullPath($Output)
$emuMetadata = [ordered]@{
    type = 'environment'
    schema_version = 1
    captured_utc = [DateTime]::UtcNow.ToString('o')
    os = [Environment]::OSVersion.VersionString
    logical_processors = [Environment]::ProcessorCount
    powershell = $PSVersionTable.PSVersion.ToString()
    stopwatch_frequency = [Diagnostics.Stopwatch]::Frequency
    executable = $emuExecutable
    executable_sha256 = (Get-FileHash -LiteralPath $emuExecutable -Algorithm SHA256).Hash
    elf = $emuElf
    elf_sha256 = (Get-FileHash -LiteralPath $emuElf -Algorithm SHA256).Hash
    guest_arguments = $GuestArguments
    requested_runs = $Runs
    requested_warmup = $Warmup
}
# CreateNew preserves previous captures. No output directory is created implicitly.
$emuStream = [IO.File]::Open($emuOutput, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
$emuWriter = [IO.StreamWriter]::new($emuStream, [Text.UTF8Encoding]::new($false))
$emuFailures = 0
try {
    $emuWriter.WriteLine(($emuMetadata | ConvertTo-Json -Depth 5 -Compress))
    $emuWriter.Flush()
    for ($emuIndex = 0; $emuIndex -lt $Warmup + $Runs; $emuIndex++) {
        $emuStopwatch = [Diagnostics.Stopwatch]::StartNew()
        $emuRaw = @(& $emuExecutable 'profile' $emuElf '--' @GuestArguments 2>&1)
        $emuExit = $LASTEXITCODE
        $emuStopwatch.Stop()
        $emuText = ($emuRaw | ForEach-Object { $_.ToString() }) -join "`n"
        $emuMetrics = $null
        $emuDiagnostic = $null
        try {
            $emuParsed = $emuText | ConvertFrom-Json -ErrorAction Stop
            if ($emuParsed.schema_version -ne 1 -or $emuParsed.backend -ne 'native' -or
                $emuParsed.exit_code -ne $emuExit) {
                throw 'Missing or inconsistent runtime report.'
            }
            $emuMetrics = $emuParsed
            if ($emuExit -ne 0) { $emuFailures++ }
        } catch {
            $emuFailures++
            $emuDiagnostic = $emuText
        }
        $emuRecord = [ordered]@{
            type = 'run'
            phase = $(if ($emuIndex -lt $Warmup) { 'warmup' } else { 'measured' })
            index = $emuIndex
            parent_elapsed_ns = [long]($emuStopwatch.ElapsedTicks * (1000000000.0 / [Diagnostics.Stopwatch]::Frequency))
            host_exit_code = $emuExit
            metrics = $emuMetrics
            diagnostic = $emuDiagnostic
        }
        $emuWriter.WriteLine(($emuRecord | ConvertTo-Json -Depth 5 -Compress))
        $emuWriter.Flush()
    }
} finally {
    $emuWriter.Dispose()
}
Write-Output "Saved $Runs measured runs and $Warmup warmup runs to $emuOutput ($emuFailures failures)."
if ($emuFailures -gt 0) { throw 'One or more runs failed; the capture preserves their diagnostics.' }
