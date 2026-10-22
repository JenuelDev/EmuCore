param([ValidateSet('compiled-guest','mapped-guest')][string]$Guest = 'compiled-guest')

$ErrorActionPreference = 'Stop'
$emuRoot = Split-Path -Parent $PSScriptRoot
$emuOldCargoHome = $env:CARGO_HOME
$emuOldRustupHome = $env:RUSTUP_HOME
$emuOldPath = $env:PATH
Push-Location -LiteralPath $emuRoot
try {
    $emuLocalBin = Join-Path $emuRoot '.tools\cargo\bin'
    if (Test-Path -LiteralPath (Join-Path $emuLocalBin 'rustc.exe')) {
        $env:CARGO_HOME = Join-Path $emuRoot '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $emuRoot '.tools\rustup'
        $env:PATH = $emuLocalBin + [IO.Path]::PathSeparator + $env:PATH
    }
    $emuSource = "fixtures/$Guest.rs"
    & rustfmt.exe --check --edition=2024 $emuSource
    if ($LASTEXITCODE -ne 0) { throw 'Compiled guest source formatting check failed' }
    New-Item -ItemType Directory -Path artifacts -Force | Out-Null
    $emuOutputs = "--emit=link=artifacts/$Guest.elf,asm=artifacts/$Guest.s"
    & rustc.exe +1.99.1 --edition=2024 --target x86_64-unknown-none -D warnings -C panic=abort -C opt-level=2 -C relocation-model=static -C code-model=small -C link-arg=-e_start -C link-arg=--build-id=none -C link-arg=-z -C link-arg=norelro $emuOutputs $emuSource
    $emuResult = $LASTEXITCODE
} finally {
    $env:CARGO_HOME = $emuOldCargoHome
    $env:RUSTUP_HOME = $emuOldRustupHome
    $env:PATH = $emuOldPath
    Pop-Location
}
exit $emuResult
