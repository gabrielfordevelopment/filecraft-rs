//! Lazily loaded folder tree with three-state selection.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::directory::{IgnoredFolderNames, display_name, has_subdirectories, list_directory};
use crate::path_key::path_key;
use crate::settings::FolderState;

pub type NodeId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    Indeterminate,
}

#[derive(Debug, Clone)]
enum Children {
    /// Not read yet. The flag tells whether an expander should be shown.
    Unloaded {
        has_subdirectories: bool,
    },
    Loaded(Vec<NodeId>),
}

#[derive(Debug, Clone)]
pub struct FolderNode {
    pub name: String,
    pub path: PathBuf,
    pub parent: Option<NodeId>,
    pub state: CheckState,
    pub expanded: bool,
    /// Set when the folder content could not be read.
    pub load_error: Option<String>,
    children: Children,
}

/// A folder whose files are included: all files below it when `recursive`, otherwise only the
/// files directly inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderScope {
    pub path: PathBuf,
    pub recursive: bool,
}

#[derive(Debug, Clone)]
pub struct FolderTree {
    nodes: Vec<FolderNode>,
    ignored: IgnoredFolderNames,
}

impl FolderTree {
    pub const ROOT: NodeId = 0;

    /// Builds a tree for `root_path` with the root expanded and nothing selected.
    /// Returns `None` when the path is not an existing directory.
    pub fn load(root_path: &Path, ignored: IgnoredFolderNames) -> Option<Self> {
        if !root_path.is_dir() {
            return None;
        }
        let mut tree = Self {
            nodes: Vec::new(),
            ignored,
        };
        tree.push_node(root_path.to_path_buf(), None);
        tree.set_expanded(Self::ROOT, true);
        Some(tree)
    }

    /// Builds a tree and restores saved folder states. Without saved states the root is expanded,
    /// like a newly selected source folder. With saved states only the saved folders are expanded.
    pub fn load_with_states(
        root_path: &Path,
        ignored: IgnoredFolderNames,
        states: Option<&[FolderState]>,
    ) -> Option<Self> {
        let Some(states) = states else {
            return Self::load(root_path, ignored);
        };
        if !root_path.is_dir() {
            return None;
        }
        let mut tree = Self {
            nodes: Vec::new(),
            ignored,
        };
        tree.push_node(root_path.to_path_buf(), None);
        let by_path: HashMap<String, &FolderState> = states
            .iter()
            .map(|state| (path_key(Path::new(&state.full_path)), state))
            .collect();
        tree.apply_states(Self::ROOT, &by_path);
        Some(tree)
    }

    /// Folders that are selected, partially selected or expanded, in tree order.
    pub fn folder_states(&self) -> Vec<FolderState> {
        let mut states = Vec::new();
        self.collect_states(Self::ROOT, &mut states);
        states
    }

    fn collect_states(&self, id: NodeId, states: &mut Vec<FolderState>) {
        let node = &self.nodes[id];
        if node.state != CheckState::Unchecked || node.expanded {
            states.push(FolderState {
                full_path: node.path.to_string_lossy().into_owned(),
                is_selected: match node.state {
                    CheckState::Checked => Some(true),
                    CheckState::Unchecked => Some(false),
                    CheckState::Indeterminate => None,
                },
                is_expanded: node.expanded,
            });
        }
        for child in self.children(id) {
            self.collect_states(*child, states);
        }
    }

    fn apply_states(&mut self, id: NodeId, by_path: &HashMap<String, &FolderState>) {
        if let Some(state) = by_path.get(&path_key(&self.nodes[id].path)) {
            self.nodes[id].state = match state.is_selected {
                Some(true) => CheckState::Checked,
                Some(false) => CheckState::Unchecked,
                None => CheckState::Indeterminate,
            };
            if state.is_expanded {
                self.set_expanded(id, true);
            }
            if let Some(parent) = self.nodes[id].parent {
                self.refresh_from_children(parent);
            }
        }
        for child in self.children(id).to_vec() {
            self.apply_states(child, by_path);
        }
    }

