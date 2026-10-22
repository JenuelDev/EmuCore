# Desktop game library prototype

P12 is in progress. The Windows Rust client has a persisted library, a provisional Win32 view, and a background worker around the external SDK reference. Opening or reloading the library does not inspect the SDK, create an engine worker, or boot Android. Select a row and choose **Launch selected**, or double-click the row. Ordinary selection and keyboard browsing do not launch anything.

This is a development frontend. The direct ELF runtime does not run Android. PUBG Mobile, Mobile Legends, low-end gameplay, and engine redistribution remain unverified. The optional icon path is persisted, but custom icon rendering is still pending. The Win32 view is a small integration prototype; the final UI toolkit decision still requires low-end measurements and interaction review.

## Build and try

```powershell
.\scripts\cargo.ps1 -CargoArgs @('build', '--locked', '--release', '--bin', 'emucore-client')
.\target\release\emucore-client.exe
```

The default library is `.tools/client/library.json`, under the development workspace compiled into the executable. An optional first argument selects a different JSON library. A missing file displays an empty library without creating it. Malformed or unsupported metadata is preserved and reported through `.tools/client/startup-error.txt`; it is never silently replaced. This workspace location is for development, not a final installer/storage policy.

The explicit preparation command verifies the owned x86-64 fixture on the prepared reference AVD, stops Android, then writes a new library with installation evidence:

```powershell
.\scripts\build-android-fixture.ps1
.\scripts\prepare-android-reference.ps1
.\scripts\cargo.ps1 -CargoArgs @('run', '--locked', '--release', '--example', 'client_fixture', '--', 'C:\Users\me\AppData\Local\Android\Sdk', '.tools/client/library.json')
```

Preparation is an explicit device action and boots Android. Opening the desktop client is separate and stays idle. Preparation refuses an existing metadata file and preserves another writer's nonempty library. It installs only `dev.emucore.probe`, does not download games or SDK components, and does not modify unrelated devices. The current reference uses fixed private ports and a single project-owned AVD, not independent per-profile VM disks.

## Metadata contract

`src/game_library.rs` defines schema version 1. A runtime profile records its ID, display name, engine type, and explicit SDK path. A game records its ID, title, package/activity component, profile ID, installation state, optional icon path, and optional owned-fixture metadata. An installed record needs a positive verification timestamp. That timestamp is cached evidence, not a guarantee that the app remains installed in an offline or subsequently modified device.

Loading is limited to 256 KiB, 256 games, and 16 profiles. Unknown fields, future schemas, invalid IDs/paths/components, duplicate IDs, duplicate normalized targets within a profile, and missing profile references fail validation. Unicode titles and paths are preserved. Library loading does not check path existence or start tools.

