//! The `debug.*` command handlers (spec §5.5, ADR-0008): testbed actions that change the sim,
//! recorded and replayed like any other command. Every payload is `DEBUG_ONLY`: accepted only
//! when the match header's `cheats_allowed` is set, and free to target any player's units.
//!
//! `"debug.set_terrain"` journals `"core.terrain_changed"` (phase `Input`).

use bevy_ecs::component::Component;
use bevy_ecs::world::{EntityRef, World};
use fl_content::TerrainDef;
use fl_math::Fx;
use fl_protocol::{
    DebugInvulnerableCmd, DebugSetHpCmd, DebugSetOwnerCmd, DebugSetTerrainCmd, DebugSpawnCmd,
    DebugTeleportCmd, DebugWanderCmd,
};
use serde::{Deserialize, Serialize};

use crate::build::Content;
use crate::builder::SimBuilder;
use crate::classify::Class;
use crate::commands::{CommandCtx, CommandHandler, INVALID_TARGET, RejectReason};
use crate::hash::{SimHash, StateHasher};
use crate::map::MapInfo;
use crate::nav::{NavGrid, profile_of};
use crate::observe::{Journal, JournalRecord};
use crate::path::{PathFollow, PathState};
use crate::players::Players;
use crate::schedule::Phase;
use crate::spawn::{SpawnRequest, blocked_for};
use crate::terrain::{CellRect, TerrainGrid};
use crate::units::{Footprint, Health, Mobility, MoveOrder, Owner, Pose, Wander};

/// The unit takes no damage (set by `"debug.invulnerable"`). Hashed `"core.invulnerable"`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invulnerable;

/// No data: presence is the state.
impl SimHash for Invulnerable {
    fn sim_hash(&self, _h: &mut StateHasher) {}
}

/// `"debug.spawn"`: request a unit of the type named `unit` for `owner` at `pos` (clamped to
/// the map); it appears at the end of the tick, through the spawn funnel. An unknown unit name
/// is [`RejectReason::UnknownContent`], an unknown owner [`RejectReason::UnknownPlayer`],
/// without a map no position is bounded ([`RejectReason::NoValidUnits`]), and a cell the unit's
/// profile cannot stand on is `"core.invalid_target"`, so an applied command always spawns.
struct DebugSpawnHandler;

impl CommandHandler<DebugSpawnCmd> for DebugSpawnHandler {
    fn apply(
        &self,
        payload: &DebugSpawnCmd,
        world: &mut World,
        _ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        let unit = world
            .resource::<Content>()
            .0
            .unit_id(&payload.unit)
            .ok_or(RejectReason::UnknownContent)?;
        if world.resource::<Players>().get(payload.owner).is_none() {
            return Err(RejectReason::UnknownPlayer);
        }
        let pos = world
            .get_resource::<MapInfo>()
            .ok_or(RejectReason::NoValidUnits)?
            .clamp(payload.pos);
        let content = &world.resource::<Content>().0;
        if let (Some(nav), Some(def)) = (world.get_resource::<NavGrid>(), content.unit(unit))
            && blocked_for(content, def, nav, pos)
        {
            return Err(INVALID_TARGET);
        }
        world.write_message(SpawnRequest {
            unit: Some(unit),
            owner: payload.owner,
            pos,
            heading: payload.heading,
            init: None,
        });
        Ok(())
    }
}

/// `"debug.set_owner"`: hand the units to `owner` ([`RejectReason::UnknownPlayer`] if there
/// is no such player).
struct DebugSetOwnerHandler;

impl CommandHandler<DebugSetOwnerCmd> for DebugSetOwnerHandler {
    fn apply(
        &self,
        payload: &DebugSetOwnerCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        if world.resource::<Players>().get(payload.owner).is_none() {
            return Err(RejectReason::UnknownPlayer);
        }
        for &(_, entity) in ctx.units {
            world.entity_mut(entity).insert(Owner(payload.owner));
        }
        Ok(())
    }
}

