//! Design tokens of the island interface: colour, type, space, radius,
//! elevation, layers and motion. One table; widgets read roles from it and
//! never carry colours of their own.
//!
//! Source of truth: `docs/superpowers/specs/2026-11-11-island-ui-redesign.md`
//! section 6. The contrast test at the bottom recomputes every pair that
//! section lists, so a palette edit cannot quietly break legibility.
use bevy::prelude::*;

const fn hex(value: u32) -> Color {
    Color::srgb(
        ((value >> 16) & 0xFF) as f32 / 255.0,
        ((value >> 8) & 0xFF) as f32 / 255.0,
        (value & 0xFF) as f32 / 255.0,
    )
}

// --- Chrome: always-visible bars, docks, tooltips, toasts ------------------
pub const BARK_900: Color = hex(0x2B2018);
pub const BARK_800: Color = hex(0x3A2C21);
pub const BARK_700: Color = hex(0x4C3B2D);
pub const BARK_600: Color = hex(0x6B5443);
/// Chrome is drawn at this alpha over the world.
pub const CHROME_ALPHA: f32 = 0.94;
pub const CREAM_50: Color = hex(0xFBF5E6);
pub const CREAM_300: Color = hex(0xD8C8AA);

// --- Parchment: things that are opened and read ----------------------------
pub const PARCHMENT_100: Color = hex(0xF5ECD7);
pub const PARCHMENT_200: Color = hex(0xECDFC1);
pub const PARCHMENT_300: Color = hex(0xDFCEA8);
pub const BORDER_SOFT: Color = hex(0xC8B48A);
pub const BORDER_CONTROL: Color = hex(0x8C744D);
pub const INK_900: Color = hex(0x2B1F16);
pub const INK_700: Color = hex(0x55432F);
pub const INK_500: Color = hex(0x6A5642);

// --- Accents ---------------------------------------------------------------
pub const HONEY_500: Color = hex(0xE8B04B);
pub const HONEY_200: Color = hex(0xF8E3A8);
pub const HONEY_700: Color = hex(0x8A5A0B);
pub const MOSS_600: Color = hex(0x3F6B34);
pub const MOSS_700: Color = hex(0x33582A);
pub const MOSS_800: Color = hex(0x294A22);
pub const SEA_600: Color = hex(0x1F6A8C);
pub const SCRIM: Color = Color::srgba(24.0 / 255.0, 16.0 / 255.0, 10.0 / 255.0, 0.55);
pub const SHADOW: Color = hex(0x18100A);

// --- Status: always with a shape, and a word where there is room -----------
pub const GOOD_INK: Color = hex(0x0F6E5A);
pub const WARN_INK: Color = hex(0xB26A00);
pub const CRIT_INK: Color = hex(0xA3262E);
pub const INFO_INK: Color = hex(0x2A5DB0);
pub const GOOD_LIGHT: Color = hex(0x63D6C0);
pub const WARN_LIGHT: Color = hex(0xF7C64A);
pub const CRIT_LIGHT: Color = hex(0xFF7A66);
pub const INFO_LIGHT: Color = hex(0x9DBDF5);

// --- Dusk: the optional dark panel style -----------------------------------
pub const DUSK_100: Color = BARK_800;
pub const DUSK_200: Color = BARK_700;
pub const DUSK_300: Color = hex(0x5A4636);
pub const DUSK_CAPTION: Color = hex(0xBEAC8E);
pub const DUSK_BORDER_CONTROL: Color = hex(0xA08A70);
pub const DUSK_SELECTED: Color = hex(0x5C4A22);

/// The two weights of the island's face (Barlow Regular and SemiBold).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    SemiBold,
}

