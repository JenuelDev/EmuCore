# First implementation: constrained ELF execution

Date: 2026-10-22. Status: historical P0 implementation snapshot, validated on the Windows x86-64 development machine. See [current runtime status](runtime.md) for later additions and current dependency/feature information.

## First steps and completion criteria

| Step | Deliverable | Status |
| --- | --- | --- |
| 1 | Read-only tool and virtualization inventory | Initial inventory completed; limitations below |
| 2 | Pin Rust and provide repeatable local build commands | Rust 1.99.1 MSVC, formatting, Clippy, and check script |
| 3 | Define an owned static Linux ELF fixture | Deterministic Rust generator and reference assembly |
| 4 | Validate and load ELF without trusting host pointers | Bounded image, permissions, alignment, and range checks |
| 5 | Translate a restricted instruction subset into native Windows code | Implemented on Windows x86-64 |
| 6 | Implement output and exit calls with defined error behavior | Implemented and tested |
| 7 | Run through a disposable worker and verify release behavior | Worker integration tests and release tests pass |

Success for this slice means the generated fixture prints its exact message and returns the expected exit status, while malformed and unsupported inputs are rejected. It does not establish Android or game compatibility.

## Run it

From the project root in PowerShell:

```powershell
# Format, lint, test debug and release builds, then build the release executable.
.\scripts\check.ps1

New-Item -ItemType Directory -Path artifacts -Force | Out-Null
.\target\release\emucore.exe fixture artifacts\hello.elf
.\target\release\emucore.exe inspect artifacts\hello.elf
.\target\release\emucore.exe run artifacts\hello.elf
```

Expected guest output:

```text
Hello from EmuCore's Linux ELF fixture!
```

`fixture` refuses to overwrite an existing file. Reuse the generated file for subsequent inspect/run commands, or supply a new filename. `run` propagates the guest's low eight exit-status bits. Prototype errors return 125 and include a diagnostic on stderr; a guest can also explicitly exit with 125, so retain the diagnostic when interpreting failures.

`scripts/cargo.ps1` uses the project-local toolchain if present and otherwise uses the installed Cargo. For arguments containing Cargo's `--` separator, use an explicit array so PowerShell does not consume the separator:

```powershell
.\scripts\cargo.ps1 -CargoArgs @('test', '--all-targets')
.\scripts\cargo.ps1 -CargoArgs @('clippy', '--all-targets', '--', '-D', 'warnings')
```

