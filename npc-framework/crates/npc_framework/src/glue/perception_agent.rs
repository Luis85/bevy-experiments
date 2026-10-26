//! Perception × agent-io (features `perception` + `agent-io`): observations
//! from the unit's point of view (agent-control spec §2.3, anti-goal §4).
//!
//! [`PerceptionAgentPlugin`] registers the **perception provider**
//! [`PERCEPTION_SECTION`] (`"perception"`) with `npc_agent_io`. With it,
//! `agent/observe` sets `omniscient: false`, and the section lists only what
//! the unit's `PerceptionMemory` holds:
//!
//! ```text
//! "perception": {
//!   "sensors": true,
//!   "sensed":     [{"id":7,"kind":"item","name":"store_key #7","kinds":1,"pos":[x,z],"dist_mm":4200,"via":"vision","confidence":1000,"last_seen_tick":120}],
//!   "remembered": [{"id":9,"kind":"actor","name":"actor #9","kinds":2,"pos":[x,z],"dist_mm":..,"via":"hearing","last_seen_tick":80,"age_ticks":40,"confidence":800}],
//!   "last_noise": {"tick":..,"pos":[x,z],"source":3|null,"kind":1} | null,
//!   "reach_mm": 1500 | null, "within_reach": [54, 15],
//!   "night": false, "vision_reduced_at_night": false,
//!   "searched": {"cell_mm":10000, "radius_mm":60000, "within_radius_pct":34,
//!                "map_pct":12, "sectors":[{"dir":"N","explored":3,"total":9}, ..8]}
//! }
//! ```
//!
//! Text: `You perceive: store_key #26 at (7.50 m, -1.50 m), 12.30 m away
//! (vision); ..` (the [`PERCEIVE_TEXT_LIMIT`] nearest),
//! `Within reach: chest #54 (behind you), forge #15.` (only with a
//! `ReachSense`: the sensed things within its radius, nearest first; one
//! sensed by reach alone is out of view: `behind you` or `beside you`), and
//! `You remember: .. last seen at (x m, z m), N ticks ago` (the
//! [`REMEMBER_TEXT_LIMIT`] most recently seen). Both lists also show every
//! vehicle and unit (a `Brain`, `Commandable` or `Vehicle`) whatever the
//! cap, and end with `; and N more (40 tree, 12 item)` by kind name; the
//! JSON lists everything. (Proposal 009: the remembered line used to stop after the eight
//! lowest `SimId`s, so the last-spawned things, vehicles and units, fell into
//! "and N more" whenever more was remembered.)
//! `name` comes from the registered describers (`npc_agent_io::observe::
//! describe`: the item kind, a door's state, ..). What the unit holds or
//! rides (its possessions) is left out of both lists. At night a line says
//! that vision is not reduced (the perception model has no light level).
//!
//! It also registers the **known provider** `perception`
//! ([`perceived_or_remembered`]): with it, `npc_agent_io` offers only
//! affordances whose target the unit perceives or remembers (or holds or
//! rides): an item inside a closed store is not offered until seen, and a
//! target forgotten (`Lost {forgotten: true}`) disappears from the list.
//! Imperceptible things (no `Perceivable`, e.g. a yield spawned at runtime)
//! within [`TOUCH_MM`] count as known by touch. A unit without `Sensors` has
//! no point of view: it is not gated (`omniscient: true`).
//!
//! Positions are where the unit last perceived a thing (never its live
//! position), `dist_mm` is from the unit's current position to that point,
//! and `confidence` of remembered percepts decays with age
//! (`Percept::confidence_at`). A unit without `Sensors` perceives nothing.
//! `world/state` (debug tooling) still shows everything.
//!
//! **Robot sensors** (plan Task 12, spec G10): a unit with a LIDAR, camera
//! or radar also gets the perception sections `lidar`, `camera` and `radar`
//! (absent without that sensor). JSON is the sensor's ROS-shaped readings
//! (`npc_perception::SensorReadings`):
//!
//! ```text
//! "lidar":  {"angle_min_mdeg":-180000,"angle_increment_mdeg":1000,"range_min_mm":50,
//!            "range_max_mm":20000,"ranges_mm":[4500, .., 20001],"tick":120}
//! "camera": {"tick":120,"detections":[{"class":"item","track_id":3,"bearing_mdeg":12000,
//!            "distance_mm":5000,"size_mm":500,"confidence_ppm":860000}]}
//! "radar":  {"tick":120,"tracks":[{"bearing_mdeg":-6000,"range_mm":7500,
//!            "radial_velocity_mm_per_tick":-48,"amplitude_ppm":820000}]}
//! ```
//!
//! Bearings are relative to the facing, + = left; a LIDAR miss is
//! `range_max_mm + 1`. The `lidar` JSON's `tick` is the LIDAR's own scan
//! tick (half a stagger after the camera/radar pass). The text lines come
//! together in the perception section's text, right after `You perceive`
//! (the three sections carry only the JSON): `LIDAR (20.00 m, 360 beams): nearest obstacle
//! N 4.50 m, NE 6.36 m, E clear, ..` (the 8 compass sectors, `+z` north),
//! one `Camera: item (track 3) 5.00 m north-west, 12 degrees left (86%
//! sure, 0.50 m wide).` line per detection ([`CAMERA_TEXT_LIMIT`], then
//! `Camera: and N more (4 tree, 1 item).`) and one `Radar: echo 7.50 m
//! north, 6 degrees right, closing at 48 mm/tick (strong echo, it moves).`
//! line per track ([`RADAR_TEXT_LIMIT`], then `Radar: and N more (2 moving,
//! 9 still).`). Camera and radar feed the memory, so the perception
//! section, `You perceive` and the affordances follow them; the LIDAR
//! names nothing.
//!
//! Content sections that list world entities directly (e.g. the village's
//! `village` section) are the content's responsibility; a content crate that
//! wants a strict point of view filters them through the same memory.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use std::collections::BTreeMap;

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_agent_io::observe::{describe, metres, possessions};
use npc_agent_io::{AgentCtx, AgentIoAppExt, Section};
use npc_kernel::spatial::SpatialGrid;
use npc_kernel::{SimId, SimIndex, SimPos, inspect};
use npc_perception::{
    Facing, Perceivable, PerceptionConfig, PerceptionMemory, ReachSense, SenseKind, SensorReadings,
    Sensors, dir_ppm, facing_angle_mdeg,
};
use serde_json::{Value, json};

