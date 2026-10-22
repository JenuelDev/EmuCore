# EmuCore development documentation

EmuCore aims to make PUBG Mobile and Mobile Legends responsive on low-end Windows PCs. See the [project README](../README.md) for the proposed hardware and gameplay targets.

| Document | Purpose |
| --- | --- |
| [Shared agent guide](../AGENT.md) | Engineering rules and linked CLAUDE.md/AGENTS.md entry points |
| [Development plan](development-plan.md) | Milestones, completion criteria, and immediate tasks |
| [Architecture](architecture.md) | Proposed components, language direction, and technical unknowns |
| [AOW research](aow-research.md) | Primary sources, recreation feasibility, and direct-execution experiments |
| [Engine comparison](engine-comparison.md) | Windows candidates, Android/image dependencies, graphics gaps, and reference recommendation |
| [Android reference](android-reference.md) | Isolated external boot probe, renderer/input observations, memory, and retained failures |
| [Owned Android fixture](android-native-fixture.md) | Source/build procedure and checked x86-64/ARM64 JNI execution with touch input |
| [Host process ownership](host-processes.md) | Rust Windows jobs, process-tree cleanup, memory accounting, and regression evidence |
| [Rust Android controller](rust-android-controller.md) | Explicit lifecycle, launch/cancellation, diagnostics, and live SDK-reference evidence |
| [Desktop game library](desktop-client.md) | Inert persisted metadata, native client, lazy worker, validation, and remaining P12 work |
| [First prototype](first-prototype.md) | Implemented ELF execution slice, build/run commands, limits, and next steps |
| [Current runtime](runtime.md) | Current instruction coverage, limits, fixtures, and validation |
| [Process startup](process-startup.md) | Guest stack, arguments, environment, auxiliary vector, and stack semantics |
| [Memory syscall profile](memory-syscalls.md) | Implemented mapping/heap subset, errors, restrictions, and evidence |
| [Decisions](decisions.md) | Decisions and their reasoning, including what is still provisional |
| [Benchmarking](benchmarking.md) | Repeatable startup, memory, and gameplay measurements |
| [Runtime measurements](runtime-measurements.md) | Static prototype profiling, metric boundaries, and initial raw captures |
| [Investigation log](investigation-log.md) | Observations, hypotheses, and evidence about GameLoop and candidate engines |
| [Development log](development-log.md) | Completed work, validation, and next steps |
| [Benchmark run template](templates/benchmark-run.md) | Copy for each measured run |

## Current focus

P0 through P11 are implemented and tested. The direct runtime executes two compiled libc-free guests and provides stage profiling. The Rust controller manages the external SDK reference through owned process jobs, passes owned x86-64/ARM64 JNI and touch checks, and handles live startup cancellation. P12 adds the installed-game client with on-demand Android launch. Android on EmuCore's direct runtime and target-game compatibility remain unverified.

## Keeping these docs useful

- Update the development log when a meaningful piece of work is completed.
- Record architecture choices in the decision log, with evidence and a status.
- Keep observed facts separate from assumptions and proposed work.
- Link benchmark claims to raw captures and a completed run record.
- Update milestone checkboxes only when their completion criteria are met.
- Use dates in YYYY-MM-DD format and identify the timezone for timed captures.
- Keep downloaded game files, VM disks, personal account data, and large captures out of Git. Record their local location, version, and hash where appropriate.
