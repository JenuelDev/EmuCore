# Decision log

Use proposed, accepted, superseded, or rejected as the status. A recommendation is not evidence of measured performance.

## D001: Initial target games

- Date: 2026-10-22
- Status: accepted
- Decision: prioritize PUBG Mobile and Mobile Legends.
- Reason: user-selected games define the initial compatibility and input requirements.
- Consequence: verify each game's installation, launch, and playable sessions independently.

## D002: Low-end hardware focus

- Date: 2026-10-22
- Status: accepted as a development target; support remains unverified
- Decision: prioritize the 8 GB integrated-graphics baseline in the project README. Treat 4 GB systems as an experimental stretch tier.
- Reason: low-end usability is the project's main product requirement.
- Consequence: the development laptop alone cannot validate support. All FPS goals require physical-hardware measurements.

## D003: Prefer Rust for new host-side code

- Date: 2026-10-22
- Status: proposed
- Decision: use Rust for the host controller and new performance-sensitive host components, subject to prototype validation.
- Reason: native execution, no required garbage collector, and memory-safety support make Rust a suitable systems-language candidate. See the [Rust project overview](https://rust-lang.org/).
- Alternatives: C or C++ for new host code; integration with an existing engine regardless of its implementation language.
- Consequence: allow existing C/C++ dependencies and separate engine processes. No commitment to writing the hypervisor, Android runtime, or GPU translator from scratch.
- Validation needed: confirm toolchain, Windows API integration, chosen engine control, startup measurements, and resource use in M1.

## D004: Prototype before product UI

- Date: 2026-10-22
- Status: proposed
- Decision: prove accelerated Android boot and investigate game compatibility before committing to a desktop UI toolkit.
- Reason: engine, graphics, and app-ABI feasibility are unresolved.
- Consequence: the first runnable milestone may be a command-line controller and a separate guest display.

## D005: Investigate an independent AOW-inspired runtime

- Date: 2026-10-22
- Status: accepted for investigation; implementation architecture remains proposed
- Decision: investigate direct execution as an explicit EmuCore path, keeping the existing VM investigation as a separate baseline.
- Reason: the user wants to recreate an AOW-like engine, and the [primary-source research](aow-research.md) gives us a historical design to investigate.
- Consequence: do not describe a Rust-controlled VM as an AOW recreation. Begin the direct path with an owned executable fixture and evidence of correct execution before attempting Android services or games.
- Validation needed: executable and syscall correctness, Android compatibility, ARM translation availability, graphics performance, and distribution suitability.

## D006: Implement a restricted native translation slice first

- Date: 2026-10-22
- Status: accepted for the initial prototype
- Decision: implement the first runtime slice in Rust using a validated static x86-64 ELF fixture, a closed instruction subset, native Windows code generation, and output/exit syscall adaptation.
- Reason: this provides an executable, testable foundation before attempting a complete Android environment. It does not depend on unresolved ARM translation or game integration.
- Consequence: guest addresses remain logical checked addresses. General Linux process startup, arbitrary instruction execution, Android services, and target-game support are outside this slice.
- Validation: 17 tests pass in debug and release; strict Clippy, formatting, and release smoke execution pass. See [first prototype](first-prototype.md).
- Product scope: this decision validates Rust for this host prototype, while the full engine architecture remains open.

## D007: Maintained decoding with a closed execution subset

- Date: 2026-10-22
- Status: accepted
- Decision: use pinned iced-x86 decoding, then explicitly lower supported instructions into our IR. Keep logical guest memory separate from host pointers and apply permissions/resource bounds.
- Reason: operand/register coverage needs reliable instruction decoding; decoding an instruction does not establish that we implement its behavior.
- Validation: register widths, indexed addressing, permissions, precise faults, and native/reference comparisons pass. Dependency notices are retained.

## D008: Explicit guest process state

- Date: 2026-10-22
- Status: accepted for the static prototype
- Decision: build a bounded, non-executable startup stack with explicit argv/environment and documented auxiliary metadata. Do not inherit host environment variables automatically.
- Consequence: this is a defined static, libc-free startup subset, with synthetic identity and metadata choices documented in [process startup](process-startup.md).

## D009: Bounded and validated native control flow

- Date: 2026-10-22
- Status: accepted for the prototype
- Decision: permit branches and returns only to decoded instruction boundaries, and enforce 100,000 guest instructions per run in both backends. Keep guest return addresses out of the host call stack.
- Consequence: native RET dispatch uses a read-only table of generated instruction offsets. General indirect branches and runtime-generated guest code remain unsupported.
- Validation: backward loops, signed/unsigned branches, nested calls, invalid returns, deterministic exhaustion, and precise fault-state tests pass.

## D010: Independently compiled static guest as a regression

- Date: 2026-10-22
- Status: accepted
- Decision: compile an owned Rust guest using the pinned freestanding target and compare native/reference observable state during the check workflow.
- Reason: compiler-generated code and linker metadata exercise compatibility beyond our byte-generator fixtures.
- Consequence: development checks require the additional x86_64-unknown-none target; this does not add a runtime dependency to the shipped host executable.
- Validation: the compiled checksum/output guest succeeds in the supervised worker and release native/reference comparison. Full Linux, Android, and game compatibility remain open.

## New decision template

- ID and title:
- Date:
- Status:
- Problem:
- Decision:
- Alternatives considered:
- Evidence and sources:
- Consequences:
- Validation still needed:
- Supersedes, if applicable:
