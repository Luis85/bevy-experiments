//! The camera's detections ([`Detection`], plan Task 12, spec G10):
//! detections only, never images.
//!
//! A thing is detected when it is perceivable, within the camera's range
//! and cone ([`Camera`](crate::Camera), the integer field-of-view test of
//! vision) and in line of sight (the same grid line of sight as vision).
//! Each detection names the thing's class (its registered kind name, as a
//! detector's label would), its bearing (+ = left) and distance, an
//! estimated size and a confidence falling linearly from
//! [`CONFIDENCE_NEAR_PPM`] at 0 to [`CONFIDENCE_FAR_PPM`] at the camera's
//! range. A tracker gives each thing in view a track id that stays the same
//! while it stays in view (a real detector + tracker knows no world ids); a
//! thing that leaves the view loses it and gets a new one when it is back.
//!
//! Detection is automatic (no detection check, no dice): a camera reports
//! what is in view. The detections feed the memory as `camera`.

use bevy_ecs::prelude::*;
use npc_kernel::SimId;
use npc_kernel::spatial::{Footprint, GridBlocker, GridOccluder};
use serde::{Deserialize, Serialize};

use crate::readings::SensorReadings;

/// Confidence of a detection right at the camera, ppm.
pub const CONFIDENCE_NEAR_PPM: u32 = 990_000;
/// Confidence of a detection at the camera's range, ppm.
pub const CONFIDENCE_FAR_PPM: u32 = 600_000;
/// Size of a thing without a blocker or occluder footprint, mm.
pub const DEFAULT_SIZE_MM: i32 = 500;

/// One detection shaped like a ROS `vision_msgs` detection (mm,
/// millidegrees, ppm). Re-exported at the crate root as `CameraDetection`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detection {
    /// What it is (the kind name, e.g. `item`, `door`, `actor`).
    pub class: String,
    /// The tracker's id, stable while the thing stays in view.
    pub track_id: u32,
    /// Bearing from the facing, + = left, mdeg.
    pub bearing_mdeg: i32,
    pub distance_mm: i32,
    /// Its estimated width (the larger side of its blocker or occluder
    /// footprint, else [`DEFAULT_SIZE_MM`]).
    pub size_mm: i32,
    pub confidence_ppm: u32,
}

/// The confidence of a detection `dist_mm` away by a camera of `range_mm`.
pub fn confidence_ppm(dist_mm: i64, range_mm: i32) -> u32 {
    let range = i64::from(range_mm.max(1));
    let span = i64::from(CONFIDENCE_NEAR_PPM - CONFIDENCE_FAR_PPM);
    let c = i64::from(CONFIDENCE_NEAR_PPM) - span * dist_mm.clamp(0, range) / range;
    c as u32
}

/// The estimated width of `entity` (see [`Detection::size_mm`]).
pub fn size_mm(world: &World, entity: Entity) -> i32 {
    let width = |f: Footprint| f.half_x_mm.max(f.half_z_mm).max(0).saturating_mul(2);
    let w = world
        .get::<GridBlocker>(entity)
        .map(|b| width(b.0))
        .into_iter()
        .chain(world.get::<GridOccluder>(entity).map(|o| width(o.0)))
        .max()
        .unwrap_or(0);
    if w > 0 { w } else { DEFAULT_SIZE_MM }
}

/// One thing the camera sees this scan (gathered nearest first).
#[derive(Clone, Copy, Debug)]
pub(crate) struct CameraHit {
    pub id: SimId,
    pub class: &'static str,
    pub bearing_mdeg: i32,
    pub distance_mm: i32,
    pub size_mm: i32,
    pub confidence_ppm: u32,
}

/// Writes this scan's hits (nearest first) into `readings`: track ids from
/// the tracker (new ones for things that came into view), tracks of things
/// out of view dropped. Reuses the detections' buffers. `ids` is scratch.
pub(crate) fn publish(readings: &mut SensorReadings, hits: &[CameraHit], ids: &mut Vec<SimId>) {
    ids.clear();
    ids.extend(hits.iter().map(|h| h.id));
    ids.sort_unstable();
    readings
        .tracks
        .retain(|id, _| ids.binary_search(id).is_ok());
    readings.camera.truncate(hits.len());
    for (i, h) in hits.iter().enumerate() {
        let track_id = match readings.tracks.get(&h.id) {
            Some(&t) => t,
            None => {
                // The next id not held by a thing still in view (ids wrap
                // around after `u32::MAX`, skipping 0).
                let mut t = readings.next_track.max(1);
                while readings.tracks.values().any(|&v| v == t) {
                    t = t.checked_add(1).unwrap_or(1);
                }
                readings.next_track = t.checked_add(1).unwrap_or(1);
                readings.tracks.insert(h.id, t);
                t
            }
        };
        if i == readings.camera.len() {
            readings.camera.push(Detection::default());
        }
        let d = &mut readings.camera[i];
        d.class.clear();
        d.class.push_str(h.class);
        d.track_id = track_id;
        d.bearing_mdeg = h.bearing_mdeg;
        d.distance_mm = h.distance_mm;
        d.size_mm = h.size_mm;
        d.confidence_ppm = h.confidence_ppm;
    }
}
