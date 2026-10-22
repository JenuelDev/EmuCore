# Rust host process ownership

Updated: 2026-10-22. The Windows-only host_process module supplies the process-lifecycle foundation for the external Android controller and future game-library client. It does not itself start Android during library initialization.

## Contract

ManagedProcess::spawn accepts a configured Rust Command, creates a hidden suspended child, assigns a private Windows job, then resumes the child's primary thread. The job enables kill-on-close, does not permit breakaway, and its handle is non-inheritable. Assignment happens before user code runs so the child cannot start ordinary descendants outside the owned job during that setup window. Setup failures terminate and reap the suspended child.

The implementation uses stable Rust and generated pinned Windows bindings. Unsafe calls are confined to src/native/process.rs, underneath the existing native boundary. The public host module forbids unsafe code. Handles use Rust OwnedHandle, and command arguments retain Rust's normal argument handling. No shell command is constructed. Existing creation flags are replaced with CREATE_SUSPENDED and CREATE_NO_WINDOW; configure standard streams and environment on Command first.

The owner supports nonblocking status checks, bounded polling waits, explicit job termination, optional extraction of configured stdio pipes, and Windows job memory peaks. A wait timeout returns None without silently terminating the process. Dropping the owner terminates the job, including remaining descendants after a normal root-process exit, then reaps the root. If the host exits without destructors, closing its last non-inherited job handle supplies the kernel cleanup path.

Explicit termination uses exit status 125 for processes still running. A previously exited root retains its actual status while its remaining descendants are terminated. Callers requesting piped output must drain it while the process runs, or use files; a bounded wait alone cannot prevent a child from blocking on full output pipes. The Android controller will use file-backed diagnostics to preserve output without pipe deadlock.

Memory peaks come from QueryInformationJobObject's extended-limit information. They concern Windows job/process committed-memory accounting, not resident physical memory, GPU memory, or the direct runtime's logical guest mappings. They are suitable for comparing a controlled engine process tree under stated conditions; system memory pressure still needs separate measurement.

This is lifecycle/resource ownership, not a security sandbox. Normal CreateProcess descendants inherit job membership; other process-creation services and privileged external interference are outside this contract. Windows versions/configurations that reject job assignment fail explicitly. Product Windows support and external-engine behavior remain to be validated independently.

## Validation

The full project check passes 82 tests in debug and release, strict Clippy, formatting, both compiled static-guest comparisons, PowerShell syntax checks, and documentation checks. New subprocess tests verify:

- A normal child exit preserves quoted, Unicode, and trailing-backslash arguments.
- A running owned helper times out without stopping prematurely; dropping its owner terminates it.
- A descendant remains alive after the root exits normally, then terminates when the job owner drops.
- Explicit job termination produces the expected status and remains observable through subsequent waits.
- A missing executable returns an I/O error without leaving a running child.
- A deliberately allocated 16 MiB helper is included in job/process memory high-water accounting.

All helpers are owned test processes. Tests observe live process handles to verify termination rather than treating disappearance of a PID as sufficient evidence.

## Sources

The implementation follows [Windows job ownership and inheritance](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), [CREATE_SUSPENDED behavior](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags), [thread snapshot traversal](https://learn.microsoft.com/en-us/windows/win32/toolhelp/traversing-the-thread-list), and [job memory accounting](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information). [Rust's Windows CommandExt](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html) documents stable creation flags; its direct raw-attribute spawn interface is still nightly-only in the pinned toolchain and is not used. Dependency versions and notices are in [third-party notices](../THIRD_PARTY_NOTICES.md).
