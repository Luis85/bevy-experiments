//! Two-level tooltips.
//!
//! Level 1 is a title line and one sentence, shown after a short rest on a
//! control. Level 2 adds facts and keys under a divider after a further
//! rest, or at once while Alt is held. Keyboard focus shows them too. A
//! tooltip never holds a control and never covers its anchor; what it says
//! is also reachable without hovering.
use super::icon::{self, Icon};
use super::scale;
use super::tokens::{self as t, Surface};
use super::widgets::{self, Skin, Tone};
use bevy::input_focus::InputFocus;
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;

/// What a control says about itself. Put it on any node with `Hovered`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Tooltip {
    pub icon: Option<Icon>,
    pub title: String,
    /// The exact number behind a rounded one ("42 of 84 beds").
    pub value: Option<String>,
    /// The binding, drawn as a key cap. Never written into the title.
    pub key: Option<String>,
    /// One sentence.
    pub body: String,
    /// Level 2: one fact per line.
    pub detail: Vec<String>,
    /// Stand beside the control (right, or above), as a menu's do: for rows
    /// stacked in a list, where a card below one covers the next.
    pub beside: bool,
}

impl Tooltip {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            ..default()
        }
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
    pub fn detail(mut self, lines: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.detail = lines.into_iter().map(Into::into).collect();
        self
    }
    /// Beside its control, never over the next row of a list.
    pub fn beside(mut self) -> Self {
        self.beside = true;
        self
    }
    /// The deepest level this tooltip has.
    pub fn levels(&self) -> u8 {
        if self.detail.is_empty() { 1 } else { 2 }
    }
}

/// The player's Tooltip delay setting.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipDelay {
    Short,
    #[default]
    Normal,
    Long,
}

impl TooltipDelay {
    pub const fn secs(self) -> f32 {
        match self {
            Self::Short => t::motion::TOOLTIP_SHORT,
            Self::Normal => t::motion::TOOLTIP_NORMAL,
            Self::Long => t::motion::TOOLTIP_LONG,
        }
    }
}

/// Which level shows after `rest` seconds on one element.
///
/// `carried` is true when another tooltip closed a moment ago: the next one
/// shows at once. `focused` is keyboard focus, which waits a little longer.
pub fn level(rest: f32, delay: TooltipDelay, alt: bool, carried: bool, focused: bool) -> u8 {
    let first = if carried {
        0.0
    } else if focused {
        t::motion::TOOLTIP_FOCUS
    } else {
        delay.secs()
    };
    if rest < first {
        0
    } else if alt || rest >= first + t::motion::TOOLTIP_DETAIL {
        2
    } else {
        1
    }
}

/// Where a tooltip goes: beside its anchor, inside the window, never over it.
/// Returns `(left, top or bottom offset, below)`.
pub fn place(anchor: Rect, window: Vec2, width: f32) -> (f32, f32, bool) {
    let margin = t::SCREEN_MARGIN;
    let left = anchor.min.x.min(window.x - width - margin).max(margin);
    let below = anchor.center().y < window.y * 0.5;
    let offset = if below {
        anchor.max.y + 6.0
    } else {
        window.y - anchor.min.y + 6.0
    };
    (left, offset, below)
}

/// A placed tooltip card. Whatever hides the island's roots under a host
/// menu must leave it alone: a menu control's tooltip is one of these too.
/// Where a menu control's tooltip stands: right of the control, or above
/// it when the window has no room on the right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Beside {
    Right {
        left: f32,
        top: f32,
    },
    /// Distance of the card's bottom edge from the window's bottom.
    Above {
        left: f32,
        bottom: f32,
    },
}

pub(crate) fn place_beside(anchor: Rect, window: Vec2, width: f32) -> Beside {
    let margin = t::SCREEN_MARGIN;
    if anchor.max.x + 8.0 + width <= window.x - margin {
        return Beside::Right {
            left: anchor.max.x + 8.0,
            // Room below for a two-line card; a card taller than that is
            // rare in a menu and stays inside the window from this top.
            top: anchor.min.y.min(window.y - margin - 96.0).max(margin),
        };
    }
    Beside::Above {
        left: anchor.min.x.min(window.x - width - margin).max(margin),
        bottom: window.y - anchor.min.y + 6.0,
    }
}

#[derive(Component)]
pub(crate) struct TooltipRoot;

