---
name: filecraft-code-review
description: Review FileCraft (Rust) diffs or pull requests for correctness, output format stability, saved-data compatibility, UI regressions, platform portability, dependency licensing and merge readiness.
---

# Code review

Inspect the complete relevant diff, its base, surrounding callers and tests. A review alone does not authorize modifying code. Prioritize concrete user impact over stylistic preferences.

- Check that changed behavior keeps the existing wording, defaults, validation, and selection rules unless the change is intended, and that output files keep their format (encoding and BOM, line endings, ordering, separators, excluded and ignored folders, error handling).
- For saved data, check that existing save files still load, that malformed data is not silently replaced, and that failed writes keep the previous file.
- For UI changes, check that the look follows the look and feel rules in [AGENTS.md](../../../AGENTS.md), that strings live in `strings.rs`, that logic stays in `filecraft-core`, and that long file system work does not block the UI thread.
- Check portability: `std::path` usage, `path_key` comparisons, no hardcoded separators or drive letters, and Windows-only code behind `cfg(windows)`.
- Verify that dependencies are necessary, correctly licensed including transitive crates and enabled features, and listed in `THIRD-PARTY-NOTICES.md`.
- Check written text for the punctuation and attribution rules in [AGENTS.md](../../../AGENTS.md).
- Require unit tests for new `filecraft-core` logic.

Report findings in severity order with impact, a precise file and line, a practical remedy, and whether they block merge. Finish with a clear readiness verdict. If no findings remain, say so.
