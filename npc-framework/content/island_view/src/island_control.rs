//! Native island input and control authority. Desired mode is host state; control
//! changes are recorded commands and reconcile only after their receipts settle.
//! This module never constructs widgets or mutates authoritative actor state.
use crate::player_actions::{self, ControlState};
use bevy::prelude::*;
use island_content::village::VillageState;
use npc_framework::kernel::commands::{CommandQueue, SkippedCommands};
use npc_framework::prelude::{Owner, SimClock, SimCommand, SimControl, SimId, SimIndex, SimPos};
use npc_framework::view_helpers::command_ui::{
    CommandBindings, CommandSelection, CommandUi, NpcCommandUiSet, select_units,
};
use npc_framework::view_helpers::{Selected, ViewHidden, ViewSystems};

const CHARACTER_READY: &str = "WASD moves · E interacts · choose an activity from the toolbar.";

/// Host presentation mode; never saved, hashed, or consulted by simulation systems.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IslandMode {
    #[default]
    Inspector,
    Character,
    Editor,
}

#[derive(Resource, Default)]
struct CharacterWalk {
    last_tick: Option<u64>,
    moving: bool,
}

/// A host-side receipt, scoped to the exact actor/world instance that accepted it.
/// The selected IslandMode is the latest desired mode; this records only the
/// control change already in flight, never a second source of simulation authority.
#[derive(Clone, Copy)]
struct PendingControl {
    key: (u64, u32),
    actor: (SimId, Entity, u64),
}

struct PointerContext {
    actor: SimId,
    previous: Option<CommandUi>,
}

#[derive(Resource, Default)]
struct IslandControl {
    pending: Option<PendingControl>,
    /// Scope the player's intent as well as its receipt, so reset/new life
    /// never silently claims a different actor after the old receipt settled.
    actor: Option<(SimId, Entity, u64)>,
    pointer: Option<PointerContext>,
    select_when_ready: bool,
    message: String,
    revision: u64,
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<IslandMode>()
        .init_resource::<IslandControl>()
        .init_resource::<CharacterWalk>()
        .add_systems(Startup, initialize)
        .add_systems(Update, character_walk.in_set(ViewSystems::Input))
        .add_systems(
            Update,
            prepare_pointer_input
                .in_set(ViewSystems::Input)
                .before(NpcCommandUiSet),
        )
        .add_systems(
            Update,
            reconcile
                .in_set(ViewSystems::Present)
                .after(player_actions::tick),
        );
}
fn initialize(world: &mut World) {
    if world
        .get_resource::<crate::FrontendGate>()
        .is_some_and(|gate| gate.0)
    {
        return;
    }
    let mode = *world.resource::<IslandMode>();
    if let Err(error) = request_mode(world, mode) {
        set_status(world, error);
    }
}
fn reconcile(world: &mut World) {
    if world
        .get_resource::<crate::FrontendGate>()
        .is_some_and(|gate| gate.0)
        && world.resource::<IslandControl>().pending.is_none()
    {
        return;
    }
    if let Err(error) = reconcile_control(world) {
        set_status(world, error);
    }
}
pub(crate) fn clear_view(world: &mut World) {
    if world.contains_resource::<IslandControl>() {
        set_mode(world, IslandMode::Inspector);
        *world.resource_mut::<IslandControl>() = IslandControl::default();
        *world.resource_mut::<CharacterWalk>() = CharacterWalk::default();
    }
}
/// Clear only the view's held-key latch. Physical movement remains command owned.
pub(crate) fn clear_navigation_input(world: &mut World) -> bool {
    world
        .get_resource_mut::<CharacterWalk>()
        .is_some_and(|mut input| {
            let moving = input.moving;
            *input = CharacterWalk::default();
            moving
        })
}
pub(crate) fn control_pending(world: &World) -> bool {
    world
        .get_resource::<IslandControl>()
        .is_some_and(|state| state.pending.is_some())
}
pub(crate) fn pending_receipt(world: &World) -> Option<(u64, u32)> {
    let actor = control_actor(world)?;
    world
        .get_resource::<IslandControl>()
        .and_then(|state| state.pending)
        .filter(|pending| {
            pending.actor == actor
                && world
                    .get_resource::<CommandQueue>()
                    .is_some_and(|queue| queue.pending.contains_key(&pending.key))
        })
        .map(|pending| pending.key)
        .or_else(|| queued_control_receipt(world, actor.0))
}
fn queued_control_receipt(world: &World, actor: SimId) -> Option<(u64, u32)> {
    // A restored snapshot retains commands, not the view's receipt. Recover only
    // this identity's native ownership changes; foreign controllers stay foreign.
    world
        .get_resource::<CommandQueue>()?
        .pending
        .iter()
        .rev()
        .find_map(|(key, command)| {
            let SimCommand::Content { kind, payload } = command else {
                return None;
            };
            let relevant = kind == "JoinIsland"
                || (matches!(kind.as_str(), "AttachController" | "DetachController")
                    && payload
                        .get("controller")
                        .and_then(serde_json::Value::as_str)
                        == Some(player_actions::NATIVE_CONTROLLER));
            (relevant && command.unit_in(world) == Some(actor)).then_some(*key)
        })
}
pub(crate) fn status(world: &World) -> Option<(u64, &str)> {
    world
        .get_resource::<IslandControl>()
        .map(|state| (state.revision, state.message.as_str()))
}
fn set_status(world: &mut World, message: impl Into<String>) {
    let message = message.into();
    let mut state = world.resource_mut::<IslandControl>();
    if state.message == message {
        return;
    }
    state.message = message.clone();
    state.revision = state.revision.wrapping_add(1);
    world
        .get_resource_or_init::<player_actions::NativeActions>()
        .feedback = Some(message);
}
fn set_mode(world: &mut World, mode: IslandMode) {
    if mode != IslandMode::Character {
        let context = world.resource_mut::<IslandControl>().pointer.take();
        world.resource_mut::<IslandControl>().select_when_ready = false;
        if let Some(context) = context {
            if let Some(previous) = context.previous {
                world.insert_resource(previous);
            } else {
                world.remove_resource::<CommandUi>();
            }
            if let Some(mut selected) = world.get_resource_mut::<CommandSelection>() {
                selected.units.clear();
            }
            if let Some(mut selected) = world.get_resource_mut::<Selected>()
                && selected.0 == Some(context.actor)
            {
                selected.0 = None;
            }
        }
    }
    *world.resource_mut::<IslandMode>() = mode;
    world.resource_mut::<CharacterWalk>().moving = false;
}

