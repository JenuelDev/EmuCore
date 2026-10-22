# Execution-path feasibility comparison

Reviewed: 2026-10-22. This compares prototype paths, not proven gaming products. No candidate has passed either target game's playable-session test on this project.

## Candidate matrix

| Candidate | Windows CPU execution | Android and graphics integration | Current assessment |
| --- | --- | --- | --- |
| EmuCore direct-execution experiment | Our native translator executes a bounded static x86-64 subset without a VM | No Android framework, linker, Binder, graphics, threads, signals, or ARM bridge | Retain as an AOW-inspired research path. Two owned compiled guests are evidence of the restricted runtime only. |
| Android Emulator with its supported system images | Official Windows WHPX path; host and guest architecture constraints apply | Integrated Android images and configurable host graphics backend | Best immediate external reference because the development PC already has a usable emulator and image. This is not a selected redistributable product engine. |
| QEMU with WHPX plus an Android image | Upstream documents Windows x86-64 WHPX, tested from Windows 10 2004 | Android image, guest drivers/HALs, and accelerated Windows renderer must be integrated and verified together | Useful independent CPU/device baseline. Do not infer Windows gaming graphics from WHPX availability. |
| crosvm with WHPX plus an Android image | Windows build instructions and WHPX backend exist | Virtio/gfxstream capability exists, but compatible guest and Windows build integration remain work | Strong Rust candidate for a later controlled build probe. WHPX is marked not tested upstream in its hypervisor support table. |
| Cuttlefish | Official setup reviewed here depends on KVM and Debian host packages | Modern AOSP virtual-device workflow | Useful Linux-side Android reference; not the immediate native Windows prototype. |

Evidence: [Android acceleration](https://developer.android.com/studio/run/emulator-acceleration), [Android command line](https://developer.android.com/studio/run/emulator-commandline), [QEMU WHPX](https://www.qemu.org/docs/master/system/whpx.html), [QEMU virtio GPU](https://www.qemu.org/docs/master/system/devices/virtio/virtio-gpu.html), [crosvm Windows build](https://raw.githubusercontent.com/google/crosvm/main/docs/book/src/building_crosvm/windows.md), [crosvm hypervisor support](https://github.com/google/crosvm/blob/main/hypervisor/README.md), [crosvm device overview](https://github.com/google/crosvm/blob/main/README.md), and [Cuttlefish setup](https://source.android.com/docs/devices/cuttlefish/get-started).

CPU acceleration is not graphics acceleration. QEMU documents separate 2D, virglrenderer, and rutabaga/gfxstream variants, with substantial Linux host/guest requirements for documented accelerated configurations. The cited pages do not prove a usable accelerated Windows Android stack for a chosen QEMU build. That is an unresolved integration experiment, not a claim that acceleration is impossible.

Google currently recommends WHPX for the Android Emulator and documents AEHD's sunset on December 31, 2026. Avoid making a new dependency on AEHD. This does not require changing this PC's security or virtualization settings; the installed emulator already confirms WHPX is usable. [Official acceleration guidance](https://developer.android.com/studio/run/emulator-acceleration).

## Images and app architecture

The installed Android 35 Google Play x86_64 image, revision 9, is suitable for an external boot/reference experiment. It is not an EmuCore image, and its contents are not copied into this repository or a distributable. The reference device's writable data belongs under the ignored project .tools directory. Pin and record image metadata for each test.

Android-x86 9.0-r2 is a documented Android 9 release from March 2020 with a QEMU launch procedure. Its age and that procedure do not establish compatibility with current target-game builds or this Windows graphics path. Treat it as a historical fallback, not the default modern gaming image. [Android-x86 release notes](https://www.android-x86.org/releases/releasenote-9-0-r2.html).

A guest's x86_64 CPU architecture does not establish support for ARM app libraries. AOSP provides a native-bridge integration mechanism, but its documentation explicitly distinguishes that mechanism from an actual translator implementation. Inspect the exact legitimate APK/split set and the guest's ABI/bridge properties before choosing a translation dependency. A property naming a bridge does not establish game compatibility or distribution rights. [AOSP native-bridge documentation](https://android.googlesource.com/platform/art/+/main/libnativebridge/README.md).

Neither target game's exact version, native-library ABI inventory, engine acceptance, or playable-session result has been recorded. No unofficial game download or proprietary translation binary has been added.

## Source and distribution dependencies

Component licensing must be checked for the actual selected build and image. QEMU's top-level license identifies GPLv2 for the emulator as a whole and separately licensed firmware. crosvm's top-level license contains BSD-style source/binary redistribution and notice conditions; dependencies retain their own terms. Neither label alone establishes licensing for Android images, Google services, ARM translation, or a complete product bundle. [QEMU license](https://raw.githubusercontent.com/qemu/qemu/master/LICENSE), [crosvm license](https://github.com/google/crosvm/blob/main/LICENSE).

Google's SDK agreement distinguishes SDK restrictions from separately licensed open-source components. AOSP itself uses multiple licenses. An installed SDK/system image is not a blanket permission to redistribute a consumer emulator bundle. Keep the present reference external and perform a component-level review if choosing components for distribution. [SDK agreement, sections 3.4 and 3.5](https://developer.android.com/studio/terms), [AOSP licensing overview](https://source.android.com/docs/setup/contribute/licenses).

## Recommended next experiment

Use the already installed Android Emulator as an external reference. Create a fresh project-owned Android 35 device, request WHPX and host graphics, disable snapshots, and capture bounded boot/package readiness, renderer diagnostics, guest ABI/bridge properties, a screenshot, and graceful shutdown. Preserve failures. This validates the measurement/lifecycle procedure and gathers Android observations without suggesting that we have recreated AOW.

Then add a Rust host controller around an explicitly configured external engine. Keep preparation separate from launch so routine startup does not install packages, accept licenses, or modify existing user devices. After the reference works, investigate a pinned crosvm WHPX build and matching guest/gfxstream combination. QEMU remains an alternative if the crosvm combination cannot be made reproducible. Product selection waits for target ABI evidence, graphics correctness, game tests, and low-end captures.

The direct-execution path remains an independent research branch with a much larger Android compatibility gap. Prioritize a representative Android dependency probe over extending its instruction set without an application-driven need.