#[derive(Resource, Default)]
pub(super) struct TooltipState {
    target: Option<Entity>,
    focused: bool,
    rest: f32,
    /// Seconds since a tooltip last closed.
    since_close: f32,
    carried: bool,
    shown: Option<(Entity, u8, Tooltip)>,
    /// The interface scale the shown card was built at.
    scale: f32,
}

fn ignore_pointer(world: &mut World, entity: Entity) {
    world.entity_mut(entity).insert(Pickable::IGNORE);
    let children = world
        .get::<Children>(entity)
        .map(|children| children.to_vec())
        .unwrap_or_default();
    for child in children {
        ignore_pointer(world, child);
    }
}

/// Build a tooltip card under `parent` (or as a root) at `level`. The live
/// system places it; the gallery shows it in the flow.
pub fn card(world: &mut World, parent: Option<Entity>, tip: &Tooltip, level: u8) -> Entity {
    let detailed = level >= 2 && !tip.detail.is_empty();
    let entity = world
        .spawn((
            Name::new(format!("Tooltip: {}", tip.title)),
            Skin::ChromeSolid,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                max_width: px(if detailed {
                    t::TOOLTIP_WIDTH_DETAIL
                } else {
                    t::TOOLTIP_WIDTH
                }),
                padding: UiRect::axes(px(10), px(8)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_MD)),
                ..default()
            },
            BackgroundColor(t::BARK_900),
            BorderColor::all(t::BARK_600),
            t::shadow(t::SHADOW_2),
        ))
        .id();
    if let Some(parent) = parent {
        world.entity_mut(entity).insert(ChildOf(parent));
    }
    let title = world
        .spawn((
            ChildOf(entity),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6),
                ..default()
            },
        ))
        .id();
    if let Some(picture) = tip.icon {
        icon::icon(world, title, picture, 16.0, Tone::text(Surface::Chrome));
    }
    widgets::label(
        world,
        title,
        &tip.title,
        t::DENSE,
        Tone::text(Surface::Chrome),
    );
    if let Some(value) = &tip.value {
        widgets::label(
            world,
            title,
            value,
            t::DENSE,
            Tone::secondary(Surface::Chrome),
        );
    }
    if let Some(key) = &tip.key {
        world.spawn((
            ChildOf(title),
            Node {
                flex_grow: 1.0,
                min_width: px(8),
                ..default()
            },
        ));
        widgets::keycap(world, title, Surface::Chrome, key);
    }
    if !tip.body.is_empty() {
        widgets::paragraph(
            world,
            entity,
            &tip.body,
            t::DENSE,
            Tone::secondary(Surface::Chrome),
        );
    }
    if detailed {
        widgets::divider(world, entity, Surface::Chrome, false);
        for fact in &tip.detail {
            widgets::paragraph(
                world,
                entity,
                fact,
                t::DENSE,
                Tone::secondary(Surface::Chrome),
            );
        }
    }
    ignore_pointer(world, entity);
    entity
}

pub(crate) fn anchor_rect(world: &World, entity: Entity) -> Option<Rect> {
    let node = world.get::<ComputedNode>(entity)?;
    let transform = world.get::<UiGlobalTransform>(entity)?;
    let scale = node.inverse_scale_factor();
    let half = node.size() * 0.5 * scale;
    let centre = transform.translation * scale;
    (half.x > 0.0 && half.y > 0.0).then(|| Rect::from_center_half_size(centre, half))
}

/// Above the host's screens (their root stands at 100).
const MENU_TOOLTIP: i32 = 200;

/// Whether a control belongs to a host screen (title, pause, Settings).
fn in_menu(world: &World, mut entity: Entity) -> bool {
    loop {
        if world.get::<crate::FrontendUiRoot>(entity).is_some() {
            return true;
        }
        match world.get::<ChildOf>(entity) {
            Some(parent) => entity = parent.parent(),
            None => return false,
        }
    }
}

/// The tooltip layer, or just above the layer its anchor stands in when that
/// is higher (a modal card's button): a tooltip is never under its panel.
fn layer_over(world: &World, mut entity: Entity) -> i32 {
    loop {
        if let Some(layer) = world.get::<GlobalZIndex>(entity) {
            return t::layer::TOOLTIP.max(layer.0 + 1);
        }
        match world.get::<ChildOf>(entity) {
            Some(parent) => entity = parent.parent(),
            None => return t::layer::TOOLTIP,
        }
    }
}

