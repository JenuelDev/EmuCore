# Development plan

## P0: First executable runtime slice

- [x] Pin Rust and establish repeatable formatting, lint, test, and release-build commands.
- [x] Generate an owned static x86-64 Linux ELF fixture with known output and exit behavior.
- [x] Implement bounded ELF loading and reject unsupported layouts and instructions.
- [x] Translate the supported instruction subset into native Windows code.
- [x] Implement output, exit, and exit_group handling with guest-pointer checks and output limits.
- [x] Execute through a worker process and verify debug and release behavior.

Completed on 2026-10-22. See [implementation details and next slice](first-prototype.md). This completes a deliberately restricted experiment, not Android execution or M0 product feasibility.

## P1: Writable guest memory and instruction coverage

- [x] Add a bounded address space with explicit read/write/execute permissions and atomic range validation.
- [x] Use a maintained instruction decoder, retaining an explicit supported-instruction allowlist.
- [x] Model all sixteen x86-64 general-purpose registers and partial-register writes correctly.
- [x] Add MOV memory operands and LEA using logical guest addresses and checked memory helpers.
- [x] Demonstrate an owned fixture that changes writable data and prints it.
- [x] Verify read-only memory protection, cross-region accesses, overflow handling, native execution, and release behavior.

Completion: a writable-memory fixture executes correctly, memory faults stop execution with diagnostics, and the original fixture continues to work. No guest addresses may become host pointers.

## P2: Initial process state

- [x] Specify a static, libc-free process target and the initial register/stack contract.
- [x] Build an aligned guest stack containing argc, argv, environment pointers, and a documented auxiliary vector.
- [x] Add fixtures that read their initial stack and print a guest argument.
- [x] Add guest stack operations only with checked guest-memory access.

Completion: an owned static fixture reads a provided argument from its initial process state; guest state remains independent from the host environment.

## P3: Arithmetic, flags, and bounded control flow

- [x] Model arithmetic flags with independently checked carry, overflow, sign, zero, parity, and auxiliary-carry behavior.
- [x] Add integer arithmetic, logical operations, comparisons, and direct conditional/unconditional branches.
- [x] Reject branch targets outside the decoded code or into the middle of instructions.
- [x] Apply a deterministic execution budget to native and reference execution, in addition to worker timeout.
- [x] Upgrade the argument fixture to compute its own string length and handle missing arguments.
- [x] Verify forward branches, loops, signed/unsigned conditions, and deterministic exhaustion.

Completion: a bounded guest loop prints variable-length arguments correctly, and all tests/checks pass in debug and release.

## P4: Compiled static guest and function calls

- [x] Compile an owned freestanding Rust guest with the pinned toolchain and retain its source/build procedure.
- [x] Validate linker-emitted program-header, non-executable-stack, and read-only unwind metadata without treating them as executable mappings.
- [x] Implement direct guest CALL and validated guest RET using logical stack memory.
- [x] Handle explicit guest traps with diagnostics, including compiler-inserted unreachable instructions.
- [x] Verify nested calls, invalid return targets, stack-fault precision, and actual compiler-generated code.

Completion: an independently compiled static guest computes a checksum, calls a write function, prints the expected message, and exits successfully through the release worker. This remains a libc-free compatibility test, not Android execution.

## P5: Bounded guest memory-management calls

- [x] Specify a documented anonymous-mapping and heap subset using Linux syscall references.
- [x] Add mapping resize/protection/removal operations with complete validation before state changes.
- [x] Implement bounded anonymous mmap, mprotect, and munmap behavior, including six-argument syscall register handling.
- [x] Implement brk query/grow/shrink semantics without unbounded guest allocations.
- [x] Verify alignment, zero initialization, errno returns, permission changes, removal, and failed-operation atomicity.
- [x] Add an owned allocation fixture and keep the compiled Rust guest regression passing.

Completion: owned programs can allocate, modify, protect, and release guest data memory within the documented subset. Unsupported options must be explicit; dynamic guest code execution remains outside this slice.

## P6: Reachable code and ELF entry coverage