/// `"debug.invulnerable"`: insert or remove [`Invulnerable`].
struct DebugInvulnerableHandler;

impl CommandHandler<DebugInvulnerableCmd> for DebugInvulnerableHandler {
    fn apply(
        &self,
        payload: &DebugInvulnerableCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        for &(_, entity) in ctx.units {
            let mut unit = world.entity_mut(entity);
            if payload.on {
                unit.insert(Invulnerable);
            } else {
                unit.remove::<Invulnerable>();
            }
        }
        Ok(())
    }
}

/// `"debug.set_hp"`: set hit points to `hp`, capped at the maximum. Needs [`Health`].
struct DebugSetHpHandler;

impl CommandHandler<DebugSetHpCmd> for DebugSetHpHandler {
    fn unit_can(&self, _payload: &DebugSetHpCmd, unit: EntityRef<'_>) -> bool {
        unit.contains::<Health>()
    }

    fn apply(
        &self,
        payload: &DebugSetHpCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        for &(_, entity) in ctx.units {
            if let Some(mut health) = world.get_mut::<Health>(entity) {
                health.hp = payload.hp.min(health.max);
            }
        }
        Ok(())
    }
}

/// `"debug.teleport"`: put the units at `pos` (clamped to the map), drop their orders and
/// set their paths back to `Idle`. Needs [`Pose`] and a map ([`RejectReason::NoValidUnits`] without one). A cell
/// one of the moving units cannot stand on is `"core.invalid_target"`: no unit moves.
struct DebugTeleportHandler;

impl CommandHandler<DebugTeleportCmd> for DebugTeleportHandler {
    fn unit_can(&self, _payload: &DebugTeleportCmd, unit: EntityRef<'_>) -> bool {
        unit.contains::<Pose>()
    }

    fn apply(
        &self,
        payload: &DebugTeleportCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        let pos = world
            .get_resource::<MapInfo>()
            .ok_or(RejectReason::NoValidUnits)?
            .clamp(payload.pos);
        if let Some(nav) = world.get_resource::<NavGrid>() {
            let (x, y) = nav.cell_of(pos);
            let blocked = ctx.units.iter().any(|&(_, entity)| {
                world.get::<Mobility>(entity).is_some_and(|mobility| {
                    let footprint = world.get::<Footprint>(entity);
                    let profile =
                        profile_of(mobility, footprint, nav.cell_m(), nav.max_clearance());
                    !nav.passable(profile, i64::from(x), i64::from(y))
                })
            });
            if blocked {
                return Err(INVALID_TARGET);
            }
        }
        for &(_, entity) in ctx.units {
            if let Some(mut pose) = world.get_mut::<Pose>(entity) {
                pose.pos = pos;
            }
            if let Some(mut order) = world.get_mut::<MoveOrder>(entity) {
                order.orders.clear();
            }
            if let Some(mut follow) = world.get_mut::<PathFollow>(entity) {
                follow.state = PathState::Idle;
                follow.corners.clear();
                follow.next = 0;
            }
        }
        Ok(())
    }
}

/// `"debug.wander"`: wander within `radius` of the current position; a radius of 0 (or
/// less) stops wandering. The radius is capped at the map's longer side. Needs [`Mobility`],
/// [`Pose`] and a map ([`RejectReason::NoValidUnits`] without one).
struct DebugWanderHandler;

impl CommandHandler<DebugWanderCmd> for DebugWanderHandler {
    fn unit_can(&self, _payload: &DebugWanderCmd, unit: EntityRef<'_>) -> bool {
        unit.contains::<Mobility>() && unit.contains::<Pose>()
    }

