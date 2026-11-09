//! Non-blocking notifications in the bottom right corner, used for successful operations. Errors
//! keep using modal dialogs, so they are not missed.

use std::path::PathBuf;

use egui::{Align, Align2, Area, Frame, Id, Layout, Order, Sense, Ui, vec2};

use crate::icons::{self, Icon};
use crate::platform;
use crate::strings;
use crate::theme;
use crate::widgets::{self, ButtonKind, ButtonStyle};

/// How long a notification stays visible while the pointer is not over it.
const VISIBLE_SECONDS: f64 = 6.0;
const WIDTH: f32 = 340.0;
/// Space between a notification and the window edges.
const MARGIN: f32 = 28.0;
const SPACING: f32 = 8.0;
const MAX_VISIBLE: usize = 3;

pub struct Toast {
    icon: Icon,
    title: String,
    message: String,
    /// A written file. Its name and folder are shown, with "Open folder" and "Copy path" buttons.
    file: Option<PathBuf>,
    /// Short results shown as badges, such as "12 files", in groups with an optional caption.
    facts: Vec<(Option<String>, Vec<String>)>,
    copied: bool,
    /// Seconds the notification was shown without the pointer over it.
    shown_for: f64,
}

impl Toast {
    pub fn success(title: &str, message: String) -> Self {
        Self {
            icon: icons::SUCCESS,
            title: title.to_owned(),
            message,
            file: None,
            facts: Vec::new(),
            copied: false,
            shown_for: 0.0,
        }
    }

    pub fn with_file(mut self, file: PathBuf) -> Self {
        self.file = Some(file);
        self
    }

    pub fn with_facts(mut self, facts: Vec<String>) -> Self {
        self.facts.push((None, facts));
        self
    }

    /// Adds badges below a small caption, such as the savings of removed comments.
    pub fn with_fact_group(mut self, caption: &str, facts: Vec<String>) -> Self {
        self.facts.push((Some(caption.to_owned()), facts));
        self
    }
}

#[derive(Default)]
pub struct Toasts {
    items: Vec<(Id, Toast)>,
    next_id: u64,
}

impl Toasts {
    pub fn push(&mut self, toast: Toast) {
        self.next_id += 1;
        self.items
            .push((Id::unique(("toast", self.next_id)), toast));
        if self.items.len() > MAX_VISIBLE {
            self.items.remove(0);
        }
    }

    /// Shows the notifications, newest at the bottom. Returns an error message when an action of a
    /// notification failed.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }
        let dt = f64::from(ctx.input(|input| input.stable_dt)).min(0.1);
        let screen = ctx.content_rect();
        let mut bottom = screen.bottom() - MARGIN;
        let mut closed = Vec::new();
        let mut error = None;
        for (index, (id, toast)) in self.items.iter_mut().enumerate().rev() {
            let area = Area::new(*id)
                .order(Order::Foreground)
                .pivot(Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(screen.right() - MARGIN, bottom))
                .show(ctx, |ui| toast_ui(ui, toast));
            let (close, failed) = area.inner;
            error = error.or(failed);
            if area.response.contains_pointer() {
                toast.shown_for = 0.0;
            } else {
                toast.shown_for += dt;
            }
            if close || toast.shown_for >= VISIBLE_SECONDS {
                closed.push(index);
            }
            bottom = area.response.rect.top() - SPACING;
        }
        for index in closed {
            self.items.remove(index);
        }
        // Keep counting down while nothing else repaints.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        error
    }
}

/// Draws one notification. Returns whether it was closed and the error of a failed action.
fn toast_ui(ui: &mut Ui, toast: &mut Toast) -> (bool, Option<String>) {
    let colors = theme::palette();
    let mut close = false;
    let mut error = None;
    Frame::new()
        .fill(colors.surface)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .corner_radius(6)
        .inner_margin(12)
        .shadow(egui::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: egui::Color32::from_black_alpha(40),
        })
        .show(ui, |ui| {
            ui.set_width(WIDTH - 24.0);
            ui.horizontal(|ui| {
                widgets::glyph(ui, toast.icon.glyph, 20.0, toast.icon.color());
                ui.add_space(8.0);
                widgets::label(
                    ui,
                    &toast.title,
                    theme::semibold(theme::BODY_SIZE),
                    colors.text,
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (rect, response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 3, colors.hover);
                    }
                    widgets::paint_glyph(ui, rect.center(), icons::CLOSE, 16.0, colors.text_muted);
                    close = response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(strings::TOAST_CLOSE_TOOLTIP)
                        .clicked();
                });
            });
            let indented = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    ui.vertical(|ui| add(ui));
                });
            };
            if let Some(file) = &toast.file {
                ui.add_space(6.0);
                indented(ui, &mut |ui| {
                    let name = file
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    wrapped(ui, &name, theme::semibold(theme::BODY_SIZE), colors.text);
                    if let Some(folder) = file.parent() {
                        wrapped(
                            ui,
                            &folder.to_string_lossy(),
                            theme::body(11.0),
                            colors.text_muted,
                        );
                    }
                });
            }
            if !toast.message.is_empty() {
                ui.add_space(6.0);
                indented(ui, &mut |ui| {
                    wrapped(
                        ui,
                        &toast.message,
                        theme::body(theme::BODY_SIZE),
                        colors.text_muted,
                    );
                });
            }
            for (caption, facts) in &toast.facts {
                ui.add_space(8.0);
                indented(ui, &mut |ui| {
                    if let Some(caption) = caption {
                        wrapped(ui, caption, theme::body(11.0), colors.text_muted);
                        ui.add_space(4.0);
                    }
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    ui.horizontal_wrapped(|ui| {
                        for fact in facts {
                            widgets::badge(ui, fact);
                        }
                    });
                });
            }
            if let Some(file) = &toast.file {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(28.0);
                    let style = ButtonStyle {
                        height: Some(24.0),
                        horizontal_padding: 8.0,
                        ..ButtonStyle::new(ButtonKind::Secondary, egui::Vec2::ZERO)
                    };
                    if widgets::styled_button(ui, strings::TOAST_OPEN_FOLDER_BUTTON, style, true)
                        .clicked()
                        && let Some(folder) = file.parent()
                        && let Err(failed) = platform::open_folder(folder)
                    {
                        error = Some(strings::open_folder_error(&failed.to_string()));
                    }
                    ui.add_space(6.0);
                    let copy_text = if toast.copied {
                        strings::TOAST_PATH_COPIED
                    } else {
                        strings::TOAST_COPY_PATH_BUTTON
                    };
                    if widgets::styled_button(ui, copy_text, style, true).clicked() {
                        ui.ctx().copy_text(file.to_string_lossy().into_owned());
                        toast.copied = true;
                    }
                });
            }
        });
    (close, error)
}

/// Text that wraps at the available width.
fn wrapped(ui: &mut Ui, text: &str, font: egui::FontId, color: egui::Color32) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(font).color(color)).wrap());
}