/// Name of the perception section.
pub const PERCEPTION_SECTION: &str = "perception";

/// The "You perceive" line lists the this-many nearest things (plus every
/// vehicle and unit; the JSON has them all).
pub const PERCEIVE_TEXT_LIMIT: usize = 8;

/// The "You remember" line lists the this-many most recently seen things
/// (plus every vehicle and unit; the JSON has them all).
pub const REMEMBER_TEXT_LIMIT: usize = 24;

/// Whether `id` is a vehicle or a unit, from framework facts only: a
/// behaviour tree (`Brain`), an RTS `Commandable`, or a `Vehicle`. These
/// are always listed on the text lines (proposal 009: a thing one can
/// return to, ride or meet is never hidden in "and N more").
fn vehicle_or_unit(world: &World, id: SimId) -> bool {
    let Some(e) = world.get_resource::<SimIndex>().and_then(|i| i.get(id)) else {
        return false;
    };
    #[allow(unused_mut)]
    let mut yes = world.get::<npc_ai::Brain>(e).is_some();
    #[cfg(feature = "command")]
    {
        yes |= world.get::<npc_command::Commandable>(e).is_some();
    }
    #[cfg(feature = "vehicles")]
    {
        yes |= world.get::<npc_vehicles::Vehicle>(e).is_some();
    }
    yes
}

/// One thing of a text line.
struct Entry {
    /// A vehicle or unit ([`vehicle_or_unit`]): always listed.
    always: bool,
    kind: String,
    text: String,
}

