//! Image Tools layout using the same cards, controls and footer as the export tabs.

use egui::{Align, Layout, Rect, Ui, UiBuilder, pos2, vec2};
use filecraft_core::image_tools::StampCorner;

use super::*;
use crate::export_footer::{self, FooterButton, Stat, StatTone};
use crate::widgets::{self, ButtonKind, ComboItem};

const GAP: f32 = 10.0;
const SETTINGS_HEIGHT: f32 = 150.0;

#[cfg(test)]
mod tests;
const CORNERS: [StampCorner; 4] = [
    StampCorner::TopLeft,
    StampCorner::TopRight,
    StampCorner::BottomLeft,
    StampCorner::BottomRight,
];

impl ImageToolsTab {
    pub fn ui(&mut self, ui: &mut Ui) -> Option<TabRequest> {
        let busy = self.running.is_some() || self.preview.is_some();
        let full = ui.available_rect_before_wrap();
        let previous_options = self.options.clone();
        let previous_source = self.source.clone();
        let folder = Rect::from_min_size(
            full.min + vec2(5.0, 5.0),
            vec2(full.width() - 10.0, widgets::floating_box_height(ui)),
        );
        self.source_ui(ui, folder, !busy);

        let settings_top = folder.bottom() + GAP;
        let column_width = (folder.width() - GAP) / 2.0;
        let left = Rect::from_min_max(
            pos2(folder.left(), settings_top),
            pos2(folder.left() + column_width, settings_top + SETTINGS_HEIGHT),
        );
        let right = Rect::from_min_max(
            pos2(left.right() + GAP, settings_top),
            pos2(folder.right(), settings_top + SETTINGS_HEIGHT),
        );
        let date_frame = self.date_settings(ui, left, !busy);
        let resize_frame = self.resize_settings(ui, right, !busy);
        self.options.max_width = self.width_enabled.then_some(self.width);
        self.options.max_height = self.height_enabled.then_some(self.height);
        if self.options != previous_options || self.source != previous_source {
            self.rows.clear();
            self.output = None;
            self.results = false;
        }

        let (footer, mut request) = self.footer(ui, full, busy);
        let results = Rect::from_min_max(
            pos2(
                folder.left(),
                date_frame.bottom().max(resize_frame.bottom()) + GAP,
            ),
            pos2(folder.right(), footer.top() - GAP),
        );
        let mut preview_clicked = false;
        let mut open_clicked = false;
        widgets::card_in(
            ui,
            results,
            true,
            |ui| {
                widgets::card_title(
                    ui,
                    if self.results {
                        strings::IMAGE_RESULTS
                    } else {
                        strings::IMAGE_PREVIEW_TITLE
                    },
                );
                if busy {
                    ui.add_space(GAP);
                    widgets::busy_label(
                        ui,
                        if self.running.is_some() {
                            strings::IMAGE_PROCESSING
                        } else {
                            strings::IMAGE_READING
                        },
                    );
                }
            },
            |ui| {
                if self.output.is_some() {
                    open_clicked = widgets::button(
                        ui,
                        strings::IMAGE_OPEN_OUTPUT,
                        ButtonKind::Secondary,
                        !busy,
                        egui::Vec2::ZERO,
                    )
                    .clicked();
                    ui.add_space(GAP);
                }
                preview_clicked = widgets::button(
                    ui,
                    strings::IMAGE_PREVIEW,
                    ButtonKind::Secondary,
                    !busy && self.source.is_some(),
                    egui::Vec2::ZERO,
                )
                .clicked();
            },
            |ui| self.results_ui(ui),
        );
        if preview_clicked {
            request = self.start(ui.ctx(), true).or(request);
        }
        if open_clicked
            && let Some(output) = &self.output
            && let Err(error) = crate::platform::open_folder(output)
        {
            request = Some(error_request(error.to_string()));
        }
        ui.advance_cursor_after_rect(full);
        request
    }