Edits acquire a nonblocking OS file lock, reload current data, validate the edit, write and sync a new exclusive temporary file in the same directory, close it, then rename it over the old file. Rejected edits and normal pre-rename failures preserve the old bytes. The persistent `.lock` file is an OS-lock anchor, not a stale lock marker. Locks release on handle close; temporary files from a crashed process may remain. This does not claim a power-loss transaction guarantee. See [Rust file locks](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock) and [rename behavior](https://doc.rust-lang.org/std/fs/fn.rename.html).

## Launch and cleanup

`ClientController::new` is inert. The first accepted launch creates one worker thread, which owns all SDK process/session work. The UI drains progress messages every 200 ms. Metadata edits stay on the UI thread; engine calls stay off it. The command queue is bounded and the launch gate permits one pending launch at a time.

A launch waits for Android readiness before requesting the selected component. A compatible running instance is reused only when the complete runtime profile matches. Repeated requests still ask Android whether that activity is resumed; cached UI state cannot prove it stayed open. Changing profiles requires stopping the current emulator. A failed launch drops the instance and enables an explicit retry. Capture directories under `.tools/client/runs` identify SDK errors.

Stop during startup signals cancellation and invalidates its launch ticket. Stale replies cannot revive a cancelled launch. Stop while running requests graceful shutdown, with owned-job cleanup on failure. The worker polls for a user-closed guest process every 250 ms. Closing the client cancels pending work, disconnects its queue, joins the worker, and drops its owned sessions. This is lifecycle ownership, not a security sandbox.

The desktop worker opts into an external guest window only after a launch request. Other reference captures remain headless by default. No audio or game key mapping is added here. See the [Rust controller](rust-android-controller.md) and [process ownership contract](host-processes.md).

## Validation and remaining work

The library tests cover inert missing-file loading, exact round trips, corrupt/unknown/future data preservation, concurrent edit exclusion, rename failure cleanup, duplicate references, and resource limits. Gate and worker tests cover duplicate pending requests, profile compatibility, cancellation/stale replies, launch failure/retry, external engine exit, and worker cleanup.

Four live saved-library launch attempts failed because the previously installed activity was missing after an SDK restart. Failed captures are preserved. Package-manager logs remember the package but report missing scanned code; vold also reports an unclean filesystem shutdown. Explicit non-incremental installation alone did not resolve the failure. This eliminates the incremental-install hypothesis as a sufficient explanation.

The controller now runs a bounded guest filesystem sync before requesting emulator exit, and retains verbose SDK disk initialization plus package-manager diagnostics on launch failure. The preparation run with sync passed native/touch verification and shutdown. Restart persistence after this change remains unverified. The client reports failure; ordinary launch never silently installs the fixture. Successful verification within one boot does not prove restart persistence. Android's documented behavior is to [preserve installed apps in userdata across restarts](https://developer.android.com/studio/run/emulator-commandline); the observed failure needs resolution before the client flow is accepted. ADB's [installation-mode implementation](https://android.googlesource.com/platform/packages/modules/adb/+/ef5ecc5ae3b084748ee82fa322a23239e864cba8/client/adb_install.cpp) explains the explicit non-incremental mode.

The complete check workflow passed 110 tests in debug and release, strict Clippy, formatting, both compiled static-guest comparisons, script syntax, and documentation checks. The subsequent three-line sync-before-exit addition compiled and passed the live preparation/shutdown check; the full workflow should be repeated after the persistence fix is settled.

The computer-use helper failed to initialize with `CreateRestrictedToken failed: 87`, including the prescribed retry/reset. Visual layout, mouse/keyboard interaction, and the visible guest-window launch remain unverified. Do not mark P12 complete from compilation or backend tests alone.

Measure the release client independently of Android:

```powershell
.\scripts\measure-client.ps1 -OutputDirectory artifacts/client-idle-01
```

This hidden-process measurement records process creation to `WaitForInputIdle`, idle CPU delta, private/working-set snapshots, and absence of the private reference listeners. It does not send UI input or validate rendering. Every sample forcibly ends its own client process. There is no cold-cache guarantee. Measurements on the development laptop cannot establish low-end support.

Five samples completed on the development laptop: launch-to-message-queue-idle was 106.6 to 264.5 ms (median 110.8 ms); working-set snapshots were approximately 10.2 to 10.6 MiB (median 10.54 MiB); private-byte snapshots were approximately 1.66 to 1.70 MiB. Observed idle periods were 8.0 to 8.4 seconds because listener queries add overhead to the five requested one-second sleeps. Four process CPU deltas were zero at the counter's resolution; one was 15.625 ms. These are hidden-window process observations, not proof of zero CPU use, visual responsiveness, game-launch latency, or low-end support. No private reference listeners were observed and no measured client process remained afterward. An earlier capture was rejected because a separate diagnostic VM was still running; it produced no valid idle samples.

Selected raw evidence is in [client measurements](measurements/client-2026-10-22/README.md), including failures, the sync preparation, executable/library identifiers, and source hashes captured at handoff.

Next: resolve and regression-check installation persistence, finish live library-to-game checks, visually review the window, render icons, and measure the physical low-end PC before closing P12.
