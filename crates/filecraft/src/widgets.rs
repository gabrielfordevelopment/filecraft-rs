//! Custom controls shared by the tabs and dialogs.

use egui::{
    Align, Align2, Color32, CornerRadius, Id, Layout, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use filecraft_core::folder_tree::CheckState;

use crate::icons::Icon;
use crate::theme;

/// Duration of the hover color animations.
const HOVER_ANIMATION_SECONDS: f32 = 0.1;

/// Blends two colors. `Color32` stores premultiplied channels, so blending them directly keeps a
/// fade from a transparent color free of dark in-between frames.
pub fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
        channel(from.a(), to.a()),
    )
}

/// The hover animation of a control, from 0 to 1. `active` is false for disabled or selected
/// controls. Call it every frame the control is laid out, so the animation never resumes from a
/// value it had before the control was disabled or selected.
pub fn hover_amount(ui: &Ui, response: &Response, active: bool) -> f32 {
    ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        active && response.hovered(),
        HOVER_ANIMATION_SECONDS,
    )
}

/// The role of a button. Each view has at most one primary button, for its main action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// The main action, filled with the accent color.
    Primary,
    /// Every other action: neutral with a border.
    Secondary,
    /// Actions that delete or discard data: red text and border.
    Danger,
}

struct ButtonColors {
    fill: Color32,
    hover_fill: Color32,
    border: Color32,
    text: Color32,
}

impl ButtonKind {
    fn colors(self) -> ButtonColors {
        let colors = theme::palette();
        match self {
            Self::Primary => ButtonColors {
                fill: colors.accent,
                hover_fill: colors.accent_hover,
                border: colors.accent,
                text: colors.on_accent,
            },
            Self::Secondary => ButtonColors {
                fill: colors.surface,
                hover_fill: colors.hover,
                border: colors.border,
                text: colors.text,
            },
            Self::Danger => ButtonColors {
                fill: colors.surface,
                hover_fill: colors.danger_soft,
                border: colors.danger,
                text: colors.danger,
            },
        }
    }

    /// The look of every disabled button, whatever its kind.
    fn disabled_colors() -> ButtonColors {
        let colors = theme::palette();
        ButtonColors {
            fill: colors.surface_alt,
            hover_fill: colors.surface_alt,
            border: colors.border_subtle,
            text: colors.text_hint,
        }
    }
}

/// Width of a [`button`] with this text and no minimum size.
pub fn button_width(ui: &Ui, text: &str) -> f32 {
    text_width(ui, text, theme::semibold(theme::BODY_SIZE)) + 10.0 * 2.0 + 2.0
}

/// The standard button: rounded, semibold text, darker fill on hover.
pub fn button(
    ui: &mut Ui,
    text: &str,
    kind: ButtonKind,
    enabled: bool,
    min_size: Vec2,
) -> Response {
    styled_button(ui, text, ButtonStyle::new(kind, min_size), enabled)
}

/// Variations of the standard button used by lists and windows.
#[derive(Debug, Clone, Copy)]
pub struct ButtonStyle {
    pub kind: ButtonKind,
    pub min_size: Vec2,
    pub font: f32,
    pub bold: bool,
    pub horizontal_padding: f32,
    /// A Material Icons glyph before the text, 5 px apart.
    pub glyph: Option<char>,
    /// A fixed height instead of the text box height.
    pub height: Option<f32>,
}

impl ButtonStyle {
    pub fn new(kind: ButtonKind, min_size: Vec2) -> Self {
        Self {
            kind,
            min_size,
            font: theme::BODY_SIZE,
            bold: false,
            horizontal_padding: 10.0,
            glyph: None,
            height: None,
        }
    }
}

pub fn styled_button(ui: &mut Ui, text: &str, style: ButtonStyle, enabled: bool) -> Response {
    let kind = style.kind;
    let font = if style.bold {
        theme::bold(style.font)
    } else {
        theme::semibold(style.font)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let glyph_width = style.glyph.map_or(0.0, |_| style.font + 5.0);
    let padding = vec2(style.horizontal_padding, 5.0);
    let content_width = glyph_width + galley.size().x;
    let height = style.height.unwrap_or_else(|| control_height(ui));
    let size = vec2(content_width + padding.x * 2.0 + 2.0, height).max(style.min_size);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hover = hover_amount(ui, &response, enabled);
    if ui.is_rect_visible(rect) {
        let colors = if enabled {
            kind.colors()
        } else {
            ButtonKind::disabled_colors()
        };
        let fill = lerp_color(colors.fill, colors.hover_fill, hover);
        let border = colors.border;
        let text_color = colors.text;
        ui.painter()
            .rect(rect, 4, fill, Stroke::new(1.0, border), StrokeKind::Inside);
        let left = rect.center().x - content_width / 2.0;
        if let Some(glyph) = style.glyph {
            paint_glyph(
                ui,
                pos2(left + style.font / 2.0, rect.center().y),
                glyph,
                style.font,
                text_color,
            );
        }
        let text_pos = pos2(left + glyph_width, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, text_color);
    }
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A borderless icon button: a transparent square with a hover background.
pub fn icon_button(ui: &mut Ui, icon: Icon, side: f32, glyph_size: f32, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), sense);
    let hover = hover_amount(ui, &response, enabled);
    if ui.is_rect_visible(rect) {
        let fill = lerp_color(Color32::TRANSPARENT, theme::palette().hover, hover);
        ui.painter().rect_filled(rect, 3, fill);
        let color = if enabled {
            icon.color()
        } else {
            theme::faded(icon.color(), 0.5)
        };
        paint_glyph(ui, rect.center(), icon.glyph, glyph_size, color);
    }
    response
}

