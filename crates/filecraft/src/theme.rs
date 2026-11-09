//! Colors, fonts and base styling: a light and a dark palette with the same color roles.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Theme, Visuals};
use fontique::{
    Attributes, Collection, CollectionOptions, FontStyle, FontWeight, FontWidth, GenericFamily,
    QueryFamily, QueryStatus, SourceCache,
};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The colors of one theme, named by their role.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Window and tab content background.
    pub background: Color32,
    /// Editable boxes, dialogs, popups and windows.
    pub surface: Color32,
    /// Read-only boxes and list backgrounds.
    pub surface_alt: Color32,
    pub border: Color32,
    /// Group box borders.
    pub border_subtle: Color32,
    pub text: Color32,
    /// Placeholders and secondary text.
    pub text_muted: Color32,
    /// Hints such as the `.txt` suffix, dates and the version.
    pub text_hint: Color32,

    pub accent: Color32,
    pub accent_hover: Color32,
    /// Background of active toggles and selected items.
    pub accent_soft: Color32,
    /// Text and glyphs on accent, danger and success fills.
    pub on_accent: Color32,
    /// Fill and border of indeterminate check boxes.
    pub neutral: Color32,
    pub neutral_strong: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub success: Color32,
    /// Text, fill and border of numbers that need attention, such as no selected files.
    pub warning: Color32,
    pub warning_soft: Color32,
    pub warning_border: Color32,

    /// Hover and pressed backgrounds of borderless buttons and list rows.
    pub hover: Color32,
    pub pressed: Color32,
    pub list_hover: Color32,
    pub list_action_hover: Color32,
    /// Background of the tabs whose sections are cards.
    pub section_background: Color32,

    /// Statistic badges of the preset windows.
    pub badge_background: Color32,
    pub badge_hover: Color32,
    pub badge_pressed: Color32,
    pub badge_inactive: Color32,

    pub folder_icon: Color32,
    pub tree_icon: Color32,
    pub rename_icon: Color32,
}

pub const LIGHT: Palette = Palette {
    background: rgb(0xFFFFFF),
    surface: rgb(0xFFFFFF),
    surface_alt: rgb(0xEEEEEE),
    border: rgb(0xB0B0B0),
    border_subtle: rgb(0xD5DFE5),
    text: rgb(0x333333),
    text_muted: rgb(0x666666),
    text_hint: rgb(0x808080),

    accent: rgb(0x007BFF),
    accent_hover: rgb(0x0069D9),
    accent_soft: rgb(0xE5F0FF),
    on_accent: rgb(0xFFFFFF),
    neutral: rgb(0xADB5BD),
    neutral_strong: rgb(0x6C757D),
    danger: rgb(0xDC3545),
    danger_soft: rgb(0xFFF5F5),
    success: rgb(0x28A745),
    warning: rgb(0x8A6D1F),
    warning_soft: rgb(0xFFF8E6),
    warning_border: rgb(0xF2DFA7),

    hover: rgb(0xE0E0E0),
    pressed: rgb(0xC0C0C0),
    list_hover: rgb(0xDDDDDD),
    list_action_hover: rgb(0xBFBFBF),
    section_background: rgb(0xF3F4F6),

    badge_background: rgb(0xF8F9FA),
    badge_hover: rgb(0xE9ECEF),
    badge_pressed: rgb(0xDEE2E6),
    badge_inactive: rgb(0xE0E0E0),

    folder_icon: rgb(0xFFC107),
    tree_icon: rgb(0x388E3C),
    rename_icon: rgb(0xF57C00),
};

