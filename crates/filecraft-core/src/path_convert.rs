//! Relative and absolute paths for presets.

use std::path::{Component, Path, PathBuf};

use crate::path_key::name_key;
use crate::settings::{FolderState, SaveData};

/// Resolves `.` and `..` components without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => parts.push(component),
            },
            _ => parts.push(component),
        }
    }
    parts.iter().collect()
}

fn same_component(a: &Component, b: &Component) -> bool {
    let (a, b) = (
        a.as_os_str().to_string_lossy(),
        b.as_os_str().to_string_lossy(),
    );
    if cfg!(windows) {
        name_key(&a) == name_key(&b)
    } else {
        a == b
    }
}

fn is_root_component(component: &Component) -> bool {
    matches!(component, Component::Prefix(_) | Component::RootDir)
}

/// The path of `path` relative to `base`, or `.` when they are the same folder. Paths on another
/// drive are returned unchanged. Names are compared ignoring case on Windows.
pub fn relative_path(base: &str, path: &str) -> String {
    let base = normalize(Path::new(base));
    let target = normalize(Path::new(path));
    let base_parts: Vec<Component> = base.components().collect();
    let target_parts: Vec<Component> = target.components().collect();

    let base_root = base_parts.iter().take_while(|c| is_root_component(c));
    let target_root = target_parts.iter().take_while(|c| is_root_component(c));
    if base_root.clone().count() != target_root.clone().count()
        || !base_root
            .zip(target_root)
            .all(|(a, b)| same_component(a, b))
    {
        return target.to_string_lossy().into_owned();
    }

    let common = base_parts
        .iter()
        .zip(&target_parts)
        .take_while(|(a, b)| same_component(a, b))
        .count();
    let mut relative = PathBuf::new();
    for _ in common..base_parts.len() {
        relative.push("..");
    }
    for part in &target_parts[common..] {
        relative.push(part.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        relative.to_string_lossy().into_owned()
    }
}

/// `path` resolved against `base`. Absolute paths stay as they are, apart from normalization.
pub fn absolute_path(base: &str, path: &str) -> String {
    normalize(&Path::new(base).join(path))
        .to_string_lossy()
        .into_owned()
}

/// Converts the folder and file paths of a saved state relative to `base` or back to absolute
/// paths. Empty paths stay empty. The source path becomes `.` or `base`. Ignored comment paths are
/// already relative to the source folder and are kept.
pub fn convert_save_data_paths(data: &SaveData, base: &str, to_relative: bool) -> SaveData {
    let convert = |path: &String| -> String {
        if path.trim().is_empty() {
            String::new()
        } else if to_relative {
            relative_path(base, path)
        } else {
            absolute_path(base, path)
        }
    };
    let states = |states: &[FolderState]| -> Vec<FolderState> {
        states
            .iter()
            .map(|state| FolderState {
                full_path: convert(&state.full_path),
                ..state.clone()
            })
            .collect()
    };

    let mut converted = data.clone();
    converted.source_path = if to_relative {
        ".".to_owned()
    } else {
        base.to_owned()
    };
    converted.destination_path = convert(&data.destination_path);
    converted.file_content_export.selected_file_paths = data
        .file_content_export
        .selected_file_paths
        .iter()
        .map(convert)
        .collect();
    converted.file_content_export.folder_tree_state =
        states(&data.file_content_export.folder_tree_state);
    converted.folder_content_export.folder_tree_state =
        states(&data.folder_content_export.folder_tree_state);
    converted.tree_generator.folder_tree_state = states(&data.tree_generator.folder_tree_state);
    converted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native(path: &str) -> String {
        path.replace('/', std::path::MAIN_SEPARATOR_STR)
    }

    #[cfg(windows)]
    #[test]
    fn relative_paths_on_windows() {
        assert_eq!(relative_path(r"C:\project", r"C:\project"), ".");
        assert_eq!(
            relative_path(r"C:\project", r"C:\project\src\a.cs"),
            r"src\a.cs"
        );
        assert_eq!(relative_path(r"C:\Project", r"c:\project\SRC"), "SRC");
        assert_eq!(
            relative_path(r"C:\project\src", r"C:\project\docs"),
            r"..\docs"
        );
        assert_eq!(relative_path(r"C:\project", r"D:\other"), r"D:\other");
        assert_eq!(
            absolute_path(r"C:\project", r"src\a.cs"),
            r"C:\project\src\a.cs"
        );
        assert_eq!(absolute_path(r"C:\project", "."), r"C:\project");
        assert_eq!(
            absolute_path(r"C:\project\src", r"..\docs"),
            r"C:\project\docs"
        );
        assert_eq!(absolute_path(r"C:\project", r"D:\other"), r"D:\other");
    }

    #[test]
    fn relative_and_absolute_paths_round_trip() {
        let base = native("/work/project");
        for path in [
            "/work/project/src/a.cs",
            "/work/project",
            "/work/docs/readme.md",
        ] {
            let path = native(path);
            let base = if cfg!(windows) {
                format!("C:{base}")
            } else {
                base.clone()
            };
            let path = if cfg!(windows) {
                format!("C:{path}")
            } else {
                path
            };
            assert_eq!(absolute_path(&base, &relative_path(&base, &path)), path);
        }
    }

    #[test]
    fn converts_save_data_paths_both_ways() {
        let base = if cfg!(windows) {
            r"C:\project".to_owned()
        } else {
            "/project".to_owned()
        };
        let join =
            |relative: &str| format!("{base}{}{}", std::path::MAIN_SEPARATOR, native(relative));
        let mut data = SaveData {
            source_path: base.clone(),
            destination_path: join("out"),
            ..SaveData::default()
        };
        data.file_content_export.selected_file_paths = vec![join("src/a.cs")];
        data.file_content_export.ignored_comment_file_paths = vec![native("src/a.cs")];
        data.tree_generator.folder_tree_state = vec![FolderState {
            full_path: base.clone(),
            is_selected: Some(true),
            is_expanded: true,
        }];

        let relative = convert_save_data_paths(&data, &base, true);
        assert_eq!(relative.source_path, ".");
        assert_eq!(relative.destination_path, "out");
        assert_eq!(
            relative.file_content_export.selected_file_paths,
            [native("src/a.cs")]
        );
        assert_eq!(
            relative.file_content_export.ignored_comment_file_paths,
            [native("src/a.cs")]
        );
        assert_eq!(relative.tree_generator.folder_tree_state[0].full_path, ".");

        assert_eq!(convert_save_data_paths(&relative, &base, false), data);
    }
}
