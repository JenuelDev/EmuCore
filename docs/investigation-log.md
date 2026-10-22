# Investigation log

Use this log for GameLoop observations and candidate-engine experiments. Keep observed facts, hypotheses, and next experiments distinct.

## I001: Initial project state

- Date: 2026-10-22
- Scope: repository and read-only development-machine inspection.
- Observed: the repository initially contained a README and Git attributes, without emulator implementation files.
- Observed: Windows reports an Intel Core i9-14900HX, approximately 32 GB RAM, and an NVIDIA GeForce RTX 4080 Laptop GPU.
- Interpretation: this is a development machine, not the low-end reference configuration.
- Not verified: GameLoop installation or behavior, virtualization availability, Rust toolchain, target-game compatibility, and performance.
- Next experiment: read-only inventory of installed tools, GameLoop, and Windows virtualization capabilities.

## I002: Candidate acceleration path

- Date: 2026-10-22
- Scope: documentation review only.
- Documented fact: QEMU documents a Windows Hypervisor Platform acceleration backend. See [WHPX documentation](https://www.qemu.org/docs/master/system/whpx.html).
- Documented fact: Android distinguishes VM acceleration from graphics acceleration and specifies guest/host architecture constraints. See [Android acceleration documentation](https://developer.android.com/studio/run/emulator-acceleration).
- Hypothesis: an existing accelerated engine controlled by a Rust host could be a practical prototype foundation.
- Not verified: compatible Android images, integrated-GPU rendering, ARM app translation, either target game, or distribution suitability.
- Next experiment: compare concrete engine/image combinations and their requirements before selecting one.

## I003: Public AOW research

- Date: 2026-10-22
- Scope: public-source research, without local binary inspection.
- Evidence: [AOW research and source links](aow-research.md).
- Finding: primary material provides a historical architecture to investigate, with significant compatibility and translation questions still open.
- Interpretation: direct execution deserves its own feasibility experiment; the existing VM path remains a separate baseline.
- Not verified: the current installed GameLoop engine, reproducibility of historical results, or either target game's compatibility with EmuCore.
- Next experiment: inventory the development environment and installed GameLoop version, then specify the first direct-execution fixture.

## I004: First native execution experiment

- Date: 2026-10-22
- Scope: local read-only environment inventory followed by the owned Rust prototype.
- Environment: Visual Studio Community 2022, MSVC 14.44 x64 tools, project-local Rust 1.99.1 for x86_64-pc-windows-msvc.
- Observed: Windows reports an active hypervisor; conflicting processor capability fields do not settle firmware or WHPX configuration. No GameLoop registration matched the checked uninstall keys; a portable or unregistered installation remains possible.
- Procedure: run the check script, generate the owned ELF, inspect it, then execute it through the release CLI worker.
- Observed result: ELF entry 0x401000, 4,136-byte logical image, eight decoded instructions, exact fixture message, and exit status zero.
- Fixture SHA-256: A637637368705F39AC5FBB6E4D54DB1B1459B92A00E6203BAF691F4E847D314F.
- Interpretation: the restricted translation and output/exit bridge work on this host. This does not establish a general Linux ABI, Android compatibility, or game performance.
- Evidence: [prototype details](first-prototype.md), source and tests in the repository, and generated artifacts/hello.elf plus target/release/emucore.exe (both ignored by Git).
- Next experiment: specify writable guest memory, memory operands, and initial-process semantics using owned fixtures.

## I005: External Android reference

- Date: 2026-10-22.
- Scope: installed Android SDK, fresh project-owned AVD, and external-engine boot/input/graphics/memory observations.
- Procedure and evidence: [Android reference](android-reference.md) and its retained reports.
- Observed: WHPX is usable; Android 15 boots with host RTX graphics; Settings-to-Home input and graceful shutdown succeed. The image advertises x86_64/arm64-v8a and names libndk_translation.so.
- Observed: requested 2 GB becomes 2.5 GB configured guest RAM; post-boot emulator process-tree private memory is approximately 3.6 GiB on this development PC.
- Interpretation: the external reference supports lifecycle and ABI experiments, but its memory footprint needs investigation for the 8 GB objective. Advertised ARM support still needs actual native-code execution tests.
- Unknowns: game builds/ABIs, low-end GPU behavior, sustained memory pressure, ARM compatibility for real workloads, image/translator distribution suitability, networking/audio, and playable games.
- Next experiment: independently build and execute owned x86_64 and ARM64 JNI fixtures with checked computation and touch response.

## Entry template

- ID and title:
- Date:
- Scope and question:
- Component versions and settings:
- Procedure:
- Observed facts:
- Evidence location or source:
- Hypothesis or interpretation:
- Unknowns and limitations:
- Next experiment:
