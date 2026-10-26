//! F1 debug overlay: text panel plus gizmos. Every part is optional; a
//! missing resource or component just leaves that line out.

use std::fmt::Write as _;

use bevy::{color::palettes::css, prelude::*};
use npc_framework::needs::NEED_MAX;
use npc_framework::prelude::{
    AiRegistry, Approach, BbValue, Blackboard, Brain, BrainState, NeedLevel, NeedRegistry, Needs,
    Reservations, SimClock, SimConfig, SimControl, SimId, SimIndex, SimPos, WorldSeed,
};
use npc_framework::view_helpers::{Selected, ViewSystems, VisualIndex, VisualOf, sim_to_world};
use village_content::needs::VillagerNeeds;
use village_content::villager::{BB_DEST, BB_TARGET};
use village_content::{Activity, Stockpile, Villager};

/// Whether the overlay (text + gizmos) is shown. Toggle with F1.
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
            (update_hash, update_text, draw_gizmos)
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
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.95, 0.9)),
        BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.72)),
        Node {
            position_type: PositionType::Absolute,
            top: px(10),
            left: px(10),
            padding: UiRect::all(px(10)),
            ..default()
        },
    ));
}

/// Overlay gizmos draw on top of the models (reserved targets such as the
/// field would otherwise hide their circles) and a bit thicker.
fn configure_gizmos(store: Option<ResMut<GizmoConfigStore>>) {
    let Some(mut store) = store else { return };
    let (config, _) = store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -1.0;
    config.line.width = 3.0;
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

/// `state_hash` needs the full sim (`NpcPlugins` installs `WorldSeed`,
/// `SimIndex` and `Reservations`); without it the hash line shows "-".
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

fn level(l: NeedLevel) -> &'static str {
    match l {
        NeedLevel::Ok => "ok",
        NeedLevel::Low => "LOW",
        NeedLevel::Urgent => "URGENT",
    }
}

fn pct(value: i32) -> f32 {
    value as f32 / NEED_MAX as f32 * 100.0
}

fn bar(value: i32) -> String {
    let filled = ((pct(value) / 10.0).round() as i32).clamp(0, 10) as usize;
    format!("[{}{}]", "#".repeat(filled), ".".repeat(10 - filled))
}

fn bb(value: BbValue) -> String {
    match value {
        BbValue::None => "-".into(),
        BbValue::U32(v) => format!("#{v}"),
        BbValue::I32(v) => format!("{v}"),
        BbValue::Pos(x, z) => format!("({:.2}, {:.2}) m", x as f32 / 1000.0, z as f32 / 1000.0),
    }
}

type VillagerRow<'a> = (
    Option<&'a SimId>,
    Option<&'a Needs>,
    Option<&'a Activity>,
    Option<&'a Brain>,
    Option<&'a BrainState>,
    Option<&'a Blackboard>,
);

