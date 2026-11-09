//! File Content Export: finding files by extension, filtering, `///` comment handling and the
//! export file.
//!
//! Character counts are UTF-16 code units.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::directory::{IgnoredFolderNames, display_name};
use crate::file_name::output_path;
use crate::folder_tree::FolderScope;
use crate::text::{LINE_ENDING, UTF8_BOM, read_text_lines, utf16_len};

/// A file below the source folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub full_path: String,
    /// Path relative to the source folder, with platform separators.
    pub relative_path: String,
    pub file_name: String,
    /// Extension including the dot, see [`extension_of`].
    pub extension: String,
}

/// The extension of a file name: the part from the last dot,
/// including the dot. `.gitignore` has the extension `.gitignore`, and a trailing dot gives no
/// extension.
pub fn extension_of(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(index) if index + 1 < file_name.len() => file_name[index..].to_owned(),
        _ => String::new(),
    }
}

/// Case-insensitive key used for extensions and relative paths.
pub fn ignore_case_key(text: &str) -> String {
    text.to_lowercase()
}

/// Lists all files of the selected folders. Recursive scopes skip ignored folder names at every
/// level, including the scope folder itself. Unreadable folders are skipped.
pub fn collect_files(
    source_path: &Path,
    scopes: &[FolderScope],
    ignored: &IgnoredFolderNames,
) -> Vec<FileEntry> {
    let mut files = Vec::new();
    for scope in scopes {
        if !scope.path.is_dir() {
            continue;
        }
        if scope.recursive {
            let mut stack = vec![scope.path.clone()];
            while let Some(directory) = stack.pop() {
                if ignored.contains(&directory) {
                    continue;
                }
                let Ok(entries) = fs::read_dir(&directory) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else {
                        files.push(file_entry(source_path, path));
                    }
                }
            }
        } else if let Ok(entries) = fs::read_dir(&scope.path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    files.push(file_entry(source_path, path));
                }
            }
        }
    }
    files.sort_by(|a, b| compare_paths(&a.full_path, &b.full_path));
    files
}

fn file_entry(source_path: &Path, path: PathBuf) -> FileEntry {
    let file_name = display_name(&path);
    let relative_path = path
        .strip_prefix(source_path)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    FileEntry {
        extension: extension_of(&file_name),
        full_path: path.to_string_lossy().into_owned(),
        relative_path,
        file_name,
    }
}

/// Orders paths ignoring case, with an ordinal tie-break.
pub fn compare_paths(a: &str, b: &str) -> std::cmp::Ordering {
    ignore_case_key(a)
        .cmp(&ignore_case_key(b))
        .then_with(|| a.cmp(b))
}

/// Distinct extensions of the files, sorted. The first spelling of an extension is kept.
pub fn available_extensions(files: &[FileEntry]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut extensions: Vec<String> = files
        .iter()
        .filter(|file| !file.extension.is_empty())
        .filter(|file| seen.insert(ignore_case_key(&file.extension)))
        .map(|file| file.extension.clone())
        .collect();
    extensions.sort_by(|a, b| compare_paths(a, b));
    extensions
}

/// Replaces both separator characters with the platform separator, so typed paths match on every
/// platform.
pub fn normalize_separators(text: &str) -> String {
    let separator = std::path::MAIN_SEPARATOR;
    text.chars()
        .map(|c| if c == '/' || c == '\\' { separator } else { c })
        .collect()
}

/// The search box filter: space separated terms that must all occur in the full path, ignoring
/// case. A term starting with `-` excludes paths that contain the rest of the term.
#[derive(Debug, Clone, Default)]
pub struct PathFilter {
    include: Vec<String>,
    exclude: Vec<String>,
}

impl PathFilter {
    pub fn new(text: &str) -> Self {
        let mut filter = Self::default();
        for term in text.split(' ').filter(|term| !term.is_empty()) {
            match term.strip_prefix('-') {
                Some(rest) if !rest.is_empty() => filter
                    .exclude
                    .push(ignore_case_key(&normalize_separators(rest))),
                _ => filter
                    .include
                    .push(ignore_case_key(&normalize_separators(term))),
            }
        }
        filter
    }

    pub fn matches(&self, full_path: &str) -> bool {
        let path = ignore_case_key(full_path);
        self.include.iter().all(|term| path.contains(term.as_str()))
            && !self.exclude.iter().any(|term| path.contains(term.as_str()))
    }
}

/// Byte index of the `//` that starts a comment, ignoring `//` inside quotes and after `:`
/// (as in `http://`).
pub fn find_comment_index(line: &str) -> Option<usize> {
    let mut in_double = false;
    let mut in_single = false;
    let mut previous: Option<char> = None;
    let mut chars = line.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        let escaped = previous == Some('\\');
        if c == '"' && !escaped {
            in_double = !in_double;
        } else if c == '\'' && !escaped {
            in_single = !in_single;
        }
        if !in_double
            && !in_single
            && c == '/'
            && chars.peek().is_some_and(|(_, next)| *next == '/')
            && previous != Some(':')
        {
            return Some(index);
        }
        previous = Some(c);
    }
    None
}

