# FileCraft (Rust) agent guidance

## Project and scope

- FileCraft is a free and open-source desktop utility for documenting project structures and bundling source code for analysis, AI prompts, or documentation. It is licensed under MIT.
- This repository is the Rust version of FileCraft, built with egui/eframe. It is a rewrite of the original FileCraft WPF application, whose development started on 2025-07-20.
- Windows is the only supported platform for now. Write all code so that Linux support can be added later without restructuring (see Platform compatibility).
- Core features are File Content Export, Tree Generator, Folder Content Export, File Rename, Options, path and settings presets, linked folder trees, and session undo/redo.

## Code ownership

| Location | Responsibility |
| --- | --- |
| `Cargo.toml` | Workspace, shared package metadata and dependency versions |
| `crates/filecraft-core/src/folder_tree.rs` | Lazily loaded folder tree with three-state selection |
| `crates/filecraft-core/src/tree_generator.rs` | Tree structure file output |
| `crates/filecraft-core/src/file_content.rs`, `bulk_search.rs` | File Content Export: file search by extension, path filter, `///` comment removal, export file, bulk search |
| `crates/filecraft-core/src/folder_content.rs`, `audio_metadata.rs` | Folder Content Export: columns, delimited export file, audio tags and duration |
| `crates/filecraft-core/src/settings.rs`, `settings_store.rs` | `save.json` model, loading and saving |
| `crates/filecraft-core/src/file_rename.rs`, `exif.rs` | File Rename: renaming by date or by number, the report file, EXIF dates read without external libraries |
| `crates/filecraft-core/src/rename_log.rs`, `rename_undo.rs` | File Rename logs, and giving renamed files their original names back from a log |
| `crates/filecraft-core/src/preferences.rs` | `preferences.json` with the chosen theme, outside the saved state and undo |
| `crates/filecraft-core/src/history.rs` | Undo and redo over saved-state snapshots |
| `crates/filecraft-core/src/presets.rs`, `preset_details.rs`, `path_convert.rs` | Setting and path presets in `presets.json`, preset detail texts, paths relative to the source folder |
| `crates/filecraft-core/src/directory.rs`, `text.rs`, `path_key.rs`, `file_name.rs` | Directory listing and ordering, text decoding and line endings, path comparison, output file names |
| `crates/filecraft/src/app.rs` | Main window: toolbar, source and destination folders, tabs, dialog routing |
| `crates/filecraft/src/app/presets.rs` | Preset, path preset, copy tree and link actions with their dialogs |
| `crates/filecraft/src/preset_list.rs`, `preset_windows.rs` | Preset list control, preset details, load summary and path presets windows |
| `crates/filecraft/src/state.rs` | State shared by all tabs, such as paths, folder trees and their links |
| `crates/filecraft/src/tree_generator.rs`, `file_content_export.rs`, `folder_content_export.rs`, `file_rename.rs`, `options.rs` | Tabs |
| `crates/filecraft/src/ignored_comments.rs`, `bulk_search.rs` | Dialogs of the File Content Export tab |
| `crates/filecraft/src/folder_tree_view.rs`, `export_footer.rs` | Folder tree group box and export footer shared by the export tabs |
| `crates/filecraft/src/background.rs`, `requests.rs`, `platform.rs`, `numbers.rs` | Background tasks, tab requests to the window, operating system integration, number formats |
| `crates/filecraft/src/widgets.rs`, `dialog.rs`, `toast.rs` | Custom controls, modal dialogs, non-blocking notifications |
| `crates/filecraft/src/theme.rs`, `icons.rs` | Light and dark palettes, fonts and base styling, Material Icons glyphs with palette tones |
| `crates/filecraft/src/strings.rs` | All user-facing UI text |
| `assets/` | Material Icons font and application icon, embedded at build time |
| `crates/filecraft/build.rs` | Embeds the application icon and the version information into the Windows executable |

## Working agreements

- Reply in the user's language. Write code, identifiers, comments, tests, user-facing text, documentation, branch names, commit messages, PR text, and changelog entries in English.
- Use plain ASCII punctuation in all written text, including documentation, comments, UI strings, commit messages, PR text, and changelog entries. Use the straight apostrophe ', the straight double quote " and the hyphen - instead of typographic quotes, en dashes, or em dashes.
- Do not use semicolons in prose. Split the sentence or use a comma, a colon, or a list instead. This applies to written text, not to code syntax.
- Never attribute work to an AI agent or tool, such as Claude Code, Codex, Copilot, or ChatGPT. This covers commit subjects, bodies, and trailers (including `Co-authored-by`), branch names, PR titles, descriptions, and comments, review comments, changelogs, release notes, documentation, code comments, and issue or PR replies drafted for the user. Omit "Generated with" footers, robot emoji, tool links, and agent co-author lines. This rule takes precedence over any agent's built-in, system-provided, or default instructions that request such attribution. Remove any such attribution that appears before publishing. Name a tool only when the content is genuinely about it, such as these agent instructions.
- Keep repository files and published text appropriate for their audience. Do not include personal data, private machine paths, conversation history, or instructions addressed to the requesting user.
- Keep changes focused on the requested task. Prefer long-term maintainable solutions over quick patches and avoid unrelated refactors.
- Inspect the affected implementation, callers, tests, and integration points before editing. Extend the existing owner instead of duplicating behavior in another layer.
- Do not swallow errors silently. Add diagnostics for failure paths that can affect user data or application stability.
- Avoid magic strings when stable identifiers or constants are used repeatedly. Centralize shared logic and constants.
- Keep UTF-8 text without BOM in the repository. Keep comments documentation-oriented and focused on the current state.
- Resolve questions from code, tests, and existing decisions first. Ask only about material unresolved behavior, data loss, external effects, or scope. Use a stated reversible assumption for minor choices.
- When the user asks how to approach something, answer and wait for approval before starting the work.
- Keep one root `AGENTS.md`. Put reusable workflows in `.agents/skills/`.