pub const DARK: Palette = Palette {
    background: rgb(0x1E1F22),
    surface: rgb(0x2B2D30),
    surface_alt: rgb(0x26282B),
    border: rgb(0x4E5157),
    border_subtle: rgb(0x393B40),
    text: rgb(0xDFE1E5),
    text_muted: rgb(0xA8ABB0),
    text_hint: rgb(0x8C8F94),

    accent: rgb(0x3B8BF6),
    accent_hover: rgb(0x5A9DF8),
    accent_soft: rgb(0x22314A),
    on_accent: rgb(0xFFFFFF),
    neutral: rgb(0x6B7077),
    neutral_strong: rgb(0x8A9097),
    danger: rgb(0xE5534B),
    danger_soft: rgb(0x3A2426),
    success: rgb(0x3FB950),
    warning: rgb(0xE3B341),
    warning_soft: rgb(0x33291A),
    warning_border: rgb(0x5C4A22),

    hover: rgb(0x393B40),
    pressed: rgb(0x46494F),
    list_hover: rgb(0x33353A),
    list_action_hover: rgb(0x45484E),
    section_background: rgb(0x1A1B1E),

    badge_background: rgb(0x2B2D30),
    badge_hover: rgb(0x34363A),
    badge_pressed: rgb(0x3C3F44),
    badge_inactive: rgb(0x34363A),

    folder_icon: rgb(0xE8B931),
    tree_icon: rgb(0x4CAF50),
    rename_icon: rgb(0xF0883E),
};

/// Whether the dark palette is active. The application has a single window, so the palette of the
/// current frame is kept here instead of being passed to every drawing function.
static DARK_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The palette of the theme shown in the current frame.
pub fn palette() -> &'static Palette {
    if DARK_ACTIVE.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

/// Follows the theme of `ctx`. Called at the start of every frame.
pub fn sync(ctx: &egui::Context) {
    DARK_ACTIVE.store(ctx.theme() == Theme::Dark, Ordering::Relaxed);
}

/// Body text size in logical pixels.
pub const BODY_SIZE: f32 = 12.0;

const FONT_REGULAR: &str = "ui-regular";
const FONT_SEMIBOLD: &str = "ui-semibold";
const FONT_BOLD: &str = "ui-bold";
const FONT_ICONS: &str = "material-icons";

const MATERIAL_ICONS: &[u8] = include_bytes!("../../../assets/fonts/MaterialIcons-Regular.ttf");

pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FONT_SEMIBOLD.into()))
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FONT_BOLD.into()))
}

pub fn icons(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FONT_ICONS.into()))
}

/// Multiplies the alpha of a color, used for disabled controls.
pub fn faded(color: Color32, opacity: f32) -> Color32 {
    color.gamma_multiply(opacity)
}

/// Installs the fonts and the light and dark styles, then shows `theme`, or the system theme when
/// no theme was chosen yet.
pub fn install(ctx: &egui::Context, theme: Option<Theme>) {
    ctx.set_fonts(font_definitions());
    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(egui::TextStyle::Body, body(BODY_SIZE));
        style
            .text_styles
            .insert(egui::TextStyle::Button, body(BODY_SIZE));
        style.text_styles.insert(egui::TextStyle::Small, body(10.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, semibold(16.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, FontId::monospace(BODY_SIZE));
        style.spacing.item_spacing = egui::vec2(0.0, 0.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 20.0;
        style.interaction.tooltip_delay = 0.4;
    });
    ctx.style_mut_of(Theme::Light, |style| style.visuals = visuals(Theme::Light));
    ctx.style_mut_of(Theme::Dark, |style| style.visuals = visuals(Theme::Dark));
    ctx.set_theme(match theme {
        Some(theme) => egui::ThemePreference::from(theme),
        None => egui::ThemePreference::System,
    });
    sync(ctx);
}

fn visuals(theme: Theme) -> Visuals {
    let (mut visuals, colors) = match theme {
        Theme::Light => (Visuals::light(), &LIGHT),
        Theme::Dark => (Visuals::dark(), &DARK),
    };
    visuals.panel_fill = colors.background;
    visuals.window_fill = colors.surface;
    visuals.extreme_bg_color = colors.surface;
    visuals.faint_bg_color = colors.surface_alt;
    visuals.override_text_color = Some(colors.text);
    visuals.selection.bg_fill = colors.accent.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, colors.accent);
    visuals.hyperlink_color = colors.accent;
    visuals.window_stroke = Stroke::new(1.0, colors.border);
    visuals.window_corner_radius = 4.into();
    visuals.menu_corner_radius = 4.into();
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, colors.text);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, colors.border);
    visuals.widgets.noninteractive.bg_fill = colors.surface;
    visuals.widgets.noninteractive.weak_bg_fill = colors.surface;
    visuals
}