/// The action icon button of preset rows: a glyph in a square that turns gray on hover.
pub fn list_icon_button(ui: &mut Ui, glyph: char, color: Color32, side: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    let hover = hover_amount(ui, &response, true);
    if ui.is_rect_visible(rect) {
        let fill = lerp_color(
            Color32::TRANSPARENT,
            theme::palette().list_action_hover,
            hover,
        );
        ui.painter().rect_filled(rect, 3, fill);
        paint_glyph(ui, rect.center(), glyph, side - 8.0, color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The fullscreen toggle of group box headers. Active toggles use the warning colors.
pub fn fullscreen_toggle(ui: &mut Ui, active: bool, normal: Icon, exit: Icon) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let colors = theme::palette();
        let fill = if response.is_pointer_button_down_on() {
            colors.pressed
        } else if response.hovered() {
            colors.hover
        } else if active {
            colors.accent_soft
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 3, fill);
        let (icon, color) = if active {
            (exit, colors.accent)
        } else {
            (normal, normal.color())
        };
        paint_glyph(ui, rect.center(), icon.glyph, 24.0, color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The fullscreen toggle with its tooltip. Returns true when it was clicked.
pub fn fullscreen_header_toggle(ui: &mut Ui, active: bool) -> bool {
    let tooltip = if active {
        crate::strings::EXIT_FULLSCREEN_TOOLTIP
    } else {
        crate::strings::FULLSCREEN_TOOLTIP
    };
    fullscreen_toggle(
        ui,
        active,
        crate::icons::FULLSCREEN,
        crate::icons::FULLSCREEN_EXIT,
    )
    .on_hover_text(tooltip)
    .clicked()
}

pub fn paint_glyph(ui: &Ui, center: Pos2, glyph: char, size: f32, color: Color32) {
    ui.painter().text(
        center,
        Align2::CENTER_CENTER,
        glyph,
        theme::icons(size),
        color,
    );
}

/// A colored glyph placed in the layout.
pub fn glyph(ui: &mut Ui, glyph: char, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_glyph(ui, rect.center(), glyph, size, color);
    response
}

/// A small spinner and muted text, shown while background work runs.
pub fn busy_label(ui: &mut Ui, text: &str) -> Response {
    let colors = theme::palette();
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    egui::Spinner::new()
        .size(12.0)
        .color(colors.text_muted)
        .paint_at(ui, rect);
    ui.add_space(6.0);
    label(ui, text, theme::body(theme::BODY_SIZE), colors.text_muted)
}

/// Plain text placed in the layout without wrapping.
pub fn label(ui: &mut Ui, text: &str, font: egui::FontId, color: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
    response
}

/// Height of single-line text boxes and buttons with the default font.
pub fn control_height(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body).ceil() + 12.0
}

/// Room above a box with a floating label, for the label on its top border.
const FLOATING_LABEL_SPACE: f32 = 6.0;
/// Font size of a floating label while it sits on the border.
const FLOATING_LABEL_SIZE: f32 = 10.0;
/// Space between the border of a box and its text.
const TEXT_BOX_PADDING: f32 = 8.0;
const SEARCH_ICON_SIZE: f32 = 16.0;

/// Height of a box with a floating label, including the room for the label above it.
pub fn floating_box_height(ui: &Ui) -> f32 {
    control_height(ui) + FLOATING_LABEL_SPACE
}

/// The box of a widget with a floating label, below the room for the label.
fn floating_box_rect(outer: Rect) -> Rect {
    Rect::from_min_max(
        pos2(outer.left(), outer.top() + FLOATING_LABEL_SPACE),
        outer.max,
    )
}

/// Paints the label of a box. While `raised` is 0 it sits inside the box like placeholder text,
/// at 1 it sits small on the top border, which is interrupted behind it. The box fill is `fill`.
fn paint_floating_label(
    ui: &Ui,
    rect: Rect,
    label: &str,
    raised: f32,
    fill: Color32,
    color: Color32,
) {
    let colors = theme::palette();
    let size = theme::BODY_SIZE + (FLOATING_LABEL_SIZE - theme::BODY_SIZE) * raised;
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme::body(size), Color32::PLACEHOLDER);
    let left = rect.left() + TEXT_BOX_PADDING;
    let center_y = rect.center().y + (rect.top() + 0.5 - rect.center().y) * raised;
    if raised > 0.0 {
        // Covers the 1 px border line behind the label with the box fill.
        let gap = Rect::from_min_max(
            pos2(left - 3.0, rect.top()),
            pos2(left + galley.size().x + 3.0, rect.top() + 1.0),
        );
        ui.painter().rect_filled(gap, 0, fill);
    }
    let color = lerp_color(colors.text_muted, color, raised);
    ui.painter()
        .galley(pos2(left, center_y - galley.size().y / 2.0), galley, color);
}

/// What a text box shows besides its text.
#[derive(Default)]
struct TextBoxExtras<'a> {
    /// Gray text while the box is empty.
    hint: &'a str,
    /// Gray text at the right end, such as the extension of a file name.
    suffix: &'a str,
    /// A Material Icons glyph at the left end.
    icon: Option<char>,
}

/// An editable text box with a blue border on hover and focus.
pub fn text_box(ui: &mut Ui, id_salt: &str, text: &mut String, width: f32) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, control_height(ui)), Sense::hover());
    text_box_in(ui, rect, id_salt, text, TextBoxExtras::default())
}