/// `entries` (already in preference order) as a line's list: the first
/// `limit`, plus every `always` one after them, in that order, then
/// `"; and N more (40 tree, 12 item)"` for the rest, by kind name (count
/// descending, then name).
fn list_with_summary(entries: &[Entry], limit: usize) -> String {
    let mut shown: Vec<&str> = Vec::new();
    let mut rest: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        if i < limit || e.always {
            shown.push(&e.text);
        } else {
            *rest.entry(&e.kind).or_default() += 1;
        }
    }
    let mut s = shown.join("; ");
    if !rest.is_empty() {
        let more: usize = rest.values().sum();
        let mut kinds: Vec<(&str, usize)> = rest.into_iter().collect();
        kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let kinds: Vec<String> = kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
        s.push_str(&format!("; and {more} more ({})", kinds.join(", ")));
    }
    s
}

/// Imperceptible things (no `Perceivable`) within this distance are known by
/// touch (the default interaction reach).
pub const TOUCH_MM: i32 = 1500;

fn dist_mm(from: SimPos, to: SimPos) -> u64 {
    // `dist2` is a non-negative i64.
    u64::try_from(from.dist2(to)).unwrap_or(0).isqrt()
}

/// What `ctx.unit` perceives or remembers (known provider `perception`):
/// the ids in its `PerceptionMemory`. With it registered, affordances only
/// target these (plus the unit itself and its possessions).
/// A unit without `Sensors` has no point of view (`None`): it is not gated
/// and its observation says `omniscient: true`.
///
/// Things the perception model cannot represent (no `Perceivable`, e.g. a
/// yield spawned at runtime) are known by touch when within [`TOUCH_MM`] of
/// the unit, so an agent can pick up what it just mined at its feet.
pub fn perceived_or_remembered(world: &World, ctx: &AgentCtx) -> Option<Vec<SimId>> {
    world.get::<Sensors>(ctx.entity)?;
    let mut out: Vec<SimId> = world
        .get::<PerceptionMemory>(ctx.entity)
        .map(|m| m.percepts.keys().copied().collect())
        .unwrap_or_default();
    if let (Some(grid), Some(&here), Some(index)) = (
        world.get_resource::<SpatialGrid>(),
        world.get::<SimPos>(ctx.entity),
        world.get_resource::<SimIndex>(),
    ) {
        out.extend(grid.in_radius(here, TOUCH_MM).into_iter().filter(|&id| {
            id != ctx.unit
                && index
                    .get(id)
                    .is_some_and(|e| world.get::<Perceivable>(e).is_none())
        }));
    }
    Some(out)
}

