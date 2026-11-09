//! The Options tab.

use egui::{Align, Align2, Layout, Rect, Sense, Ui, UiBuilder, pos2, vec2};

use crate::icons;
use crate::preset_list::{PresetList, PresetListAction, PresetListItem};
use crate::state::{SharedState, TreeTab};
use crate::strings;
use crate::theme;
use crate::widgets::{self, ButtonKind, ComboItem};

const GROUP_PADDING: f32 = 10.0;
const GROUP_MARGIN: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fullscreen {
    None,
    Presets,
    CopyTree,
    IgnoredFolders,
}

impl Fullscreen {
    fn toggled(self, target: Self) -> Self {
        if self == target { Self::None } else { target }
    }
}

/// What the tab asks the application to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsRequest {
    OpenSaveFolder,
    DeleteCurrentSave,
    EditIgnoredFolders,
    Preset(PresetListAction),
    CopyTree(TreeTab, TreeTab),
    Link(TreeTab, TreeTab),
    Unlink(TreeTab),
}

pub struct OptionsTab {
    fullscreen: Fullscreen,
    pub presets: PresetList,
    copy_source: Option<TreeTab>,
    copy_destination: Option<TreeTab>,
}

impl Default for OptionsTab {
    fn default() -> Self {
        Self {
            fullscreen: Fullscreen::None,
            presets: PresetList::new("save_presets"),
            copy_source: None,
            copy_destination: None,
        }
    }
}

