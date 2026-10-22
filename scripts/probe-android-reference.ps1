#Requires -Version 7.0
param(
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [string]$SdkRoot = (Join-Path $env:LOCALAPPDATA 'Android/Sdk'),
    [ValidateRange(30, 300)][int]$BootTimeoutSeconds = 120,
    [string]$FixtureApk,
    [ValidateSet('x86_64','arm64-v8a')][string]$ExpectedFixtureAbi
)
$ErrorActionPreference = 'Stop'
$emuRoot = Split-Path $PSScriptRoot -Parent
$emuAndroid = Join-Path $emuRoot '.tools/android'
$emuAvdHome = Join-Path $emuAndroid 'avd'
$emuAvd = Join-Path $emuAvdHome 'EmuCoreReference35.avd'
$emuMarker = Join-Path $emuAvd '.emucore-reference'
$emuExecutable = Join-Path $SdkRoot 'emulator/emulator.exe'
$emuAdb = Join-Path $SdkRoot 'platform-tools/adb.exe'
if (-not (Test-Path -LiteralPath $emuMarker) -or [IO.File]::ReadAllText($emuMarker) -ne "EmuCore Android reference v1`n") {
    throw 'Run scripts/prepare-android-reference.ps1 first.'
}
foreach ($emuTool in @($emuExecutable,$emuAdb)) {
    if (-not (Test-Path -LiteralPath $emuTool -PathType Leaf)) { throw "Missing installed SDK tool: $emuTool" }
}
if ([bool]$FixtureApk -ne [bool]$ExpectedFixtureAbi) { throw 'FixtureApk and ExpectedFixtureAbi must be supplied together.' }
if ($FixtureApk) {
    $FixtureApk = (Resolve-Path -LiteralPath $FixtureApk).Path
    $emuZip = [IO.Compression.ZipFile]::OpenRead($FixtureApk)
    try {
        $emuNativeEntries = @($emuZip.Entries | Where-Object { $_.FullName -like 'lib/*' })
        if ($emuNativeEntries.Count -ne 1 -or $emuNativeEntries[0].FullName -ne "lib/$ExpectedFixtureAbi/libemucore_probe.so") { throw 'The fixture must contain exactly the expected owned JNI library.' }
        $emuHeader = [byte[]]::new(20)
        $emuEntryStream = $emuNativeEntries[0].Open()
        try {
            $emuRead = 0
            while ($emuRead -lt $emuHeader.Length) {
                $emuCount = $emuEntryStream.Read($emuHeader, $emuRead, $emuHeader.Length - $emuRead)
                if ($emuCount -eq 0) { throw 'Truncated JNI ELF header.' }
                $emuRead += $emuCount
            }
        } finally { $emuEntryStream.Dispose() }
        $emuMachine = $(if ($ExpectedFixtureAbi -eq 'x86_64') { 62 } else { 183 })
        if ([BitConverter]::ToString($emuHeader[0..6]) -ne '7F-45-4C-46-02-01-01' -or [BitConverter]::ToUInt16($emuHeader, 18) -ne $emuMachine -or [BitConverter]::ToUInt16($emuHeader, 16) -ne 3) { throw 'The packaged JNI ELF header does not match its advertised ABI.' }
    } finally { $emuZip.Dispose() }
}
# The probe owns these ports and never targets a user's existing adb serial.
foreach ($emuPort in @(5039,5580,5581)) {
    $emuListener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $emuPort)
    try { $emuListener.Start() } finally { $emuListener.Stop() }
}
$emuOutput = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $emuOutput) { throw 'Use a new output directory to preserve existing captures.' }
New-Item -ItemType Directory -Path $emuOutput | Out-Null
$emuSaved = @{}
foreach ($emuName in @('ANDROID_AVD_HOME','ANDROID_USER_HOME','ANDROID_EMULATOR_HOME','ANDROID_ADB_SERVER_PORT','ADB_SERVER_SOCKET')) {
    $emuSaved[$emuName] = [Environment]::GetEnvironmentVariable($emuName, 'Process')
}
$emuProcess = $null
$emuServer = $null
$emuRecord = [ordered]@{
    schema_version = 1
    started_utc = [DateTime]::UtcNow.ToString('o')
    sdk_root = $SdkRoot
    avd = 'EmuCoreReference35'
    serial = 'emulator-5580'
    adb_server_port = 5039
    snapshot = 'disabled'
    graphics_requested = 'host'
    guest_memory_mb_requested = 2048
    guest_cores = 2
    boot_completed_ms = $null
    package_service_ready_ms = $null
    graceful_shutdown = $false
    failure = $null
    cleanup_errors = @()
}
function Invoke-EmuAdb([string[]]$AdbArguments, [int]$TimeoutMilliseconds = 5000) {
    $emuInfo = [Diagnostics.ProcessStartInfo]::new($emuAdb)
    $emuInfo.UseShellExecute = $false
    $emuInfo.CreateNoWindow = $true
    $emuInfo.RedirectStandardOutput = $true
    $emuInfo.RedirectStandardError = $true
    foreach ($emuArgument in @('-P','5039','-s','emulator-5580') + $AdbArguments) { $emuInfo.ArgumentList.Add($emuArgument) }
    $emuChild = [Diagnostics.Process]::Start($emuInfo)
    try {
        $emuStdout = $emuChild.StandardOutput.ReadToEndAsync()
        $emuStderr = $emuChild.StandardError.ReadToEndAsync()
        if (-not $emuChild.WaitForExit($TimeoutMilliseconds)) { $emuChild.Kill($true); $emuChild.WaitForExit(); throw "adb command '$($AdbArguments -join ' ')' exceeded $TimeoutMilliseconds milliseconds." }
        return [pscustomobject]@{ ExitCode=$emuChild.ExitCode; Stdout=$emuStdout.GetAwaiter().GetResult(); Stderr=$emuStderr.GetAwaiter().GetResult() }
    } finally {
        if (-not $emuChild.HasExited) { $emuChild.Kill($true); $emuChild.WaitForExit() }
        $emuChild.Dispose()
    }
}
try {
    $env:ANDROID_AVD_HOME = $emuAvdHome
    $env:ANDROID_USER_HOME = Join-Path $emuAndroid 'user'
    $env:ANDROID_EMULATOR_HOME = Join-Path $emuAndroid 'user'
    $env:ANDROID_ADB_SERVER_PORT = '5039'
    $env:ADB_SERVER_SOCKET = 'tcp:127.0.0.1:5039'
    $emuRecord.emulator_version = (& $emuExecutable -version 2>&1 | Out-String).Trim()
    $emuRecord.acceleration_check = (& $emuExecutable -accel-check 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $emuRecord.acceleration_check -notmatch 'WHPX.*installed and usable') { throw 'WHPX is not confirmed usable.' }
    $emuRecord.emulator_sha256 = (Get-FileHash -LiteralPath $emuExecutable -Algorithm SHA256).Hash
    Copy-Item -LiteralPath (Join-Path $SdkRoot 'system-images/android-35/google_apis_playstore/x86_64/source.properties') -Destination (Join-Path $emuOutput 'image-source.properties')
    Copy-Item -LiteralPath (Join-Path $emuAvd 'config.ini') -Destination (Join-Path $emuOutput 'config.ini')
    $emuServer = Start-Process -FilePath $emuAdb -ArgumentList @('-L','tcp:5039','nodaemon','server') -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $emuOutput 'adb-server.stdout.log') -RedirectStandardError (Join-Path $emuOutput 'adb-server.stderr.log')
    $emuServerReady = [Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        if ($emuServer.HasExited) { throw 'Private adb server exited during startup.' }
        $emuClient = [Net.Sockets.TcpClient]::new()
        try { $emuClient.Connect('127.0.0.1', 5039); break } catch { if ($emuServerReady.Elapsed.TotalSeconds -ge 5) { throw 'Private adb server startup timed out.' } } finally { $emuClient.Dispose() }
        Start-Sleep -Milliseconds 100
    }
    $emuArguments = @('-avd','EmuCoreReference35','-port','5580','-accel','on','-gpu','host','-memory','2048','-cores','2','-no-window','-no-audio','-no-snapshot','-no-boot-anim','-no-metrics','-camera-back','none','-camera-front','none')
    $emuRecord.arguments = $emuArguments
    $emuClock = [Diagnostics.Stopwatch]::StartNew()
    $emuProcess = Start-Process -FilePath $emuExecutable -ArgumentList $emuArguments -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $emuOutput 'emulator.stdout.log') -RedirectStandardError (Join-Path $emuOutput 'emulator.stderr.log')
    while ($emuClock.Elapsed.TotalSeconds -lt $BootTimeoutSeconds) {
        if ($emuProcess.HasExited) { throw "Emulator exited during boot with code $($emuProcess.ExitCode)." }
        $emuBoot = Invoke-EmuAdb @('shell','getprop','sys.boot_completed')
        if ($emuBoot.ExitCode -eq 0 -and $emuBoot.Stdout.Trim() -eq '1') { $emuRecord.boot_completed_ms = $emuClock.Elapsed.TotalMilliseconds; break }
        Start-Sleep -Milliseconds 500
    }
    if ($null -eq $emuRecord.boot_completed_ms) { throw "Android boot exceeded $BootTimeoutSeconds seconds." }
    $emuPackage = Invoke-EmuAdb @('shell','pm','path','android')
    if ($emuPackage.ExitCode -ne 0 -or $emuPackage.Stdout -notmatch '^package:') { throw 'Android package service did not answer the readiness probe.' }
    $emuRecord.package_service_ready_ms = $emuClock.Elapsed.TotalMilliseconds
    $emuHardwarePath = Join-Path $emuAvd 'hardware-qemu.ini'
    $emuHardware = [IO.File]::ReadAllText($emuHardwarePath)
    if ($emuHardware -notmatch '(?m)^hw\.ramSize\s*=\s*(\d+)') { throw 'Actual emulator RAM configuration was unavailable.' }
    $emuRecord.guest_memory_mb_configured = [int]$Matches[1]
    Copy-Item -LiteralPath $emuHardwarePath -Destination (Join-Path $emuOutput 'hardware-qemu.ini')
    foreach ($emuProperty in @('ro.build.fingerprint','ro.product.cpu.abilist','ro.dalvik.vm.native.bridge','ro.hardware.egl','ro.hardware.vulkan')) {
        $emuValue = Invoke-EmuAdb @('shell','getprop',$emuProperty)
        if ($emuValue.ExitCode -ne 0) { throw "Could not read $emuProperty" }
        $emuRecord[$emuProperty] = $emuValue.Stdout.Trim()
    }
    $emuMeminfo = Invoke-EmuAdb @('shell','cat','/proc/meminfo')
    if ($emuMeminfo.ExitCode -ne 0 -or $emuMeminfo.Stdout -notmatch '(?m)^MemTotal:\s+(\d+) kB') { throw 'Guest memory evidence was unavailable.' }
    $emuRecord.guest_memtotal_kib = [long]$Matches[1]
    [IO.File]::WriteAllText((Join-Path $emuOutput 'guest-meminfo.txt'), $emuMeminfo.Stdout)
    $emuHostProcesses = @(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId)
    $emuOwnedIds = [Collections.Generic.HashSet[int]]::new()
    $emuOwnedIds.Add($emuProcess.Id) | Out-Null
    do {
        $emuAdded = $false
        foreach ($emuCandidate in $emuHostProcesses) {
            if ($emuOwnedIds.Contains([int]$emuCandidate.ParentProcessId) -and $emuOwnedIds.Add([int]$emuCandidate.ProcessId)) { $emuAdded = $true }
        }
    } while ($emuAdded)
    $emuMemoryRows = @(foreach ($emuOwnedId in $emuOwnedIds) {
        $emuOwned = Get-Process -Id $emuOwnedId -ErrorAction SilentlyContinue
        if ($null -ne $emuOwned) { [pscustomobject]@{ pid=$emuOwnedId; name=$emuOwned.ProcessName; private_bytes=$emuOwned.PrivateMemorySize64; working_set_bytes=$emuOwned.WorkingSet64 } }
    })
    $emuRecord.process_tree_memory_snapshot = $emuMemoryRows
    $emuRecord.process_tree_private_bytes_snapshot = ($emuMemoryRows | Measure-Object -Property private_bytes -Sum).Sum
    $emuRecord.private_adb_server_bytes_snapshot = (Get-Process -Id $emuServer.Id).PrivateMemorySize64
    $emuGraphics = Invoke-EmuAdb @('shell','dumpsys','SurfaceFlinger')
    if ($emuGraphics.ExitCode -ne 0) { throw 'SurfaceFlinger diagnostics were unavailable.' }
    [IO.File]::WriteAllText((Join-Path $emuOutput 'surfaceflinger.txt'), $emuGraphics.Stdout + $emuGraphics.Stderr)
    $emuSettings = Invoke-EmuAdb -AdbArguments @('shell','am','start','-W','-a','android.settings.SETTINGS') -TimeoutMilliseconds 15000
    [IO.File]::WriteAllText((Join-Path $emuOutput 'settings-start.txt'), $emuSettings.Stdout + $emuSettings.Stderr)
    if ($emuSettings.ExitCode -ne 0 -or $emuSettings.Stdout -notmatch 'Status:\s+ok') { throw 'Settings launch did not succeed.' }
    $emuBeforeHome = Invoke-EmuAdb @('shell','dumpsys','activity','activities')
    [IO.File]::WriteAllText((Join-Path $emuOutput 'activity-before-home.txt'), $emuBeforeHome.Stdout)
    if ($emuBeforeHome.ExitCode -ne 0 -or $emuBeforeHome.Stdout -notmatch '(?m)(?:topResumedActivity|mResumedActivity)=.*com\.android\.settings') { throw 'Settings was not the resumed activity before input.' }
    $emuHome = Invoke-EmuAdb @('shell','input','keyevent','KEYCODE_HOME')
    if ($emuHome.ExitCode -ne 0) { throw 'Home input command failed.' }
    $emuInputClock = [Diagnostics.Stopwatch]::StartNew()
    do {
        $emuAfterHome = Invoke-EmuAdb @('shell','dumpsys','activity','activities')
        if ($emuAfterHome.ExitCode -eq 0 -and $emuAfterHome.Stdout -match '(?m)(?:topResumedActivity|mResumedActivity)=.*com\.google\.android\.apps\.nexuslauncher') { break }
        Start-Sleep -Milliseconds 100
    } while ($emuInputClock.Elapsed.TotalSeconds -lt 5)
    [IO.File]::WriteAllText((Join-Path $emuOutput 'activity-after-home.txt'), $emuAfterHome.Stdout)
    $emuRecord.home_input_verified = $emuAfterHome.ExitCode -eq 0 -and $emuAfterHome.Stdout -match '(?m)(?:topResumedActivity|mResumedActivity)=.*com\.google\.android\.apps\.nexuslauncher'
    if (-not $emuRecord.home_input_verified) { throw 'Home input did not return to the launcher within five seconds.' }
    # Resumed activity can precede its first visible frame; allow the transition to finish.
    Start-Sleep -Milliseconds 500
    if ($FixtureApk) {
        $emuRecord.fixture = [ordered]@{ apk=$FixtureApk; sha256=(Get-FileHash -LiteralPath $FixtureApk -Algorithm SHA256).Hash; expected_abi=$ExpectedFixtureAbi; native_ready_verified=$false; native_touch_verified=$false }
        $emuInstall = Invoke-EmuAdb -AdbArguments @('install','-r',$FixtureApk) -TimeoutMilliseconds 15000
        [IO.File]::WriteAllText((Join-Path $emuOutput 'fixture-install.txt'), $emuInstall.Stdout + $emuInstall.Stderr)
        if ($emuInstall.ExitCode -ne 0 -or $emuInstall.Stdout -notmatch 'Success') { throw 'Owned fixture installation failed.' }
        $emuClear = Invoke-EmuAdb @('logcat','-c')
        if ($emuClear.ExitCode -ne 0) { throw 'Could not clear the owned guest log before the fixture.' }
        $emuLaunch = Invoke-EmuAdb -AdbArguments @('shell','am','start','-W','-n','dev.emucore.probe/.ProbeActivity') -TimeoutMilliseconds 15000
        [IO.File]::WriteAllText((Join-Path $emuOutput 'fixture-launch.txt'), $emuLaunch.Stdout + $emuLaunch.Stderr)
        if ($emuLaunch.ExitCode -ne 0 -or $emuLaunch.Stdout -notmatch 'Status:\s+ok') { throw 'Owned fixture launch failed.' }
        $emuFixtureClock = [Diagnostics.Stopwatch]::StartNew()
        do {
            $emuFixtureLog = Invoke-EmuAdb @('logcat','-d','-v','brief','-s','EmuCoreProbe:I','*:S')
            if ($emuFixtureLog.Stdout.Contains("READY abi=$ExpectedFixtureAbi checksum=2206585887 touches=0")) { break }
            if ($emuFixtureLog.Stdout -match 'FAIL') { break }
            Start-Sleep -Milliseconds 100
        } while ($emuFixtureClock.Elapsed.TotalSeconds -lt 5)
        [IO.File]::WriteAllText((Join-Path $emuOutput 'fixture-ready.log'), $emuFixtureLog.Stdout + $emuFixtureLog.Stderr)
        $emuRecord.fixture.native_ready_verified = $emuFixtureLog.ExitCode -eq 0 -and $emuFixtureLog.Stdout.Contains("READY abi=$ExpectedFixtureAbi checksum=2206585887 touches=0")
        if (-not $emuRecord.fixture.native_ready_verified) { throw 'Owned fixture native startup/checksum verification failed.' }
        $emuUiDump = Invoke-EmuAdb -AdbArguments @('shell','uiautomator','dump','/data/local/tmp/emucore-ui.xml') -TimeoutMilliseconds 10000
        if ($emuUiDump.ExitCode -ne 0) { throw 'Owned fixture UI hierarchy capture failed.' }
        $emuUiText = Invoke-EmuAdb @('shell','cat','/data/local/tmp/emucore-ui.xml')
        [IO.File]::WriteAllText((Join-Path $emuOutput 'fixture-ui-before.xml'), $emuUiText.Stdout)
        [xml]$emuUi = $emuUiText.Stdout
        $emuButtons = $emuUi.SelectNodes('//node[@content-desc="Run native check"]')
        if ($emuButtons.Count -ne 1 -or $emuButtons[0].bounds -notmatch '^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$') { throw 'Could not identify the owned fixture button.' }
        $emuTapX = [int][Math]::Floor(([int]$Matches[1] + [int]$Matches[3]) / 2.0)
        $emuTapY = [int][Math]::Floor(([int]$Matches[2] + [int]$Matches[4]) / 2.0)
        if ($emuTapX -lt 0 -or $emuTapX -ge 720 -or $emuTapY -lt 0 -or $emuTapY -ge 1280) { throw 'Fixture button center is outside the expected display.' }
        $emuTap = Invoke-EmuAdb @('shell','input','tap',"$emuTapX","$emuTapY")
        if ($emuTap.ExitCode -ne 0) { throw 'Owned fixture touch injection failed.' }
        $emuFixtureClock.Restart()
        do {
            $emuFixtureLog = Invoke-EmuAdb @('logcat','-d','-v','brief','-s','EmuCoreProbe:I','*:S')
            if ($emuFixtureLog.Stdout.Contains("TOUCH abi=$ExpectedFixtureAbi checksum=2873771296 touches=1")) { break }
            if ($emuFixtureLog.Stdout -match 'FAIL') { break }
            Start-Sleep -Milliseconds 100
        } while ($emuFixtureClock.Elapsed.TotalSeconds -lt 5)
        [IO.File]::WriteAllText((Join-Path $emuOutput 'fixture-touch.log'), $emuFixtureLog.Stdout + $emuFixtureLog.Stderr)
        $emuRecord.fixture.native_touch_verified = $emuFixtureLog.ExitCode -eq 0 -and $emuFixtureLog.Stdout.Contains("TOUCH abi=$ExpectedFixtureAbi checksum=2873771296 touches=1")
        if (-not $emuRecord.fixture.native_touch_verified) { throw 'Owned fixture native touch/checksum verification failed.' }
        Start-Sleep -Milliseconds 500
    }
    $emuScreenshot = Invoke-EmuAdb @('shell','screencap','-p','/data/local/tmp/emucore-reference.png')
    if ($emuScreenshot.ExitCode -ne 0) { throw 'Guest screenshot capture failed.' }
    $emuPulled = Invoke-EmuAdb @('pull','/data/local/tmp/emucore-reference.png',(Join-Path $emuOutput 'screen.png'))
    if ($emuPulled.ExitCode -ne 0) { throw 'Guest screenshot transfer failed.' }
    $emuShutdown = Invoke-EmuAdb @('emu','kill')
    if ($emuShutdown.ExitCode -eq 0 -and $emuProcess.WaitForExit(10000)) { $emuRecord.graceful_shutdown = $true }
    else { throw 'Emulator did not finish graceful shutdown within ten seconds.' }
} catch {
    $emuRecord.failure = $_.ToString()
    throw
} finally {
    foreach ($emuOwnedProcess in @($emuProcess,$emuServer)) {
        if ($null -ne $emuOwnedProcess) {
            try { if (-not $emuOwnedProcess.HasExited) { $emuOwnedProcess.Kill($true); $emuOwnedProcess.WaitForExit(5000) | Out-Null } }
            catch { $emuRecord.cleanup_errors += $_.ToString() }
            finally { $emuOwnedProcess.Dispose() }
        }
    }
    foreach ($emuName in $emuSaved.Keys) { [Environment]::SetEnvironmentVariable($emuName, $emuSaved[$emuName], 'Process') }
    [IO.File]::WriteAllText((Join-Path $emuOutput 'report.json'), ($emuRecord | ConvertTo-Json -Depth 5))
}
if ($emuRecord.cleanup_errors.Count -gt 0) { throw 'Reference process cleanup reported errors; inspect report.json.' }
Write-Output "Android reference probe saved to $emuOutput"
