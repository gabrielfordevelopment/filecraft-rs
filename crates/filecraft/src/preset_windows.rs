//! Preset windows: the preset details, the load summary and the path presets, shown as modal
//! windows.

use egui::{
    Align, Align2, Color32, Layout, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use filecraft_core::preset_details::{
    path_preset_files, paths_tree_text, save_preset_files, save_preset_folders, save_preset_summary,
};
use filecraft_core::presets::{PathPreset, SavePreset};

use crate::dialog::ModalLayer;
use crate::icons;
use crate::ignored_comments::dialog_frame;
use crate::preset_list::{PresetList, PresetListAction, PresetListItem};
use crate::strings;
use crate::theme;
use crate::widgets::{self, ButtonKind, ButtonStyle};

/// The window size, kept inside the application window.
fn window_size(ctx: &egui::Context, width: f32, height: f32) -> egui::Vec2 {
    let screen = ctx.content_rect();
    vec2(
        width.min(screen.width() - 40.0),
        height.min(screen.height() - 40.0),
    )
}

/// Read-only monospace text that can be selected and scrolled.
fn read_only_text(ui: &mut Ui, id_salt: &str, text: &str, size: f32, color: Color32) {
    egui::ScrollArea::both()
        .id_salt(id_salt)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut text = text;
            let font = egui::FontId::monospace(size);
            let mut layouter = |ui: &Ui, buffer: &dyn egui::TextBuffer, _wrap_width: f32| {
                ui.painter().layout_job(egui::text::LayoutJob::simple(
                    buffer.as_str().to_owned(),
                    font.clone(),
                    color,
                    f32::INFINITY,
                ))
            };
            egui::TextEdit::multiline(&mut text)
                .id_salt((id_salt, "text"))
                .font(egui::FontId::monospace(size))
                .text_color(color)
                .layouter(&mut layouter)
                .frame(egui::Frame::new())
                .desired_width(f32::INFINITY)
                .show(ui);
        });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailList {
    Files,
    Folders,
}

/// A statistic badge. Badges with a list open it when clicked.
struct Badge {
    label: &'static str,
    value: String,
    glyph: char,
    list: Option<DetailList>,
}

pub struct PresetDetailsWindow {
    name: String,
    last_modified: chrono::NaiveDateTime,
    description: String,
    badges: Vec<Badge>,
    content_header: &'static str,
    summary: String,
    files: Vec<String>,
    folders: Vec<String>,
    detail: Option<DetailList>,
    tree_mode: bool,
    layer: ModalLayer,
}

impl PresetDetailsWindow {
    fn new(name: &str, last_modified: chrono::NaiveDateTime, description: &str) -> Self {
        Self {
            name: name.to_owned(),
            last_modified,
            description: description.to_owned(),
            badges: Vec::new(),
            content_header: "",
            summary: String::new(),
            files: Vec::new(),
            folders: Vec::new(),
            detail: None,
            tree_mode: false,
            layer: ModalLayer::new(),
        }
    }

    pub fn for_save_preset(preset: &SavePreset) -> Self {
        let mut window = Self::new(&preset.name, preset.last_modified, &preset.description);
        window.files = save_preset_files(preset);
        window.folders = save_preset_folders(preset);
        window.badges = vec![
            Badge {
                label: strings::DETAILS_APP_VERSION,
                value: preset.app_version.clone(),
                glyph: icons::INFO_GLYPH,
                list: None,
            },
            Badge {
                label: strings::DETAILS_FOLDERS_CONFIGURED,
                value: preset.statistics.folder_count.to_string(),
                glyph: icons::FOLDER,
                list: Some(DetailList::Folders),
            },
            Badge {
                label: strings::DETAILS_EXPLICIT_FILES,
                value: preset.statistics.file_count.to_string(),
                glyph: icons::DESCRIPTION,
                list: Some(DetailList::Files),
            },
        ];
        window.content_header = strings::DETAILS_CONFIGURATION_SUMMARY_HEADER;
        window.summary = save_preset_summary(preset);
        window
    }