/// The `perception` section of `ctx.unit` (see the module docs).
pub fn perception_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let clock = inspect::clock(world);
    if world.get::<Sensors>(ctx.entity).is_none() {
        return Some(
            Section::new(json!({
                "sensors": false, "sensed": [], "remembered": [], "last_noise": null,
                "reach_mm": null, "within_reach": [],
                "night": clock.night, "vision_reduced_at_night": false,
            }))
            .line("You perceive nothing (no senses)."),
        );
    }
    let now = clock.tick;
    let forget = world
        .get_resource::<PerceptionConfig>()
        .copied()
        .unwrap_or_default()
        .forget_after_ticks;
    let here = world.get::<SimPos>(ctx.entity).copied().unwrap_or_default();
    let facing = world.get::<Facing>(ctx.entity).copied().unwrap_or_default();
    let reach = world
        .get::<ReachSense>(ctx.entity)
        .map(|r| u64::try_from(r.radius_mm.max(0)).unwrap_or(0));
    let empty = PerceptionMemory::default();
    let memory = world.get::<PerceptionMemory>(ctx.entity).unwrap_or(&empty);
    let index = world.get_resource::<SimIndex>();
    // What the unit holds or rides is not "seen" or "remembered" (it is
    // listed by the inventory / vehicle sections).
    let mine = possessions(world, ctx);
    let mut sensed = Vec::new();
    let mut remembered = Vec::new();
    // `(dist, id, entry)` of the sensed things.
    let mut seen_text: Vec<(u64, SimId, Entry)> = Vec::new();
    // `(dist, id, text)` of the sensed things within reach.
    let mut reach_text: Vec<(u64, SimId, String)> = Vec::new();
    // `(last seen, dist, id, entry)` of the remembered things.
    let mut remembered_text: Vec<(u64, u64, SimId, Entry)> = Vec::new();
    for (&id, p) in &memory.percepts {
        if mine.contains(&id) {
            continue;
        }
        let d = dist_mm(here, p.pos);
        let kind = index
            .and_then(|i| i.get(id))
            .map_or("entity", |e| inspect::kind_name(world, e));
        let name = describe(world, id);
        let at = format!(
            "({} m, {} m)",
            metres(i64::from(p.pos.x)),
            metres(i64::from(p.pos.z))
        );
        if p.sensed {
            sensed.push(json!({
                "id": id, "kind": kind, "name": name, "kinds": p.kinds, "pos": [p.pos.x, p.pos.z],
                "dist_mm": d, "via": p.via.name(), "confidence": p.confidence,
                "last_seen_tick": p.last_seen_tick,
            }));
            seen_text.push((
                d,
                id,
                Entry {
                    always: vehicle_or_unit(world, id),
                    kind: kind.to_owned(),
                    text: format!(
                        "{name} at {at}, {} m away ({})",
                        metres(i64::try_from(d).unwrap_or(i64::MAX)),
                        p.via.name()
                    ),
                },
            ));
            if reach.is_some_and(|r| d <= r) {
                let note = if p.via != SenseKind::Reach {
                    ""
                } else if behind(facing, here, p.pos) {
                    " (behind you)"
                } else {
                    " (beside you)"
                };
                reach_text.push((d, id, format!("{name}{note}")));
            }
        } else {
            let age = now.saturating_sub(p.last_seen_tick);
            remembered.push(json!({
                "id": id, "kind": kind, "name": name, "kinds": p.kinds, "pos": [p.pos.x, p.pos.z],
                "dist_mm": d, "via": p.via.name(), "last_seen_tick": p.last_seen_tick,
                "age_ticks": age, "confidence": p.confidence_at(now, forget),
            }));
            remembered_text.push((
                p.last_seen_tick,
                d,
                id,
                Entry {
                    always: vehicle_or_unit(world, id),
                    kind: kind.to_owned(),
                    text: format!("{name} last seen at {at}, {age} ticks ago"),
                },
            ));
        }
    }
    reach_text.sort_by_key(|(d, id, _)| (*d, *id));
    // Nearest first, then by id.
    seen_text.sort_by_key(|(d, id, _)| (*d, *id));
    // Most recently seen first; then nearest, then by id.
    remembered_text.sort_by_key(|(t, d, id, _)| (std::cmp::Reverse(*t), *d, *id));
    let within_reach: Vec<SimId> = reach_text.iter().map(|(_, id, _)| *id).collect();
    let last_noise = memory.last_noise.map(
        |n| json!({"tick": n.tick, "pos": [n.pos.x, n.pos.z], "source": n.source, "kind": n.kind}),
    );
    let searched = super::fog_agent::searched_summary(world, ctx.entity, here);
    let mut data = json!({
        "sensors": true,
        "sensed": sensed,
        "remembered": remembered,
        "last_noise": last_noise.unwrap_or(Value::Null),
        "reach_mm": reach,
        "within_reach": within_reach,
        "night": clock.night,
        // The perception model has no light level: vision is the same by
        // day and by night.
        "vision_reduced_at_night": false,
    });
    if let (Some(s), Some(map)) = (&searched, data.as_object_mut()) {
        map.insert("searched".to_owned(), s.json());
    }
    let mut section = Section::new(data);
    section = section.line(if seen_text.is_empty() {
        "You perceive nothing right now.".to_owned()
    } else {
        let entries: Vec<Entry> = seen_text.into_iter().map(|(_, _, e)| e).collect();
        format!(
            "You perceive: {}.",
            list_with_summary(&entries, PERCEIVE_TEXT_LIMIT)
        )
    });
    // The robot sensors' lines, together, right after what is perceived
    // (their sections carry only the JSON).
    for parts in [lidar_parts, camera_parts, radar_parts] {
        if let Some(p) = parts(world, ctx) {
            for l in p.lines {
                section = section.line(l);
            }
        }
    }
    if reach.is_some() && !reach_text.is_empty() {
        let names: Vec<&str> = reach_text.iter().map(|(_, _, t)| t.as_str()).collect();
        section = section.line(format!("Within reach: {}.", names.join(", ")));
    }
    if !remembered_text.is_empty() {
        let entries: Vec<Entry> = remembered_text.into_iter().map(|(_, _, _, e)| e).collect();
        section = section.line(format!(
            "You remember: {}.",
            list_with_summary(&entries, REMEMBER_TEXT_LIMIT)
        ));
    }
    if let Some(s) = &searched {
        section = section.line(s.text());
    }
    if let Some(n) = memory.last_noise {
        section = section.line(format!(
            "Last noise heard at tick {} near ({} m, {} m).",
            n.tick,
            metres(i64::from(n.pos.x)),
            metres(i64::from(n.pos.z))
        ));
    }
    if clock.night {
        section = section.line(
            "It is night; darkness does not reduce your vision in this world (no light model).",
        );
    }
    Some(section)
}

