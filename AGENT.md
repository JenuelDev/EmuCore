# Shared project instructions

This is the shared engineering guide for EmuCore. [CLAUDE.md](CLAUDE.md) is the Claude entry point, and [AGENTS.md](AGENTS.md) is the standard agent entry point. Read this guide once, then follow the linked project documents. User instructions and current session authorization take precedence.

## Product direction

Build an AOW-inspired Android gaming platform for PUBG Mobile and Mobile Legends, with low-end Windows PCs as the performance objective. Prefer Rust for the host, lifecycle controller, diagnostics, and new performance-sensitive components. Reuse suitable existing components when justified by evidence.

The planned desktop client lists installed games without booting Android. Only a game launch request starts its configured emulator, waits for readiness, and launches that game's package/activity. Keep the client responsive, prevent duplicate launches, expose progress/errors, and support cancellation and owned-process cleanup. This requirement is tracked under P12.

Maintain a clear distinction between the direct ELF runtime, the external Android SDK reference, and the eventual product engine. The SDK reference's Android, graphics, and ARM translation are external components. Do not describe them as implemented by EmuCore. Game compatibility, redistribution suitability, and low-end performance require separate evidence.

## Read before changing behavior

- [Documentation index](docs/README.md): current status and detailed references.
- [Development plan](docs/development-plan.md): milestones and immediate work.
- [Architecture](docs/architecture.md) and [decisions](docs/decisions.md): responsibilities and unresolved choices.
- [Runtime contract](docs/runtime.md), [host process ownership](docs/host-processes.md), and [Android reference](docs/android-reference.md): implemented boundaries.
- [Development log](docs/development-log.md): completed work and validation.

Update the relevant contract and development log when behavior changes. Mark milestones complete only after their acceptance criteria are met. Record failed experiments and distinguish observations from hypotheses.

## Engineering rules

- Make incremental, reviewable changes. Preserve unrelated user changes and existing captures.
- Keep guest logical addresses separate from host pointers. Validate full memory ranges before mutation and retain resource/instruction budgets.
- Confine unsafe Rust and platform FFI to the native boundary, with documented safety invariants. Safe host and runtime modules forbid unsafe code.
- Keep guest writable and executable memory separate. Validate translated control-flow targets and guest return addresses.
- Use the tested owned-job process API for Rust-controlled external process trees. Keep lifecycle/resource management distinct from security isolation.
- Configure external tools explicitly. Keep writable test-device state inside the workspace and require the ownership marker before controlling the reference AVD.
- Opening configuration or library metadata must not launch Android. Startup is an explicit lifecycle action.
- Avoid silently installing SDK components, accepting licenses, modifying Windows settings, or changing existing user devices as part of routine launch.
- Pin dependencies, retain lockfiles and applicable notices, and prefer maintained libraries or generated bindings when they improve correctness.
- Keep keys, toolchains, VM disks, SDK images, and generated builds in ignored locations. Preserve small selected evidence under docs/measurements when useful.
- Measure optimized builds and define metric boundaries. Do not infer game FPS or low-end support from fixture timings or the development laptop.

## Validation

Run the checks appropriate to the change. The standard Rust workflow is:

```powershell
.\scripts\check.ps1
```

It checks formatting, strict Clippy, debug/release tests, both compiled static guests, PowerShell syntax, and project text/documentation. It does not automatically boot Android. External Android checks are documented in [the reference procedure](docs/android-reference.md) and [the owned JNI fixture](docs/android-native-fixture.md).

When forwarding arguments after a literal separator through the Cargo wrapper, use its explicit array parameter:

```powershell
.\scripts\cargo.ps1 -CargoArgs @('clippy', '--locked', '--all-targets', '--', '-D', 'warnings')
```

The pinned project-local Rust toolchain is under .tools when present. Follow actual execution permissions. Do not treat an earlier environment-specific sandbox failure as permanent authorization or a reason to skip checks.

If Flutter/Dart work is introduced on this Windows setup, run flutter and dart outside the sandbox because the SDK/cache is outside the workspace. Request the required tool approval and execute routine commands directly rather than asking the user to run them. Prefer persistent command-prefix approval when available. Always run `flutter analyze && dart format .` for Flutter changes.

## Communication and handoff

- Never use Unicode U+2014 in responses, documents, or generated text.
- Use concise progress updates and report concrete changes, validation, and remaining limitations.
- Continue authorized work without routine confirmation requests. Incorporate new user requirements without dropping the active objective.
- Do not spawn sub-agents unless the user or applicable instructions explicitly request delegation.
- Never claim a test, compatibility result, usage limit, or milestone that was not verified.

## Current handoff (2026-10-22)

P0 through P11 are documented as complete. The latest full check passed 110 tests in debug and release, plus both compiled static-guest comparisons. The Rust controller passed external Android x86-64/ARM64 native and touch checks, graceful shutdown, and live startup cancellation. This is separate from the direct runtime. See [controller evidence](docs/rust-android-controller.md).

P12 is in progress: bounded JSON metadata, atomic edits, launch gating, a lazy engine worker, and the provisional Win32 view are implemented. Library/gate/worker tests pass. Four live saved-library launches exposed lost app code after an external-SDK restart. Non-incremental installation alone did not fix it. Explicit guest sync before shutdown was added and its preparation run passed; a subsequent restart verification remains pending. Five hidden release-client idle measurements completed with no reference listeners, approximately 111 ms median launch-to-message-queue-idle and 10.5 MiB median working-set snapshot on the development laptop. Visual review is pending because the computer-use helper fails initialization with Windows error 87. See the [desktop client contract](docs/desktop-client.md). Keep configuration/library loading inert and start the configured emulator only after an explicit game launch. Consult the development plan and log for subsequent updates rather than relying indefinitely on this dated handoff.