    pub fn for_path_preset(preset: &PathPreset) -> Self {
        let description = strings::path_preset_description(preset.file_paths.len());
        let mut window = Self::new(&preset.name, preset.last_modified, &description);
        window.files = path_preset_files(preset);
        window.badges = vec![Badge {
            label: strings::DETAILS_TOTAL_FILES,
            value: preset.file_paths.len().to_string(),
            glyph: icons::DESCRIPTION,
            list: Some(DetailList::Files),
        }];
        window.content_header = strings::DETAILS_INCLUDED_FILES_HEADER;
        window.summary = window.files.join("\n");
        window
    }

    fn open_list(&mut self, list: DetailList) {
        let paths = match list {
            DetailList::Files => &self.files,
            DetailList::Folders => &self.folders,
        };
        if !paths.is_empty() {
            self.detail = Some(list);
            self.tree_mode = false;
        }
    }

    fn detail_title(list: DetailList) -> &'static str {
        match list {
            DetailList::Files => strings::DETAILS_FILES_TITLE,
            DetailList::Folders => strings::DETAILS_FOLDERS_TITLE,
        }
    }

    /// Returns true when the window was closed.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let size = window_size(ctx, 800.0, 550.0);
        let modal = crate::dialog::stacked_modal(ctx, self.layer)
            .frame(dialog_frame().inner_margin(20))
            .show(ctx, |ui| {
                ui.set_width(size.x - 40.0);
                ui.set_height(size.y - 40.0);
                self.contents(ui)
            });
        modal.inner || ctx.input(|input| input.key_pressed(egui::Key::Escape))
    }

    fn contents(&mut self, ui: &mut Ui) -> bool {
        let area = ui.max_rect();
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        match self.detail {
            None => {
                widgets::label(ui, &self.name, theme::bold(24.0), theme::palette().accent);
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    widgets::label(
                        ui,
                        strings::LAST_MODIFIED_LABEL,
                        theme::body(theme::BODY_SIZE),
                        theme::palette().text_hint,
                    );
                    ui.add_space(5.0);
                    let now = chrono::Local::now().naive_local();
                    widgets::label(
                        ui,
                        &strings::preset_date(self.last_modified, now),
                        theme::semibold(theme::BODY_SIZE),
                        theme::palette().text,
                    );
                });
                if !self.description.is_empty() {
                    ui.add_space(5.0);
                    ui.label(
                        egui::RichText::new(&self.description)
                            .font(theme::body(theme::BODY_SIZE))
                            .color(theme::palette().text_hint)
                            .italics(),
                    );
                }
            }
            Some(list) => {
                ui.horizontal(|ui| {
                    let back = widgets::icon_button(
                        ui,
                        icons::Icon {
                            glyph: icons::KEYBOARD_ARROW_LEFT,
                            tone: icons::Tone::Accent,
                        },
                        32.0,
                        32.0,
                        true,
                    )
                    .on_hover_text(strings::DETAILS_BACK);
                    if back.clicked() {
                        self.detail = None;
                    }
                    ui.add_space(10.0);
                    widgets::label(
                        ui,
                        Self::detail_title(list),
                        theme::bold(24.0),
                        theme::palette().accent,
                    );
                });
            }
        }
        ui.add_space(20.0);
        self.badges_ui(ui);
        ui.add_space(20.0);

        let close_height = widgets::control_height(ui);
        let content_rect = Rect::from_min_max(
            pos2(area.left(), ui.cursor().top()),
            pos2(area.right(), area.bottom() - close_height - 15.0),
        );
        self.content_ui(ui, content_rect);

        let close_rect =
            Rect::from_min_max(pos2(area.left(), area.bottom() - close_height), area.max);
        let mut close_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(close_rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        widgets::button(
            &mut close_ui,
            strings::COMMON_CLOSE_BUTTON,
            ButtonKind::Secondary,
            true,
            vec2(80.0, 0.0),
        )
        .clicked()
    }

    fn badges_ui(&mut self, ui: &mut Ui) {
        let mut open = None;
        ui.horizontal(|ui| {
            for badge in &self.badges {
                let selected = badge.list.is_some() && badge.list == self.detail;
                if badge_ui(ui, badge, selected).clicked()
                    && let Some(list) = badge.list
                {
                    open = Some(list);
                }
                ui.add_space(10.0);
            }
        });
        if let Some(list) = open {
            self.open_list(list);
        }
    }

    fn content_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let detail = self.detail;
        let mut toggle = false;
        let header_height = match detail {
            Some(_) => 24.0_f32.max(widgets::text_header_height(ui)),
            None => widgets::text_header_height(ui),
        };
        let text = match detail {
            None => self.summary.clone(),
            Some(list) => {
                let paths = match list {
                    DetailList::Files => &self.files,
                    DetailList::Folders => &self.folders,
                };
                if self.tree_mode {
                    paths_tree_text(paths)
                } else {
                    paths.join("\n")
                }
            }
        };
        let tree_mode = self.tree_mode;
        widgets::group_box_in(
            ui,
            rect,
            true,
            header_height,
            |ui| match detail {
                None => {
                    widgets::header_text(ui, self.content_header, false);
                }
                Some(list) => {
                    widgets::header_text(ui, Self::detail_title(list), true);
                    ui.add_space(15.0);
                    let text = if tree_mode {
                        strings::DETAILS_SWITCH_TO_LIST
                    } else {
                        strings::DETAILS_SWITCH_TO_TREE
                    };
                    let style = ButtonStyle {
                        font: 11.0,
                        horizontal_padding: 8.0,
                        height: Some(24.0),
                        ..ButtonStyle::new(ButtonKind::Secondary, vec2(0.0, 24.0))
                    };
                    toggle = widgets::styled_button(ui, text, style, true).clicked();
                }
            },
            |ui| {
                read_only_text(
                    ui,
                    "preset_details_text",
                    &text,
                    12.0,
                    theme::palette().text,
                )
            },
        );
        if toggle {
            self.tree_mode = !self.tree_mode;
        }
    }
}