/// A search box: a magnifier at its left end and gray placeholder text while it is empty.
pub fn search_box(
    ui: &mut Ui,
    id_salt: &str,
    text: &mut String,
    width: f32,
    placeholder: &str,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, control_height(ui)), Sense::hover());
    let extras = TextBoxExtras {
        hint: placeholder,
        icon: Some(crate::icons::SEARCH),
        ..TextBoxExtras::default()
    };
    text_box_in(ui, rect, id_salt, text, extras)
}

/// A [`text_box`] whose label floats on its top border, with an optional `suffix` at its right
/// end. The label moves into the box while it is empty and not focused.
pub fn floating_text_box(
    ui: &mut Ui,
    id_salt: &str,
    text: &mut String,
    width: f32,
    label: &str,
    suffix: &str,
) -> Response {
    let (outer, _) = ui.allocate_exact_size(vec2(width, floating_box_height(ui)), Sense::hover());
    let rect = floating_box_rect(outer);
    let extras = TextBoxExtras {
        suffix,
        ..TextBoxExtras::default()
    };
    let response = text_box_in(ui, rect, id_salt, text, extras);
    let raised = ui.ctx().animate_bool_with_time(
        response.id.with("floating"),
        response.has_focus() || !text.is_empty(),
        HOVER_ANIMATION_SECONDS,
    );
    let color = if response.has_focus() {
        theme::palette().accent
    } else {
        theme::palette().text_muted
    };
    paint_floating_label(ui, rect, label, raised, theme::palette().surface, color);
    response
}

/// Lays out an editable box in `rect` and paints its border.
fn text_box_in(
    ui: &mut Ui,
    rect: Rect,
    id_salt: &str,
    text: &mut String,
    extras: TextBoxExtras<'_>,
) -> Response {
    let colors = theme::palette();
    let background = ui.painter().add(Shape::Noop);
    let suffix = (!extras.suffix.is_empty()).then(|| {
        ui.painter().layout_no_wrap(
            extras.suffix.to_owned(),
            theme::body(theme::BODY_SIZE),
            colors.text_hint,
        )
    });
    let icon_space = extras.icon.map_or(0.0, |_| SEARCH_ICON_SIZE + 4.0);
    // The text stops before the suffix.
    let suffix_space = suffix.as_ref().map_or(0.0, |suffix| suffix.size().x + 4.0);
    let edit_rect = Rect::from_min_max(
        pos2(rect.left() + icon_space, rect.top()),
        pos2(rect.right() - suffix_space, rect.bottom()),
    );
    let padding = TEXT_BOX_PADDING as i8;
    let output = egui::TextEdit::singleline(text)
        .id_salt(id_salt)
        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(padding, 0)))
        .align(Align2::LEFT_CENTER)
        .min_size(edit_rect.size())
        .desired_width(edit_rect.width() - TEXT_BOX_PADDING * 2.0)
        .text_color(colors.text)
        .hint_text(egui::RichText::new(extras.hint).color(colors.text_muted))
        .show(&mut ui.new_child(UiBuilder::new().max_rect(edit_rect)));
    let response = output.response.response;
    if let Some(icon) = extras.icon {
        paint_glyph(
            ui,
            pos2(
                rect.left() + TEXT_BOX_PADDING + SEARCH_ICON_SIZE / 2.0 - 2.0,
                rect.center().y,
            ),
            icon,
            SEARCH_ICON_SIZE,
            colors.text_hint,
        );
    }
    if let Some(suffix) = suffix {
        ui.painter().galley(
            pos2(
                rect.right() - TEXT_BOX_PADDING - suffix.size().x,
                rect.center().y - suffix.size().y / 2.0,
            ),
            suffix,
            colors.text_hint,
        );
    }
    let highlighted = response.hovered() || response.has_focus();
    let border = if highlighted {
        colors.accent
    } else {
        colors.border
    };
    ui.painter().set(
        background,
        Shape::Rect(egui::epaint::RectShape::new(
            rect,
            4,
            colors.surface,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        )),
    );
    response
}