    pub fn root_path(&self) -> &Path {
        &self.nodes[Self::ROOT].path
    }

    pub fn node(&self, id: NodeId) -> &FolderNode {
        &self.nodes[id]
    }

    /// Loaded children of a node. Unloaded nodes return an empty slice.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        match &self.nodes[id].children {
            Children::Loaded(children) => children,
            Children::Unloaded { .. } => &[],
        }
    }

    /// Whether the node shows an expander.
    pub fn has_children(&self, id: NodeId) -> bool {
        match &self.nodes[id].children {
            Children::Unloaded { has_subdirectories } => *has_subdirectories,
            Children::Loaded(children) => !children.is_empty(),
        }
    }

    /// Expands or collapses a node, reading its subfolders the first time it is expanded.
    pub fn set_expanded(&mut self, id: NodeId, expanded: bool) {
        self.nodes[id].expanded = expanded;
        if expanded && matches!(self.nodes[id].children, Children::Unloaded { .. }) {
            self.load_children(id);
        }
    }

    /// Toggles a checkbox: a checked node becomes unchecked, any other state becomes checked.
    /// The new state is applied to all loaded descendants and the ancestors are updated.
    pub fn toggle(&mut self, id: NodeId) {
        let checked = self.nodes[id].state != CheckState::Checked;
        let state = if checked {
            CheckState::Checked
        } else {
            CheckState::Unchecked
        };
        self.set_state_down(id, state);
        if let Some(parent) = self.nodes[id].parent {
            self.refresh_from_children(parent);
        }
    }

    /// Number of loaded folders that are checked or partially checked.
    pub fn selected_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| node.state != CheckState::Unchecked)
            .count()
    }

    /// Folders whose files are included:
    /// a checked folder includes everything below it, a partially checked folder includes its own
    /// files and the scopes of its loaded subfolders.
    pub fn selection_scopes(&self) -> Vec<FolderScope> {
        let mut scopes = Vec::new();
        self.collect_scopes(Self::ROOT, &mut scopes);
        scopes
    }

    fn collect_scopes(&self, id: NodeId, scopes: &mut Vec<FolderScope>) {
        let node = &self.nodes[id];
        match node.state {
            CheckState::Checked => scopes.push(FolderScope {
                path: node.path.clone(),
                recursive: true,
            }),
            CheckState::Indeterminate => {
                scopes.push(FolderScope {
                    path: node.path.clone(),
                    recursive: false,
                });
                for child in self.children(id) {
                    self.collect_scopes(*child, scopes);
                }
            }
            CheckState::Unchecked => {}
        }
    }

    /// Folders that are checked, without their descendants. Partially selected folders contribute
    /// their checked children but not their own files.
    pub fn checked_folders(&self) -> Vec<PathBuf> {
        let mut folders = Vec::new();
        self.collect_checked(Self::ROOT, &mut folders);
        folders
    }

    fn collect_checked(&self, id: NodeId, folders: &mut Vec<PathBuf>) {
        match self.nodes[id].state {
            CheckState::Checked => folders.push(self.nodes[id].path.clone()),
            CheckState::Indeterminate => {
                for child in self.children(id) {
                    self.collect_checked(*child, folders);
                }
            }
            CheckState::Unchecked => {}
        }
    }

    /// Path keys of loaded folders that are unchecked. Folders below them are skipped during generation.
    pub fn excluded_path_keys(&self) -> HashSet<String> {
        self.nodes
            .iter()
            .filter(|node| node.state == CheckState::Unchecked)
            .map(|node| path_key(&node.path))
            .collect()
    }

    fn push_node(&mut self, path: PathBuf, parent: Option<NodeId>) -> NodeId {
        let (has_subdirectories, load_error) = match has_subdirectories(&path, &self.ignored) {
            Ok(value) => (value, None),
            Err(error) => (false, Some(error.to_string())),
        };
        let state = parent
            .map(|parent| self.nodes[parent].state)
            .filter(|state| *state != CheckState::Indeterminate)
            .unwrap_or(CheckState::Unchecked);
        self.nodes.push(FolderNode {
            name: display_name(&path),
            path,
            parent,
            state,
            expanded: false,
            load_error,
            children: Children::Unloaded { has_subdirectories },
        });
        self.nodes.len() - 1
    }

    fn load_children(&mut self, id: NodeId) {
        let path = self.nodes[id].path.clone();
        let directories = match list_directory(&path) {
            Ok(listing) => listing.directories,
            Err(error) => {
                self.nodes[id].load_error = Some(error.to_string());
                Vec::new()
            }
        };
        let mut children = Vec::with_capacity(directories.len());
        for directory in directories {
            if !self.ignored.contains(&directory) {
                children.push(self.push_node(directory, Some(id)));
            }
        }
        self.nodes[id].children = Children::Loaded(children);
    }

    fn set_state_down(&mut self, id: NodeId, state: CheckState) {
        self.nodes[id].state = state;
        for child in self.children(id).to_vec() {
            self.set_state_down(child, state);
        }
    }

    fn refresh_from_children(&mut self, id: NodeId) {
        let children = self.children(id);
        if children.is_empty() {
            return;
        }
        let state = if children
            .iter()
            .all(|child| self.nodes[*child].state == CheckState::Checked)
        {
            CheckState::Checked
        } else if children
            .iter()
            .all(|child| self.nodes[*child].state == CheckState::Unchecked)
        {
            CheckState::Unchecked
        } else {
            CheckState::Indeterminate
        };
        if self.nodes[id].state == state {
            return;
        }
        self.nodes[id].state = state;
        if let Some(parent) = self.nodes[id].parent {
            self.refresh_from_children(parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for folder in ["a/a1", "a/a2", "b", "node_modules/x"] {
            fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        dir
    }

    fn find(tree: &FolderTree, name: &str) -> NodeId {
        (0..tree.nodes.len())
            .find(|id| tree.node(*id).name == name)
            .unwrap()
    }

    #[test]
    fn root_is_expanded_and_unchecked() {
        let dir = sample();
        let tree = FolderTree::load(dir.path(), IgnoredFolderNames::default()).unwrap();
        let root = tree.node(FolderTree::ROOT);
        assert!(root.expanded);
        assert_eq!(root.state, CheckState::Unchecked);
        let names: Vec<_> = tree
            .children(FolderTree::ROOT)
            .iter()
            .map(|id| tree.node(*id).name.as_str())
            .collect();
        assert_eq!(names, ["a", "b", "node_modules"]);
        assert_eq!(tree.selected_count(), 0);
    }

    #[test]
    fn ignored_folders_are_hidden() {
        let dir = sample();
        let tree = FolderTree::load(dir.path(), IgnoredFolderNames::new(["NODE_MODULES"])).unwrap();
        assert_eq!(tree.children(FolderTree::ROOT).len(), 2);
    }

    #[test]
    fn checking_root_checks_loaded_children_and_lazy_children_inherit() {
        let dir = sample();
        let mut tree = FolderTree::load(dir.path(), IgnoredFolderNames::default()).unwrap();
        tree.toggle(FolderTree::ROOT);
        assert_eq!(tree.selected_count(), 4);

        let a = find(&tree, "a");
        assert!(tree.has_children(a));
        tree.set_expanded(a, true);
        assert_eq!(tree.node(find(&tree, "a1")).state, CheckState::Checked);
        assert_eq!(tree.selected_count(), 6);
    }

    #[test]
    fn unchecking_child_makes_parent_indeterminate() {
        let dir = sample();
        let mut tree = FolderTree::load(dir.path(), IgnoredFolderNames::default()).unwrap();
        tree.toggle(FolderTree::ROOT);
        let b = find(&tree, "b");
        tree.toggle(b);
        assert_eq!(tree.node(b).state, CheckState::Unchecked);
        assert_eq!(tree.node(FolderTree::ROOT).state, CheckState::Indeterminate);
        assert!(
            tree.excluded_path_keys()
                .contains(&path_key(&tree.node(b).path))
        );

        tree.toggle(FolderTree::ROOT);
        assert_eq!(tree.node(FolderTree::ROOT).state, CheckState::Checked);
        assert_eq!(tree.node(b).state, CheckState::Checked);
    }

    #[test]
    fn checking_all_children_checks_parent() {
        let dir = sample();
        let mut tree = FolderTree::load(dir.path(), IgnoredFolderNames::default()).unwrap();
        for name in ["a", "b", "node_modules"] {
            let id = find(&tree, name);
            tree.toggle(id);
        }
        assert_eq!(tree.node(FolderTree::ROOT).state, CheckState::Checked);
    }

    #[test]
    fn folder_states_round_trip() {
        let dir = sample();
        let mut tree = FolderTree::load(dir.path(), IgnoredFolderNames::default()).unwrap();
        let a = find(&tree, "a");
        tree.set_expanded(a, true);
        tree.toggle(find(&tree, "a1"));
        let states = tree.folder_states();
        // Root (expanded), a (partial and expanded) and a1 (checked).
        assert_eq!(states.len(), 3);
        assert_eq!(states[1].is_selected, None);

        let restored =
            FolderTree::load_with_states(dir.path(), IgnoredFolderNames::default(), Some(&states))
                .unwrap();
        assert_eq!(restored.folder_states(), states);
        assert_eq!(restored.selected_count(), tree.selected_count());
    }

    #[test]
    fn empty_saved_states_keep_root_collapsed() {
        let dir = sample();
        let tree =
            FolderTree::load_with_states(dir.path(), IgnoredFolderNames::default(), Some(&[]))
                .unwrap();
        assert!(!tree.node(FolderTree::ROOT).expanded);
        assert!(tree.has_children(FolderTree::ROOT));
    }

    #[test]
    fn checked_saved_folder_checks_lazily_loaded_children() {
        let dir = sample();
        let root = dir.path().to_string_lossy().into_owned();
        let states = [FolderState {
            full_path: root,
            is_selected: Some(true),
            is_expanded: true,
        }];
        let tree =
            FolderTree::load_with_states(dir.path(), IgnoredFolderNames::default(), Some(&states))
                .unwrap();
        assert_eq!(tree.selected_count(), 4);
    }

    #[test]
    fn selection_scopes_follow_check_states() {
        let dir = sample();
        let mut tree = FolderTree::load(dir.path(), IgnoredFolderNames::default()).unwrap();
        assert!(tree.selection_scopes().is_empty());
        tree.toggle(find(&tree, "b"));
        let scopes = tree.selection_scopes();
        assert_eq!(scopes.len(), 2);
        assert_eq!(
            scopes[0],
            FolderScope {
                path: dir.path().to_path_buf(),
                recursive: false
            }
        );
        assert_eq!(
            scopes[1],
            FolderScope {
                path: dir.path().join("b"),
                recursive: true
            }
        );
    }

    #[test]
    fn checked_folders_skip_partially_selected_parents() {
        let dir = sample();
        let mut tree = FolderTree::load(dir.path(), IgnoredFolderNames::default()).unwrap();
        assert!(tree.checked_folders().is_empty());
        tree.toggle(find(&tree, "b"));
        assert_eq!(tree.checked_folders(), [dir.path().join("b")]);
        tree.toggle(FolderTree::ROOT);
        assert_eq!(tree.checked_folders(), [dir.path().to_path_buf()]);
    }

    #[test]
    fn missing_root_returns_none() {
        let dir = sample();
        assert!(
            FolderTree::load(&dir.path().join("missing"), IgnoredFolderNames::default()).is_none()
        );
    }
}
