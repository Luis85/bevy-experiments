//! Keyboard reach for the session bar and the panels beside it.
//!
//! In Oversee and Shape, Tab walks one ring: the bar first, then the mode's
//! own panels and docks in the order of the spec's section 8.6, and wraps. In Live the workspace
//! keeps its own Tab loop; Shift+Tab from the world reaches the bar and
//! tabbing off either end returns to the world. The bar and the Shape palette
//! take focus from the keyboard only: a click leaves focus alone, so Space
//! still pauses after one. Focus is view state; nothing here reaches the sim.
use crate::IslandMode;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Button as WidgetButton;

/// A control reached with Tab that a click does not focus. The widget kit
/// names the same marker `KeyFocus`.
pub(crate) use crate::game_ui::KeyFocus as KeyReach;

/// The panels of the ring, in Tab order.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FocusRegion {
    Bar,
    /// A lasting toast's own button ("Try again" after a failed save).
    Toast,
    Palette,
    Alerts,
    /// The right column: the resident inspector or a building card.
    Inspector,
    /// The right column: a ledger page.
    Ledger,
    /// The Oversee build menu.
    Build,
    Legend,
    /// Overlays and Places.
    DockLeft,
    /// The list a left-dock button opened.
    Popover,
    DockBuild,
    DockLedger,
    /// Developer launches: the button that opens the ledger.
    Launcher,
    /// Live: the goal tracker and the needs.
    LiveHud,
    /// Live: the hotbar.
    LiveHotbar,
    /// A first-run tip's own button, last in every ring.
    Coach,
}

/// What the played session is showing, as far as Tab is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    /// Character play, or a life prompt: the workspace owns Tab.
    Live {
        workspace_open: bool,
    },
    Oversee,
    Shape,
}

pub(crate) fn surface(world: &World) -> Surface {
    let hud = crate::player_hud::active(world);
    match world.get_resource::<IslandMode>() {
        Some(IslandMode::Inspector) if !hud => Surface::Oversee,
        Some(IslandMode::Editor) if !hud => Surface::Shape,
        _ => Surface::Live {
            workspace_open: crate::player_hud::owns_tab(world),
        },
    }
}

/// The regions Tab visits on a surface, in order. Hidden ones drop out when
/// the ring is collected.
pub(crate) fn regions(surface: Surface) -> &'static [FocusRegion] {
    match surface {
        // The bar, the goal and the needs, then the hotbar. The open
        // workspace keeps its own Tab loop.
        Surface::Live { .. } => &[
            FocusRegion::Bar,
            FocusRegion::Toast,
            FocusRegion::LiveHud,
            FocusRegion::LiveHotbar,
            FocusRegion::Coach,
        ],
        // Top bar; alerts; right column; build menu; left dock; build
        // dock; ledger dock. Only one of the right column's two is shown.
        Surface::Oversee => &[
            FocusRegion::Bar,
            FocusRegion::Toast,
            FocusRegion::Alerts,
            FocusRegion::Inspector,
            FocusRegion::Ledger,
            FocusRegion::Build,
            FocusRegion::Legend,
            FocusRegion::DockLeft,
            FocusRegion::Popover,
            FocusRegion::DockBuild,
            FocusRegion::DockLedger,
            FocusRegion::Launcher,
            FocusRegion::Coach,
        ],
        Surface::Shape => &[
            FocusRegion::Bar,
            FocusRegion::Toast,
            FocusRegion::Palette,
            FocusRegion::Launcher,
            FocusRegion::Coach,
        ],
    }
}

fn collect(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    if world
        .get::<Node>(entity)
        .is_some_and(|node| node.display == Display::None)
    {
        return;
    }
    let reachable = world.get::<KeyReach>(entity).is_some()
        || world.get::<TabIndex>(entity).is_some_and(|tab| tab.0 >= 0);
    if reachable
        && world.get::<WidgetButton>(entity).is_some()
        && world.get::<InteractionDisabled>(entity).is_none()
    {
        out.push(entity);
    }
    if let Some(children) = world.get::<Children>(entity) {
        for child in children.iter() {
            collect(world, child, out);
        }
    }
}