/// A read-only path box with a floating label. The label sits inside the
/// box while there is no path. The full path is shown as tooltip.
pub fn floating_path_box(ui: &mut Ui, text: &str, width: f32, label: &str) -> Response {
    let (outer, response) =
        ui.allocate_exact_size(vec2(width, floating_box_height(ui)), Sense::hover());
    let rect = floating_box_rect(outer);
    if ui.is_rect_visible(outer) {
        let colors = theme::palette();
        ui.painter().rect(
            rect,
            4,
            colors.surface_alt,
            Stroke::new(1.0, colors.border),
            StrokeKind::Inside,
        );
        let inner = rect.shrink2(vec2(TEXT_BOX_PADDING + 1.0, 0.0));
        ui.painter()
            .with_clip_rect(inner.intersect(ui.clip_rect()))
            .text(
                pos2(inner.left(), inner.center().y),
                Align2::LEFT_CENTER,
                text,
                theme::body(theme::BODY_SIZE),
                colors.text,
            );
        let raised = if text.is_empty() { 0.0 } else { 1.0 };
        paint_floating_label(
            ui,
            rect,
            label,
            raised,
            colors.surface_alt,
            colors.text_muted,
        );
    }
    if text.is_empty() {
        response
    } else {
        response.on_hover_text(text)
    }
}

/// A 15 px three-state check box.
pub fn check_box(ui: &mut Ui, state: CheckState) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(15.0), Sense::click());
    paint_check_box(ui, rect, state, response.hovered());
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Paints a check box in `rect`, which is 15 px square.
pub fn paint_check_box(ui: &Ui, rect: Rect, state: CheckState, hovered: bool) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let (fill, border) = match state {
        CheckState::Checked => (theme::palette().accent, theme::palette().accent_hover),
        CheckState::Indeterminate => (theme::palette().neutral, theme::palette().neutral_strong),
        CheckState::Unchecked => (Color32::TRANSPARENT, theme::palette().border),
    };
    let border = if hovered {
        theme::palette().accent
    } else {
        border
    };
    ui.painter()
        .rect(rect, 3, fill, Stroke::new(1.0, border), StrokeKind::Inside);
    match state {
        CheckState::Checked => {
            let origin = rect.center() - vec2(5.0, 4.5);
            let points = [pos2(0.0, 5.0), pos2(4.0, 9.0), pos2(10.0, 0.0)]
                .map(|point| origin + point.to_vec2());
            ui.painter().add(Shape::line(
                points.to_vec(),
                Stroke::new(2.0, theme::palette().on_accent),
            ));
        }
        CheckState::Indeterminate => {
            let bar = Rect::from_center_size(rect.center(), vec2(8.0, 2.0));
            ui.painter().rect_filled(bar, 0, theme::palette().on_accent);
        }
        CheckState::Unchecked => {}
    }
}

/// A two-state check box with a label. Returns the response of the whole control.
pub fn labeled_check_box(ui: &mut Ui, value: &mut bool, text: &str) -> Response {
    let state = if *value {
        CheckState::Checked
    } else {
        CheckState::Unchecked
    };
    let response = ui
        .horizontal(|ui| {
            let box_response = check_box(ui, state);
            ui.add_space(5.0);
            let label_response = label(
                ui,
                text,
                theme::body(theme::BODY_SIZE),
                theme::palette().text,
            )
            .interact(Sense::click());
            box_response.union(label_response)
        })
        .inner;
    if response.clicked() {
        *value = !*value;
    }
    response
}

/// The chevron expander of the shared tree view item template. It rotates when expanded.
pub fn expander(ui: &mut Ui, id: Id, expanded: bool, visible: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(19.0, 16.0), Sense::click());
    if visible && ui.is_rect_visible(rect) {
        let angle =
            ui.ctx()
                .animate_bool_with_time(id.with("expander"), expanded, HOVER_ANIMATION_SECONDS)
                * std::f32::consts::FRAC_PI_2;
        let color = if response.hovered() {
            theme::palette().accent
        } else {
            theme::palette().text
        };
        let center = pos2(rect.left() + 8.0, rect.center().y);
        let rotation = egui::emath::Rot2::from_angle(angle);
        let points = [vec2(-2.0, -4.0), vec2(2.0, 0.0), vec2(-2.0, 4.0)]
            .map(|offset| center + rotation * offset);
        ui.painter()
            .add(Shape::line(points.to_vec(), Stroke::new(1.5, color)));
    }
    response
}

/// Space between the border of a card and its content.
const CARD_PADDING: f32 = 10.0;
/// Height of the title row of a card. It fits a 28 px button.
const CARD_HEADER_HEIGHT: f32 = 28.0;
const CARD_RADIUS: u8 = 8;
const CARD_TITLE_SIZE: f32 = 13.0;

