//! Bulk search over relative paths.

use crate::file_content::{compare_paths, ignore_case_key, normalize_separators};

/// The result of matching the input lines against the files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkMatch {
    /// Number of matching files for each input line, `None` for lines without a match.
    pub counts: Vec<Option<usize>>,
    /// Indices of all matched files, ordered by relative path.
    pub matched: Vec<usize>,
    /// Number of input lines with at least one match.
    pub lines_with_matches: usize,
}

/// Splits input text into lines at `\r\n`, `\r` or `\n`, keeping empty lines.
fn input_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                lines.push(&text[start..index]);
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
                start = index + 1;
            }
            b'\n' => {
                lines.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    lines.push(&text[start..]);
    lines
}

/// Each trimmed input line matches the relative paths that contain it, ignoring case.
pub fn match_lines(text: &str, relative_paths: &[String]) -> BulkMatch {
    let keys: Vec<String> = relative_paths
        .iter()
        .map(|path| ignore_case_key(path))
        .collect();
    let mut matched = vec![false; relative_paths.len()];
    let mut result = BulkMatch::default();
    for line in input_lines(text) {
        let term = ignore_case_key(&normalize_separators(line.trim()));
        let mut count = 0;
        if !term.is_empty() {
            for (index, key) in keys.iter().enumerate() {
                if key.contains(term.as_str()) {
                    count += 1;
                    matched[index] = true;
                }
            }
        }
        if count > 0 {
            result.lines_with_matches += 1;
        }
        result.counts.push((count > 0).then_some(count));
    }
    result.matched = (0..relative_paths.len())
        .filter(|index| matched[*index])
        .collect();
    result
        .matched
        .sort_by(|a, b| compare_paths(&relative_paths[*a], &relative_paths[*b]));
    result
}

/// Removes all whitespace from every line and drops empty lines.
pub fn remove_whitespaces(text: &str) -> String {
    text.split(['\r', '\n'])
        .map(|line| {
            line.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Splits every line at `delimiter` and puts each trimmed, non-empty part on its own line.
pub fn break_into_lines(text: &str, delimiter: char) -> String {
    text.split(['\r', '\n'])
        .flat_map(|line| line.split(delimiter))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Vec<String> {
        let sep = std::path::MAIN_SEPARATOR;
        vec![
            format!("src{sep}main.rs"),
            format!("src{sep}lib.rs"),
            "README.md".to_owned(),
        ]
    }

    #[test]
    fn counts_matches_per_line() {
        let result = match_lines("src/\n\n  readme \nnothing", &paths());
        assert_eq!(result.counts, [Some(2), None, Some(1), None]);
        assert_eq!(result.lines_with_matches, 2);
        // Ordered by relative path: README.md, src/lib.rs, src/main.rs.
        assert_eq!(result.matched, [2, 1, 0]);
    }

    #[test]
    fn empty_input_has_one_empty_line() {
        let result = match_lines("", &paths());
        assert_eq!(result.counts, [None]);
        assert!(result.matched.is_empty());
    }

    #[test]
    fn removes_whitespace_and_empty_lines() {
        assert_eq!(remove_whitespaces(" a b \r\n\r\n c\t"), "ab\nc");
    }

    #[test]
    fn breaks_lines_at_delimiter() {
        assert_eq!(break_into_lines("a b  c\nd", ' '), "a\nb\nc\nd");
        assert_eq!(break_into_lines("a; b ;;c", ';'), "a\nb\nc");
    }
}
