# Benchmarking procedure

No Android or gameplay performance measurements have been collected yet. The targets in the project README are provisional. Initial measurements of the static ELF prototype are available in [runtime measurements](runtime-measurements.md), with raw captures and separate metric boundaries.

## Record the environment

Copy the [run template](templates/benchmark-run.md) for each run. Include exact versions of Windows, drivers, emulator, Android image, game, and measurement tools. Record power mode, plugged-in state, CPU allocation, guest memory, renderer, resolution, frame cap, game graphics options, and thermal conditions.

Use the same PC and matched settings for comparisons. Record unavailable settings and differences instead of describing unequal configurations as equivalent. Keep required security and virtualization settings visible in the run record.

## Startup measurements

Measure these intervals separately with explicit start and end events:

1. Host process launch to host controller ready.
2. Guest start request to Android ready for interaction.
3. Game launch request to interactive main menu.
4. Session start request to playable training or practice scene.

Label runs as fresh guest boot, snapshot resume, or game relaunch. Record whether the host was rebooted and whether caches were warm. Do not imply that a fresh guest boot means the host disk cache was cleared.

## Gameplay scenarios

Start with the scenarios below and write exact steps, duration, map, hero, route, and actions into the first capture's record. Reuse those steps afterward.

| Game | Scenario | Focus |
| --- | --- | --- |
| PUBG Mobile | Training-area movement and repeated camera turns | Input response and frame pacing |
| PUBG Mobile | Repeatable training-area combat sequence | Rendering and combat-related stalls |
| Mobile Legends | Practice movement and repeated skill aiming | Control response and frame pacing |
| Mobile Legends | Reproducible combat sequence with multiple heroes where available | Performance under a more crowded scene |

Use a fixed warm-up period and a five-minute capture per scenario initially. Run each scenario at least three times per configuration, with comparable thermal conditions. Preserve failed runs and explain exclusions. If a scene cannot be reproduced exactly, describe the variation.

## Metrics

- Average FPS and p50, p95, and p99 frame times.
- Number and duration of stalls above a declared threshold. Start by reporting frames over 50 ms and 100 ms, keeping the same thresholds across comparisons.
- Host CPU and GPU usage, including the actual GPU and graphics engine used.
- Emulator process-tree private memory, system committed memory, available physical memory, and signs of paging.
- Startup times for the separate stages above.
- Input-to-display latency only when a stated method can measure it. Otherwise mark it not measured.
- Network latency separately from local rendering and input response.

Identify the measurement point for frame data. A host window presenting at 60 Hz does not by itself prove that the guest game renders 60 distinct frames per second. Validate what the chosen counter measures before using it for performance claims. Record capture-tool overhead when known and avoid double-counting guest RAM through multiple process metrics.

## Reporting

Link raw captures and include per-run results plus the range or median across repeats. Identify measurement limitations. Do not claim low-end support from CPU or memory restrictions applied to the development laptop, and do not claim improvement based on a single successful run.

Define final acceptance thresholds for frame pacing and memory after the first baseline is collected. Average FPS alone is insufficient to establish smooth gameplay.
