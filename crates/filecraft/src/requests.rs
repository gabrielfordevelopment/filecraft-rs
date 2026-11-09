//! What a tab asks the application to do after a frame.

use crate::dialog::Dialog;
use crate::toast::Toast;

/// An action that runs when a confirmation dialog is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmAction {
    GenerateTree,
    ExportFileContents,
    ExportFolderContents,
    RenameFiles,
}

pub enum TabRequest {
    ShowDialog(Dialog),
    Confirm(Dialog, ConfirmAction),
    /// A non-blocking notification, such as a finished export.
    Notify(Toast),
    /// The "Manage path presets" button was clicked.
    OpenPathPresets,
    /// The empty folder tree asked for a source folder.
    ChooseSourceFolder,
    /// Saved settings finished loading in the background. The change is not a user edit, so it
    /// must not become an undo step.
    SettingsResolved,
}
