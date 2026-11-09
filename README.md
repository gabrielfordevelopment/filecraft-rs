# FileCraft (Rust)

[README](README.md) | [CHANGELOG](CHANGELOG.md)

FileCraft is a free and open-source desktop utility that helps developers and power users document project structures and bundle source code for analysis, AI prompts, or documentation purposes.

This is a rewrite of the earlier FileCraft WPF application in Rust. Windows is the supported platform, and the code is kept portable so other platforms can follow later.

FileCraft is a highly specific tool that I made for my own needs. Its features and decisions reflect my workflow, not a general-purpose design. I share it openly because parts of it may be useful to others. Suggestions are welcome, but new features follow my own needs first.

## Features

- **Tree Generator**: writes the structure of a folder as a tree or as a list of paths.
- **File Content Export**: bundles the contents of the selected files into one text file, with extension selection, path filters, bulk search, path presets, and optional removal of `///` comments.
- **Folder Content Export**: lists the files of folders with their details and audio tags in a delimited text file.
- **File Rename**: renames photos and videos by date or numbers the files of a folder, with a preview before renaming and an undo afterwards.
- **Options**: setting presets, copying and linking folder trees between tabs, and ignored folders.
- Saved settings with undo and redo, and a light and a dark theme.

## Build and run

Install the stable Rust toolchain and the MSVC C++ build tools with the Windows SDK, then run:

```text
cargo run --release
```

## Layout

| Crate | Purpose |
| --- | --- |
| `crates/filecraft-core` | Logic without UI: folder trees, file search and exports, file renaming, saved settings and presets, undo history |
| `crates/filecraft` | The desktop application |

## Checks

```text
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

## License

FileCraft is licensed under the MIT License. See [LICENSE](LICENSE). Third-party component licenses are listed in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
