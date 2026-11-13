//! The key to the resident markers: which colour is which trade. Shown while
//! overseeing, only at the zooms where the markers themselves are drawn, and
//! collapsible to its header. It names the trades living on the island right
//! now; nothing here is interactive beyond the collapse toggle.
use std::collections::BTreeSet;

use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use island_content::village::{Role, Villager, Vitality};

use super::markers::{ROLES, marker_fade, role_color};
use crate::game_ui::tokens as t;
use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Reach, Size};
use crate::island_shell::focus::FocusRegion;
use crate::{IslandMode, RtsCamera, game_ui as ui};

/// Markers fainter than this are not worth a legend.
const SHOWN_FROM: f32 = 0.15;
/// Above the left dock: the legend of the active overlay sits here.
const BOTTOM: f32 = 72.0;

#[derive(Resource, Default)]
pub(crate) struct Legend {
    pub(crate) collapsed: bool,
    listed: Option<Vec<Role>>,
}

#[derive(Component)]
pub(crate) struct LegendRoot;
#[derive(Component)]
struct LegendBody;
#[derive(Component)]
pub(crate) struct LegendToggle;
#[derive(Component)]
struct ToggleLabel;
#[derive(Component)]
struct LegendEntry(Role);

/// Title-case trade name for a legend entry ("Well keeper").
pub(crate) fn entry_label(role: Role) -> String {
    let name = role.name().replace('_', " ");
    let mut letters = name.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// A legend entry's words in this world: the trade's display name from the
/// world's data (a data-only trade too), else its own name.
fn label_in(world: &World, role: Role) -> String {
    let display = crate::colony_panel::trade_display(world, role);
    if display.is_empty() {
        entry_label(role)
    } else {
        display
    }
}

/// Whether the legend is on screen: overseeing in the player session, at a
/// zoom that shows markers, with at least one marker drawn. At night, with
/// everybody indoors, there is nothing for it to explain.
pub(crate) fn legend_shown(overseeing: bool, camera_distance: Option<f32>, markers: usize) -> bool {
    overseeing
        && markers > 0
        && camera_distance.is_some_and(|distance| marker_fade(distance) >= SHOWN_FROM)
}

/// The trades to list: every trade with a living resident, the shipped
/// trades in marker order, then the trades only the world's data defines by
/// their keys.
pub(crate) fn listed_roles(present: &BTreeSet<Role>) -> Vec<Role> {
    let mut listed: Vec<Role> = ROLES
        .into_iter()
        .filter(|role| present.contains(role))
        .collect();
    let mut data: Vec<Role> = present
        .iter()
        .copied()
        .filter(|role| role.is_data_only())
        .collect();
    data.sort_by_key(|role| role.key());
    listed.extend(data);
    listed
}

pub(crate) fn spawn(world: &mut World) {
    let root = world
        .spawn((
            Name::new("Resident marker legend"),
            LegendRoot,
            FocusRegion::Legend,
            GlobalZIndex(t::layer::CHROME),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    left: px(8),
                    bottom: px(BOTTOM),
                    width: px(236),
                    max_width: percent(40),
                    padding: UiRect::all(px(8)),
                    row_gap: px(5),
                    flex_direction: FlexDirection::Column,
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_LG,
            ),
        ))
        .id();
    let header = world
        .spawn((
            ChildOf(root),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                ..default()
            },
        ))
        .id();
    world.spawn((
        ChildOf(header),
        ui::text("Trades", 14.0, t::CREAM_50),
        Node {
            flex_grow: 1.0,
            ..default()
        },
    ));
    let toggle = w::button(
        world,
        header,
        ButtonSpec::new("Hide", Kind::Quiet, crate::game_ui::tokens::Surface::Chrome)
            .size(Size::Small)
            .reach(Reach::Ring),
        LegendToggle,
    );
    for child in world
        .get::<Children>(toggle)
        .map(|children| children.to_vec())
        .unwrap_or_default()
    {
        world.entity_mut(child).insert(ToggleLabel);
    }
    let body = world
        .spawn((
            ChildOf(root),
            LegendBody,
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(3),
                ..default()
            },
        ))
        .id();
    for role in ROLES {
        let label = label_in(world, role);
        spawn_entry(world, body, role, label);
    }
}