/// The byte index where a `///` documentation comment starts, if the line has one.
pub fn xml_comment_start(line: &str) -> Option<usize> {
    let index = find_comment_index(line)?;
    line[index..].starts_with("///").then_some(index)
}

/// Total length of `///` comments in a file, in UTF-16 code units.
pub fn count_xml_comment_chars(path: &Path) -> io::Result<u64> {
    Ok(read_text_lines(path)?
        .iter()
        .filter_map(|line| xml_comment_start(line).map(|start| utf16_len(&line[start..])))
        .sum())
}

#[derive(Debug, Clone)]
pub struct FileExportRequest {
    pub destination_path: PathBuf,
    /// Output file name without extension.
    pub file_name: String,
    /// Files in export order.
    pub files: Vec<FileEntry>,
    /// Relative paths, compared ignoring case, whose `///` comments are removed.
    pub ignore_comments_for: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileExportResult {
    pub output_path: PathBuf,
    pub exported_lines: u64,
    pub exported_characters: u64,
    pub files_with_ignored_comments: u64,
    pub ignored_lines: u64,
    pub ignored_characters: u64,
}

/// Writes the selected files into one text file: a header per file, the trimmed lines, and a
/// separator between files. Unreadable files get an error line instead of their content.
pub fn export_file_contents(request: &FileExportRequest) -> io::Result<FileExportResult> {
    if request.files.is_empty() {
        return Err(io::Error::other("No files were selected for export."));
    }
    let output_path = output_path(&request.destination_path, &request.file_name);
    let mut writer = ExportWriter {
        out: BufWriter::new(File::create(&output_path)?),
        result: FileExportResult::default(),
    };
    writer.out.write_all(UTF8_BOM)?;

    let ignored: HashSet<String> = request
        .ignore_comments_for
        .iter()
        .map(|path| ignore_case_key(path))
        .collect();

    for (index, file) in request.files.iter().enumerate() {
        writer.line(&format!("=== {} file contains:", file.relative_path))?;
        writer.line("")?;
        let ignore_comments = ignored.contains(&ignore_case_key(&file.relative_path));
        if ignore_comments {
            writer.result.files_with_ignored_comments += 1;
        }
        match read_text_lines(Path::new(&file.full_path)) {
            Ok(lines) => {
                for line in &lines {
                    writer.content_line(line, ignore_comments)?;
                }
                if index + 1 < request.files.len() {
                    writer.line("")?;
                    writer.line("===========")?;
                }
            }
            Err(error) => {
                writer.line(&format!(
                    "--- Could not read file: {}. Error: {error} ---",
                    file.file_name
                ))?;
                writer.line("")?;
            }
        }
    }
    writer.out.flush()?;
    let mut result = writer.result;
    result.output_path = output_path;
    Ok(result)
}

struct ExportWriter<W: Write> {
    out: W,
    result: FileExportResult,
}

impl<W: Write> ExportWriter<W> {
    fn line(&mut self, text: &str) -> io::Result<()> {
        self.out.write_all(text.as_bytes())?;
        self.out.write_all(LINE_ENDING.as_bytes())?;
        self.result.exported_lines += 1;
        self.result.exported_characters += utf16_len(text) + utf16_len(LINE_ENDING);
        Ok(())
    }

    /// Writes a source line without trailing whitespace. With `ignore_comments`, a `///` comment
    /// is removed and a line that becomes empty is skipped.
    fn content_line(&mut self, line: &str, ignore_comments: bool) -> io::Result<()> {
        if ignore_comments && let Some(start) = xml_comment_start(line) {
            self.result.ignored_characters += utf16_len(&line[start..]);
            self.result.ignored_lines += 1;
            let kept = line[..start].trim_end();
            if kept.is_empty() {
                return Ok(());
            }
            return self.line(kept);
        }
        self.line(line.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_extensions() {
        assert_eq!(extension_of("main.rs"), ".rs");
        assert_eq!(extension_of("archive.tar.gz"), ".gz");
        assert_eq!(extension_of(".gitignore"), ".gitignore");
        assert_eq!(extension_of("Makefile"), "");
        assert_eq!(extension_of("trailing."), "");
    }

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src/deep")).unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("README.md"), "readme").unwrap();
        fs::write(root.join("src/main.RS"), "fn main() {}").unwrap();
        fs::write(root.join("src/deep/lib.rs"), "").unwrap();
        fs::write(root.join("bin/app.exe"), "").unwrap();
        dir
    }

    #[test]
    fn collects_recursive_and_flat_scopes() {
        let dir = sample();
        let ignored = IgnoredFolderNames::new(["bin"]);
        let recursive = [FolderScope {
            path: dir.path().to_path_buf(),
            recursive: true,
        }];
        let files = collect_files(dir.path(), &recursive, &ignored);
        let names: Vec<_> = files.iter().map(|file| file.file_name.as_str()).collect();
        assert_eq!(names, ["README.md", "lib.rs", "main.RS"]);
        assert_eq!(available_extensions(&files), [".md", ".rs"]);

        let flat = [FolderScope {
            path: dir.path().to_path_buf(),
            recursive: false,
        }];
        let files = collect_files(dir.path(), &flat, &ignored);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "README.md");
    }

