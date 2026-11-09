//! State shared by all tabs: the paths and the folder trees with their links. Each tab with a
//! folder tree has its own tree of
//! the common source folder, unless it is linked to other tabs: linked tabs share the tree of the
//! first tab of their group.

use std::path::Path;

use filecraft_core::directory::IgnoredFolderNames;
use filecraft_core::folder_tree::FolderTree;
use filecraft_core::settings::{DEFAULT_IGNORED_FOLDERS, FolderState};

use crate::icons::{self, Icon};
use crate::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeTab {
    FileContentExport,
    TreeGenerator,
    FolderContentExport,
}

impl TreeTab {
    pub const ALL: [Self; 3] = [
        Self::FileContentExport,
        Self::TreeGenerator,
        Self::FolderContentExport,
    ];

    fn index(self) -> usize {
        match self {
            Self::FileContentExport => 0,
            Self::TreeGenerator => 1,
            Self::FolderContentExport => 2,
        }
    }

    /// The tab's name in the tab strip.
    pub fn title(self) -> &'static str {
        match self {
            Self::FileContentExport => strings::TAB_FILE_CONTENT_EXPORT,
            Self::TreeGenerator => strings::TAB_TREE_GENERATOR,
            Self::FolderContentExport => strings::TAB_FOLDER_CONTENT_EXPORT,
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Self::FileContentExport => icons::FILE_CONTENT_EXPORT,
            Self::TreeGenerator => icons::TREE_GENERATOR,
            Self::FolderContentExport => icons::FOLDER_CONTENT_EXPORT,
        }
    }

    /// The folder tree manager id stored in `LinkedFolderTreeGroups`.
    pub fn link_id(self) -> &'static str {
        match self {
            Self::FileContentExport => "FileContentExport",
            Self::TreeGenerator => "TreeGenerator",
            Self::FolderContentExport => "FolderContentExport",
        }
    }

    fn from_link_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.link_id() == id)
    }
}

pub struct SharedState {
    pub source_path: String,
    pub destination_path: String,
    ignored_folders: Vec<String>,
    /// Folder trees of the source path by `TreeTab`, `None` when no valid source is selected. The
    /// tree of a linked tab that is not first in its group is not used.
    trees: [Option<FolderTree>; 3],
    /// Groups of linked tabs. The first tab of a group owns the shared tree.
    links: Vec<Vec<TreeTab>>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            source_path: String::new(),
            destination_path: String::new(),
            ignored_folders: DEFAULT_IGNORED_FOLDERS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            trees: [None, None, None],
            links: Vec::new(),
        }
    }
}

impl SharedState {
    pub fn tree(&self, tab: TreeTab) -> Option<&FolderTree> {
        self.trees[self.owner(tab).index()].as_ref()
    }

    pub fn tree_mut(&mut self, tab: TreeTab) -> Option<&mut FolderTree> {
        let owner = self.owner(tab);
        self.trees[owner.index()].as_mut()
    }

    fn group(&self, tab: TreeTab) -> Option<&Vec<TreeTab>> {
        self.links.iter().find(|group| group.contains(&tab))
    }

    /// The tab whose tree `tab` uses.
    fn owner(&self, tab: TreeTab) -> TreeTab {
        self.group(tab)
            .and_then(|group| group.first().copied())
            .unwrap_or(tab)
    }

    pub fn link_groups(&self) -> &[Vec<TreeTab>] {
        &self.links
    }

    pub fn is_linked(&self, tab: TreeTab) -> bool {
        self.group(tab).is_some()
    }

    /// The other tabs linked to `tab`.
    pub fn linked_peers(&self, tab: TreeTab) -> Vec<TreeTab> {
        self.group(tab)
            .map(|group| group.iter().copied().filter(|peer| *peer != tab).collect())
            .unwrap_or_default()
    }

    /// Link groups as stored in `LinkedFolderTreeGroups`.
    pub fn link_group_ids(&self) -> Vec<Vec<String>> {
        self.links
            .iter()
            .map(|group| group.iter().map(|tab| tab.link_id().to_owned()).collect())
            .collect()
    }

