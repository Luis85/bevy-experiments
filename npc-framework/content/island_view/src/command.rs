//! RTS command UI wiring: `npc_view_helpers::command_ui` (click/box
//! selection, Ctrl+1–9 groups, right-click context orders with the glue's
//! item/door/vehicle resolvers, shift-queue, order gizmos), plus the
//! testbed's own resolver (a tree or the ore rock → `use_tool_on`), the
//! formation hotkeys and the Hold/Stop/Unload keys. Everything ends in
//! recorded `IssueOrder` commands.

use bevy::prelude::*;
use island_content::IslandUnit;
use npc_framework::interaction::builtin::USE_TOOL_ON;
use npc_framework::prelude::{
    Formation, FormationKind, IssueOrder, MountedIn, PlayerId, SimId, SimIndex, ToolTarget,
    UnitOrder, Vehicle, submit,
};
use npc_framework::view_helpers::command_ui::{
    CommandFormation, CommandSelection, CommandUi, CommandUiPlugin, ContextOrders,
};
use npc_framework::view_helpers::{Selected, ViewSystems};

/// Formation spacing of the hotkeys, mm.
pub const FORMATION_SPACING_MM: i32 = 1500;

pub(crate) fn plugin(app: &mut App) {
    if !app.is_plugin_added::<CommandUiPlugin>() {
        app.add_plugins(CommandUiPlugin);
    }
    app.world_mut()
        .get_resource_or_init::<ContextOrders>()
        .add("testbed.use_tool", context_tool_order);
    // The order keys are developer keys: in the player session H, B, U, Z,
    // X, C and V mean other things (or nothing) and never order the played
    // character. Click and right-click orders stay.
    app.add_systems(
        Update,
        order_keys
            .in_set(ViewSystems::Input)
            .run_if(crate::player_hud::developer_keys),
    )
    .add_systems(Update, inspect_target.in_set(ViewSystems::Animate));
}

/// Right-click resolver: a tree or the ore rock with uses left →
/// `use_tool_on` (the unit needs the right tool equipped; the order fails
/// with the interaction's reason otherwise).
pub fn context_tool_order(world: &World, _: PlayerId, target: Entity) -> Option<UnitOrder> {
    let id = *world.get::<SimId>(target)?;
    let t = world.get::<ToolTarget>(target)?;
    (t.remaining != Some(0)).then(|| UnitOrder::Interact {
        verb: USE_TOOL_ON.to_owned(),
        target: id,
    })
}

/// Z / X / C: Line / Box / Wedge formation for group moves, V: the sim's
/// default. H: Hold, B: Stop, U: Unload (vehicles), for the selection.
fn order_keys(world: &mut World) {
    let Some(keys) = world.get_resource::<ButtonInput<KeyCode>>().cloned() else {
        return;
    };
    let formation = [
        (KeyCode::KeyZ, Some(FormationKind::Line)),
        (KeyCode::KeyX, Some(FormationKind::Box)),
        (KeyCode::KeyC, Some(FormationKind::Wedge)),
        (KeyCode::KeyV, None),
    ];
    for (key, kind) in formation {
        if keys.just_pressed(key) {
            world.get_resource_or_init::<CommandFormation>().0 =
                kind.map(|k| Formation::new(k, FORMATION_SPACING_MM));
        }
    }
    let order = if keys.just_pressed(KeyCode::KeyH) {
        UnitOrder::Hold
    } else if keys.just_pressed(KeyCode::KeyB) {
        UnitOrder::Stop
    } else if keys.just_pressed(KeyCode::KeyU) {
        UnitOrder::Unload
    } else {
        return;
    };
    let ui = world
        .get_resource::<CommandUi>()
        .copied()
        .unwrap_or_default();
    let mut units = world
        .get_resource::<CommandSelection>()
        .map(|s| s.units.clone())
        .unwrap_or_default();
    if order == UnitOrder::Unload {
        // Unload is a vehicle order: send it to the vehicles selected or
        // ridden by the selection.
        let index = world.resource::<SimIndex>();
        let mut vehicles: Vec<SimId> = units
            .iter()
            .filter_map(|id| {
                let e = index.get(*id)?;
                if world.get::<Vehicle>(e).is_some() {
                    Some(*id)
                } else {
                    world.get::<MountedIn>(e).map(|m| m.vehicle)
                }
            })
            .collect();
        vehicles.sort_unstable();
        vehicles.dedup();
        units = vehicles;
    }
    if units.is_empty() {
        return;
    }
    let cmd = IssueOrder::new(ui.player, units, order).to_command();
    if let Err(err) = submit(world, None, cmd) {
        warn!("order rejected: {err}");
    }
}

/// The panels inspect actors and vehicles. A click that landed on a door,
/// an item or a tree (selectable for right-click orders) falls back to the
/// first commanded unit.
fn inspect_target(
    sims: Option<Res<SimIndex>>,
    sel: Res<CommandSelection>,
    mut selected: ResMut<Selected>,
    inspectable: Query<(), Or<(With<IslandUnit>, With<Vehicle>)>>,
) {
    let Some(id) = selected.0 else { return };
    let ok = sims
        .as_deref()
        .and_then(|s| s.get(id))
        .is_some_and(|e| inspectable.contains(e));
    if !ok {
        let want = sel.units.first().copied();
        if selected.0 != want {
            selected.0 = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeveloperKeys, PlayerExperience};
    use bevy::ecs::system::RunSystemOnce;

    fn queued(app: &App) -> usize {
        app.world()
            .get_resource::<npc_framework::kernel::commands::CommandQueue>()
            .map_or(0, |queue| queue.pending.len())
    }

    /// Press H, B, U, Z, X, C and V one at a time; the orders recorded.
    fn press_all(app: &mut App) -> usize {
        let before = queued(app);
        for key in [
            KeyCode::KeyH,
            KeyCode::KeyB,
            KeyCode::KeyU,
            KeyCode::KeyZ,
            KeyCode::KeyX,
            KeyCode::KeyC,
            KeyCode::KeyV,
        ] {
            {
                let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                input.reset_all();
                input.press(key);
            }
            // The plugin's run condition, then the system.
            let allowed = app
                .world_mut()
                .run_system_once(crate::player_hud::developer_keys)
                .unwrap();
            if allowed {
                app.world_mut().run_system_once(order_keys).unwrap();
            }
        }
        queued(app) - before
    }

    #[test]
    fn order_hotkeys_do_nothing_in_the_player_session() {
        let mut app = island_content::new_sim_app(7, "living_world");
        app.init_resource::<ButtonInput<KeyCode>>();
        let played = app
            .world()
            .resource::<island_content::village::VillageState>()
            .played;
        app.world_mut()
            .get_resource_or_init::<CommandSelection>()
            .units = vec![played];
        app.insert_resource(PlayerExperience { enabled: true })
            .insert_resource(DeveloperKeys(false));
        assert_eq!(press_all(&mut app), 0, "no order from the player session");
        assert!(
            app.world()
                .get_resource::<CommandFormation>()
                .is_none_or(|f| f.0.is_none()),
            "no formation was chosen either"
        );
        // With the developer keys the formation keys act again.
        app.insert_resource(DeveloperKeys(true));
        press_all(&mut app);
        assert!(
            app.world().get_resource::<CommandFormation>().is_some(),
            "the developer keys still reach the orders"
        );
    }
}
