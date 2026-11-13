//! First-run tips: one small card at a time, beside the thing it explains.
//!
//! Passive: no modal, no pause. "Got it" dismisses a tip for good; the host
//! remembers which were dismissed (in its settings file, never in the world)
//! and can show them all again. With tips switched off none appears.
use super::focus::FocusRegion;
use crate::IslandMode;
use crate::game_ui::scale::{self, Layout, LayoutClass};
use crate::game_ui::tokens::{self as t, Surface};
use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Reach, Size, Tone};
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::Activate;
use npc_framework::view_helpers::ViewSystems;

/// The player's tips setting and what they have dismissed. Host data: the
/// host fills it from its settings and takes `dismissed` to remember it.
/// Without a host (developer launches) the resource is absent and no tip
/// is shown.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct IslandTips {
    /// First-run tips and the goal tracker's hints are wanted.
    pub show: bool,
    /// Tips already dismissed, one bit per entry of [`IslandTips::NAMES`].
    pub seen: u16,
    /// Tips dismissed since the host last looked.
    pub dismissed: Vec<u8>,
}

impl IslandTips {
    /// Stable names of the tips, in bit order, for the settings file.
    pub const NAMES: [&'static str; 6] = [
        "oversee_totals",
        "oversee_build",
        "oversee_resident",
        "oversee_alerts",
        "live",
        "shape",
    ];
}

/// Whether hints are wanted at all; true where no host says otherwise.
pub(crate) fn hints_wanted(world: &World) -> bool {
    world
        .get_resource::<IslandTips>()
        .is_none_or(|tips| tips.show)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tip {
    Totals,
    Build,
    Resident,
    Alerts,
    Live,
    Shape,
}

impl Tip {
    pub(crate) const ALL: [Self; 6] = [
        Self::Totals,
        Self::Build,
        Self::Resident,
        Self::Alerts,
        Self::Live,
        Self::Shape,
    ];
    pub(crate) fn index(self) -> u8 {
        Self::ALL
            .iter()
            .position(|tip| *tip == self)
            .expect("listed") as u8
    }
    pub(crate) fn mode(self) -> IslandMode {
        match self {
            Self::Live => IslandMode::Character,
            Self::Shape => IslandMode::Editor,
            _ => IslandMode::Inspector,
        }
    }
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Totals => "The village at a glance",
            Self::Build => "Build for the village",
            Self::Resident => "Meet a resident",
            Self::Alerts => "What needs you",
            Self::Live => "This is your life",
            Self::Shape => "Shape the land",
        }
    }
    pub(crate) fn body(self) -> &'static str {
        match self {
            Self::Totals => {
                "The top bar counts residents, morale, food, water and the civic fund. Rest the pointer on a total for its sentence; choose it for the ledger page behind it."
            }
            Self::Build => {
                "Choose a kind of building below, then a place for it. Residents carry the materials and do the work; the civic fund pays them."
            }
            Self::Resident => {
                "Click anybody on the island to see how they are, what they are doing and why they chose it."
            }
            Self::Alerts => {
                "Alerts list what is going wrong, the most urgent first. Choose one to go to the place."
            }
            Self::Live => {
                "Your goal is top left and your needs are bottom left. The hotbar opens what you carry, make, build and trade; E uses what is in reach."
            }
            Self::Shape => {
                "Pick a tool on the left and drag across the island. Ctrl+Z takes a stroke back. Where the simulation refuses, the outline turns red and the tag says why."
            }
        }
    }
}

/// The tip to show in `mode`: the first of that mode not yet dismissed, with
/// its place among the mode's tips ("2 of 4"). None with tips off.
pub(crate) fn current(mode: IslandMode, show: bool, seen: u16) -> Option<(Tip, usize, usize)> {
    if !show {
        return None;
    }
    let of_mode: Vec<Tip> = Tip::ALL
        .into_iter()
        .filter(|tip| tip.mode() == mode)
        .collect();
    let at = of_mode
        .iter()
        .position(|tip| seen & (1 << tip.index()) == 0)?;
    Some((of_mode[at], at + 1, of_mode.len()))
}

/// Width of a tip card.
pub(crate) const WIDTH: f32 = 300.0;

/// Where a tip stands: `(left, top or bottom)`, with `true` for a distance
/// from the top. Each sits beside what it explains and inside the window.
/// The toast lane under the bar: the Paused pill (34 high), one toast
/// under it (a failed save's lasts until the next attempt) and the gaps.
const TOAST_LANE: f32 = 92.0;

