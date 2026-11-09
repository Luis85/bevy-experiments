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
            .get_resource::<testbed_content::village::VillageState>()?
            .played
            .0,
        npc_framework::agent_io::api::epoch(world),
    ))
}

pub(super) fn needs_prompt(world: &World) -> bool {
    dead(world)
        && (world.get_resource::<IslandMode>() == Some(&IslandMode::Character)
            || world
                .get_resource::<LifeView>()
                .and_then(|view| view.watched)
                != stamp(world))
}

pub(super) fn watching(world: &World) -> bool {
    enabled(world)
        && dead(world)
        && !needs_prompt(world)
        && !world
            .get_resource::<FrontendGate>()
            .is_some_and(|gate| gate.0)
}

pub(super) fn watch(world: &mut World) -> Result<(), String> {
    crate::request_island_mode(world, IslandMode::Inspector)?;
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
    Ok(())
}

pub(super) fn show_choices(world: &mut World) {
    world.get_resource_or_init::<LifeView>().watched = None;
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.clear();
    }
}

pub(super) fn spawn(world: &mut World) {
    let bar = root(
        world,
        "Spectator life choices",
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            bottom: px(12),
            width: px(300),
            max_width: percent(45),
            padding: UiRect::all(px(12)),
            row_gap: px(8),
            flex_direction: FlexDirection::Column,
            display: Display::None,
            ..default()
        },
    );
    world.entity_mut(bar).insert(SpectatorLifeRoot);
    world.spawn((
        ChildOf(bar),
        SpectatorLifeStatus,
        ui::text("", 14., ui::TEXT),
    ));
    button(world, bar, "Life choices · L", Action::LifeChoices, false);
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
        node.display = if prompt { Display::Flex } else { Display::None };
    }
    for mut node in world
        .query_filtered::<&mut Node, With<SpectatorLifeRoot>>()
        .iter_mut(world)
    {
        node.display = if watching {
            Display::Flex
        } else {
            Display::None
        };
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
    let rate = if stopped {
        "Paused".into()
    } else {
        format!(
            "{}×",
            world
                .get_resource::<SimControl>()
                .map_or(1, |control| control.multiplier)
        )
    };
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
                "\n\nOpen Menu and resume the world before beginning a successor."
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
