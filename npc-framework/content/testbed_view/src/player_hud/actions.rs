//! Input interpretation: shared action adapter only, no domain mutation or file I/O.
use super::*;
use npc_framework::agent_io::api::ActStep;
use npc_framework::view_helpers::sim_to_world;

pub(super) fn restore_follow(world: &mut World) {
    let previous = world
        .get_resource_mut::<PlayerState>()
        .and_then(|mut state| state.preview_follow.take());
    if let Some(previous) = previous
        && let Some(mut follow) = world.get_resource_mut::<crate::village_world::FollowHero>()
        && follow.enabled != previous
    {
        follow.toggle();
    }
}
fn preview_site(world: &mut World, id: u32) {
    let Ok(observed) = player_actions::observe(world) else {
        return;
    };
    let Some(plot) = model::array(&observed["sections"]["homestead"]["plots"])
        .iter()
        .find(|plot| model::id(&plot["id"]) == Some(id))
    else {
        return;
    };
    let Some(position) = model::position(&plot["position"]) else {
        return;
    };
    if world.resource::<PlayerState>().preview_follow.is_none() {
        let enabled = world
            .get_resource::<crate::village_world::FollowHero>()
            .map(|follow| follow.enabled);
        world.resource_mut::<PlayerState>().preview_follow = enabled;
    }
    if let Some(mut follow) = world.get_resource_mut::<crate::village_world::FollowHero>()
        && follow.enabled
    {
        follow.toggle();
    }
    let panels: Vec<_> = world
        .query_filtered::<(&Node, &ComputedNode, &UiGlobalTransform), With<PlayerRoot>>()
        .iter(world)
        .filter(|(node, size, _)| node.display != Display::None && size.size().min_element() > 0.0)
        .map(|(_, size, transform)| {
            Rect::from_center_size(
                transform.translation * size.inverse_scale_factor(),
                size.size() * size.inverse_scale_factor(),
            )
        })
        .collect();
    for (projection, mut camera, mut transform) in world
        .query::<(Option<&Camera>, &mut crate::RtsCamera, &mut Transform)>()
        .iter_mut(world)
    {
        camera.focus = sim_to_world(position);
        camera.distance = 24.;
        if let Some(projection) = projection
            && let Some(viewport) = projection.logical_viewport_rect()
            && let Some(focus) = crate::village_world::framed_focus(
                projection,
                &camera,
                camera.focus,
                crate::village_world::player_screen_anchor(viewport, &panels),
            )
        {
            camera.focus = focus;
        }
        *transform = camera.transform();
    }
}
fn execute(world: &mut World, request: WorkRequest) -> Result<(), String> {
    if world
        .get_resource::<SimControl>()
        .is_some_and(|control| control.is_stopped())
    {
        return Err(
            "The world is paused. Press Space or Resume in the top bar, then start work.".into(),
        );
    }
    restore_follow(world);
    crate::island_control::clear_navigation_input(world);
    let action = ActStep {
        verb: Some(request.verb),
        target: request.target,
        args: Some(request.args),
        ..default()
    };
    match request.approach {
        Some(position) => player_actions::walk_then_act(world, position, action),
        None => player_actions::act(world, action),
    }
    .map(|_| ())?;
    let mut native = world.resource_mut::<NativeActions>();
    if let Some(pending) = native.pending.as_mut() {
        pending.label = request.label;
    }
    Ok(())
}
pub(super) fn process(world: &mut World) {
    if !world
        .get_resource::<FrontendGate>()
        .is_some_and(|gate| gate.0)
    {
        let pending = world.resource::<PlayerState>().successor_from;
        if let Some(previous) = pending
            && let Some(current) = world
                .get_resource::<testbed_content::village::VillageState>()
                .map(|state| state.played)
            && current != previous
        {
            world.resource_mut::<PlayerState>().successor_from = None;
            if let Err(error) = crate::request_island_mode(world, IslandMode::Character) {
                world.resource_mut::<NativeActions>().feedback = Some(error);
            }
        }
    }
    let watching = life::watching(world);
    if !active(world) && !watching {
        world.resource_mut::<PlayerState>().requests.clear();
        return;
    }
    let requests = std::mem::take(&mut world.resource_mut::<PlayerState>().requests);
    let preserve_focus = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space));
    for request in requests {
        if watching
            && !matches!(
                request,
                Action::Menu | Action::Successor | Action::WatchVillage | Action::LifeChoices
            )
        {
            continue;
        }
        world.resource_mut::<PlayerState>().dirty = true;
        let result = match request {
            Action::Open(tab) => {
                restore_follow(world);
                if matches!(tab, Tab::Build | Tab::Garden)
                    && let Ok(observed) = player_actions::observe(world)
                {
                    let current = world.resource::<PlayerState>().plot;
                    let chosen = model::preferred_plot(&observed, tab, current);
                    let mut state = world.resource_mut::<PlayerState>();
                    state.plot = chosen;
                    if tab == Tab::Garden {
                        state.bed = model::preferred_bed(&observed, chosen).unwrap_or(0);
                        state.crop = model::preferred_crop(&observed, &state.crop);
                    }
                }
                let mut state = world.resource_mut::<PlayerState>();
                state.tab = Some(tab);
                state.need_filter = None;
                if matches!(tab, Tab::Build | Tab::Garden)
                    && let Some(plot) = world.resource::<PlayerState>().plot
                {
                    preview_site(world, plot);
                }
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::Toggle(tab) => {
                // The hotbar tile of the open workspace closes it.
                let next = if world.resource::<PlayerState>().tab == Some(tab) {
                    Action::Close
                } else {
                    Action::Open(tab)
                };
                world.resource_mut::<PlayerState>().requests.push(next);
                Ok(())
            }
            Action::Close => {
                restore_follow(world);
                let mut state = world.resource_mut::<PlayerState>();
                state.tab = None;
                state.need_filter = None;
                state.gift_for = None;
                Ok(())
            }
            Action::Item(kind) => {
                let mut state = world.resource_mut::<PlayerState>();
                state.item = (state.item.as_deref() != Some(kind.as_str())).then_some(kind);
                Ok(())
            }
            Action::CraftAll(all) => {
                world.resource_mut::<PlayerState>().craft_all = all;
                Ok(())
            }
            Action::GoalFold => {
                let mut state = world.resource_mut::<PlayerState>();
                state.goal_folded = !state.goal_folded;
                Ok(())
            }
            Action::Menu => {
                world.resource_mut::<PlayerMenuRequest>().0 = true;
                Ok(())
            }
            Action::Inspect | Action::WatchVillage => watch_village(world),
            Action::LifeChoices => {
                life::show_choices(world);
                Ok(())
            }
            Action::Successor => {
                if world.resource::<PlayerState>().successor_from.is_some() {
                    continue;
                }
                let id = world
                    .resource::<testbed_content::village::VillageState>()
                    .played;
                player_actions::begin_successor(world)
                    .map(|_| world.resource_mut::<PlayerState>().successor_from = Some(id))
            }
            Action::Stop => {
                // Stop also ends a repeat order, so the next round does not start.
                if depth::repeat_words(world).is_some() {
                    let _ = depth::send(world, &depth::LiveRequest::new("stop_repeat"));
                }
                player_actions::stop(world).map(|_| ())
            }
            Action::Live(request) => {
                if request.verb == "gift" {
                    world.resource_mut::<PlayerState>().gift_for = None;
                }
                depth::send(world, &request)
            }
            Action::GiftFor(resident) => {
                world.resource_mut::<PlayerState>().gift_for = resident;
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::NeedHelp(need) => {
                let mut state = world.resource_mut::<PlayerState>();
                state.need_filter = need;
                state.item = None;
                state.tab = Some(Tab::Pouch);
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::Repeat(count) => {
                world.resource_mut::<PlayerState>().repeat = count;
                Ok(())
            }
            Action::DismissSummary => {
                rest::dismiss(world);
                Ok(())
            }
            Action::Plot(id) => {
                let bed = player_actions::observe(world)
                    .ok()
                    .and_then(|observed| model::preferred_bed(&observed, Some(id)))
                    .unwrap_or(0);
                let mut state = world.resource_mut::<PlayerState>();
                state.plot = Some(id);
                state.bed = bed;
                if let Some(mut selected) = world.get_resource_mut::<Selected>() {
                    selected.0 = None;
                }
                preview_site(world, id);
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::ChoosePlot => {
                restore_follow(world);
                world.resource_mut::<PlayerState>().plot = None;
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::InspectPlot(id) => {
                let mut state = world.resource_mut::<PlayerState>();
                state.plot = Some(id);
                state.bed = 0;
                state.tab = Some(Tab::Build);
                if let Some(mut selected) = world.get_resource_mut::<Selected>() {
                    selected.0 = None;
                }
                preview_site(world, id);
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::Bed(index) => {
                world.resource_mut::<PlayerState>().bed = index;
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::Crop(kind) => {
                world.resource_mut::<PlayerState>().crop = kind;
                Ok(())
            }
            Action::Person(id) => {
                restore_follow(world);
                let mut state = world.resource_mut::<PlayerState>();
                state.person = Some(id);
                state.tab = Some(Tab::People);
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::ChoosePerson => {
                world.resource_mut::<PlayerState>().person = None;
                reset_workspace(world, preserve_focus);
                Ok(())
            }
            Action::Walk(pos) => {
                restore_follow(world);
                crate::island_control::clear_navigation_input(world);
                player_actions::walk(world, pos).map(|_| ())
            }
            Action::Work(request) => execute(world, request),
            Action::Goal => {
                let Ok(observed) = player_actions::observe(world) else {
                    continue;
                };
                let action = goal_action(world, &observed);
                world.resource_mut::<PlayerState>().requests.push(action);
                Ok(())
            }
        };
        if let Err(error) = result {
            world.resource_mut::<NativeActions>().feedback = Some(error);
        }
        if !preserve_focus {
            world.resource_mut::<InputFocus>().clear();
        }
    }
}

fn reset_workspace(world: &mut World, focus_first: bool) {
    for mut scroll in world
        .query_filtered::<&mut ScrollPosition, With<WorkspaceBody>>()
        .iter_mut(world)
    {
        scroll.0 = Vec2::ZERO;
    }
    world.resource_mut::<PlayerState>().focus_workspace = focus_first;
}
/// What the goal tracker's button does. The content projection is the one
/// source of objective priorities and facts.
pub(super) fn goal_action(world: &World, observed: &Value) -> Action {
    let task = testbed_content::village::player_journey::project(observed).next;
    if let Some(hint) = task.action {
        let door = if matches!(hint.verb.as_str(), "claim_plot" | "build_cottage") {
            model::array(&observed["sections"]["homestead"]["plots"])
                .iter()
                .find(|plot| model::id(&plot["id"]) == hint.target.map(|id| id.0))
                .and_then(|plot| model::position(&plot["front_door"]))
        } else {
            None
        };
        let approach = hint.position.or(door).or_else(|| {
            hint.target
                .and_then(|id| player_actions::known_position(world, id))
        });
        return Action::Work(WorkRequest {
            label: task.title.into(),
            verb: hint.verb,
            target: hint.target.map(|id| id.0),
            args: hint.args,
            approach,
        });
    }
    Action::Open(match task.id {
        "water" | "food" | "timber" | "planks" | "seed" => Tab::Craft,
        "claim" | "cottage" | "building" => Tab::Build,
        "plant" | "water_crop" | "harvest" => Tab::Garden,
        "trade" => Tab::Trade,
        "neighbors" | "village_life" => Tab::People,
        _ => Tab::Pouch,
    })
}
