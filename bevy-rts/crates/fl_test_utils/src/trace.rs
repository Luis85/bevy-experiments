//! Pose traces: a hash of every unit's pose after each tick, recorded once and compared later
//! to prove that a change leaves unit movement exactly as it was (M2 prelude Task 2).

use std::fmt::Write as _;
use std::io;
use std::path::Path;

use bevy_ecs::world::World;
use fl_protocol::Tick;
use fl_sim::{Health, Pose, SimIndex, StateHasher};

use crate::Scenario;

/// A hash over every unit in [`SimIndex`] order: its `SimId`, `Pose.pos`, `Pose.heading` and
/// `Health.hp` (`u32::MAX` without [`Health`]). A unit without a [`Pose`] contributes its id
/// and hit points only.
pub fn pose_hash(world: &World) -> u64 {
    let mut h = StateHasher::new();
    for (id, entity) in world.resource::<SimIndex>().iter() {
        h.write_u32(id.0);
        if let Some(pose) = world.get::<Pose>(entity) {
            h.write_fx(pose.pos.x);
            h.write_fx(pose.pos.y);
            h.write_u16(pose.heading.raw());
        }
        h.write_u32(
            world
                .get::<Health>(entity)
                .map_or(u32::MAX, |health| health.hp),
        );
    }
    h.finish()
}

/// Runs the next `ticks` ticks and returns, after each, `(the tick that ran, pose_hash)`.
pub fn pose_trace(scenario: &mut Scenario, ticks: u32) -> Vec<(Tick, u64)> {
    (0..ticks)
        .map(|_| {
            let ran = scenario.tick();
            scenario.run_ticks(1);
            (ran, pose_hash(scenario.world()))
        })
        .collect()
}

/// Writes `trace` as `"<tick> <hash as 016x>\n"` per entry.
pub fn write_pose_trace(path: &Path, trace: &[(Tick, u64)]) -> io::Result<()> {
    let mut text = String::new();
    for (tick, hash) in trace {
        writeln!(text, "{} {hash:016x}", tick.0).expect("writing to a String");
    }
    std::fs::write(path, text)
}

/// Reads a trace written by [`write_pose_trace`].
pub fn read_pose_trace(path: &Path) -> io::Result<Vec<(Tick, u64)>> {
    let bad = |line: &str| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: bad pose trace line {line:?}", path.display()),
        )
    };
    std::fs::read_to_string(path)?
        .lines()
        .map(|line| {
            let (tick, hash) = line.split_once(' ').ok_or_else(|| bad(line))?;
            let tick = tick.parse().map_err(|_| bad(line))?;
            let hash = u64::from_str_radix(hash, 16).map_err(|_| bad(line))?;
            Ok((Tick(tick), hash))
        })
        .collect()
}
