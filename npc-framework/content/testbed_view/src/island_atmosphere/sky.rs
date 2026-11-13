//! Sun, sky light and haze for the regional island as pure functions of the
//! day curve. A clear day is bright and warm, the hours around sunrise and
//! sunset are golden, and the night is blue and dim but still readable.
use bevy::prelude::*;

/// Everything the island's lights and haze need for one moment of the day.
/// Colours are sRGB triples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SkyLight {
    pub sun_lux: f32,
    pub sun_color: [f32; 3],
    pub ambient: f32,
    pub ambient_color: [f32; 3],
    pub sky: [f32; 3],
    pub fog: [f32; 3],
    pub fog_extinction: [f32; 3],
    pub fog_inscatter: [f32; 3],
}

const NOON_LUX: f32 = 14_000.0;
/// Moonlight and the night sky. The first regional curve set these to 1,500
/// and 340, below the older island night (2,300 and 420): away from a lamp
/// the ground was nearly black. These are that older level, a little over.
const MOON_LUX: f32 = 2_300.0;
const NOON_AMBIENT: f32 = 1_000.0;
const NIGHT_AMBIENT: f32 = 450.0;

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn smoothstep(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of the noon light is there. The day curve is 0.25 at sunrise;
/// a linear mapping left the first and last hours of "daylight" at a quarter
/// of noon, which read as dusk. This rises quickly: 0.5 at sunrise, 0.8 an
/// hour in, 1.0 at noon and 0.0 at midnight.
pub(crate) fn sun_strength(daylight: f32) -> f32 {
    1.0 - (1.0 - daylight.clamp(0.0, 1.0)).powf(2.4)
}

/// Peaks just after sunrise and before sunset, zero at noon and at night.
pub(crate) fn golden_hour(daylight: f32) -> f32 {
    (1.0 - (daylight - 0.3).abs() / 0.3).clamp(0.0, 1.0)
}

pub(crate) fn sky_light(daylight: f32) -> SkyLight {
    let strength = sun_strength(daylight);
    let golden = golden_hour(daylight);
    let sun_base = mix(
        [0.60, 0.72, 1.0],
        [1.0, 0.95, 0.85],
        smoothstep(0.05, 0.3, daylight),
    );
    let tint = |color: [f32; 3], warm: [f32; 3], amount: f32| mix(color, warm, golden * amount);
    SkyLight {
        sun_lux: MOON_LUX + (NOON_LUX - MOON_LUX) * strength,
        sun_color: tint(sun_base, [1.0, 0.74, 0.50], 0.6),
        ambient: NIGHT_AMBIENT + (NOON_AMBIENT - NIGHT_AMBIENT) * strength,
        ambient_color: tint(
            mix([0.58, 0.69, 1.0], [0.92, 0.95, 1.0], strength),
            [1.0, 0.88, 0.80],
            0.25,
        ),
        sky: tint(
            mix([0.045, 0.065, 0.13], [0.52, 0.74, 0.93], strength),
            [0.95, 0.66, 0.48],
            0.32,
        ),
        fog: tint(
            mix([0.09, 0.12, 0.20], [0.62, 0.74, 0.80], strength),
            [0.92, 0.70, 0.55],
            0.3,
        ),
        fog_extinction: mix([0.15, 0.20, 0.31], [0.40, 0.54, 0.62], strength),
        fog_inscatter: tint(
            mix([0.18, 0.24, 0.36], [0.74, 0.83, 0.88], strength),
            [0.95, 0.74, 0.58],
            0.3,
        ),
    }
}

/// The sun circles the island once a day and climbs with the day curve; at
/// night the same light stands in for the moon.
pub(crate) fn sun_position(phase: f32, daylight: f32) -> Vec3 {
    let angle = phase * std::f32::consts::TAU;
    Vec3::new(
        angle.sin() * 40.0,
        12.0 + daylight * 30.0,
        angle.cos() * 25.0,
    )
}

/// Haze is tied to the zoom so the settlement under the camera stays crisp
/// (about nine tenths of its own colour) and only the far distance pales.
/// The earlier 3.5 x distance, 210 m minimum, washed half the colour out of
/// the ground at settlement zoom.
pub(crate) fn fog_visibility(camera_distance: f32) -> f32 {
    (camera_distance * 14.0).max(900.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::island_atmosphere::daylight::daylight;

    fn luminance(color: [f32; 3]) -> f32 {
        0.2126 * color[0] + 0.7152 * color[1] + 0.0722 * color[2]
    }

    #[test]
    fn the_first_hour_of_day_is_already_well_lit() {
        let sunrise = sky_light(daylight(0.0, 300));
        let noon = sky_light(daylight(0.35, 300));
        let midnight = sky_light(daylight(0.85, 300));
        assert!(sunrise.sun_lux > noon.sun_lux * 0.5, "{sunrise:?}");
        assert!(sunrise.ambient > noon.ambient * 0.55);
        assert!(noon.sun_lux > 13_000.0 && noon.ambient > 900.0);
        // A tenth of the way into the day the light is within a quarter of noon.
        let morning = sky_light(daylight(0.07, 300));
        assert!(morning.sun_lux > noon.sun_lux * 0.78, "{morning:?}");
        // Night stays readable without competing with the lamps: at least
        // the older island night, and still under a fifth of the noon sun
        // and half of the noon sky.
        assert!(midnight.sun_lux >= 2_300.0 && midnight.sun_lux < noon.sun_lux * 0.2);
        assert!(midnight.ambient >= 420.0 && midnight.ambient < noon.ambient * 0.5);
        // Sunrise is still more than three times the night.
        assert!(sunrise.sun_lux > midnight.sun_lux * 3.0);
    }

    #[test]
    fn noon_is_warm_dusk_is_golden_and_night_is_blue() {
        let noon = sky_light(1.0);
        assert!(noon.sun_color[0] > noon.sun_color[2]);
        assert!(noon.sun_color[2] > 0.8, "noon stays close to white");
        let dusk = sky_light(0.25);
        assert!(dusk.sun_color[0] - dusk.sun_color[2] > 0.25, "{dusk:?}");
        assert!(dusk.sky[0] > dusk.sky[2] * 0.8, "a warm horizon at dusk");
        let night = sky_light(0.0);
        assert!(night.sun_color[2] > night.sun_color[0] + 0.3);
        assert!(night.ambient_color[2] > night.ambient_color[0] + 0.3);
        assert!(luminance(night.sky) < 0.1);
        assert_eq!(golden_hour(1.0), 0.0);
        assert_eq!(golden_hour(0.0), 0.0);
    }

    #[test]
    fn light_changes_smoothly_through_the_whole_day() {
        let mut previous = sky_light(daylight(0.0, 300));
        for step in 1..=2400 {
            let now = sky_light(daylight(step as f32 / 2400.0, 300));
            assert!(
                (now.sun_lux - previous.sun_lux).abs() < 120.0,
                "sun jumps at {step}"
            );
            for channel in 0..3 {
                assert!((now.sun_color[channel] - previous.sun_color[channel]).abs() < 0.02);
                assert!((now.sky[channel] - previous.sky[channel]).abs() < 0.02);
            }
            previous = now;
        }
    }

    #[test]
    fn haze_keeps_the_focused_settlement_crisp_at_every_zoom() {
        for distance in [8.0_f32, 30.0, 62.0, 150.0, 330.0, 600.0] {
            let visibility = fog_visibility(distance);
            // Bevy derives extinction from a 5 % contrast threshold; the
            // extinction colour only lowers it further.
            let transmittance = (-2.996 * distance / visibility).exp();
            assert!(transmittance > 0.8, "{distance} m keeps {transmittance}");
        }
        assert!(fog_visibility(62.0) >= 868.0);
    }

    #[test]
    fn the_sun_stays_above_the_horizon_and_moves_through_the_day() {
        let dawn = sun_position(0.0, daylight(0.0, 300));
        let noon = sun_position(0.35, daylight(0.35, 300));
        assert!(dawn.y > 10.0 && noon.y > dawn.y + 15.0);
        assert!(dawn.distance(noon) > 30.0);
    }
}
