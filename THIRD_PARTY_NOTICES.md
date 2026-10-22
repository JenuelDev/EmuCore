# Third-party components

| Component | Version | License | Notice |
| --- | --- | --- | --- |
| [iced-x86](https://github.com/icedland/iced) | 1.21.0 | MIT | [Upstream license](third_party/iced-x86-LICENSE.txt) |
| [lazy_static](https://github.com/rust-lang-nursery/lazy-static.rs) | 1.5.1 | MIT OR Apache-2.0; MIT notice retained here | [Package MIT license](third_party/lazy_static-LICENSE-MIT.txt) |
| [windows-sys](https://github.com/microsoft/windows-rs) | 0.61.2 | MIT OR Apache-2.0; MIT notice retained here | [Package MIT license](third_party/windows-sys-LICENSE-MIT.txt) |
| [windows-link](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0; MIT notice retained here | [Package MIT license](third_party/windows-link-LICENSE-MIT.txt) |

Versions and registry checksums are recorded in Cargo.lock. iced-x86 is enabled only for its standard-library and decoder features. The presence of a decoder does not imply support for executing every instruction it recognizes.

Windows bindings are target-specific and enabled only for the process/job/thread APIs used by the host lifecycle boundary. External Android SDK/NDK tools and reference system images are development prerequisites; they are not copied into this repository or bundled with the Rust executable. Their own component notices and terms apply separately.
