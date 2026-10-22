# Static guest process startup contract

Status: P2 implemented and validated. Initial target: owned, static, libc-free Linux x86-64 ELF programs. This is not a claim of complete System V or Android process compatibility.

## Initial state

The guest receives a 64 KiB writable, non-executable logical stack at 0x7fffff000000. Its initial RSP points to a 16-byte-aligned structure containing argc, argv pointers, a null pointer, environment pointers, another null pointer, and auxiliary-vector type/value pairs ending in AT_NULL. Strings and auxiliary data are stored in the same stack mapping.

Arguments and environment are explicit process options. The default argv is `emucore-guest`, and the default guest environment is empty. Host environment variables are never copied automatically. Strings are UTF-8, cannot contain NUL, and are bounded by a 16 KiB combined limit, with at most 64 arguments and 64 environment entries. Environment entries must have a non-empty name followed by `=`.

General-purpose registers start at zero except RSP, which points to the guest stack. Initial flags are 0x202. This is a defined prototype state, not a promise that all otherwise unspecified Linux startup registers have these values.

## Auxiliary metadata

Planned auxiliary entries include AT_PHDR, AT_PHENT, AT_PHNUM, AT_PAGESZ, AT_BASE, AT_FLAGS, AT_ENTRY, AT_UID, AT_EUID, AT_GID, AT_EGID, AT_PLATFORM, AT_SECURE, AT_RANDOM, and AT_EXECFN. Static executables use AT_BASE zero. UID/GID values are synthetic 1000 and do not expose host identity. No VDSO or CPU-feature capability claims are supplied.

If the program-header table is already covered by a loaded segment, AT_PHDR points to it. Otherwise its bytes are copied into a separate read-only guest metadata mapping at 0x7ffffef00000. This synthetic mapping is a deliberate prototype adaptation, required by our fixtures whose ELF headers are outside their PT_LOAD segments. Overlaps with reserved runtime mappings are rejected.

AT_RANDOM points to sixteen bytes obtained from the host OS random source for each run. Unit tests may provide a deterministic seed directly to the stack builder; normal execution does not use a fixed seed. AT_EXECFN points to argv[0], and AT_PLATFORM points to `x86_64`.

## Stack operations

PUSH and POP operate only on checked guest memory. The host stack remains separate. An invalid access produces a runtime diagnostic, with register updates ordered so failed pushes or pops do not publish partial state. Pushing RSP uses its pre-decrement value, and popping into RSP assigns the popped value after the old stack slot has been read.

Dynamic linking, TLS setup, signal frames, environment inheritance, process creation, and a libc startup path remain unimplemented. P3's argument fixture now calculates string length in guest code. P4 adds checked CALL/RET frames; invalid return targets are diagnosed before RSP is advanced.

## Reference

The layout is informed by the [x86-64 psABI process-initialization specification](https://gitlab.com/x86-psABIs/x86-64-ABI/-/blob/master/x86-64-ABI/low-level-sys-info.tex). The bounded mappings and synthetic metadata choices above are EmuCore design decisions.