/// One step of the type scale: size and line height in logical pixels, and
/// the weight. Headings, panel titles and key figures are SemiBold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Type {
    pub size: f32,
    pub line: f32,
    pub weight: Weight,
}
pub const DISPLAY: Type = Type {
    size: 56.0,
    line: 60.0,
    weight: Weight::SemiBold,
};
pub const TITLE: Type = Type {
    size: 28.0,
    line: 34.0,
    weight: Weight::SemiBold,
};
pub const HEADING: Type = Type {
    size: 20.0,
    line: 26.0,
    weight: Weight::SemiBold,
};
pub const SUBHEADING: Type = Type {
    size: 16.0,
    line: 22.0,
    weight: Weight::Regular,
};
pub const BODY: Type = Type {
    size: 14.0,
    line: 20.0,
    weight: Weight::Regular,
};
pub const DENSE: Type = Type {
    size: 13.0,
    line: 18.0,
    weight: Weight::Regular,
};
/// Nothing is smaller than this.
pub const CAPTION: Type = Type {
    size: 12.0,
    line: 16.0,
    weight: Weight::Regular,
};
/// A key figure (a total on the bar, a count): body size, SemiBold.
pub const FIGURE: Type = Type {
    size: 14.0,
    line: 20.0,
    weight: Weight::SemiBold,
};
pub const TYPE_SCALE: [Type; 7] = [DISPLAY, TITLE, HEADING, SUBHEADING, BODY, DENSE, CAPTION];

/// Spacing steps; panel padding is `SPACE_12`, the screen margin `SPACE_8`.
pub const SPACE: [f32; 8] = [2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0];
pub const SCREEN_MARGIN: f32 = 8.0;
pub const PANEL_PADDING: f32 = 12.0;

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 10.0;

// Control heights.
pub const BUTTON_SMALL: f32 = 28.0;
pub const BUTTON_MEDIUM: f32 = 36.0;
pub const BUTTON_LARGE: f32 = 44.0;
pub const ICON_BUTTON: f32 = 32.0;
pub const DOCK_TILE: (f32, f32) = (64.0, 44.0);
pub const DOCK_TILE_STANDARD: f32 = 56.0;
pub const SEGMENT_CHROME: f32 = 30.0;
pub const SEGMENT_PAPER: f32 = 26.0;
pub const CHIP_CHROME: f32 = 28.0;
pub const CHIP_PAPER: f32 = 24.0;
pub const ROW_ONE_LINE: f32 = 28.0;
pub const ROW_TWO_LINES: f32 = 44.0;
pub const TABLE_ROW: f32 = 27.0;
pub const TAB: f32 = 36.0;
pub const KEY_CAP: f32 = 18.0;
pub const TOP_BAR: f32 = 44.0;
pub const DOCK: f32 = 56.0;
/// Smallest pointer target: height, then width.
pub const TARGET_MIN: (f32, f32) = (28.0, 32.0);
/// On-screen icon sizes.
pub const ICON_SIZES: [f32; 6] = [12.0, 14.0, 16.0, 20.0, 24.0, 32.0];

/// Right-column widths at the Wide layout: inspector, ledger.
pub const COLUMN_INSPECTOR: f32 = 344.0;
pub const COLUMN_LEDGER: f32 = 440.0;
/// The same at the Standard layout (1024 to 1279 effective pixels wide).
pub const COLUMN_INSPECTOR_STANDARD: f32 = 320.0;
pub const COLUMN_LEDGER_STANDARD: f32 = 400.0;
pub const ATTENTION_COLUMN: f32 = 316.0;
pub const ATTENTION_COLUMN_STANDARD: f32 = 300.0;
pub const TOOLTIP_WIDTH: f32 = 280.0;
pub const TOOLTIP_WIDTH_DETAIL: f32 = 340.0;

/// One `BoxShadow` step: y offset, blur, alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Elevation {
    pub y: f32,
    pub blur: f32,
    pub alpha: f32,
}
/// World tags.
pub const SHADOW_1: Elevation = Elevation {
    y: 1.0,
    blur: 2.0,
    alpha: 0.30,
};
/// Panels, docks, tooltips.
pub const SHADOW_2: Elevation = Elevation {
    y: 4.0,
    blur: 12.0,
    alpha: 0.35,
};
/// Modals.
pub const SHADOW_3: Elevation = Elevation {
    y: 12.0,
    blur: 32.0,
    alpha: 0.45,
};
pub fn shadow(level: Elevation) -> BoxShadow {
    BoxShadow::new(
        SHADOW.with_alpha(level.alpha),
        px(0),
        px(level.y),
        px(0),
        px(level.blur),
    )
}

