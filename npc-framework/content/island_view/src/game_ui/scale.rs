//! Interface size: an automatic factor from the window, times the player's
//! setting, and the layout class that follows from the effective size.
//!
//! Host state only. The host stores the setting beside the island save and
//! writes [`InterfaceScale`]; this module turns it into Bevy's `UiScale`.
use bevy::prelude::*;
use bevy::ui::UiScale;
use bevy::window::PrimaryWindow;

/// The design frame: one logical pixel is one design pixel at this size.
pub const DESIGN_SIZE: (f32, f32) = (1280.0, 720.0);
pub const AUTO_RANGE: (f32, f32) = (1.0, 2.5);
/// The player's own factor, in percent.
pub const PERCENT_RANGE: (u8, u8) = (80, 140);

/// `clamp(round_down_to_0.05(min(width / 1280, height / 720)), 1.0, 2.5)`.
pub fn auto_factor(width: f32, height: f32) -> f32 {
    let fit = (width / DESIGN_SIZE.0).min(height / DESIGN_SIZE.1);
    // A hair of slack, so 1920 / 1280 = 1.5 does not round down to 1.45.
    let stepped = ((fit + 1e-4) * 20.0).floor() / 20.0;
    stepped.clamp(AUTO_RANGE.0, AUTO_RANGE.1)
}

/// The player's Interface size setting.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterfaceScale {
    /// Follow the window: the default.
    pub auto: bool,
    /// 80 to 140; multiplies the automatic factor.
    pub percent: u8,
}

impl Default for InterfaceScale {
    fn default() -> Self {
        Self {
            auto: true,
            percent: 100,
        }
    }
}

impl InterfaceScale {
    /// The UI scale for a window of this logical size.
    pub fn factor(self, width: f32, height: f32) -> f32 {
        let own = f32::from(self.percent.clamp(PERCENT_RANGE.0, PERCENT_RANGE.1)) / 100.0;
        if self.auto {
            auto_factor(width, height) * own
        } else {
            own
        }
    }
}

/// How much room the layout has, from the effective width and height.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayoutClass {
    /// Below 1024 wide, or below 700 high.
    Compact,
    /// 1024 to 1279 wide.
    Standard,
    /// 1280 and more.
    Wide,
}

impl LayoutClass {
    pub fn of(effective_width: f32, effective_height: f32) -> Self {
        // Half a pixel of slack for sizes that are a rounded division.
        if effective_width < 1023.5 || effective_height < 699.5 {
            Self::Compact
        } else if effective_width < 1279.5 {
            Self::Standard
        } else {
            Self::Wide
        }
    }
}

/// The window in design pixels and the class it falls in. Recomputed when
/// the window or the setting changes.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub class: LayoutClass,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            width: DESIGN_SIZE.0,
            height: DESIGN_SIZE.1,
            scale: 1.0,
            class: LayoutClass::Wide,
        }
    }
}

impl Layout {
    pub fn new(window_width: f32, window_height: f32, setting: InterfaceScale) -> Self {
        let scale = setting.factor(window_width, window_height).max(0.1);
        let (width, height) = (window_width / scale, window_height / scale);
        Self {
            width,
            height,
            scale,
            class: LayoutClass::of(width, height),
        }
    }
}

/// The current layout; the design frame where no window exists (tests).
pub fn layout(world: &World) -> Layout {
    world.get_resource::<Layout>().copied().unwrap_or_default()
}

pub(super) fn apply(
    setting: Option<Res<InterfaceScale>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut scale: ResMut<UiScale>,
    mut layout: ResMut<Layout>,
) {
    let Some(window) = windows.iter().next() else {
        return;
    };
    // Without the host's setting (developer launches) the scale is left as
    // it is; the layout still follows the window.
    let next = match setting {
        Some(setting) => Layout::new(window.width(), window.height(), *setting),
        None => {
            let current = scale.0.max(0.1);
            let (width, height) = (window.width() / current, window.height() / current);
            Layout {
                width,
                height,
                scale: current,
                class: LayoutClass::of(width, height),
            }
        }
    };
    if *layout != next {
        *layout = next;
    }
    if scale.0 != next.scale {
        scale.0 = next.scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_automatic_factor_matches_the_spec_table() {
        for ((width, height), expected) in [
            ((1280.0, 720.0), 1.0),
            ((1024.0, 768.0), 1.0),
            ((1920.0, 1080.0), 1.5),
            ((2560.0, 1440.0), 2.0),
            ((3840.0, 2160.0), 2.5),
            ((2560.0, 1080.0), 1.5),
            ((1600.0, 900.0), 1.25),
            ((1366.0, 768.0), 1.05),
            ((800.0, 600.0), 1.0),
            ((7680.0, 4320.0), 2.5),
        ] {
            assert_eq!(auto_factor(width, height), expected, "{width} x {height}");
        }
        // Rounded down to a twentieth, never up.
        assert_eq!(auto_factor(1407.0, 900.0), 1.05);
        assert_eq!(auto_factor(1408.0, 900.0), 1.1);
    }

    #[test]
    fn the_setting_multiplies_the_automatic_factor_or_stands_alone() {
        let auto = InterfaceScale::default();
        assert!(auto.auto && auto.percent == 100, "Auto is the default");
        assert_eq!(auto.factor(1920.0, 1080.0), 1.5);
        let larger = InterfaceScale {
            auto: true,
            percent: 140,
        };
        assert!((larger.factor(1280.0, 720.0) - 1.4).abs() < 1e-6);
        assert!((larger.factor(1920.0, 1080.0) - 2.1).abs() < 1e-6);
        let fixed = InterfaceScale {
            auto: false,
            percent: 80,
        };
        assert_eq!(fixed.factor(3840.0, 2160.0), 0.8);
        // A value outside the range is held in it.
        let odd = InterfaceScale {
            auto: false,
            percent: 250,
        };
        assert_eq!(odd.factor(1280.0, 720.0), 1.4);
    }

    #[test]
    fn body_text_reaches_eighteen_pixels_at_1080p() {
        let scale = InterfaceScale::default().factor(1920.0, 1080.0);
        assert!(crate::game_ui::tokens::BODY.size * scale >= 18.0);
    }

    #[test]
    fn the_layout_class_follows_the_effective_size() {
        let auto = InterfaceScale::default();
        let class = |width, height, setting| Layout::new(width, height, setting).class;
        assert_eq!(class(1280.0, 720.0, auto), LayoutClass::Wide);
        assert_eq!(class(1024.0, 768.0, auto), LayoutClass::Standard);
        assert_eq!(class(1920.0, 1080.0, auto), LayoutClass::Wide);
        assert_eq!(class(2560.0, 1440.0, auto), LayoutClass::Wide);
        assert_eq!(class(1279.0, 720.0, auto), LayoutClass::Standard);
        assert_eq!(class(1000.0, 768.0, auto), LayoutClass::Compact);
        assert_eq!(class(1280.0, 690.0, auto), LayoutClass::Compact);
        // 140% on a 1280 x 720 window: effective 914 x 514.
        let large = InterfaceScale {
            auto: true,
            percent: 140,
        };
        let layout = Layout::new(1280.0, 720.0, large);
        assert_eq!(layout.class, LayoutClass::Compact);
        assert_eq!(
            (layout.width.round(), layout.height.round()),
            (914.0, 514.0)
        );
        // 2560 x 1440 is the design frame at twice the size.
        let big = Layout::new(2560.0, 1440.0, auto);
        assert_eq!((big.width, big.height, big.scale), (1280.0, 720.0, 2.0));
    }
}