/// Shared entry point for ledger buttons and Home & farm. It records latest
/// intent immediately and never duplicates a pending arrival/attach/detach.
pub(crate) fn request_mode(world: &mut World, mode: IslandMode) -> Result<(), String> {
    if mode == IslandMode::Character {
        if matches!(player_actions::control(world), ControlState::Foreign { .. }) {
            return Err(
                "Another controller owns the hero; native input will not take it over".into(),
            );
        }
        if !player_actions::available(world) {
            return Err("This life is unavailable; choose a new life before playing".into());
        }
    }
    let actor = control_actor(world);
    {
        let mut state = world.resource_mut::<IslandControl>();
        state.actor = actor;
        if state
            .pending
            .is_some_and(|pending| Some(pending.actor) != actor)
        {
            state.pending = None;
        }
    }
    if mode != IslandMode::Character
        && player_actions::control(world) == ControlState::Native
        && let Some(key) = player_actions::prepare_suspend(world)?
        && let Some(actor) = actor
    {
        world.resource_mut::<IslandControl>().pending = Some(PendingControl { key, actor });
    }
    set_mode(world, mode);
    if mode == IslandMode::Character {
        world.resource_mut::<IslandControl>().select_when_ready = true;
    }
    let native_ready = player_actions::control(world) == ControlState::Native;
    set_status(
        world,
        match mode {
            IslandMode::Inspector => {
                "Inspect residents and set colony policies while the village lives on."
            }
            IslandMode::Character if native_ready => CHARACTER_READY,
            IslandMode::Character => {
                "Joining the island. WASD walks; F2 opens work, food and trade. Resume if paused."
            }
            IslandMode::Editor => "Click land to paint. Resume the world to apply changes.",
        },
    );
    reconcile_control(world)
}

fn character_walk(world: &mut World) {
    if *world.resource::<IslandMode>() != IslandMode::Character
        || control_pending(world)
        || player_actions::control(world) != ControlState::Native
        || world.resource::<SimControl>().is_stopped()
    {
        return;
    }
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    // Shortcut chords belong to the host/UI, including a complete Ctrl+S tap
    // whose modifier was released before this frame's input systems run.
    let modifiers = [
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ];
    if keys.any_pressed(modifiers) || keys.any_just_pressed(modifiers) {
        return;
    }
    let fresh = keys.any_just_pressed([KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD]);
    let down = |key| keys.pressed(key) || keys.just_pressed(key);
    let axis = Vec2::new(
        i32::from(down(KeyCode::KeyD)) as f32 - i32::from(down(KeyCode::KeyA)) as f32,
        i32::from(down(KeyCode::KeyS)) as f32 - i32::from(down(KeyCode::KeyW)) as f32,
    );
    let now = world.resource::<SimClock>().tick;
    if axis == Vec2::ZERO {
        if world.resource::<CharacterWalk>().moving {
            // A tap can begin and end before one simulation tick. Let its
            // recorded order reach the executor/nav pipeline before cancelling.
            if world
                .resource::<CharacterWalk>()
                .last_tick
                .is_some_and(|tick| now >= tick && now - tick < 3)
            {
                return;
            }
            let _ = player_actions::stop(world);
            world.resource_mut::<CharacterWalk>().moving = false;
        }
        return;
    }
    if !fresh
        && world
            .resource::<CharacterWalk>()
            .last_tick
            .is_some_and(|last| now >= last && now - last < 6)
    {
        return;
    }
    let yaw = world
        .query::<&crate::RtsCamera>()
        .iter(world)
        .next()
        .map_or(0.0, |camera| camera.yaw);
    let direction = Quat::from_rotation_y(yaw) * Vec3::new(axis.x, 0.0, axis.y).normalize();
    let actor = world.resource::<VillageState>().played;
    let Some(position) = world
        .resource::<SimIndex>()
        .get(actor)
        .and_then(|entity| world.get::<npc_framework::prelude::SimPos>(entity))
        .copied()
    else {
        return;
    };
    // Only the input waypoint is quantized here. The authoritative mover and pathfinder decide motion.
    let target = npc_framework::prelude::SimPos::new(
        position
            .x
            .saturating_add((direction.x * 700.0).round() as i32),
        position
            .z
            .saturating_add((direction.z * 700.0).round() as i32),
    );
    if let Err(error) = player_actions::walk(world, target) {
        set_status(world, error.clone());
        world
            .resource_mut::<player_actions::NativeActions>()
            .feedback = Some(error);
    }
    let mut movement = world.resource_mut::<CharacterWalk>();
    movement.last_tick = Some(now);
    movement.moving = true;
}