fn hidden(world: &World, mut entity: Entity) -> bool {
    loop {
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
        {
            return true;
        }
        match world.get::<ChildOf>(entity) {
            Some(parent) => entity = parent.parent(),
            None => return false,
        }
    }
}

/// The element the pointer rests on, else the one that holds keyboard focus.
fn target(world: &mut World, gated: bool) -> Option<(Entity, bool)> {
    let hovered: Vec<(Entity, f32)> = world
        .query::<(Entity, &Hovered, &Tooltip, &ComputedNode)>()
        .iter(world)
        .filter(|(_, hovered, ..)| hovered.get())
        .map(|(entity, _, _, node)| (entity, node.size().x * node.size().y))
        .collect();
    // The smallest hovered element is the innermost one.
    let pointed = hovered
        .into_iter()
        .filter(|(entity, _)| !hidden(world, *entity) && (!gated || in_menu(world, *entity)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(entity, _)| entity);
    if let Some(entity) = pointed {
        return Some((entity, false));
    }
    let focused = world.get_resource::<InputFocus>()?.get()?;
    (world.get::<Tooltip>(focused).is_some()
        && !hidden(world, focused)
        && (!gated || in_menu(world, focused)))
    .then_some((focused, true))
}

pub(super) fn update(world: &mut World) {
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let gated = world
        .get_resource::<crate::FrontendGate>()
        .is_some_and(|gate| gate.0);
    // Under a host menu only the menu's own controls explain themselves.
    let found = target(world, gated);
    let alt = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]));
    let delay = world
        .get_resource::<TooltipDelay>()
        .copied()
        .unwrap_or_default();
    let wanted = {
        let mut state = world.resource_mut::<TooltipState>();
        state.since_close += delta;
        if state.target != found.map(|(entity, _)| entity) {
            // Moving on within a moment of a tooltip closing shows the next at once.
            let was_showing = state.shown.is_some();
            if was_showing {
                state.since_close = 0.0;
            }
            state.carried =
                found.is_some() && (was_showing || state.since_close < t::motion::TOOLTIP_CARRY);
            state.target = found.map(|(entity, _)| entity);
            state.focused = found.is_some_and(|(_, focused)| focused);
            state.rest = 0.0;
        } else {
            state.rest += delta;
        }
        state.target.map(|entity| {
            (
                entity,
                level(state.rest, delay, alt, state.carried, state.focused),
            )
        })
    };
    let wanted = wanted.and_then(|(entity, level)| {
        let tip = world.get::<Tooltip>(entity)?.clone();
        let level = level.min(tip.levels());
        (level > 0).then_some((entity, level, tip))
    });
    // A card built at another interface size is built again: its wrapped
    // text was laid out for the old one.
    let scale = scale::layout(world).scale;
    {
        let state = world.resource::<TooltipState>();
        if state.shown == wanted && (state.scale == scale || wanted.is_none()) {
            return;
        }
    }
    world.resource_mut::<TooltipState>().scale = scale;
    let roots: Vec<Entity> = world
        .query_filtered::<Entity, With<TooltipRoot>>()
        .iter(world)
        .collect();
    for root in roots {
        world.despawn(root);
    }
    if let Some((entity, level, tip)) = &wanted
        && let Some(anchor) = anchor_rect(world, *entity)
    {
        let layout = scale::layout(world);
        let width = if *level >= 2 {
            t::TOOLTIP_WIDTH_DETAIL
        } else {
            t::TOOLTIP_WIDTH
        };
        let window = Vec2::new(layout.width, layout.height);
        let menu = in_menu(world, *entity);
        let root = card(world, None, tip, *level);
        // A host screen stands above every island layer; its tooltips above it.
        let layer = if in_menu(world, *entity) {
            MENU_TOOLTIP
        } else {
            layer_over(world, *entity)
        };
        world
            .entity_mut(root)
            .insert((TooltipRoot, GlobalZIndex(layer)));
        let mut node = world.get_mut::<Node>(root).expect("tooltip");
        node.position_type = PositionType::Absolute;
        // A menu stacks its buttons: its tooltips stand beside the button
        // (or above it), never over the next one.
        // A card whose lines wrap gets its full width up front: measured at
        // its one-line width it came out a line short and clipped its end.
        let long = |text: &str| text.chars().count() as f32 * 6.5 > width - 20.0;
        let wraps = long(&tip.body) || (*level >= 2 && tip.detail.iter().any(|fact| long(fact)));
        let full = if wraps { px(width) } else { Val::Auto };
        match (menu || tip.beside).then(|| place_beside(anchor, window, width)) {
            Some(Beside::Right { left, top }) => {
                node.left = px(left);
                node.top = px(top);
                node.width = full;
            }
            Some(Beside::Above { left, bottom }) => {
                node.left = px(left);
                node.bottom = px(bottom);
                node.width = full;
            }
            None => {
                let (left, offset, below) = place(anchor, window, width);
                node.width = full;
                node.left = px(left);
                if below {
                    node.top = px(offset);
                } else {
                    node.bottom = px(offset);
                }
            }
        }
    }
    world.resource_mut::<TooltipState>().shown = wanted;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_a_host_menu_only_the_menus_own_controls_explain_themselves() {
        let mut world = World::new();
        world.init_resource::<TooltipState>();
        world.insert_resource(crate::FrontendGate(true));
        let menu = world.spawn((crate::FrontendUiRoot, Node::default())).id();
        let button = world
            .spawn((
                ChildOf(menu),
                Node::default(),
                ComputedNode::default(),
                Hovered(true),
                Tooltip::new("Settings", "Sound, display, controls and accessibility."),
            ))
            .id();
        let island = world
            .spawn((
                Node::default(),
                ComputedNode::default(),
                Hovered(true),
                Tooltip::new("Residents", "42 housed."),
            ))
            .id();
        let found = target(&mut world, true);
        assert_eq!(found.map(|(entity, _)| entity), Some(button));
        assert!(!in_menu(&world, island));
    }

    /// The native fault: the card of a menu control was spawned and, in the
    /// same frame, hidden with every other root while the menu was open.
    #[test]
    fn a_menu_controls_tooltip_survives_the_roots_hidden_under_the_menu() {
        let mut world = World::new();
        world.init_resource::<TooltipState>();
        world.init_resource::<scale::Layout>();
        world.insert_resource(crate::FrontendGate(true));
        world.init_resource::<crate::player_hud::HiddenRoots>();
        world.insert_resource(Time::<()>::default());
        let menu = world.spawn((crate::FrontendUiRoot, Node::default())).id();
        let button = world
            .spawn((
                ChildOf(menu),
                Node::default(),
                ComputedNode {
                    size: Vec2::new(200.0, 40.0),
                    inverse_scale_factor: 1.0,
                    ..default()
                },
                UiGlobalTransform::from_translation(Vec2::new(300.0, 300.0)),
                Hovered(true),
                Tooltip::new("Settings", "Sound, display, controls and accessibility."),
            ))
            .id();
        // An island root, which the menu hides.
        let island = world.spawn(Node::default()).id();
        for _ in 0..3 {
            world
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_millis(300));
            update(&mut world);
            crate::player_hud::hide_roots(&mut world);
        }
        let cards: Vec<Entity> = world
            .query_filtered::<Entity, With<TooltipRoot>>()
            .iter(&world)
            .collect();
        assert_eq!(cards.len(), 1, "the menu button's tooltip is up");
        assert_ne!(world.get::<Node>(cards[0]).unwrap().display, Display::None);
        assert_eq!(world.get::<Node>(island).unwrap().display, Display::None);
        assert_eq!(world.resource::<TooltipState>().target, Some(button));
    }

    #[test]
    fn a_tooltip_stands_above_the_panel_of_its_anchor() {
        let mut world = World::new();
        let modal = world.spawn(GlobalZIndex(t::layer::MODAL)).id();
        let button = world.spawn(ChildOf(modal)).id();
        assert_eq!(layer_over(&world, button), t::layer::MODAL + 1);
        let bar = world.spawn(GlobalZIndex(t::layer::CHROME)).id();
        let chip = world.spawn(ChildOf(bar)).id();
        assert_eq!(layer_over(&world, chip), t::layer::TOOLTIP);
    }

    #[test]
    fn a_menu_tooltip_stands_beside_its_button_or_above_it() {
        let window = Vec2::new(1024.0, 768.0);
        let button = Rect::new(72.0, 266.0, 392.0, 322.0);
        assert_eq!(
            place_beside(button, window, 280.0),
            Beside::Right {
                left: 400.0,
                top: 266.0
            }
        );
        // A wide button at the right edge: the card goes above it.
        let wide = Rect::new(500.0, 400.0, 1000.0, 440.0);
        let Beside::Above { left, bottom } = place_beside(wide, window, 280.0) else {
            panic!("above");
        };
        assert!(left + 280.0 <= 1024.0 - t::SCREEN_MARGIN && bottom == 768.0 - 400.0 + 6.0);
    }

    #[test]
    fn levels_follow_the_delays_alt_and_focus() {
        let normal = TooltipDelay::Normal;
        assert_eq!(level(0.0, normal, false, false, false), 0);
        assert_eq!(level(0.39, normal, false, false, false), 0);
        assert_eq!(level(0.41, normal, false, false, false), 1);
        assert_eq!(level(1.59, normal, false, false, false), 1);
        assert_eq!(level(1.61, normal, false, false, false), 2);
        // Alt shows the detail as soon as the tooltip is due.
        assert_eq!(level(0.41, normal, true, false, false), 2);
        assert_eq!(level(0.1, normal, true, false, false), 0);
        // Short and Long move the first level only.
        assert_eq!(level(0.21, TooltipDelay::Short, false, false, false), 1);
        assert_eq!(level(0.7, TooltipDelay::Long, false, false, false), 0);
        assert_eq!(level(0.81, TooltipDelay::Long, false, false, false), 1);
        // Keyboard focus waits 600 ms; a tooltip that just closed carries over.
        assert_eq!(level(0.5, normal, false, false, true), 0);
        assert_eq!(level(0.61, normal, false, false, true), 1);
        assert_eq!(level(0.0, normal, false, true, false), 1);
        assert_eq!(level(1.3, normal, false, true, true), 2);
    }

    #[test]
    fn a_tooltip_stays_in_the_window_and_off_its_anchor() {
        let window = Vec2::new(1280.0, 720.0);
        // A total in the top bar: below it, left edges aligned.
        let chip = Rect::new(300.0, 8.0, 360.0, 36.0);
        assert_eq!(place(chip, window, 280.0), (300.0, 42.0, true));
        // The speed cluster at the right edge: pulled inside the window.
        let speed = Rect::new(1200.0, 7.0, 1272.0, 37.0);
        let (left, top, below) = place(speed, window, 340.0);
        assert!(below && top == 43.0);
        assert_eq!(left, 1280.0 - 340.0 - 8.0);
        // A dock tile at the bottom: above it, measured from the bottom edge.
        let tile = Rect::new(930.0, 662.0, 994.0, 706.0);
        let (left, bottom, below) = place(tile, window, 280.0);
        assert!(!below);
        assert_eq!((left, bottom), (930.0, 720.0 - 662.0 + 6.0));
        // Never left of the margin.
        assert_eq!(place(Rect::new(0.0, 0.0, 20.0, 20.0), window, 280.0).0, 8.0);
    }

    #[test]
    fn the_card_has_a_title_a_sentence_and_detail_only_at_level_two() {
        let tip = Tooltip::new("Planks", "Sawn from logs by carpenters.")
            .icon(Icon::Planks)
            .value("11")
            .key("F2")
            .detail([
                "Harbour 6 · Alderwood 5 · Stone Ridge 0",
                "Two sites need 13.",
            ]);
        assert_eq!(tip.levels(), 2);
        let mut world = World::new();
        let texts = |world: &mut World, root: Entity| {
            let mut found = Vec::new();
            let mut stack = vec![root];
            while let Some(entity) = stack.pop() {
                if let Some(text) = world.get::<Text>(entity) {
                    found.push(text.0.clone());
                }
                if let Some(children) = world.get::<Children>(entity) {
                    stack.extend(children.iter().rev());
                }
            }
            found
        };
        let first = card(&mut world, None, &tip, 1);
        assert_eq!(
            texts(&mut world, first),
            ["Planks", "11", "F2", "Sawn from logs by carpenters."]
        );
        assert_eq!(world.get::<Node>(first).unwrap().max_width, px(280.0));
        let second = card(&mut world, None, &tip, 2);
        assert_eq!(texts(&mut world, second).len(), 6);
        assert_eq!(world.get::<Node>(second).unwrap().max_width, px(340.0));
        // Nothing in a tooltip takes the pointer.
        assert!(world.get::<Pickable>(second).is_some());
        // A tooltip without detail has one level.
        assert_eq!(Tooltip::new("Menu", "").levels(), 1);
    }
}
