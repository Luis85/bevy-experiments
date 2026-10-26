use crate::{GeoError, GeoPosition, MEAN_EARTH_RADIUS_M};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TravelMode {
    Ground,
    Water,
    Air,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Human,
    Animal,
    Creature,
    Vehicle,
    Ship,
    Helicopter,
    Balloon,
    Airplane,
}

impl ActorKind {
    pub const ALL: [Self; 8] = [
        Self::Human,
        Self::Animal,
        Self::Creature,
        Self::Vehicle,
        Self::Ship,
        Self::Helicopter,
        Self::Balloon,
        Self::Airplane,
    ];

    /// Default capability. A game may override this for swimmers, flying
    /// creatures, amphibious vehicles, grounded aircraft, etc.
    pub fn travel_mode(self) -> TravelMode {
        match self {
            Self::Ship => TravelMode::Water,
            Self::Helicopter | Self::Balloon | Self::Airplane => TravelMode::Air,
            _ => TravelMode::Ground,
        }
    }

    /// Generous sample-game validation limits, not physical performance data.
    /// Hosting games should provide their own per-archetype movement rules.
    pub fn speed_limit_m_s(self) -> f64 {
        match self {
            Self::Human => 15.0,
            Self::Animal => 50.0,
            Self::Creature => 100.0,
            Self::Vehicle => 100.0,
            Self::Ship => 40.0,
            Self::Helicopter => 120.0,
            Self::Balloon => 50.0,
            Self::Airplane => 400.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorState {
    pub id: u64,
    pub kind: ActorKind,
    pub position: GeoPosition,
    /// Physical velocity axes are east/north/up (not renderer east/up/south).
    pub velocity_enu_m_s: [f64; 3],
    pub heading_deg: f32,
}

impl ActorState {
    pub fn validate(&self) -> Result<(), GeoError> {
        self.position.validate()?;
        if self.velocity_enu_m_s.iter().any(|x| !x.is_finite()) || !self.heading_deg.is_finite() {
            return Err(GeoError::NonFinite);
        }
        Ok(())
    }

    pub fn speed_m_s(&self) -> f64 {
        self.velocity_enu_m_s[0]
            .hypot(self.velocity_enu_m_s[1])
            .hypot(self.velocity_enu_m_s[2])
    }

    /// Kinematic movement on a mean-radius sphere, retaining explicit altitude.
    ///
    /// Handles antimeridian/pole crossings without longitude division. This is
    /// a reusable movement fallback, not flight, rigid-body or hydrodynamic
    /// simulation. Terrain, road, water and collision constraints are applied
    /// by the hosting game's environment/physics systems after this step.
    pub fn advance(&mut self, dt: f64) -> Result<(), GeoError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(GeoError::TimeStep);
        }
        self.validate()?;
        let [east, north, up] = self.velocity_enu_m_s;
        let speed = east.hypot(north);
        let distance = speed * dt;
        let altitude = self.position.altitude_m + up * dt;
        if !distance.is_finite() || !altitude.is_finite() {
            return Err(GeoError::NonFinite);
        }
        if speed <= f64::EPSILON || dt == 0.0 {
            self.position.altitude_m = altitude;
            return Ok(());
        }
        let bearing = east.atan2(north);
        let angle = (distance / MEAN_EARTH_RADIUS_M).rem_euclid(std::f64::consts::TAU);
        let latitude = self.position.latitude.to_radians();
        let longitude = self.position.longitude.to_radians();
        let latitude2 = (latitude.sin() * angle.cos()
            + latitude.cos() * angle.sin() * bearing.cos())
        .clamp(-1.0, 1.0)
        .asin();
        let longitude2 = longitude
            + (bearing.sin() * angle.sin() * latitude.cos())
                .atan2(angle.cos() - latitude.sin() * latitude2.sin());
        self.position =
            GeoPosition::new(latitude2.to_degrees(), longitude2.to_degrees(), altitude)?;
        self.heading_deg = bearing.to_degrees().rem_euclid(360.0) as f32;
        Ok(())
    }
}
