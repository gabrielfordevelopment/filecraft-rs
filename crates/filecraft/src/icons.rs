//! Material Icons glyphs and their colors.

use egui::Color32;

use crate::theme;

/// The palette color of an icon, resolved when it is drawn so it follows the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Text,
    Muted,
    Accent,
    Danger,
    Success,
    Folder,
    Tree,
    Rename,
}

#[derive(Debug, Clone, Copy)]
pub struct Icon {
    pub glyph: char,
    pub tone: Tone,
}

impl Icon {
    pub fn color(self) -> Color32 {
        let colors = theme::palette();
        match self.tone {
            Tone::Text => colors.text,
            Tone::Muted => colors.text_muted,
            Tone::Accent => colors.accent,
            Tone::Danger => colors.danger,
            Tone::Success => colors.success,
            Tone::Folder => colors.folder_icon,
            Tone::Tree => colors.tree_icon,
            Tone::Rename => colors.rename_icon,
        }
    }
}

const fn icon(glyph: char, tone: Tone) -> Icon {
    Icon { glyph, tone }
}

pub const FILE_CONTENT_EXPORT: Icon = icon('\u{f1c8}', Tone::Folder);
pub const TREE_GENERATOR: Icon = icon('\u{ea63}', Tone::Tree);
pub const FOLDER_CONTENT_EXPORT: Icon = icon('\u{e875}', Tone::Accent);
pub const FILE_RENAME: Icon = icon('\u{e9a2}', Tone::Rename);
pub const OPTIONS: Icon = icon('\u{e8b8}', Tone::Muted);

pub const UNDO: Icon = icon('\u{e166}', Tone::Text);
pub const REDO: Icon = icon('\u{e15a}', Tone::Text);
pub const SAVE: Icon = icon('\u{e161}', Tone::Text);
pub const FULLSCREEN: Icon = icon('\u{e5d0}', Tone::Text);
pub const FULLSCREEN_EXIT: Icon = icon('\u{e5d1}', Tone::Text);
/// Shown in the light theme: switches to the dark theme.
pub const DARK_MODE: Icon = icon('\u{e51c}', Tone::Text);
/// Shown in the dark theme: switches to the light theme.
pub const LIGHT_MODE: Icon = icon('\u{e518}', Tone::Text);

pub const WARNING: Icon = icon('\u{e002}', Tone::Danger);
pub const ERROR: Icon = icon('\u{e000}', Tone::Danger);
pub const SUCCESS: Icon = icon('\u{e86c}', Tone::Success);
pub const INFO: Icon = icon('\u{e88e}', Tone::Accent);

pub const LINK: Icon = icon('\u{e157}', Tone::Accent);
pub const LINK_OFF: Icon = icon('\u{e16f}', Tone::Danger);

pub const ACCOUNT_TREE: char = '\u{e97a}';
pub const ADD: char = '\u{e145}';
pub const SAVE_GLYPH: char = '\u{e161}';
pub const VISIBILITY: char = '\u{e8f4}';
pub const EDIT: char = '\u{e3c9}';
pub const DELETE: char = '\u{e872}';
pub const ARROW_UPWARD: char = '\u{e5d8}';
pub const ARROW_DOWNWARD: char = '\u{e5db}';
pub const FUNCTIONS: char = '\u{e24a}';
pub const CHECK_CIRCLE: char = '\u{e86c}';
pub const ERROR_GLYPH: char = '\u{e000}';
pub const INFO_GLYPH: char = '\u{e88e}';
pub const FOLDER: char = '\u{e2c7}';
pub const DESCRIPTION: char = '\u{e873}';
pub const KEYBOARD_ARROW_LEFT: char = '\u{e314}';
pub const CLOSE: char = '\u{e5cd}';
pub const NOTES: char = '\u{e26c}';
pub const SEARCH: char = '\u{e8b6}';
