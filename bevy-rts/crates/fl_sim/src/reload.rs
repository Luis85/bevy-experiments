//! Hot reload, sim side (ADR-0012 and its M1 refinement): [`apply_content_values`] puts new
//! content with the same ID sets into a running match, in place, between ticks.
//!
//! Only a testbed match reloads: it needs `cheats_allowed`. The new content's values are
//! content, not wire input: [`load_dirs`](fl_content::load_dirs) range-checked them in its link
//! pass (R104), as at startup. Content whose ID sets differ cannot apply in place, since dense
//! ids would change under live entities; the driver rebuilds the match from a save instead
//! ([`load_world_with`](crate::load_world_with) with
//! [`LoadOptions::drop_unknown_units`](crate::LoadOptions)), and the load re-plans the units
//! whose flow field may be stale ([`replan_loaded_flow_units`]), as step 5 below does.
//!
//! Neither path rewrites the Hashed [`TerrainGrid`](crate::TerrainGrid): an edited map changes
//! the content and the rules hash, and shows when a match next starts from it.
//!
//! Journal `"core.content_reload"` (phase `Input` of the tick the match runs on with).

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use fl_content::{ContentDb, DefRegistry, SpawnCtx};
use fl_math::{Angle, FxVec2};
use fl_protocol::{PlayerId, SimId, Tick};
use serde::Serialize;

use crate::build::{Content, MatchHeaderRes};
use crate::builder::SimBuilder;
use crate::clock::SimClock;
use crate::ids::SimIndex;
use crate::nav::{MoveProfile, PathRules};
use crate::observe::{Journal, JournalRecord};
use crate::orders::{Reason, idle, journal_order_changed};
use crate::path::{PathFollow, PathState, unit_profile};
use crate::save::rebuild_derived;
use crate::schedule::Phase;
use crate::units::{Owner, Pose, UnitType};

/// What [`apply_content_values`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadReport {
    /// The tick the match runs on with under the new content (the next to run).
    pub tick: Tick,
    pub old_rules_hash: u64,
    pub rules_hash: u64,
    /// The units whose def components were reloaded.
    pub units_reloaded: u32,
}

/// Why [`apply_content_values`] refused the content; the match is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReloadError {
    #[error("hot reload needs cheats_allowed")]
    CheatsOff,
    /// The lines of [`ContentDb::id_set_diff`].
    #[error("the content's ids changed: {0:?}")]
    IdSetsDiffer(Vec<String>),
    #[error("the content was loaded with another feature set")]
    RegistryMismatch,
}

/// Journal `"core.content_reload"`: the match runs on under content with `rules_hash` (it had
/// `old_rules_hash`), with the def components of `units` units reloaded.
#[derive(Debug, Serialize)]
struct ContentReloaded {
    old_rules_hash: u64,
    rules_hash: u64,
    units: u32,
}

impl JournalRecord for ContentReloaded {
    const KIND: &'static str = "core.content_reload";
}

/// Apply `content` to the running match in `world`, between ticks (a value-only hot reload,
/// ADR-0012):
/// 1. refuse without `cheats_allowed` ([`ReloadError::CheatsOff`]), for content loaded with
///    another feature set ([`ReloadError::RegistryMismatch`]) or whose ID sets differ from
///    the running content's ([`ReloadError::IdSetsDiffer`], [`ContentDb::id_set_diff`]),
///    changing nothing;
/// 2. replace [`Content`] and the [`MatchHeaderRes`] rules hash (the string tables are the
///    same: the ID sets are);
/// 3. in [`SimIndex`] order, reload every unit's def components from the new content, in
///    component-name order ([`DefComponent::reload`](fl_content::DefComponent::reload): the
///    hull keeps its HP ratio, mobility keeps orders and paths);
/// 4. run every feature's `Derived` rebuild in feature load order, as a load does (R74, R101:
///    the navigation grid, HPA* graphs and an empty flow cache, through `install_nav`);
/// 5. in [`SimIndex`] order, send every unit whose movement profile changed (its class, or its
///    size in cells) and that waits on a path request, follows a path or follows a flow field
///    back to `Idle`, journaled `"core.order_changed"` `"replanned"`:
///    [`advance_orders`](crate::orders) asks for a new path next tick, and the old request
///    turns stale, so no path state names a profile the rebuilt graphs lack. When
///    `Path.cluster_cells` changed, every unit on a flow field counts as changed too: its
///    [`FlowKey`](crate::FlowKey) names a cluster of the old layout;
/// 6. journal `"core.content_reload"`.
///
/// What a value reload leaves as it was:
/// - the [`TerrainGrid`](crate::TerrainGrid), which is match state;
/// - the [`HashInterval`](crate::HashInterval): a new `StateHash.interval_ticks` applies from
///   the next match start or load, like an edited map (R120; it is External, never hashed);
/// - the runtime components of a def component the new content no longer gives a unit: they
///   stay on live units, as a rebuild from a save keeps them too (R121; a removal hook comes
///   with the first stateful feature defs, M2).
///
/// # Errors
/// [`ReloadError`], with the match unchanged.
///
/// # Panics
/// If `world` was not built by [`build_sim_app`](crate::build_sim_app).
pub fn apply_content_values(
    world: &mut World,
    content: Arc<ContentDb>,
) -> Result<ReloadReport, ReloadError> {
    if !world.resource::<MatchHeaderRes>().0.cheats_allowed {
        return Err(ReloadError::CheatsOff);
    }
    if world.resource::<DefRegistry>().fingerprint() != content.registry().fingerprint() {
        return Err(ReloadError::RegistryMismatch);
    }
    let old = Arc::clone(&world.resource::<Content>().0);
    if !old.same_id_sets(&content) {
        return Err(ReloadError::IdSetsDiffer(old.id_set_diff(&content)));
    }

    let tick = world.resource::<SimClock>().tick;
    let clusters_changed =
        old.rules::<PathRules>().cluster_cells != content.rules::<PathRules>().cluster_cells;
    let units: Vec<(SimId, Entity)> = world.resource::<SimIndex>().iter().collect();
    let profiles: Vec<Option<MoveProfile>> = units
        .iter()
        .map(|&(_, entity)| unit_profile(world, entity))
        .collect();

    world.insert_resource(Content(Arc::clone(&content)));
    let old_rules_hash = {
        let mut header = world.resource_mut::<MatchHeaderRes>();
        std::mem::replace(&mut header.0.rules_hash, content.rules_hash())
    };
    let mut units_reloaded: u32 = 0;
    for &(_, entity) in &units {
        if reload_unit(world, &content, entity) {
            units_reloaded = units_reloaded.saturating_add(1);
        }
    }
    rebuild_derived(world);
    for (&(id, entity), old_profile) in units.iter().zip(profiles) {
        replan_if_changed(world, tick, id, entity, old_profile, clusters_changed);
    }

    let rules_hash = content.rules_hash();
    let mut journal = world.resource_mut::<Journal>();
    if journal.enabled() {
        journal.record(
            tick,
            Phase::Input,
            None,
            None,
            &ContentReloaded {
                old_rules_hash,
                rules_hash,
                units: units_reloaded,
            },
        );
    }
    tracing::info!(
        tick = tick.0,
        old_rules_hash = format_args!("{old_rules_hash:016x}"),
        rules_hash = format_args!("{rules_hash:016x}"),
        units = units_reloaded,
        "content reloaded in place"
    );
    Ok(ReloadReport {
        tick,
        old_rules_hash,
        rules_hash,
        units_reloaded,
    })
}