/// Draws a section card inside `outer` without moving the cursor: a rounded panel with a title
/// row on top. `header` lays out the title from the left, `actions` the buttons from the right.
/// Without `fill_height` the card ends below its content. Returns the body result and the card
/// rectangle.
pub fn card_in<R>(
    ui: &mut Ui,
    outer: Rect,
    fill_height: bool,
    header: impl FnOnce(&mut Ui),
    actions: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui) -> R,
) -> (R, Rect) {
    let background = ui.painter().add(Shape::Noop);
    let header_rect = Rect::from_min_size(
        pos2(
            outer.left() + CARD_PADDING,
            outer.top() + CARD_PADDING / 2.0,
        ),
        vec2(
            (outer.width() - CARD_PADDING * 2.0).max(0.0),
            CARD_HEADER_HEIGHT,
        ),
    );
    header(
        &mut ui.new_child(
            UiBuilder::new()
                .max_rect(header_rect)
                .layout(Layout::left_to_right(Align::Center)),
        ),
    );
    actions(
        &mut ui.new_child(
            UiBuilder::new()
                .max_rect(header_rect)
                .layout(Layout::right_to_left(Align::Center)),
        ),
    );
    let body_max = Rect::from_min_max(
        pos2(outer.left() + CARD_PADDING, header_rect.bottom() + 4.0),
        pos2(outer.right() - CARD_PADDING, outer.bottom() - CARD_PADDING),
    );
    let mut body_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(body_max)
            .layout(Layout::top_down(Align::Min)),
    );
    let result = body(&mut body_ui);
    let bottom = if fill_height {
        outer.bottom()
    } else {
        body_ui.min_rect().bottom() + CARD_PADDING
    };
    let frame = Rect::from_min_max(outer.min, pos2(outer.right(), bottom));
    ui.painter().set(
        background,
        Shape::Rect(egui::epaint::RectShape::new(
            frame,
            CARD_RADIUS,
            theme::palette().surface,
            Stroke::new(1.0, theme::palette().border_subtle),
            StrokeKind::Inside,
        )),
    );
    (result, frame)
}

/// The title of a [`card_in`].
pub fn card_title(ui: &mut Ui, text: &str) -> Response {
    label(
        ui,
        text,
        theme::semibold(CARD_TITLE_SIZE),
        theme::palette().text,
    )
}

/// A small gray pill with a number, such as the selected count next to a title.
pub fn count_badge(ui: &mut Ui, text: &str) -> Response {
    let colors = theme::palette();
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), theme::semibold(11.0), colors.text_muted);
    let size = vec2((galley.size().x + 12.0).max(20.0), 17.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 9, colors.badge_hover);
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        colors.text_muted,
    );
    response
}

/// The hover background of a list row, faded in by `hover`.
pub fn paint_row_hover(ui: &Ui, rect: Rect, hover: f32) {
    if hover > 0.0 {
        let fill = lerp_color(Color32::TRANSPARENT, theme::palette().list_hover, hover);
        ui.painter().rect_filled(rect, 4, fill);
    }
}

const GROUP_BOX_PADDING: f32 = 5.0;
const GROUP_BOX_HEADER_INSET: f32 = 4.0;

/// Height of a group box header with a 28 px button, such as the fullscreen toggle.
pub const BUTTON_HEADER_HEIGHT: f32 = 28.0;

/// Height of a group box header that only contains text.
pub fn text_header_height(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body).ceil()
}

/// Draws a group box with the default 5 px padding. See [`group_box_padded_in`].
pub fn group_box_in<R>(
    ui: &mut Ui,
    outer: Rect,
    fill_height: bool,
    header_height: f32,
    header: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui) -> R,
) -> (R, Rect) {
    group_box_padded_in(
        ui,
        outer,
        fill_height,
        header_height,
        GROUP_BOX_PADDING,
        header,
        body,
    )
}

/// Draws a group box, a rounded border whose top line is interrupted by the header,
/// inside `outer` without moving the cursor. Returns the body result and the border rectangle.
pub fn group_box_padded_in<R>(
    ui: &mut Ui,
    outer: Rect,
    fill_height: bool,
    header_height: f32,
    padding: f32,
    header: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui) -> R,
) -> (R, Rect) {
    // The border is painted in three clipped parts that leave a gap behind the header, so the
    // group box works on any background.
    let border_shapes = [(); 3].map(|_| ui.painter().add(Shape::Noop));

    let header_max = Rect::from_min_size(
        pos2(outer.left() + GROUP_BOX_HEADER_INSET, outer.top()),
        vec2(
            (outer.width() - GROUP_BOX_HEADER_INSET * 2.0).max(0.0),
            header_height,
        ),
    );
    let mut header_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(header_max)
            .layout(Layout::left_to_right(Align::Center)),
    );
    header_ui.add_space(3.0);
    header(&mut header_ui);
    header_ui.add_space(3.0);
    let header_rect = header_ui.min_rect();
    let border_top = outer.top() + header_height / 2.0;

    let body_max = Rect::from_min_max(
        pos2(
            outer.left() + padding + 1.0,
            outer.top() + header_height + padding,
        ),
        pos2(
            outer.right() - padding - 1.0,
            outer.bottom() - padding - 1.0,
        ),
    );
    let mut body_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(body_max)
            .layout(Layout::top_down(Align::Min)),
    );
    let result = body(&mut body_ui);
    let body_bottom = if fill_height {
        body_max.bottom()
    } else {
        body_ui.min_rect().bottom()
    };

    let frame = Rect::from_min_max(
        pos2(outer.left(), border_top),
        pos2(outer.right(), body_bottom + padding + 1.0),
    );
    let border = egui::epaint::RectShape::stroke(
        frame,
        CornerRadius::same(4),
        Stroke::new(1.0, theme::palette().border_subtle),
        StrokeKind::Inside,
    );
    let everything = Rect::EVERYTHING;
    let clips = [
        Rect::from_min_max(everything.min, pos2(header_rect.left(), everything.max.y)),
        Rect::from_min_max(pos2(header_rect.right(), everything.min.y), everything.max),
        Rect::from_min_max(
            pos2(header_rect.left(), header_rect.bottom()),
            pos2(header_rect.right(), everything.max.y),
        ),
    ];
    for (shape, clip) in border_shapes.into_iter().zip(clips) {
        ui.painter()
            .with_clip_rect(clip.intersect(ui.clip_rect()))
            .set(shape, border.clone());
    }
    (result, frame)
}

