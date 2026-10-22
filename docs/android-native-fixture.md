# Owned Android native-code fixture

Updated: 2026-10-22. The owned fixture tests JNI execution and touch input on the [external Android reference](android-reference.md). It does not run on EmuCore's direct-execution runtime or establish target-game compatibility.

## Build and run

Prerequisites: the reference SDK/image, Android Build Tools 35.0.0, platform android-35, NDK 28.2.13676358, Java 17 tools, and PowerShell 7. These versions are already installed on the development PC. The script does not download dependencies or accept licenses.

```powershell
.\scripts\build-android-fixture.ps1
.\scripts\probe-android-reference.ps1 -OutputDirectory artifacts/probe-x86-new -FixtureApk artifacts/android-fixture/probe-x86_64.apk -ExpectedFixtureAbi x86_64
.\scripts\probe-android-reference.ps1 -OutputDirectory artifacts/probe-arm-new -FixtureApk artifacts/android-fixture/probe-arm64-v8a.apk -ExpectedFixtureAbi arm64-v8a
```

Run probes sequentially because they use the same owned device and private ports. Use fresh output directories. To rebuild, supply `build-android-fixture.ps1 -OutputDirectory artifacts/android-fixture-new`. Existing builds and captures are preserved. APK replacement is limited to the fresh project reference. It does not install onto other adb devices. Both variants use package dev.emucore.probe and the same local debug signing key, so each replaces the previous variant on that device.

Source consists of [the manifest](../fixtures/android/AndroidManifest.xml), [the Java activity](../fixtures/android/ProbeActivity.java), and [the JNI library](../fixtures/android/native-probe.c). Java provides a small Android UI; C provides the minimal ABI-specific native function. The host/runtime direction remains Rust. No external app, network permission, account, storage permission, or game file is required.

The build script compiles Java with warnings denied, emits DEX, compiles each JNI ABI with warnings denied, links a manifest, packages exactly one ABI library, aligns, signs, then verifies the signature and alignment. Native ELF segments and uncompressed native ZIP entries use 16 KiB alignment. This build choice is not a test on a 16 KiB-page guest. See [Android ABI packaging](https://developer.android.com/ndk/guides/abis), [native page alignment](https://developer.android.com/guide/practices/page-sizes), and [apksigner ordering](https://developer.android.com/tools/apksigner).

The debug key stays in ignored .tools/android/fixture-key.p12 and uses the conventional public debug password. It is only for owned fixtures, with no production signing role. APKs, compiled libraries, and intermediate build files stay under artifacts. The build report records tool versions, source hashes, APK hashes, and library hashes. Rebuilding with different signing keys or archive timestamps can change APK bytes, so use the recorded hashes to identify a tested artifact instead of assuming bit-for-bit reproducibility.

## What is checked

Each APK includes exactly one libemucore_probe.so under its ABI directory. Before launching a device, the probe opens the ZIP without extraction and checks the library's ELF64/little-endian/shared-object header and actual machine code identifier: 62 for x86-64 or 183 for AArch64. An advertised ABI mismatch is rejected before output creation or device launch.

The activity loads that library through System.loadLibrary. A native function reports its compile-time architecture. A second function runs 64 iterations of a 32-bit linear-congruential calculation using unsigned C arithmetic. Java independently computes the same values with masked 64-bit arithmetic and checks the JNI result.

At startup, seed 0x13579bdf must produce 2,206,585,887. A UI button increments the input seed, calls JNI again, and must produce 2,873,771,296 with touches=1. The probe verifies exact readiness and touch log messages. It discovers the real button bounds from the guest UI hierarchy, taps its center, checks the second native result, then captures a settled screenshot. Merely installing or opening an app is insufficient for this check.

Most adb operations retain a five-second command timeout. Activity launches and installation get 15 seconds; UI hierarchy capture gets ten. The probe preserves logs and failures and cleans up its owned process trees. No snapshot is loaded or saved. This is a functional compatibility test, not a frame-rate, graphics workload, startup comparison, or input-latency benchmark.

## Observed results

Raw evidence is retained in [the native fixture capture directory](measurements/android-native-2026-10-22), including the build report, failed attempt, successful reports, exact touch logs, and screenshots. Full workstation captures remain under artifacts/android-native-*.

| Probe | Result |
| --- | --- |
| First x86-64 attempt | Android booted, then Settings activity launch exceeded the original five-second harness allowance before fixture installation. Failure and fallback cleanup were retained. |
| Second x86-64 attempt | Startup checksum passed, native ABI reported x86_64, UI touch checksum passed, screenshot inspected, graceful shutdown succeeded. |
| First ARM64 attempt | ARM64-only APK installed; startup checksum passed, native ABI reported arm64-v8a, UI touch checksum passed, screenshot inspected, graceful shutdown succeeded. |
| Deliberate APK/ABI mismatch | Rejected before device launch or capture-directory creation. |

The successful screenshots show the [x86-64 result](measurements/android-native-2026-10-22/x86-64-screen.png) and [ARM64 result](measurements/android-native-2026-10-22/arm64-screen.png), each with the expected checksum, checks passed, and one touch. Subsequent port inspection found no listening reference sockets. The activity timeout was increased specifically for bounded cold-launch behavior, not to omit the failed attempt.

Observed APK hashes:

- x86_64: EE2B7C125692F49B49D81449849B41471B03B27D2FC251E791C4C50FD34DD95F.
- arm64-v8a: 48F31D52B7F942E9A333F8D7C68B46EBF3C34548A66599CB2016014D590C2B5C.

The reference runs an x86_64 Android system image that advertises arm64-v8a and names libndk_translation.so. Executing this verified ARM64-only library provides concrete evidence that this owned JNI workload works through that external image's cross-architecture path. It does not validate arbitrary ARM64 libraries, 32-bit ARM, either target game, high-performance rendering, or permission to redistribute the bridge. No ARM translator was implemented or imported into EmuCore.

Next: implement the Rust external-engine lifecycle controller with reliable descendant cleanup and resource accounting, then use owned Android workloads to measure representative behavior before selecting a product foundation.