- [x] Decode reachable instructions from the ELF entry point across executable load segments, including functions before the entry point.
- [x] Validate overlapping decodes and branch/call targets while excluding unreachable padding or embedded data from execution.
- [x] Record explicit fallthrough edges so sorted native-code placement cannot change guest control flow.
- [x] Preserve validated return dispatch, instruction/resource budgets, and native/reference agreement.
- [x] Add fixtures for before-entry functions, multiple executable segments, unreachable unsupported bytes, and invalid overlaps/fallthroughs.

Completion: supported owned programs execute correct control flow across executable image regions without weakening memory or target validation. Runtime-generated code and dynamic linking remain separate work.

## P7: Runtime measurements

- [x] Measure decoding, process setup, native emission, executable allocation/protection, and execution independently.
- [x] Keep nondeterministic measurements separate from deterministic guest results.
- [x] Track final and peak logical guest backing without presenting it as host process memory.
- [x] Add a supervised profile command and repeatable release capture script that preserves failed runs.
- [x] Capture initial compiled-guest measurements and document boundaries and limitations.

Completion: reproducible raw measurements identify the costs of the current static runtime without implying Android startup or gaming performance.

## P8: External Android reference

- [x] Compare Windows engine/image candidates and record a provisional experiment recommendation.
- [x] Confirm installed tools and usable acceleration without changing Windows settings.
- [x] Prepare a project-owned reference device, isolated from user AVDs.
- [x] Add bounded boot/package readiness probes, logs, screenshots, and owned-process cleanup.
- [x] Capture effective guest RAM, process-memory snapshots, renderer, and ABI/bridge properties.
- [x] Verify a Settings-to-Home input transition and repeated graceful shutdown.

Completion: documented external Android reference observations exist, including failed attempts and limitations. This does not mean Android runs on EmuCore's translator or that M1/product integration is complete. See [reference results](android-reference.md).

## P9: Owned Android ABI fixture

- [x] Build a small owned JNI app using installed, pinned Android build tools and NDK.
- [x] Package and sign separate x86_64 and arm64-v8a test variants.
- [x] Verify a known native computation, visible result, and touch response on the project reference.
- [x] Preserve ABI-specific logs, APK hashes, screenshots, and any failures.

Completion: the reference executes the owned library in each tested ABI with independently checked output. This does not establish target-game compatibility or translator redistribution rights.

## P10: Rust process-tree ownership

- [x] Start Windows child processes suspended, attach an owned kill-on-close job, then resume only the owned primary thread.
- [x] Provide safe host APIs for bounded waiting, normal exit, forced termination, and descendant cleanup.
- [x] Use generated Windows bindings with retained licenses and keep unsafe calls in the native boundary.
- [x] Test normal exit, timeout/drop cleanup, descendants, missing executable, and preserved arguments.

Completion: the Rust host reliably owns external process lifetimes before implementing the Android reference lifecycle. This is process management, not a security sandbox.

## P11: Rust external-engine lifecycle

- [ ] Configure an explicit external engine, SDK/image paths, owned device, and resource profile.
- [ ] Start, observe readiness, collect diagnostics/accounting, and stop through the tested Rust process owner.
- [ ] Launch the owned Android fixture and preserve its existing native/input regressions.
- [ ] Expose lifecycle states and actionable errors for the future game-library client.

Completion: Rust coordinates an external Android reference reproducibly, with bounded failures and descendant cleanup. Engine/image distribution and target-game compatibility remain separate gates.

## P12: Game-library client and on-demand launch

User requirement added on 2026-10-22: provide a client showing installed games; clicking a game is when its emulator starts.

- [ ] Build a lightweight desktop game library that opens without booting Android.
- [ ] Persist installed-game records and display each game's name/icon, installation state, and configured runtime profile.
- [ ] On game click, validate installation and configuration, start the required emulator only then, wait for Android readiness, and launch that game's package/activity.
- [ ] Show starting, Android-ready, game-launching, running, and failed states while keeping the client responsive.
- [ ] Prevent duplicate launch requests and handle an already running compatible instance explicitly.
- [ ] Surface useful failure/retry controls through the Rust lifecycle API, including cancellation and process cleanup.
- [ ] Verify the entire library-to-game flow using the owned fixture before testing legitimately installed target games.
- [ ] Measure client startup, idle memory/CPU, and launch overhead on the low-end reference before selecting the final UI toolkit.