    fn apply(
        &self,
        payload: &DebugWanderCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        let map = world
            .get_resource::<MapInfo>()
            .ok_or(RejectReason::NoValidUnits)?;
        let radius = payload.radius.min(map.size.x.max(map.size.y));
        for &(_, entity) in ctx.units {
            let mut unit = world.entity_mut(entity);
            if radius <= Fx::ZERO {
                unit.remove::<Wander>();
            } else if let Some(&Pose { pos, .. }) = unit.get::<Pose>() {
                unit.insert(Wander { home: pos, radius });
            }
        }
        Ok(())
    }
}

/// Journal `"core.terrain_changed"`: `debug.set_terrain` made the cells `min..=max` terrain
/// `terrain`.
#[derive(Debug, Serialize)]
struct TerrainChanged {
    min: (u32, u32),
    max: (u32, u32),
    terrain: String,
}

impl JournalRecord for TerrainChanged {
    const KIND: &'static str = "core.terrain_changed";
}

/// `"debug.set_terrain"`: make the cells `min_cell..=max_cell` terrain `terrain`, then bring
/// the navigation data up to date ([`on_terrain_changed`](crate::nav::on_terrain_changed),
/// which counts `"nav.cells_updated"`), and journal `"core.terrain_changed"`. An id that is no
/// `TerrainDef` is [`RejectReason::UnknownContent`]; `min > max` on an axis, a rect not inside
/// the map, no map, or a full palette is `"core.invalid_target"`.
struct DebugSetTerrainHandler;

impl CommandHandler<DebugSetTerrainCmd> for DebugSetTerrainHandler {
    fn apply(
        &self,
        payload: &DebugSetTerrainCmd,
        world: &mut World,
        ctx: &CommandCtx<'_>,
    ) -> Result<(), RejectReason> {
        let known = world
            .resource::<Content>()
            .0
            .kind::<TerrainDef>()
            .get(&payload.terrain)
            .is_some();
        if !known {
            return Err(RejectReason::UnknownContent);
        }
        let terrain = world.get_resource::<TerrainGrid>().ok_or(INVALID_TARGET)?;
        let rect = CellRect {
            min: payload.min_cell,
            max: payload.max_cell,
        };
        let inside = rect.min.0 <= rect.max.0
            && rect.min.1 <= rect.max.1
            && rect.max.0 < terrain.cells_x
            && rect.max.1 < terrain.cells_y;
        // A palette names loaded kinds only, so it stays below 255 names (ADR-0014); the check
        // keeps a palette that outlived a content change from overflowing.
        let fits = terrain.palette.len() < usize::from(TerrainGrid::OPEN)
            || terrain.palette.contains(&payload.terrain);
        if !inside || !fits {
            return Err(INVALID_TARGET);
        }
        world
            .resource_mut::<TerrainGrid>()
            .set_rect(rect, &payload.terrain);
        crate::nav::on_terrain_changed(world, rect);
        let mut journal = world.resource_mut::<Journal>();
        if journal.enabled() {
            journal.record(
                ctx.tick,
                Phase::Input,
                None,
                Some(ctx.player),
                &TerrainChanged {
                    min: rect.min,
                    max: rect.max,
                    terrain: payload.terrain.clone(),
                },
            );
        }
        Ok(())
    }
}

/// [`Invulnerable`], the seven `debug.*` handlers and the `"core.terrain_changed"` record.
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.register_component::<Invulnerable>("core.invulnerable", Class::hashed(1))
        .register_command::<DebugSpawnCmd>(DebugSpawnHandler)
        .register_command::<DebugSetOwnerCmd>(DebugSetOwnerHandler)
        .register_command::<DebugInvulnerableCmd>(DebugInvulnerableHandler)
        .register_command::<DebugSetHpCmd>(DebugSetHpHandler)
        .register_command::<DebugTeleportCmd>(DebugTeleportHandler)
        .register_command::<DebugWanderCmd>(DebugWanderHandler)
        .register_command::<DebugSetTerrainCmd>(DebugSetTerrainHandler)
        .register_journal_record::<TerrainChanged>();
}