#[allow(clippy::too_many_arguments)]
fn update_text(
    state: Res<OverlayState>,
    clock: Option<Res<SimClock>>,
    config: Option<Res<SimConfig>>,
    speed: Option<Res<SimControl>>,
    needs_registry: Option<Res<NeedRegistry>>,
    hash: Res<HashCache>,
    selected: Option<Res<Selected>>,
    registry: Option<Res<AiRegistry>>,
    reservations: Option<Res<Reservations>>,
    stockpiles: Query<(Option<&SimId>, &Stockpile)>,
    villagers: Query<VillagerRow, With<Villager>>,
    mut texts: Query<&mut Text, With<OverlayText>>,
) {
    if !state.visible {
        return;
    }
    let mut s = String::new();

    if let Some(clock) = clock.as_deref() {
        let _ = write!(s, "tick {}", clock.tick);
        if let Some(cfg) = config.as_deref().filter(|c| c.day_length > 0) {
            // The day starts at 06:00; with the default 30 % night, night is 22:48-06:00.
            let minutes = ((clock.tick % cfg.day_length) * 1440 / cfg.day_length + 360) % 1440;
            let night = if cfg.is_night(clock.tick) {
                "  night"
            } else {
                ""
            };
            let _ = write!(
                s,
                "   day {}  {:02}:{:02}{night}",
                cfg.day(clock.tick) + 1,
                minutes / 60,
                minutes % 60
            );
        }
        s.push('\n');
    } else {
        s.push_str("tick -  (no SimClock)\n");
    }
    match speed.as_deref() {
        Some(sp) if sp.paused => s.push_str("speed PAUSED\n"),
        Some(sp) => {
            let _ = writeln!(s, "speed {}x", sp.multiplier);
        }
        None => s.push_str("speed -\n"),
    }
    match hash.hash {
        Some(h) => {
            let _ = writeln!(s, "hash  {h:016x}");
        }
        None => s.push_str("hash  -\n"),
    }
    let selected = selected.and_then(|sel| sel.0);
    match selected {
        Some(id) => {
            let _ = writeln!(s, "selected #{}", id.0);
        }
        None => s.push_str("selected -  (click a villager)\n"),
    }

    let mut piles: Vec<_> = stockpiles.iter().collect();
    piles.sort_by_key(|(id, _)| id.copied());
    for (id, pile) in piles {
        let id = id.map_or_else(|| "?".into(), |i| format!("#{}", i.0));
        let _ = writeln!(s, "stockpile {id}  food {}/{}", pile.food, pile.capacity);
    }

    let mut rows: Vec<_> = villagers.iter().collect();
    rows.sort_by_key(|row| row.0.copied());
    for (id, needs, activity, brain, brain_state, blackboard) in rows {
        let mark = if id.is_some() && id.copied() == selected {
            "> "
        } else {
            ""
        };
        let id = id.map_or_else(|| "?".into(), |i| format!("#{}", i.0));
        let activity = activity.map_or_else(|| "-".into(), |a| format!("{a:?}"));
        let _ = writeln!(s, "\n{mark}villager {id}  {activity}");
        let needs = needs.zip(needs_registry.as_deref()).and_then(|(n, reg)| {
            let get = |name| {
                reg.id(name)
                    .and_then(|id| n.values.get(id.0 as usize).copied())
            };
            Some(VillagerNeeds {
                hunger: get(village_content::HUNGER)?,
                energy: get(village_content::ENERGY)?,
                thirst: get(village_content::THIRST)?,
            })
        });
        if let Some(n) = needs {
            let _ = writeln!(
                s,
                "  hunger {} {:5.1}% {}\n  energy {} {:5.1}% {}\n  thirst {} {:5.1}% {}",
                bar(n.hunger.value),
                pct(n.hunger.value),
                level(n.hunger.level),
                bar(n.energy.value),
                pct(n.energy.value),
                level(n.energy.level),
                bar(n.thirst.value),
                pct(n.thirst.value),
                level(n.thirst.level),
            );
        }
        if let (Some(brain), Some(bs), Some(reg)) = (brain, brain_state, registry.as_deref()) {
            let path = reg
                .trees
                .get(brain.tree.0 as usize)
                .filter(|t| {
                    bs.running_path
                        .iter()
                        .all(|&i| (i as usize) < t.nodes.len())
                })
                .map(|t| t.path_string(&bs.running_path));
            match path {
                Some(p) if !p.is_empty() => {
                    let _ = writeln!(s, "  tree   {p}");
                }
                _ => s.push_str("  tree   -\n"),
            }
        }
        if let Some(b) = blackboard {
            let _ = writeln!(
                s,
                "  target {}   dest {}",
                bb(b.get(BB_TARGET)),
                bb(b.get(BB_DEST))
            );
        }
    }

    if let Some(res) = reservations.as_deref() {
        s.push_str("\nreservations:");
        if res.claims.values().all(Vec::is_empty) {
            s.push_str(" none");
        }
        for (target, by) in &res.claims {
            if by.is_empty() {
                continue;
            }
            let by: Vec<_> = by.iter().map(|b| format!("#{}", b.0)).collect();
            let _ = write!(s, "  #{} <- {}", target.0, by.join(","));
        }
        s.push('\n');
    }
    s.push_str(
        "\n[F1] overlay [F3] journal [F4] perf\n[Space] pause [.] step [1-4] 1x/3x/10x/0x\n[WASD] pan [Q/E] rotate [wheel] zoom [click] select",
    );

    for mut text in &mut texts {
        if text.0 != s {
            text.0.clone_from(&s);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_gizmos(
    mut gizmos: Gizmos,
    state: Res<OverlayState>,
    index: Res<VisualIndex>,
    reservations: Option<Res<Reservations>>,
    positions: Query<(&SimId, &SimPos)>,
    approaches: Query<&Approach>,
    villagers: Query<(Entity, &SimPos, Option<&Blackboard>), With<Villager>>,
    transforms: Query<&GlobalTransform, With<VisualOf>>,
) {
    if !state.visible {
        return;
    }
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    let lift = Vec3::Y * 0.05;

    for approach in &approaches {
        gizmos.circle(
            Isometry3d::new(sim_to_world(approach.0) + lift, flat),
            0.25,
            css::LIGHT_SKY_BLUE,
        );
    }

    for (sim, pos, blackboard) in &villagers {
        let from = index
            .map
            .get(&sim)
            .and_then(|v| transforms.get(*v).ok())
            .map_or(sim_to_world(*pos), |t| t.translation());
        if let Some(BbValue::Pos(x, z)) = blackboard.map(|b| b.get(BB_DEST)) {
            let dest = sim_to_world(SimPos::new(x, z)) + lift;
            gizmos.line(from.with_y(0.0) + lift, dest, css::YELLOW);
            gizmos.circle(Isometry3d::new(dest, flat), 0.35, css::YELLOW);
        }
    }

    if let Some(res) = reservations.as_deref() {
        for (target, by) in &res.claims {
            if by.is_empty() {
                continue;
            }
            if let Some((_, pos)) = positions.iter().find(|(id, _)| *id == target) {
                gizmos.circle(
                    Isometry3d::new(sim_to_world(*pos) + lift, flat),
                    1.6,
                    css::ORANGE_RED,
                );
            }
        }
    }
}