    fn source_ui(&mut self, ui: &mut Ui, rect: Rect, enabled: bool) {
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Max)),
        );
        let browse = widgets::button(
            &mut row,
            strings::BROWSE_BUTTON,
            ButtonKind::Secondary,
            enabled,
            egui::Vec2::ZERO,
        )
        .clicked();
        row.add_space(GAP);
        let text = self
            .source
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let width = row.available_width();
        widgets::floating_path_box(&mut row, &text, width, strings::IMAGE_SOURCE)
            .on_hover_text(strings::IMAGE_FORMATS);
        if browse
            && enabled
            && let Some(path) = rfd::FileDialog::new()
                .set_title(strings::IMAGE_SELECT_SOURCE)
                .pick_folder()
        {
            self.source = Some(path);
        }
    }

    fn date_settings(&mut self, ui: &mut Ui, rect: Rect, enabled: bool) -> Rect {
        let mut selected = self.options.add_date;
        let (_, frame) = widgets::card_in(
            ui,
            rect,
            true,
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    widgets::labeled_check_box(ui, &mut selected, strings::IMAGE_STAMP);
                });
            },
            |_| {},
            |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.add_enabled_ui(enabled && self.options.add_date, |ui| {
                    egui::Grid::new("image_stamp_settings")
                        .spacing(vec2(10.0, 5.0))
                        .show(ui, |ui| {
                            ui.label(strings::IMAGE_POSITION);
                            let current = CORNERS
                                .iter()
                                .position(|&corner| corner == self.options.corner)
                                .unwrap_or(2);
                            let response = widgets::combo_value(
                                ui,
                                130.0,
                                None,
                                strings::IMAGE_CORNERS[current],
                                None,
                            );
                            let items = strings::IMAGE_CORNERS.map(|text| ComboItem {
                                icon: None,
                                text,
                                trailing: None,
                            });
                            if let Some(index) = widgets::combo_popup(&response, 130.0, &items) {
                                self.options.corner = CORNERS[index];
                            }
                            ui.end_row();
                            ui.label(strings::IMAGE_TEXT_HEIGHT)
                                .on_hover_text(strings::IMAGE_STAMP_STYLE);
                            ui.add_sized(
                                vec2(70.0, widgets::control_height(ui)),
                                egui::DragValue::new(&mut self.options.text_height_percent)
                                    .range(0.1..=10.0)
                                    .speed(0.1),
                            );
                            ui.end_row();
                            ui.label(strings::IMAGE_MARGIN)
                                .on_hover_text(strings::IMAGE_STAMP_STYLE);
                            ui.add_sized(
                                vec2(70.0, widgets::control_height(ui)),
                                egui::DragValue::new(&mut self.options.margin_percent)
                                    .range(0.0..=25.0)
                                    .speed(0.1),
                            );
                            ui.end_row();
                        });
                });
            },
        );
        self.options.add_date = selected;
        frame
    }

    fn resize_settings(&mut self, ui: &mut Ui, rect: Rect, enabled: bool) -> Rect {
        let mut selected = self.options.resize;
        let (_, frame) = widgets::card_in(
            ui,
            rect,
            true,
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    widgets::labeled_check_box(ui, &mut selected, strings::IMAGE_RESIZE);
                });
            },
            |_| {},
            |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.add_enabled_ui(enabled && self.options.resize, |ui| {
                    egui::Grid::new("image_resize_settings")
                        .spacing(vec2(10.0, 5.0))
                        .show(ui, |ui| {
                            widgets::labeled_check_box(
                                ui,
                                &mut self.width_enabled,
                                strings::IMAGE_MAX_WIDTH,
                            );
                            ui.add_enabled_ui(self.width_enabled, |ui| {
                                ui.add_sized(
                                    vec2(70.0, widgets::control_height(ui)),
                                    egui::DragValue::new(&mut self.width).range(1..=65535),
                                );
                            });
                            ui.end_row();
                            widgets::labeled_check_box(
                                ui,
                                &mut self.height_enabled,
                                strings::IMAGE_MAX_HEIGHT,
                            );
                            ui.add_enabled_ui(self.height_enabled, |ui| {
                                ui.add_sized(
                                    vec2(70.0, widgets::control_height(ui)),
                                    egui::DragValue::new(&mut self.height).range(1..=65535),
                                );
                            });
                            ui.end_row();
                        });
                });
                ui.label(
                    egui::RichText::new(strings::IMAGE_RESIZE_HINT)
                        .small()
                        .color(theme::palette().text_muted),
                );
            },
        );
        self.options.resize = selected;
        frame
    }

    fn footer(&mut self, ui: &mut Ui, full: Rect, busy: bool) -> (Rect, Option<TabRequest>) {
        let ready = self.rows.iter().filter(|row| row.outcome.is_ok()).count();
        let errors = self.rows.len() - ready;
        let stats = [
            Stat::count(
                self.rows.len(),
                strings::IMAGE_FILES_LABEL,
                StatTone::Neutral,
            ),
            Stat::count(
                ready,
                if self.results {
                    strings::IMAGE_SAVED_LABEL
                } else {
                    strings::IMAGE_READY_LABEL
                },
                StatTone::needed(ready),
            ),
            Stat::count(
                errors,
                strings::IMAGE_ERRORS_LABEL,
                if errors == 0 {
                    StatTone::Neutral
                } else {
                    StatTone::Warning
                },
            ),
        ];
        let button_width = [strings::IMAGE_PROCESS, strings::IMAGE_PROCESSING]
            .iter()
            .map(|text| widgets::button_width(ui, text))
            .fold(0.0, f32::max)
            + 20.0;
        let stop_width = self.running.as_ref().map_or(0.0, |_| {
            widgets::button_width(ui, strings::IMAGE_CANCEL) + GAP
        });
        let stacked =
            export_footer::stats_width(ui, &stats) + GAP * 2.0 + button_width + stop_width
                > full.width() - 20.0;
        let button_height = export_footer::button_height(ui);
        let height = button_height + 20.0 + if stacked { 30.0 } else { 0.0 };
        let area = Rect::from_min_max(pos2(full.left(), full.bottom() - height), full.max);
        export_footer::separator(ui, area);
        let inner = area.shrink(10.0);
        let controls = Rect::from_min_max(
            pos2(inner.left(), inner.bottom() - button_height),
            inner.max,
        );
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(controls)
                .layout(Layout::right_to_left(Align::Center)),
        );
        let blocker = if busy {
            Some(strings::IMAGE_BUSY_REASON)
        } else if self.source.is_none() {
            Some(strings::IMAGE_NO_SOURCE)
        } else if !self.options.add_date && !self.options.resize {
            Some(strings::IMAGE_NO_ACTION)
        } else if self.options.resize && !self.width_enabled && !self.height_enabled {
            Some(strings::IMAGE_NO_SIZE_LIMIT)
        } else {
            None
        };
        let button = FooterButton {
            text: strings::IMAGE_PROCESS,
            disabled_reason: blocker,
            busy_text: self.running.as_ref().map(|_| strings::IMAGE_PROCESSING),
        };
        let mut request = None;
        if export_footer::export_button(&mut row, &button, button_width, button_height) {
            request = self.start(ui.ctx(), false);
        }
        if let Some(run) = &self.running {
            row.add_space(GAP);
            let stopping = run.cancel.load(Ordering::Relaxed);
            if widgets::button(
                &mut row,
                if stopping {
                    strings::IMAGE_CANCELLING
                } else {
                    strings::IMAGE_CANCEL
                },
                ButtonKind::Secondary,
                !stopping,
                egui::Vec2::ZERO,
            )
            .clicked()
            {
                run.cancel.store(true, Ordering::Relaxed);
            }
        }
        let center_y = if stacked {
            inner.top() + 11.0
        } else {
            controls.center().y
        };
        export_footer::paint_stats(ui, inner.left(), center_y, &stats);
        (area, request)
    }

    fn results_ui(&self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing = vec2(10.0, 6.0);
        if let Some(output) = &self.output {
            let text = output.display().to_string();
            let galley = widgets::ellipsized(
                ui,
                &text,
                theme::body(theme::BODY_SIZE),
                theme::palette().text_muted,
                ui.available_width(),
            );
            ui.add(egui::Label::new(galley)).on_hover_text(text);
        }
        if self.rows.is_empty() {
            ui.label(strings::IMAGE_EMPTY);
            return;
        }
        egui::ScrollArea::both()
            .id_salt("image_results")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let column_width = (ui.available_width() - ui.spacing().item_spacing.x * 3.0) / 4.0;
                egui::Grid::new("image_results_grid")
                    .num_columns(4)
                    .min_col_width(column_width)
                    .max_col_width(column_width)
                    .striped(true)
                    .show(ui, |ui| {
                        for heading in [
                            strings::IMAGE_FILE_COLUMN,
                            strings::IMAGE_SIZE_COLUMN,
                            strings::IMAGE_DATE_COLUMN,
                            strings::IMAGE_STATUS_COLUMN,
                        ] {
                            ui.strong(heading);
                        }
                        ui.end_row();
                        for row in &self.rows {
                            ui.label(&row.name);
                            match &row.outcome {
                                Ok(item) => {
                                    ui.label(strings::image_size_pair(
                                        item.original_size,
                                        item.output_size,
                                    ));
                                    ui.label(
                                        item.date
                                            .map(|d| d.format("%Y.%m.%d").to_string())
                                            .unwrap_or_else(|| strings::IMAGE_NO_VALUE.to_owned()),
                                    );
                                    let status = match (self.results, item.copied) {
                                        (true, true) => strings::IMAGE_UNCHANGED,
                                        (true, false) => strings::IMAGE_WRITTEN,
                                        (false, true) => strings::IMAGE_COPY,
                                        (false, false) => strings::IMAGE_EDIT,
                                    };
                                    ui.vertical(|ui| {
                                        ui.label(status);
                                        for note in &item.notes {
                                            ui.colored_label(theme::palette().warning, note);
                                        }
                                    });
                                }
                                Err(error) => {
                                    ui.label(strings::IMAGE_NO_VALUE);
                                    ui.label(strings::IMAGE_NO_VALUE);
                                    ui.colored_label(theme::palette().danger, error);
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
    }
}