    #[test]
    fn path_filter_supports_exclusions() {
        let filter = PathFilter::new("src -deep");
        let sep = std::path::MAIN_SEPARATOR;
        assert!(filter.matches(&format!("C:{sep}p{sep}SRC{sep}main.rs")));
        assert!(!filter.matches(&format!("C:{sep}p{sep}src{sep}deep{sep}lib.rs")));
        assert!(PathFilter::new("src/deep").matches(&format!("x{sep}src{sep}deep{sep}y")));
        assert!(PathFilter::new("  ").matches("anything"));
    }

    #[test]
    fn finds_comments_outside_strings_and_urls() {
        assert_eq!(find_comment_index("let a = 1; // note"), Some(11));
        assert_eq!(find_comment_index("let s = \"// not\";"), None);
        assert_eq!(find_comment_index("url = http://example.com"), None);
        assert_eq!(find_comment_index("let c = '\\''; // after"), Some(14));
        assert_eq!(xml_comment_start("    /// doc"), Some(4));
        assert_eq!(xml_comment_start("    // plain"), None);
    }

    fn export(
        dir: &tempfile::TempDir,
        files: Vec<FileEntry>,
        ignore: Vec<String>,
    ) -> (String, FileExportResult) {
        let result = export_file_contents(&FileExportRequest {
            destination_path: dir.path().to_path_buf(),
            file_name: "out".to_owned(),
            files,
            ignore_comments_for: ignore,
        })
        .unwrap();
        let bytes = fs::read(&result.output_path).unwrap();
        assert!(bytes.starts_with(UTF8_BOM));
        let text = String::from_utf8(bytes[UTF8_BOM.len()..].to_vec()).unwrap();
        (text.replace("\r\n", "\n"), result)
    }

    fn entry(dir: &tempfile::TempDir, relative: &str) -> FileEntry {
        file_entry(dir.path(), dir.path().join(relative))
    }

    #[test]
    fn exports_files_with_headers_and_separators() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.cs"), "line one  \r\nline two\n").unwrap();
        fs::write(dir.path().join("b.cs"), "only").unwrap();
        let (text, result) = export(&dir, vec![entry(&dir, "a.cs"), entry(&dir, "b.cs")], vec![]);
        assert_eq!(
            text,
            "=== a.cs file contains:\n\nline one\nline two\n\n===========\n=== b.cs file contains:\n\nonly\n"
        );
        assert_eq!(result.exported_lines, 9);
        let newline = utf16_len(LINE_ENDING);
        let expected_chars = [23, 0, 8, 8, 0, 11, 23, 0, 4]
            .iter()
            .map(|len| len + newline)
            .sum::<u64>();
        assert_eq!(result.exported_characters, expected_chars);
    }

    #[test]
    fn removes_xml_comments_for_selected_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("a.cs"),
            "/// <summary>\nint x; /// trailing\nint y; // kept\n",
        )
        .unwrap();
        let (text, result) = export(&dir, vec![entry(&dir, "a.cs")], vec!["A.CS".to_owned()]);
        assert_eq!(text, "=== a.cs file contains:\n\nint x;\nint y; // kept\n");
        assert_eq!(result.files_with_ignored_comments, 1);
        assert_eq!(result.ignored_lines, 2);
        assert_eq!(result.ignored_characters, 13 + 12);
    }

    #[test]
    fn unreadable_file_gets_an_error_line() {
        let dir = tempfile::tempdir().unwrap();
        let mut missing = entry(&dir, "gone.txt");
        missing.file_name = "gone.txt".to_owned();
        let (text, _) = export(&dir, vec![missing], vec![]);
        assert!(text.starts_with(
            "=== gone.txt file contains:\n\n--- Could not read file: gone.txt. Error: "
        ));
        assert!(text.ends_with(" ---\n\n"));
    }

    #[test]
    fn counts_xml_comment_characters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.cs");
        fs::write(&path, "/// ab\nx // no\n  ///c\n").unwrap();
        assert_eq!(count_xml_comment_chars(&path).unwrap(), 6 + 4);
    }
}