/// A statistic badge: a glyph, the value in bold and the label below it.
fn badge_ui(ui: &mut Ui, badge: &Badge, selected: bool) -> egui::Response {
    let interactive = badge.list.is_some();
    let value_font = theme::bold(16.0);
    let label_font = theme::body(11.0);
    let text_width = widgets::text_width(ui, &badge.value, value_font.clone())
        .max(widgets::text_width(ui, badge.label, label_font.clone()));
    let value_height = widgets::text_height(ui, value_font.clone());
    let label_height = widgets::text_height(ui, label_font.clone());
    let size = vec2(
        (15.0 + 24.0 + 10.0 + text_width + 15.0).max(120.0),
        10.0 + (value_height + label_height).max(24.0) + 10.0,
    );
    let sense = if interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let (fill, border, width) = if !interactive {
            (
                theme::palette().badge_inactive,
                theme::palette().border,
                1.0,
            )
        } else if selected {
            (theme::palette().badge_pressed, theme::palette().accent, 2.0)
        } else if response.is_pointer_button_down_on() {
            (theme::palette().badge_pressed, theme::palette().accent, 1.0)
        } else if response.hovered() {
            (theme::palette().badge_hover, theme::palette().accent, 1.0)
        } else {
            (
                theme::palette().badge_background,
                theme::palette().border,
                1.0,
            )
        };
        ui.painter().rect(
            rect,
            4,
            fill,
            Stroke::new(width, border),
            StrokeKind::Inside,
        );
        let (icon_color, value_color) = if interactive {
            (theme::palette().accent, theme::palette().text)
        } else {
            (theme::palette().text_hint, theme::palette().text_hint)
        };
        widgets::paint_glyph(
            ui,
            pos2(rect.left() + 15.0 + 12.0, rect.center().y),
            badge.glyph,
            24.0,
            icon_color,
        );
        let text_left = rect.left() + 15.0 + 24.0 + 10.0;
        let text_top = rect.center().y - (value_height + label_height) / 2.0;
        ui.painter().text(
            pos2(text_left, text_top),
            Align2::LEFT_TOP,
            &badge.value,
            value_font,
            value_color,
        );
        ui.painter().text(
            pos2(text_left, text_top + value_height),
            Align2::LEFT_TOP,
            badge.label,
            label_font,
            theme::palette().text_hint,
        );
    }
    if interactive {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// The outcome of loading a path preset. Paths are shown relative to
/// the source folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathPresetLoadResult {
    pub total: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub loaded: Vec<String>,
    pub not_found: Vec<String>,
}

