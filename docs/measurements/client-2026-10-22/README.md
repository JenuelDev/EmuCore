# Client captures, 2026-10-22

These selected files support the [desktop client contract](../../desktop-client.md). Large VM images, APKs, and full captures remain in ignored workspace directories. The development machine is the i9-14900HX / 32 GB / RTX 4080 Laptop host used for the reference probes, not the physical low-end target.

- [Idle report](idle-report.json): five hidden release-client process samples, with executable/library SHA-256 hashes. No input or SDK launch was requested. WaitForInputIdle is a process/message-queue boundary, not visual verification. Processes were forcibly ended after measurement.
- [Initial failure](initial-failure-events.txt) and [verbose failure](verbose-failure-events.txt): a saved verified entry could not launch after restart.
- [Package diagnostics](package-failure-events.txt) and [guest boot log](package-restart.log): missing scanned probe code and unclean guest-filesystem shutdown observations. The .log preserves the original guest log whitespace.
- [Non-incremental failure](streamed-failure-events.txt): disabling incremental installation alone did not fix restart persistence.
- [Sync preparation events](synced-preparation-events.txt) and [result](synced-preparation-result.txt): native/touch checks, bounded guest sync, and emulator shutdown passed. A subsequent restart remains unverified.
- [Source hashes](source-hashes.json): selected source and Cargo.lock SHA-256 identifiers captured at handoff. These identify the handoff source; they are not claimed to reproduce the earlier idle executable's exact build inputs.

The complete check workflow passed 110 tests per debug/release profile before the final sync-before-shutdown addition. That addition compiled and passed the live preparation run. Visual preview was blocked by computer-use initialization error 87. No target-game or low-end performance claim follows.