/// `GlobalZIndex` layers, back to front.
pub mod layer {
    pub const WORLD_TAG: i32 = 10;
    pub const CHROME: i32 = 30;
    pub const PANEL: i32 = 32;
    pub const POPOVER: i32 = 34;
    pub const TOAST: i32 = 40;
    /// Above toasts: a tooltip is what the pointer asked for this moment
    /// (the Paused pill covered the totals' tooltips in the native capture).
    pub const TOOLTIP: i32 = 42;
    pub const SCRIM: i32 = 50;
    pub const MODAL: i32 = 52;
}

/// Durations in seconds. Animation is view time, never simulation time.
pub mod motion {
    pub const HOVER: f32 = 0.080;
    pub const PRESS: f32 = 0.060;
    pub const PANEL_OPEN: f32 = 0.160;
    pub const PANEL_CLOSE: f32 = 0.120;
    pub const TOOLTIP_FADE: f32 = 0.100;
    pub const TOAST_IN: f32 = 0.200;
    pub const TOAST_STAY: f32 = 3.0;
    pub const TOAST_STAY_ALERT: f32 = 6.0;
    pub const TOAST_OUT: f32 = 0.400;
    pub const MODAL: f32 = 0.200;
    pub const PULSE: f32 = 0.600;
    pub const PULSE_EVERY: f32 = 2.0;
    /// Tooltip level 1 after this rest; level 2 after a further `TOOLTIP_DETAIL`.
    pub const TOOLTIP_SHORT: f32 = 0.2;
    pub const TOOLTIP_NORMAL: f32 = 0.4;
    pub const TOOLTIP_LONG: f32 = 0.8;
    pub const TOOLTIP_DETAIL: f32 = 1.2;
    pub const TOOLTIP_FOCUS: f32 = 0.6;
    /// Moving to another element within this of a tooltip closing shows the next at once.
    pub const TOOLTIP_CARRY: f32 = 0.3;
}

/// The two contexts a widget can sit in. A widget takes its colours from the
/// context, never from its caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Surface {
    /// Dark warm chrome: top bar, docks, tooltips, toasts.
    Chrome,
    /// A panel that was opened to be read: parchment, or Dusk.
    Paper,
}

/// The player's panel style. Chrome is the same in both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelStyle {
    #[default]
    Parchment,
    Dusk,
}

/// What a status colour means. Never shown without its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Good,
    Warn,
    Crit,
    Info,
    /// Unchanged, empty, to do.
    Idle,
}

/// Role colours of one surface. Dusk is this table with other values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roles {
    pub surface: Color,
    /// Cards, rows, secondary buttons, bar tracks.
    pub raised: Color,
    /// Hover and pressed; key caps.
    pub sunken: Color,
    pub border_soft: Color,
    pub border_control: Color,
    /// The frame of a panel, or the edge of a bar.
    pub frame: Color,
    pub ink: Color,
    pub ink_secondary: Color,
    pub ink_caption: Color,
    /// Selected row, card or slot.
    pub selected: Color,
    pub selected_edge: Color,
    /// The lit segment or tile, and the text on it.
    pub active: Color,
    pub on_active: Color,
    pub focus: Color,
    pub link: Color,
    pub good: Color,
    pub warn: Color,
    pub crit: Color,
    pub info: Color,
    /// Progress fill.
    pub progress: Color,
    /// Key cap fill, edge and text.
    pub key: Color,
    pub key_edge: Color,
    pub key_ink: Color,
}

impl Roles {
    pub const fn status(&self, status: Status) -> Color {
        match status {
            Status::Good => self.good,
            Status::Warn => self.warn,
            Status::Crit => self.crit,
            Status::Info => self.info,
            Status::Idle => self.ink_caption,
        }
    }
}

pub const CHROME: Roles = Roles {
    surface: BARK_900,
    raised: BARK_800,
    sunken: BARK_700,
    border_soft: BARK_600,
    border_control: BARK_600,
    frame: BARK_600,
    ink: CREAM_50,
    ink_secondary: CREAM_300,
    ink_caption: CREAM_300,
    selected: BARK_700,
    selected_edge: HONEY_500,
    active: HONEY_500,
    on_active: INK_900,
    focus: HONEY_500,
    link: INFO_LIGHT,
    good: GOOD_LIGHT,
    warn: WARN_LIGHT,
    crit: CRIT_LIGHT,
    info: INFO_LIGHT,
    progress: HONEY_500,
    key: BARK_700,
    key_edge: BARK_600,
    key_ink: CREAM_300,
};