pub struct LoadSummaryWindow {
    result: PathPresetLoadResult,
    loaded_visible: bool,
    not_found_visible: bool,
    layer: ModalLayer,
}

impl LoadSummaryWindow {
    pub fn new(result: PathPresetLoadResult) -> Self {
        Self {
            result,
            loaded_visible: false,
            not_found_visible: false,
            layer: ModalLayer::new(),
        }
    }

    /// Returns true when the window was closed.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let size = window_size(ctx, 600.0, 650.0);
        let modal = crate::dialog::stacked_modal(ctx, self.layer)
            .frame(dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(size.x - 30.0);
                ui.set_height(size.y - 30.0);
                self.contents(ui)
            });
        modal.inner
            || ctx.input(|input| {
                input.key_pressed(egui::Key::Escape) || input.key_pressed(egui::Key::Enter)
            })
    }

    fn contents(&mut self, ui: &mut Ui) -> bool {
        let area = ui.max_rect();
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        widgets::label(
            ui,
            strings::PRESET_SUMMARY_TITLE,
            theme::bold(20.0),
            theme::palette().accent,
        );
        ui.add_space(10.0);
        summary_row(
            ui,
            SummaryRow {
                glyph: icons::FUNCTIONS,
                glyph_color: theme::palette().accent,
                label: strings::PRESET_SUMMARY_TOTAL,
                text_color: theme::palette().text,
                value: self.result.total,
                fill: theme::palette().badge_background,
                border: theme::palette().border,
                clickable: false,
            },
        );
        ui.add_space(4.0);
        let width = ui.available_width();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            count_box(
                ui,
                (width - 4.0) / 2.0,
                strings::PRESET_SUMMARY_CHANGED,
                self.result.changed,
                theme::palette().text,
            );
            ui.add_space(4.0);
            count_box(
                ui,
                (width - 4.0) / 2.0,
                strings::PRESET_SUMMARY_UNCHANGED,
                self.result.unchanged,
                theme::palette().text_hint,
            );
        });
        ui.add_space(8.0);

        let has_loaded = !self.result.loaded.is_empty();
        if summary_row(
            ui,
            SummaryRow {
                glyph: icons::CHECK_CIRCLE,
                glyph_color: theme::palette().success,
                label: strings::PRESET_SUMMARY_LOADED,
                text_color: theme::palette().text,
                value: self.result.loaded.len(),
                fill: theme::palette().badge_background,
                border: theme::palette().border,
                clickable: has_loaded,
            },
        )
        .clicked()
        {
            self.loaded_visible = !self.loaded_visible;
        }
        ui.add_space(4.0);

        let button_height = 35.0;
        let row_height = ui.cursor().height().max(0.0);
        let _ = row_height;
        let not_found_row = 8.0 + 24.0 + 8.0 + 4.0;
        let lists_bottom = area.bottom() - button_height - 5.0;
        let open_lists = usize::from(self.loaded_visible) + usize::from(self.not_found_visible);
        let free =
            (lists_bottom - ui.cursor().top() - not_found_row - 10.0 * open_lists as f32).max(0.0);
        let list_height = if open_lists > 0 {
            free / open_lists as f32
        } else {
            0.0
        };
        if self.loaded_visible {
            list_box(
                ui,
                "loaded_paths",
                &self.result.loaded.join("\n"),
                list_height,
                theme::palette().success,
            );
            ui.add_space(10.0);
        }

        let has_not_found = !self.result.not_found.is_empty();
        if summary_row(
            ui,
            SummaryRow {
                glyph: icons::ERROR_GLYPH,
                glyph_color: theme::palette().danger,
                label: strings::PRESET_SUMMARY_NOT_FOUND,
                text_color: theme::palette().danger,
                value: self.result.not_found.len(),
                fill: theme::palette().danger_soft,
                border: theme::palette().danger,
                clickable: has_not_found,
            },
        )
        .clicked()
        {
            self.not_found_visible = !self.not_found_visible;
        }
        ui.add_space(4.0);
        if self.not_found_visible {
            list_box(
                ui,
                "not_found_paths",
                &self.result.not_found.join("\n"),
                list_height,
                theme::palette().danger,
            );
        }

        let close_rect =
            Rect::from_min_max(pos2(area.left(), area.bottom() - button_height), area.max);
        let mut close_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(close_rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let style = ButtonStyle {
            font: 14.0,
            height: Some(button_height),
            ..ButtonStyle::new(ButtonKind::Primary, vec2(100.0, button_height))
        };
        widgets::styled_button(&mut close_ui, strings::COMMON_CLOSE_BUTTON, style, true).clicked()
    }
}