/// Group box header text. Plain headers use the regular weight.
pub fn header_text(ui: &mut Ui, text: &str, semibold: bool) -> Response {
    let font = if semibold {
        theme::semibold(theme::BODY_SIZE)
    } else {
        theme::body(theme::BODY_SIZE)
    };
    label(ui, text, font, theme::palette().text)
}

pub fn text_width(ui: &Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER)
        .size()
        .x
}

/// Height of one line of text in this font.
pub fn text_height(ui: &Ui, font: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(" ".to_owned(), font, Color32::PLACEHOLDER)
        .size()
        .y
}

/// One line of text cut with an ellipsis at `width`, like `TextTrimming="CharacterEllipsis"`.
pub fn ellipsized(
    ui: &Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(1.0));
    ui.painter().layout_job(job)
}

/// Width of a [`segmented`] control with these options.
pub fn segmented_width(ui: &Ui, options: &[&str]) -> f32 {
    options
        .iter()
        .map(|text| text_width(ui, text, theme::semibold(theme::BODY_SIZE)) + 12.0 * 2.0)
        .sum::<f32>()
        + 2.0
}

/// Mutually exclusive options in one bordered row, such as a list filter. Returns the index of the
/// clicked option.
pub fn segmented(ui: &mut Ui, options: &[&str], selected: usize) -> Option<usize> {
    let height = control_height(ui);
    let (rect, _) =
        ui.allocate_exact_size(vec2(segmented_width(ui, options), height), Sense::hover());
    let colors = theme::palette();
    let font = theme::semibold(theme::BODY_SIZE);
    ui.painter().rect(
        rect,
        4,
        colors.surface,
        Stroke::new(1.0, colors.border),
        StrokeKind::Inside,
    );
    let mut clicked = None;
    let mut x = rect.left() + 1.0;
    for (index, text) in options.iter().enumerate() {
        let width = text_width(ui, text, font.clone()) + 12.0 * 2.0;
        let segment = Rect::from_min_max(
            pos2(x, rect.top() + 1.0),
            pos2(x + width, rect.bottom() - 1.0),
        );
        let response = ui
            .interact(
                segment,
                ui.scope_id().with(("segment", index)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let is_selected = index == selected;
        let hover = hover_amount(ui, &response, !is_selected);
        let first = index == 0;
        let last = index + 1 == options.len();
        let radius = CornerRadius {
            nw: if first { 3 } else { 0 },
            sw: if first { 3 } else { 0 },
            ne: if last { 3 } else { 0 },
            se: if last { 3 } else { 0 },
        };
        if is_selected {
            ui.painter()
                .rect_filled(segment, radius, colors.accent_soft);
        } else {
            if hover > 0.0 {
                ui.painter().rect_filled(
                    segment,
                    radius,
                    lerp_color(Color32::TRANSPARENT, colors.hover, hover),
                );
            }
        }
        if !first {
            ui.painter().line_segment(
                [
                    pos2(segment.left() - 0.5, segment.top()),
                    pos2(segment.left() - 0.5, segment.bottom()),
                ],
                Stroke::new(1.0, colors.border),
            );
        }
        let text_color = if is_selected {
            colors.accent
        } else {
            colors.text
        };
        ui.painter().text(
            segment.center(),
            Align2::CENTER_CENTER,
            text,
            font.clone(),
            text_color,
        );
        if response.clicked() {
            clicked = Some(index);
        }
        x += width;
    }
    clicked
}

/// A small rounded badge with a short fact, such as "12 files".
pub fn badge(ui: &mut Ui, text: &str) -> Response {
    let colors = theme::palette();
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), theme::semibold(11.0), colors.text);
    let size = galley.size() + vec2(12.0, 6.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(
        rect,
        4,
        colors.surface_alt,
        Stroke::new(1.0, colors.border_subtle),
        StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, colors.text);
    response
}

/// A borderless text button in the accent color, underlined on hover.
pub fn text_link(ui: &mut Ui, text: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        theme::body(theme::BODY_SIZE),
        Color32::PLACEHOLDER,
    );
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    if ui.is_rect_visible(rect) {
        let colors = theme::palette();
        let color = if response.hovered() {
            colors.accent_hover
        } else {
            colors.accent
        };
        ui.painter().galley(rect.min, galley, color);
        if response.hovered() {
            let y = rect.bottom().round() - 0.5;
            ui.painter().line_segment(
                [pos2(rect.left(), y), pos2(rect.right(), y)],
                Stroke::new(1.0, color),
            );
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small tag with a remove button, such as a selected extension. Returns true when the remove
/// button was clicked.
pub fn chip(ui: &mut Ui, text: &str, remove_tooltip: &str) -> bool {
    let colors = theme::palette();
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), theme::body(theme::BODY_SIZE), colors.text);
    let close_size = 14.0;
    let size = vec2(8.0 + galley.size().x + 4.0 + close_size + 4.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let close_rect = Rect::from_center_size(
        pos2(rect.right() - 4.0 - close_size / 2.0, rect.center().y),
        Vec2::splat(close_size + 2.0),
    );
    let close = ui
        .interact(
            close_rect,
            ui.scope_id().with(("chip", text)),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(remove_tooltip);
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            11,
            colors.surface,
            Stroke::new(1.0, colors.border),
            StrokeKind::Inside,
        );
        ui.painter().galley(
            pos2(rect.left() + 8.0, rect.center().y - galley.size().y / 2.0),
            galley,
            colors.text,
        );
        let close_color = if close.hovered() {
            colors.danger
        } else {
            colors.text_muted
        };
        paint_glyph(
            ui,
            close_rect.center(),
            crate::icons::CLOSE,
            close_size,
            close_color,
        );
    }
    close.clicked()
}

/// Inner margin of the gray read-only text boxes.
const INFO_BOX_MARGIN: f32 = 5.0;

/// A gray bordered box with wrapped, scrollable gray text, used for read-only summaries.
pub fn info_box(ui: &mut Ui, rect: Rect, id_salt: &str, text: &str) {
    if rect.height() <= 0.0 || rect.width() <= 0.0 {
        return;
    }
    ui.painter().rect(
        rect,
        4,
        theme::palette().surface_alt,
        Stroke::new(1.0, theme::palette().border),
        StrokeKind::Inside,
    );
    let mut inner = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink(1.0 + INFO_BOX_MARGIN))
            .layout(Layout::top_down(Align::Min)),
    );
    egui::ScrollArea::vertical()
        .id_salt(id_salt)
        .auto_shrink([false, false])
        .show(&mut inner, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .font(theme::body(theme::BODY_SIZE))
                    .color(theme::palette().text_muted),
            );
        });
}