/// Reload the def components of `entity` from `content`'s def of its [`UnitType`], in
/// component-name order. `false` if it is no unit (no `UnitType`, or none `content` has).
fn reload_unit(world: &mut World, content: &ContentDb, entity: Entity) -> bool {
    let Some(&UnitType(unit)) = world.get::<UnitType>(entity) else {
        return false;
    };
    let Some(def) = content.unit(unit) else {
        return false;
    };
    let player = world
        .get::<Owner>(entity)
        .map_or(PlayerId(0), |owner| owner.0);
    let (pos, heading) = world
        .get::<Pose>(entity)
        .map_or((FxVec2::ZERO, Angle::ZERO), |pose| (pose.pos, pose.heading));
    let ctx = SpawnCtx {
        content,
        unit,
        player,
        pos,
        heading,
    };
    let mut entity = world.entity_mut(entity);
    for component in def.components.values() {
        component.reload(&ctx, &mut entity);
    }
    true
}

/// Re-plan the units of a just-loaded world whose flow field may be stale, with the helper of
/// [`apply_content_values`]' step 5 (Task 12 review Minor 2). A save does not record the
/// content it was made under, so with `content_changed` (the save's rules hash is not the
/// content's) the [`FlowKey`](crate::FlowKey) of every unit on a flow field may name a profile
/// the rebuilt graphs lack or a cluster of another `Path.cluster_cells` layout: in
/// [`SimIndex`] order each goes back to `Idle`, journaled `"core.order_changed"`
/// `"replanned"`, and asks for a new path next tick. Otherwise, a unit whose key names a
/// profile other than its own does. The ID-set rebuild
/// ([`LoadOptions::drop_unknown_units`](crate::LoadOptions)) always loads under changed
/// content; a load under the content the save was made with changes nothing.
pub(crate) fn replan_loaded_flow_units(world: &mut World, content_changed: bool) {
    let tick = world.resource::<SimClock>().tick;
    let units: Vec<(SimId, Entity)> = world.resource::<SimIndex>().iter().collect();
    for (id, entity) in units {
        if let Some(PathState::Flow { key }) = world.get::<PathFollow>(entity).map(|f| f.state) {
            replan_if_changed(world, tick, id, entity, Some(key.profile), content_changed);
        }
    }
}

/// Step 5 of [`apply_content_values`] for one unit, whose profile was `old` before the reload
/// (after a load, [`replan_loaded_flow_units`]: its field's profile); with `flow_stale`
/// (`Path.cluster_cells` changed, or after a load the content did) a unit on a flow field
/// counts as changed whatever its profile.
fn replan_if_changed(
    world: &mut World,
    tick: Tick,
    id: SimId,
    entity: Entity,
    old: Option<MoveProfile>,
    flow_stale: bool,
) {
    let Some(state) = world.get::<PathFollow>(entity).map(|follow| follow.state) else {
        return;
    };
    let changed = match state {
        PathState::Flow { .. } if flow_stale => true,
        PathState::Waiting { .. } | PathState::Following | PathState::Flow { .. } => {
            unit_profile(world, entity) != old
        }
        PathState::Idle | PathState::Done | PathState::Failed => false,
    };
    if !changed {
        return;
    }
    idle(world, entity);
    journal_order_changed(world, tick, Phase::Input, id, None, Reason::Replanned);
}

/// The `"core.content_reload"` journal record.
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.register_journal_record::<ContentReloaded>();
}