struct SummaryRow {
    glyph: char,
    glyph_color: Color32,
    label: &'static str,
    text_color: Color32,
    value: usize,
    fill: Color32,
    border: Color32,
    clickable: bool,
}

/// A full-width row with a glyph, a label and the value on the right.
fn summary_row(ui: &mut Ui, row: SummaryRow) -> egui::Response {
    let height = 8.0 + 24.0 + 8.0;
    let sense = if row.clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), sense);
    if ui.is_rect_visible(rect) {
        let danger = row.fill == theme::palette().danger_soft;
        let fill = if !row.clickable || danger {
            row.fill
        } else if response.is_pointer_button_down_on() {
            theme::palette().badge_pressed
        } else if response.hovered() {
            theme::palette().badge_hover
        } else {
            row.fill
        };
        let opacity = if danger && row.clickable && response.is_pointer_button_down_on() {
            0.6
        } else if danger && row.clickable && response.hovered() {
            0.8
        } else {
            1.0
        };
        let tint = |color: Color32| theme::faded(color, opacity);
        ui.painter().rect(
            rect,
            4,
            tint(fill),
            Stroke::new(1.0, tint(row.border)),
            StrokeKind::Inside,
        );
        let inner = rect.shrink(8.0);
        widgets::paint_glyph(
            ui,
            pos2(inner.left() + 10.0, inner.center().y),
            row.glyph,
            20.0,
            tint(row.glyph_color),
        );
        ui.painter().text(
            pos2(inner.left() + 20.0 + 10.0, inner.center().y),
            Align2::LEFT_CENTER,
            row.label,
            theme::body(14.0),
            tint(row.text_color),
        );
        let value_color = if row.glyph_color == theme::palette().accent {
            theme::palette().text
        } else {
            row.glyph_color
        };
        ui.painter().text(
            pos2(inner.right() - 10.0, inner.center().y),
            Align2::RIGHT_CENTER,
            row.value.to_string(),
            theme::bold(18.0),
            tint(value_color),
        );
    }
    if row.clickable {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A bordered box with a small label and a large value, for the changed and unchanged counts.
fn count_box(ui: &mut Ui, width: f32, label: &str, value: usize, value_color: Color32) {
    let label_height = widgets::text_height(ui, theme::body(12.0));
    let value_height = widgets::text_height(ui, theme::bold(18.0));
    let (rect, _) = ui.allocate_exact_size(
        vec2(width, 8.0 + label_height + 2.0 + value_height + 8.0),
        Sense::hover(),
    );
    ui.painter().rect(
        rect,
        4,
        theme::palette().badge_background,
        Stroke::new(1.0, theme::palette().border),
        StrokeKind::Inside,
    );
    let inner = rect.shrink(8.0);
    ui.painter().text(
        inner.min,
        Align2::LEFT_TOP,
        label,
        theme::body(12.0),
        theme::palette().text_hint,
    );
    ui.painter().text(
        pos2(inner.left(), inner.top() + label_height + 2.0),
        Align2::LEFT_TOP,
        value.to_string(),
        theme::bold(18.0),
        value_color,
    );
}

/// The read-only list of loaded or missing paths.
fn list_box(ui: &mut Ui, id_salt: &str, text: &str, height: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect(
        rect,
        4,
        theme::palette().surface,
        Stroke::new(1.0, theme::palette().border),
        StrokeKind::Inside,
    );
    let mut inner = ui.new_child(UiBuilder::new().max_rect(rect.shrink(8.0)));
    read_only_text(&mut inner, id_salt, text, 13.0, color);
}

pub enum PathPresetsEvent {
    Action(PresetListAction),
    Close,
}

/// The "Path Presets Manager" window with its preset list.
pub struct PathPresetsWindow {
    pub list: PresetList,
    layer: ModalLayer,
}

impl PathPresetsWindow {
    pub fn new(list: PresetList) -> Self {
        Self {
            list,
            layer: ModalLayer::new(),
        }
    }

    /// `topmost` is false while a dialog is shown above the window, so Escape closes the dialog
    /// only.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        items: &[PresetListItem],
        topmost: bool,
    ) -> Option<PathPresetsEvent> {
        let size = window_size(ctx, 450.0, 550.0);
        let modal = crate::dialog::stacked_modal(ctx, self.layer)
            .frame(dialog_frame())
            .show(ctx, |ui| {
                ui.set_width(size.x - 30.0);
                ui.set_height(size.y - 30.0);
                let area = ui.max_rect();
                let title_height = widgets::text_height(ui, theme::semibold(16.0)).max(24.0);
                let title_rect = Rect::from_min_size(area.min, vec2(area.width(), title_height));
                ui.painter().text(
                    pos2(title_rect.left(), title_rect.center().y),
                    Align2::LEFT_CENTER,
                    strings::PATH_PRESET_TITLE,
                    theme::semibold(16.0),
                    theme::palette().text,
                );
                let mut close_ui = ui.new_child(
                    UiBuilder::new()
                        .max_rect(title_rect)
                        .layout(Layout::right_to_left(Align::Center)),
                );
                let close = widgets::icon_button(
                    &mut close_ui,
                    icons::Icon {
                        glyph: icons::CLOSE,
                        tone: icons::Tone::Text,
                    },
                    24.0,
                    18.0,
                    true,
                )
                .on_hover_text(strings::COMMON_CLOSE_BUTTON)
                .clicked();
                let list_rect =
                    Rect::from_min_max(pos2(area.left(), title_rect.bottom() + 10.0), area.max);
                let action = self.list.ui(
                    ui,
                    list_rect,
                    items,
                    strings::PATH_PRESET_SAVE_CURRENT_BUTTON,
                );
                if close {
                    Some(PathPresetsEvent::Close)
                } else {
                    action.map(PathPresetsEvent::Action)
                }
            });
        if modal.inner.is_some() {
            return modal.inner;
        }
        if topmost && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            return Some(PathPresetsEvent::Close);
        }
        None
    }
}
