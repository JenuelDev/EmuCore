# EmuCore

EmuCore is a Windows Android gaming emulator project, using GameLoop as a reference for behavior, usability, and performance.

The goal is responsive gameplay, stable frame rates, consistent frame pacing, and low input latency. Performance will be measured on defined hardware and games rather than described as universally lag-free.

## Initial target games

| Game | Planned benchmark scenarios | Input priorities |
| --- | --- | --- |
| PUBG Mobile | Training-area movement, aiming, camera rotation, and combat | Mouse aiming, keyboard movement, and configurable action mappings |
| Mobile Legends | Practice-mode movement, skill aiming, and combat with multiple heroes | Movement controls, directional skill aiming, and configurable action mappings |

For both games, verify installation, launch, and supported emulator compatibility before measuring gameplay. Record each game's version, graphics settings, resolution, and available frame-rate options. Compatibility and performance have not yet been established.

## Proposed low-end performance targets

These are engineering goals, not verified minimum requirements or performance claims. Prioritize the baseline tier first, then investigate the stretch tier.

| Tier | Reference hardware | Initial gameplay goal |
| --- | --- | --- |
| Low-end baseline | Intel Core i5-8250U, Intel UHD 620 integrated graphics, 8 GB dual-channel RAM, SSD, hardware virtualization enabled | 1280 x 720 at low graphics settings; PUBG Mobile at a stable 30 FPS and Mobile Legends at 60 FPS, where the games expose those frame-rate modes |
| Experimental stretch | Older 2-core/4-thread CPU with hardware virtualization, 4 GB RAM, integrated graphics | Investigate Mobile Legends at low settings and 30 FPS, reducing resolution if needed; make no PUBG Mobile performance commitment until measured |

Select an exact stretch-tier PC before benchmarking. Record Windows edition and build for every test; Windows support and the Android version remain to be decided during the compatibility investigation.

The baseline deliberately includes integrated graphics so a dedicated gaming GPU is not a design assumption. For an 8 GB host, measure total system memory pressure as well as emulator memory; avoid optimizing FPS at the cost of sustained paging or an unresponsive desktop.

Use repeatable gameplay captures to evaluate frame-time percentiles and stalls, alongside average FPS. A 30 FPS target has a 33.3 ms frame budget; a 60 FPS target has a 16.7 ms frame budget. Distinguish network latency from local rendering and input latency.

For context, [GameLoop's PUBG Mobile listing](https://www.gameloop.com/game/action/pubg-mobile) lists an i3-8300, GTX 1050, and 8 GB RAM as its minimum configuration (checked October 22, 2026). EmuCore's integrated-graphics target is an unproven development objective, not evidence that this game already runs well on that hardware.

The current development PC reports an Intel Core i9-14900HX, approximately 32 GB RAM, and an NVIDIA GeForce RTX 4080 Laptop GPU. It is useful for development, but release performance must also be validated on physical low-end hardware. Limiting CPU or memory on this machine does not reproduce an older integrated GPU's behavior.

## Project direction

- Study GameLoop's observable behavior, configuration, resource usage, startup, rendering, and input handling.
- Record observations separately from hypotheses about its internals.
- Build and benchmark EmuCore incrementally, starting with a constrained executable runtime experiment, then investigating Android execution and rendering.
- Choose the virtualization, Android runtime, graphics, and desktop UI approach after the initial investigation.

## Roadmap

1. Validate the proposed low-end reference configuration, choose the Windows and Android versions, and obtain baseline measurements for PUBG Mobile and Mobile Legends.
2. Establish a repeatable GameLoop baseline, including startup time, CPU and RAM usage, average FPS, frame-time percentiles, and input latency where measurable.
3. Investigate emulator foundations and build a minimal prototype that boots Android and displays its output.
4. Add keyboard and mouse mapping, audio, window controls, and game installation.
5. Profile representative gameplay, address measured bottlenecks, and compare against the baseline under the same conditions.

## Initial investigation

Record the GameLoop version and configuration, Windows version, CPU, GPU, RAM, graphics driver, virtualization settings, game version, and benchmark procedure for each comparison. Keep benchmark settings and evidence alongside the findings so results can be reproduced.

## Current status

The planned desktop client will show installed games without starting Android. Clicking a game will start its configured emulator and launch that game after Android is ready. This on-demand library flow is tracked as P12 in the [development plan](docs/development-plan.md).

The Rust prototype executes two independently compiled, libc-free x86-64 guests using checked memory, process startup, arithmetic, bounded control flow across executable segments, function calls, and anonymous memory-management calls. A reference backend checks native translation behavior. Stage profiling and [initial runtime measurements](docs/runtime-measurements.md) are available. It does not run Android or either target game yet. No gaming performance results exist.

See [current runtime and run commands](docs/runtime.md) to build and try it, including writable-memory execution. Run `.\scripts\check.ps1` for formatting verification, strict linting, debug/release tests, and a release build.

Development documentation starts at [docs/README.md](docs/README.md). Product architecture, Android integration, graphics backend, and UI framework remain open decisions.

An [external Android reference](docs/android-reference.md) now boots using the already installed SDK with WHPX and host graphics. An [owned JNI fixture](docs/android-native-fixture.md) passes checked native computations and touch input in separate x86-64 and ARM64 APKs. This reference uses the SDK's engine and translation components; Android does not yet run on EmuCore's direct-execution runtime.

