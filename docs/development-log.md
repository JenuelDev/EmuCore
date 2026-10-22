# Development log

## 2026-10-22: Project direction and documentation

Completed:

- Established PUBG Mobile and Mobile Legends as initial target games.
- Defined an 8 GB integrated-graphics baseline and an experimental 4 GB stretch tier.
- Recorded the development machine's reported CPU, RAM, and GPU.
- Created the milestone plan, architecture direction, decision log, investigation log, and benchmark procedure and template.
- Recorded Rust as the preferred host-side language, pending prototype validation.

Validation: checked documentation changes for whitespace errors, local Markdown links, and the prohibited em dash character. No application tests apply because no implementation exists.

Remaining: no emulator, game compatibility result, or measured performance improvement exists yet.

Next: perform the read-only environment inventory, compare engine/image candidates, and record feasibility findings under M0.

## 2026-10-22: AOW recreation research

Completed:

- Found and reviewed GameLoop's product description, Tencent-coauthored DAOW papers, and a Tencent patent naming AOW.
- Added a sourced research report that distinguishes historical evidence, vendor claims, proposed experiments, and unresolved dependencies.
- Updated the architecture and milestone plan to investigate direct execution separately from the VM baseline.
- Recorded decision D005 for the user-requested AOW-inspired investigation.

Validation: checked documentation links, whitespace, and the prohibited character. This was documentation research; no binary execution or game test was performed.

Next: read-only environment inventory and a specification for the first owned ELF execution fixture.

## 2026-10-22: First Rust runtime slice implemented

Completed:

- Installed a checksum-verified, project-local Rust toolchain without modifying system PATH, then pinned Rust 1.99.1 for reproducibility.
- Added a dependency-free Cargo application, lockfile, local-toolchain wrapper, and repeatable check script.
- Implemented bounded ELF loading, restricted instruction decoding, native Windows code generation, checked output handling, and exit calls.
- Added worker-process supervision, a deterministic owned fixture generator, and reference assembly.
- Documented supported behavior, implementation limits, inventory findings, and next steps.

Validation: rustfmt check and Clippy with warnings denied pass. All 17 tests pass in both debug and release builds. The release CLI generated and inspected the ELF, printed the exact fixture message, and returned zero. The release executable is 272,896 bytes on this build; no startup or gaming benchmark claim is inferred from its size.

Engineering details: unsafe code is confined to the Windows backend with documented invariants. Generated code transitions from writable to executable; guest instruction bytes and guest addresses are not treated as executable host code or host pointers. Tests cover malformed input, resource bounds, native register behavior, syscall errors, and actual worker execution.

Remaining: no Android execution, ARM translation, GPU rendering, target-game compatibility, or low-end performance result exists. The worker is not a security sandbox.

Next: define the guest process model and add owned fixtures for writable guest memory and memory operands, while continuing M0 architecture and GameLoop investigation.

## 2026-10-22: Writable guest memory and expanded decoding

Completed P1: added a bounded, permission-checked address space; all sixteen general-purpose registers; partial-register semantics; MOV memory operands; and indexed LEA. Switched instruction decoding to pinned iced-x86 with an explicit execution allowlist and retained dependency notices. Added a reference backend and native/reference comparisons.

Validation: formatting, strict Clippy, and all 29 tests pass in debug and release. The release memory fixture writes guest data and prints the expected message. Tests also verify that later permission faults cannot partially write earlier regions.

Next: P2 initial guest stack, explicit arguments, and startup metadata. Android and target-game support remain unimplemented.

## 2026-10-22: Initial guest process state

Completed P2: implemented a bounded 64 KiB guest stack, explicit arguments and environment, an auxiliary vector, OS entropy for AT_RANDOM, and checked guest PUSH/POP operations. The CLI forwards arguments through the worker after `--`. Added startup-layout, Unicode/space forwarding, stack-ordering, and fault-state tests. Fixed an intermediate arithmetic overflow in program-header address translation and added a regression test.