pub(crate) fn place(tip: Tip, layout: Layout, top: f32) -> (f32, f32, bool) {
    let centred = ((layout.width - WIDTH) * 0.5).round().max(t::SCREEN_MARGIN);
    let above_docks = t::SCREEN_MARGIN + t::DOCK + 12.0;
    let alerts = if layout.class == LayoutClass::Wide {
        t::ATTENTION_COLUMN
    } else {
        t::ATTENTION_COLUMN_STANDARD
    };
    // Right of the alert column and under the toast lane, so the Paused
    // pill and a toast stay readable beside it.
    let beside_alerts =
        (t::SCREEN_MARGIN + alerts + t::SCREEN_MARGIN).min(layout.width - WIDTH - t::SCREEN_MARGIN);
    let under_toasts = top + TOAST_LANE;
    match tip {
        // Under the totals, right of the alert column.
        Tip::Totals => (beside_alerts, under_toasts, true),
        Tip::Build => (centred, above_docks, false),
        Tip::Resident => (centred, (layout.height * 0.30).round(), true),
        // Beside the alert column, not over its journal line.
        Tip::Alerts => (beside_alerts, under_toasts, true),
        // Above the hotbar and the prompt.
        Tip::Live => (centred, above_docks + 52.0, false),
        // Right of the tool rail, under the toast lane: an export's toast
        // (with its button) and the Paused pill stay readable over it.
        Tip::Shape => (t::SCREEN_MARGIN + 76.0 + 12.0, under_toasts, true),
    }
}

#[derive(Component)]
struct TipRoot;
#[derive(Component)]
struct TipDismiss;

/// Where the first-run tip stands on screen while it shows (logical
/// pixels): world labels keep out from under it.
pub(crate) fn tip_rect(world: &mut World) -> Option<Rect> {
    let shown: Vec<Entity> = world
        .query_filtered::<(Entity, &Node), With<TipRoot>>()
        .iter(world)
        .filter(|(_, node)| node.display != Display::None)
        .map(|(entity, _)| entity)
        .collect();
    shown
        .into_iter()
        .find_map(|entity| crate::game_ui::tooltip::anchor_rect(world, entity))
}

#[derive(Resource, Default)]
struct TipParts {
    title: Option<Entity>,
    body: Option<Entity>,
    count: Option<Entity>,
    shown: Option<(Tip, usize, usize)>,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<TipParts>()
        .add_observer(dismiss)
        .add_systems(Startup, spawn)
        .add_systems(Update, refresh.in_set(ViewSystems::Present));
}

fn spawn(world: &mut World) {
    let root = world
        .spawn((
            Name::new("Island tip"),
            TipRoot,
            FocusRegion::Coach,
            GlobalZIndex(t::layer::POPOVER),
            w::chrome(
                Node {
                    position_type: PositionType::Absolute,
                    width: px(WIDTH),
                    padding: UiRect::all(px(12)),
                    row_gap: px(6),
                    flex_direction: FlexDirection::Column,
                    display: Display::None,
                    ..default()
                },
                t::RADIUS_LG,
            ),
        ))
        .id();
    world.entity_mut(root).insert(t::shadow(t::SHADOW_2));
    let title = w::label(world, root, "", t::SUBHEADING, Tone::text(Surface::Chrome));
    let body = w::paragraph(world, root, "", t::BODY, Tone::secondary(Surface::Chrome));
    // A definite width: the sentence wraps inside the card.
    if let Some(mut node) = world.get_mut::<Node>(body) {
        node.width = px(WIDTH - 26.0);
        node.flex_shrink = 0.0;
    }
    let foot = world
        .spawn((
            ChildOf(root),
            Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                margin: UiRect::top(px(2)),
                ..default()
            },
        ))
        .id();
    let count = w::label(world, foot, "", t::CAPTION, Tone::caption(Surface::Chrome));
    w::button(
        world,
        foot,
        ButtonSpec::new("Got it", Kind::Primary, Surface::Chrome)
            .size(Size::Small)
            .reach(Reach::Ring),
        TipDismiss,
    );
    *world.resource_mut::<TipParts>() = TipParts {
        title: Some(title),
        body: Some(body),
        count: Some(count),
        shown: None,
    };
}

fn dismiss(
    event: On<Activate>,
    buttons: Query<Has<InteractionDisabled>, With<TipDismiss>>,
    parts: Res<TipParts>,
    tips: Option<ResMut<IslandTips>>,
) {
    if buttons.get(event.entity) != Ok(false) {
        return;
    }
    let (Some((tip, _, _)), Some(mut tips)) = (parts.shown, tips) else {
        return;
    };
    // Seen at once for this session; the host remembers it for the next.
    tips.seen |= 1 << tip.index();
    tips.dismissed.push(tip.index());
}