Rust is pinned by `rust-toolchain.toml`. On a new machine, install Rust using the [official Windows instructions](https://rust-lang.org/tools/install/) and the MSVC C++ build prerequisites. `.tools`, build output, and generated artifacts are ignored by Git; Cargo.lock is retained. The implementation has no third-party crate dependencies.

## Execution pipeline

1. Read at most 16 MiB plus one byte to detect oversized files.
2. Validate ELF metadata and load segments into a bounded, zero-initialized guest image represented by a Rust byte buffer.
3. Decode the entire file-backed entry block into a restricted intermediate representation. Reject unsupported instructions before allocating executable code.
4. Emit native Windows x64 instructions that operate on a private four-register guest state and call the host syscall bridge. Guest instruction bytes are never copied into executable memory.
5. Allocate the generated code as read/write, copy it, switch it to read/execute, and flush the instruction cache. Release it through an owning RAII wrapper.
6. Buffer guest output with a combined limit, then return the exit status. The CLI executes this in a worker with a five-second deadline and inherited output handles.

This is native translation of a restricted block. It is not a general ELF process launcher, an interpreter for arbitrary x86, or a complete recreation of AOW. Guest virtual addresses remain logical addresses checked by the memory layer. A Linux initial stack, auxiliary vector, stack instructions, flags, and general memory operations are not implemented.

## Supported subset

- ELF64, little-endian, x86-64, ET_EXEC, System V/Linux identification, ABI version zero.
- Up to 16 program headers; only PT_LOAD and PT_NULL. No dynamic linking, interpreter, TLS, or relocations.
- Readable, disjoint segments; writable executable segments rejected; entry must be executable and file-backed. Section tables are not used for execution.
- At most 16 MiB of guest virtual-address span, 64 KiB of entry code, and 1,024 decoded instructions.
- Register-immediate MOV (32 or 64 bits), register-to-register MOV32, self-XOR32, RIP-relative LEA into RSI, and SYSCALL.
- Register operands limited to RAX, RDI, RSI, and RDX. No branches, calls, guest stack operations, arbitrary memory operands, or flags-dependent instructions.

| Linux x86-64 call | Behavior |
| --- | --- |
| write (1) | Capture stdout (fd 1) or stderr (fd 2) from readable guest memory |
| exit (60) | Stop the translated block, returning the low eight status bits |
| exit_group (231) | Same behavior, since the prototype has one execution thread |

Writes return byte count, -EBADF for unsupported descriptors, -EFAULT for invalid non-empty buffers, or -EFBIG when the combined 32 KiB output limit would be exceeded. This output-limit behavior is a deliberate prototype constraint. Unsupported syscall numbers are fatal diagnostics. Reaching the end without an exit call is an error. Output is buffered and published after a successful runtime result; fatal prototype errors do not publish partial guest output.

## Engineering boundaries

- Only `src/native.rs` permits unsafe code. Each unsafe block states its invariant; the other implementation modules forbid unsafe code.
- The code generator follows the Windows x64 calling convention, preserves R12, and reserves the required call shadow space and stack alignment.
- Generated code has no unwind metadata. Callbacks must not unwind through it; this is not a general exception-handling implementation.
- A worker is crash containment, not a security sandbox. It runs with the host user's privileges. The prototype is intended for owned test fixtures, and has not been audited for hostile inputs.
- No Windows features, firmware settings, drivers, or game installations were modified.
- No Android services, ARM translation, GPU rendering, or game support exists yet. There are no low-end performance results.

## Inventory findings

Visual Studio Community 2022 and the MSVC 14.44 x64 compiler/linker were present. Rust was not found in PATH or the checked standard installation locations, so the official rustup installer was downloaded, checked against its published SHA-256 value, and used to install a project-local toolchain without changing system PATH.

Windows reported an active hypervisor. Its processor WMI fields also reported false for virtualization capability flags; those values alone do not establish that firmware virtualization is disabled in this environment. Availability of WHPX and an accelerated VM remains unverified. No GameLoop entry matched the checked uninstall registrations; an unregistered or portable installation has not been ruled out.

## Validation

17 tests pass in both debug and release configurations. Coverage includes every truncated prefix of the owned ELF, header and segment errors, address overflow, zero-filled memory, unsupported instruction rejection, Linux error returns, output limits, register semantics, repeated native execution, and the actual CLI worker. Formatting and Clippy with warnings denied pass. A release executable was built successfully.

## Next slice

Define the initial guest process model and add owned fixtures for memory access and writable guest memory, extending decoder and memory tests together. Record whether the next executable target is a static libc-free program or a dynamically linked Android binary before implementing stack setup, auxiliary vectors, ELF interpreter loading, or Android-specific interfaces. Continue GameLoop/version and ARM-translation investigations separately; this first slice does not settle the product architecture.

## Technical references

- [ELF header specification](https://gabi.xinuos.com/elf/02-eheader.html) and [program-header specification](https://refspecs.linuxfoundation.org/elf/gabi4%2B/ch5.pheader.html).
- [Windows x64 calling convention](https://learn.microsoft.com/en-us/cpp/build/x64-calling-convention).
- [VirtualAlloc](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualalloc), [VirtualProtect](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualprotect), and [FlushInstructionCache](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-flushinstructioncache).
- [Linux x86-64 syscall table](https://github.com/torvalds/linux/blob/master/arch/x86/entry/syscalls/syscall_64.tbl).
