#Requires -Version 7.0
param([string]$SdkRoot = (Join-Path $env:LOCALAPPDATA 'Android/Sdk'))
$ErrorActionPreference = 'Stop'
$emuRoot = Split-Path $PSScriptRoot -Parent
$emuAndroid = Join-Path $emuRoot '.tools/android'
$emuAvdHome = Join-Path $emuAndroid 'avd'
$emuAvd = Join-Path $emuAvdHome 'EmuCoreReference35.avd'
$emuMarker = Join-Path $emuAvd '.emucore-reference'
$emuManager = Join-Path $SdkRoot 'cmdline-tools/latest/bin/avdmanager.bat'
$emuImage = Join-Path $SdkRoot 'system-images/android-35/google_apis_playstore/x86_64/source.properties'
if (-not (Test-Path -LiteralPath $emuImage -PathType Leaf)) { throw 'The reference requires the already installed Android 35 Google Play x86_64 image, revision 9.' }
if (-not ((Get-Content -LiteralPath $emuImage) -contains 'Pkg.Revision=9')) { throw 'The reference image revision differs from the recorded revision 9.' }
if (Test-Path -LiteralPath $emuMarker) {
    if ([IO.File]::ReadAllText($emuMarker) -ne "EmuCore Android reference v1`n") { throw 'Unexpected reference ownership marker.' }
    Write-Output "Reusing project reference: $emuAvd"
    return
}
if ((Test-Path -LiteralPath $emuAvd) -or (Test-Path -LiteralPath (Join-Path $emuAvdHome 'EmuCoreReference35.ini'))) {
    throw 'An unmarked device exists at the project reference path; refusing to change it.'
}
if (-not (Test-Path -LiteralPath $emuManager -PathType Leaf)) { throw 'Installed SDK avdmanager is required.' }
New-Item -ItemType Directory -Path $emuAvdHome,(Join-Path $emuAndroid 'user') -Force | Out-Null
$emuSavedAvd = $env:ANDROID_AVD_HOME
$emuSavedUser = $env:ANDROID_USER_HOME
try {
    $env:ANDROID_AVD_HOME = $emuAvdHome
    $env:ANDROID_USER_HOME = Join-Path $emuAndroid 'user'
    $emuLog = 'no' | & $emuManager create avd --name EmuCoreReference35 --package 'system-images;android-35;google_apis_playstore;x86_64' --path $emuAvd --device pixel_2 2>&1
    $emuExit = $LASTEXITCODE
    [IO.File]::WriteAllLines((Join-Path $emuAndroid 'prepare.log'), [string[]]$emuLog)
    if ($emuExit -ne 0) { throw "avdmanager failed with exit $emuExit; see .tools/android/prepare.log" }
} finally {
    $env:ANDROID_AVD_HOME = $emuSavedAvd
    $env:ANDROID_USER_HOME = $emuSavedUser
}
$emuConfiguration = Join-Path $emuAvd 'config.ini'
$emuSettings = [ordered]@{
    'hw.lcd.width' = '720'
    'hw.lcd.height' = '1280'
    'hw.lcd.density' = '320'
    'hw.cpu.ncore' = '2'
    'hw.ramSize' = '2048'
    'disk.dataPartition.size' = '2G'
    'hw.camera.back' = 'none'
    'hw.camera.front' = 'none'
    'hw.sdCard' = 'no'
    'hw.gpu.enabled' = 'yes'
    'hw.gpu.mode' = 'host'
    'fastboot.forceColdBoot' = 'yes'
    'fastboot.forceFastBoot' = 'no'
}
$emuText = [IO.File]::ReadAllText($emuConfiguration)
foreach ($emuKey in $emuSettings.Keys) {
    $emuLine = "$emuKey=$($emuSettings[$emuKey])"
    $emuPattern = '(?m)^' + [regex]::Escape($emuKey) + '=.*$'
    if ([regex]::IsMatch($emuText, $emuPattern)) { $emuText = [regex]::Replace($emuText, $emuPattern, $emuLine) }
    else { $emuText += "`n$emuLine`n" }
}
[IO.File]::WriteAllText($emuConfiguration, $emuText)
[IO.File]::WriteAllText($emuMarker, "EmuCore Android reference v1`n")
Write-Output "Prepared project reference: $emuAvd"