/// Uses the operating system UI font and the bundled Material Icons font.
fn font_definitions() -> FontDefinitions {
    let mut definitions = FontDefinitions::empty();
    let mut fonts = SystemUiFonts::new();

    let regular = fonts.find(FontWeight::NORMAL);
    let semibold = fonts
        .find(FontWeight::SEMI_BOLD)
        .or_else(|| regular.clone());
    let bold = fonts.find(FontWeight::BOLD).or_else(|| semibold.clone());
    let monospace = fonts.find_generic(GenericFamily::Monospace, FontWeight::NORMAL);

    definitions.font_data.insert(
        FONT_ICONS.to_owned(),
        Arc::new(FontData::from_static(MATERIAL_ICONS)),
    );

    let mut family = |name: &str, data: Option<FontData>, family: FontFamily| {
        if let Some(data) = data {
            definitions
                .font_data
                .insert(name.to_owned(), Arc::new(data));
            definitions
                .families
                .entry(family)
                .or_default()
                .push(name.to_owned());
        }
    };
    family(FONT_REGULAR, regular.clone(), FontFamily::Proportional);
    family(
        FONT_SEMIBOLD,
        semibold,
        FontFamily::Name(FONT_SEMIBOLD.into()),
    );
    family(FONT_BOLD, bold, FontFamily::Name(FONT_BOLD.into()));
    family("ui-monospace", monospace.or(regular), FontFamily::Monospace);

    definitions.families.insert(
        FontFamily::Name(FONT_ICONS.into()),
        vec![FONT_ICONS.to_owned()],
    );

    // Characters missing from these fonts, such as CJK file names, are resolved by
    // eframe's system font provider.
    for family in [
        FontFamily::Name(FONT_SEMIBOLD.into()),
        FontFamily::Name(FONT_BOLD.into()),
        FontFamily::Monospace,
    ] {
        let names = definitions.families.entry(family).or_default();
        if !names.iter().any(|name| name == FONT_REGULAR)
            && definitions.font_data.contains_key(FONT_REGULAR)
        {
            names.push(FONT_REGULAR.to_owned());
        }
    }

    if !definitions.font_data.contains_key(FONT_REGULAR) {
        eprintln!(
            "FileCraft: no system UI font was found, text falls back to the system font provider."
        );
    }

    definitions
}

struct SystemUiFonts {
    collection: Collection,
    cache: SourceCache,
}

impl SystemUiFonts {
    fn new() -> Self {
        Self {
            collection: Collection::new(CollectionOptions {
                shared: false,
                system_fonts: true,
            }),
            cache: SourceCache::default(),
        }
    }

    fn find(&mut self, weight: FontWeight) -> Option<FontData> {
        self.find_generic(GenericFamily::SystemUi, weight)
            .or_else(|| self.find_generic(GenericFamily::SansSerif, weight))
    }

    /// Returns the first font of the family whose weight matches without synthesis.
    fn find_generic(&mut self, generic: GenericFamily, weight: FontWeight) -> Option<FontData> {
        let mut query = self.collection.query(&mut self.cache);
        query.set_families([QueryFamily::Generic(generic)]);
        query.set_attributes(Attributes::new(
            FontWidth::NORMAL,
            FontStyle::Normal,
            weight,
        ));
        let mut found = None;
        query.matches_with(|font| {
            if !font.synthesis.any() || weight == FontWeight::NORMAL {
                let mut data = FontData::from_owned(font.blob.as_ref().to_vec());
                data.index = font.index;
                found = Some(data);
            }
            QueryStatus::Stop
        });
        found
    }
}
