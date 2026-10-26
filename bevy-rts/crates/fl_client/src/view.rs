//! The sim view: drives the runner with [`LocalDriver`], receives snapshots and events, and keeps
//! one visual entity per visible snapshot record, interpolated between the last two snapshots
//! (spec §5.3, §10, ADR-0002). The client never reads sim components.
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use crossbeam_channel::TryRecvError;
use fl_content::ContentDb;
use fl_math::{Angle, FxVec2};
use fl_protocol::{PlayerId, SimId, Tick, UnitTypeId};
use fl_view::{EntityRecord, FromRunner, RunnerFailure, TickDriver, ToRunner};

use crate::driver::{LocalDriver, MAX_ADVANCE};
use crate::link::{
    InputLog, JournalBuffer, PendingReplies, RunnerBacklog, RunnerLink, RunnerStatus, SimEventMsg,
    SimJournalMsg, SnapshotAge, SnapshotBuffer,
};
use crate::map_view::MapView;
use crate::terrain::TerrainView;

/// The loaded content, shared with the sim runner.
#[derive(Resource, Clone)]
pub struct ClientContent(pub Arc<ContentDb>);

/// The player this client controls and views as.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalPlayer(pub PlayerId);

/// Render override: draw every record regardless of `visible_to`.
#[derive(Resource, Default, Clone, Copy)]
pub struct RevealAll(pub bool);

/// The visual entity of one snapshot record.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitView {
    pub sim_id: SimId,
    pub unit: UnitTypeId,
    pub owner: PlayerId,
}

/// The pose of a visual in the previous and the current snapshot, in world space.
#[derive(Component, Clone, Copy, Debug)]
pub struct ViewPose {
    pub prev: Vec3,
    pub curr: Vec3,
    pub prev_yaw: f32,
    pub curr_yaw: f32,
}

impl ViewPose {
    /// A pose that stands still at `pos`, facing `yaw`.
    #[must_use]
    pub fn at_rest(pos: Vec3, yaw: f32) -> Self {
        ViewPose {
            prev: pos,
            curr: pos,
            prev_yaw: yaw,
            curr_yaw: yaw,
        }
    }

    /// The translation and yaw a fraction `t` of the way from `prev` to `curr`.
    #[must_use]
    pub fn sample(&self, t: f32) -> (Vec3, f32) {
        (
            self.prev.lerp(self.curr, t),
            lerp_yaw(self.prev_yaw, self.curr_yaw, t),
        )
    }
}

/// Visual entities by `SimId`.
#[derive(Resource, Default)]
pub struct ViewIndex(pub BTreeMap<SimId, Entity>);

/// The systems of [`SimViewPlugin`], in `Update`: drive the runner, sync views, interpolate.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimViewSystems;

/// Sim `(x, y)` in metres to world `(x, height, y)`.
#[must_use]
pub fn sim_to_world(pos: FxVec2, height: f32) -> Vec3 {
    Vec3::new(pos.x.to_f32(), height, pos.y.to_f32())
}

/// A sim heading (counter-clockwise from +x) to a rotation about world +Y.
#[must_use]
pub fn heading_to_yaw(h: Angle) -> f32 {
    -h.to_radians_f32()
}

/// Interpolates from yaw `a` to yaw `b` along the shorter arc.
#[must_use]
pub fn lerp_yaw(a: f32, b: f32, t: f32) -> f32 {
    let diff = (b - a + PI).rem_euclid(TAU) - PI;
    a + diff * t
}

/// The world pose of a record.
fn record_pose(record: &EntityRecord, terrain: Option<&TerrainView>) -> (Vec3, f32) {
    let height = terrain.map_or(0.0, |t| {
        t.height_at(record.pos.x.to_f32(), record.pos.y.to_f32())
    });
    (
        sim_to_world(record.pos, height),
        heading_to_yaw(record.heading),
    )
}

/// Whether the local view draws `record`.
///
/// M0 has no player → team map in the client and the sim marks every record visible to all
/// teams, so a record is drawn when any team sees it; M3 narrows this to the local team.
fn is_drawn(record: &EntityRecord, reveal: RevealAll) -> bool {
    reveal.0 || record.visible_to != 0
}

