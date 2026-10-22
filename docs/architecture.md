# Architecture direction

Status: product architecture proposed. The [current Rust ELF runtime](runtime.md) executes an owned compiled static guest with bounded memory and control flow. The full Android runtime described here does not exist yet.

## Language recommendation

Prefer Rust for new host-side code: process lifecycle, configuration, diagnostics, input routing, and performance instrumentation. Rust offers native-code performance without a required garbage collector, which makes it a suitable candidate for performance-sensitive host components. See the [Rust project overview](https://rust-lang.org/).

C and C++ remain reasonable integration languages when a selected engine or library already uses them. Do not rewrite a mature engine solely to make the project entirely Rust. Keep foreign-function boundaries small and document ownership, thread requirements, and error handling if they become necessary.

The host language alone does not establish Android boot speed or game FPS. Measure time spent in the launcher, VM initialization, Android startup, game loading, graphics translation, and disk access separately.

## Proposed responsibilities

| Component | Responsibility | Current direction |
| --- | --- | --- |
| Host controller | Start, stop, monitor, configure, and collect diagnostics | Rust preferred |
| VM engine | CPU virtualization, devices, guest memory, and guest execution | Evaluate existing engines first |
| Android guest | Android framework, drivers, app runtime, and installed games | Image and Android version undecided |
| Native-code compatibility | Execute the game build's native libraries | Inspect game ABIs and evaluate translation only if required |
| Graphics path | Translate or forward guest graphics to the host GPU and present frames | Backend and display integration undecided |
| Input and audio | Map host devices to guest input and handle sound | Validate early in the prototype |
| Desktop UI | Profiles, settings, diagnostics, and game launch controls | Select after the minimal prototype |

## Game-library client

User requirement recorded on 2026-10-22: show installed games in a lightweight client, then start the emulator only when the user clicks a game. Client startup and library browsing must not boot Android. The library reads persisted installed-game/profile metadata so it can render without a running guest.

A launch request flows from the selected library entry to the Rust controller, which validates the configured installation/profile, starts the required runtime, observes Android readiness, and launches the selected package/activity. Lifecycle state and failures flow back to the client. Duplicate clicks must not create duplicate emulator processes; an existing compatible runtime is handled explicitly. Keep launch work off the interface thread and retain cancellation and owned-process cleanup.

Per-game profiles will map each entry to its runtime/device configuration and launch target. Validate the flow first with a clearly identified owned test fixture, then with legitimately installed PUBG Mobile and Mobile Legends after compatibility checks. The UI toolkit remains undecided until startup and idle-resource measurements exist. See P11 and P12 in the [development plan](development-plan.md).

## Engine investigation paths

The user wants to recreate an AOW-like engine. [AOW research](aow-research.md) establishes a historical direct-execution design, distinct from a full VM. Investigate an independent compatibility runtime alongside the existing-engine baseline. Neither path is selected for the product yet.

For the VM baseline, evaluate an existing Windows-capable engine with hardware acceleration before considering a custom hypervisor. QEMU with its WHPX backend is one candidate to investigate, not a selected gaming solution. [QEMU's WHPX documentation](https://www.qemu.org/docs/master/system/whpx.html) establishes Windows acceleration support, but does not establish that a particular Android image or either target game will work well.

The [candidate comparison](engine-comparison.md) recommends the already installed Android Emulator as an external boot/measurement reference, then a controlled crosvm WHPX integration probe. Neither is selected as the final product engine. The reference's SDK/image components remain external to the repository and product package.

For the direct-execution path, begin with an owned minimal ELF fixture and a Rust loader plus the required execution and syscall adaptation. Android runtime and graphics integration follow only after executable behavior is demonstrated. The VM-engine row above applies to the VM path; the direct path instead needs a binary-execution and OS-compatibility component. ARM app translation remains a separate unresolved dependency in either path when the chosen game ABI requires it.

Investigate CPU acceleration and GPU acceleration separately. Android's emulator documentation distinguishes the two and explains the architecture constraints on accelerated guest execution. An ARM guest on an x86 host cannot use the same VM acceleration path described there for an x86 guest. This is separate from translating ARM app libraries inside an x86 Android guest. See [Android hardware acceleration documentation](https://developer.android.com/studio/run/emulator-acceleration).

## Questions that must be answered

1. Which guest architecture and Android version work with the selected game builds?
2. Are x86 native libraries available, or is ARM app translation needed?
3. Can the engine, Android image, and any translation components be distributed with the product?
4. Which accelerated graphics path works on the reference integrated GPU?
5. Can both games reach playable sessions in the chosen environment?
6. Which Windows versions and virtualization configurations can be supported?
7. How much total host memory is needed to keep an 8 GB PC responsive?

## Performance principles

- Measure optimized release builds when evaluating host-code performance.
- Keep synchronous logging, unnecessary allocations, and blocking work out of input and rendering paths where profiles show they matter.
- Avoid redundant frame copies and software rendering in the intended performance configuration.
- Measure cold boot separately from snapshot resume; report both if snapshots are introduced.
- Check that improvements reduce total resource use or latency rather than shifting work into another process.
- Choose the UI toolkit after measuring its contribution to startup and memory.

Sources checked: 2026-10-22. Recheck component documentation when selecting and pinning versions.
