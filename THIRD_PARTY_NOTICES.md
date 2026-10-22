# Third-party components

| Component | Version | License | Notice |
| --- | --- | --- | --- |
| [iced-x86](https://github.com/icedland/iced) | 1.21.0 | MIT | [Upstream license](third_party/iced-x86-LICENSE.txt) |
| [lazy_static](https://github.com/rust-lang-nursery/lazy-static.rs) | 1.5.1 | MIT OR Apache-2.0; MIT notice retained here | [Package MIT license](third_party/lazy_static-LICENSE-MIT.txt) |
| [windows-sys](https://github.com/microsoft/windows-rs) | 0.61.2 | MIT OR Apache-2.0; MIT notice retained here | [Package MIT license](third_party/windows-sys-LICENSE-MIT.txt) |
| [windows-link](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0; MIT notice retained here | [Package MIT license](third_party/windows-link-LICENSE-MIT.txt) |

| serde | 1.0.229 | MIT OR Apache-2.0 | [MIT](third_party/serde-LICENSE-MIT.txt) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [MIT](third_party/serde_core-LICENSE-MIT.txt) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [MIT](third_party/serde_derive-LICENSE-MIT.txt) |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | [MIT](third_party/serde_json-LICENSE-MIT.txt) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [MIT](third_party/itoa-LICENSE-MIT.txt) |
| memchr | 2.8.3 | Unlicense OR MIT | [MIT](third_party/memchr-LICENSE-MIT.txt) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [MIT](third_party/proc-macro2-LICENSE-MIT.txt) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [MIT](third_party/quote-LICENSE-MIT.txt) |
| syn | 3.0.9 | MIT OR Apache-2.0 | [MIT](third_party/syn-LICENSE-MIT.txt) |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | [MIT](third_party/unicode-ident-LICENSE-MIT.txt), [Unicode](third_party/unicode-ident-LICENSE-UNICODE.txt) |
| zmij | 1.0.23 | MIT | [MIT](third_party/zmij-LICENSE-MIT.txt) |

Versions and registry checksums are recorded in Cargo.lock. MIT is selected where offered as an alternative above. iced-x86 is enabled only for its standard-library and decoder features. The presence of a decoder does not imply support for executing every instruction it recognizes.

Windows bindings are target-specific and cover owned processes/jobs/threads and the native desktop prototype's window, controls, font, and DPI APIs. Serde and serde_json handle bounded library metadata. External Android SDK/NDK tools and reference system images are development prerequisites; they are not copied into this repository or bundled with the Rust executable. Their own component notices and terms apply separately.