/// Whether `at` lies behind an observer at `here` facing `facing` (integer
/// dot product below zero).
fn behind(facing: Facing, here: SimPos, at: SimPos) -> bool {
    let (fx, fz) = facing.reduced();
    let dx = i64::from(at.x) - i64::from(here.x);
    let dz = i64::from(at.z) - i64::from(here.z);
    i64::from(fx) * dx + i64::from(fz) * dz < 0
}

// ------------------------------------------------------------ robot sensors

/// Name of the LIDAR section (plan Task 12).
pub const LIDAR_SECTION: &str = "lidar";
/// Name of the camera section (plan Task 12).
pub const CAMERA_SECTION: &str = "camera";
/// Name of the radar section (plan Task 12).
pub const RADAR_SECTION: &str = "radar";
/// The camera text lists this many detections, one line each (nearest
/// first), then one line summing up the rest by class.
pub const CAMERA_TEXT_LIMIT: usize = 8;
/// The radar text lists this many tracks, one line each (nearest first),
/// then one line summing up the rest (moving / still).
pub const RADAR_TEXT_LIMIT: usize = 8;

/// `(sensors, readings)` of `ctx.unit` when it has the sensor `has` asks
/// about; the readings are `None` before its first scan.
fn robot<'w>(
    world: &'w World,
    ctx: &AgentCtx,
    has: fn(&Sensors) -> bool,
) -> Option<(&'w Sensors, Option<&'w SensorReadings>)> {
    let sensors = world.get::<Sensors>(ctx.entity).filter(|s| has(s))?;
    Some((sensors, world.get::<SensorReadings>(ctx.entity)))
}

/// A bearing (mdeg, + = left) as text: `ahead`, `12 degrees left`, ..
fn bearing_text(mdeg: i32) -> String {
    let deg = (i64::from(mdeg).abs() + 500) / 1000;
    match (deg, mdeg > 0) {
        (0, _) => "ahead".to_owned(),
        (180, _) => "behind".to_owned(),
        (1, true) => "1 degree left".to_owned(),
        (1, false) => "1 degree right".to_owned(),
        (d, true) => format!("{d} degrees left"),
        (d, false) => format!("{d} degrees right"),
    }
}

/// The compass sector ([`super::fog_agent::SECTORS`] index) of world angle
/// `mdeg` (from +x east towards +z north).
fn sector_of_angle(mdeg: i64) -> usize {
    let (x, z) = dir_ppm(mdeg);
    super::fog_agent::sector_of(x, z)
}

