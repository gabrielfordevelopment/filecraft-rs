//! The searchable, sortable preset list.
//! It is used for the setting presets of the Options tab and for path presets.

use chrono::NaiveDateTime;
use egui::{Align2, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use filecraft_core::file_content::{compare_paths, ignore_case_key};
use filecraft_core::presets::PresetSortCriteria;

use crate::icons;
use crate::strings;
use crate::theme;
use crate::widgets::{self, ButtonKind, ButtonStyle, ComboItem};

const ROW_PADDING: f32 = 5.0;
const ACTION_SIZE: f32 = 24.0;

pub struct PresetListItem {
    pub id: i32,
    pub name: String,
    pub last_modified: NaiveDateTime,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetListAction {
    SaveNew,
    Load(i32),
    Overwrite(i32),
    Details(i32),
    Rename(i32),
    Delete(i32),
    /// The sort criteria or direction changed.
    SortChanged,
}

pub struct PresetList {
    id_salt: &'static str,
    search: String,
    pub sort_by: PresetSortCriteria,
    pub descending: bool,
}

impl PresetList {
    pub fn new(id_salt: &'static str) -> Self {
        Self {
            id_salt,
            search: String::new(),
            sort_by: PresetSortCriteria::DateModified,
            descending: true,
        }
    }

    /// Items that match the search, in list order.
    fn visible<'a>(&self, items: &'a [PresetListItem]) -> Vec<&'a PresetListItem> {
        let search = ignore_case_key(&self.search);
        let mut visible: Vec<&PresetListItem> = items
            .iter()
            .filter(|item| {
                self.search.trim().is_empty() || ignore_case_key(&item.name).contains(&search)
            })
            .collect();
        visible.sort_by(|a, b| {
            let order = match self.sort_by {
                PresetSortCriteria::Name => compare_paths(&a.name, &b.name),
                PresetSortCriteria::DateModified => a.last_modified.cmp(&b.last_modified),
            };
            if self.descending {
                order.reverse()
            } else {
                order
            }
        });
        visible
    }

    /// Draws the list in `rect`. `save_tooltip` describes the save button.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        items: &[PresetListItem],
        save_tooltip: &str,
    ) -> Option<PresetListAction> {
        let mut action = None;
        let control = widgets::control_height(ui);

        let save_style = ButtonStyle {
            bold: true,
            glyph: Some(icons::ADD),
            ..ButtonStyle::new(ButtonKind::Primary, vec2(0.0, control))
        };
        let save_width = widgets::text_width(
            ui,
            strings::COMMON_SAVE_BUTTON,
            theme::bold(theme::BODY_SIZE),
        ) + theme::BODY_SIZE
            + 5.0
            + 22.0;
        let search_rect =
            Rect::from_min_size(rect.min, vec2(rect.width() - save_width - 5.0, control));
        let mut search_ui = ui.new_child(UiBuilder::new().max_rect(search_rect));
        widgets::search_box(
            &mut search_ui,
            &format!("{}_search", self.id_salt),
            &mut self.search,
            search_rect.width(),
            strings::PRESET_LIST_SEARCH_PLACEHOLDER,
        );
        let save_rect = Rect::from_min_size(
            pos2(rect.right() - save_width, rect.top()),
            vec2(save_width, control),
        );
        let mut save_ui = ui.new_child(UiBuilder::new().max_rect(save_rect));
        if widgets::styled_button(&mut save_ui, strings::COMMON_SAVE_BUTTON, save_style, true)
            .on_hover_text(save_tooltip)
            .clicked()
        {
            action = Some(PresetListAction::SaveNew);
        }

        let sort_top = rect.top() + control + 10.0;
        action = action.or(self.sort_row(
            ui,
            Rect::from_min_size(
                pos2(rect.left(), sort_top),
                vec2(rect.width(), 28.0_f32.max(control)),
            ),
        ));

        let count_height = widgets::text_height(ui, theme::body(10.0)) + 2.0;
        let list_rect = Rect::from_min_max(
            pos2(rect.left(), sort_top + 28.0_f32.max(control) + 5.0),
            pos2(rect.right(), rect.bottom() - count_height),
        );
        let visible = self.visible(items);
        action = action.or(self.rows(ui, list_rect, &visible));
        ui.painter().text(
            pos2(rect.right(), rect.bottom()),
            Align2::RIGHT_BOTTOM,
            strings::preset_count(visible.len()),
            theme::body(10.0),
            theme::palette().text_hint,
        );
        action
    }

    /// "Sort by:", the criteria combo box and the direction toggle.
    fn sort_row(&mut self, ui: &mut Ui, rect: Rect) -> Option<PresetListAction> {
        let label_width = widgets::text_width(
            ui,
            strings::PRESET_LIST_SORT_BY_LABEL,
            theme::body(theme::BODY_SIZE),
        );
        ui.painter().text(
            pos2(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            strings::PRESET_LIST_SORT_BY_LABEL,
            theme::body(theme::BODY_SIZE),
            theme::palette().text_hint,
        );
        let toggle_rect = Rect::from_center_size(
            pos2(rect.right() - 2.0 - 14.0, rect.center().y),
            vec2(28.0, 28.0),
        );
        let combo_left = rect.left() + label_width + 5.0;
        let combo_width = toggle_rect.left() - 2.0 - 5.0 - combo_left;
        let control = widgets::control_height(ui);
        let combo_rect = Rect::from_min_size(
            pos2(combo_left, rect.center().y - control / 2.0),
            vec2(combo_width, control),
        );
        let mut changed = false;
        let mut combo_ui = ui.new_child(UiBuilder::new().max_rect(combo_rect));
        let response =
            widgets::combo_value(&mut combo_ui, combo_width, None, self.sort_by.name(), None);
        let items: Vec<ComboItem> = PresetSortCriteria::ALL
            .iter()
            .map(|criteria| ComboItem {
                icon: None,
                text: criteria.name(),
                trailing: None,
            })
            .collect();
        if let Some(index) = widgets::combo_popup(&response, combo_width, &items) {
            let sort_by = PresetSortCriteria::ALL[index];
            changed |= sort_by != self.sort_by;
            self.sort_by = sort_by;
        }

        let mut toggle_ui = ui.new_child(UiBuilder::new().max_rect(toggle_rect));
        let (glyph, tooltip) = if self.descending {
            (
                icons::ARROW_DOWNWARD,
                strings::PRESET_LIST_DESCENDING_TOOLTIP,
            )
        } else {
            (icons::ARROW_UPWARD, strings::PRESET_LIST_ASCENDING_TOOLTIP)
        };
        if widgets::icon_button(
            &mut toggle_ui,
            icons::Icon {
                glyph,
                tone: icons::Tone::Text,
            },
            28.0,
            16.0,
            true,
        )
        .on_hover_text(tooltip)
        .clicked()
        {
            self.descending = !self.descending;
            changed = true;
        }
        changed.then_some(PresetListAction::SortChanged)
    }

    fn rows(
        &self,
        ui: &mut Ui,
        rect: Rect,
        visible: &[&PresetListItem],
    ) -> Option<PresetListAction> {
        ui.painter().rect(
            rect,
            4,
            theme::palette().surface_alt,
            egui::Stroke::new(1.0, theme::palette().border),
            egui::StrokeKind::Inside,
        );
        let mut inner = ui.new_child(UiBuilder::new().max_rect(rect.shrink(1.0)));
        inner.set_clip_rect(rect.shrink(1.0).intersect(ui.clip_rect()));
        let row_height = widgets::text_height(ui, theme::body(14.0))
            + widgets::text_height(ui, theme::body(11.0))
            + ROW_PADDING * 2.0;
        let now = chrono::Local::now().naive_local();
        let mut action = None;
        egui::ScrollArea::vertical()
            .id_salt(self.id_salt)
            .auto_shrink([false, false])
            .show_rows(&mut inner, row_height, visible.len(), |ui, range| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for item in &visible[range] {
                    action = action.or(row(ui, item, row_height, now));
                }
            });
        action
    }
}

