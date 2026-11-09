//! Modal message, confirmation and input dialogs: the icon on a soft circle, the title and the
//! content in a column next to it, and the buttons below a divider.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Align, Layout, Margin, Modal, Sense, Vec2, vec2};

use crate::icons::Icon;
use crate::strings;
use crate::theme;
use crate::widgets::{self, ButtonKind};

const DIALOG_WIDTH: f32 = 460.0;
/// Space between the dialog border and its content.
const PADDING: i8 = 20;
const ICON_SIZE: f32 = 22.0;
/// Diameter of the soft circle behind the icon.
const ICON_BADGE_SIZE: f32 = 40.0;
/// Space between the icon circle and the column of the title and the content.
const ICON_GAP: f32 = 14.0;
/// How strongly the icon color tints the circle behind it.
const ICON_BADGE_TINT: f32 = 0.14;
const BUTTON_MIN_WIDTH: f32 = 80.0;
const MULTILINE_MAX_HEIGHT: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogResult {
    Primary,
    /// The cancel button, Escape, or the only button of a notification.
    Secondary,
    /// The optional middle button, such as "Don't save".
    Tertiary,
}

/// One tab of the copy tree confirmation: its icon, name and selected folder count.
#[derive(Debug, Clone)]
pub struct TabSummary {
    pub icon: Icon,
    pub name: &'static str,
    pub folders: usize,
}