Validation: all 40 tests pass in debug and release; formatting and strict Clippy pass. Release argument-fixture execution prints the expected four-byte argument. That fixture's fixed width is a temporary P2 limit, to be removed by the P3 loop implementation.

Next: arithmetic flags, direct branches, and deterministic execution budgets. Full Linux/Android process compatibility remains unverified.

## 2026-10-22: Bounded arithmetic and control flow

Completed P3: integer arithmetic/logical operations, modeled flags, direct branches validated against instruction boundaries, and the same 100,000-instruction budget in native and reference backends. The argument fixture now computes strlen in guest code and handles empty/missing arguments.

Validation: 51 tests passed in debug and release, including exhaustive byte addition/subtraction over 65,536 operand pairs, independent flag expectations, signed/unsigned conditions, backward loops, precise arithmetic faults, and deterministic native-loop exhaustion. Formatting and strict Clippy passed.

## 2026-10-22: Independently compiled static Rust guest

Completed P4: added direct CALL, checked RET and stack cleanup, immutable native return dispatch, and explicit guest trap diagnostics. Extended ELF validation for mapped program-header/unwind metadata and non-executable stack declarations. Added an owned freestanding Rust guest plus a pinned build procedure and native/reference comparison example.

Validation: all 58 tests pass in debug and release. Formatting, strict Clippy, compiled-guest formatting/build checks, the release native/reference comparison, and project text/documentation checks pass. The guest computes its checksum, calls its output function, prints `Compiled Rust guest executed successfully!`, and exits zero. A supervised-worker smoke execution also succeeded.

Remaining: no libc, Android runtime, ARM translation, graphics, or game support. Calls/returns are currently limited to decoded entry-region instruction boundaries.

Next: specify and implement a bounded anonymous memory-management syscall subset, then add owned heap/mapping fixtures before attempting libc or Android binaries.

## 2026-10-22: Anonymous mappings and bounded program break

Completed P5: added a documented data-only mmap/mprotect/munmap profile, raw brk query/grow/shrink, checked page rounding, recoverable resource failures, whole-region protection/removal, and bounded backing resize. Partial anonymous operations are explicitly rejected. Captured all six x86-64 syscall arguments and retained separate raw-kernel brk semantics.

Added a second compiled Rust guest that allocates a page, fills it through volatile writes, protects it read-only, prints `Guest mmap works!`, releases it, and exits. The check workflow builds both guests and verifies native/reference observable state and exact output.

Validation: all 66 tests pass in debug and release, with formatting and strict Clippy. Allocation/protection/removal, errno handling, zero initialization, limits, heap data retention/regrowth, and failed-operation atomicity are covered. Worker smoke execution of the compiled allocation guest succeeds. Documentation/text checks pass.

Next: P6 reachable-code traversal, explicit fallthrough edges, and execution across executable segments. General allocators, libc, Android, ARM translation, and games remain unverified.

## 2026-10-22: Reachable code across executable segments

Completed P6: replaced entry-forward linear decoding with bounded traversal of reachable instructions. Supported helpers may precede entry or occupy another executable segment. Recorded explicit fallthrough edges in both backends so physical native-code ordering cannot create a guest fallthrough. Rejected overlapping reachable decodes and non-executable targets while skipping unreachable invalid padding.

Validation: all 72 tests pass in debug and release, with strict Clippy, formatting, both compiled-guest comparisons, and documentation checks. Six new regressions cover before-entry calls, separated and adjacent executable segments, unreachable bytes, invalid overlaps/targets, and missing fallthrough.

Next: collect stage-level runtime timings and guest-memory high-water marks before choosing an optimization.

## 2026-10-22: Runtime stage measurements

Completed P7: added a supervised JSON profile command, a library profile API separate from deterministic guest state, native stage timings, and guest-backing high-water marks. Added a repeatable release capture script with binary/input hashes, explicit warmups, raw records, overwrite refusal, and retained failures.