/// Pointer orders use the same settled actor as WASD. Selection is established
/// once on entry/reveal, and refreshed only for an explicit order gesture so a
/// harmless click on empty ground cannot disable character movement.
fn select_character(world: &mut World) -> Result<bool, String> {
    let (id, entity, _) = control_actor(world).ok_or("The character is no longer present")?;
    if world
        .get_resource::<ViewHidden>()
        .is_some_and(|hidden| hidden.hides(entity))
    {
        return Ok(false);
    }
    let owner = world
        .get::<Owner>(entity)
        .copied()
        .ok_or("The character cannot accept movement orders")?;
    if world.resource::<IslandControl>().pointer.is_none() {
        let previous = world.get_resource::<CommandUi>().copied();
        world.resource_mut::<IslandControl>().pointer = Some(PointerContext {
            actor: id,
            previous,
        });
    } else if let Some(context) = world.resource_mut::<IslandControl>().pointer.as_mut() {
        context.actor = id;
    }
    {
        let mut ui = world.get_resource_or_init::<CommandUi>();
        ui.player = owner.0;
        ui.enabled = true;
    }
    select_units(world, [id], false);
    world.get_resource_or_init::<Selected>().0 = Some(id);
    world.resource_mut::<IslandControl>().select_when_ready = false;
    Ok(true)
}

fn prepare_pointer_input(world: &mut World) {
    if *world.resource::<IslandMode>() != IslandMode::Character {
        return;
    }
    let order = world
        .get_resource::<CommandBindings>()
        .map_or(MouseButton::Right, |bindings| bindings.order);
    if !world
        .get_resource::<ButtonInput<MouseButton>>()
        .is_some_and(|buttons| buttons.just_pressed(order))
    {
        return;
    }
    let ready = !control_pending(world) && player_actions::control(world) == ControlState::Native;
    let selected = if ready {
        select_character(world)
    } else {
        Ok(false)
    };
    if let Err(error) = &selected {
        set_status(world, error);
    }
    if matches!(selected, Ok(true))
        && world.contains_resource::<island_content::village::frontier::terrain::TerrainState>()
        && !world
            .query_filtered::<&bevy::picking::hover::Hovered, With<Node>>()
            .iter(world)
            .any(bevy::picking::hover::Hovered::get)
    {
        let cursor = world
            .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
            .single(world)
            .ok()
            .and_then(Window::cursor_position);
        let ray = cursor.and_then(|cursor| {
            world
                .query_filtered::<(&Camera, &GlobalTransform), With<Camera3d>>()
                .single(world)
                .ok()
                .and_then(|(camera, transform)| camera.viewport_to_world(transform, cursor).ok())
        });
        if let Some(ray) = ray
            && npc_framework::view_helpers::command_ui::pick_unit(world, ray).is_none()
        {
            let hit = world
                .get_resource::<island_content::village::frontier::terrain::TerrainState>()
                .and_then(|terrain| crate::colony_scene::living_scene::raycast(terrain, ray));
            if let Some(hit) = hit {
                let position = SimPos::new(
                    (hit.x * 1000.0).round() as i32,
                    (hit.z * 1000.0).round() as i32,
                );
                if let Err(error) = player_actions::walk(world, position) {
                    set_status(world, error);
                }
            }
            // Prevent the generic y=0 plane resolver from issuing a second order.
            if let Some(mut buttons) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
                buttons.clear_just_pressed(order);
            }
        }
    }
    if !matches!(selected, Ok(true))
        && let Some(mut buttons) = world.get_resource_mut::<ButtonInput<MouseButton>>()
    {
        buttons.clear_just_pressed(order);
    }
}

fn control_actor(world: &World) -> Option<(SimId, Entity, u64)> {
    let actor = world.get_resource::<VillageState>()?.played;
    let entity = world.get_resource::<SimIndex>()?.get(actor)?;
    Some((actor, entity, npc_framework::agent_io::api::epoch(world)))
}

fn waiting_control_message(world: &mut World) {
    let mode = match world.resource::<IslandMode>() {
        IslandMode::Inspector => "inspection",
        IslandMode::Character => "character control",
        IslandMode::Editor => "landscape editing",
    };
    set_status(
        world,
        format!("Switching to {mode}. Resume the world to finish switching."),
    );
}

