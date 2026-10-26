//! F5 character sheet and F6 perception panels for the selected unit, and
//! the F6 gizmos (its vision cone, line-of-sight rays to what it senses,
//! markers where it remembers things, the last noise it heard). With F6 on
//! and nothing selected, the generic all-agents perception gizmos show.

use std::fmt::Write as _;

use bevy::{color::palettes::css, prelude::*};
use npc_framework::perception::{SenseKind, Sensor};
use npc_framework::prelude::{
    CharacterSheet, Experience, Facing, Perceivable, PerceptionConfig, PerceptionMemory, Sensors,
    SimClock, SimIndex, SimPos, SpendPolicy,
};
use npc_framework::progression::ProgressionConfig;
use npc_framework::rules::{RuleSet, Rules, RulesRegistry};
use npc_framework::view_helpers::perception_gizmos::PerceptionGizmos;
use npc_framework::view_helpers::{Selected, ViewSystems, VisualIndex, VisualOf, sim_to_world};
use testbed_content::TestbedIds;
use testbed_content::world::kinds;

use crate::overlay::{label, metres, names};

/// Which testbed panels are shown (F5 sheet, F6 perception). Insert it
/// before the plugin to choose the initial state.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TestbedPanels {
    pub sheet: bool,
    pub perception: bool,
    /// F7 diary and proposals (explorer mode only).
    pub explorer: bool,
}

#[derive(Component)]
struct SheetText;

#[derive(Component)]
struct PerceptionText;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<TestbedPanels>()
        .add_systems(Startup, spawn_panels)
        .add_systems(Update, toggle_panels.in_set(ViewSystems::Input))
        .add_systems(
            Update,
            (sheet_text, perception_text, perception_gizmos).in_set(ViewSystems::Present),
        );
}

fn panel_text(marker: impl Component) -> impl Bundle {
    (
        marker,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::srgb(0.93, 0.95, 0.9)),
        BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.8)),
        Node {
            padding: UiRect::all(px(8)),
            max_width: px(560),
            display: Display::None,
            ..default()
        },
    )
}

fn spawn_panels(mut commands: Commands) {
    commands
        .spawn((
            Name::new("testbed panels"),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(10),
                right: px(10),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: px(6),
                ..default()
            },
        ))
        .with_children(|p| {
            p.spawn((Name::new("perception panel"), panel_text(PerceptionText)));
            p.spawn((Name::new("sheet panel"), panel_text(SheetText)));
        });
}

