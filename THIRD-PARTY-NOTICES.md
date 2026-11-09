# Third-Party Notices

FileCraft uses the following third-party components. Runtime dependencies must be licensed under MIT or Apache-2.0, or dual licensed with one of them, except for the permissive exceptions listed below.

## Runtime Libraries

`Cargo.lock` lists the exact versions of all transitive crates.

| Component | Version | License | Source |
| --- | --- | --- | --- |
| eframe, egui | 0.37 | MIT OR Apache-2.0 | https://github.com/emilk/egui |
| fontique | 0.11 | Apache-2.0 OR MIT | https://github.com/linebender/parley |
| rfd | 0.17 | MIT | https://github.com/PolyMeilex/rfd |
| image | 0.25 | MIT OR Apache-2.0 | https://github.com/image-rs/image |
| chrono | 0.4 | MIT OR Apache-2.0 | https://github.com/chronotope/chrono |
| lofty | 0.25 | MIT OR Apache-2.0 | https://github.com/Serial-ATA/lofty-rs |

Transitive crates with other permissive licenses:

| Component | License | Used by | Source |
| --- | --- | --- | --- |
| foldhash | Zlib | hashbrown, used by egui and accesskit | https://github.com/orlp/foldhash |
| libloading | ISC | OpenGL loading | https://github.com/nagisa/rust_libloading |
| clipboard-win, error-code | BSL-1.0 | Windows clipboard access | https://github.com/DoumanAsh/clipboard-win |

`self_cell` is dual licensed and used under Apache-2.0. `byteorder` and `memchr` are dual licensed and used under MIT.

The application does not bundle the egui default fonts. It uses the operating system UI font at runtime.

## Bundled Assets

| Component | License | Source |
| --- | --- | --- |
| Material Icons font | Apache-2.0 | https://github.com/google/material-design-icons |

## Project Assets

Application-specific assets that are not listed above, such as the application icon, are FileCraft project assets and are covered by the project license.
