//! The radar's tracks ([`RadarTrack`], plan Task 12, spec G10).
//!
//! A thing is tracked when it is perceivable, within the radar's range and
//! (wide) cone and in line of sight: walls block it. Each track gives the
//! bearing (+ = left) and range rounded to the radar's resolution, the
//! radial velocity (mm per tick, + = moving away: the rate at which the
//! range changed since the radar's previous scan, so the radar's own motion
//! counts, as a Doppler radar measures it; 0 for a thing it did not see
//! then) and an amplitude, higher for a thing that moved since that scan
//! ([`AMPLITUDE_MOVING_PPM`] vs [`AMPLITUDE_STILL_PPM`], falling to half at
//! the radar's range).
//!
//! Detection is automatic (no detection check, no dice). The tracks feed
//! the memory as `radar`.
//!
//! (`PrevSimPos` cannot give the velocity here: the perception pass runs in
//! `NpcSet::Sense`, after `NpcSet::Inputs` copied `SimPos` into it and
//! before anything moved, so the two are equal. The radar keeps its
//! previous echoes in [`SensorReadings::radar_prev`] instead.)

use npc_kernel::{SimId, SimPos};
use serde::{Deserialize, Serialize};

use crate::readings::{SensorReadings, quantize};
use crate::sensors::Radar;

/// Amplitude of a thing that moved since the previous scan, at range 0, ppm.
pub const AMPLITUDE_MOVING_PPM: u32 = 900_000;
/// Amplitude of a thing that did not move, at range 0, ppm.
pub const AMPLITUDE_STILL_PPM: u32 = 400_000;

/// One radar track shaped like a ROS `radar_msgs/RadarTrack` (mm,
/// millidegrees, mm per tick, ppm).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadarTrack {
    /// Bearing from the facing, + = left, a multiple of the bearing
    /// resolution.
    pub bearing_mdeg: i32,
    /// Range, a multiple of the range resolution.
    pub range_mm: i32,
    /// + = moving away.
    pub radial_velocity_mm_per_tick: i32,
    pub amplitude_ppm: u32,
}

/// A thing as the radar saw it in its previous scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadarEcho {
    pub pos: SimPos,
    pub range_mm: i32,
}

/// One thing the radar sees this scan (gathered nearest first).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RadarHit {
    pub id: SimId,
    pub pos: SimPos,
    pub range_mm: i32,
    pub bearing_mdeg: i32,
}

/// The amplitude of a target `range_mm` away (moving or not) for a radar
/// of `max_mm`.
pub fn amplitude_ppm(moving: bool, range_mm: i32, max_mm: i32) -> u32 {
    let base = i64::from(if moving {
        AMPLITUDE_MOVING_PPM
    } else {
        AMPLITUDE_STILL_PPM
    });
    let max = i64::from(max_mm.max(1));
    let r = i64::from(range_mm).clamp(0, max);
    (base - base * r / (2 * max)) as u32
}

/// Writes this scan's hits (nearest first) into `readings` and remembers
/// them as the previous echoes. `ids` is scratch.
pub(crate) fn publish(
    readings: &mut SensorReadings,
    radar: &Radar,
    hits: &[RadarHit],
    tick: u64,
    ids: &mut Vec<SimId>,
) {
    let dt = tick.saturating_sub(readings.radar_tick);
    readings.radar.clear();
    for h in hits {
        let prev = readings.radar_prev.get(&h.id).filter(|_| dt > 0);
        let (velocity, moving) = prev.map_or((0, false), |p| {
            let v = (i64::from(h.range_mm) - i64::from(p.range_mm)) / dt as i64;
            (v as i32, p.pos != h.pos)
        });
        readings.radar.push(RadarTrack {
            bearing_mdeg: quantize(h.bearing_mdeg, radar.bearing_res_mdeg),
            range_mm: quantize(h.range_mm, radar.range_res_mm),
            radial_velocity_mm_per_tick: velocity,
            amplitude_ppm: amplitude_ppm(moving, h.range_mm, radar.range_mm),
        });
    }
    ids.clear();
    ids.extend(hits.iter().map(|h| h.id));
    ids.sort_unstable();
    readings
        .radar_prev
        .retain(|id, _| ids.binary_search(id).is_ok());
    for h in hits {
        readings.radar_prev.insert(
            h.id,
            RadarEcho {
                pos: h.pos,
                range_mm: h.range_mm,
            },
        );
    }
    readings.radar_tick = tick;
}