fn toggle_panels(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut panels: ResMut<TestbedPanels>,
    selected: Res<Selected>,
    mut generic: Option<ResMut<PerceptionGizmos>>,
    mut nodes: Query<(&mut Node, Has<SheetText>, Has<PerceptionText>)>,
) {
    if let Some(keys) = keys {
        if keys.just_pressed(KeyCode::F5) {
            panels.sheet = !panels.sheet;
        }
        if keys.just_pressed(KeyCode::F6) {
            panels.perception = !panels.perception;
        }
    }
    for (mut node, sheet, perception) in &mut nodes {
        let on = (sheet && panels.sheet) || (perception && panels.perception);
        if !(sheet || perception) {
            continue;
        }
        let want = if on { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    if let Some(g) = generic.as_deref_mut() {
        let want = panels.perception && selected.0.is_none();
        if g.enabled != want {
            g.enabled = want;
        }
    }
}

fn set_text(texts: &mut Query<&mut Text, impl bevy::ecs::query::QueryFilter>, s: &str) {
    for mut t in texts.iter_mut() {
        if t.0 != s {
            t.0 = s.to_owned();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sheet_text(
    panels: Res<TestbedPanels>,
    selected: Res<Selected>,
    index: Option<Res<SimIndex>>,
    rules: (Option<Res<RuleSet>>, Option<Res<RulesRegistry>>),
    progression: Option<Res<ProgressionConfig>>,
    ids: Option<Res<TestbedIds>>,
    units: Query<(
        Option<&CharacterSheet>,
        Option<&Experience>,
        Option<&SpendPolicy>,
    )>,
    mut texts: Query<&mut Text, With<SheetText>>,
) {
    if !panels.sheet {
        return;
    }
    let names = names(ids.as_deref());
    let mut s = String::new();
    let row = selected
        .0
        .and_then(|id| Some((id, index.as_deref()?.get(id)?)))
        .and_then(|(id, e)| units.get(e).ok().map(|r| (id, r)));
    match (row, rules.0.as_deref(), rules.1.as_deref()) {
        (None, ..) => s.push_str("F5 SHEET  (select a unit)"),
        (Some((id, (None, ..))), ..) => {
            let _ = write!(s, "F5 SHEET  {}: no character sheet", label(&names, id));
        }
        (Some(_), None, _) | (Some(_), _, None) => s.push_str("F5 SHEET  (no rules)"),
        (Some((id, (Some(sheet), exp, policy))), Some(set), Some(reg)) => {
            let view = Rules::new(set, reg).view(sheet);
            let _ = writeln!(s, "F5 SHEET  {}   ({})", label(&names, id), view.rule_set);
            let stats: Vec<String> = view
                .stats
                .iter()
                .map(|st| {
                    if st.name.eq_ignore_ascii_case("speed") {
                        format!("{} {}", st.name, view.basic_speed)
                    } else {
                        format!("{} {}", st.name, st.value)
                    }
                })
                .collect();
            for chunk in stats.chunks(5) {
                let _ = writeln!(s, "  {}", chunk.join("   "));
            }
            if view.traits.is_empty() {
                s.push_str("traits  -\n");
            } else {
                let t: Vec<String> = view
                    .traits
                    .iter()
                    .map(|t| format!("{} {} ({} pts)", t.name, t.level, t.points))
                    .collect();
                let _ = writeln!(s, "traits  {}", t.join(", "));
            }
            s.push_str("skills            pts  level\n");
            for sk in &view.skills {
                let level = sk.level.map_or_else(|| "-".into(), |l| l.to_string());
                let rel = sk.relative_label.clone().unwrap_or_default();
                let _ = writeln!(s, "  {:<15} {:>3}  {:>4}  {rel}", sk.name, sk.points, level);
            }
            let p = &view.points;
            let _ = writeln!(
                s,
                "points  attr {} sec {} traits {} skills {} = {} spent",
                p.attributes, p.secondary, p.traits, p.skills, p.spent
            );
            let _ = write!(s, "unspent {}   total {}", p.unspent, p.total);
            if let Some(exp) = exp {
                let next = progression.as_deref().and_then(|c| {
                    (exp.level < c.max_level)
                        .then(|| c.curve.total_for(exp.level + 1))
                        .flatten()
                });
                let next = next.map_or_else(|| "max".into(), |n| n.to_string());
                let _ = write!(
                    s,
                    "\nlevel {}   xp {} / {next}   policy {:?}",
                    exp.level,
                    exp.xp,
                    policy.copied().unwrap_or_default()
                );
            }
        }
    }
    set_text(&mut texts, &s);
}

fn kind_name(bits: u32) -> &'static str {
    match bits {
        b if b & kinds::UNIT != 0 => "actor",
        b if b & kinds::VEHICLE != 0 => "vehicle",
        b if b & kinds::DOOR != 0 => "door",
        b if b & kinds::TREE != 0 => "tree",
        b if b & kinds::ORE != 0 => "ore",
        b if b & kinds::STATION != 0 => "station",
        b if b & kinds::ITEM != 0 => "item",
        b if b & kinds::CONTAINER != 0 => "container",
        _ => "?",
    }
}

fn sense(v: SenseKind) -> String {
    format!("{v:?}").to_lowercase()
}

fn dist_mm(a: SimPos, b: SimPos) -> i32 {
    let (dx, dz) = (f64::from(a.x - b.x), f64::from(a.z - b.z));
    (dx * dx + dz * dz).sqrt() as i32
}

#[allow(clippy::too_many_arguments)]
fn perception_text(
    panels: Res<TestbedPanels>,
    selected: Res<Selected>,
    index: Option<Res<SimIndex>>,
    clock: Option<Res<SimClock>>,
    config: Option<Res<PerceptionConfig>>,
    ids: Option<Res<TestbedIds>>,
    agents: Query<(
        &SimPos,
        Option<&Sensors>,
        Option<&Facing>,
        Option<&PerceptionMemory>,
    )>,
    perceivable: Query<&Perceivable>,
    mut texts: Query<&mut Text, With<PerceptionText>>,
) {
    if !panels.perception {
        return;
    }
    let names = names(ids.as_deref());
    let now = clock.map_or(0, |c| c.tick);
    let forget = config.map_or(0, |c| c.forget_after_ticks);
    let mut s = String::new();
    let row = selected
        .0
        .and_then(|id| Some((id, index.as_deref()?.get(id)?)))
        .and_then(|(id, e)| agents.get(e).ok().map(|r| (id, e, r)));
    match row {
        None => s.push_str("F6 PERCEPTION  (select a unit; showing every agent's cone)"),
        Some((id, _, (_, None, ..))) => {
            let _ = write!(s, "F6 PERCEPTION  {}: no sensors", label(&names, id));
        }
        Some((id, e, (pos, Some(sensors), facing, memory))) => {
            let _ = writeln!(s, "F6 PERCEPTION  {}", label(&names, id));
            let f = facing.copied().unwrap_or_default();
            let mut line = format!("  facing ({}, {})", f.x, f.z);
            for sensor in &sensors.sensors {
                match sensor {
                    Sensor::Vision(v) => {
                        let _ = write!(
                            line,
                            "  vision {} m +/-{} deg",
                            metres(v.range_mm),
                            v.half_fov_deg
                        );
                    }
                    Sensor::Hearing(h) => {
                        let _ = write!(line, "  hearing {} m", metres(h.radius_mm));
                    }
                    Sensor::Proximity(p) => {
                        let _ = write!(line, "  near {} m", metres(p.radius_mm));
                    }
                    Sensor::Lidar(l) => {
                        let _ = write!(
                            line,
                            "  lidar {} m {} deg x{}",
                            metres(l.range_mm),
                            l.fov_deg,
                            l.beams
                        );
                    }
                    Sensor::Camera(c) => {
                        let _ = write!(
                            line,
                            "  camera {} m +/-{} deg",
                            metres(c.range_mm),
                            c.half_fov_deg
                        );
                    }
                    Sensor::Radar(r) => {
                        let _ = write!(
                            line,
                            "  radar {} m +/-{} deg",
                            metres(r.range_mm),
                            r.half_fov_deg
                        );
                    }
                }
            }
            let _ = writeln!(s, "{line}");
            if let Some(p) = perceivable.get(e).ok().filter(|p| p.kinds != 0) {
                let _ = writeln!(s, "  is a {}", kind_name(p.kinds));
            }
            let memory = memory.cloned().unwrap_or_default();
            let (sensed, remembered): (Vec<_>, Vec<_>) =
                memory.percepts.iter().partition(|(_, p)| p.sensed);
            let _ = writeln!(
                s,
                "{:<31}{:>7}  {:<9} {:>4}",
                format!("sensed ({})", sensed.len()),
                "dist",
                "via",
                "conf"
            );
            for (pid, p) in sensed.iter().take(10) {
                let _ = writeln!(
                    s,
                    "  {:<20} {:<7} {:>5} m  {:<9} {:>4}",
                    label(&names, **pid),
                    kind_name(p.kinds),
                    metres(dist_mm(*pos, p.pos)),
                    sense(p.via),
                    p.confidence_at(now, forget)
                );
            }
            let _ = writeln!(
                s,
                "{:<31}{:>7}  {:<11}{:>4}",
                format!("remembered ({})", remembered.len()),
                "dist",
                "last seen",
                "conf"
            );
            for (pid, p) in remembered.iter().take(8) {
                let _ = writeln!(
                    s,
                    "  {:<20} {:<7} {:>5} m  -{:<4} t    {:>4}",
                    label(&names, **pid),
                    kind_name(p.kinds),
                    metres(dist_mm(*pos, p.pos)),
                    now.saturating_sub(p.last_seen_tick),
                    p.confidence_at(now, forget)
                );
            }
            match memory.last_noise {
                Some(n) => {
                    let _ = write!(
                        s,
                        "heard  noise at ({}, {}) m, {} ticks ago",
                        metres(n.pos.x),
                        metres(n.pos.z),
                        now.saturating_sub(n.tick)
                    );
                }
                None => s.push_str("heard  -"),
            }
        }
    }
    set_text(&mut texts, &s);
}

const ARC_SEGMENTS: usize = 20;

#[allow(clippy::too_many_arguments)]
fn perception_gizmos(
    panels: Res<TestbedPanels>,
    selected: Res<Selected>,
    index: Option<Res<SimIndex>>,
    vis_index: Res<VisualIndex>,
    transforms: Query<&GlobalTransform, With<VisualOf>>,
    agents: Query<(
        &SimPos,
        &Sensors,
        Option<&Facing>,
        Option<&PerceptionMemory>,
    )>,
    positions: Query<&SimPos>,
    mut gizmos: Gizmos,
) {
    if !panels.perception {
        return;
    }
    let Some((e, (pos, sensors, facing, memory))) = selected
        .0
        .and_then(|id| index.as_deref()?.get(id))
        .and_then(|e| agents.get(e).ok().map(|r| (e, r)))
    else {
        return;
    };
    let visual_of = |sim: Entity| {
        vis_index
            .map
            .get(&sim)
            .and_then(|v| transforms.get(*v).ok())
            .map(|t| t.translation().with_y(0.0))
    };
    let lift = Vec3::Y * 0.06;
    let origin = visual_of(e).unwrap_or_else(|| sim_to_world(*pos)) + lift;
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    let f = facing.copied().unwrap_or_default();
    let dir = Vec2::new(f.x as f32, f.z as f32).normalize_or(Vec2::Y);
    for sensor in &sensors.sensors {
        match *sensor {
            Sensor::Vision(v) => {
                let range = v.range_mm.max(0) as f32 / 1000.0;
                let half = f32::from(v.half_fov_deg).to_radians();
                let point = |angle: f32, r: f32| {
                    let d = Vec2::from_angle(angle).rotate(dir) * r;
                    origin + Vec3::new(d.x, 0.0, d.y)
                };
                for r in [range, range * 0.5] {
                    let arc: Vec<Vec3> = (0..=ARC_SEGMENTS)
                        .map(|i| point(-half + 2.0 * half * i as f32 / ARC_SEGMENTS as f32, r))
                        .collect();
                    gizmos.linestrip(arc, css::SKY_BLUE);
                }
                if v.half_fov_deg < 180 {
                    gizmos.line(origin, point(-half, range), css::SKY_BLUE);
                    gizmos.line(origin, point(half, range), css::SKY_BLUE);
                }
            }
            Sensor::Hearing(h) => {
                let r = h.radius_mm.max(0) as f32 / 1000.0;
                gizmos.circle(Isometry3d::new(origin, flat), r, css::SLATE_GRAY);
            }
            Sensor::Proximity(p) => {
                let r = p.radius_mm.max(0) as f32 / 1000.0;
                gizmos.circle(Isometry3d::new(origin, flat), r, css::WHITE);
            }
            // Robots are drawn from Task 13 (E3) on.
            Sensor::Lidar(_) | Sensor::Camera(_) | Sensor::Radar(_) => {}
        }
    }
    let Some(memory) = memory else { return };
    for (id, p) in &memory.percepts {
        let remembered_at = sim_to_world(p.pos) + lift;
        if p.sensed {
            // Line of sight to where the target is drawn now.
            let to = index
                .as_deref()
                .and_then(|i| i.get(*id))
                .and_then(|t| {
                    visual_of(t).or_else(|| positions.get(t).ok().map(|p| sim_to_world(*p)))
                })
                .map_or(remembered_at, |p| p + lift);
            gizmos.line(origin + Vec3::Y * 1.2, to + Vec3::Y * 0.8, css::LIME);
            gizmos.circle(Isometry3d::new(to, flat), 0.45, css::LIME);
        } else {
            // Memory marker: a grey cross and ring where it was last seen.
            let r = 0.35;
            gizmos.line(
                remembered_at + Vec3::new(-r, 0.0, -r),
                remembered_at + Vec3::new(r, 0.0, r),
                css::LIGHT_GRAY,
            );
            gizmos.line(
                remembered_at + Vec3::new(-r, 0.0, r),
                remembered_at + Vec3::new(r, 0.0, -r),
                css::LIGHT_GRAY,
            );
            gizmos.circle(Isometry3d::new(remembered_at, flat), 0.5, css::DARK_GRAY);
            gizmos.line(origin, remembered_at, css::DIM_GRAY);
        }
    }
    if let Some(n) = memory.last_noise {
        let at = sim_to_world(n.pos) + lift;
        gizmos.circle(Isometry3d::new(at, flat), 0.6, css::ORANGE);
        gizmos.circle(Isometry3d::new(at, flat), 0.9, css::ORANGE);
    }
}