/// The compass name of a bearing seen by a unit facing `facing`.
fn compass_of_bearing(facing: Facing, bearing: i32) -> &'static str {
    let world = i64::from(facing_angle_mdeg(facing)) + i64::from(bearing);
    super::fog_agent::SECTORS[sector_of_angle(world)].1
}

/// The `lidar` section: JSON = the `LaserScan` (ROS `sensor_msgs/LaserScan`
/// in mm / millidegrees, a miss = `range_max_mm + 1`) plus the scan's
/// `tick`; text = the nearest thing hit in each of the 8 compass sectors
/// (`+z` north, `+x` east). `None` without a LIDAR.
pub fn lidar_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    lidar_parts(world, ctx).map(|s| Section::new(s.value))
}

/// The `lidar` JSON and its text lines.
fn lidar_parts(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let (sensors, readings) = robot(world, ctx, |s| s.lidar_sensor().is_some())?;
    let Some((tick, scan)) = readings.and_then(|r| r.lidar.as_ref().map(|l| (r.lidar_tick, l)))
    else {
        let range = sensors.lidar_sensor().map_or(0, |l| l.range_mm);
        return Some(
            Section::new(json!({"range_max_mm": range, "ranges_mm": [], "tick": null}))
                .line("LIDAR: no scan yet."),
        );
    };
    let mut data = serde_json::to_value(scan).unwrap_or(Value::Null);
    if let Some(map) = data.as_object_mut() {
        map.insert("tick".to_owned(), json!(tick));
    }
    let facing = world.get::<Facing>(ctx.entity).copied().unwrap_or_default();
    let base = i64::from(facing_angle_mdeg(facing));
    let mut nearest: [Option<i32>; 8] = [None; 8];
    for (i, &r) in scan.ranges_mm.iter().enumerate() {
        if scan.is_miss(r) {
            continue;
        }
        let s = sector_of_angle(base + scan.angle_of(i));
        nearest[s] = Some(nearest[s].map_or(r, |n| n.min(r)));
    }
    let parts: Vec<String> = super::fog_agent::SECTORS
        .iter()
        .zip(nearest)
        .map(|((dir, _), n)| match n {
            Some(r) => format!("{dir} {} m", metres(i64::from(r))),
            None => format!("{dir} clear"),
        })
        .collect();
    let line = format!(
        "LIDAR ({} m, {} beams): nearest obstacle {}.",
        metres(i64::from(scan.range_max_mm)),
        scan.ranges_mm.len(),
        parts.join(", ")
    );
    Some(Section::new(data).line(line))
}

/// The `camera` section: JSON `{"tick", "detections": [..]}` (ROS
/// `vision_msgs` style: class, track id, bearing (+ = left), distance,
/// size, confidence in ppm); text = one line per detection (the
/// [`CAMERA_TEXT_LIMIT`] nearest), then the rest summed up by class.
/// `None` without a camera.
pub fn camera_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    camera_parts(world, ctx).map(|s| Section::new(s.value))
}

/// The `camera` JSON and its text lines.
fn camera_parts(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let (_, readings) = robot(world, ctx, |s| s.camera_sensor().is_some())?;
    let facing = world.get::<Facing>(ctx.entity).copied().unwrap_or_default();
    let detections = readings.map_or(&[][..], |r| r.camera.as_slice());
    let mut section = Section::new(json!({
        "tick": readings.map(|r| r.tick),
        "detections": detections,
    }));
    if detections.is_empty() {
        return Some(section.line("Camera: nothing in view."));
    }
    let mut rest: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, d) in detections.iter().enumerate() {
        if i >= CAMERA_TEXT_LIMIT {
            *rest.entry(d.class.as_str()).or_default() += 1;
            continue;
        }
        section = section.line(format!(
            "Camera: {} (track {}) {} m {}, {} ({}% sure, {} m wide).",
            d.class,
            d.track_id,
            metres(i64::from(d.distance_mm)),
            compass_of_bearing(facing, d.bearing_mdeg),
            bearing_text(d.bearing_mdeg),
            d.confidence_ppm / 10_000,
            metres(i64::from(d.size_mm)),
        ));
    }
    if !rest.is_empty() {
        let more: usize = rest.values().sum();
        let mut kinds: Vec<(&str, usize)> = rest.into_iter().collect();
        kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let kinds: Vec<String> = kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
        section = section.line(format!("Camera: and {more} more ({}).", kinds.join(", ")));
    }
    Some(section)
}

