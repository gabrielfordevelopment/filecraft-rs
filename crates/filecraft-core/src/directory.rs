use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::path_key::name_key;

/// The immediate entries of a directory, each list sorted by name.
#[derive(Debug, Default)]
pub struct DirectoryListing {
    pub directories: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
}

/// A set of folder names that are skipped everywhere, compared case-insensitively.
#[derive(Debug, Clone, Default)]
pub struct IgnoredFolderNames(HashSet<String>);

impl IgnoredFolderNames {
    pub fn new<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self(
            names
                .into_iter()
                .map(|name| name_key(name.as_ref()))
                .collect(),
        )
    }

    pub fn contains(&self, path: &Path) -> bool {
        !self.0.is_empty() && self.0.contains(&name_key(&display_name(path)))
    }
}

/// Lists a directory. Directories are detected by following symbolic links.
pub fn list_directory(path: &Path) -> io::Result<DirectoryListing> {
    let mut listing = DirectoryListing::default();
    for entry in fs::read_dir(path)? {
        let entry_path = entry?.path();
        if entry_path.is_dir() {
            listing.directories.push(entry_path);
        } else {
            listing.files.push(entry_path);
        }
    }
    listing.directories.sort_by(|a, b| compare_names(a, b));
    listing.files.sort_by(|a, b| compare_names(a, b));
    Ok(listing)
}

/// Returns whether the directory has at least one subdirectory that is not ignored.
pub fn has_subdirectories(path: &Path, ignored: &IgnoredFolderNames) -> io::Result<bool> {
    for entry in fs::read_dir(path)? {
        let entry_path = entry?.path();
        if entry_path.is_dir() && !ignored.contains(&entry_path) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns the last path component, or the whole path for roots such as `C:\` or `/`.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Orders entries by name ignoring case, with an ordinal tie-break so the order is deterministic.
fn compare_names(a: &Path, b: &Path) -> Ordering {
    let a_name = display_name(a);
    let b_name = display_name(b);
    name_key(&a_name)
        .cmp(&name_key(&b_name))
        .then_with(|| a_name.cmp(&b_name))
}