#[derive(Debug, Clone)]
pub struct Dialog {
    pub title: String,
    pub message: String,
    /// A sentence shown below the message in the danger color, such as a warning that the action
    /// cannot be undone.
    pub emphasis: Option<&'static str>,
    /// A folder path shown in a box below the message.
    pub detail: Option<String>,
    pub icon: Option<Icon>,
    /// A count and what it counts, such as "files affected.".
    pub affected: Option<(usize, &'static str)>,
    pub primary_text: &'static str,
    pub primary_kind: ButtonKind,
    pub secondary_text: Option<&'static str>,
    pub tertiary: Option<(&'static str, ButtonKind)>,
    /// Text entered by the user, shown as a text box when present.
    pub input: Option<String>,
    /// A single-line input selects its text when shown and accepts Enter.
    single_line: bool,
    input_focused: bool,
    /// The source and destination of the copy tree confirmation.
    copy_tree: Option<(TabSummary, TabSummary)>,
    layer: ModalLayer,
}

impl Dialog {
    pub fn notification(title: &str, message: String, icon: Icon) -> Self {
        Self {
            title: title.to_owned(),
            message,
            emphasis: None,
            detail: None,
            icon: Some(icon),
            affected: None,
            primary_text: strings::COMMON_OK_BUTTON,
            primary_kind: ButtonKind::Primary,
            secondary_text: None,
            tertiary: None,
            input: None,
            single_line: false,
            input_focused: false,
            copy_tree: None,
            layer: ModalLayer::new(),
        }
    }

    pub fn confirmation(title: &str, message: String, icon: Icon) -> Self {
        Self {
            primary_text: strings::COMMON_YES_BUTTON,
            secondary_text: Some(strings::COMMON_CANCEL_BUTTON),
            ..Self::notification(title, message, icon)
        }
    }

    /// Replaces the text and the look of the primary button.
    pub fn with_primary(mut self, text: &'static str, kind: ButtonKind) -> Self {
        self.primary_text = text;
        self.primary_kind = kind;
        self
    }

    /// Shows `emphasis` below the message in the danger color.
    pub fn with_emphasis(mut self, emphasis: &'static str) -> Self {
        self.emphasis = Some(emphasis);
        self
    }

    /// Shows the folder path `detail` in a box below the message.
    pub fn with_detail(mut self, detail: String) -> Self {
        self.detail = Some(detail);
        self
    }

    pub fn with_affected(mut self, count: usize, label: &'static str) -> Self {
        self.affected = Some((count, label));
        self
    }

    /// Asks before an export: the question, the destination folder and
    /// the number of affected items. When the file already exists, the replace question is asked
    /// instead.
    pub fn export_confirmation(
        title: &str,
        question: &str,
        path: &std::path::Path,
        affected: (usize, &'static str),
    ) -> Self {
        let dialog = if path.exists() {
            Self::replace_confirmation(title, path)
        } else {
            Self::confirmation(title, question.to_owned(), crate::icons::INFO)
                .with_detail(parent_text(path))
        };
        dialog.with_affected(affected.0, affected.1)
    }

    /// Asks before an export replaces an existing file.
    pub fn replace_confirmation(title: &str, path: &std::path::Path) -> Self {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            primary_text: strings::REPLACE_FILE_BUTTON,
            primary_kind: ButtonKind::Danger,
            ..Self::confirmation(
                title,
                strings::replace_file_message(&file_name),
                crate::icons::WARNING,
            )
        }
        .with_detail(parent_text(path))
    }

    /// Save, don't save and cancel, asked before closing with unsaved changes.
    pub fn exit_confirmation(title: &str, message: String, icon: Icon) -> Self {
        Self {
            primary_text: strings::COMMON_SAVE_BUTTON,
            secondary_text: Some(strings::COMMON_CANCEL_BUTTON),
            tertiary: Some((
                strings::EXIT_CONFIRMATION_DONT_SAVE_BUTTON,
                ButtonKind::Danger,
            )),
            ..Self::notification(title, message, icon)
        }
    }

    /// A dialog without icon that edits multi-line text.
    pub fn text_input(title: &str, message: String, text: String) -> Self {
        Self {
            icon: None,
            input: Some(text),
            secondary_text: Some(strings::COMMON_CANCEL_BUTTON),
            ..Self::notification(title, message, crate::icons::INFO)
        }
    }

    /// A dialog without icon that edits one line of text.
    pub fn line_input(title: &str, message: String, text: String) -> Self {
        Self {
            single_line: true,
            ..Self::text_input(title, message, text)
        }
    }

    /// Asks whether to copy the folder tree of one tab to another.
    pub fn copy_tree_confirmation(
        title: &str,
        source: TabSummary,
        destination: TabSummary,
    ) -> Self {
        Self {
            primary_kind: ButtonKind::Primary,
            copy_tree: Some((source, destination)),
            ..Self::confirmation(title, String::new(), crate::icons::WARNING)
        }
    }

    /// Shows the dialog and returns the chosen button once it is closed. Enter selects the primary
    /// button unless a multi-line input is shown, Escape the secondary one.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<DialogResult> {
        let colors = theme::palette();
        let modal = stacked_modal(ctx, self.layer)
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .stroke(egui::Stroke::new(1.0, colors.border))
                    .corner_radius(6)
                    .inner_margin(1),
            )
            .show(ctx, |ui| {
                ui.set_width(DIALOG_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                egui::Frame::new()
                    .inner_margin(Margin::same(PADDING))
                    .show(ui, |ui| {
                        ui.set_width(DIALOG_WIDTH - f32::from(PADDING) * 2.0);
                        ui.horizontal_top(|ui| {
                            if let Some(icon) = self.icon {
                                icon_badge(ui, icon);
                                ui.add_space(ICON_GAP);
                            }
                            ui.vertical(|ui| {
                                ui.set_width(ui.available_width());
                                ui.add_space(2.0);
                                widgets::label(ui, &self.title, theme::semibold(15.0), colors.text);
                                ui.add_space(8.0);
                                self.content(ui);
                            });
                        });
                    });
                let footer = egui::Frame::new()
                    .inner_margin(Margin::symmetric(PADDING, 12))
                    .show(ui, |ui| {
                        ui.set_width(DIALOG_WIDTH - f32::from(PADDING) * 2.0);
                        self.buttons(ui)
                    });
                let top = footer.response.rect.top();
                ui.painter().line_segment(
                    [
                        egui::pos2(footer.response.rect.left(), top),
                        egui::pos2(footer.response.rect.right(), top),
                    ],
                    egui::Stroke::new(1.0, colors.border_subtle),
                );
                footer.inner
            });

        if let Some(result) = modal.inner {
            return Some(result);
        }
        if (self.input.is_none() || self.single_line)
            && ctx.input(|input| input.key_pressed(egui::Key::Enter))
        {
            return Some(DialogResult::Primary);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            return Some(DialogResult::Secondary);
        }
        None
    }

    /// The message, the emphasized sentence, the folder box, the affected count and the optional
    /// extra parts.
    fn content(&mut self, ui: &mut egui::Ui) {
        let colors = theme::palette();
        let mut first = true;
        let gap = |ui: &mut egui::Ui, first: &mut bool| {
            if !*first {
                ui.add_space(10.0);
            }
            *first = false;
        };
        if !self.message.is_empty() {
            gap(ui, &mut first);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&self.message)
                        .font(theme::body(13.0))
                        .color(colors.text),
                )
                .wrap(),
            );
        }
        if let Some(emphasis) = self.emphasis {
            gap(ui, &mut first);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(emphasis)
                        .font(theme::semibold(13.0))
                        .color(colors.danger),
                )
                .wrap(),
            );
        }
        if let Some(detail) = &self.detail {
            gap(ui, &mut first);
            egui::Frame::new()
                .fill(colors.surface_alt)
                .stroke(egui::Stroke::new(1.0, colors.border_subtle))
                .corner_radius(4)
                .inner_margin(Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        widgets::glyph(ui, crate::icons::FOLDER, 16.0, colors.folder_icon);
                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(detail)
                                    .font(theme::body(theme::BODY_SIZE))
                                    .color(colors.text_muted),
                            )
                            .wrap(),
                        );
                    });
                });
        }
        if let Some((count, label)) = self.affected {
            gap(ui, &mut first);
            ui.horizontal(|ui| widgets::badge(ui, &format!("{count} {label}")));
        }
        if let Some((source, destination)) = &self.copy_tree {
            gap(ui, &mut first);
            copy_tree_content(ui, source, destination);
        }
        if let Some(input) = &mut self.input {
            gap(ui, &mut first);
            let response = if self.single_line {
                let width = ui.available_width();
                widgets::text_box(ui, "dialog_line_input", input, width)
            } else {
                multiline_box(ui, input)
            };
            if !self.input_focused {
                response.request_focus();
                if self.single_line {
                    select_all(ui, response.id, input);
                }
                self.input_focused = true;
            }
        }
    }

    /// Primary, tertiary and secondary buttons from left to right, right-aligned.
    fn buttons(&self, ui: &mut egui::Ui) -> Option<DialogResult> {
        let min_size = vec2(BUTTON_MIN_WIDTH, 0.0);
        // A row of exactly one button height, so the footer does not grow to the free space.
        let row = vec2(ui.available_width(), widgets::control_height(ui));
        ui.allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
            let mut result = None;
            if let Some(secondary) = self.secondary_text
                && widgets::button(ui, secondary, ButtonKind::Secondary, true, min_size).clicked()
            {
                result = Some(DialogResult::Secondary);
            }
            if let Some((tertiary, kind)) = self.tertiary
                && widgets::button(ui, tertiary, kind, true, min_size).clicked()
            {
                result = Some(DialogResult::Tertiary);
            }
            if widgets::button(ui, self.primary_text, self.primary_kind, true, min_size).clicked() {
                result = Some(DialogResult::Primary);
            }
            result
        })
        .inner
    }
}

