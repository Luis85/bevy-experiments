//! A death notice can be dismissed without changing the life it describes.
//! Spectator intent is presentation state, scoped to the current actor and world.
use super::*;

type LifeStamp = (u32, u64);

#[derive(Resource, Default)]
pub(super) struct LifeView {
    watched: Option<LifeStamp>,
    focused_prompt: Option<LifeStamp>,
}

fn stamp(world: &World) -> Option<LifeStamp> {
    Some((
        world
            .get_resource::<island_content::village::VillageState>()?
            .played
            .0,
        npc_framework::agent_io::api::epoch(world),
    ))
}

/// A life that ended before it was ever played: the character of an
/// Oversee start that never arrived. Nothing about its end is shown.
fn never_played(world: &World) -> bool {
    world
        .get_resource::<island_content::village::VillageState>()
        .and_then(|state| {
            world
                .get_resource::<npc_framework::prelude::SimIndex>()?
                .get(state.played)
        })
        .is_some_and(|actor| {
            world
                .get::<island_content::village::colony::AwaitingArrival>(actor)
                .is_some()
        })
}

pub(super) fn needs_prompt(world: &World) -> bool {
    dead(world)
        && !never_played(world)
        && (world.get_resource::<IslandMode>() == Some(&IslandMode::Character)
            || world
                .get_resource::<LifeView>()
                .and_then(|view| view.watched)
                != stamp(world))
}

pub(super) fn watching(world: &World) -> bool {
    enabled(world)
        && dead(world)
        && !never_played(world)
        // Shape has its own rail and bar at the bottom left.
        && world.get_resource::<IslandMode>() != Some(&IslandMode::Editor)
        && !needs_prompt(world)
        && !world
            .get_resource::<FrontendGate>()
            .is_some_and(|gate| gate.0)
}

pub(super) fn watch(world: &mut World) -> Result<(), String> {
    crate::request_island_mode(world, IslandMode::Inspector)?;
    leave(world);
    Ok(())
}

/// The character view gives way to another mode: close its workspace and
/// acknowledge an ended life. This requests no mode and changes no actor.
pub(super) fn leave(world: &mut World) {
    actions::restore_follow(world);
    let watched = dead(world).then(|| stamp(world)).flatten();
    world.get_resource_or_init::<LifeView>().watched = watched;
    if let Some(mut state) = world.get_resource_mut::<PlayerState>() {
        state.tab = None;
        state.dirty = true;
    }
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.clear();
    }
}

pub(super) fn show_choices(world: &mut World) {
    world.get_resource_or_init::<LifeView>().watched = None;
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.clear();
    }
}

pub(super) fn spawn(world: &mut World) {
    use crate::game_ui::tokens::{self as t, Surface};
    use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Reach, Size, Tone};
    // Watching after a life has ended: one line and the way back to the choices.
    let bar = root(
        world,
        "Spectator life choices",
        w::chrome(
            Node {
                position_type: PositionType::Absolute,
                left: px(t::SCREEN_MARGIN),
                bottom: px(t::SCREEN_MARGIN),
                width: px(300),
                max_width: percent(45),
                padding: UiRect::axes(px(12), px(8)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            t::RADIUS_LG,
        ),
    );
    world.entity_mut(bar).insert(SpectatorLifeRoot);
    let status = w::paragraph(world, bar, "", t::DENSE, Tone::text(Surface::Chrome));
    world.entity_mut(status).insert(SpectatorLifeStatus);
    w::button(
        world,
        bar,
        ButtonSpec::new("Life choices", Kind::Secondary, Surface::Chrome)
            .size(Size::Small)
            .key("L")
            .reach(Reach::Tab(0)),
        (PlayerControl, PlayerAction(Action::LifeChoices)),
    );
}

pub(super) fn refresh(world: &mut World) {
    let prompt = enabled(world)
        && needs_prompt(world)
        && !world
            .get_resource::<FrontendGate>()
            .is_some_and(|gate| gate.0);
    let watching = watching(world);
    for mut node in world
        .query_filtered::<&mut Node, With<DeathRoot>>()
        .iter_mut(world)
    {
        // Only a real change: a written node is laid out again.
        let display = if prompt { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    for mut node in world
        .query_filtered::<&mut Node, With<SpectatorLifeRoot>>()
        .iter_mut(world)
    {
        let display = if watching {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    let stopped = world
        .get_resource::<SimControl>()
        .is_some_and(SimControl::is_stopped);
    let pending = world.resource::<PlayerState>().successor_from.is_some();
    let buttons: Vec<_> = world
        .query_filtered::<(Entity, Has<SuccessorButton>), Or<(With<SuccessorButton>, With<WatchButton>)>>()
        .iter(world).collect();
    for (entity, successor) in buttons {
        disabled(world, entity, pending || successor && stopped);
    }
    let rate = crate::island_shell::clock(world).rate();
    text::<SpectatorLifeStatus>(
        world,
        format!("Watching the village · {rate}\nThis life has ended. A successor can begin later."),
    );
    text::<DeathDetail>(
        world,
        format!(
            "Your home, possessions and diary remain in the living village. A successor begins a new mortal life. You can also keep watching the village.{}",
            if pending {
                "\n\nA new life is arriving."
            } else if stopped {
                "\n\nTime is paused. Press Space to let it run before beginning a successor."
            } else {
                ""
            }
        ),
    );
    let current = prompt.then(|| stamp(world)).flatten();
    let previous = world.resource::<LifeView>().focused_prompt;
    if current.is_some() && current != previous {
        let button = world
            .query_filtered::<Entity, With<WatchButton>>()
            .iter(world)
            .next();
        if let Some(button) = button {
            world
                .resource_mut::<InputFocus>()
                .set(button, FocusCause::Navigated);
        }
    }
    world.resource_mut::<LifeView>().focused_prompt = current;
}