/// The 1 px `Separator` with 4 px vertical margin.
pub fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    let y = rect.center().y.round() + 0.5;
    ui.painter().line_segment(
        [pos2(rect.left(), y), pos2(rect.right(), y)],
        Stroke::new(1.0, theme::palette().border),
    );
}

/// A closed combo box: a white box with placeholder text and a drop-down arrow.
pub fn combo_field(ui: &mut Ui, width: f32, placeholder: &str) -> Response {
    let (response, inner) = combo_frame(ui, width);
    if ui.is_rect_visible(inner) {
        ui.painter().text(
            pos2(inner.left(), inner.center().y),
            Align2::LEFT_CENTER,
            placeholder,
            theme::body(theme::BODY_SIZE),
            theme::palette().text_muted,
        );
    }
    response
}

/// A combo box showing the selected item: an optional colored glyph, the text and an optional
/// trailing glyph.
pub fn combo_value(
    ui: &mut Ui,
    width: f32,
    icon: Option<Icon>,
    text: &str,
    trailing: Option<Icon>,
) -> Response {
    let (response, inner) = combo_frame(ui, width);
    if ui.is_rect_visible(inner) && paint_item(ui, inner, icon, text, trailing) {
        return response.on_hover_text(text);
    }
    response
}

/// Paints a glyph, a text and a trailing glyph left-aligned in `rect`. A text too long for `rect`
/// is cut short with "...", leaving room for the trailing glyph. Returns whether it was cut.
pub fn paint_item(
    ui: &Ui,
    rect: Rect,
    icon: Option<Icon>,
    text: &str,
    trailing: Option<Icon>,
) -> bool {
    let mut x = rect.left();
    if let Some(icon) = icon {
        paint_glyph(
            ui,
            pos2(x + 10.0, rect.center().y),
            icon.glyph,
            20.0,
            icon.color(),
        );
        x += 25.0;
    }
    let trailing_width = if trailing.is_some() { 5.0 + 16.0 } else { 0.0 };
    let galley = ellipsized(
        ui,
        text,
        theme::body(theme::BODY_SIZE),
        theme::palette().text,
        rect.right() - x - trailing_width,
    );
    let elided = galley.elided;
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley.clone(),
        theme::palette().text,
    );
    x += galley.size().x;
    if let Some(icon) = trailing {
        paint_glyph(
            ui,
            pos2(x + 5.0 + 8.0, rect.center().y),
            icon.glyph,
            16.0,
            icon.color(),
        );
    }
    elided
}