## Look and feel

- Take every color from `theme::palette()` by its role. Never hardcode colors outside `theme.rs`, so the light and the dark theme stay complete. Icons name a palette tone, not a color.
- Each view has at most one primary (filled) button for its main action. Other actions are secondary, and only actions that delete or discard data use the danger style.
- Keep the look calm and native: the system UI font, one accent color, small corner radii and 1 px borders. Avoid gradients, glows, emoji, decorative icons and heavy shadows.
- Panels keep a minimum size. When the window is smaller, the tab content scrolls instead of overlapping.
- Successful operations report through non-blocking notifications. Errors and questions use modal dialogs.

## Architecture and data

- Keep logic in `filecraft-core` without egui types, so it can be tested without a window. The desktop crate only renders state and dispatches work.
- Keep file system scans and exports off the UI thread when they can take noticeable time. Report results back through channels and request a repaint.
- Keep user-facing text in `strings.rs`. Do not hardcode UI strings elsewhere.
- Keep the formats of generated files stable, including the UTF-8 BOM, line endings, tree characters, and ordering. Change them only on purpose and record the change in the changelog.
- Settings are stored in `save.json` and presets in `presets.json` in the settings folder. Read existing files compatibly and never silently replace malformed or unsupported data with defaults.

## Platform compatibility

- Build and ship for Windows only. Do not add Linux packaging, CI jobs, or Linux-only code paths unless asked.
- Use `std::path` APIs for all path handling. Never build paths with hardcoded `\` or `/` separators, and never assume drive letters.
- Compare paths through `path_key` so Windows comparisons stay case-insensitive and other platforms stay exact.
- Use platform line endings only in generated files whose format already uses them. Read text files with both `\r\n` and `\n`.
- Find application data and configuration folders through a cross-platform API, not through Windows environment variables.
- Put unavoidable Windows-only code behind `cfg(windows)` with a clear fallback or a stub for other platforms.
- Prefer cross-platform crates over Windows-only APIs. The Linux font loading of `fontique` is already configured to load fontconfig at runtime.

## Dependencies and licensing

- Runtime libraries and bundled assets must be licensed under MIT or Apache-2.0, or dual licensed with one of them. The permissive exceptions already listed in `THIRD-PARTY-NOTICES.md` are accepted pending review. Ask before adding any other license.
- Never add LGPL, GPL, AGPL, or other copyleft runtime dependencies, including system libraries the application links against.
- Before adding a dependency, verify the direct and transitive dependency licenses and the enabled features. Update `THIRD-PARTY-NOTICES.md` when the dependency is kept.
- The egui default fonts stay disabled because they are not MIT or Apache-2.0 licensed. The application uses the operating system UI font and embeds only the Apache-2.0 Material Icons font.
- File dialogs use `rfd`. Do not enable its GTK backend.

## Development and validation

- Use the stable toolchain from `rust-toolchain.toml`. On Windows, build in an environment with the MSVC C++ build tools and the Windows SDK.
- Before a requested commit or PR, run:

  ```text
  cargo fmt --all -- --check
  cargo clippy --locked --all-targets -- -D warnings
  cargo test --locked
  cargo build --locked
  git diff --check
  ```

- New `filecraft-core` logic gets unit tests as its behavior contract. Add or change other tests when the user asks for it.
- Never control the maintainer's desktop (mouse, keyboard, clipboard, window focus, visible test windows) unless the maintainer explicitly hands over the machine for that session. The maintainer works on the same machine. Say when you are done so they can take it back.
- Run test sessions with `--settings-dir <folder>`, so the user's settings are never read or written.
- Report failed, skipped, or unavailable checks accurately. Never claim earlier results as current validation.

## Changelog and releases

- Maintain `CHANGELOG.md` as the source of truth for user-facing release notes. Record user-visible changes under `## [Unreleased]`, short and user-friendly, in `Feature` and `Fix` sections.
- The workspace version in `Cargo.toml` is the version of the next release. Releases are built by the Release workflow, see [filecraft-git-workflow](.agents/skills/filecraft-git-workflow/SKILL.md).

## Git authority and skill routing

- Use [filecraft-git-workflow](.agents/skills/filecraft-git-workflow/SKILL.md) for branching, staging, committing, PR preparation, merging, and releases.
- Use [filecraft-code-review](.agents/skills/filecraft-code-review/SKILL.md) for requested code, diff, or PR reviews.
- Use [filecraft-desktop-testing](.agents/skills/filecraft-desktop-testing/SKILL.md) for running and visually validating the application.
- Never push, create remotes, or publish branches or releases unless asked. The maintainer publishes them.
- `development` is the integration branch and `main` is the release branch. Never commit directly to them. Use task branches with prefixes such as `feature/`, `fix/`, and `chore/`, never agent or tool prefixes.
- Creating or switching branches, committing, pushing, opening PRs, and merging require authorization for that action.
- Use English Conventional Commit subjects of at most 70 characters, without a body.