pub const PARCHMENT: Roles = Roles {
    surface: PARCHMENT_100,
    raised: PARCHMENT_200,
    sunken: PARCHMENT_300,
    border_soft: BORDER_SOFT,
    border_control: BORDER_CONTROL,
    frame: BARK_700,
    ink: INK_900,
    ink_secondary: INK_700,
    ink_caption: INK_500,
    selected: HONEY_200,
    selected_edge: HONEY_700,
    active: BARK_700,
    on_active: CREAM_50,
    focus: SEA_600,
    link: SEA_600,
    good: GOOD_INK,
    warn: WARN_INK,
    crit: CRIT_INK,
    info: INFO_INK,
    progress: BARK_700,
    key: PARCHMENT_300,
    key_edge: BORDER_CONTROL,
    key_ink: INK_700,
};

pub const DUSK: Roles = Roles {
    surface: DUSK_100,
    raised: DUSK_200,
    sunken: DUSK_300,
    border_soft: BARK_600,
    border_control: DUSK_BORDER_CONTROL,
    frame: BARK_600,
    ink: CREAM_50,
    ink_secondary: CREAM_300,
    ink_caption: DUSK_CAPTION,
    selected: DUSK_SELECTED,
    selected_edge: HONEY_500,
    active: HONEY_500,
    on_active: INK_900,
    focus: HONEY_500,
    link: INFO_LIGHT,
    good: GOOD_LIGHT,
    warn: WARN_LIGHT,
    crit: CRIT_LIGHT,
    info: INFO_LIGHT,
    progress: HONEY_500,
    key: DUSK_300,
    key_edge: DUSK_BORDER_CONTROL,
    key_ink: CREAM_300,
};

/// The active panel style. View state: never saved in a world or hashed.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Theme {
    pub style: PanelStyle,
}

impl Theme {
    pub const fn roles(&self, surface: Surface) -> &'static Roles {
        match (surface, self.style) {
            (Surface::Chrome, _) => &CHROME,
            (Surface::Paper, PanelStyle::Parchment) => &PARCHMENT,
            (Surface::Paper, PanelStyle::Dusk) => &DUSK,
        }
    }
}

/// The chrome fill as drawn over the world.
pub fn chrome_fill() -> Color {
    BARK_900.with_alpha(CHROME_ALPHA)
}

