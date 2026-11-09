//! Texts of the preset details window.

use crate::file_content::compare_paths;
use crate::folder_content::column;
use crate::presets::{PathPreset, SavePreset};
use crate::tree_generator::TreeGenerationMode;

/// Paths sorted for display.
fn sorted(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut paths: Vec<String> = paths.into_iter().collect();
    paths.sort_by(|a, b| compare_paths(a, b));
    paths
}

/// The files of a path preset, sorted.
pub fn path_preset_files(preset: &PathPreset) -> Vec<String> {
    sorted(preset.file_paths.iter().cloned())
}

/// The explicitly selected files of a save preset, sorted.
pub fn save_preset_files(preset: &SavePreset) -> Vec<String> {
    sorted(
        preset
            .data
            .file_content_export
            .selected_file_paths
            .iter()
            .cloned(),
    )
}

/// The fully selected folders of all trees of a save preset, without duplicates, sorted.
pub fn save_preset_folders(preset: &SavePreset) -> Vec<String> {
    let data = &preset.data;
    let mut folders: Vec<String> = Vec::new();
    for state in data
        .file_content_export
        .folder_tree_state
        .iter()
        .chain(&data.folder_content_export.folder_tree_state)
        .chain(&data.tree_generator.folder_tree_state)
        .filter(|state| state.is_selected == Some(true))
    {
        if !folders.contains(&state.full_path) {
            folders.push(state.full_path.clone());
        }
    }
    sorted(folders)
}

/// The "Configuration Summary" text: one block per tab that has a selection.
pub fn save_preset_summary(preset: &SavePreset) -> String {
    let data = &preset.data;
    let any_selected = |states: &[crate::settings::FolderState]| {
        states.iter().any(|state| state.is_selected == Some(true))
    };
    let mut text = String::new();
    let mut line = |value: String| {
        text.push_str(&value);
        text.push('\n');
    };

    let file_content = &data.file_content_export;
    if !file_content.selected_file_paths.is_empty() || any_selected(&file_content.folder_tree_state)
    {
        line("--- File Content Export ---".to_owned());
        line(format!("Output: {}", file_content.output_file_name));
        if !file_content.selected_extensions.is_empty() {
            line(format!(
                "Extensions: {}",
                file_content.selected_extensions.join(", ")
            ));
        }
        line(format!(
            "Selected Files: {}",
            file_content.selected_file_paths.len()
        ));
        line(String::new());
    }

    let tree = &data.tree_generator;
    if any_selected(&tree.folder_tree_state) {
        line("--- Tree Generator ---".to_owned());
        line(format!("Output: {}", tree.output_file_name));
        let mode = match tree.generation_mode {
            TreeGenerationMode::Structured => "Structured",
            TreeGenerationMode::PathsOnly => "PathsOnly",
        };
        line(format!("Mode: {mode}"));
        line(String::new());
    }

    let folder_content = &data.folder_content_export;
    if any_selected(&folder_content.folder_tree_state) {
        line("--- Folder Content Export ---".to_owned());
        line(format!("Output: {}", folder_content.output_file_name));
        if !folder_content.selected_columns.is_empty() {
            let columns: Vec<&str> = folder_content
                .selected_columns
                .iter()
                .map(|id| column(id).map_or(id.as_str(), |column| column.display_name))
                .collect();
            line(format!("Columns: {}", columns.join(", ")));
        }
    }
    text
}

struct TreeNode {
    name: String,
    children: Vec<TreeNode>,
}

impl TreeNode {
    fn add(&mut self, parts: &[&str]) {
        let Some((first, rest)) = parts.split_first() else {
            return;
        };
        if *first == "." || first.is_empty() {
            self.add(rest);
            return;
        }
        let key = first.to_lowercase();
        let index = match self
            .children
            .iter()
            .position(|child| child.name.to_lowercase() == key)
        {
            Some(index) => index,
            None => {
                self.children.push(TreeNode {
                    name: (*first).to_owned(),
                    children: Vec::new(),
                });
                self.children.len() - 1
            }
        };
        self.children[index].add(rest);
    }

    fn print(&self, indent: &str, last: bool, text: &mut String) {
        text.push_str(indent);
        text.push_str(if last { "└── " } else { "├── " });
        text.push_str(&self.name);
        text.push('\n');
        let child_indent = format!("{indent}{}", if last { "    " } else { "│   " });
        for (index, child) in self.children.iter().enumerate() {
            child.print(&child_indent, index + 1 == self.children.len(), text);
        }
    }
}

/// The paths as a tree below `.`, in the given order, with names merged ignoring case.
pub fn paths_tree_text(paths: &[String]) -> String {
    let mut root = TreeNode {
        name: ".".to_owned(),
        children: Vec::new(),
    };
    for path in paths {
        let parts: Vec<&str> = path
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .collect();
        root.add(&parts);
    }
    let mut text = ".\n".to_owned();
    for (index, child) in root.children.iter().enumerate() {
        child.print("", index + 1 == root.children.len(), &mut text);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::FolderState;

    #[test]
    fn builds_tree_text() {
        let paths = vec![
            "src/a.cs".to_owned(),
            "src\\B.cs".to_owned(),
            "SRC/sub/c.cs".to_owned(),
            "readme.md".to_owned(),
        ];
        assert_eq!(
            paths_tree_text(&paths),
            ".\n├── src\n│   ├── a.cs\n│   ├── B.cs\n│   └── sub\n│       └── c.cs\n└── readme.md\n"
        );
    }

    #[test]
    fn summarizes_tabs_with_selections() {
        let mut preset = SavePreset::default();
        let data = &mut preset.data;
        data.file_content_export.selected_extensions = vec![".cs".to_owned(), ".md".to_owned()];
        data.file_content_export.selected_file_paths = vec!["a.cs".to_owned()];
        data.folder_content_export.folder_tree_state = vec![FolderState {
            full_path: ".".to_owned(),
            is_selected: Some(true),
            is_expanded: true,
        }];
        data.folder_content_export.selected_columns = vec!["file.name".to_owned(), "x".to_owned()];
        assert_eq!(
            save_preset_summary(&preset),
            "--- File Content Export ---\nOutput: FileContents\nExtensions: .cs, .md\nSelected Files: 1\n\n\
             --- Folder Content Export ---\nOutput: FolderContents\nColumns: Name, x\n"
        );
        assert_eq!(save_preset_folders(&preset), ["."]);
        assert_eq!(save_preset_files(&preset), ["a.cs"]);
    }
}
