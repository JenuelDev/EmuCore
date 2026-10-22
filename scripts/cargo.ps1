param([Parameter(ValueFromRemainingArguments = $true)][string[]]$CargoArgs)

$ErrorActionPreference = 'Stop'
$emuProjectRoot = Split-Path -Parent $PSScriptRoot
$emuLocalCargo = Join-Path $emuProjectRoot '.tools\cargo\bin\cargo.exe'
$emuOldCargoHome = $env:CARGO_HOME
$emuOldRustupHome = $env:RUSTUP_HOME
$emuOldPath = $env:PATH
Push-Location -LiteralPath $emuProjectRoot
try {
    if (Test-Path -LiteralPath $emuLocalCargo) {
        $env:CARGO_HOME = Join-Path $emuProjectRoot '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $emuProjectRoot '.tools\rustup'
        $env:PATH = (Split-Path -Parent $emuLocalCargo) + [IO.Path]::PathSeparator + $env:PATH
        & $emuLocalCargo @CargoArgs
    } else {
        & cargo.exe @CargoArgs
    }
    $emuCargoExitCode = $LASTEXITCODE
} finally {
    $env:CARGO_HOME = $emuOldCargoHome
    $env:RUSTUP_HOME = $emuOldRustupHome
    $env:PATH = $emuOldPath
    Pop-Location
}
exit $emuCargoExitCode