    /// Links two tabs. A tab joining a group uses the group's tree, and merged
    /// groups use the tree of `first`'s group.
    pub fn link(&mut self, first: TreeTab, second: TreeTab) {
        let first_group = self.links.iter().position(|group| group.contains(&first));
        let second_group = self.links.iter().position(|group| group.contains(&second));
        match (first_group, second_group) {
            (Some(a), Some(b)) if a == b => {}
            (None, None) => self.links.push(vec![first, second]),
            (Some(a), None) => self.links[a].push(second),
            (None, Some(b)) => self.links[b].push(first),
            (Some(a), Some(b)) => {
                let merged: Vec<TreeTab> = self.links[a]
                    .iter()
                    .chain(&self.links[b])
                    .copied()
                    .collect();
                let (low, high) = (a.min(b), a.max(b));
                self.links.remove(high);
                self.links.remove(low);
                self.links.push(merged);
            }
        }
    }

    /// Removes a tab from its group. The tab keeps a copy of the shared tree. A
    /// group with one tab left is dissolved.
    pub fn unlink(&mut self, tab: TreeTab) {
        let Some(position) = self.links.iter().position(|group| group.contains(&tab)) else {
            return;
        };
        let shared = self.trees[self.links[position][0].index()].clone();
        self.links[position].retain(|member| *member != tab);
        self.trees[tab.index()] = shared.clone();
        let group = &self.links[position];
        if group.len() < 2 {
            for member in group.clone() {
                self.trees[member.index()] = shared.clone();
            }
            self.links.remove(position);
        } else {
            self.trees[group[0].index()] = shared;
        }
    }

    /// Replaces the tree of `destination` with the selection and expansion of `source`'s tree.
    pub fn copy_tree(&mut self, source: TreeTab, destination: TreeTab) {
        let states = self.folder_states(source);
        let owner = self.owner(destination);
        self.trees[owner.index()] = self.load_tree(Some(&states));
    }

    pub fn ignored_folders(&self) -> &[String] {
        &self.ignored_folders
    }

    pub fn ignored_folder_names(&self) -> IgnoredFolderNames {
        IgnoredFolderNames::new(&self.ignored_folders)
    }

    /// Saved selection and expansion of a tab's tree.
    pub fn folder_states(&self, tab: TreeTab) -> Vec<FolderState> {
        self.tree(tab)
            .map(FolderTree::folder_states)
            .unwrap_or_default()
    }

    /// Selects a new source folder. Every tree is reloaded with only its root expanded.
    pub fn set_source_path(&mut self, path: String) {
        self.source_path = path;
        for tab in TreeTab::ALL {
            self.trees[tab.index()] = self.load_tree(None);
        }
    }

    pub fn clear_paths(&mut self) {
        self.source_path.clear();
        self.destination_path.clear();
        self.trees = [None, None, None];
    }

    pub fn can_clear_paths(&self) -> bool {
        !self.source_path.trim().is_empty() || !self.destination_path.trim().is_empty()
    }

    /// Changes the ignored folders and reloads the trees, keeping their selection and expansion.
    pub fn set_ignored_folders(&mut self, names: Vec<String>) {
        let states = TreeTab::ALL.map(|tab| self.folder_states(tab));
        self.ignored_folders = names;
        self.restore_trees(states.each_ref().map(|states| states.as_slice()));
    }

    /// Applies saved paths, ignored folders, links and tree states. Unknown tab ids in the link
    /// groups are skipped.
    pub fn apply(
        &mut self,
        source_path: &str,
        destination_path: &str,
        ignored_folders: &[String],
        link_groups: &[Vec<String>],
        states: [&[FolderState]; 3],
    ) {
        self.source_path = source_path.to_owned();
        self.destination_path = destination_path.to_owned();
        self.ignored_folders = ignored_folders.to_vec();
        self.links = Vec::new();
        for ids in link_groups {
            let mut group: Vec<TreeTab> = Vec::new();
            for tab in ids.iter().filter_map(|id| TreeTab::from_link_id(id)) {
                if !group.contains(&tab) && self.group(tab).is_none() {
                    group.push(tab);
                }
            }
            if !group.is_empty() {
                self.links.push(group);
            }
        }
        self.restore_trees(states);
    }

    fn restore_trees(&mut self, states: [&[FolderState]; 3]) {
        for tab in TreeTab::ALL {
            self.trees[tab.index()] = self.load_tree(Some(states[tab.index()]));
        }
    }

    fn load_tree(&self, states: Option<&[FolderState]>) -> Option<FolderTree> {
        if self.source_path.trim().is_empty() {
            return None;
        }
        FolderTree::load_with_states(
            Path::new(&self.source_path),
            self.ignored_folder_names(),
            states,
        )
    }
}
