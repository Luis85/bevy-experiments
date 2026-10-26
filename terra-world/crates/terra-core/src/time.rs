use crate::GeoPosition;
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

/// UTC simulation clock. No operating-system timezone enters environmental
/// calculations. Negative time scale is supported for environmental scrubbing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldClock {
    pub unix_seconds: f64,
    pub time_scale: f64,
    pub paused: bool,
}

impl Default for WorldClock {
    fn default() -> Self {
        Self {
            unix_seconds: 1_782_043_200.0,
            time_scale: 60.0,
            paused: false,
        }
    }
}

impl WorldClock {
    /// Ignore invalid real-time deltas rather than poisoning all world state.
    /// Simulation schedulers are responsible for capping accumulated frame lag.
    pub fn advance(&mut self, real_seconds: f64) {
        if self.paused
            || !real_seconds.is_finite()
            || real_seconds < 0.0
            || !self.time_scale.is_finite()
            || !self.unix_seconds.is_finite()
        {
            return;
        }
        let next = self.unix_seconds + real_seconds * self.time_scale;
        if next.is_finite() {
            self.unix_seconds = next;
        }
    }

    pub fn utc_hour(&self) -> f64 {
        self.safe_seconds().rem_euclid(86_400.0) / 3_600.0
    }

    pub fn day_of_year(&self) -> u16 {
        let (year, month, day) = self.date();
        let month_days = [
            31_u16,
            28 + u16::from(is_leap_year(year)),
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        month_days[..month.saturating_sub(1) as usize]
            .iter()
            .sum::<u16>()
            + day as u16
    }

    /// Proleptic Gregorian UTC date. Supports negative Unix timestamps.
    pub fn date(&self) -> (i32, u8, u8) {
        // Bound extreme f64 input before integer conversion/arithmetic.
        let days = (self.safe_seconds() / 86_400.0)
            .floor()
            .clamp(-365_000_000.0, 365_000_000.0) as i64;
        civil_from_days(days)
    }

    pub fn days_in_year(&self) -> u16 {
        if is_leap_year(self.date().0) {
            366
        } else {
            365
        }
    }

    /// Human-readable UTC clock with a stable, locale-independent format.
    pub fn label(&self) -> String {
        let (year, month, day) = self.date();
        let seconds = self.safe_seconds().rem_euclid(86_400.0) as u32;
        format!(
            "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
            seconds / 3_600,
            seconds / 60 % 60
        )
    }

    fn safe_seconds(&self) -> f64 {
        if self.unix_seconds.is_finite() {
            self.unix_seconds
        } else {
            0.0
        }
    }
}

pub fn is_leap_year(year: i32) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

// Gregorian 400-year eras; astronomical year numbering (year 0 exists).
fn civil_from_days(days_since_epoch: i64) -> (i32, u8, u8) {
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year as i32, month as u8, day as u8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

/// Meteorological season, hemispheres reversed at the equator.
/// Tropical wet/dry seasons require regional climate data and are not inferred.
pub fn season(position: &GeoPosition, clock: &WorldClock) -> Season {
    let north = match clock.date().1 {
        3..=5 => Season::Spring,
        6..=8 => Season::Summer,
        9..=11 => Season::Autumn,
        _ => Season::Winter,
    };
    if position.latitude >= 0.0 {
        return north;
    }
    match north {
        Season::Spring => Season::Autumn,
        Season::Summer => Season::Winter,
        Season::Autumn => Season::Spring,
        Season::Winter => Season::Summer,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolarPosition {
    /// Geometric altitude above the horizon; atmospheric refraction excluded.
    pub elevation_deg: f64,
    /// Clockwise from geographic north, in [0,360).
    pub azimuth_deg: f64,
    /// Artistic light factor in [0,1], with civil twilight from -6 to +6 deg.
    pub daylight: f64,
}

impl SolarPosition {
    /// Direction toward the sun in the local east/up/south coordinate frame.
    pub fn direction_local(self) -> [f64; 3] {
        let elevation = self.elevation_deg.to_radians();
        let azimuth = self.azimuth_deg.to_radians();
        [
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
            -elevation.cos() * azimuth.cos(),
        ]
    }
}

/// NOAA fractional-year approximation for geometric solar direction.
///
/// Reference: <https://gml.noaa.gov/grad/solcalc/solareqns.PDF>.
/// This is an environment/lighting approximation, not an astronomical ephemeris.
pub fn solar_position(position: &GeoPosition, clock: &WorldClock) -> SolarPosition {
    let gamma = TAU / clock.days_in_year() as f64
        * (clock.day_of_year() as f64 - 1.0 + (clock.utc_hour() - 12.0) / 24.0);
    let eqtime = 229.18
        * (0.000_075 + 0.001_868 * gamma.cos()
            - 0.032_077 * gamma.sin()
            - 0.014_615 * (2.0 * gamma).cos()
            - 0.040_849 * (2.0 * gamma).sin());
    let declination = 0.006_918 - 0.399_912 * gamma.cos() + 0.070_257 * gamma.sin()
        - 0.006_758 * (2.0 * gamma).cos()
        + 0.000_907 * (2.0 * gamma).sin()
        - 0.002_697 * (3.0 * gamma).cos()
        + 0.001_480 * (3.0 * gamma).sin();
    let true_minutes =
        (clock.utc_hour() * 60.0 + eqtime + 4.0 * position.longitude).rem_euclid(1_440.0);
    let hour_angle = (true_minutes / 4.0 - 180.0).to_radians();
    let latitude = position.latitude.to_radians();
    let sin_alt =
        latitude.sin() * declination.sin() + latitude.cos() * declination.cos() * hour_angle.cos();
    let elevation_deg = sin_alt.clamp(-1.0, 1.0).asin().to_degrees();
    // atan2 form stays finite at the poles and avoids dividing by sin(zenith).
    let azimuth_deg = (hour_angle
        .sin()
        .atan2(hour_angle.cos() * latitude.sin() - declination.tan() * latitude.cos())
        .to_degrees()
        + 180.0)
        .rem_euclid(360.0);
    let twilight = ((elevation_deg + 6.0) / 12.0).clamp(0.0, 1.0);
    SolarPosition {
        elevation_deg,
        azimuth_deg,
        daylight: twilight * twilight * (3.0 - 2.0 * twilight),
    }
}