/// One entry of a combo box list: an optional glyph, the text and an optional trailing glyph.
pub struct ComboItem<'a> {
    pub icon: Option<Icon>,
    pub text: &'a str,
    pub trailing: Option<Icon>,
}

/// The drop-down list of a combo box opened by `response`. Returns the clicked item.
pub fn combo_popup(response: &Response, width: f32, items: &[ComboItem<'_>]) -> Option<usize> {
    let mut chosen = None;
    egui::Popup::from_toggle_button_response(response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .width(width)
        .show(|ui| {
            ui.set_min_width(width - 12.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for (index, item) in items.iter().enumerate() {
                let (rect, row) = ui.allocate_exact_size(
                    vec2(ui.available_width(), control_height(ui)),
                    Sense::click(),
                );
                if row.hovered() {
                    ui.painter().rect_filled(rect, 2, theme::palette().hover);
                }
                let elided = paint_item(
                    ui,
                    rect.shrink2(vec2(6.0, 0.0)),
                    item.icon,
                    item.text,
                    item.trailing,
                );
                let row = if elided {
                    row.on_hover_text(item.text)
                } else {
                    row
                };
                if row.clicked() {
                    chosen = Some(index);
                }
            }
        });
    chosen
}

/// The frame and arrow of a combo box. Returns its response and the content area.
fn combo_frame(ui: &mut Ui, width: f32) -> (Response, Rect) {
    let (rect, response) = ui.allocate_exact_size(vec2(width, control_height(ui)), Sense::click());
    let inner = Rect::from_min_max(
        pos2(rect.left() + 10.0, rect.top()),
        pos2(rect.right() - 24.0, rect.bottom()),
    );
    if ui.is_rect_visible(rect) {
        let border = if response.hovered() {
            theme::palette().accent
        } else {
            theme::palette().border
        };
        ui.painter().rect(
            rect,
            4,
            theme::palette().surface,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
        let arrow_color = if response.hovered() {
            theme::palette().accent
        } else {
            theme::palette().text
        };
        let tip = pos2(rect.right() - 12.0, rect.center().y + 2.0);
        ui.painter().add(Shape::convex_polygon(
            vec![tip + vec2(-4.0, -4.0), tip + vec2(4.0, -4.0), tip],
            arrow_color,
            Stroke::NONE,
        ));
    }
    (
        response.on_hover_cursor(egui::CursorIcon::PointingHand),
        inner,
    )
}

/// Height of a [`check_row`].
pub fn check_row_height(ui: &Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body).max(15.0) + 2.0
}

/// A list row with a check box and a label trimmed with an ellipsis. Clicking anywhere on the row
/// toggles it.
pub fn check_row(ui: &mut Ui, state: CheckState, text: &str, bold: bool) -> Response {
    let font = if bold {
        theme::semibold(theme::BODY_SIZE)
    } else {
        theme::body(theme::BODY_SIZE)
    };
    let job =
        egui::text::LayoutJob::simple_singleline(text.to_owned(), font, theme::palette().text);
    check_row_with(ui, state, job)
}

/// A [`check_row`] for a relative path. The folders are gray, so the file name stands out.
pub fn path_check_row(ui: &mut Ui, state: CheckState, path: &str) -> Response {
    let colors = theme::palette();
    let font = theme::body(theme::BODY_SIZE);
    let name_start = path.rfind(['\\', '/']).map_or(0, |index| index + 1);
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &path[..name_start],
        0.0,
        egui::TextFormat::simple(font.clone(), colors.text_hint),
    );
    job.append(
        &path[name_start..],
        0.0,
        egui::TextFormat::simple(font, colors.text),
    );
    check_row_with(ui, state, job)
}

fn check_row_with(ui: &mut Ui, state: CheckState, mut job: egui::text::LayoutJob) -> Response {
    let height = check_row_height(ui);
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let hover = hover_amount(ui, &response, true);
    if ui.is_rect_visible(rect) {
        paint_row_hover(ui, rect, hover);
        let box_rect = Rect::from_min_size(
            pos2(rect.left() + 4.0, rect.center().y - 7.5),
            Vec2::splat(15.0),
        );
        paint_check_box(ui, box_rect, state, response.hovered());
        let text_left = box_rect.right() + 5.0;
        job.wrap = egui::text::TextWrapping::truncate_at_width((rect.right() - text_left).max(1.0));
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(
            pos2(text_left, rect.center().y - galley.size().y / 2.0),
            galley,
            theme::palette().text,
        );
    }
    response
}

/// Paints the 14 px circle of a radio button around `center`.
pub fn paint_radio(ui: &Ui, center: Pos2, selected: bool, hovered: bool) {
    let colors = theme::palette();
    let border = if hovered {
        colors.accent
    } else {
        colors.border
    };
    ui.painter()
        .circle(center, 6.5, colors.surface, Stroke::new(1.0, border));
    if selected {
        ui.painter().circle_filled(center, 3.5, colors.text);
    }
}
