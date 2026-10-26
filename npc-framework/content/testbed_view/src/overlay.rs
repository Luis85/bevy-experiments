//! F1 overlay: sim status, the command UI state and the selected unit (pose,
//! interaction, crafting, ride, needs, tree path, order queue, inventory and
//! equipment). Every part is optional; a missing resource or component
//! leaves its line out.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use bevy::{color::palettes::css, prelude::*};
use npc_framework::agent_io::ExternalController;
use npc_framework::crafting::Crafting;
use npc_framework::interaction::Interacting;
use npc_framework::needs::NEED_MAX;
use npc_framework::prelude::{
    AiRegistry, Brain, BrainState, CarriedBy, Equipment, Inventory, Item, MountedIn, NeedRegistry,
    Needs, OrderQueue, Owner, Reservations, SimClock, SimControl, SimId, SimIndex, SimPos,
    UnitOrder, Vehicle, WorldSeed,
};
use npc_framework::view_helpers::command_ui::{CommandFormation, CommandSelection, CommandUi};
use npc_framework::view_helpers::{Selected, ViewSystems, VisualIndex, sim_to_world};
use testbed_content::{Prop, TestbedIds, TestbedUnit};

use crate::anim::pose_of;
use crate::fog::{Fogged, Look, marker_pos};
use crate::visuals::ActorVisual;

/// Whether the overlay (text + item markers) is shown. Toggle with F1.
#[derive(Resource, Clone, Copy, Debug)]
pub struct OverlayState {
    pub visible: bool,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self { visible: true }
    }
}

/// Latest `state_hash`, recomputed when the tick changes.
#[derive(Resource, Default, Debug)]
struct HashCache {
    tick: Option<u64>,
    hash: Option<u64>,
}

#[derive(Component)]
struct OverlayText;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<OverlayState>()
        .init_resource::<HashCache>()
        .add_systems(Startup, (spawn_overlay, configure_gizmos))
        .add_systems(Update, toggle_overlay.in_set(ViewSystems::Input))
        .add_systems(
            Update,
            (update_hash, update_text, draw_markers)
                .chain()
                .in_set(ViewSystems::Present),
        );
}

fn spawn_overlay(mut commands: Commands) {
    commands.spawn((
        Name::new("overlay"),
        OverlayText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.95, 0.9)),
        BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.72)),
        Node {
            position_type: PositionType::Absolute,
            top: px(10),
            left: px(10),
            max_width: px(470),
            padding: UiRect::all(px(8)),
            ..default()
        },
    ));
}

/// Gizmos draw on top of the models and a bit thicker.
fn configure_gizmos(store: Option<ResMut<GizmoConfigStore>>) {
    let Some(mut store) = store else { return };
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -1.0;
    config.line.width = 2.5;
}

fn toggle_overlay(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut state: ResMut<OverlayState>,
    mut texts: Query<&mut Visibility, With<OverlayText>>,
) {
    if keys.is_some_and(|k| k.just_pressed(KeyCode::F1)) {
        state.visible = !state.visible;
    }
    let want = if state.visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut texts {
        if *v != want {
            *v = want;
        }
    }
}

fn update_hash(world: &mut World) {
    let visible = world
        .get_resource::<OverlayState>()
        .is_some_and(|s| s.visible);
    let installed = world.contains_resource::<WorldSeed>()
        && world.contains_resource::<SimIndex>()
        && world.contains_resource::<Reservations>();
    let tick = world.get_resource::<SimClock>().map(|c| c.tick);
    let Some(tick) = tick.filter(|_| visible && installed) else {
        return;
    };
    if world
        .get_resource::<HashCache>()
        .is_some_and(|c| c.tick == Some(tick))
    {
        return;
    }
    let hash = npc_framework::kernel::hash::state_hash(world);
    if let Some(mut cache) = world.get_resource_mut::<HashCache>() {
        cache.tick = Some(tick);
        cache.hash = Some(hash);
    }
}

/// `SimId` → testbed name (`TestbedIds`, reversed).
pub(crate) fn names(ids: Option<&TestbedIds>) -> BTreeMap<SimId, String> {
    ids.map(|ids| ids.names.iter().map(|(n, id)| (*id, n.clone())).collect())
        .unwrap_or_default()
}

pub(crate) fn label(names: &BTreeMap<SimId, String>, id: SimId) -> String {
    match names.get(&id) {
        Some(n) => format!("#{} {n}", id.0),
        None => format!("#{}", id.0),
    }
}

pub(crate) fn metres(mm: i32) -> String {
    format!("{:.1}", mm as f32 / 1000.0)
}

fn pos_str(p: (i32, i32)) -> String {
    format!("({}, {}) m", metres(p.0), metres(p.1))
}

