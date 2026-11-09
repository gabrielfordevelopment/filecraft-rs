//! The Tree Generator tab.

use std::path::PathBuf;

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use filecraft_core::folder_tree::FolderTree;
use filecraft_core::settings::{DEFAULT_TREE_STRUCTURE_NAME, FolderState, TreeGeneratorSettings};
use filecraft_core::tree_generator::{
    TreeGenerationError, TreeGenerationMode, TreeGenerationRequest, generate_tree_file,
};

use crate::background::Task;
use crate::dialog::Dialog;
use crate::export_footer::{self, ExportOptions, FooterButton, Stat, StatTone};
use crate::folder_tree_view::{self, FoldersAction};
use crate::icons;
use crate::requests::{ConfirmAction, TabRequest};
use crate::state::{SharedState, TreeTab};
use crate::strings;
use crate::theme;
use crate::toast::Toast;
use crate::widgets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fullscreen {
    None,
    Folders,
    Settings,
}

impl Fullscreen {
    fn toggled(self, target: Self) -> Self {
        if self == target { Self::None } else { target }
    }
}

pub struct TreeGeneratorTab {
    options: ExportOptions,
    mode: TreeGenerationMode,
    fullscreen: Fullscreen,
    running: Option<Task<Result<PathBuf, TreeGenerationError>>>,
    /// The file name chosen when the generation was requested, used once it is confirmed.
    pending_file_name: Option<String>,
}

impl Default for TreeGeneratorTab {
    fn default() -> Self {
        Self {
            options: ExportOptions::new(DEFAULT_TREE_STRUCTURE_NAME),
            mode: TreeGenerationMode::Structured,
            fullscreen: Fullscreen::None,
            running: None,
            pending_file_name: None,
        }
    }
}

impl TreeGeneratorTab {
    /// The tab's part of the saved state.
    pub fn settings(
        &self,
        folder_tree_state: Vec<FolderState>,
        base: &TreeGeneratorSettings,
    ) -> TreeGeneratorSettings {
        TreeGeneratorSettings {
            folder_tree_state,
            generation_mode: self.mode,
            output_file_name: self.options.output_file_name.clone(),
            append_timestamp: self.options.append_timestamp,
            extra: base.extra.clone(),
        }
    }

    pub fn apply_settings(&mut self, settings: &TreeGeneratorSettings) {
        self.options.output_file_name = settings.output_file_name.clone();
        self.options.append_timestamp = settings.append_timestamp;
        self.mode = settings.generation_mode;
    }

    fn included_folders_count(shared: &SharedState) -> usize {
        shared
            .tree(TreeTab::TreeGenerator)
            .map_or(0, FolderTree::selected_count)
    }

    /// Asks before generating. The question warns when the file would be replaced.
    fn request_generation(&mut self, shared: &SharedState) -> TabRequest {
        let (file_name, path) = self.options.target(&shared.destination_path);
        self.pending_file_name = Some(file_name);
        let dialog = Dialog::export_confirmation(
            strings::TREE_GEN_GENERATE_TITLE,
            strings::TREE_GEN_CONFIRM_GENERATE_MESSAGE,
            &path,
            (
                Self::included_folders_count(shared),
                strings::CONFIRMATION_FOLDERS_AFFECTED_LABEL,
            ),
        );
        TabRequest::Confirm(dialog, ConfirmAction::GenerateTree)
    }

    /// Starts writing the tree structure file on a background thread.
    pub fn start_generation(&mut self, shared: &SharedState, ctx: &egui::Context) {
        let Some(tree) = shared.tree(TreeTab::TreeGenerator) else {
            return;
        };
        let file_name = self
            .pending_file_name
            .take()
            .unwrap_or_else(|| self.options.target(&shared.destination_path).0);
        let request = TreeGenerationRequest {
            source_path: PathBuf::from(&shared.source_path),
            destination_path: PathBuf::from(&shared.destination_path),
            file_name,
            mode: self.mode,
            excluded_path_keys: tree.excluded_path_keys(),
            ignored_folder_names: shared.ignored_folder_names(),
        };
        self.running = Some(Task::spawn(ctx, move || generate_tree_file(&request)));
    }