/// One preset: name and date on the left, load and the icon actions on the right.
fn row(
    ui: &mut Ui,
    item: &PresetListItem,
    height: f32,
    now: NaiveDateTime,
) -> Option<PresetListAction> {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return None;
    }
    if response.contains_pointer() {
        ui.painter()
            .rect_filled(rect, 4, theme::palette().list_hover);
    }
    let content = rect.shrink(ROW_PADDING);
    let load_style = ButtonStyle {
        font: 11.0,
        horizontal_padding: 8.0,
        height: Some(ACTION_SIZE),
        ..ButtonStyle::new(ButtonKind::Secondary, vec2(0.0, ACTION_SIZE))
    };
    let load_width = widgets::text_width(ui, strings::COMMON_LOAD_BUTTON, theme::semibold(11.0))
        + 8.0 * 2.0
        + 2.0;
    let actions_width = load_width + 5.0 + (ACTION_SIZE + 4.0) * 4.0;
    let mut actions = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(content.right() - actions_width, content.top()),
                content.max,
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    actions.spacing_mut().item_spacing = vec2(0.0, 0.0);
    let mut action = None;
    if widgets::styled_button(&mut actions, strings::COMMON_LOAD_BUTTON, load_style, true).clicked()
    {
        action = Some(PresetListAction::Load(item.id));
    }
    actions.add_space(5.0);
    let buttons = [
        (
            icons::SAVE_GLYPH,
            theme::palette().success,
            strings::PRESET_LIST_OVERWRITE_BUTTON,
            PresetListAction::Overwrite(item.id),
        ),
        (
            icons::VISIBILITY,
            theme::palette().text_hint,
            strings::PRESET_LIST_DETAILS_BUTTON,
            PresetListAction::Details(item.id),
        ),
        (
            icons::EDIT,
            theme::palette().accent,
            strings::RENAME_PRESET_TITLE,
            PresetListAction::Rename(item.id),
        ),
        (
            icons::DELETE,
            theme::palette().danger,
            strings::DELETE_PRESET_TOOLTIP,
            PresetListAction::Delete(item.id),
        ),
    ];
    for (glyph, color, tooltip, button_action) in buttons {
        actions.add_space(2.0);
        if widgets::list_icon_button(&mut actions, glyph, color, ACTION_SIZE)
            .on_hover_text(tooltip)
            .clicked()
        {
            action = Some(button_action);
        }
        actions.add_space(2.0);
    }

    let text_right = content.right() - actions_width - 15.0;
    let name_height = widgets::text_height(ui, theme::body(14.0));
    let text_top =
        content.center().y - (name_height + widgets::text_height(ui, theme::body(11.0))) / 2.0;
    let name_rect = Rect::from_min_max(
        pos2(content.left(), text_top),
        pos2(text_right, text_top + name_height),
    );
    let name = widgets::ellipsized(
        ui,
        &item.name,
        theme::bold(14.0),
        theme::palette().text,
        name_rect.width(),
    );
    let truncated = name.elided;
    ui.painter()
        .galley(name_rect.min, name, theme::palette().text);
    if truncated {
        ui.interact(
            name_rect,
            ui.scope_id().with(("preset_name", item.id)),
            Sense::hover(),
        )
        .on_hover_text(&item.name);
    }

    let detail_top = text_top + name_height;
    let date = strings::preset_date(item.last_modified, now);
    let date_galley =
        ui.painter()
            .layout_no_wrap(date, theme::body(11.0), theme::palette().text_hint);
    let date_width = date_galley.size().x;
    ui.painter().galley(
        pos2(content.left(), detail_top),
        date_galley,
        theme::palette().text_hint,
    );
    let description_left = content.left() + date_width + 10.0;
    if description_left < text_right && !item.description.is_empty() {
        let mut job = egui::text::LayoutJob::single_section(
            item.description.clone(),
            egui::TextFormat {
                font_id: theme::body(11.0),
                color: theme::palette().text_hint,
                italics: true,
                ..Default::default()
            },
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(text_right - description_left);
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(
            pos2(description_left, detail_top),
            galley,
            theme::palette().text_hint,
        );
    }
    action
}