/// One order, briefly.
pub(crate) fn describe_order(names: &BTreeMap<SimId, String>, order: &UnitOrder) -> String {
    match order {
        UnitOrder::Move { pos } => format!("Move {}", pos_str(*pos)),
        UnitOrder::AttackMove { pos } => format!("AttackMove {}", pos_str(*pos)),
        UnitOrder::Stop => "Stop".into(),
        UnitOrder::Hold => "Hold".into(),
        UnitOrder::Follow { target } => format!("Follow {}", label(names, *target)),
        UnitOrder::Patrol { points } => format!("Patrol {} pts", points.len()),
        UnitOrder::Interact { verb, target } => format!("{verb} {}", label(names, *target)),
        UnitOrder::Mount { vehicle } => format!("Mount {}", label(names, *vehicle)),
        UnitOrder::Unload => "Unload".into(),
        UnitOrder::Custom { name, .. } => name.clone(),
    }
}

fn item_str(item: &Item) -> String {
    let mut s = item.kind.clone();
    if item.stack > 1 {
        let _ = write!(s, " x{}", item.stack);
    }
    if item.quality != 0 {
        let _ = write!(s, " q{:+}", item.quality);
    }
    s
}

type UnitRow<'a> = (
    &'a SimId,
    &'a SimPos,
    Option<&'a TestbedUnit>,
    Option<&'a Owner>,
    (Option<&'a Brain>, Option<&'a BrainState>, Option<&'a Needs>),
    (
        Option<&'a OrderQueue>,
        Option<&'a Interacting>,
        Option<&'a Crafting>,
        Option<&'a MountedIn>,
    ),
    (
        Option<&'a Inventory>,
        Option<&'a Equipment>,
        Option<&'a ExternalController>,
        Option<&'a Vehicle>,
    ),
);