fn reconcile_control(world: &mut World) -> Result<(), String> {
    let actor = control_actor(world);
    if world
        .resource::<IslandControl>()
        .actor
        .is_some_and(|previous| Some(previous) != actor)
    {
        let mut state = world.resource_mut::<IslandControl>();
        state.actor = actor;
        state.pending = None;
        set_mode(world, IslandMode::Inspector);
        return Err(
            "The active life or world changed. Choose Play character again to control it.".into(),
        );
    }
    let mut settled = false;
    if world.resource::<IslandControl>().pending.is_none()
        && let Some(actor) = actor
        && let Some(key) = queued_control_receipt(world, actor.0)
    {
        world.resource_mut::<IslandControl>().pending = Some(PendingControl { key, actor });
    }
    if let Some(pending) = world.resource::<IslandControl>().pending {
        if control_actor(world) != Some(pending.actor) {
            world.resource_mut::<IslandControl>().pending = None;
            set_mode(world, IslandMode::Inspector);
            return Err(
                "The active life or world changed. Choose Play character again to control it."
                    .into(),
            );
        }
        if world
            .get_resource::<CommandQueue>()
            .is_some_and(|queue| queue.pending.contains_key(&pending.key))
        {
            waiting_control_message(world);
            return Ok(());
        }
        world.resource_mut::<IslandControl>().pending = None;
        let failure = world.get_resource::<SkippedCommands>().and_then(|skipped| {
            skipped
                .entries
                .iter()
                .rev()
                .find(|entry| (entry.tick, entry.seq) == pending.key)
                .map(|entry| entry.error.to_string())
        });
        if let Some(error) = failure {
            let actual = if player_actions::control(world) == ControlState::Native {
                IslandMode::Character
            } else {
                IslandMode::Inspector
            };
            set_mode(world, actual);
            return Err(player_actions::refusal_message(world, &error));
        }
        settled = true;
    }

    let desired = *world.resource::<IslandMode>();
    let actual = player_actions::control(world);
    let request = if desired == IslandMode::Character {
        match actual {
            ControlState::Native => None,
            ControlState::Free => Some(player_actions::take_control(world)),
            ControlState::Foreign { .. } | ControlState::Unavailable => {
                set_mode(world, IslandMode::Inspector);
                return Err("Character control is unavailable. The current controller and life are unchanged.".into());
            }
        }
    } else if actual == ControlState::Native {
        if player_actions::committed_work(world) {
            set_status(
                world,
                "Your current activity continues while you inspect. Character control releases when it finishes.",
            );
            return Ok(());
        }
        Some(player_actions::release(world))
    } else {
        None
    };
    if let Some(request) = request {
        let response = request?;
        let tick = response["applies_at"]
            .as_u64()
            .ok_or("Control request returned no simulation tick")?;
        let seq = response["command_seq"]
            .as_u64()
            .and_then(|seq| u32::try_from(seq).ok())
            .ok_or("Control request returned no command receipt")?;
        let actor = control_actor(world).ok_or("The active character no longer exists")?;
        world.resource_mut::<IslandControl>().pending = Some(PendingControl {
            key: (tick, seq),
            actor,
        });
        waiting_control_message(world);
    } else {
        if desired == IslandMode::Character && world.resource::<IslandControl>().select_when_ready {
            select_character(world)?;
        }
        if !settled {
            return Ok(());
        }
        let message = if desired == IslandMode::Character {
            CHARACTER_READY
        } else if desired == IslandMode::Editor {
            "Landscape editor ready. Choose a brush, then click land to paint."
        } else {
            "You are inspecting. The village continues independently."
        };
        set_status(world, message);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_content::village::Villager;
    use npc_framework::kernel::{commands::CommandLog, hash::state_hash};
    use npc_framework::prelude::run_ticks;
    fn fixture() -> App {
        let mut app = island_content::new_sim_app(7, "living_world");
        app.init_resource::<IslandMode>()
            .init_resource::<IslandControl>()
            .init_resource::<CharacterWalk>()
            .init_resource::<SimControl>()
            .init_resource::<player_actions::NativeActions>();
        app
    }
    fn step(app: &mut App) {
        run_ticks(app.world_mut(), 1);
        player_actions::tick(app.world_mut());
        reconcile_control(app.world_mut()).unwrap();
    }
    fn pending_count(app: &App, kind: &str) -> usize {
        app.world()
            .resource::<CommandQueue>()
            .pending
            .values()
            .filter(|command| command.kind() == kind)
            .count()
    }
    fn applied_count(app: &App, kind: &str) -> usize {
        app.world()
            .resource::<CommandLog>()
            .entries
            .iter()
            .filter(|(_, _, command)| command.kind() == kind)
            .count()
    }

    fn cottage_fixture() -> (App, SimId, Entity, Entity) {
        use island_content::village::{economy, homestead};
        use npc_framework::prelude::*;
        let mut app = fixture();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        let id = app.world().resource::<VillageState>().played;
        let actor = app.world().resource::<SimIndex>().get(id).unwrap();
        let (plot_id, plot) = homestead::plots(app.world())
            .into_iter()
            .find(|(_, entity)| {
                homestead::definition(app.world(), *entity)
                    .unwrap()
                    .use_kind
                    == "housing"
                    && app
                        .world()
                        .get::<homestead::HomesteadPlot>(*entity)
                        .unwrap()
                        .owner
                        .is_none()
            })
            .unwrap();
        let at = homestead::front_door(*app.world().get::<SimPos>(plot).unwrap());
        app.world_mut().entity_mut(actor).insert(at);
        homestead::HomesteadAct::new(id, "claim_plot", plot_id)
            .perform(app.world_mut())
            .unwrap();
        // Isolate authority transfer from gathering; the full native rehearsal
        // separately obtains all materials through player actions.
        economy::grant(app.world_mut(), actor, Item::new("plank").stack(8));
        economy::grant(app.world_mut(), actor, Item::new("log").stack(4));
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&homestead::HomesteadAct::new(id, "build_cottage", plot_id)),
        )
        .unwrap();
        (app, id, actor, plot)
    }

    #[test]
    fn inspecting_and_editing_preserve_queued_and_running_cottage_until_latest_mode_releases() {
        use island_content::village::{economy, homestead};
        use npc_framework::prelude::Reservations;
        let (mut app, id, actor, plot) = cottage_fixture();
        request_mode(app.world_mut(), IslandMode::Inspector).unwrap();
        assert_eq!(pending_count(&app, "DetachController"), 0);
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
        step(&mut app);
        let build = app
            .world()
            .get::<homestead::CottageBuild>(actor)
            .unwrap()
            .clone();
        request_mode(app.world_mut(), IslandMode::Editor).unwrap();
        for _ in 0..30 {
            step(&mut app);
        }
        assert_eq!(
            app.world().get::<homestead::CottageBuild>(actor),
            Some(&build)
        );
        assert_eq!(player_actions::control(app.world()), ControlState::Native);
        for material in &build.materials {
            assert!(
                app.world()
                    .resource::<Reservations>()
                    .holds(material.item, id)
            );
        }
        // A newer character choice cancels the deferred release, not the work.
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        for _ in 0..10 {
            step(&mut app);
        }
        assert_eq!(applied_count(&app, "DetachController"), 0);
        request_mode(app.world_mut(), IslandMode::Inspector).unwrap();
        while app.world().resource::<SimClock>().tick <= build.ends + 2 {
            step(&mut app);
        }
        assert!(
            app.world()
                .get::<homestead::HomesteadPlot>(plot)
                .unwrap()
                .cottage_built
        );
        assert_eq!(economy::count(app.world(), actor, "plank"), 0);
        assert_eq!(economy::count(app.world(), actor, "log"), 0);
        assert_eq!(player_actions::control(app.world()), ControlState::Free);
        assert_eq!(applied_count(&app, "DetachController"), 1);
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
    }

    #[test]
    fn regional_restoration_survives_mode_changes_before_and_after_application() {
        use island_content::village::{
            ResourcePatch, catalog, economy,
            frontier::{
                activities::{EcologyWork, FrontierAct},
                ecology::EcologicalPatch,
            },
        };
        use npc_framework::prelude::*;
        let mut app = island_content::new_sim_app(7, "living_world");
        app.init_resource::<IslandMode>()
            .init_resource::<IslandControl>()
            .init_resource::<CharacterWalk>()
            .init_resource::<SimControl>()
            .init_resource::<player_actions::NativeActions>();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        let id = app.world().resource::<VillageState>().played;
        let actor = app.world().resource::<SimIndex>().get(id).unwrap();
        let (patch, site) = app
            .world()
            .resource::<SimIndex>()
            .map
            .iter()
            .find(|(_, e)| {
                app.world()
                    .get::<ResourcePatch>(**e)
                    .is_some_and(|p| p.kind == "log")
            })
            .map(|(id, e)| (*id, *e))
            .unwrap();
        let position = *app.world().get::<SimPos>(site).unwrap();
        app.world_mut().entity_mut(actor).insert(position);
        app.world_mut()
            .get_mut::<EcologicalPatch>(site)
            .unwrap()
            .biomass_milli = 50000;
        economy::grant(app.world_mut(), actor, Item::new("log"));
        economy::grant(app.world_mut(), actor, Item::new(catalog::WATER));
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&FrontierAct {
                unit: id,
                target: patch,
                verb: "plant_tree".into(),
            }),
        )
        .unwrap();
        request_mode(app.world_mut(), IslandMode::Inspector).unwrap();
        assert_eq!(pending_count(&app, "DetachController"), 0);
        step(&mut app);
        let work = app.world().get::<EcologyWork>(actor).unwrap().clone();
        request_mode(app.world_mut(), IslandMode::Editor).unwrap();
        assert_eq!(
            player_actions::prepare_suspend(app.world_mut()).unwrap(),
            None
        );
        for claim in &work.materials {
            assert!(app.world().resource::<Reservations>().holds(claim.item, id));
        }
        while app.world().resource::<SimClock>().tick <= work.ends + 2 {
            step(&mut app);
        }
        assert_eq!(
            app.world()
                .get::<EcologicalPatch>(site)
                .unwrap()
                .planted_units,
            1
        );
        assert!(app.world().get::<EcologyWork>(actor).is_none());
        assert_eq!(player_actions::control(app.world()), ControlState::Free);
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Editor);
    }

    #[test]
    fn explicit_stop_during_inspection_cancels_work_and_returns_reserved_materials() {
        use island_content::village::{economy, homestead};
        let (mut app, _, actor, plot) = cottage_fixture();
        step(&mut app);
        request_mode(app.world_mut(), IslandMode::Inspector).unwrap();
        assert!(app.world().get::<homestead::CottageBuild>(actor).is_some());
        player_actions::stop(app.world_mut()).unwrap();
        for _ in 0..3 {
            step(&mut app);
        }
        assert!(app.world().get::<homestead::CottageBuild>(actor).is_none());
        assert!(
            !app.world()
                .get::<homestead::HomesteadPlot>(plot)
                .unwrap()
                .cottage_built
        );
        assert_eq!(economy::count(app.world(), actor, "plank"), 8);
        assert_eq!(economy::count(app.world(), actor, "log"), 4);
        assert_eq!(player_actions::control(app.world()), ControlState::Free);
    }

    #[test]
    fn crafting_finishes_under_retained_native_authority_before_inspector_detach() {
        use island_content::village::{catalog, economy};
        use npc_framework::agent_io::{
            api::ActStep,
            control::{AttachController, ControlMode, IdlePolicy},
        };
        use npc_framework::crafting::Crafting;
        use npc_framework::prelude::*;
        let mut app = fixture();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        let id = app.world().resource::<VillageState>().played;
        let actor = app.world().resource::<SimIndex>().get(id).unwrap();
        let station = app
            .world_mut()
            .query::<(&SimId, &island_content::village::VillagePlace)>()
            .iter(app.world())
            .filter(|(_, place)| place.role == island_content::village::Role::Carpenter)
            .map(|(id, _)| *id)
            .min()
            .unwrap();
        let carpenter = app
            .world()
            .resource::<island_content::IslandIds>()
            .id("Jun");
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&AttachController {
                unit: carpenter.0,
                controller: "fixture-carpenter".into(),
                mode: ControlMode::Direct,
                on_idle: IdlePolicy::Idle,
                force: false,
            }),
        )
        .unwrap();
        step(&mut app);
        let station_entity = app.world().resource::<SimIndex>().get(station).unwrap();
        let at = *app.world().get::<SimPos>(station_entity).unwrap();
        app.world_mut().entity_mut(actor).insert(at);
        economy::grant(app.world_mut(), actor, Item::new("log"));
        for _ in 0..32 {
            step(&mut app);
        }
        player_actions::act(
            app.world_mut(),
            ActStep {
                verb: Some(format!("craft:{}", catalog::SAW)),
                target: Some(station.0),
                ..default()
            },
        )
        .unwrap();
        request_mode(app.world_mut(), IslandMode::Inspector).unwrap();
        assert_eq!(pending_count(&app, "DetachController"), 0);
        for _ in 0..3 {
            step(&mut app);
        }
        let craft = app.world().get::<Crafting>(actor).unwrap().clone();
        assert_eq!(player_actions::control(app.world()), ControlState::Native);
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        assert_eq!(app.world().get::<Crafting>(actor), Some(&craft));
        request_mode(app.world_mut(), IslandMode::Editor).unwrap();
        while app.world().resource::<SimClock>().tick <= craft.ends + 3 {
            step(&mut app);
        }
        assert!(app.world().get::<Crafting>(actor).is_none());
        assert_eq!(economy::count(app.world(), actor, "plank"), 4);
        assert_eq!(economy::count(app.world(), actor, "log"), 0);
        assert_eq!(player_actions::control(app.world()), ControlState::Free);
    }

    #[test]
    fn title_is_quiet_and_restored_pending_join_is_adopted_without_duplicate_commands() {
        let mut app = fixture();
        app.world_mut().insert_resource(crate::FrontendGate(true));
        *app.world_mut().resource_mut::<IslandMode>() = IslandMode::Character;
        initialize(app.world_mut());
        reconcile(app.world_mut());
        assert_eq!(pending_count(&app, "JoinIsland"), 0);
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        let receipt = pending_receipt(app.world()).unwrap();
        assert_eq!(
            player_actions::prepare_suspend(app.world_mut()).unwrap(),
            Some(receipt)
        );
        // A saved command queue survives replacement of host-only coordinator state.
        clear_view(app.world_mut());
        assert!(!control_pending(app.world()));
        assert_eq!(pending_receipt(app.world()), Some(receipt));
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        assert_eq!(pending_count(&app, "JoinIsland"), 1);
        assert_eq!(pending_count(&app, "AttachController"), 1);
        app.world_mut().resource_mut::<crate::FrontendGate>().0 = false;
        step(&mut app);
        assert_eq!(player_actions::control(app.world()), ControlState::Native);
        assert_eq!(applied_count(&app, "JoinIsland"), 1);
        assert_eq!(applied_count(&app, "AttachController"), 1);
    }

    #[test]
    fn paused_character_then_inspector_settles_without_a_native_controller() {
        let mut app = fixture();
        app.world_mut().resource_mut::<SimControl>().paused = true;
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        request_mode(app.world_mut(), IslandMode::Inspector).unwrap();
        for _ in 0..4 {
            reconcile_control(app.world_mut()).unwrap();
        }
        assert_eq!(pending_count(&app, "JoinIsland"), 1);
        assert_eq!(pending_count(&app, "AttachController"), 1);
        assert_eq!(player_actions::control(app.world()), ControlState::Free);
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
        app.world_mut().resource_mut::<SimControl>().paused = false;
        step(&mut app);
        assert!(
            control_pending(app.world()),
            "the correcting release is recorded for the next tick"
        );
        step(&mut app);
        assert_eq!(player_actions::control(app.world()), ControlState::Free);
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
        assert!(!control_pending(app.world()));
        assert_eq!(applied_count(&app, "JoinIsland"), 1);
        assert_eq!(applied_count(&app, "DetachController"), 1);
    }

    #[test]
    fn paused_release_then_character_reacquires_once_without_rejoining() {
        let mut app = fixture();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        app.world_mut().resource_mut::<SimControl>().paused = true;
        request_mode(app.world_mut(), IslandMode::Inspector).unwrap();
        for _ in 0..4 {
            request_mode(app.world_mut(), IslandMode::Character).unwrap();
        }
        assert_eq!(pending_count(&app, "DetachController"), 1);
        assert_eq!(pending_count(&app, "JoinIsland"), 0);
        assert_eq!(
            pending_count(&app, "AttachController"),
            0,
            "wait for the existing release receipt"
        );
        app.world_mut().resource_mut::<SimControl>().paused = false;
        step(&mut app);
        assert!(control_pending(app.world()));
        step(&mut app);
        assert_eq!(player_actions::control(app.world()), ControlState::Native);
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Character);
        assert!(!control_pending(app.world()));
        assert_eq!(applied_count(&app, "JoinIsland"), 1);
        assert_eq!(applied_count(&app, "AttachController"), 2);
    }

    #[test]
    fn repeated_paused_play_requests_share_one_arrival_and_attachment() {
        let mut app = fixture();
        app.world_mut().resource_mut::<SimControl>().paused = true;
        for _ in 0..8 {
            request_mode(app.world_mut(), IslandMode::Character).unwrap();
        }
        assert_eq!(pending_count(&app, "JoinIsland"), 1);
        assert_eq!(pending_count(&app, "AttachController"), 1);
        assert!(status(app.world()).unwrap().1.contains("Resume the world"));
        app.world_mut().resource_mut::<SimControl>().paused = false;
        step(&mut app);
        assert_eq!(player_actions::control(app.world()), ControlState::Native);
        assert!(!control_pending(app.world()));
        // Continue clears host presentation while keeping the saved native owner.
        clear_view(app.world_mut());
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        let resumed = status(app.world()).unwrap().1;
        assert!(
            resumed.contains("WASD") && !resumed.contains("Joining") && !resumed.contains("Resume")
        );
        assert_eq!(pending_count(&app, "JoinIsland"), 0);
        assert_eq!(pending_count(&app, "AttachController"), 0);
        app.world_mut()
            .resource_mut::<player_actions::NativeActions>()
            .feedback = Some("Building your cottage…".into());
        for _ in 0..4 {
            reconcile_control(app.world_mut()).unwrap();
        }
        assert_eq!(
            app.world()
                .resource::<player_actions::NativeActions>()
                .feedback
                .as_deref(),
            Some("Building your cottage…")
        );
    }

    #[test]
    fn settled_control_intent_does_not_cross_a_world_epoch() {
        let mut app = fixture();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        // Host epoch isolates the reset boundary; no authoritative actor state
        // is modified to manufacture a different control result.
        app.world_mut()
            .resource_mut::<npc_framework::agent_io::api::AgentIoState>()
            .epoch += 1;
        assert!(reconcile_control(app.world_mut()).is_err());
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
        assert_eq!(pending_count(&app, "JoinIsland"), 0);
        assert_eq!(pending_count(&app, "AttachController"), 0);
    }

    #[test]
    fn an_external_controller_winning_the_tick_is_not_reclaimed() {
        use npc_framework::agent_io::control::{AttachController, ControlMode, IdlePolicy};
        use npc_framework::prelude::{SimCommand, submit};
        let mut app = fixture();
        let id = app.world().resource::<VillageState>().played;
        submit(
            app.world_mut(),
            None,
            SimCommand::content(&AttachController {
                unit: id.0,
                controller: "external-player".into(),
                mode: ControlMode::Direct,
                on_idle: IdlePolicy::Idle,
                force: false,
            }),
        )
        .unwrap();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        run_ticks(app.world_mut(), 1);
        assert!(reconcile_control(app.world_mut()).is_err());
        for _ in 0..4 {
            reconcile_control(app.world_mut()).unwrap();
        }
        assert_eq!(
            player_actions::control(app.world()),
            ControlState::Foreign {
                controller: "external-player".into()
            }
        );
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Inspector);
        assert_eq!(pending_count(&app, "AttachController"), 0);
    }
    #[test]
    fn initial_character_mode_records_arrival_before_native_attachment() {
        let mut app = fixture();
        let id = app.world().resource::<VillageState>().played;
        let entity = app.world().resource::<SimIndex>().get(id).unwrap();
        assert!(
            app.world()
                .get::<island_content::village::colony::AwaitingArrival>(entity)
                .is_some()
        );
        let before = state_hash(app.world_mut());
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        assert_eq!(*app.world().resource::<IslandMode>(), IslandMode::Character);
        assert_eq!(
            before,
            state_hash(app.world_mut()),
            "starting play only records commands"
        );
        run_ticks(app.world_mut(), 2);
        player_actions::tick(app.world_mut());
        reconcile_control(app.world_mut()).unwrap();
        assert!(
            app.world()
                .get::<island_content::village::colony::AwaitingArrival>(entity)
                .is_none()
        );
        assert!(app.world().get::<Villager>(entity).is_some());
        assert_eq!(player_actions::control(app.world()), ControlState::Native);
        let kinds: Vec<_> = app
            .world()
            .resource::<CommandLog>()
            .entries
            .iter()
            .map(|(_, _, command)| command.kind())
            .collect();
        let arrival = kinds.iter().position(|kind| *kind == "JoinIsland").unwrap();
        let attach = kinds
            .iter()
            .position(|kind| *kind == "AttachController")
            .unwrap();
        assert!(
            arrival < attach,
            "arrival precedes control in the recorded log"
        );
    }

    #[test]
    fn character_entry_selects_hero_owner_for_ground_orders_and_restores_context_on_exit() {
        use npc_framework::prelude::{PlayerId, SimPos, UnitOrder};
        use npc_framework::view_helpers::command_ui::{
            RightClick, RightClickKind, issue_to_selection, right_click_order,
        };
        let mut app = fixture();
        let previous = CommandUi {
            player: PlayerId(77),
            enabled: true,
            gizmos: false,
        };
        app.insert_resource(previous)
            .init_resource::<ButtonInput<MouseButton>>();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        let id = app.world().resource::<VillageState>().played;
        let entity = app.world().resource::<SimIndex>().get(id).unwrap();
        let owner = app.world().get::<Owner>(entity).unwrap().0;
        assert_eq!(app.world().resource::<CommandSelection>().units, vec![id]);
        assert_eq!(app.world().resource::<CommandUi>().player, owner);
        assert_eq!(app.world().resource::<Selected>().0, Some(id));
        // The generic left-click picker can clear its RTS selection while the
        // character remains controlled. A new order gesture restores only him.
        app.world_mut()
            .resource_mut::<CommandSelection>()
            .units
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        prepare_pointer_input(app.world_mut());
        let units = app.world().resource::<CommandSelection>().units.clone();
        assert_eq!(units, vec![id]);
        let start = *app.world().get::<SimPos>(entity).unwrap();
        let destination = SimPos::new(start.x, start.z - 2500);
        let order = right_click_order(
            app.world(),
            RightClickKind::Move,
            &RightClick {
                player: owner,
                selection: &units,
                target: None,
                ground: Some(destination),
            },
        )
        .unwrap();
        assert_eq!(
            order,
            UnitOrder::Move {
                pos: (destination.x, destination.z)
            }
        );
        assert!(issue_to_selection(app.world_mut(), order, false).unwrap());
        assert_eq!(pending_count(&app, "IssueOrder"), 1);
        for _ in 0..50 {
            step(&mut app);
        }
        assert!(app.world().get::<SimPos>(entity).unwrap().z < start.z);
        request_mode(app.world_mut(), IslandMode::Inspector).unwrap();
        assert_eq!(*app.world().resource::<CommandUi>(), previous);
        assert!(app.world().resource::<CommandSelection>().units.is_empty());
    }

    #[test]
    fn initial_selection_waits_for_arrival_to_be_visible_without_reselecting_every_frame() {
        let mut app = fixture();
        let id = app.world().resource::<VillageState>().played;
        let entity = app.world().resource::<SimIndex>().get(id).unwrap();
        app.insert_resource(ViewHidden([entity].into_iter().collect()));
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        assert!(
            app.world()
                .get_resource::<CommandSelection>()
                .is_none_or(|selection| selection.units.is_empty())
        );
        app.world_mut()
            .resource_mut::<ViewHidden>()
            .0
            .remove(&entity);
        reconcile_control(app.world_mut()).unwrap();
        assert_eq!(app.world().resource::<CommandSelection>().units, vec![id]);
        app.world_mut()
            .resource_mut::<CommandSelection>()
            .units
            .clear();
        reconcile_control(app.world_mut()).unwrap();
        assert!(
            app.world().resource::<CommandSelection>().units.is_empty(),
            "an explicit pointer order, not every frame, restores a cleared selection"
        );
    }

    #[test]
    fn a_press_and_release_in_one_frame_moves_before_the_release_stop() {
        use npc_framework::prelude::SimPos;
        let mut app = fixture();
        app.init_resource::<ButtonInput<KeyCode>>();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        let id = app.world().resource::<VillageState>().played;
        let entity = app.world().resource::<SimIndex>().get(id).unwrap();
        let start = *app.world().get::<SimPos>(entity).unwrap();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            keys.release(KeyCode::KeyW);
        }
        character_walk(app.world_mut());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        let queued = app.world().resource::<CommandQueue>().pending.len();
        character_walk(app.world_mut());
        assert_eq!(
            app.world().resource::<CommandQueue>().pending.len(),
            queued,
            "release cannot cancel before a tick has run"
        );
        for _ in 0..3 {
            step(&mut app);
            character_walk(app.world_mut());
        }
        assert!(
            app.world().get::<SimPos>(entity).unwrap().z < start.z,
            "a completed input tap must not disappear between frames"
        );
        step(&mut app);
        let stopped = *app.world().get::<SimPos>(entity).unwrap();
        for _ in 0..12 {
            step(&mut app);
            character_walk(app.world_mut());
        }
        assert_eq!(*app.world().get::<SimPos>(entity).unwrap(), stopped);
    }

    #[test]
    fn a_fresh_direction_tap_is_not_dropped_by_the_held_key_repeat_interval() {
        use npc_framework::prelude::SimPos;
        let mut app = fixture();
        app.init_resource::<ButtonInput<KeyCode>>();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        let entity = app
            .world()
            .resource::<SimIndex>()
            .get(app.world().resource::<VillageState>().played)
            .unwrap();
        let start = *app.world().get::<SimPos>(entity).unwrap();
        for key in [KeyCode::KeyW, KeyCode::KeyD] {
            {
                let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                keys.press(key);
                keys.release(key);
            }
            character_walk(app.world_mut());
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
            step(&mut app);
        }
        for _ in 0..3 {
            step(&mut app);
            character_walk(app.world_mut());
        }
        assert!(
            app.world().get::<SimPos>(entity).unwrap().x > start.x,
            "fresh D tap must override the previous W waypoint inside six ticks"
        );
    }

    #[test]
    fn same_frame_ctrl_s_shortcut_never_becomes_a_backward_walk() {
        use npc_framework::prelude::SimPos;
        let mut app = fixture();
        app.init_resource::<ButtonInput<KeyCode>>();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        step(&mut app);
        let entity = app
            .world()
            .resource::<SimIndex>()
            .get(app.world().resource::<VillageState>().played)
            .unwrap();
        let start = *app.world().get::<SimPos>(entity).unwrap();
        let before = app.world().resource::<CommandQueue>().pending.len();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::KeyS);
            keys.release(KeyCode::KeyS);
            keys.release(KeyCode::ControlLeft);
        }
        character_walk(app.world_mut());
        assert_eq!(app.world().resource::<CommandQueue>().pending.len(), before);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        for _ in 0..6 {
            step(&mut app);
            character_walk(app.world_mut());
        }
        assert_eq!(*app.world().get::<SimPos>(entity).unwrap(), start);
    }

    #[test]
    fn character_wasd_moves_the_actor_and_key_release_stops_without_moving_camera() {
        let mut app = fixture();
        app.init_resource::<ButtonInput<KeyCode>>();
        let camera = crate::RtsCamera {
            yaw: 0.0,
            ..default()
        };
        let camera_entity = app.world_mut().spawn((camera, camera.transform())).id();
        request_mode(app.world_mut(), IslandMode::Character).unwrap();
        run_ticks(app.world_mut(), 2);
        player_actions::tick(app.world_mut());
        reconcile_control(app.world_mut()).unwrap();
        let id = app.world().resource::<VillageState>().played;
        let entity = app.world().resource::<SimIndex>().get(id).unwrap();
        let start = *app
            .world()
            .get::<npc_framework::prelude::SimPos>(entity)
            .unwrap();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..4 {
            character_walk(app.world_mut());
            run_ticks(app.world_mut(), 6);
            player_actions::tick(app.world_mut());
            reconcile_control(app.world_mut()).unwrap();
        }
        let walked = *app
            .world()
            .get::<npc_framework::prelude::SimPos>(entity)
            .unwrap();
        assert!(
            walked.z < start.z,
            "W must move toward the top of the camera view: {start:?} -> {walked:?}"
        );
        assert_eq!(
            *app.world().get::<crate::RtsCamera>(camera_entity).unwrap(),
            camera
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        character_walk(app.world_mut());
        run_ticks(app.world_mut(), 2);
        let stopped = *app
            .world()
            .get::<npc_framework::prelude::SimPos>(entity)
            .unwrap();
        run_ticks(app.world_mut(), 12);
        assert_eq!(
            *app.world()
                .get::<npc_framework::prelude::SimPos>(entity)
                .unwrap(),
            stopped
        );
    }
}
