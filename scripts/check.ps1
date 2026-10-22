$ErrorActionPreference = 'Stop'
$emuCargo = Join-Path $PSScriptRoot 'cargo.ps1'
& (Join-Path $PSScriptRoot 'check-scripts.ps1')

& $emuCargo -CargoArgs @('fmt', '--all', '--', '--check')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $emuCargo -CargoArgs @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $emuCargo -CargoArgs @('test', '--locked', '--all-targets')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $emuCargo -CargoArgs @('test', '--locked', '--release', '--all-targets')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $emuCargo -CargoArgs @('build', '--locked', '--release')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& (Join-Path $PSScriptRoot 'build-guest.ps1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$emuGuestOutput = & $emuCargo -CargoArgs @('run', '--locked', '--release', '--example', 'compare', '--', 'artifacts/compiled-guest.elf')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($emuGuestOutput -ne 'Compiled Rust guest executed successfully!') { throw 'Compiled guest output did not match' }
Write-Output 'Compiled guest native/reference comparison passed.'
& (Join-Path $PSScriptRoot 'build-guest.ps1') -Guest mapped-guest
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$emuMappedOutput = & $emuCargo -CargoArgs @('run', '--locked', '--release', '--example', 'compare', '--', 'artifacts/mapped-guest.elf')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($emuMappedOutput -ne 'Guest mmap works!') { throw 'Mapped guest output did not match' }
Write-Output 'Mapped guest native/reference comparison passed.'
& (Join-Path $PSScriptRoot 'check-docs.ps1')
