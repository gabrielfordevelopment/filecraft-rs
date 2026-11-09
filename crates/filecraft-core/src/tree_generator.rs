//! Tree structure export, written as UTF-8 with BOM and platform line endings.

use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::directory::{IgnoredFolderNames, display_name, list_directory};
use crate::file_name::output_path;
use crate::path_key::path_key;
use crate::text::{LINE_ENDING, UTF8_BOM};

const BRANCH: &str = "├── ";
const LAST_BRANCH: &str = "└── ";
const PIPE_INDENT: &str = "│   ";
const SPACE_INDENT: &str = "    ";
const ACCESS_DENIED: &str = "[Access Denied]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeGenerationMode {
    /// A drawn tree starting with the source folder name.
    Structured,
    /// One relative path per line.
    PathsOnly,
}

#[derive(Debug, Clone)]
pub struct TreeGenerationRequest {
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    /// Output file name without extension.
    pub file_name: String,
    pub mode: TreeGenerationMode,
    /// Path keys (see [`crate::path_key::path_key`]) of folders to skip together with their content.
    pub excluded_path_keys: HashSet<String>,
    pub ignored_folder_names: IgnoredFolderNames,
}

#[derive(Debug)]
pub enum TreeGenerationError {
    SourceMissing,
    Io(io::Error),
}

impl fmt::Display for TreeGenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceMissing => f.write_str("The selected source folder does not exist."),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for TreeGenerationError {}

impl From<io::Error> for TreeGenerationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Writes the tree structure file and returns its path.
pub fn generate_tree_file(request: &TreeGenerationRequest) -> Result<PathBuf, TreeGenerationError> {
    if !request.source_path.is_dir() {
        return Err(TreeGenerationError::SourceMissing);
    }
    let output_path = output_path(&request.destination_path, &request.file_name);
    let mut writer = BufWriter::new(File::create(&output_path)?);
    writer.write_all(UTF8_BOM)?;
    write_tree(request, &mut writer)?;
    writer.flush()?;
    Ok(output_path)
}

/// Writes the tree lines without BOM.
pub fn write_tree(request: &TreeGenerationRequest, writer: &mut impl Write) -> io::Result<()> {
    let mut generator = Generator { request, writer };
    match request.mode {
        TreeGenerationMode::Structured => {
            generator.line(&display_name(&request.source_path))?;
            generator.structured(&request.source_path, "")
        }
        TreeGenerationMode::PathsOnly => generator.paths_only(&request.source_path),
    }
}

struct Generator<'a, W: Write> {
    request: &'a TreeGenerationRequest,
    writer: &'a mut W,
}