/// The `radar` section: JSON `{"tick", "tracks": [..]}` (ROS `radar_msgs`
/// style: bearing (+ = left) and range at the radar's resolution, radial
/// velocity in mm per tick (+ = moving away), amplitude in ppm); text = one
/// line per track (the [`RADAR_TEXT_LIMIT`] nearest), then the rest summed
/// up as moving / still. `None` without a radar.
pub fn radar_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    radar_parts(world, ctx).map(|s| Section::new(s.value))
}

/// The `radar` JSON and its text lines.
fn radar_parts(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let (_, readings) = robot(world, ctx, |s| s.radar_sensor().is_some())?;
    let facing = world.get::<Facing>(ctx.entity).copied().unwrap_or_default();
    let tracks = readings.map_or(&[][..], |r| r.radar.as_slice());
    let mut section = Section::new(json!({
        "tick": readings.map(|r| r.tick),
        "tracks": tracks,
    }));
    if tracks.is_empty() {
        return Some(section.line("Radar: no echoes."));
    }
    let (mut moving, mut still) = (0usize, 0usize);
    for (i, t) in tracks.iter().enumerate() {
        let v = t.radial_velocity_mm_per_tick;
        if i >= RADAR_TEXT_LIMIT {
            if t.amplitude_ppm >= radar_moving_floor() {
                moving += 1;
            } else {
                still += 1;
            }
            continue;
        }
        let motion = match v.signum() {
            1 => format!("moving away at {v} mm/tick"),
            -1 => format!("closing at {} mm/tick", v.unsigned_abs()),
            _ => "no radial motion".to_owned(),
        };
        let strength = if t.amplitude_ppm >= radar_moving_floor() {
            "strong echo, it moves"
        } else {
            "weak echo"
        };
        section = section.line(format!(
            "Radar: echo {} m {}, {}, {motion} ({strength}).",
            metres(i64::from(t.range_mm)),
            compass_of_bearing(facing, t.bearing_mdeg),
            bearing_text(t.bearing_mdeg),
        ));
    }
    if moving + still > 0 {
        section = section.line(format!(
            "Radar: and {} more ({moving} moving, {still} still).",
            moving + still
        ));
    }
    Some(section)
}

/// The weakest amplitude of a moving target (a moving one at the radar's
/// range is half its base, still above any still one).
fn radar_moving_floor() -> u32 {
    npc_perception::radar::AMPLITUDE_MOVING_PPM / 2
}

/// Perception × agent-io glue: registers the `perception` provider (see the
/// module docs) and the robot sensors' `lidar`, `camera` and `radar`
/// sections (plan Task 12; absent for a unit without that sensor). Add it
/// with `AgentIoPlugin` and `PerceptionPlugin`.
pub struct PerceptionAgentPlugin;

impl Plugin for PerceptionAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_perception(PERCEPTION_SECTION, perception_section)
            .register_perception(LIDAR_SECTION, lidar_section)
            .register_perception(CAMERA_SECTION, camera_section)
            .register_perception(RADAR_SECTION, radar_section)
            .register_known(PERCEPTION_SECTION, perceived_or_remembered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_read_as_text() {
        assert_eq!(bearing_text(0), "ahead");
        assert_eq!(bearing_text(400), "ahead");
        assert_eq!(bearing_text(1000), "1 degree left");
        assert_eq!(bearing_text(-1200), "1 degree right");
        assert_eq!(bearing_text(12_000), "12 degrees left");
        assert_eq!(bearing_text(-27_000), "27 degrees right");
        assert_eq!(bearing_text(180_000), "behind");
    }
}