/// Every control Tab can reach on `surface`, in visiting order: regions in
/// ring order, controls in layout order, without hidden or disabled ones.
pub(crate) fn ring(world: &mut World, surface: Surface) -> Vec<Entity> {
    let wanted = regions(surface);
    let mut roots: Vec<(FocusRegion, Entity)> = world
        .query::<(Entity, &FocusRegion)>()
        .iter(world)
        .filter(|(_, region)| wanted.contains(region))
        .map(|(entity, region)| (*region, entity))
        .collect();
    roots.sort_by_key(|(region, entity)| {
        (wanted.iter().position(|wanted| wanted == region), *entity)
    });
    let mut out = Vec::new();
    for (_, root) in roots {
        collect(world, root, &mut out);
    }
    out
}

/// How many of the ring's first controls belong to the top bar.
fn bar_len(world: &World, ring: &[Entity]) -> usize {
    ring.iter()
        .take_while(|entity| region_of(world, **entity) == Some(FocusRegion::Bar))
        .count()
}

fn region_of(world: &World, mut entity: Entity) -> Option<FocusRegion> {
    loop {
        if let Some(region) = world.get::<FocusRegion>(entity) {
            return Some(*region);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

/// Where Shift+Tab from the world lands in Live: the top bar's last
/// control (the first `bar` controls of the ring are the bar's).
pub(crate) fn live_entry(ring: &[Entity], bar: usize) -> Option<Entity> {
    bar.checked_sub(1).and_then(|last| ring.get(last)).copied()
}

/// Where Tab goes next. `None` releases focus back to the world.
pub(crate) fn step(
    surface: Surface,
    ring: &[Entity],
    current: Option<Entity>,
    backwards: bool,
) -> Step {
    let at = current.and_then(|current| ring.iter().position(|entity| *entity == current));
    let last = ring.len().saturating_sub(1);
    match (surface, at) {
        (_, _) if ring.is_empty() => Step::Ignore,
        // Live: the bar is entered backwards from the world and left at
        // either end; everything else belongs to the workspace.
        (Surface::Live { .. }, Some(index)) => {
            if backwards && index > 0 {
                Step::Focus(ring[index - 1])
            } else if !backwards && index < last {
                Step::Focus(ring[index + 1])
            } else {
                Step::Release
            }
        }
        (
            Surface::Live {
                workspace_open: false,
            },
            None,
        ) if backwards && current.is_none() => Step::Focus(ring[last]),
        (Surface::Live { .. }, None) => Step::Ignore,
        (_, Some(index)) => Step::Focus(if backwards {
            ring[(index + ring.len() - 1) % ring.len()]
        } else {
            ring[(index + 1) % ring.len()]
        }),
        (_, None) => Step::Focus(if backwards { ring[last] } else { ring[0] }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Focus(Entity),
    /// Clear focus: the next key belongs to the world again.
    Release,
    /// Not ours: leave the key for the panel that owns it.
    Ignore,
}

pub(crate) fn reach_focused(
    focus: Option<Res<InputFocus>>,
    controls: Query<(), With<KeyReach>>,
) -> bool {
    focus
        .and_then(|focus| focus.get())
        .is_some_and(|entity| controls.contains(entity))
}

/// Tab and Shift+Tab. A handled key is consumed, so the ledger's and the
/// workspace's own Tab handling do not move focus a second time.
pub(crate) fn keys(world: &mut World) {
    if !super::enabled(world) {
        return;
    }
    let Some(input) = world.get_resource::<ButtonInput<KeyCode>>() else {
        return;
    };
    let tab = input.just_pressed(KeyCode::Tab)
        && !input.any_pressed([
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::SuperLeft,
            KeyCode::SuperRight,
        ]);
    let backwards = input.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let focused = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    // A window holding focus is the world; a text field keeps its keys.
    let current = focused.filter(|entity| world.get::<Window>(*entity).is_none());
    if current.is_some_and(|entity| world.get::<bevy::text::EditableText>(entity).is_some()) {
        return;
    }
    if !tab {
        return;
    }
    let surface = surface(world);
    let ring = ring(world, surface);
    let mut outcome = step(surface, &ring, current, backwards);
    // Shift+Tab from the world enters at the top bar's last control, not
    // at the end of the ring (the hotbar).
    if let (
        Surface::Live {
            workspace_open: false,
        },
        None,
        true,
    ) = (surface, current, backwards)
    {
        let bar = bar_len(world, &ring);
        if let Some(entity) = live_entry(&ring, bar) {
            outcome = Step::Focus(entity);
        }
    }
    match outcome {
        Step::Ignore => return,
        Step::Release => world.resource_mut::<InputFocus>().clear(),
        Step::Focus(entity) => {
            world
                .resource_mut::<InputFocus>()
                .set(entity, FocusCause::Navigated);
            world.trigger(bevy::ui_widgets::ScrollIntoView { entity });
        }
    }
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset(KeyCode::Tab);
}

/// Esc, first step: a focused bar, dock or panel control hands the keyboard
/// back to the world, and the key goes no further this press.
pub(crate) fn escape(world: &mut World) {
    if !super::enabled(world) {
        return;
    }
    let pressed = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape));
    let on_reach = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get())
        .is_some_and(|entity| world.get::<KeyReach>(entity).is_some());
    if pressed && on_reach {
        world.resource_mut::<InputFocus>().clear();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset(KeyCode::Escape);
    }
}

/// A click anywhere hands the keyboard back to the world: bar and palette
/// controls never keep focus from a pointer.
pub(crate) fn release_on_click(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    controls: Query<(), With<KeyReach>>,
    mut focus: ResMut<InputFocus>,
) {
    if mouse.is_some_and(|mouse| {
        mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right)
    }) && focus.get().is_some_and(|entity| controls.contains(entity))
    {
        focus.clear();
    }
}

/// Focus never rests on a control that is no longer shown or usable.
pub(crate) fn drop_hidden(world: &mut World) {
    let Some(entity) = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get())
        .filter(|entity| world.get::<KeyReach>(*entity).is_some())
    else {
        return;
    };
    let mut at = Some(entity);
    let mut hidden = world.get::<InteractionDisabled>(entity).is_some();
    while let Some(node) = at {
        hidden |= world
            .get::<Node>(node)
            .is_some_and(|node| node.display == Display::None);
        at = world.get::<ChildOf>(node).map(ChildOf::parent);
    }
    if hidden {
        world.resource_mut::<InputFocus>().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(world: &mut World, region: FocusRegion, shown: bool) -> Entity {
        world
            .spawn((
                region,
                Node {
                    display: if shown { Display::Flex } else { Display::None },
                    ..default()
                },
            ))
            .id()
    }

    fn control(world: &mut World, parent: Entity, tab_index: bool) -> Entity {
        let mut entity = world.spawn((ChildOf(parent), WidgetButton, Node::default()));
        if tab_index {
            entity.insert(TabIndex(0));
        } else {
            entity.insert(KeyReach);
        }
        entity.id()
    }

    struct Layout {
        world: World,
        bar: [Entity; 3],
        palette: [Entity; 2],
        alerts: [Entity; 2],
        legend: Entity,
        ledger: [Entity; 2],
        launcher: Entity,
    }

    /// Spawned out of ring order on purpose: order comes from the regions.
    fn layout(ledger_open: bool) -> Layout {
        let mut world = World::new();
        let ledger_root = panel(&mut world, FocusRegion::Ledger, ledger_open);
        let ledger = [
            control(&mut world, ledger_root, true),
            control(&mut world, ledger_root, true),
        ];
        let launcher_root = panel(&mut world, FocusRegion::Launcher, !ledger_open);
        let launcher = control(&mut world, launcher_root, true);
        let legend_root = panel(&mut world, FocusRegion::Legend, true);
        let legend = control(&mut world, legend_root, true);
        let alerts_root = panel(&mut world, FocusRegion::Alerts, true);
        // Rows sit one level down, inside the scrolling body.
        let body = world.spawn((ChildOf(alerts_root), Node::default())).id();
        let alerts = [
            control(&mut world, alerts_root, true),
            control(&mut world, body, true),
        ];
        // Reordered so the header control comes first in layout order.
        world
            .entity_mut(alerts_root)
            .replace_children(&[alerts[0], body]);
        let palette_root = panel(&mut world, FocusRegion::Palette, true);
        let palette = [
            control(&mut world, palette_root, false),
            control(&mut world, palette_root, false),
        ];
        let bar_root = panel(&mut world, FocusRegion::Bar, true);
        let bar = [
            control(&mut world, bar_root, false),
            control(&mut world, bar_root, false),
            control(&mut world, bar_root, false),
        ];
        Layout {
            world,
            bar,
            palette,
            alerts,
            legend,
            ledger,
            launcher,
        }
    }

    #[test]
    fn oversee_walks_bar_alerts_ledger_then_legend_and_wraps() {
        let mut ui = layout(true);
        let ring = ring(&mut ui.world, Surface::Oversee);
        let expected: Vec<Entity> = ui
            .bar
            .into_iter()
            .chain(ui.alerts)
            .chain(ui.ledger)
            .chain([ui.legend])
            .collect();
        assert_eq!(
            ring, expected,
            "the closed launcher and the palette are skipped"
        );
        // From the world, Tab starts at the bar and Shift+Tab at the far end.
        assert_eq!(
            step(Surface::Oversee, &ring, None, false),
            Step::Focus(ui.bar[0])
        );
        assert_eq!(
            step(Surface::Oversee, &ring, None, true),
            Step::Focus(ui.legend)
        );
        // Every control is reached exactly once before the ring wraps.
        let mut seen = Vec::new();
        let mut at = None;
        for _ in 0..ring.len() {
            let Step::Focus(next) = step(Surface::Oversee, &ring, at, false) else {
                panic!("the ring never releases focus in Oversee");
            };
            seen.push(next);
            at = Some(next);
        }
        assert_eq!(seen, expected);
        assert_eq!(
            step(Surface::Oversee, &ring, at, false),
            Step::Focus(ui.bar[0]),
            "Tab wraps from the last panel to the bar"
        );
        assert_eq!(
            step(Surface::Oversee, &ring, Some(ui.bar[0]), true),
            Step::Focus(ui.legend),
            "Shift+Tab wraps the other way"
        );
        // A closed ledger leaves its launcher in the ring instead.
        let mut closed = layout(false);
        let ring = super::ring(&mut closed.world, Surface::Oversee);
        assert_eq!(ring.last(), Some(&closed.launcher));
        assert!(!ring.contains(&closed.ledger[0]));
    }

    #[test]
    fn the_build_menu_joins_the_oversee_ring_after_the_right_column() {
        let mut ui = layout(true);
        let palette = panel(&mut ui.world, FocusRegion::Build, true);
        let controls = [
            control(&mut ui.world, palette, false),
            control(&mut ui.world, palette, false),
        ];
        let ring = ring(&mut ui.world, Surface::Oversee);
        let at = |entity| ring.iter().position(|found| *found == entity).unwrap();
        assert_eq!(at(controls[0]), at(ui.ledger[1]) + 1);
        assert_eq!(at(ui.legend), at(controls[1]) + 1);
        // Closed, it is not a stop; Shape has its own palette and never this one.
        assert!(!super::ring(&mut ui.world, Surface::Shape).contains(&controls[0]));
        ui.world.get_mut::<Node>(palette).unwrap().display = Display::None;
        assert!(!super::ring(&mut ui.world, Surface::Oversee).contains(&controls[0]));
    }

    #[test]
    fn the_resident_inspector_joins_the_ring_as_the_right_column_after_the_alerts() {
        let mut ui = layout(true);
        let card = panel(&mut ui.world, FocusRegion::Inspector, true);
        // Its controls are reached with Tab but never focused by a click.
        let controls = [
            control(&mut ui.world, card, false),
            control(&mut ui.world, card, false),
        ];
        let ring = ring(&mut ui.world, Surface::Oversee);
        let at = |entity| ring.iter().position(|found| *found == entity).unwrap();
        assert_eq!(at(controls[0]), at(ui.alerts[1]) + 1);
        assert_eq!(at(ui.ledger[0]), at(controls[1]) + 1);
        assert_eq!(
            step(Surface::Oversee, &ring, Some(ui.alerts[1]), false),
            Step::Focus(controls[0])
        );
        // A closed card is not a stop, and Shape never shows it.
        ui.world.get_mut::<Node>(card).unwrap().display = Display::None;
        assert!(!super::ring(&mut ui.world, Surface::Oversee).contains(&controls[0]));
        ui.world.get_mut::<Node>(card).unwrap().display = Display::Flex;
        assert!(!super::ring(&mut ui.world, Surface::Shape).contains(&controls[0]));
    }

    #[test]
    fn oversee_regions_follow_the_spec_order_with_the_docks_last() {
        let mut ui = layout(true);
        // Spawned in reverse, so the order can only come from the regions.
        let mut stops = Vec::new();
        for region in [
            FocusRegion::DockLedger,
            FocusRegion::DockBuild,
            FocusRegion::Popover,
            FocusRegion::DockLeft,
            FocusRegion::Build,
            FocusRegion::Inspector,
        ] {
            let root = panel(&mut ui.world, region, true);
            stops.push((region, control(&mut ui.world, root, false)));
        }
        let ring = ring(&mut ui.world, Surface::Oversee);
        let at = |entity| ring.iter().position(|found| *found == entity).unwrap();
        let stop = |region| stops.iter().find(|(found, _)| *found == region).unwrap().1;
        // Top bar; alerts; right column; build menu; left dock; build dock; ledger dock.
        let order = [
            at(ui.bar[0]),
            at(ui.alerts[0]),
            at(stop(FocusRegion::Inspector)),
            at(ui.ledger[0]),
            at(stop(FocusRegion::Build)),
            at(ui.legend),
            at(stop(FocusRegion::DockLeft)),
            at(stop(FocusRegion::Popover)),
            at(stop(FocusRegion::DockBuild)),
            at(stop(FocusRegion::DockLedger)),
        ];
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
        assert_eq!(
            ring.last(),
            Some(&stop(FocusRegion::DockLedger)),
            "the ledger dock is the last stop before the ring wraps"
        );
        assert_eq!(
            step(Surface::Oversee, &ring, ring.last().copied(), false),
            Step::Focus(ui.bar[0])
        );
        // The docks belong to Oversee: Shape and Live never visit them.
        let shape = super::ring(&mut ui.world, Surface::Shape);
        assert!(!shape.contains(&stop(FocusRegion::DockLedger)));
        // Esc on a focused dock control returns the keyboard and keeps the key.
        ui.world.init_resource::<InputFocus>();
        ui.world.init_resource::<ButtonInput<KeyCode>>();
        ui.world.init_resource::<super::super::ShellState>();
        ui.world
            .insert_resource(crate::PlayerExperience { enabled: true });
        ui.world
            .resource_mut::<InputFocus>()
            .set(stop(FocusRegion::DockBuild), FocusCause::Navigated);
        ui.world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        escape(&mut ui.world);
        assert_eq!(ui.world.resource::<InputFocus>().get(), None);
        assert!(
            !ui.world
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::Escape),
            "one step per press: the pause menu does not open as well"
        );
    }

    #[test]
    fn shape_walks_the_bar_then_the_palette() {
        let mut ui = layout(false);
        let ring = ring(&mut ui.world, Surface::Shape);
        let expected: Vec<Entity> = ui
            .bar
            .into_iter()
            .chain(ui.palette)
            .chain([ui.launcher])
            .collect();
        assert_eq!(
            ring, expected,
            "alerts, legend and ledger belong to Oversee"
        );
        assert_eq!(
            step(Surface::Shape, &ring, Some(ui.bar[2]), false),
            Step::Focus(ui.palette[0])
        );
    }

    #[test]
    fn hidden_and_disabled_controls_are_not_stops() {
        let mut ui = layout(true);
        ui.world.entity_mut(ui.bar[1]).insert(InteractionDisabled);
        ui.world.get_mut::<Node>(ui.alerts[1]).unwrap().display = Display::None;
        ui.world.entity_mut(ui.ledger[0]).insert(TabIndex(-1));
        let ring = ring(&mut ui.world, Surface::Oversee);
        assert_eq!(
            ring,
            [ui.bar[0], ui.bar[2], ui.alerts[0], ui.ledger[1], ui.legend]
        );
        // A control that is neither a tab stop nor keyboard-reachable is skipped.
        let stray = ui
            .world
            .spawn((ChildOf(ui.ledger[1]), WidgetButton, Node::default()))
            .id();
        assert!(!super::ring(&mut ui.world, Surface::Oversee).contains(&stray));
    }

    #[test]
    fn live_reaches_the_bar_backwards_and_leaves_it_at_either_end() {
        let mut ui = layout(false);
        let closed = Surface::Live {
            workspace_open: false,
        };
        let open = Surface::Live {
            workspace_open: true,
        };
        let ring = ring(&mut ui.world, closed);
        assert_eq!(ring, ui.bar, "only the bar joins the Live ring");
        // Tab from the world still opens the Pouch; Shift+Tab reaches the bar.
        assert_eq!(step(closed, &ring, None, false), Step::Ignore);
        assert_eq!(step(closed, &ring, None, true), Step::Focus(ui.bar[2]));
        assert_eq!(step(open, &ring, None, true), Step::Ignore);
        // Inside the bar both directions work; either end returns to the world.
        assert_eq!(
            step(closed, &ring, Some(ui.bar[2]), true),
            Step::Focus(ui.bar[1])
        );
        assert_eq!(
            step(closed, &ring, Some(ui.bar[1]), false),
            Step::Focus(ui.bar[2])
        );
        assert_eq!(step(closed, &ring, Some(ui.bar[2]), false), Step::Release);
        assert_eq!(step(closed, &ring, Some(ui.bar[0]), true), Step::Release);
        // A workspace control keeps its own loop.
        let foreign = ui.world.spawn(Node::default()).id();
        assert_eq!(step(open, &ring, Some(foreign), false), Step::Ignore);
        assert_eq!(step(closed, &ring, Some(foreign), true), Step::Ignore);
        assert_eq!(step(Surface::Oversee, &[], None, false), Step::Ignore);
    }

    #[test]
    fn shift_tab_from_the_world_enters_live_at_the_top_bar_not_the_hotbar() {
        let mut ui = layout(false);
        let ring = ring(&mut ui.world, Surface::Oversee);
        // The bar's controls come first; the last of them is the entry.
        assert_eq!(super::bar_len(&ui.world, &ring), ui.bar.len());
        assert_eq!(live_entry(&ring, ui.bar.len()), ui.bar.last().copied());
        assert_eq!(live_entry(&ring, 0), None);
    }

    #[test]
    fn focus_leaves_a_control_that_disappears() {
        let mut ui = layout(false);
        ui.world.init_resource::<InputFocus>();
        ui.world
            .resource_mut::<InputFocus>()
            .set(ui.palette[0], FocusCause::Navigated);
        drop_hidden(&mut ui.world);
        assert_eq!(ui.world.resource::<InputFocus>().get(), Some(ui.palette[0]));
        let root = ui.world.get::<ChildOf>(ui.palette[0]).unwrap().parent();
        ui.world.get_mut::<Node>(root).unwrap().display = Display::None;
        drop_hidden(&mut ui.world);
        assert_eq!(ui.world.resource::<InputFocus>().get(), None);
        // A ledger control is the ledger's to manage.
        ui.world
            .resource_mut::<InputFocus>()
            .set(ui.launcher, FocusCause::Navigated);
        let root = ui.world.get::<ChildOf>(ui.launcher).unwrap().parent();
        ui.world.get_mut::<Node>(root).unwrap().display = Display::None;
        drop_hidden(&mut ui.world);
        assert_eq!(ui.world.resource::<InputFocus>().get(), Some(ui.launcher));
    }
}