fn refresh(world: &mut World) {
    let gated = world
        .get_resource::<crate::FrontendGate>()
        .is_some_and(|gate| gate.0);
    let wanted = (!gated && super::enabled(world))
        .then(|| {
            let tips = world.get_resource::<IslandTips>()?;
            let mode = *world.get_resource::<IslandMode>()?;
            current(mode, tips.show, tips.seen)
        })
        .flatten()
        // A tip about living waits while a life has ended or not begun.
        .filter(|(tip, _, _)| *tip != Tip::Live || crate::player_hud::active(world));
    let parts = world.resource::<TipParts>();
    let (title, body, count, shown) = (parts.title, parts.body, parts.count, parts.shown);
    if wanted != shown {
        if let (Some((tip, at, of)), Some(title), Some(body), Some(count)) =
            (wanted, title, body, count)
        {
            w::set_text(world, title, tip.title());
            w::set_text(world, body, tip.body());
            w::set_text(world, count, &format!("Tip {at} of {of}"));
        }
        world.resource_mut::<TipParts>().shown = wanted;
    }
    let layout = scale::layout(world);
    let top = super::content_top(world);
    // Shape's resources panel stands right of the rail: the tip moves past it.
    let beyond = if crate::shape_tools::resources::active(world) {
        crate::shape_tools::resources::PANEL_WIDTH + t::SCREEN_MARGIN
    } else {
        0.0
    };
    for mut node in world
        .query_filtered::<&mut Node, With<TipRoot>>()
        .iter_mut(world)
    {
        let Some((tip, _, _)) = wanted else {
            if node.display != Display::None {
                node.display = Display::None;
            }
            continue;
        };
        let (left, offset, from_top) = place(tip, layout, top);
        // Moved past the resources panel while it is open.
        let left = if tip == Tip::Shape && beyond > 0.0 {
            (left + beyond).min(layout.width - WIDTH - t::SCREEN_MARGIN)
        } else {
            left
        };
        let mut next = node.clone();
        next.display = Display::Flex;
        next.left = px(left);
        next.top = if from_top { px(offset) } else { Val::Auto };
        next.bottom = if from_top { Val::Auto } else { px(offset) };
        if *node != next {
            *node = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_tip_at_a_time_per_mode_and_never_again_once_dismissed() {
        // Tips off: nothing, whatever was seen.
        assert_eq!(current(IslandMode::Inspector, false, 0), None);
        // Oversee walks its four in order.
        let mut seen = 0_u16;
        let mut walked = Vec::new();
        while let Some((tip, at, of)) = current(IslandMode::Inspector, true, seen) {
            assert_eq!((at, of), (walked.len() + 1, 4));
            walked.push(tip);
            seen |= 1 << tip.index();
        }
        assert_eq!(
            walked,
            [Tip::Totals, Tip::Build, Tip::Resident, Tip::Alerts]
        );
        // Live and Shape have one each, untouched by Oversee's.
        assert_eq!(
            current(IslandMode::Character, true, seen),
            Some((Tip::Live, 1, 1))
        );
        assert_eq!(
            current(IslandMode::Editor, true, seen),
            Some((Tip::Shape, 1, 1))
        );
        seen |= 1 << Tip::Live.index() | 1 << Tip::Shape.index();
        for mode in [
            IslandMode::Inspector,
            IslandMode::Character,
            IslandMode::Editor,
        ] {
            assert_eq!(current(mode, true, seen), None, "{mode:?}");
        }
        // Showing them again starts over.
        assert_eq!(
            current(IslandMode::Inspector, true, 0),
            Some((Tip::Totals, 1, 4))
        );
        assert_eq!(IslandTips::NAMES.len(), Tip::ALL.len());
        for tip in Tip::ALL {
            assert!(!tip.title().is_empty() && tip.body().ends_with('.'));
        }
    }

    #[test]
    fn a_tip_stands_inside_the_window_at_both_sizes() {
        for (width, height, class) in [
            (1280.0, 720.0, LayoutClass::Wide),
            (1024.0, 768.0, LayoutClass::Standard),
            (914.0, 514.0, LayoutClass::Compact),
        ] {
            let layout = Layout {
                width,
                height,
                scale: 1.0,
                class,
            };
            for tip in Tip::ALL {
                let (left, offset, _) = place(tip, layout, 52.0);
                assert!(
                    left >= t::SCREEN_MARGIN && left + WIDTH <= width - t::SCREEN_MARGIN + 0.5,
                    "{tip:?} at {width}: {left}"
                );
                assert!(
                    offset >= 44.0 && offset < height - 120.0,
                    "{tip:?}: {offset}"
                );
            }
        }
    }
}
