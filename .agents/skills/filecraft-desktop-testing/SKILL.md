---
name: filecraft-desktop-testing
description: Validate the FileCraft (Rust) application with unit tests and, when the maintainer hands over the machine, visible test sessions with isolated settings and disposable sample folders.
---

# Desktop testing

Read [AGENTS.md](../../../AGENTS.md) for the layout and validation requirements.

## Isolate the session

- Never move the mouse, click, send keys, use the clipboard, change window focus, or open visible windows unless the maintainer explicitly hands over the machine. Tell them as soon as you are done.
- Prefer seeding the isolated `save.json` over driving native file dialogs.
- Start the application with `--settings-dir <folder>` so tests never read or write the user's settings.
- Use a disposable sample folder outside the repository as the source and a disposable output folder as the destination. Do not point exports at personal folders or overwrite files the task did not create.
- Do not modify the user's saved settings or presets.
- Use sample trees with nested and empty folders, files at several levels, long names, names that differ only in case, non-ASCII names, and an ignored folder name.

## Exercise and report

1. Prefer unit tests in `filecraft-core` for logic that does not need a window.
2. For UI checks, capture screenshots of the confirmed window. Cover the affected states: hover, pressed, checked, indeterminate and disabled controls, resizing, and fullscreen panels.
3. Keep screenshots limited to useful, confirmed states.
4. Stop only task-owned processes and delete only task-owned sample data after use.
5. Report the build, the sample data, the actions, the observed results, and the gaps.