/// Drives the runner from the [`LocalDriver`] and keeps visuals in step with its snapshots.
pub struct SimViewPlugin;

impl Plugin for SimViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SnapshotBuffer>()
            .init_resource::<RunnerStatus>()
            .init_resource::<PendingReplies>()
            .init_resource::<RunnerBacklog>()
            .init_resource::<ViewIndex>()
            .init_resource::<RevealAll>()
            .init_resource::<SnapshotAge>()
            .init_resource::<JournalBuffer>()
            .add_message::<SimEventMsg>()
            .add_message::<SimJournalMsg>()
            .add_systems(
                Update,
                (
                    drive_runner
                        .run_if(resource_exists::<RunnerLink>)
                        .run_if(resource_exists::<LocalDriver>),
                    sync_views,
                    interpolate_views.run_if(resource_exists::<LocalDriver>),
                )
                    .chain()
                    .in_set(SimViewSystems),
            );
    }
}

/// Stops the match: records the first failure and pauses the driver.
fn fail(status: &mut RunnerStatus, driver: &mut LocalDriver, failure: RunnerFailure) {
    driver.pause();
    if status.failed.is_none() {
        tracing::error!(
            tick = failure.tick.0,
            message = %failure.message,
            "sim runner failed; the match is stopped"
        );
        status.failed = Some(failure);
    }
}

/// The failure recorded when the runner's channel closes without a report.
fn disconnected(status: &RunnerStatus, driver: &LocalDriver) -> RunnerFailure {
    RunnerFailure {
        tick: status
            .last_tick
            .map_or_else(|| driver.next_tick(), Tick::next),
        message: "the sim runner disconnected".to_owned(),
    }
}

/// The runner's outputs, and the [`InputLog`] of what it was sent, as one system parameter.
#[derive(SystemParam)]
struct Received<'w> {
    buffer: ResMut<'w, SnapshotBuffer>,
    age: ResMut<'w, SnapshotAge>,
    journal: ResMut<'w, JournalBuffer>,
    events: MessageWriter<'w, SimEventMsg>,
    journal_msgs: MessageWriter<'w, SimJournalMsg>,
    log: Option<ResMut<'w, InputLog>>,
}

/// Advances the driver by the frame's real time, sends every due `TickInput`, then drains every
/// message from the runner, the [`RunnerBacklog`] first: ticks rotate the snapshot buffer
/// (restarting [`SnapshotAge`]) and write their events and journal entries, replies go to
/// [`PendingReplies`], and a failure stops the match. With an [`InputLog`], every input sent is
/// recorded and every state hash received appended.
fn drive_runner(
    time: Res<Time<Real>>,
    link: Res<RunnerLink>,
    mut driver: ResMut<LocalDriver>,
    mut status: ResMut<RunnerStatus>,
    mut replies: ResMut<PendingReplies>,
    mut backlog: ResMut<RunnerBacklog>,
    mut received: Received,
) {
    received.age.0 += time.delta().min(MAX_ADVANCE);
    if status.failed.is_none() {
        driver.advance(time.delta());
        while let Some(input) = driver.poll() {
            if let Some(log) = received.log.as_deref_mut() {
                log.record(&input);
            }
            if link.port.tx.send(ToRunner::Tick(input)).is_err() {
                let failure = disconnected(&status, &driver);
                fail(&mut status, &mut driver, failure);
                break;
            }
        }
    }
    loop {
        let message = match backlog.0.pop_front() {
            Some(message) => Ok(message),
            None => link.port.rx.try_recv(),
        };
        match message {
            Ok(FromRunner::Tick(output)) => {
                status.last_tick = Some(output.tick);
                if let Some(hash) = output.hash {
                    status.last_hash = Some(hash);
                    if let Some(log) = received.log.as_deref_mut() {
                        log.hashes.push((output.tick, hash));
                    }
                }
                received.buffer.push(output.snapshot);
                received.age.0 = Duration::ZERO;
                received
                    .events
                    .write_batch(output.events.into_iter().map(SimEventMsg));
                for entry in output.journal {
                    received.journal.push(entry.clone());
                    received.journal_msgs.write(SimJournalMsg(entry));
                }
            }
            Ok(FromRunner::Reply { id, reply }) => replies.insert(id, reply),
            Ok(FromRunner::Failed(failure)) => fail(&mut status, &mut driver, failure),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                if status.failed.is_none() {
                    let failure = disconnected(&status, &driver);
                    fail(&mut status, &mut driver, failure);
                }
                break;
            }
        }
    }
}

