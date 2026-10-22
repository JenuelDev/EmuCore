#Requires -Version 7.0
param(
    [string]$SdkRoot = (Join-Path $env:LOCALAPPDATA 'Android/Sdk'),
    [string]$OutputDirectory = (Join-Path (Split-Path $PSScriptRoot -Parent) 'artifacts/android-fixture')
)
$ErrorActionPreference = 'Stop'
$emuRoot = Split-Path $PSScriptRoot -Parent
$emuSource = Join-Path $emuRoot 'fixtures/android'
$emuBuildTools = Join-Path $SdkRoot 'build-tools/35.0.0'
$emuPlatform = Join-Path $SdkRoot 'platforms/android-35/android.jar'
$emuNdk = Join-Path $SdkRoot 'ndk/28.2.13676358'
$emuToolchain = Join-Path $emuNdk 'toolchains/llvm/prebuilt/windows-x86_64'
$emuClang = Join-Path $emuToolchain 'bin/clang.exe'
foreach ($emuRequired in @($emuPlatform,$emuClang,(Join-Path $emuBuildTools 'aapt2.exe'),(Join-Path $emuBuildTools 'lib/d8.jar'),(Join-Path $emuBuildTools 'lib/apksigner.jar'))) {
    if (-not (Test-Path -LiteralPath $emuRequired -PathType Leaf)) { throw "Missing pinned Android build prerequisite: $emuRequired" }
}
$emuJava = (Get-Command java -CommandType Application -ErrorAction Stop).Source
$emuJavac = (Get-Command javac -CommandType Application -ErrorAction Stop).Source
$emuKeytool = (Get-Command keytool -CommandType Application -ErrorAction Stop).Source
$emuOutput = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $emuOutput) { throw 'Use a new fixture output directory; existing builds are preserved.' }
New-Item -ItemType Directory -Path $emuOutput | Out-Null
$emuClasses = Join-Path $emuOutput 'classes'
$emuDex = Join-Path $emuOutput 'dex'
New-Item -ItemType Directory -Path $emuClasses,$emuDex | Out-Null
& $emuJavac --release 8 -Xlint:all -Werror -classpath $emuPlatform -d $emuClasses (Join-Path $emuSource 'ProbeActivity.java')
if ($LASTEXITCODE -ne 0) { throw 'Owned Java fixture compilation failed.' }
$emuClassFiles = @(Get-ChildItem -LiteralPath $emuClasses -Recurse -Filter '*.class' | Select-Object -ExpandProperty FullName)
& $emuJava -cp (Join-Path $emuBuildTools 'lib/d8.jar') com.android.tools.r8.D8 --lib $emuPlatform --min-api 29 --output $emuDex @emuClassFiles
if ($LASTEXITCODE -ne 0) { throw 'Owned fixture DEX compilation failed.' }
$emuKey = Join-Path $emuRoot '.tools/android/fixture-key.p12'
if (-not (Test-Path -LiteralPath $emuKey)) {
    New-Item -ItemType Directory -Path (Split-Path $emuKey -Parent) -Force | Out-Null
    # This fixture uses the conventional public debug password, not production signing.
    & $emuKeytool -genkeypair -keystore $emuKey -storetype PKCS12 -storepass android -keypass android -alias emucore-probe -dname 'CN=EmuCore Owned Fixture' -keyalg RSA -keysize 2048 -validity 3650 -noprompt
    if ($LASTEXITCODE -ne 0) { throw 'Owned debug fixture key generation failed.' }
}
$emuBuild = [ordered]@{
    schema_version = 1
    built_utc = [DateTime]::UtcNow.ToString('o')
    build_tools = '35.0.0'
    platform = 'android-35'
    ndk = '28.2.13676358'
    minimum_api = 29
    java_version = (& $emuJava -version 2>&1 | Out-String).Trim()
    source_hashes = @(Get-ChildItem -LiteralPath $emuSource -File | ForEach-Object { [ordered]@{ name=$_.Name; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash } })
    apks = @()
}
foreach ($emuAbi in @('x86_64','arm64-v8a')) {
    $emuAbiDirectory = Join-Path $emuOutput $emuAbi
    New-Item -ItemType Directory -Path $emuAbiDirectory | Out-Null
    $emuTriple = $(if ($emuAbi -eq 'x86_64') { 'x86_64-linux-android29' } else { 'aarch64-linux-android29' })
    $emuLibrary = Join-Path $emuAbiDirectory 'libemucore_probe.so'
    & $emuClang "--target=$emuTriple" "--sysroot=$(Join-Path $emuToolchain 'sysroot')" -shared -fPIC -O2 -Wall -Wextra -Werror -fstack-protector-strong '-Wl,-z,max-page-size=16384' '-Wl,-z,relro,-z,now' '-Wl,-soname,libemucore_probe.so' -o $emuLibrary (Join-Path $emuSource 'native-probe.c')
    if ($LASTEXITCODE -ne 0) { throw "Owned $emuAbi JNI compilation failed." }
    $emuBase = Join-Path $emuAbiDirectory 'base.apk'
    & (Join-Path $emuBuildTools 'aapt2.exe') link --manifest (Join-Path $emuSource 'AndroidManifest.xml') -I $emuPlatform --min-sdk-version 29 --target-sdk-version 35 --version-code 1 --version-name 0.1 -o $emuBase
    if ($LASTEXITCODE -ne 0) { throw 'Owned fixture manifest linking failed.' }
    $emuArchive = [IO.Compression.ZipFile]::Open($emuBase, [IO.Compression.ZipArchiveMode]::Update)
    try {
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($emuArchive, (Join-Path $emuDex 'classes.dex'), 'classes.dex', [IO.Compression.CompressionLevel]::Optimal) | Out-Null
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($emuArchive, $emuLibrary, "lib/$emuAbi/libemucore_probe.so", [IO.Compression.CompressionLevel]::NoCompression) | Out-Null
    } finally { $emuArchive.Dispose() }
    $emuAligned = Join-Path $emuAbiDirectory 'aligned.apk'
    & (Join-Path $emuBuildTools 'zipalign.exe') -P 16 4 $emuBase $emuAligned
    if ($LASTEXITCODE -ne 0) { throw 'Owned fixture ZIP alignment failed.' }
    $emuApk = Join-Path $emuOutput "probe-$emuAbi.apk"
    & $emuJava -jar (Join-Path $emuBuildTools 'lib/apksigner.jar') sign --ks $emuKey --ks-key-alias emucore-probe --ks-pass pass:android --key-pass pass:android --out $emuApk $emuAligned
    if ($LASTEXITCODE -ne 0) { throw 'Owned fixture signing failed.' }
    & $emuJava -jar (Join-Path $emuBuildTools 'lib/apksigner.jar') verify $emuApk
    if ($LASTEXITCODE -ne 0) { throw 'Owned fixture signature verification failed.' }
    & (Join-Path $emuBuildTools 'zipalign.exe') -c -P 16 4 $emuApk
    if ($LASTEXITCODE -ne 0) { throw 'Signed fixture alignment verification failed.' }
    $emuBuild.apks += [ordered]@{ abi=$emuAbi; path=$emuApk; sha256=(Get-FileHash -LiteralPath $emuApk -Algorithm SHA256).Hash; library_sha256=(Get-FileHash -LiteralPath $emuLibrary -Algorithm SHA256).Hash }
}
[IO.File]::WriteAllText((Join-Path $emuOutput 'build-report.json'), ($emuBuild | ConvertTo-Json -Depth 6))
Write-Output "Built owned x86_64 and ARM64 Android probes in $emuOutput"