impl OptionsTab {
    /// Sets both tab selectors back to "None", like after a copy or a new link.
    pub fn clear_tab_selection(&mut self) {
        self.copy_source = None;
        self.copy_destination = None;
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        shared: &SharedState,
        presets: &[PresetListItem],
    ) -> Option<OptionsRequest> {
        let full = ui.available_rect_before_wrap();
        let version_height = if self.fullscreen == Fullscreen::None {
            ui.text_style_height(&egui::TextStyle::Body) + 20.0
        } else {
            0.0
        };
        let top = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - version_height));
        let mut request = None;

        let (presets_rect, right_rect) = match self.fullscreen {
            Fullscreen::None => {
                let split = top.left() + top.width() * 0.6;
                (
                    Some(Rect::from_min_max(top.min, pos2(split, top.bottom()))),
                    Some(Rect::from_min_max(pos2(split, top.top()), top.max)),
                )
            }
            Fullscreen::Presets => (Some(top), None),
            Fullscreen::CopyTree | Fullscreen::IgnoredFolders => (None, Some(top)),
        };

        if let Some(rect) = presets_rect {
            request = request.or(self.presets_ui(ui, rect.shrink(GROUP_MARGIN), presets));
        }
        if let Some(rect) = right_rect {
            request = request.or(self.right_column_ui(ui, rect, shared));
        }

        if version_height > 0.0 {
            ui.painter().text(
                pos2(full.center().x, full.bottom() - 10.0),
                Align2::CENTER_BOTTOM,
                format!(
                    "{} v{}",
                    strings::COMMON_VERSION_LABEL,
                    env!("CARGO_PKG_VERSION")
                ),
                theme::body(theme::BODY_SIZE),
                theme::palette().text_hint,
            );
        }
        ui.advance_cursor_after_rect(full);
        request
    }

    fn presets_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        presets: &[PresetListItem],
    ) -> Option<OptionsRequest> {
        let mut toggle = false;
        let active = self.fullscreen == Fullscreen::Presets;
        let list = &mut self.presets;
        let (request, _) = widgets::group_box_padded_in(
            ui,
            rect,
            true,
            widgets::BUTTON_HEADER_HEIGHT,
            GROUP_PADDING,
            |ui| {
                widgets::header_text(ui, strings::MANAGE_PRESETS_HEADER, true);
                ui.add_space(10.0);
                toggle = widgets::fullscreen_header_toggle(ui, active);
            },
            |ui| {
                let body = ui.max_rect();
                let button_height = widgets::control_height(ui);
                let buttons_top = body.bottom() - button_height * 2.0 - 5.0;
                let list_rect =
                    Rect::from_min_max(body.min, pos2(body.right(), buttons_top - 10.0));
                let mut request = list
                    .ui(
                        ui,
                        list_rect,
                        presets,
                        strings::PATH_PRESET_SAVE_CURRENT_BUTTON,
                    )
                    .map(OptionsRequest::Preset);

                let mut buttons = ui.new_child(
                    UiBuilder::new()
                        .max_rect(Rect::from_min_max(pos2(body.left(), buttons_top), body.max))
                        .layout(Layout::top_down(Align::Min)),
                );
                let full_width = vec2(buttons.available_width(), 0.0);
                if widgets::button(
                    &mut buttons,
                    strings::OPEN_SAVE_FOLDER_BUTTON,
                    ButtonKind::Secondary,
                    true,
                    full_width,
                )
                .clicked()
                {
                    request = Some(OptionsRequest::OpenSaveFolder);
                }
                buttons.add_space(5.0);
                if widgets::button(
                    &mut buttons,
                    strings::DELETE_CURRENT_SAVE_BUTTON,
                    ButtonKind::Danger,
                    true,
                    full_width,
                )
                .clicked()
                {
                    request = Some(OptionsRequest::DeleteCurrentSave);
                }
                request
            },
        );
        if toggle {
            self.fullscreen = self.fullscreen.toggled(Fullscreen::Presets);
        }
        request
    }

    fn right_column_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        shared: &SharedState,
    ) -> Option<OptionsRequest> {
        let mut request = None;
        let mut ignored_top = rect.top();
        if self.fullscreen != Fullscreen::IgnoredFolders {
            let fill = self.fullscreen == Fullscreen::CopyTree;
            let mut toggle = false;
            let (copy_request, frame) = widgets::group_box_padded_in(
                ui,
                rect.shrink(GROUP_MARGIN),
                fill,
                widgets::BUTTON_HEADER_HEIGHT,
                GROUP_PADDING,
                |ui| {
                    widgets::header_text(ui, strings::COPY_FOLDER_TREE_CONTENT_HEADER, true);
                    ui.add_space(10.0);
                    toggle = widgets::fullscreen_header_toggle(ui, fill);
                },
                |ui| self.copy_tree_ui(ui, shared),
            );
            request = copy_request;
            if toggle {
                self.fullscreen = self.fullscreen.toggled(Fullscreen::CopyTree);
            }
            ignored_top = frame.bottom() + GROUP_MARGIN;
        }
        if self.fullscreen == Fullscreen::CopyTree {
            return request;
        }

        let ignored_rect = Rect::from_min_max(pos2(rect.left(), ignored_top), rect.max);
        let mut toggle = false;
        let active = self.fullscreen == Fullscreen::IgnoredFolders;
        let (ignored_request, _) = widgets::group_box_padded_in(
            ui,
            ignored_rect.shrink(GROUP_MARGIN),
            true,
            widgets::BUTTON_HEADER_HEIGHT,
            GROUP_PADDING,
            |ui| {
                widgets::header_text(ui, strings::IGNORE_FOLDERS_HEADER, true);
                ui.add_space(10.0);
                toggle = widgets::fullscreen_header_toggle(ui, active);
            },
            |ui| {
                let body = ui.max_rect();
                let button_height = widgets::control_height(ui);
                let list_rect = Rect::from_min_max(
                    body.min,
                    pos2(body.right(), body.bottom() - button_height - 10.0),
                );
                ignored_folders_box(ui, list_rect, shared.ignored_folders());
                let mut row = ui.new_child(
                    UiBuilder::new()
                        .max_rect(Rect::from_min_max(
                            pos2(body.left(), body.bottom() - button_height),
                            body.max,
                        ))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                widgets::button(
                    &mut row,
                    strings::EDIT_FOLDERS_BUTTON,
                    ButtonKind::Secondary,
                    true,
                    egui::Vec2::ZERO,
                )
                .clicked()
                .then_some(OptionsRequest::EditIgnoredFolders)
            },
        );
        if toggle {
            self.fullscreen = self.fullscreen.toggled(Fullscreen::IgnoredFolders);
        }
        request.or(ignored_request)
    }

    /// The link groups, the two tab selectors and the copy and link buttons.
    fn copy_tree_ui(&mut self, ui: &mut Ui, shared: &SharedState) -> Option<OptionsRequest> {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let mut request = None;
        for group in shared.link_groups() {
            ui.add_space(2.0);
            if let Some(tab) = link_group_ui(ui, group) {
                request = Some(OptionsRequest::Unlink(tab));
            }
            ui.add_space(2.0);
        }
        ui.add_space(10.0);

        widgets::label(
            ui,
            strings::COPY_FROM_TAB_LABEL,
            theme::body(theme::BODY_SIZE),
            theme::palette().text,
        );
        ui.add_space(2.0);
        tab_selector(ui, "copy_source", &mut self.copy_source, shared);
        ui.add_space(10.0);
        widgets::label(
            ui,
            strings::COPY_TO_TAB_LABEL,
            theme::body(theme::BODY_SIZE),
            theme::palette().text,
        );
        ui.add_space(2.0);
        tab_selector(ui, "copy_destination", &mut self.copy_destination, shared);
        ui.add_space(15.0);

        let pair = self
            .copy_source
            .zip(self.copy_destination)
            .filter(|(source, destination)| source != destination);
        let can_copy = pair.is_some_and(|(source, _)| {
            !shared.source_path.trim().is_empty() && !shared.is_linked(source)
        });
        let can_link = pair.is_some_and(|(source, destination)| {
            !shared.linked_peers(source).contains(&destination)
        });
        ui.horizontal(|ui| {
            if widgets::button(
                ui,
                strings::COPY_FOLDER_TREE_BUTTON,
                ButtonKind::Secondary,
                can_copy,
                egui::Vec2::ZERO,
            )
            .clicked()
                && can_copy
                && let Some((source, destination)) = pair
            {
                request = Some(OptionsRequest::CopyTree(source, destination));
            }
            ui.add_space(5.0);
            if widgets::button(
                ui,
                strings::OPTIONS_LINK_FOLDER_TREES_BUTTON,
                ButtonKind::Secondary,
                can_link,
                egui::Vec2::ZERO,
            )
            .clicked()
                && can_link
                && let Some((source, destination)) = pair
            {
                request = Some(OptionsRequest::Link(source, destination));
            }
        });
        request
    }
}

