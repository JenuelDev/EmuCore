# Tencent AOW research and recreation feasibility

Research date: 2026-10-22. Status: public-source investigation, with no local GameLoop binary analysis or prototype validation yet.

## Assessment

An independent engine with similar goals is technically plausible. Reproducing Tencent's exact implementation or matching its present-day game compatibility and performance is not established by the public material found here. Treat this as a substantial systems-software project with staged feasibility experiments.

Our goal is an independently implemented EmuCore runtime. Rust remains the preferred language for new host-side code. That choice does not remove the need for executable loading, ABI compatibility, Android services, graphics, and game-specific validation.

## Primary-source findings

### GameLoop's own description

GameLoop identifies AOW as self-developed and advertises operation without VT setup, faster startup, and low-end performance. These are vendor claims, not our measurements. Its product page also describes Tencent Protect integration; EmuCore cannot inherit those product assurances. Source: [GameLoop product page](https://www.gameloop.com/product/gameloop-download).

### Tencent-coauthored DAOW research

The 2019 MobiCom paper describes direct Android execution on Windows using customized Android-x86, binary rewriting, a Linux-syscall compatibility driver, shared-memory media integration, and Intel Houdini for ARM-library translation. This provides a concrete historical design, not proof of every current GameLoop implementation. Source: [DAOW paper, Figure 1 and sections 4.2 through 4.6](https://tianyin.github.io/pub/daow.pdf).

The accompanying demo shows Tencent Gaming Buddy with Turbo AOW branding, supporting the historical product connection. It reports 48% shorter game startup and 22% lower memory use against the authors' AOVB comparator. These are historical study results, not predictions for current GameLoop, our hardware, or EmuCore. Source: [DAOW demo paper, Figure 2 and section 2](https://tianyin.github.io/pub/daow-demo.pdf).

### Tencent patent naming AOW

Tencent's CN108170503B describes AOW implementations with Linux-compatible calls and functionality mapped onto Windows, ARM-to-x86 conversion, memory and filesystem support, and OpenGL ES forwarding to host graphics APIs. It discusses user-mode and kernel-mode embodiments. A patent disclosure is not evidence that a specific released build uses every embodiment. Source: [CN108170503B, description](https://patents.google.com/patent/CN108170503B/en).

The patent is a concrete item for distribution and patent review before a commercial implementation is committed. This research does not determine legal status, claim scope, territorial coverage, or permission to reuse code or components.

## What this changes in our plan

The existing VM investigation remains useful as a baseline, but it is a different architecture from the historical direct-execution design. An Android VM controlled by Rust would not by itself recreate AOW's approach.

Investigate two paths explicitly:

| Path | First purpose | Key uncertainty |
| --- | --- | --- |
| Existing accelerated VM engine | Establish Android boot and game compatibility baseline | Image, graphics, translation, and low-end resource use |
| Independent direct-execution runtime | Test whether a small Android/Linux binary can execute through a Windows compatibility layer | Correct binary execution, syscall semantics, and expansion to Android |

Do not combine results from these paths or call the first an AOW recreation.

## Proposed direct-execution experiments

These are our proposed experiments, not a recovered Tencent implementation specification.

1. Define the initial binary architecture and ABI. Build an owned minimal static Linux/Android ELF fixture for that ABI so dependencies and expected behavior are explicit.
2. Create a Rust loader that validates ELF segments, maps memory, establishes the initial execution state, and reports unsupported features clearly.
3. Implement the minimum execution adaptation and syscall handling needed for that fixture to produce known output and exit correctly. The adaptation must match the selected ABI; passing a file parser test alone is not an execution milestone.
4. Add focused fixtures for virtual memory, files, threads, synchronization, and signals. Record semantics and gaps against the chosen ABI.
5. Investigate Android-specific needs, including Binder IPC, Bionic, ART, framework services, graphics, and audio. Confirm requirements rather than assuming modern Android behaves like the historical system.
6. Evaluate ARM translation as its own dependency: architecture coverage, correctness, performance, licensing, and availability. Do not assume Houdini can be bundled or replaced by a small amount of Rust code.
7. Progress from a simple owned native program to a simple Android app, then a rendering workload, before attempting either target game.

Start with a user-mode experiment to limit implementation scope and simplify debugging. Whether it can support the required semantics and performance is an open question. Kernel work should follow a demonstrated requirement, not an assumption that drivers automatically improve speed.

## Unknowns that block performance promises

- Current GameLoop build, engine lineage, Android version, loaded modules, and relevant execution mode.
- Actual native ABIs required by the selected PUBG Mobile and Mobile Legends builds.
- Availability of a suitable ARM translation solution for our product.
- Required Android/Linux interfaces and compatibility behavior for modern game builds.
- Whether a playable game session is supported in our independently implemented environment.
- Frame pacing, startup time, memory pressure, and driver behavior on physical low-end hardware.

No official public AOW engine source repository or reusable SDK was located in this search. That is a search result limitation, not proof that none exists.

## Immediate next work

Inventory the local development environment and any installed GameLoop build. Record exact versions and hashes of relevant components before drawing conclusions about current behavior. Then write the direct-execution fixture specification and compare it against the VM feasibility path. Keep both prototype choices provisional until their first experiments produce evidence.
