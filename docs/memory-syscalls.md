# Guest memory-management subset

Status: P5 implemented and validated for owned static programs. This is a deliberately limited profile, not general Linux memory-management compatibility.

## Scope

The runtime provides anonymous data mappings, permission changes, removal, and a bounded program break. These operate entirely on logical guest memory backed by Rust storage. They do not change host process permissions, Windows features, or the executable native-code allocation.

Implemented x86-64 Linux calls are mmap (9), mprotect (10), munmap (11), and brk (12). Arguments come from RDI, RSI, RDX, R10, R8, and R9 before syscall clobbers. Ordinary errors return negative errno values in RAX. brk follows the raw kernel convention: return the resulting break on success and the current break on failure.

## Exact supported profile

- Pages are 4 KiB, with checked range/rounding arithmetic. The 32 MiB logical mapped-memory cap and 128-region cap include image, stack, metadata, heap, and anonymous mappings.
- mmap accepts only address zero, flags 0x22 (MAP_PRIVATE | MAP_ANONYMOUS), fd -1 in its low 32 bits, and offset zero. Requiring that fd value is a prototype restriction, even though Linux anonymous mappings normally ignore fd.
- Accepted protection values are 0 (none), 1 (read), and 3 (read/write). Executable protection returns -EACCES (13); other unsupported protection modes, including write-only, return -EOPNOTSUPP (95).
- Unsupported address hints or mapping flags return -EOPNOTSUPP. Zero length or invalid fd/offset returns -EINVAL (22). Length overflow, allocation failure, and mapping/resource exhaustion return -ENOMEM (12).
- New pages are zero initialized. Data remains intact across permission changes. Inaccessible mappings remain allocated and can have permissions restored.
- mprotect and munmap support one complete anonymous mapping only, with a page-aligned base and page-rounded length matching its allocation. Partial requests inside an existing mapping return -EOPNOTSUPP. Unknown mappings, invalid alignment, and invalid lengths return -EINVAL. ELF, stack, metadata, and heap mappings are outside these two operations' profile.
- Removal releases the logical mapping and its backing storage; later guest access faults. Validation/resource failures leave prior data, permissions, mapping records, and heap state unchanged.
- The initial break is the page-rounded end of the highest loaded segment. brk(0) queries it; requests below that base or beyond resource/overlap limits return the unchanged break. Successful requests retain the exact byte break separately from page-rounded backing size.
- Shrinking within a page retains that page's data. Reclaimed whole pages are zeroed if subsequently regrown. Heap backing is resized without retaining a large Vec capacity after a shrink.
- No guest-code protection changes, runtime-generated code execution, file mappings, shared mappings, fixed placement, or partial mapping splits are implemented.

These restrictions are deliberate and differ from full Linux semantics. They must remain visible when evaluating libc or Android compatibility.

## Validation

Tests cover invalid/overflow requests, alignment, flags, caps, zero initialization, permissions, release, failed-operation atomicity, and heap growth/shrinkage. Guest instruction tests verify the fourth, fifth, and sixth syscall argument registers and raw brk return values.

[mapped-guest.rs](../fixtures/mapped-guest.rs) is an independently compiled Rust program that maps a data page, fills it with volatile writes, changes it to read-only, prints `Guest mmap works!`, releases the mapping, and exits. `scripts/check.ps1` builds it and compares native/reference output, registers, flags, exit status, and instruction count, while retaining the original checksum guest regression.

These tests establish only the documented subset; they do not establish compatibility with a general allocator, dynamic linker, libc, ART, or either target game.

## References

- [Linux mmap/munmap manual](https://man7.org/linux/man-pages/man2/mmap.2.html).
- [Linux mprotect manual](https://man7.org/linux/man-pages/man2/mprotect.2.html).
- [Linux brk manual, including raw syscall differences](https://man7.org/linux/man-pages/man2/brk.2.html).
- [Linux syscall argument conventions](https://man7.org/linux/man-pages/man2/syscall.2.html).
