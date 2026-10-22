# External Android reference

Updated: 2026-10-22. This experiment uses the installed Android Emulator as a reference. Android is not running on EmuCore's direct-execution runtime. No engine or system-image binary is bundled with EmuCore.

## Reproduce

Prerequisites: Windows with usable WHPX, PowerShell 7, Android Emulator, adb, avdmanager with Java 17, and the installed Android 35 Google Play x86_64 system image revision 9. Current observed emulator: 36.6.11, build 15507667. Preparation does not install packages or accept licenses.

```powershell
.\scripts\prepare-android-reference.ps1
.\scripts\probe-android-reference.ps1 -OutputDirectory artifacts/android-reference-new
```

An explicit SDK location can be passed with `-SdkRoot`. The default is the current user's LocalAppData Android/Sdk directory. The probe uses a fresh capture directory and refuses to overwrite one. Preparation creates only the project device named EmuCoreReference35 under .tools/android/avd. A marker identifies the owned device; an existing unmarked device is not changed. Reusing a prepared device preserves its userdata, while every probe disables snapshot loading and saving.

The probe requests two virtual CPUs, 2,048 MB RAM, a 720x1280 display, host graphics, and WHPX. It runs hidden/headless, disables cameras/audio/metrics, uses emulator ports 5580/5581, and starts its own foreground adb server on loopback port 5039. Occupied ports cause a preflight failure. Guest commands target only emulator-5580 through that server. Environment overrides are scoped to the probe and restored afterward. Existing user AVDs are not launched or modified.

Boot completion is the first successful poll of sys.boot_completed=1. Package readiness is a successful `pm path android` response. These are readiness proxies, not proof that all Android background work has stopped. Polling adds observation latency. The probe captures build/ABI/bridge properties, actual emulator hardware settings, guest memory, SurfaceFlinger diagnostics, a process-memory snapshot, a Settings-to-Home input round trip, and a screenshot after a short transition delay.

Most adb commands have a five-second timeout. Activity launches and optional fixture installation get 15 seconds, and UI hierarchy capture gets ten. Boot defaults to 120 seconds, configurable from 30 to 300; a command already in flight may extend that boundary by its command timeout. Graceful shutdown gets ten seconds. Finally blocks terminate the owned emulator/server process trees on failure and retain a report plus available logs. Cleanup errors are recorded and reported. The probe is a development tool, not a security sandbox or a production lifecycle implementation.

## Observed results

The [raw report directory](measurements/android-reference-2026-10-22) preserves all four attempts. Full logs remain in the ignored artifacts/android-reference-first through fourth directories on this workstation.

| Attempt | Result | Boot observed | Package readiness |
| --- | --- | ---: | ---: |
| First | Harness failure before boot: installed adb rejected the original listener syntax | Not reached | Not reached |
| Second | Android boot, renderer diagnostics, screenshot, graceful shutdown | 45.58 s | 45.65 s |
| Third | Boot plus verified Settings-to-Home activity transition and graceful shutdown | 18.64 s | 18.72 s |
| Fourth | Repeated input verification, effective RAM capture, settled launcher screenshot, graceful shutdown | 22.72 s | See raw report |

The first failure was corrected by using the adb version's supported loopback listener syntax. The second attempt was the device's first successful boot. Later attempts reused initialized userdata with snapshots still disabled. These results are not interchangeable fresh-device or cold-host measurements and do not establish an optimization improvement.

Observed platform: Android 15 build AE3A.240806.036, x86_64 system image revision 9, two virtual CPUs. WHPX reports installed and usable, and the emulator log confirms its accelerator is operational. SurfaceFlinger identifies the NVIDIA RTX 4080 Laptop GPU through Android Emulator's OpenGL ES translator, reporting GLES 3.1. This establishes that the reference initialized a host GPU path on the development laptop; it does not establish integrated-GPU correctness or performance.

The guest advertises x86_64 and arm64-v8a and names libndk_translation.so as its native bridge. These properties do not prove that either game or an arbitrary ARM library works. The next owned fixture will test native code in both ABIs explicitly. The observed translator is an external system-image component, not an EmuCore implementation or an approved distribution dependency.

The emulator increased requested RAM from 2,048 to 2,560 MB. The guest reported MemTotal=2,532,904 KiB after reserved memory. Process-tree private-memory snapshots were 3,947,794,432 bytes on attempt three and 3,904,917,504 bytes on attempt four, approximately 3.68 and 3.64 GiB. The private adb server added roughly 5 MB separately. These are snapshots after boot, not peaks or steady-idle averages. GPU/system/shared memory and Windows pressure require further capture. Do not add guest RAM to process private bytes, because the latter already includes the process's guest backing.

The process tree includes the launched emulator and descendants present during the snapshot. Summed private bytes are not resident physical memory; summed working sets can double-count shared pages. No low-end PC test or sustained system-memory-pressure measurement exists. This reference's footprint is a concrete concern for the proposed 8 GB tier, rather than evidence that it meets that target.

Home input was verified by observing Settings as resumed, sending KEYCODE_HOME, then observing Nexus Launcher resumed. The [final screenshot](measurements/android-reference-2026-10-22/screen.png) was inspected and shows the launcher. This is a functional adb input check, not keyboard/mouse mapping or an input-latency measurement. Audio, game installation, networking behavior, and playable sessions remain untested. Probes shut down gracefully; subsequent inspection found no reference emulator process or listening socket on the owned ports. TIME_WAIT TCP entries are expected after shutdown.

## Next work

The [owned JNI fixture](android-native-fixture.md) now passes startup and touch computations separately for x86_64 and arm64-v8a on this reference. Next, implement a Rust host lifecycle controller with explicit external-engine configuration and bounded cleanup. See the [engine comparison](engine-comparison.md) for the product-path investigation.