/// One legend entry: the marker's colour (the world's own, also for a
/// data-only trade) and the trade's name. Hidden until the trade is present.
fn spawn_entry(world: &mut World, body: Entity, role: Role, label: String) {
    let [r, g, b] = role_color(role);
    let entry = world
        .spawn((
            ChildOf(body),
            LegendEntry(role),
            Node {
                width: percent(47),
                align_items: AlignItems::Center,
                column_gap: px(6),
                display: Display::None,
                ..default()
            },
        ))
        .id();
    // The same dark rim the world markers carry.
    world.spawn((
        ChildOf(entry),
        Node {
            width: px(11),
            height: px(11),
            flex_shrink: 0.0,
            border: UiRect::all(px(1.5)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BackgroundColor(Color::srgb(r, g, b)),
        BorderColor::all(super::markers::OUTLINE),
    ));
    world.spawn((ChildOf(entry), ui::text(label, 12.0, t::CREAM_300)));
}

pub(crate) fn activate(
    event: On<Activate>,
    toggles: Query<(), With<LegendToggle>>,
    mut state: ResMut<Legend>,
) {
    if toggles.contains(event.entity) {
        state.collapsed = !state.collapsed;
    }
}

pub(crate) fn refresh(world: &mut World) {
    let overseeing = crate::island_shell::enabled(world)
        && world.get_resource::<IslandMode>() == Some(&IslandMode::Inspector)
        && !crate::player_hud::active(world)
        // The resident inspector uses the left column down to this corner.
        && !crate::resident_inspector::open(world)
        && !crate::island_build::column_taken(world);
    let distance = world
        .query::<&RtsCamera>()
        .iter(world)
        .next()
        .map(|camera| camera.distance);
    let markers = world
        .query::<(&super::markers::ResidentMarker, &Visibility)>()
        .iter(world)
        .filter(|(_, visibility)| **visibility != Visibility::Hidden)
        .count();
    let shown = legend_shown(overseeing, distance, markers);
    let collapsed = world.resource::<Legend>().collapsed;
    for mut node in world
        .query_filtered::<&mut Node, With<LegendRoot>>()
        .iter_mut(world)
    {
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    if !shown {
        return;
    }
    for mut node in world
        .query_filtered::<&mut Node, With<LegendBody>>()
        .iter_mut(world)
    {
        let display = if collapsed {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
    }
    let label = if collapsed { "Show" } else { "Hide" };
    for mut text in world
        .query_filtered::<&mut Text, With<ToggleLabel>>()
        .iter_mut(world)
    {
        if text.0 != label {
            text.0 = label.into();
        }
    }
    if collapsed {
        return;
    }
    let present: BTreeSet<Role> = world
        .query::<(&Villager, Option<&Vitality>)>()
        .iter(world)
        .filter(|(_, vitality)| vitality.is_none_or(|vitality| vitality.died_at.is_none()))
        .map(|(villager, _)| villager.role)
        .collect();
    let listed = listed_roles(&present);
    if world.resource::<Legend>().listed.as_ref() == Some(&listed) {
        return;
    }
    // A trade only the world's data defines gets its entry when it first
    // appears, after the shipped ones.
    let known: BTreeSet<Role> = world
        .query::<&LegendEntry>()
        .iter(world)
        .map(|entry| entry.0)
        .collect();
    let body = world
        .query_filtered::<Entity, With<LegendBody>>()
        .iter(world)
        .next();
    if let Some(body) = body {
        for role in listed.iter().copied().filter(|role| !known.contains(role)) {
            let label = label_in(world, role);
            spawn_entry(world, body, role, label);
        }
    }
    for (entry, mut node) in world.query::<(&LegendEntry, &mut Node)>().iter_mut(world) {
        let display = if listed.contains(&entry.0) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    world.resource_mut::<Legend>().listed = Some(listed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_legend_shows_only_while_overseeing_at_marker_zoom() {
        // Settlement zoom: markers and their key.
        assert!(legend_shown(true, Some(62.0), 12));
        assert!(legend_shown(true, Some(120.0), 1));
        // Up close and at the overview the markers are gone, and so is it.
        assert!(!legend_shown(true, Some(12.0), 12));
        assert!(!legend_shown(true, Some(330.0), 12));
        // Never in Live or Shape, and never without a camera.
        assert!(!legend_shown(false, Some(62.0), 12));
        assert!(!legend_shown(true, None, 12));
        // At night everybody is indoors: no marker is drawn, so no key.
        assert!(!legend_shown(true, Some(62.0), 0));
    }

    #[test]
    fn the_legend_names_the_trades_present_in_marker_order() {
        let present: BTreeSet<Role> = [
            Role::Smith,
            Role::from_key("potter"),
            Role::Farmer,
            Role::from_key("cooper"),
            Role::WaterKeeper,
        ]
        .into_iter()
        .collect();
        assert_eq!(
            listed_roles(&present),
            [
                Role::Farmer,
                Role::WaterKeeper,
                Role::Smith,
                Role::from_key("cooper"),
                Role::from_key("potter"),
            ],
            "the shipped trades in marker order, then the data's by key"
        );
        assert_eq!(entry_label(Role::from_key("rope_maker")), "Rope maker");
        assert!(listed_roles(&BTreeSet::new()).is_empty());
        assert_eq!(entry_label(Role::WaterKeeper), "Well keeper");
        assert_eq!(entry_label(Role::Smith), "Blacksmith");
        // Every trade has a name short enough for half the legend's width.
        for role in ROLES {
            let label = entry_label(role);
            assert!(!label.is_empty() && label.len() <= 12, "{label}");
        }
    }

    #[test]
    fn the_toggle_collapses_and_reopens_the_legend() {
        let mut app = App::new();
        app.init_resource::<Legend>().add_observer(activate);
        let toggle = app.world_mut().spawn(LegendToggle).id();
        let other = app.world_mut().spawn_empty().id();
        app.world_mut().trigger(Activate { entity: other });
        assert!(!app.world().resource::<Legend>().collapsed);
        app.world_mut().trigger(Activate { entity: toggle });
        assert!(app.world().resource::<Legend>().collapsed);
        app.world_mut().trigger(Activate { entity: toggle });
        assert!(!app.world().resource::<Legend>().collapsed);
    }
}