/// A bordered row with the first tab of a group, the unlink button and the other tabs. Returns
/// the first tab when the unlink button was clicked.
fn link_group_ui(ui: &mut Ui, group: &[TreeTab]) -> Option<TreeTab> {
    let first = *group.first()?;
    let height = 5.0 + 28.0 + 5.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        4,
        egui::Stroke::new(1.0, theme::palette().border),
        egui::StrokeKind::Inside,
    );
    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(8.0, 5.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing = vec2(0.0, 0.0);
    let icon = first.icon();
    widgets::glyph(&mut row, icon.glyph, 22.0, icon.color()).on_hover_text(first.title());
    row.add_space(5.0 + 5.0);
    let (button_rect, button) = row.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let link = if button.hovered() {
        icons::LINK_OFF
    } else {
        icons::LINK
    };
    widgets::paint_glyph(&row, button_rect.center(), link.glyph, 22.0, link.color());
    let clicked = button
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(strings::OPTIONS_UNLINK_TOOLTIP)
        .clicked();
    row.add_space(5.0);
    for tab in &group[1..] {
        let icon = tab.icon();
        widgets::glyph(&mut row, icon.glyph, 22.0, icon.color()).on_hover_text(tab.title());
        row.add_space(5.0);
    }
    clicked.then_some(first)
}

/// A combo box with "None" and the tabs that have a folder tree. Linked tabs show a link icon.
fn tab_selector(ui: &mut Ui, id_salt: &str, selected: &mut Option<TreeTab>, shared: &SharedState) {
    let width = ui.available_width();
    let linked = |tab: TreeTab| shared.is_linked(tab).then_some(icons::LINK);
    let response = ui
        .push_id(id_salt, |ui| match *selected {
            Some(tab) => {
                widgets::combo_value(ui, width, Some(tab.icon()), tab.title(), linked(tab))
            }
            None => widgets::combo_value(ui, width, None, strings::OPTIONS_TAB_NONE, None),
        })
        .inner;
    let mut items = vec![ComboItem {
        icon: None,
        text: strings::OPTIONS_TAB_NONE,
        trailing: None,
    }];
    items.extend(TreeTab::ALL.iter().map(|tab| ComboItem {
        icon: Some(tab.icon()),
        text: tab.title(),
        trailing: linked(*tab),
    }));
    if let Some(index) = widgets::combo_popup(&response, width, &items) {
        *selected = index.checked_sub(1).map(|index| TreeTab::ALL[index]);
    }
}

/// The read-only gray box listing the ignored folder names.
fn ignored_folders_box(ui: &mut Ui, rect: Rect, ignored_folders: &[String]) {
    let text = if ignored_folders.is_empty() {
        strings::IGNORED_FOLDERS_NONE.to_owned()
    } else {
        filecraft_core::settings::format_ignored_folders(ignored_folders)
    };
    widgets::info_box(ui, rect, "ignored_folders", &text);
}