/// Merges the current snapshot into the visuals: spawns views for new records, despawns views
/// whose record vanished (or is no longer drawn), and moves the rest from their previous to
/// their new pose. Runs when a new snapshot arrives or [`RevealAll`] changes.
fn sync_views(
    mut commands: Commands,
    buffer: Res<SnapshotBuffer>,
    reveal: Res<RevealAll>,
    map: Option<Res<MapView>>,
    mut index: ResMut<ViewIndex>,
    mut views: Query<(&mut UnitView, &mut ViewPose)>,
    mut synced: Local<Option<Tick>>,
) {
    let Some(curr) = buffer.curr.as_deref() else {
        return;
    };
    let fresh = *synced != Some(curr.tick);
    if !fresh && !reveal.is_changed() {
        return;
    }
    *synced = Some(curr.tick);
    let reveal = *reveal;
    let prev = buffer.prev.as_deref();
    let terrain = map.as_deref().and_then(|map| map.terrain.as_ref());

    index.0.retain(|&sim_id, &mut entity| {
        let keep = curr
            .get(sim_id)
            .is_some_and(|record| is_drawn(record, reveal));
        if !keep {
            commands.entity(entity).try_despawn();
        }
        keep
    });

    for record in curr.entities.iter().filter(|r| is_drawn(r, reveal)) {
        let (pos, yaw) = record_pose(record, terrain);
        let view = UnitView {
            sim_id: record.sim_id,
            unit: record.unit,
            owner: record.owner,
        };
        // A view despawned behind our back is spawned again.
        let existing = match index.0.get(&record.sim_id) {
            Some(&entity) => views.get_mut(entity).ok(),
            None => None,
        };
        if let Some((mut unit_view, mut pose)) = existing {
            if *unit_view != view {
                *unit_view = view;
            }
            if fresh {
                let (prev_pos, prev_yaw) = prev
                    .and_then(|snapshot| snapshot.get(record.sim_id))
                    .map_or((pose.curr, pose.curr_yaw), |old| record_pose(old, terrain));
                *pose = ViewPose {
                    prev: prev_pos,
                    curr: pos,
                    prev_yaw,
                    curr_yaw: yaw,
                };
            }
            continue;
        }
        let pose = prev
            .and_then(|snapshot| snapshot.get(record.sim_id))
            .map_or(ViewPose::at_rest(pos, yaw), |old| {
                let (prev_pos, prev_yaw) = record_pose(old, terrain);
                ViewPose {
                    prev: prev_pos,
                    curr: pos,
                    prev_yaw,
                    curr_yaw: yaw,
                }
            });
        let entity = commands
            .spawn((
                Name::new(format!("unit {}", record.sim_id)),
                view,
                pose,
                Transform::from_translation(pose.prev)
                    .with_rotation(Quat::from_rotation_y(pose.prev_yaw)),
                Visibility::default(),
            ))
            .id();
        index.0.insert(record.sim_id, entity);
    }
}

/// How far the view is from the previous snapshot to the current one, on the receive timeline
/// (ruling R42): `age` since the current snapshot arrived over one tick period at the current
/// speed, clamped to `[0, 1]`; 1 while paused, so the latest state shows (also after a step).
#[must_use]
pub fn interpolation_fraction(age: Duration, tick_period: Duration, paused: bool) -> f32 {
    if paused || tick_period.is_zero() {
        return 1.0;
    }
    (age.as_secs_f32() / tick_period.as_secs_f32()).clamp(0.0, 1.0)
}

/// Places every view between its previous and current pose at [`interpolation_fraction`].
fn interpolate_views(
    driver: Res<LocalDriver>,
    age: Res<SnapshotAge>,
    mut views: Query<(&ViewPose, &mut Transform)>,
) {
    let t = interpolation_fraction(age.0, driver.tick_period(), driver.is_paused());
    for (pose, mut transform) in &mut views {
        let (translation, yaw) = pose.sample(t);
        transform.translation = translation;
        transform.rotation = Quat::from_rotation_y(yaw);
    }
}
