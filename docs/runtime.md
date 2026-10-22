# Runtime implementation status

Updated: 2026-10-22. This is the current runtime reference. [First prototype](first-prototype.md) describes the earlier P0 snapshot.

## Implemented behavior

- Static x86-64 ELF validation and bounded image loading.
- Reachable-instruction discovery across executable load segments, including helpers before entry, with explicit branch/call/fallthrough edges and overlapping reachable decodes rejected.
- Native Windows x64 code generation for the supported instruction subset.
- A reference execution backend for correctness comparisons, exposed through the library.
- All sixteen general-purpose guest registers, including 8-bit low registers, 16-bit partial writes, and 32-bit zero extension.
- MOV with register, immediate, and checked memory operands; LEA with base/index/scale/displacement or RIP-relative addresses; NOP; and output/exit syscalls.
- ADD, SUB, AND, OR, XOR, CMP, TEST, INC, DEC, and direct conditional/unconditional branches using modeled arithmetic flags.
- Direct CALL, validated RET (including immediate stack cleanup), and guest INT3/UD2 trap diagnostics. Return addresses are checked against decoded instruction boundaries and dispatched through an immutable native offset table.
- Linker-emitted PT_PHDR and read-only GNU unwind metadata, range-checked against load segments; non-executable GNU stack declarations with no size hint.
- Bounded anonymous mmap, whole-mapping mprotect/munmap, and raw brk query/grow/shrink, with explicit prototype restrictions described in [memory syscall behavior](memory-syscalls.md).
- Logical read/write/execute permissions, adjacent-region accesses, and complete range validation before writes.
- Linux-style stdout/stderr buffering, write error returns, exit status, and syscall clobbering of RCX/R11. Integer carry, overflow, zero, sign, parity, and auxiliary-carry flags are modeled. Undefined AF after logical operations is chosen as zero; INC/DEC preserve carry.
- A 16-byte-aligned guest startup stack containing bounded arguments, explicit environment entries, and auxiliary metadata; OS-generated random startup bytes; and checked 64-bit PUSH/POP.
- Worker supervision with a five-second timeout. A worker is crash containment, not a security sandbox.
- A supervised JSON profile command with stage timings, instruction/code counts, and final/peak logical guest backing. See [runtime measurements](runtime-measurements.md) for boundaries, capture commands, and initial results.

The decoder uses the pinned iced-x86 library. A separate lowering step rejects unsupported instructions, high-byte registers, vector registers, segment overrides, address-size overrides, and lock/repeat prefixes. Only fully lowered code reaches native allocation.

Native register/immediate moves operate directly on the private guest register file. Memory instructions call checked Rust helpers, so a guest effective address never becomes a raw host pointer. Generated code uses writable allocation followed by read/execute protection and an instruction-cache flush; it is never simultaneously writable and executable.

## Resource limits

| Resource | Limit |
| --- | --- |
| ELF file | 16 MiB |
| ELF virtual-address span | 16 MiB |
| Guest mapped memory | 32 MiB |
| Guest memory regions | 128 |
| Program headers | 16 |
| Total decoded instruction bytes | 64 KiB |
| Decoded instructions | 1,024 |
| Executed guest instructions per run | 100,000 |
| Combined stdout and stderr | 32 KiB |
| Worker elapsed time | 5 seconds |

Writable executable mappings are rejected. Writes crossing into inaccessible memory fail before changing any bytes. Faults in memory instructions stop the program with a guest address diagnostic; Unix signal delivery is not implemented.

Both backends charge one execution step per guest instruction, including branches. Native budget checks are emitted inline, so an infinite loop stops deterministically even through register-only code. Direct targets and fallthroughs are discovered from entry across file-backed executable segments. Calls also discover their return continuations. Decoding stops at unconditional branches, returns, and traps; syscall continuations are conservatively considered reachable. Unvisited padding or data is skipped. Two reachable instruction decodes may not overlap. Returns can target only discovered instruction starts. Arbitrary indirect CALL/JMP, runtime-generated guest code, and translation-cache invalidation remain unimplemented. An instruction must fit inside one executable segment, although fallthrough between adjacent segments is supported.

## Try the current fixtures

```powershell
.\scripts\check.ps1
New-Item -ItemType Directory -Path artifacts -Force | Out-Null
.\target\release\emucore.exe fixture artifacts\hello.elf
.\target\release\emucore.exe memory-fixture artifacts\memory.elf
.\target\release\emucore.exe args-fixture artifacts\args-loop.elf
.\target\release\emucore.exe run artifacts\hello.elf
.\target\release\emucore.exe run artifacts\memory.elf
.\target\release\emucore.exe run artifacts\args-loop.elf -- 'A variable-length argument'
.\target\release\emucore.exe run artifacts\compiled-guest.elf
.\target\release\emucore.exe run artifacts\mapped-guest.elf
```

Fixture generation refuses to overwrite existing files. Once a fixture exists, reuse it for subsequent runs. Use a fresh filename when regenerating a fixture after its generator changes. The memory fixture writes guest data before printing `Rust memory works` with a newline. The argument fixture scans argv[1] in guest code, handles empty strings, and exits 64 if the argument is missing.

## Build prerequisites and compiled guest

In addition to the pinned Rust toolchain and MSVC build tools, the check workflow now requires the `x86_64-unknown-none` Rust target. With a regular Rust installation, add it using `rustup target add x86_64-unknown-none --toolchain 1.99.1`. On this development machine it is already installed in the ignored project-local toolchain.

`scripts/build-guest.ps1` compiles [the checksum Rust guest](../fixtures/compiled-guest.rs), checks its formatting, and emits an ELF and assembly under artifacts. It computes a checksum through a non-inlined function and calls a write helper. `scripts/build-guest.ps1 -Guest mapped-guest` builds [the allocation guest](../fixtures/mapped-guest.rs), which maps, fills, protects, prints, and releases a data page. Both use no allocator or libc and invoke the Linux syscall ABI explicitly. Startup shims establish C function-entry stack alignment. See the [Rust target documentation](https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-none.html) for compiler target properties.

`examples/compare.rs` compares native and reference outputs, registers, flags, exit status, and instruction counts for an owned fixture. It executes in-process and is intended for supported owned programs. `scripts/check.ps1` builds both compiled guests and runs those comparisons automatically, checking their exact output.

## Validation

The P7 direct runtime and P10 host ownership pass 82 project tests in both debug and release builds, plus formatting and Clippy with warnings denied. Coverage includes all 65,536 byte operand pairs for addition/subtraction, branches/budgets, nested calls, invalid returns, precise faults, linker metadata, memory-management errors, zero initialization, resource caps, permissions, release, heap resizing, cross-segment control flow, before-entry helpers, skipped padding, overlapping decodes, missing fallthroughs, profile reporting, and memory high-water marks. Host tests additionally verify [owned process lifetimes](host-processes.md). Both independently compiled guests pass release native/reference comparisons with exact expected output. Documentation and script syntax checks are part of the workflow.

## Still missing

General Linux mapping semantics, virtual files, threads, signals, dynamic linking, libc support, Android runtime/services, ARM translation, graphics/audio, and either target game. No low-end gaming performance claim has been established. See [process startup contract](process-startup.md) and [memory syscall profile](memory-syscalls.md) for implemented behavior and restrictions.

Dependency notices are retained in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md). For decoder API details, see the [pinned iced-x86 documentation](https://docs.rs/iced-x86/1.21.0/iced_x86/).