impl<W: Write> Generator<'_, W> {
    fn line(&mut self, text: &str) -> io::Result<()> {
        self.writer.write_all(text.as_bytes())?;
        self.writer.write_all(LINE_ENDING.as_bytes())
    }

    fn included_directories(&self, directories: Vec<PathBuf>) -> Vec<PathBuf> {
        directories
            .into_iter()
            .filter(|directory| {
                !self
                    .request
                    .excluded_path_keys
                    .contains(&path_key(directory))
            })
            .filter(|directory| !self.request.ignored_folder_names.contains(directory))
            .collect()
    }

    fn structured(&mut self, directory: &Path, indent: &str) -> io::Result<()> {
        let listing = match list_directory(directory) {
            Ok(listing) => listing,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                return self.line(&format!("{indent}{LAST_BRANCH}{ACCESS_DENIED}"));
            }
            Err(error) => return Err(error),
        };
        let directories = self.included_directories(listing.directories);
        let files = listing.files;

        for (index, subdirectory) in directories.iter().enumerate() {
            let is_last = index == directories.len() - 1 && files.is_empty();
            let branch = if is_last { LAST_BRANCH } else { BRANCH };
            self.line(&format!("{indent}{branch}{}", display_name(subdirectory)))?;
            let child_indent = format!(
                "{indent}{}",
                if is_last { SPACE_INDENT } else { PIPE_INDENT }
            );
            self.structured(subdirectory, &child_indent)?;
        }

        for (index, file) in files.iter().enumerate() {
            let branch = if index == files.len() - 1 {
                LAST_BRANCH
            } else {
                BRANCH
            };
            self.line(&format!("{indent}{branch}{}", display_name(file)))?;
        }
        Ok(())
    }

    fn paths_only(&mut self, directory: &Path) -> io::Result<()> {
        let listing = match list_directory(directory) {
            Ok(listing) => listing,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Ok(()),
            Err(error) => return Err(error),
        };
        for subdirectory in self.included_directories(listing.directories) {
            self.line(&self.relative(&subdirectory))?;
            self.paths_only(&subdirectory)?;
        }
        for file in &listing.files {
            self.line(&self.relative(file))?;
        }
        Ok(())
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.request.source_path)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(root.join("src/nested")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("README.md"), "").unwrap();
        fs::write(root.join("src/main.rs"), "").unwrap();
        fs::write(root.join("src/nested/deep.rs"), "").unwrap();
        fs::write(root.join("bin/out.exe"), "").unwrap();
        dir
    }

    fn request(dir: &tempfile::TempDir, mode: TreeGenerationMode) -> TreeGenerationRequest {
        TreeGenerationRequest {
            source_path: dir.path().join("project"),
            destination_path: dir.path().to_path_buf(),
            file_name: "tree".to_owned(),
            mode,
            excluded_path_keys: HashSet::new(),
            ignored_folder_names: IgnoredFolderNames::default(),
        }
    }

    fn render(request: &TreeGenerationRequest) -> String {
        let mut output = Vec::new();
        write_tree(request, &mut output).unwrap();
        String::from_utf8(output).unwrap().replace("\r\n", "\n")
    }

    #[test]
    fn writes_structured_tree() {
        let dir = sample();
        let expected = "\
project
├── bin
│   └── out.exe
├── docs
├── src
│   ├── nested
│   │   └── deep.rs
│   └── main.rs
└── README.md
";
        assert_eq!(
            render(&request(&dir, TreeGenerationMode::Structured)),
            expected
        );
    }

    #[test]
    fn last_folder_without_files_uses_last_branch() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("project/a/b")).unwrap();
        let expected = "\
project
└── a
    └── b
";
        assert_eq!(
            render(&request(&dir, TreeGenerationMode::Structured)),
            expected
        );
    }

    #[test]
    fn excluded_and_ignored_folders_are_skipped() {
        let dir = sample();
        let mut request = request(&dir, TreeGenerationMode::PathsOnly);
        request
            .excluded_path_keys
            .insert(path_key(&request.source_path.join("src")));
        request.ignored_folder_names = IgnoredFolderNames::new(["BIN"]);
        assert_eq!(render(&request), "docs\nREADME.md\n");
    }

    #[test]
    fn paths_only_lists_relative_paths_depth_first() {
        let dir = sample();
        let sep = std::path::MAIN_SEPARATOR;
        let expected =
            "bin\nbin/out.exe\ndocs\nsrc\nsrc/nested\nsrc/nested/deep.rs\nsrc/main.rs\nREADME.md\n"
                .replace('/', &sep.to_string());
        assert_eq!(
            render(&request(&dir, TreeGenerationMode::PathsOnly)),
            expected
        );
    }

    #[test]
    fn file_starts_with_bom_and_uses_platform_line_endings() {
        let dir = sample();
        let path = generate_tree_file(&request(&dir, TreeGenerationMode::Structured)).unwrap();
        assert_eq!(path, dir.path().join("tree.txt"));
        let bytes = fs::read(path).unwrap();
        assert!(bytes.starts_with(UTF8_BOM));
        assert!(bytes[UTF8_BOM.len()..].starts_with(format!("project{LINE_ENDING}").as_bytes()));
    }

    #[test]
    fn missing_source_is_reported() {
        let dir = sample();
        let mut request = request(&dir, TreeGenerationMode::Structured);
        request.source_path = dir.path().join("missing");
        assert!(matches!(
            generate_tree_file(&request),
            Err(TreeGenerationError::SourceMissing)
        ));
    }
}