/// The dialog icon on a soft circle of its own color.
fn icon_badge(ui: &mut egui::Ui, icon: Icon) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(ICON_BADGE_SIZE), Sense::hover());
    let tint = widgets::lerp_color(theme::palette().surface, icon.color(), ICON_BADGE_TINT);
    ui.painter()
        .circle_filled(rect.center(), ICON_BADGE_SIZE / 2.0, tint);
    widgets::paint_glyph(ui, rect.center(), icon.glyph, ICON_SIZE, icon.color());
}

/// The folder of `path` as text.
fn parent_text(path: &std::path::Path) -> String {
    path.parent()
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Selects the whole text of a text edit.
fn select_all(ui: &egui::Ui, id: egui::Id, text: &str) {
    let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
    let range = egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(text.chars().count()),
    );
    state.cursor.set_char_range(Some(range));
    state.store(ui.ctx(), id);
}

/// "Copy the folder tree of" the source tab "to" the destination tab, each with its icon, bold
/// name and folder count.
fn copy_tree_content(ui: &mut egui::Ui, source: &TabSummary, destination: &TabSummary) {
    let tab = |ui: &mut egui::Ui, summary: &TabSummary| {
        ui.horizontal(|ui| {
            widgets::glyph(ui, summary.icon.glyph, 20.0, summary.icon.color());
            ui.add_space(5.0);
            widgets::label(
                ui,
                summary.name,
                theme::bold(theme::BODY_SIZE),
                theme::palette().text,
            );
            ui.add_space(4.0);
            widgets::label(
                ui,
                &strings::folder_count(summary.folders),
                theme::body(theme::BODY_SIZE),
                theme::palette().text,
            );
        });
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(strings::CONFIRMATION_COPY_TREE_PART1)
                .font(theme::body(13.0))
                .color(theme::palette().text),
        )
        .wrap(),
    );
    ui.add_space(10.0);
    tab(ui, source);
    ui.add_space(5.0);
    widgets::label(
        ui,
        strings::CONFIRMATION_COPY_TREE_PART2,
        theme::body(theme::BODY_SIZE),
        theme::palette().text,
    );
    ui.add_space(5.0);
    tab(ui, destination);
}

/// The layer of one opened modal window. Every modal gets a new layer when it is opened, so it
/// is drawn above every modal opened before it. egui keeps the earlier order of layers that all
/// want to be on top, so reusing a layer of an earlier modal could put a new modal below an older
/// one.
#[derive(Debug, Clone, Copy)]
pub struct ModalLayer(egui::Id);

impl ModalLayer {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(egui::Id::unique((
            "filecraft_modal",
            NEXT.fetch_add(1, Ordering::Relaxed),
        )))
    }
}

impl Default for ModalLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// A modal on `layer`, kept above the other areas of the window.
pub fn stacked_modal(ctx: &egui::Context, layer: ModalLayer) -> Modal {
    ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, layer.0));
    Modal::new(layer.0)
}

/// A bordered multi-line text box that scrolls after 300 px.
fn multiline_box(ui: &mut egui::Ui, text: &mut String) -> egui::Response {
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme::palette().accent))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(MULTILINE_MAX_HEIGHT)
                .show(ui, |ui| {
                    egui::TextEdit::multiline(text)
                        .id_salt("dialog_input")
                        .frame(egui::Frame::new())
                        .desired_width(ui.available_width())
                        .desired_rows(3)
                        .text_color(theme::palette().text)
                        .show(ui)
                        .response
                        .response
                })
                .inner
        })
        .inner
}