Validation: all 76 tests pass in debug and release, with formatting, strict Clippy, and both compiled-guest comparisons. Collected 20 measured and 3 warmup launches for each guest, all successful. Verified capture overwrite refusal and retained invalid-input diagnostics. Raw captures and metric definitions are linked in [runtime measurements](runtime-measurements.md).

Observed median runtime totals of approximately 0.8 ms for these small fixtures on the development laptop; parent elapsed was approximately 32 ms. This does not measure Android startup, gameplay, or low-end performance. No optimization claim is inferred.

Next: compare Windows-capable VM foundations and Android/image/ABI dependencies under M0 before committing to a product execution path.

## 2026-10-22: External Android reference

Completed P8: compared Windows execution candidates, found the installed SDK/image, confirmed usable WHPX, and created a workspace-owned Android 35 reference device. Added PowerShell preparation/probe tools with private adb, bounded readiness/commands, retained logs/failures, actual memory/configuration, renderer/ABI evidence, input verification, screenshot capture, and graceful/fallback cleanup.

Validation: one pre-boot harness failure was retained and corrected; three subsequent Android boots succeeded. Settings-to-Home input was verified twice. The final launcher screenshot was visually inspected. Shutdown inspection found no owned listening ports or reference emulator. Script syntax and documentation checks pass. This external reference does not run Android on EmuCore's native translator.

Evidence and interpretation: [Android reference](android-reference.md). Effective guest RAM is 2.5 GB despite a 2 GB request; process private-memory snapshots were approximately 3.6 GiB. No low-end or gaming claim follows.

Next: owned x86_64/ARM64 JNI fixture, then a Rust controller around an explicitly configured external engine.

## 2026-10-22: Owned Android x86-64 and ARM64 JNI probes

Completed P9: added a small owned Java/JNI app, pinned SDK/NDK build procedure, separate signed ABI packages, ELF header validation, checked startup/touch calculations, and automated UI button discovery/input. Retained source/build/APK hashes, exact logs, failures, reports, and screenshots.

Validation: both x86-64 and ARM64-only APKs pass the expected native computations and actual UI touch response on the external reference. Screenshots were visually inspected; both successful probes shut down gracefully. A deliberately mismatched APK ABI is rejected before device launch. The first x86 attempt's pre-fixture Settings launch timeout was retained, and activity launch allowance was corrected to a bounded 15 seconds. See [native fixture evidence](android-native-fixture.md).

Next: a Rust process supervisor with owned Windows jobs and tested descendant cleanup, followed by explicit external-engine lifecycle integration. No game, low-end, or independently implemented ARM-translation claim follows from these fixture results.

## 2026-10-22: Installed-game client requirement

Recorded the user's game-library client requirement as P12: opening the client displays installed games without booting Android; clicking a game starts its configured emulator, waits for readiness, and launches the selected package/activity. Added launch states, duplicate-request handling, responsive UI, cleanup/cancellation, and client-resource measurements to the acceptance checklist. P11's Rust lifecycle controller remains the immediate integration prerequisite; current P10 work continues.

## 2026-10-22: Rust process-tree ownership

Completed P10: added a safe Windows host process API backed by private kill-on-close jobs. Children start suspended, join the owned job, then resume. Added bounded observation, explicit termination, descendant cleanup after root exit, pipe access, and job/process commit-memory peaks. Retained pinned generated binding/linking notices.

Validation: all 82 tests pass in debug and release, with strict Clippy, formatting, compiled static-guest comparisons, PowerShell syntax checks, and documentation checks. Subprocess regressions verify arguments, timeouts, explicit/drop termination, a live descendant after root exit, missing executables, and memory peaks. See [host process contract](host-processes.md).

Next: P11 Rust external-engine lifecycle, then the user's P12 installed-game library with Android startup on game click.

## Entry template

- Date:
- Completed work:
- Files or commit:
- Validation and results:
- Decisions or investigation links:
- Remaining issues:
- Next step:
