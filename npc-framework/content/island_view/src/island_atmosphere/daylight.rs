//! The regional day curve shared by the sun, lamps and ambience. Pure view
//! math over the sim clock; nothing here flows back into the simulation.
use bevy::prelude::{Res, ResMut, Resource};
use island_content::village::Season;
use npc_framework::prelude::SimConfig;

/// The view's day: a fixed day phase (`0.0..1.0`) for the island's sun, sky,
/// lamps and smoke shade when a capture pins one, and the season the sun's
/// path follows (the season the land is drawn in). Read by presentation
/// only; the simulation clock, calendar, schedules and HUD keep the real
/// time.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ViewDayOverride(pub Option<f32>, pub Option<Season>);

impl ViewDayOverride {
    /// `ISLAND_VIEW_DAY_PHASE=0.35` style values; anything else is ignored.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        Self(
            value
                .and_then(|value| value.trim().parse::<f32>().ok())
                .filter(|phase| phase.is_finite() && (0.0..=1.0).contains(phase))
                .map(|phase| phase.min(0.9999)),
            None,
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

/// One hour of the day, in day phase.
const HOUR: f32 = 1.0 / 24.0;

/// Sunrise and sunset the view draws, in day phase (`0` is the sim's
/// sunrise, 06:00 on the clock; `day_end` its sunset, 22:48), and how high
/// the noon sun climbs (`1` full). The sim's day and its schedules are
/// untouched; only the light follows the season:
/// - spring and autumn keep the sim's day, 06:00 to 22:48;
/// - summer is longer, 05:00 to 23:48;
/// - winter is short and low, 08:00 to 20:00 with a lower noon sun.
pub(crate) fn sun_window(season: Option<Season>, night_permille: u32) -> (f32, f32, f32) {
    let day_end = (1.0 - night_permille.min(1000) as f32 / 1000.0).clamp(0.01, 0.99);
    match season {
        Some(Season::Summer) => (-HOUR, (day_end + HOUR).min(0.99), 1.0),
        Some(Season::Winter) => (2.0 * HOUR, (day_end - 2.8 * HOUR).max(3.0 * HOUR), 0.82),
        _ => (0.0, day_end, 1.0),
    }
}

/// `1.0` at noon, `0.25` at sunrise and sunset, `0.0` at midnight.
pub(crate) fn daylight(phase: f32, night_permille: u32) -> f32 {
    seasonal_daylight(phase, night_permille, None)
}

/// [`daylight`] on the season's own sunrise and sunset ([`sun_window`]):
/// the peak is the season's noon sun.
pub(crate) fn seasonal_daylight(phase: f32, night_permille: u32, season: Option<Season>) -> f32 {
    let (rise, set, peak) = sun_window(season, night_permille);
    let length = set - rise;
    let into = (phase - rise).rem_euclid(1.0);
    if into < length {
        0.25 + 0.75 * peak * (into / length * std::f32::consts::PI).sin().max(0.0)
    } else if season == Some(Season::Winter) {
        // A long winter night: a twilight after sunset and before sunrise,
        // full night between. The sim's night curve, stretched over twelve
        // hours, kept the evening near sunset light for hours.
        let since = into - length;
        let until = 1.0 - into;
        let twilight = since.min(until) / WINTER_TWILIGHT;
        if twilight >= 1.0 {
            0.0
        } else {
            0.125 * (1.0 + (twilight * std::f32::consts::PI).cos())
        }
    } else {
        0.125 * (1.0 + ((into - length) / (1.0 - length) * std::f32::consts::TAU).cos())
    }
}

/// Twilight after a winter sunset and before its sunrise, in day phase
/// (an hour and a half).
const WINTER_TWILIGHT: f32 = 1.5 * HOUR;

/// Keeps the view's season with the season the land is drawn in.
pub(crate) fn follow_season(
    drawn: Option<Res<crate::island_world::ambient::DrawnSeason>>,
    mut day: ResMut<ViewDayOverride>,
) {
    let season = drawn.map(|drawn| drawn.0);
    if day.1 != season {
        day.1 = season;
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
    seasonal_daylight(
        view_phase(tick, config, pinned),
        config.night_permille,
        pinned.and_then(|pinned| pinned.1),
    )
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
        assert_eq!(ViewDayOverride::parse(None), ViewDayOverride(None, None));
        assert_eq!(
            ViewDayOverride::parse(Some("noon")),
            ViewDayOverride(None, None)
        );
        assert_eq!(
            ViewDayOverride::parse(Some("1.5")),
            ViewDayOverride(None, None)
        );
        let noon = ViewDayOverride::parse(Some(" 0.35 "));
        assert_eq!(noon, ViewDayOverride(Some(0.35), None));
        assert_eq!(view_phase(0, &config, None), 0.0);
        assert_eq!(
            view_phase(0, &config, Some(&ViewDayOverride(None, None))),
            0.0
        );
        assert_eq!(view_phase(0, &config, Some(&noon)), 0.35);
        assert!((view_daylight(0, &config, Some(&noon)) - 1.0).abs() < 0.001);
        assert!((view_daylight(0, &config, None) - 0.25).abs() < 0.001);
    }

    /// Winter days are short and low, summer days long; spring and autumn
    /// keep the sim's day. The light stays continuous through each day.
    #[test]
    fn the_sun_keeps_the_seasons_hours() {
        let at = |hour: f32, season| seasonal_daylight((hour - 6.0) / 24.0, 300, Some(season));
        // The sim's day for spring and autumn, exactly as before.
        for step in 0..=100 {
            let phase = step as f32 / 100.0 * 0.999;
            assert_eq!(
                seasonal_daylight(phase, 300, Some(Season::Autumn)),
                daylight(phase, 300)
            );
        }
        // Sunrise (0.25) and sunset per season, on the clock.
        assert!((at(8.0, Season::Winter) - 0.25).abs() < 0.001);
        assert!((at(20.0, Season::Winter) - 0.25).abs() < 0.001);
        assert!((at(5.0, Season::Summer) - 0.25).abs() < 0.001);
        // At 07:00 a winter morning is still night; a summer one is bright.
        assert!(at(7.0, Season::Winter) < 0.25);
        assert!(at(7.0, Season::Summer) > at(7.0, Season::Autumn));
        // At 21:00 winter lamps are lit, an autumn evening still has light;
        // by 21:30 and until 06:30 a winter night is full night.
        assert!(lamp_level(at(21.0, Season::Winter)) > 0.95);
        for hour in [21.5, 23.0, 2.0, 6.5] {
            assert!(at(hour, Season::Winter) < 0.001, "{hour}");
        }
        assert!(lamp_level(at(21.0, Season::Autumn)) < 0.5);
        // A lower noon sun in winter.
        assert!(at(14.0, Season::Winter) < 0.9 && at(14.4, Season::Summer) > 0.99);
        for season in [
            Season::Spring,
            Season::Summer,
            Season::Autumn,
            Season::Winter,
        ] {
            let mut previous = seasonal_daylight(0.0, 300, Some(season));
            for step in 1..=2000 {
                let now = seasonal_daylight(step as f32 / 2000.0, 300, Some(season));
                assert!((now - previous).abs() < 0.01, "{season:?} jumps at {step}");
                previous = now;
            }
        }
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
