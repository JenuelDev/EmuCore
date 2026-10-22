# Rust external Android controller

Updated: 2026-10-22. P11 adds an explicit Rust lifecycle controller around the installed SDK reference. Android still runs in the external SDK engine, not EmuCore's direct ELF translator. The controller is a foundation for the on-demand game-library client.

## Run the owned reference

Prepare the device and build APKs using the [Android reference](android-reference.md) and [owned JNI fixture](android-native-fixture.md) procedures. Then run from the project root:

```powershell
$emuSdk = Join-Path $env:LOCALAPPDATA 'Android/Sdk'
.\scripts\cargo.ps1 -CargoArgs @('run', '--locked', '--release', '--example', 'android_reference', '--', $emuSdk, 'artifacts/rust-reference-new', 'x86_64')
```

Replace x86_64 with arm64-v8a for the ARM64-only fixture. Use a new capture directory with an existing parent inside the workspace. These examples execute only when explicitly invoked. Building, importing the library, constructing ReferenceConfig, and running the ordinary check workflow do not start Android.

The live cancellation probe starts the owned emulator, allows two seconds of boot activity, then cancels before waiting for readiness:

```powershell
.\scripts\cargo.ps1 -CargoArgs @('run', '--locked', '--release', '--example', 'android_cancel', '--', $emuSdk, 'artifacts/rust-cancel-new')
```

No SDK packages are installed, no license is accepted, and no Windows feature is changed. The controller requires the existing project ownership marker and installed image revision 9. It uses fixed private ports 5039/5580/5581 and the documented reference profile. Other SDK images and production game profiles are not yet configurable through this implementation.

## Lifecycle and ownership

ReferenceConfig construction is inert. ReferenceSession::start validates paths, refuses an existing capture directory, reserves private ports, checks WHPX, starts its own adb server, and launches the external emulator. SDK environment overrides apply only to child commands. Writable state and resolved output locations must remain inside the workspace. Reference tools use explicit SDK paths and Rust Command arguments, without constructing shell text.

Both long-lived processes use the [Windows job owner](host-processes.md). Short-lived SDK commands get their own jobs too. File-backed stdout/stderr avoid pipe deadlock, preserve diagnostic output, and are read with a 256 KiB limit after command completion. This read limit is not a disk quota. Events record elapsed time, lifecycle state, and command/log identifiers.

Readiness observes sys.boot_completed, then the Android package service. The controller captures actual guest RAM, ABI/bridge properties, build fingerprint, renderer, and Windows job memory peaks. It reports the emulator's effective RAM, including an increase over the requested setting. These events do not mean all background Android initialization is complete.

States are Starting, AndroidReady, LaunchingApplication, ApplicationRunning, Stopping, Stopped, Cancelled, and Failed. Activity launch requires ready/running state and a restricted validated package/activity component. A repeated launch is skipped only if the requested component is still observed as resumed; a previously launched app is not permanently assumed to be running. Activity acknowledgement is not a game-ready or first-visible-frame signal.

CancellationToken can be cloned and retained by a client thread. Command waiting checks cancellation at intervals up to approximately 100 ms, while boot polling and fixed transition waits can add up to approximately 500 ms before another check. OS startup and cleanup can add time. Cancellation is a distinct error/state, and setup/readiness/fixture failures release owned jobs. cancel releases them immediately when called directly. Dropping a session also releases both process trees. A frontend must perform blocking lifecycle calls on a worker, not its UI thread.

Normal stop requests emulator shutdown, waits up to ten seconds, captures final job-memory peaks, and releases both jobs. Repeated successful stop calls return the saved result. Forced cleanup is retained for timeouts, cancellation, or failure. The existing five-second command timeout applies to ordinary commands, with 15 seconds for app launch/install and ten for UI hierarchy capture. A command already in flight can extend a higher-level observation deadline by its bounded duration.

## Owned fixture validation

The fixture API checks package/ABI metadata using pinned aapt2, installs only the owned probe package, and validates exact native readiness and touch log messages. It discovers the button from the known fixture hierarchy, validates unique bounds against the expected 720x1280 display, injects a touch, then saves a settled screenshot. This restricted fixture reader is not a general XML/UI automation parser. The earlier PowerShell probe independently checks each packaged ELF header.

The expected native results remain 2,206,585,887 at startup and 2,873,771,296 after one touch. Standard game launch will use package/activity acknowledgement; it must not substitute fixture checks for actual target-game gameplay tests.

## Evidence and validation

The full check workflow passes 90 tests in debug and release, formatting, strict Clippy, both compiled static-guest comparisons, PowerShell syntax, and documentation checks. Eight lifecycle tests cover component restrictions, exact resumed-activity matching, bounded/unique fixture coordinates, property parsing, inert validation and preserved/outside captures, output limits, cancellation before any SDK access, and cancellation of an actually running owned command. These tests require no Android installation or boot.

Selected live evidence is retained under [Rust reference captures](measurements/rust-reference-2026-10-22), with result/event files, screenshots, and source/executable identifiers. Full command logs remain in ignored artifacts directories.

| Final run | Package readiness | Fixture | Shutdown |
| --- | ---: | --- | --- |
| x86-64 | 22.06 s | Native startup and touch checks passed | Graceful |
| ARM64-only | 21.17 s | Native startup and touch checks passed | Graceful |
| Cancellation during boot | Intentionally cancelled | Not launched | Owned jobs released, Cancelled state returned |

Both successful screenshots were visually inspected, and post-run inspection found no listeners on the private ports. A first x86 run also passed before the final cancellation changes; the table uses the final runs. The cancellation-during-boot probe observed approximately 3.2 billion bytes of job peak commit before releasing the engine, so it exercised cancellation after substantial engine initialization.

Final emulator-job memory peaks were 4,140,888,064 bytes for x86-64 and 4,046,528,512 bytes for ARM64. These include engine descendants and the measured fixture/diagnostic interval, exclude the separate private adb-server job, and are commit-accounting peaks, not resident physical-memory measurements. They are not directly comparable to the earlier point-in-time private-memory snapshots. No low-end, game-performance, or optimized-startup claim follows from these two runs.

Next: persist the client library and runtime profiles, then build a lightweight desktop frontend. Loading that library must remain independent from ReferenceSession::start; selecting a game is the explicit startup trigger. Installation/compatibility evidence for PUBG Mobile and Mobile Legends remains outstanding.
