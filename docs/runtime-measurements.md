# Static runtime measurements

Updated: 2026-10-22. These measurements concern the owned static ELF prototype. They do not measure Android readiness, game loading, rendering, or low-end hardware.

## Capture

Build and validate with `scripts/check.ps1`, then run:

```powershell
.\target\release\emucore.exe profile artifacts\compiled-guest.elf
.\scripts\measure-runtime.ps1 -Elf artifacts/compiled-guest.elf -Output artifacts/runtime-new.jsonl -Runs 20 -Warmup 3
```

Use a fresh output filename. The script refuses to replace existing captures. Guest arguments can be supplied with `-GuestArguments @('one argument', 'another')`. The profile CLI accepts the same arguments after `--` as the run command.

Each profile invocation runs in the supervised worker with the normal instruction limit and five-second deadline. The command emits one JSON report on success and returns the guest's exit status. Guest stdout and stderr are captured and counted, rather than printed into the report. Runtime failures emit a diagnostic and no success report. A guest's nonzero exit still produces a report; the capture script records it as a failed run. Failed captures remain in the output file, and the script reports failure after completing the requested runs.

The capture file is newline-delimited JSON. Its first record contains environment information, stopwatch frequency, ELF and executable SHA-256 hashes, requested counts, and arguments. Subsequent records retain warmups and measured runs separately, including parent elapsed time, exit status, metrics, and failure diagnostics. Arguments are saved in the capture, so use synthetic fixture inputs. Warmup means repeated process launches with likely warm OS caches, not a persistent translation cache. Each worker independently loads and translates its program.

## Metric boundaries

All times are monotonic wall-clock nanoseconds reported by Rust Instant or the capture script's Stopwatch. Units do not imply nanosecond clock accuracy. No timing assertion requires a stage to be nonzero.

| Field | Boundary |
| --- | --- |
| load_ns | Worker file read, ELF validation, and image construction |
| decode_ns | Reachable instruction discovery and lowering |
| process_setup_ns | Guest address space, OS entropy, startup stack, context, and target table |
| translation_ns | Native emission and branch/table patching |
| executable_setup_ns | Writable native allocation, copy, RX protection, and instruction-cache flush |
| execution_ns | Invocation of generated code through its return, including helper calls and syscall handling |
| runtime_total_ns | Decode through result preparation and runtime-owned guest/native storage release |
| worker_total_ns | Start of worker command handling through successful result, excluding report serialization/output and process teardown |
| parent_elapsed_ns | PowerShell invocation of the CLI through return and output collection, including launch, worker startup/supervision, and teardown |

Runtime total excludes ELF loading. Stage sums need not equal runtime total because bookkeeping and teardown are included only in the total. Worker total excludes Windows process creation and work before worker dispatch. Parent elapsed includes script/native invocation overhead and the supervisor's polling interval. It is not a pure process-startup measurement.

`decoded_instructions` counts the discovered graph; `executed_instructions` counts actual guest steps. `native_code_bytes` includes generated instructions and immutable dispatch data, without OS page rounding. Final and peak `guest_mapped_bytes` count logical mapped backing, including the startup stack and metadata. They exclude host allocator overhead, transient resize copies, the original ELF image, decoded IR, output buffers, generated code, process working set, and system memory. A peak after a successful unmap remains greater than final backing.

The library's `run_profiled` returns timing and memory metrics separately from `RunResult`, retaining deterministic native/reference result comparisons. The reference backend reports zero native bytes, translation time, and executable setup time. CLI profiling currently uses the native backend.

## First development captures

Captured on 2026-10-22 at approximately 12:01 UTC (20:01 Asia/Manila), using the release build on the development laptop: Core i9-14900HX, approximately 32 GB RAM, RTX 4080 Laptop GPU, Windows build 26300, PowerShell 7.6.7. This CPU-only fixture does not use the GPU. Exact binary/input hashes and OS version are in the [checksum guest raw capture](measurements/2026-10-22-compiled-runtime.jsonl) and [mapping guest raw capture](measurements/2026-10-22-mapped-runtime.jsonl).

Each capture contains 3 warmup and 20 measured launches, all successful. No runs were excluded. The table reports medians of measured launches, averaging the two middle values. Each field's median is computed independently.

| Metric | Checksum guest | Mapping guest |
| --- | ---: | ---: |
| Load | 120.80 us | 120.55 us |
| Decode | 435.50 us | 437.75 us |
| Process setup | 307.25 us | 288.95 us |
| Native translation | 8.05 us | 10.40 us |
| Executable setup | 8.15 us | 8.80 us |
| Execution | 6.50 us | 8.50 us |
| Runtime total | 784.15 us | 772.05 us |
| Worker total | 904.70 us | 891.30 us |
| Parent elapsed | 32.40 ms | 31.77 ms |
| Decoded / executed instructions | 30 / 24 | 81 / 63 |
| Native bytes | 1,810 | 4,595 |
| Guest backing final / peak | 66,189 / 66,189 bytes | 66,246 / 70,342 bytes |

Measured parent elapsed ranged from 28.75 to 44.10 ms for checksum and 29.64 to 34.51 ms for mapping. Runtime total ranged from 672.50 to 854.90 us and 689.30 to 953.00 us respectively. There are too few launches to characterize tail latency or make broad performance claims.

These fixtures suggest that initialization and launch overhead dominate their very short workloads. That observation does not identify the dominant costs of an Android process or a game. No baseline optimization comparison, host-memory capture, power-mode control, thermal control, or physical low-end run has been completed. Preserve this capture as an initial observation, then measure a representative workload before changing the architecture for speed.