    /// Collects a finished background generation and turns it into a result dialog.
    fn poll_generation(&mut self) -> Option<TabRequest> {
        let result = self.running.as_ref()?.poll()?;
        self.running = None;
        let dialog = match result {
            Ok(Ok(path)) => {
                let toast =
                    Toast::success(strings::TOAST_TREE_CREATED, String::new()).with_file(path);
                return Some(TabRequest::Notify(toast));
            }
            Ok(Err(error)) => error_dialog(&error.to_string()),
            Err(failed) => error_dialog(&failed.to_string()),
        };
        Some(TabRequest::ShowDialog(dialog))
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut SharedState) -> Option<TabRequest> {
        let mut request = self.poll_generation();

        let full = ui.available_rect_before_wrap();
        let count = Self::included_folders_count(shared);
        let stats = [Stat::count(
            count,
            strings::TREE_GEN_FOLDERS_AFFECTED_LABEL,
            StatTone::Highlight,
        )];
        let button = FooterButton {
            text: strings::TREE_GEN_GENERATE_BUTTON,
            disabled_reason: self.options.blocker(shared),
            busy_text: self
                .running
                .is_some()
                .then_some(strings::TREE_GEN_GENERATING_BUTTON),
        };
        let bottom_height = if self.fullscreen == Fullscreen::None {
            export_footer::footer_height(ui, full.width(), &stats, None, &button)
        } else {
            0.0
        };
        let top = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - bottom_height));
        let (folders_rect, settings_rect) = match self.fullscreen {
            Fullscreen::None => {
                let split = top.left() + top.width() * 0.7;
                (
                    Some(Rect::from_min_max(top.min, pos2(split, top.bottom()))),
                    Some(Rect::from_min_max(pos2(split, top.top()), top.max)),
                )
            }
            Fullscreen::Folders => (Some(top), None),
            Fullscreen::Settings => (None, Some(top)),
        };

        let linked = shared.linked_peers(TreeTab::TreeGenerator);
        if let Some(rect) = folders_rect {
            match folder_tree_view::folders_group(
                ui,
                rect.shrink(5.0),
                "tree_generator_folders",
                shared.tree_mut(TreeTab::TreeGenerator),
                &linked,
                self.fullscreen == Fullscreen::Folders,
            ) {
                Some(FoldersAction::ToggleFullscreen) => {
                    self.fullscreen = self.fullscreen.toggled(Fullscreen::Folders);
                }
                Some(FoldersAction::ChooseSource) => {
                    request = Some(TabRequest::ChooseSourceFolder);
                }
                None => {}
            }
        }
        if let Some(rect) = settings_rect {
            self.settings_ui(ui, rect.shrink(5.0));
        }
        if bottom_height > 0.0 {
            let bottom = Rect::from_min_max(pos2(full.left(), top.bottom()), full.max);
            if export_footer::footer_ui(
                ui,
                bottom,
                "tree_generator_file_name",
                &mut self.options,
                &stats,
                None,
                button,
            ) {
                request = Some(self.request_generation(shared));
            }
        }
        ui.advance_cursor_after_rect(full);
        request
    }

    /// The "Generate as" card: a choosable card with a sample for each output format.
    fn settings_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let mut toggle_fullscreen = false;
        let fullscreen_active = self.fullscreen == Fullscreen::Settings;
        widgets::card_in(
            ui,
            rect,
            true,
            |ui| {
                widgets::card_title(ui, strings::TREE_GEN_GENERATE_AS_HEADER);
            },
            |ui| toggle_fullscreen = widgets::fullscreen_header_toggle(ui, fullscreen_active),
            |ui| {
                let colors = theme::palette();
                let structured = self.mode == TreeGenerationMode::Structured;
                let paths_sample: Vec<String> = strings::TREE_GEN_PATHS_ONLY_SAMPLE
                    .iter()
                    .map(|path| path.replace('/', std::path::MAIN_SEPARATOR_STR))
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("tree_generator_modes")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if mode_card(
                            ui,
                            structured,
                            (icons::ACCOUNT_TREE, colors.tree_icon),
                            strings::TREE_GEN_STRUCTURED_TREE_OPTION,
                            &strings::TREE_GEN_STRUCTURED_SAMPLE,
                        ) {
                            self.mode = TreeGenerationMode::Structured;
                        }
                        ui.add_space(MODE_CARD_GAP);
                        if mode_card(
                            ui,
                            !structured,
                            (icons::NOTES, colors.text_muted),
                            strings::TREE_GEN_PATHS_ONLY_OPTION,
                            &paths_sample,
                        ) {
                            self.mode = TreeGenerationMode::PathsOnly;
                        }
                    });
            },
        );
        if toggle_fullscreen {
            self.fullscreen = self.fullscreen.toggled(Fullscreen::Settings);
        }
    }
}

