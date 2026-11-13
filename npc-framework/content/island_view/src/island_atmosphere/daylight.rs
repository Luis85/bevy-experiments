//! The regional day curve shared by the sun, lamps and ambience. Pure view
//! math over the sim clock; nothing here flows back into the simulation.
use bevy::prelude::Resource;
use npc_framework::prelude::SimConfig;

/// Capture and review aid: a fixed day phase (`0.0..1.0`) for the island's
/// sun, sky, lamps and smoke shade. It is read by presentation only; the
/// simulation clock, calendar, schedules and HUD keep the real time.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ViewDayOverride(pub Option<f32>);

impl ViewDayOverride {
    /// `ISLAND_VIEW_DAY_PHASE=0.35` style values; anything else is ignored.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        Self(
            value
                .and_then(|value| value.trim().parse::<f32>().ok())
                .filter(|phase| phase.is_finite() && (0.0..=1.0).contains(phase))
                .map(|phase| phase.min(0.9999)),
        )
    }
}

/// Position inside the current day, `0.0..1.0`.
pub(crate) fn day_phase(tick: u64, config: &SimConfig) -> f32 {
    let length = config.day_length.max(1);
    (tick % length) as f32 / length as f32
}

/// The phase the view shows: the real one unless a capture pinned it.
pub(crate) fn view_phase(tick: u64, config: &SimConfig, pinned: Option<&ViewDayOverride>) -> f32 {
    pinned
        .and_then(|pinned| pinned.0)
        .unwrap_or_else(|| day_phase(tick, config))
}

/// `1.0` at noon, `0.25` at sunrise and sunset, `0.0` at midnight.
pub(crate) fn daylight(phase: f32, night_permille: u32) -> f32 {
    let day_end = (1.0 - night_permille.min(1000) as f32 / 1000.0).clamp(0.01, 0.99);
    if phase < day_end {
        0.25 + 0.75 * (phase / day_end * std::f32::consts::PI).sin().max(0.0)
    } else {
        0.125 * (1.0 + ((phase - day_end) / (1.0 - day_end) * std::f32::consts::TAU).cos())
    }
}

/// Lamp and window strength along the dusk curve: dark in full daylight,
/// rising through the last light and full once the sun is down.
pub(crate) fn lamp_level(daylight: f32) -> f32 {
    const OFF: f32 = 0.48;
    const FULL: f32 = 0.22;
    let t = ((OFF - daylight) / (OFF - FULL)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn lamp_level_at(tick: u64, config: &SimConfig) -> f32 {
    lamp_level(daylight(day_phase(tick, config), config.night_permille))
}

/// Daylight (`0..1`) as the view shows it, honouring a pinned phase.
pub(crate) fn view_daylight(
    tick: u64,
    config: &SimConfig,
    pinned: Option<&ViewDayOverride>,
) -> f32 {
    daylight(view_phase(tick, config, pinned), config.night_permille)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daylight_is_continuous_across_sunset_and_midnight() {
        let mut previous = daylight(0.0, 300);
        for step in 1..=2000 {
            let now = daylight(step as f32 / 2000.0, 300);
            assert!((now - previous).abs() < 0.01, "jump at {step}");
            previous = now;
        }
        assert!((daylight(0.35, 300) - 1.0).abs() < 0.001);
        assert!(daylight(0.85, 300) < 0.001);
    }

    #[test]
    fn a_pinned_phase_replaces_the_clock_for_the_view_only() {
        let config = SimConfig::default();
        assert_eq!(ViewDayOverride::parse(None), ViewDayOverride(None));
        assert_eq!(ViewDayOverride::parse(Some("noon")), ViewDayOverride(None));
        assert_eq!(ViewDayOverride::parse(Some("1.5")), ViewDayOverride(None));
        let noon = ViewDayOverride::parse(Some(" 0.35 "));
        assert_eq!(noon, ViewDayOverride(Some(0.35)));
        assert_eq!(view_phase(0, &config, None), 0.0);
        assert_eq!(view_phase(0, &config, Some(&ViewDayOverride(None))), 0.0);
        assert_eq!(view_phase(0, &config, Some(&noon)), 0.35);
        assert!((view_daylight(0, &config, Some(&noon)) - 1.0).abs() < 0.001);
        assert!((view_daylight(0, &config, None) - 0.25).abs() < 0.001);
    }

    #[test]
    fn lamps_fade_instead_of_switching() {
        assert_eq!(lamp_level(1.0), 0.0);
        assert_eq!(lamp_level(0.5), 0.0);
        assert_eq!(lamp_level(0.2), 1.0);
        assert_eq!(lamp_level(0.0), 1.0);
        let mid = lamp_level(0.35);
        assert!(mid > 0.2 && mid < 0.8, "{mid}");
        // No single sim tick moves a lamp by more than a few percent.
        let config = SimConfig::default();
        let mut previous = lamp_level_at(0, &config);
        let mut seen_partial = false;
        for tick in 1..config.day_length {
            let level = lamp_level_at(tick, &config);
            assert!((level - previous).abs() < 0.05, "hard switch at {tick}");
            seen_partial |= level > 0.05 && level < 0.95;
            previous = level;
        }
        assert!(seen_partial);
        assert!(lamp_level_at(config.day_length - 1, &config) > 0.0);
    }
}