Completion: the client lists installed entries and a click launches the selected application through its configured emulator. Opening or browsing the client alone starts no Android VM. This is an emulator frontend milestone; it does not imply the games are compatible or meet their performance targets.

## M0: Feasibility and baseline

Answer whether we have a viable path to accelerated Android gaming on Windows before investing in the product interface.

- [x] Record initial games and proposed low-end hardware targets.
- [x] Establish development documentation.
- [x] Record Rust as the preferred host-side language, pending prototype validation.
- [x] Research public AOW/DAOW sources and record a separate direct-execution investigation path.
- [x] Perform the initial read-only tool, GameLoop registration, and virtualization inventory; record detection limitations in the prototype document.
- [ ] Identify an available physical low-end test PC. The current development laptop is not a substitute.
- [ ] Record the installed GameLoop version, settings, and per-game launch compatibility.
- [ ] Capture repeatable startup and gameplay baselines using the benchmarking procedure.
- [x] Compare candidate emulator engines and Android images for Windows support, acceleration, graphics, app ABI support, maintenance, licensing, and distribution terms. The provisional reference and remaining integration gaps are in [engine comparison](engine-comparison.md).
- [x] Specify and implement the owned x86-64 ELF fixture independently from the VM path.
- [ ] Identify whether the selected game builds need ARM native-code translation, and whether a usable, distributable solution exists.
- [ ] Record an engine recommendation and unresolved blockers in the decision log.

Completion: a sourced feasibility report compares the direct-execution and VM paths, identifies a proposed prototype and its runtime dependencies, explains the graphics and game-ABI path, and links baseline captures. Any unverified target-game compatibility must remain explicit. If no candidate is viable, record that result and the evidence before expanding implementation.

## M1: Minimal accelerated Android prototype

This is the VM prototype path. If M0 selects direct execution, replace its implementation checklist with a binary-loader and syscall-runtime milestone using the experiments in [AOW research](aow-research.md). Do not count a VM boot as proof of direct execution.

- [ ] Create a small Rust host application around the selected engine.
- [ ] Boot the chosen Android image with hardware acceleration confirmed by diagnostics.
- [ ] Display guest output and verify basic input, networking, and shutdown.
- [ ] Capture host startup time, Android readiness, memory use, logs, and failures.
- [ ] Document a reproducible setup using pinned component versions.

Completion: repeatable boot and shutdown with a usable guest display and confirmed acceleration. A command-line host is sufficient; a polished launcher is not required.

## M2: Target-game compatibility

- [ ] Verify PUBG Mobile installation, launch, and a playable training session.
- [ ] Verify Mobile Legends installation, launch, and a playable practice session.
- [ ] Add basic per-game keyboard and mouse mapping.
- [ ] Validate audio, networking, and recovery from a normal guest restart.
- [ ] Record unsupported features and game or engine errors.

Completion: both games have documented compatibility results and reproducible test steps. A game that launches but cannot enter gameplay is not counted as playable.

## M3: Low-end performance

- [ ] Run the same scenarios on the baseline low-end PC.
- [ ] Profile CPU, GPU, memory, disk activity, frame pacing, and startup stages.
- [ ] Improve measured bottlenecks and compare against GameLoop under matched settings.
- [ ] Validate the proposed 30 FPS PUBG Mobile and 60 FPS Mobile Legends goals, including stalls and frame-time distributions.
- [ ] Investigate the 4 GB stretch tier only after baseline results are understood.

Completion: publish measured results, test conditions, and remaining limitations. Define numeric frame-pacing acceptance criteria from the baseline before claiming stable performance.

## M4: Product interface and packaging

- [ ] Select the UI toolkit using startup, idle memory, and integration measurements.
- [ ] Refine the P12 game-library client, per-game runtime profiles, settings, diagnostics, and install/update flows.
- [ ] Package the selected components with required licenses and notices.
- [ ] Validate installation, updates, and removal on the chosen Windows versions.

Completion: a reproducible package tested on the supported hardware and Windows matrix.

## Immediate next task

Implement P11 Rust external-engine lifecycle, then P12's lightweight installed-game client with emulator startup only after a game click. Continue M0 game/low-end evidence separately. Use the P7 metrics when selecting later direct-runtime optimizations.