#[allow(clippy::too_many_arguments)]
fn update_text(
    state: Res<OverlayState>,
    (clock, speed, hash): (
        Option<Res<SimClock>>,
        Option<Res<SimControl>>,
        Res<HashCache>,
    ),
    (ui, sel, formation): (
        Option<Res<CommandUi>>,
        Option<Res<CommandSelection>>,
        Option<Res<CommandFormation>>,
    ),
    selected: Option<Res<Selected>>,
    (index, registry, needs_registry, ids): (
        Option<Res<SimIndex>>,
        Option<Res<AiRegistry>>,
        Option<Res<NeedRegistry>>,
        Option<Res<TestbedIds>>,
    ),
    units: Query<UnitRow>,
    items: Query<&Item>,
    vis_index: Res<npc_framework::view_helpers::VisualIndex>,
    actors: Query<&ActorVisual>,
    mut texts: Query<&mut Text, With<OverlayText>>,
) {
    if !state.visible {
        return;
    }
    let names = names(ids.as_deref());
    let mut s = String::new();
    match clock.as_deref() {
        Some(c) => {
            let _ = write!(s, "tick {}", c.tick);
        }
        None => s.push_str("tick -"),
    }
    match speed.as_deref() {
        Some(sp) if sp.paused => s.push_str("   PAUSED"),
        Some(sp) => {
            let _ = write!(s, "   speed {}x", sp.multiplier);
        }
        None => {}
    }
    match hash.hash {
        Some(h) => {
            let _ = writeln!(s, "   hash {h:016x}");
        }
        None => s.push('\n'),
    }
    let player = ui.as_deref().map_or(0, |u| u.player.0);
    let form = formation.as_deref().and_then(|f| f.0).map_or_else(
        || "default".to_owned(),
        |f| format!("{:?} {} m", f.kind, metres(f.spacing_mm)),
    );
    let count = sel.as_deref().map_or(0, |s| s.units.len());
    let _ = writeln!(s, "player {player}   formation {form}   selected {count}");

    let selected = selected.and_then(|s| s.0);
    let row = selected
        .and_then(|id| index.as_deref()?.get(id))
        .and_then(|e| units.get(e).ok().map(|r| (e, r)));
    match row {
        None => s.push_str("\n(click a unit; drag to box-select)\n"),
        Some((sim, (id, pos, unit, owner, (brain, bs, needs), orders, kit))) => {
            let (queue, interacting, crafting, mounted) = orders;
            let (inventory, equipment, controller, vehicle) = kit;
            let what = unit.map_or_else(
                || {
                    if vehicle.is_some() {
                        "vehicle".into()
                    } else {
                        "?".into()
                    }
                },
                |u| format!("{} / {:?}", u.template, u.behaviour),
            );
            let _ = writeln!(
                s,
                "\n> {}  {what}  player {}",
                label(&names, *id),
                owner.map_or(0, |o| o.0.0)
            );
            let moving = vis_index
                .map
                .get(&sim)
                .and_then(|v| actors.get(*v).ok())
                .is_some_and(|a| a.motion.moving);
            let pose = pose_of(mounted, interacting, crafting, moving);
            let _ = writeln!(
                s,
                "  pose {}   at {}",
                pose.label(),
                pos_str((pos.x, pos.z))
            );
            if let Some(c) = controller {
                let _ = writeln!(s, "  agent {} ({:?})", c.id, c.mode);
            }
            if let (Some(i), Some(c)) = (interacting, clock.as_deref()) {
                let total = i.ends.saturating_sub(i.started).max(1);
                let done = c.tick.saturating_sub(i.started).min(total);
                let _ = writeln!(
                    s,
                    "  doing {} {}  {done}/{total}",
                    i.verb,
                    label(&names, i.target)
                );
            }
            if let Some(c) = crafting {
                let at = c
                    .workstation
                    .map_or_else(|| "hand".into(), |w| label(&names, w));
                let _ = writeln!(s, "  crafting {} @ {at}", c.recipe);
            }
            if let Some(m) = mounted {
                let _ = writeln!(s, "  riding {} seat {}", label(&names, m.vehicle), m.seat);
            }
            if let (Some(n), Some(reg)) = (needs, needs_registry.as_deref()) {
                let mut line = String::from("  needs");
                for (i, need) in n.values.iter().enumerate() {
                    let name = reg
                        .defs
                        .get(i)
                        .map_or_else(|| "?".to_owned(), |d| d.name.to_string());
                    let pct = need.value as f32 / NEED_MAX as f32 * 100.0;
                    let _ = write!(line, "  {name} {pct:.0}%");
                }
                let _ = writeln!(s, "{line}");
            }
            if let (Some(brain), Some(bs), Some(reg)) = (brain, bs, registry.as_deref()) {
                let path = reg
                    .trees
                    .get(brain.tree.0 as usize)
                    .filter(|t| {
                        bs.running_path
                            .iter()
                            .all(|&i| (i as usize) < t.nodes.len())
                    })
                    .map(|t| t.path_string(&bs.running_path))
                    .filter(|p| !p.is_empty());
                let _ = writeln!(s, "  tree  {}", path.as_deref().unwrap_or("-"));
            }
            if let Some(q) = queue {
                let orders: Vec<String> = q.orders().map(|o| describe_order(&names, o)).collect();
                if orders.is_empty() {
                    s.push_str("  orders -\n");
                } else {
                    let _ = writeln!(s, "  orders {}", orders.join(" > "));
                }
            }
            if let Some(eq) = equipment {
                let slots: Vec<String> = eq
                    .slots
                    .iter()
                    .map(|(slot, id)| {
                        let what = id
                            .and_then(|id| index.as_deref()?.get(id))
                            .and_then(|e| items.get(e).ok())
                            .map_or_else(|| "-".into(), item_str);
                        format!("{slot}: {what}")
                    })
                    .collect();
                let _ = writeln!(s, "  equip {}", slots.join("  "));
            }
            if let Some(inv) = inventory {
                let worn: Vec<SimId> = equipment
                    .map(|e| e.slots.values().flatten().copied().collect())
                    .unwrap_or_default();
                let list: Vec<String> = inv
                    .items
                    .iter()
                    .filter(|id| !worn.contains(id))
                    .filter_map(|id| index.as_deref()?.get(*id))
                    .filter_map(|e| items.get(e).ok())
                    .map(item_str)
                    .collect();
                let _ = writeln!(
                    s,
                    "  inv ({}/{}) {}",
                    inv.items.len(),
                    inv.slots,
                    if list.is_empty() {
                        "-".into()
                    } else {
                        list.join(", ")
                    }
                );
            }
        }
    }
    s.push_str(
        "\n[F1] overlay [F3] journal [F4] perf [F5] sheet [F6] perception\n\
         [LMB] select/box [Shift] add/queue [RMB] order [Ctrl+1-9] group\n\
         [Z/X/C/V] Line/Box/Wedge/default [H] hold [B] stop [U] unload\n\
         [Space] pause [.] step [-/=] slower/faster\n\
         [WASD] pan [Q/E] orbit [PgUp/PgDn] tilt [wheel] zoom [F] focus",
    );
    for mut text in &mut texts {
        if text.0 != s {
            text.0.clone_from(&s);
        }
    }
}

/// Small rings under items lying in the world (they are tiny at RTS zoom).
/// A gold ring under every item lying on the ground. Under fog of war the
/// ring follows the item's look (Ruling R15, [`marker_pos`]): none when its
/// visual is hidden, at the last-seen position when drawn as last seen.
fn draw_markers(
    mut gizmos: Gizmos,
    state: Res<OverlayState>,
    items: Query<(Entity, &SimPos), (With<Item>, Without<CarriedBy>, Without<Prop>)>,
    index: Option<Res<VisualIndex>>,
    looks: Query<&Fogged>,
) {
    if !state.visible {
        return;
    }
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    for (sim, pos) in &items {
        let look = index
            .as_ref()
            .and_then(|i| i.map.get(&sim))
            .and_then(|v| looks.get(*v).ok())
            .map_or(Look::Live, |f| f.0);
        let Some(at) = marker_pos(look, *pos) else {
            continue;
        };
        gizmos.circle(
            Isometry3d::new(sim_to_world(at) + Vec3::Y * 0.04, flat),
            0.32,
            css::GOLD,
        );
    }
}