const MODE_CARD_GAP: f32 = 8.0;
const MODE_CARD_PADDING: f32 = 10.0;
const MODE_TITLE_HEIGHT: f32 = 20.0;
const MODE_SAMPLE_PADDING: f32 = 6.0;
const MODE_SAMPLE_SIZE: f32 = 11.5;

/// One output format: a radio button, an icon and the name, then a few lines that show what the
/// output looks like. The selected card has an accent border. Returns true when it was clicked.
fn mode_card(
    ui: &mut Ui,
    selected: bool,
    (glyph, glyph_color): (char, Color32),
    title: &str,
    sample: &[impl AsRef<str>],
) -> bool {
    let colors = theme::palette();
    let sample_font = FontId::monospace(MODE_SAMPLE_SIZE);
    let line = widgets::text_height(ui, sample_font.clone());
    let sample_height = line * sample.len() as f32 + MODE_SAMPLE_PADDING * 2.0;
    let height = MODE_CARD_PADDING * 2.0 + MODE_TITLE_HEIGHT + 6.0 + sample_height;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hover = widgets::hover_amount(ui, &response, !selected);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let (fill, border, border_width) = if selected {
            (
                widgets::lerp_color(colors.surface, colors.accent_soft, 0.5),
                colors.accent,
                2.0,
            )
        } else {
            (
                colors.surface,
                widgets::lerp_color(colors.border_subtle, colors.accent, hover),
                1.0,
            )
        };
        painter.rect(
            rect,
            6,
            fill,
            Stroke::new(border_width, border),
            StrokeKind::Inside,
        );
        let left = rect.left() + MODE_CARD_PADDING;
        let title_y = rect.top() + MODE_CARD_PADDING + MODE_TITLE_HEIGHT / 2.0;
        widgets::paint_radio(ui, pos2(left + 7.0, title_y), selected, response.hovered());
        widgets::paint_glyph(
            ui,
            pos2(left + 14.0 + 8.0 + 9.0, title_y),
            glyph,
            18.0,
            glyph_color,
        );
        painter.text(
            pos2(left + 14.0 + 8.0 + 18.0 + 6.0, title_y),
            Align2::LEFT_CENTER,
            title,
            theme::semibold(theme::BODY_SIZE),
            colors.text,
        );
        let sample_rect = Rect::from_min_max(
            pos2(left, title_y + MODE_TITLE_HEIGHT / 2.0 + 6.0),
            pos2(
                rect.right() - MODE_CARD_PADDING,
                rect.bottom() - MODE_CARD_PADDING,
            ),
        );
        let sample_fill = if selected {
            colors.surface
        } else {
            colors.badge_background
        };
        painter.rect(
            sample_rect,
            4,
            sample_fill,
            Stroke::new(1.0, colors.border_subtle),
            StrokeKind::Inside,
        );
        let clipped = painter.with_clip_rect(sample_rect.shrink(1.0).intersect(ui.clip_rect()));
        for (index, text) in sample.iter().enumerate() {
            clipped.text(
                pos2(
                    sample_rect.left() + 8.0,
                    sample_rect.top() + MODE_SAMPLE_PADDING + line * index as f32,
                ),
                Align2::LEFT_TOP,
                text.as_ref(),
                sample_font.clone(),
                colors.text_muted,
            );
        }
    }
    response.clicked()
}

fn error_dialog(message: &str) -> Dialog {
    Dialog::notification(
        strings::COMMON_ERROR_TITLE,
        strings::tree_gen_error(message),
        icons::ERROR,
    )
}