fn channel(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG 2.1 relative luminance of an opaque sRGB colour.
pub fn luminance(color: Color) -> f32 {
    let color = color.to_srgba();
    0.2126 * channel(color.red) + 0.7152 * channel(color.green) + 0.0722 * channel(color.blue)
}

/// WCAG 2.1 contrast ratio, 1 to 21.
pub fn contrast(foreground: Color, background: Color) -> f32 {
    let (a, b) = (luminance(foreground), luminance(background));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `top` at its own alpha over an opaque `under`, in sRGB as the UI blends.
pub fn over(top: Color, under: Color) -> Color {
    let (top, under) = (top.to_srgba(), under.to_srgba());
    let mix = |a: f32, b: f32| a * top.alpha + b * (1.0 - top.alpha);
    Color::srgb(
        mix(top.red, under.red),
        mix(top.green, under.green),
        mix(top.blue, under.blue),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every pair of section 6.2, with the ratio it must reach.
    fn pairs() -> Vec<(&'static str, Color, Color, f32)> {
        let mut pairs = vec![
            ("ink-900 on parchment-100", INK_900, PARCHMENT_100, 4.5),
            ("ink-900 on parchment-200", INK_900, PARCHMENT_200, 4.5),
            ("ink-900 on parchment-300", INK_900, PARCHMENT_300, 4.5),
            ("ink-900 on honey-200", INK_900, HONEY_200, 4.5),
            ("ink-900 on honey-500", INK_900, HONEY_500, 4.5),
            ("ink-700 on parchment-100", INK_700, PARCHMENT_100, 4.5),
            ("ink-700 on parchment-200", INK_700, PARCHMENT_200, 4.5),
            ("ink-700 on parchment-300", INK_700, PARCHMENT_300, 4.5),
            ("ink-500 on parchment-100", INK_500, PARCHMENT_100, 4.5),
            ("ink-500 on parchment-200", INK_500, PARCHMENT_200, 4.5),
            ("cream-50 on bark-900", CREAM_50, BARK_900, 4.5),
            ("cream-50 on bark-800", CREAM_50, BARK_800, 4.5),
            ("cream-50 on bark-700", CREAM_50, BARK_700, 4.5),
            ("cream-300 on bark-900", CREAM_300, BARK_900, 4.5),
            ("cream-300 on bark-800", CREAM_300, BARK_800, 4.5),
            ("cream-300 on bark-700", CREAM_300, BARK_700, 4.5),
            ("cream-50 on moss-600", CREAM_50, MOSS_600, 4.5),
            ("cream-50 on moss-700", CREAM_50, MOSS_700, 4.5),
            ("cream-50 on moss-800", CREAM_50, MOSS_800, 4.5),
            ("cream-50 on crit-ink", CREAM_50, CRIT_INK, 4.5),
            ("sea-600 link on parchment-100", SEA_600, PARCHMENT_100, 4.5),
            ("sea-600 link on parchment-200", SEA_600, PARCHMENT_200, 4.5),
            ("honey-500 focus on bark-900", HONEY_500, BARK_900, 3.0),
            ("honey-500 focus on bark-800", HONEY_500, BARK_800, 3.0),
            (
                "sea-600 focus on parchment-100",
                SEA_600,
                PARCHMENT_100,
                3.0,
            ),
            (
                "sea-600 focus on parchment-200",
                SEA_600,
                PARCHMENT_200,
                3.0,
            ),
            (
                "border-control on parchment-100",
                BORDER_CONTROL,
                PARCHMENT_100,
                3.0,
            ),
            (
                "border-control on parchment-200",
                BORDER_CONTROL,
                PARCHMENT_200,
                3.0,
            ),
            ("warn-ink on parchment-100", WARN_INK, PARCHMENT_100, 3.0),
            ("warn-ink on parchment-200", WARN_INK, PARCHMENT_200, 3.0),
        ];
        for (name, ink) in [
            ("good-ink", GOOD_INK),
            ("crit-ink", CRIT_INK),
            ("info-ink", INFO_INK),
        ] {
            // Status inks are also used as text: the stricter ratio.
            pairs.push((name, ink, PARCHMENT_100, 4.5));
            pairs.push((name, ink, PARCHMENT_200, 4.5));
        }
        for (name, light) in [
            ("good-light", GOOD_LIGHT),
            ("warn-light", WARN_LIGHT),
            ("crit-light", CRIT_LIGHT),
            ("info-light", INFO_LIGHT),
        ] {
            pairs.push((name, light, BARK_900, 4.5));
            // Icons and fills only on bark-700.
            pairs.push((name, light, BARK_700, 3.0));
        }
        pairs
    }

    #[test]
    fn every_listed_pair_reaches_its_contrast() {
        for (name, foreground, background, needed) in pairs() {
            let ratio = contrast(foreground, background);
            assert!(
                // The spec rounds to one decimal; sea-600 on parchment-200 is 4.5 there.
                ratio + 0.05 >= needed,
                "{name}: {ratio:.2} is below {needed}"
            );
        }
    }

    #[test]
    fn the_spec_ratios_are_the_computed_ones() {
        for (foreground, background, listed) in [
            (INK_900, PARCHMENT_100, 13.6),
            (INK_900, HONEY_500, 8.2),
            (INK_700, PARCHMENT_100, 8.0),
            (INK_500, PARCHMENT_100, 5.9),
            (CREAM_50, BARK_900, 14.6),
            (CREAM_300, BARK_900, 9.6),
            (CREAM_50, MOSS_600, 5.7),
            (CREAM_50, CRIT_INK, 6.7),
            (SEA_600, PARCHMENT_100, 5.1),
            (HONEY_500, BARK_900, 8.1),
            (BORDER_CONTROL, PARCHMENT_100, 3.8),
            (WARN_INK, PARCHMENT_100, 3.6),
            (GOOD_LIGHT, BARK_900, 9.0),
            (CRIT_LIGHT, BARK_700, 4.2),
        ] {
            let ratio = contrast(foreground, background);
            assert!(
                (ratio - listed).abs() < 0.15,
                "listed {listed}, computed {ratio:.2}"
            );
        }
        // The rule the table gives: caption ink is not used on parchment-300.
        assert!(contrast(INK_500, PARCHMENT_300) < 4.5);
        // Amber is never text on parchment.
        assert!(contrast(WARN_INK, PARCHMENT_100) < 4.5);
    }

    #[test]
    fn chrome_text_stays_readable_over_the_world() {
        for (name, ground) in [
            ("white", Color::WHITE),
            ("sand", hex(0xE2D3A3)),
            ("grass", hex(0x6FA04A)),
            ("sea", hex(0x2F7FA8)),
            ("night", hex(0x0B1020)),
        ] {
            let chrome = over(chrome_fill(), ground);
            for (ink, needed) in [(CREAM_50, 4.5), (CREAM_300, 4.5), (HONEY_500, 3.0)] {
                assert!(contrast(ink, chrome) >= needed, "{name}");
            }
        }
    }

    #[test]
    fn dusk_and_every_role_table_keep_text_and_controls_legible() {
        for (name, roles) in [
            ("chrome", &CHROME),
            ("parchment", &PARCHMENT),
            ("dusk", &DUSK),
        ] {
            for ground in [roles.surface, roles.raised] {
                assert!(contrast(roles.ink, ground) >= 4.5, "{name} ink");
                assert!(
                    contrast(roles.ink_secondary, ground) >= 4.5,
                    "{name} secondary"
                );
                assert!(contrast(roles.ink_caption, ground) >= 4.5, "{name} caption");
                assert!(contrast(roles.focus, ground) + 0.05 >= 3.0, "{name} focus");
                assert!(
                    contrast(roles.border_control, ground) >= 3.0 || name == "chrome",
                    "{name} control edge"
                );
                for status in [Status::Good, Status::Warn, Status::Crit, Status::Info] {
                    assert!(
                        contrast(roles.status(status), ground) >= 3.0,
                        "{name} {status:?}"
                    );
                }
            }
            assert!(
                contrast(roles.on_active, roles.active) >= 4.5,
                "{name} active"
            );
            assert!(
                contrast(roles.ink, roles.selected) >= 4.5,
                "{name} selected"
            );
            assert!(contrast(roles.key_ink, roles.key) >= 4.5, "{name} key cap");
        }
        // Chrome is the same in both styles; only paper changes.
        let dusk = Theme {
            style: PanelStyle::Dusk,
        };
        assert_eq!(dusk.roles(Surface::Chrome), &CHROME);
        assert_eq!(dusk.roles(Surface::Paper), &DUSK);
        assert_eq!(Theme::default().roles(Surface::Paper), &PARCHMENT);
    }

    #[test]
    fn the_type_scale_and_targets_follow_the_spec() {
        assert!(TYPE_SCALE.iter().all(|step| step.size >= CAPTION.size));
        assert!(
            TYPE_SCALE
                .windows(2)
                .all(|pair| pair[0].size > pair[1].size)
        );
        assert!(TYPE_SCALE.iter().all(|step| step.line > step.size));
        // Headings and titles are SemiBold, reading text is Regular.
        assert!(
            [DISPLAY, TITLE, HEADING, FIGURE]
                .iter()
                .all(|step| step.weight == Weight::SemiBold)
        );
        assert!(
            [SUBHEADING, BODY, DENSE, CAPTION]
                .iter()
                .all(|step| step.weight == Weight::Regular)
        );
        const {
            assert!(BUTTON_SMALL >= TARGET_MIN.0 && ICON_BUTTON >= TARGET_MIN.1);
            assert!(layer::TOOLTIP > layer::POPOVER && layer::POPOVER > layer::PANEL);
            assert!(layer::TOOLTIP > layer::TOAST && layer::SCRIM > layer::TOOLTIP);
            assert!(layer::PANEL > layer::CHROME && layer::MODAL > layer::SCRIM);
        }
    }
}
