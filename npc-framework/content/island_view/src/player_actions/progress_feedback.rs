//! Player-facing receipts over the shared action stream. This state is transient,
//! actor/epoch scoped, and never determines whether a simulation action succeeds.
use super::*;
use npc_framework::kernel::commands::{CommandLog, CommandQueue, SkippedCommands};
use std::collections::BTreeMap;

pub(super) struct Watch {
    actor: ActorStamp,
    verb: String,
    target: Option<SimId>,
    handle: Option<u32>,
    seq: u64,
    commands: Vec<(u64, u32)>,
    started: bool,
    words: [String; 3],
}

/// Sale outcomes can arrive long after the offer-publication command completed.
/// This cursor is host feedback, not a second market ledger or saved state.
pub(super) struct Sales {
    actor: ActorStamp,
    seq: u64,
    quantities: BTreeMap<String, u64>,
    coins: u64,
    after_work: u64,
}

pub(super) fn reset_sales(world: &mut World) {
    events::harvest(world);
    let sales = actor(world).ok().map(|actor| Sales {
        actor,
        seq: events::last_seq(world),
        quantities: BTreeMap::new(),
        coins: 0,
        after_work: 0,
    });
    world.resource_mut::<NativeActions>().sales = sales;
}

fn update_sales(world: &mut World, current: Option<ActorStamp>, native_now: bool) {
    let Some(actor) = current else {
        world.resource_mut::<NativeActions>().sales = None;
        return;
    };
    let previous = world.resource_mut::<NativeActions>().sales.take();
    let Some(mut sales) = previous.filter(|sales| sales.actor == actor) else {
        // First view or new epoch: start at the present, never replay a loaded
        // journal's historical income as a newly completed sale.
        reset_sales(world);
        return;
    };
    let latest = events::last_seq(world);
    if !native_now {
        sales.seq = latest;
        sales.quantities.clear();
        sales.coins = 0;
        world.resource_mut::<NativeActions>().sales = Some(sales);
        return;
    }
    let direct_sale = world
        .resource::<NativeActions>()
        .progress
        .as_ref()
        .is_some_and(|watch| watch.actor == actor && watch.verb == "sell");
    if let Some(events) = world.get_resource::<AgentEvents>() {
        for event in events
            .after(sales.seq)
            .filter(|event| event.unit == Some(actor.id))
        {
            let JournalKind::Effect {
                effect,
                applied: true,
            } = &event.event
            else {
                continue;
            };
            if direct_sale
                || effect.get("type").and_then(Value::as_str) != Some("VillageSaleCompleted")
            {
                continue;
            }
            let (Some(kind), Some(quantity), Some(coins)) = (
                effect.get("kind").and_then(Value::as_str),
                effect
                    .get("quantity")
                    .and_then(Value::as_u64)
                    .filter(|quantity| *quantity > 0),
                effect.get("coins").and_then(Value::as_u64),
            ) else {
                continue;
            };
            let label = if island_content::village::catalog::trade_kinds(world).contains(&kind) {
                kind.strip_prefix("village_")
                    .unwrap_or(kind)
                    .replace('_', " ")
            } else {
                "goods".into()
            };
            let held = sales.quantities.entry(label).or_default();
            *held = held.saturating_add(quantity);
            sales.coins = sales.coins.saturating_add(coins);
        }
    }
    sales.seq = latest;
    let now = world.resource::<SimClock>().tick;
    let busy = world.resource::<NativeActions>().progress.is_some()
        || world.resource::<NativeActions>().pending.is_some()
        || committed_work(world);
    if busy {
        // Give the current action's terminal feedback a short interval before
        // reporting income queued during that work. No simulation state changes.
        sales.after_work = now.saturating_add(20);
    } else if !sales.quantities.is_empty() && now >= sales.after_work {
        let goods = if sales.quantities.len() == 1 {
            let (kind, quantity) = sales.quantities.first_key_value().expect("one sale kind");
            format!("{quantity} × {kind}")
        } else {
            let quantity = sales
                .quantities
                .values()
                .fold(0_u64, |sum, quantity| sum.saturating_add(*quantity));
            format!("{quantity} items")
        };
        feedback(world, format!("Sold {goods} for {} coins.", sales.coins));
        sales.quantities.clear();
        sales.coins = 0;
    }
    world.resource_mut::<NativeActions>().sales = Some(sales);
}

pub(super) fn refusal(_world: &World, reason: &str) -> String {
    let lower = reason.to_ascii_lowercase();
    let message = if lower.contains("control") {
        "Character control changed before the activity could start."
    } else if lower.contains("price") || lower.contains("limit") {
        "The price changed. Check the latest offer before trading."
    } else if lower.contains("too far") || lower.contains("within") || lower.contains("reach") {
        "Move closer before trying again."
    } else if lower.contains("afford") || lower.contains("coins") || lower.contains("buyer has") {
        "There are not enough coins for this trade."
    } else if lower.contains("capacity") || lower.contains("carry") || lower.contains("weight") {
        "Your pouch cannot carry that. Make some room first."
    } else if lower.contains("busy") || lower.contains("reserved") {
        "Finish or stop the current activity first."
    } else if lower.contains("dead") || lower.contains("life has ended") {
        "This life has ended."
    } else if lower.contains("stock") || lower.contains("depleted") || lower.contains("remaining") {
        "Those supplies are no longer available."
    } else if lower.contains("ingredient")
        || lower.contains("material")
        || lower.contains("requires")
        || lower.contains("need ")
    {
        "You do not have the required supplies or equipment."
    } else if lower.contains("owner") || lower.contains("owned") || lower.contains("claimed") {
        "This site is unavailable or must be claimed first."
    } else if lower.contains("path") || lower.contains("blocked") {
        "The route is blocked. Choose another destination."
    } else {
        "Check the activity's requirements and try again."
    };
    format!("Could not start: {message}")
}

fn words(world: &World, verb: &str) -> [String; 3] {
    if let Some(recipe) = verb.strip_prefix("craft:") {
        let item = world
            .get_resource::<npc_framework::crafting::RecipeRegistry>()
            .and_then(|registry| registry.recipes.get(recipe))
            .and_then(|recipe| recipe.outputs.first())
            .map(|output| {
                output
                    .kind
                    .strip_prefix("village_")
                    .unwrap_or(&output.kind)
                    .replace('_', " ")
            })
            .unwrap_or_else(|| "goods".into());
        return [
            format!("Making {item} requested."),
            format!("Making {item}…"),
            format!("Finished making {item}."),
        ];
    }
    let messages = match verb {
        "gather" => [
            "Gathering requested.",
            "Gathering supplies…",
            "Supplies gathered.",
        ],
        "build_cottage" => [
            "Cottage construction requested.",
            "Building your cottage…",
            "Your cottage is complete.",
        ],
        "claim_plot" => [
            "Claim requested.",
            "Claiming your site…",
            "This site is now yours.",
        ],
        "plant" => [
            "Planting requested.",
            "Planting your crop…",
            "Your crop is planted.",
        ],
        "plant_tree" => [
            "Tree planting requested.",
            "Planting a tree cutting…",
            "Tree cutting planted. Growth takes time.",
        ],
        "restore_habitat" => [
            "Habitat restoration requested.",
            "Restoring the habitat…",
            "Habitat work is complete. Recovery takes time.",
        ],
        "water_crop" => [
            "Watering requested.",
            "Watering your crop…",
            "Your crop is watered.",
        ],
        "harvest" => [
            "Harvest requested.",
            "Harvesting your crop…",
            "Your harvest is in your pouch.",
        ],
        "rest" | "rest_home" | "rest_ground" => {
            ["Rest requested.", "Resting…", "You finished resting."]
        }
        "hunt" => ["Hunt requested.", "Hunting…", "The hunt is complete."],
        "buy" | "sell" => ["Trade requested.", "Trading goods…", "Trade completed."],
        "eat" => ["Eating requested.", "Eating…", "You ate from your pouch."],
        "drink" => ["Drinking requested.", "Drinking…", "You drank some water."],
        "talk" => [
            "Conversation requested.",
            "Talking with your neighbour…",
            "You spoke with your neighbour.",
        ],
        "offer_goods" => [
            "Listing requested.",
            "Offering your goods…",
            "Your goods are listed for sale.",
        ],
        "withdraw_offer" => [
            "Withdrawal requested.",
            "Removing your listing…",
            "Your listing was withdrawn.",
        ],
        "equip" | "unequip" => [
            "Equipment change requested.",
            "Changing your equipment…",
            "Your equipment has changed.",
        ],
        "stop_work" => [
            "Stopping requested.",
            "Stopping your activity…",
            "Your activity has stopped.",
        ],
        "walk" | "execute_orders" | "order+execute_orders" => [
            "Walk requested.",
            "Walking to your destination…",
            "You reached your destination.",
        ],
        _ => [
            "Activity requested.",
            "Your activity is in progress…",
            "Your activity is complete.",
        ],
    };
    messages.map(str::to_owned)
}

pub(super) fn track(
    world: &mut World,
    actor: ActorStamp,
    target: Option<SimId>,
    response: &Value,
    seq: u64,
) {
    let verb = response["verb"].as_str().unwrap_or("activity").to_owned();
    let words = words(world, &verb);
    let commands = world
        .resource::<NativeActions>()
        .refusal
        .as_ref()
        .filter(|watch| watch.actor == actor)
        .map(|watch| watch.commands.clone())
        .unwrap_or_default();
    feedback(world, words[0].clone());
    world.resource_mut::<NativeActions>().progress = Some(Watch {
        actor,
        verb,
        target,
        handle: response["handle"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok()),
        seq,
        commands,
        started: false,
        words,
    });
}

fn long_work(verb: &str) -> bool {
    matches!(
        verb,
        "gather"
            | "hunt"
            | "build_cottage"
            | "rest"
            | "rest_home"
            | "rest_ground"
            | "plant_tree"
            | "restore_habitat"
    )
}

fn restoration(verb: &str) -> bool {
    matches!(verb, "plant_tree" | "restore_habitat")
}

fn active_work(world: &World, watch: &Watch) -> bool {
    use island_content::village::{Gathering, homestead::CottageBuild};
    let actor = watch.actor.entity;
    match watch.verb.as_str() {
        "gather" => world
            .get::<Gathering>(actor)
            .is_some_and(|work| Some(work.patch) == watch.target),
        "build_cottage" => world
            .get::<CottageBuild>(actor)
            .is_some_and(|work| Some(work.plot) == watch.target),
        "plant_tree" | "restore_habitat" => world
            .get::<island_content::village::frontier::activities::EcologyWork>(actor)
            .is_some_and(|work| Some(work.target) == watch.target && work.verb == watch.verb),
        "hunt" => island_content::village::activity(world, actor).0 == "hunting",
        "rest" | "rest_home" | "rest_ground" => {
            island_content::village::activity(world, actor).0 == "resting"
        }
        _ => false,
    }
}

fn restoration_event_matches(
    watch: &Watch,
    effect: &npc_framework::kernel::json::JsonObject,
    target_field: &str,
    verb_field: &str,
) -> bool {
    restoration(&watch.verb)
        && watch.target.is_some_and(|target| {
            effect.get(target_field).and_then(Value::as_u64) == Some(u64::from(target.0))
                && effect.get(verb_field).and_then(Value::as_str) == Some(watch.verb.as_str())
        })
}

fn effect_started(watch: &Watch, effect: &npc_framework::kernel::json::JsonObject) -> bool {
    effect.get("type").and_then(Value::as_str) == Some("LandscapeWorkStarted")
        && restoration_event_matches(watch, effect, "target", "verb")
}

fn effect_result(watch: &Watch, effect: &npc_framework::kernel::json::JsonObject) -> Option<bool> {
    let kind = effect.get("type")?.as_str()?;
    if restoration(&watch.verb) {
        return match kind {
            "HabitatRestored" if restoration_event_matches(watch, effect, "patch", "work") => {
                Some(true)
            }
            "LandscapeWorkCancelled"
                if restoration_event_matches(watch, effect, "target", "verb") =>
            {
                Some(false)
            }
            _ => None,
        };
    }
    let expected = match watch.verb.as_str() {
        "gather" => ("VillageGathered", Some("patch")),
        "build_cottage" => ("VillageCottageBuilt", Some("plot")),
        "hunt" => ("VillageHunted", Some("target")),
        "rest" | "rest_home" | "rest_ground" => ("VillageRestCompleted", None),
        "claim_plot" => ("VillagePlotClaimed", Some("plot")),
        "plant" => ("VillageCropPlanted", Some("plot")),
        "water_crop" => ("VillageCropWatered", Some("plot")),
        "harvest" => ("VillageCropHarvested", Some("plot")),
        "buy" => ("VillageTrade", None),
        "sell" => ("VillageSaleCompleted", None),
        "eat" | "drink" => ("VillageConsumed", None),
        "talk" if kind == "VillageSpeech" => ("VillageSpeech", None),
        "talk" => ("VillageConversation", None),
        "offer_goods" => ("VillageGoodsOffered", None),
        "withdraw_offer" => ("VillageGoodsWithdrawn", None),
        _ => return None,
    };
    if (watch.verb == "build_cottage"
        && matches!(kind, "VillageCottageFailed" | "VillageCottageCancelled"))
        || (watch.verb == "hunt" && matches!(kind, "VillageHuntFailed" | "VillageHuntStopped"))
    {
        return Some(false);
    }
    (kind == expected.0
        && expected.1.is_none_or(|field| {
            effect.get(field).and_then(Value::as_u64) == watch.target.map(|id| u64::from(id.0))
        }))
    .then_some(true)
}

pub(super) fn update(world: &mut World, current: Option<ActorStamp>, native_now: bool) {
    update_sales(world, current, native_now);
    let Some(mut watch) = world.resource_mut::<NativeActions>().progress.take() else {
        return;
    };
    if !native_now || current != Some(watch.actor) {
        return;
    }
    if let Some(error) = world.get_resource::<SkippedCommands>().and_then(|skips| {
        skips
            .entries
            .iter()
            .rev()
            .find(|skip| watch.commands.contains(&(skip.tick, skip.seq)))
            .map(|skip| skip.error.to_string())
    }) {
        let message = refusal(world, &error);
        feedback(world, message);
        return;
    }
    let mut result = None;
    if let Some(events) = world.get_resource::<AgentEvents>() {
        for event in events
            .after(watch.seq)
            .filter(|event| event.unit == Some(watch.actor.id))
        {
            match &event.event {
                JournalKind::ExternalAction {
                    handle,
                    verb,
                    status,
                    ..
                } if watch.handle == Some(*handle)
                    && (!restoration(&watch.verb) || *verb == watch.verb) =>
                {
                    match status.as_str() {
                        "started" if !restoration(&watch.verb) => watch.started = true,
                        // Dispatch is not physical completion of timed landscape
                        // work. Only its own domain outcome can confirm success.
                        "done" if !restoration(&watch.verb) => result = Some(true),
                        "cancelled" | "failed" | "rejected" => result = Some(false),
                        _ => {}
                    }
                }
                JournalKind::Effect {
                    effect,
                    applied: true,
                } if watch.handle.is_none() || restoration(&watch.verb) => {
                    if effect_started(&watch, effect) {
                        watch.started = true;
                    }
                    if let Some(outcome) = effect_result(&watch, effect) {
                        result = Some(outcome);
                    }
                }
                _ => {}
            }
        }
        watch.seq = events.last_seq();
    }
    if let Some(completed) = result {
        feedback(
            world,
            if completed {
                watch.words[2].clone()
            } else {
                match watch.verb.as_str() {
                    "plant_tree" => "Tree planting stopped before completion.",
                    "restore_habitat" => "Habitat restoration stopped before completion.",
                    _ => "Your activity stopped before it finished. Check its requirements before trying again.",
                }.into()
            },
        );
        return;
    }
    let pending = world.get_resource::<CommandQueue>().is_some_and(|queue| {
        watch
            .commands
            .iter()
            .any(|key| queue.pending.contains_key(key))
    });
    let applied = !watch.commands.is_empty()
        && world.get_resource::<CommandLog>().is_some_and(|log| {
            watch.commands.iter().all(|key| {
                log.entries
                    .iter()
                    .rev()
                    .any(|(tick, seq, _)| (*tick, *seq) == *key)
            })
        });
    let working = active_work(world, &watch);
    if watch.verb == "stop_work" && applied && !pending {
        feedback(world, watch.words[2].clone());
        return;
    }
    if applied && !pending && watch.handle.is_some() && !restoration(&watch.verb) {
        let queued = world
            .get::<npc_framework::agent_io::control::ExternalQueue>(watch.actor.entity)
            .is_some_and(|queue| {
                queue
                    .current
                    .as_ref()
                    .is_some_and(|action| Some(action.item.handle) == watch.handle)
                    || queue
                        .pending
                        .iter()
                        .any(|action| Some(action.handle) == watch.handle)
            });
        if !queued {
            feedback(
                world,
                "The activity ended, but its result could not be confirmed. Check your pouch before trying again.",
            );
            return;
        }
    }
    if working || watch.started && watch.handle.is_some() && !restoration(&watch.verb) {
        watch.started = true;
        feedback(world, watch.words[1].clone());
    } else if applied && !pending && long_work(&watch.verb) {
        // Gathering can fail its final physical check without a success event.
        // A disappearing busy flag alone must never be advertised as completion.
        feedback(
            world,
            "The activity ended without completing. Check supplies and reach, then try again.",
        );
        return;
    } else if applied && !pending && watch.handle.is_none() {
        feedback(
            world,
            "The request was applied, but its result could not be confirmed. Check the activity before trying again.",
        );
        return;
    }
    world.resource_mut::<NativeActions>().progress = Some(watch);
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_content::village::{ResourcePatch, colony::JoinIsland, economy};
    use npc_framework::agent_io::api::AgentIoState;
    use npc_framework::kernel::{hash::state_hash, journal};

    fn fixture() -> (App, ActorStamp, SimId) {
        let mut app = island_content::new_sim_app(7, "living_world");
        let id = app.world().resource::<VillageState>().played;
        JoinIsland { unit: id.0 }.perform(app.world_mut()).unwrap();
        take_control(app.world_mut()).unwrap();
        run_ticks(app.world_mut(), 1);
        tick(app.world_mut());
        let who = actor(app.world()).unwrap();
        let (source, at) = app
            .world_mut()
            .query::<(&SimId, &SimPos, &ResourcePatch)>()
            .iter(app.world())
            .filter(|(_, _, patch)| patch.kind == "log")
            .map(|(id, pos, _)| (*id, *pos))
            .min_by_key(|(id, _)| *id)
            .unwrap();
        app.world_mut().entity_mut(who.entity).insert(at);
        for _ in 0..32 {
            run_ticks(app.world_mut(), 1);
            tick(app.world_mut());
        }
        (app, who, source)
    }
    fn gather(app: &mut App, source: SimId) {
        act(
            app.world_mut(),
            ActStep {
                verb: Some("gather".into()),
                target: Some(source.0),
                ..default()
            },
        )
        .unwrap();
    }
    fn message(app: &App) -> &str {
        app.world()
            .resource::<NativeActions>()
            .feedback
            .as_deref()
            .unwrap_or("")
    }

    fn restoration_fixture() -> (App, ActorStamp, SimId, Entity) {
        use island_content::village::{catalog, frontier::ecology::EcologicalPatch};
        let mut app = island_content::new_sim_app(7, "living_world");
        let id = app.world().resource::<VillageState>().played;
        JoinIsland { unit: id.0 }.perform(app.world_mut()).unwrap();
        take_control(app.world_mut()).unwrap();
        run_ticks(app.world_mut(), 1);
        tick(app.world_mut());
        let who = actor(app.world()).unwrap();
        let (target, site) = app
            .world()
            .resource::<SimIndex>()
            .map
            .iter()
            .find(|(_, entity)| {
                app.world()
                    .get::<ResourcePatch>(**entity)
                    .is_some_and(|patch| patch.kind == "log")
            })
            .map(|(id, entity)| (*id, *entity))
            .unwrap();
        let at = *app.world().get::<SimPos>(site).unwrap();
        app.world_mut().entity_mut(who.entity).insert(at);
        app.world_mut()
            .get_mut::<EcologicalPatch>(site)
            .unwrap()
            .biomass_milli = 50_000;
        for (kind, count) in [("log", 1), (catalog::WATER, 1), (catalog::FIBER, 2)] {
            economy::grant(app.world_mut(), who.entity, Item::new(kind).stack(count));
        }
        for _ in 0..32 {
            run_ticks(app.world_mut(), 1);
            tick(app.world_mut());
        }
        (app, who, target, site)
    }

    fn restore(app: &mut App, target: SimId, verb: &str) {
        act(
            app.world_mut(),
            ActStep {
                verb: Some(verb.into()),
                target: Some(target.0),
                ..default()
            },
        )
        .unwrap();
    }

    #[test]
    fn restoration_feedback_tracks_real_requests_work_and_delayed_growth_completion() {
        use island_content::village::frontier::{
            activities::EcologyWork, ecology::EcologicalPatch,
        };
        for (verb, requested, working, completed) in [
            (
                "plant_tree",
                "Tree planting requested.",
                "Planting a tree cutting…",
                "Tree cutting planted. Growth takes time.",
            ),
            (
                "restore_habitat",
                "Habitat restoration requested.",
                "Restoring the habitat…",
                "Habitat work is complete. Recovery takes time.",
            ),
        ] {
            let (mut app, who, target, site) = restoration_fixture();
            restore(&mut app, target, verb);
            assert_eq!(message(&app), requested);
            run_ticks(app.world_mut(), 1);
            tick(app.world_mut());
            assert_eq!(message(&app), working);
            let ends = app.world().get::<EcologyWork>(who.entity).unwrap().ends;
            let before = state_hash(app.world_mut());
            update(app.world_mut(), Some(who), true);
            assert_eq!(
                state_hash(app.world_mut()),
                before,
                "feedback must not perform work"
            );
            while app.world().resource::<SimClock>().tick <= ends {
                run_ticks(app.world_mut(), 1);
                tick(app.world_mut());
            }
            assert_eq!(message(&app), completed);
            assert!(app.world().resource::<NativeActions>().progress.is_none());
            let ecology = app.world().get::<EcologicalPatch>(site).unwrap();
            assert_eq!(ecology.planted_units + ecology.restored_units, 1);
            assert_eq!(
                ecology.pending_growth.len(),
                1,
                "completion is planting, not instant mature growth"
            );
            app.world_mut().resource_mut::<NativeActions>().feedback = Some("New selection".into());
            tick(app.world_mut());
            assert_eq!(
                message(&app),
                "New selection",
                "terminal receipts are consumed once"
            );
        }
    }

    #[test]
    fn actual_restoration_cancellation_never_reports_completion_or_spends_materials() {
        use island_content::village::{VillageAct, catalog, frontier::ecology::EcologicalPatch};
        for (verb, stopped) in [
            ("plant_tree", "Tree planting stopped before completion."),
            (
                "restore_habitat",
                "Habitat restoration stopped before completion.",
            ),
        ] {
            let (mut app, who, target, site) = restoration_fixture();
            let before: Vec<_> = ["log", catalog::WATER, catalog::FIBER]
                .into_iter()
                .map(|kind| (kind, economy::count(app.world(), who.entity, kind)))
                .collect();
            restore(&mut app, target, verb);
            run_ticks(app.world_mut(), 1);
            tick(app.world_mut());
            // A real recorded cancellation arriving independently of the HUD
            // leaves the original restoration watch available to explain it.
            submit(
                app.world_mut(),
                None,
                SimCommand::content(&VillageAct::new(who.id, "stop_work")),
            )
            .unwrap();
            run_ticks(app.world_mut(), 1);
            tick(app.world_mut());
            assert_eq!(message(&app), stopped);
            assert!(app.world().resource::<NativeActions>().progress.is_none());
            for (kind, count) in before {
                assert_eq!(economy::count(app.world(), who.entity, kind), count);
            }
            let ecology = app.world().get::<EcologicalPatch>(site).unwrap();
            assert_eq!(ecology.planted_units + ecology.restored_units, 0);
        }
    }

    #[test]
    fn restoration_outcomes_require_matching_actor_site_verb_and_epoch() {
        let (mut app, who, target, _) = restoration_fixture();
        restore(&mut app, target, "plant_tree");
        let other = app
            .world()
            .resource::<island_content::IslandIds>()
            .id("Ada");
        let other_entity = app.world().resource::<SimIndex>().get(other).unwrap();
        // Retain the foreign events in the shared buffer so this verifies the
        // feedback actor filter, rather than relying on harvest to discard them.
        app.world_mut()
            .entity_mut(other_entity)
            .insert(ExternalController {
                id: "other-test-controller".into(),
                mode: ControlMode::Direct,
                on_idle: IdlePolicy::Idle,
            });
        for (unit, site, verb) in [
            (other, target, "plant_tree"),
            (who.id, SimId(u32::MAX), "plant_tree"),
            (who.id, target, "restore_habitat"),
        ] {
            for event in ["LandscapeWorkStarted", "LandscapeWorkCancelled"] {
                journal::record_effect(
                    app.world_mut(),
                    Some(unit),
                    event,
                    &json!({"target":site,"verb":verb}),
                );
            }
            journal::record_effect(
                app.world_mut(),
                Some(unit),
                "HabitatRestored",
                &json!({"patch":site,"work":verb}),
            );
        }
        events::harvest(app.world_mut());
        let before = state_hash(app.world_mut());
        update(app.world_mut(), Some(who), true);
        assert_eq!(message(&app), "Tree planting requested.");
        assert!(
            !app.world()
                .resource::<NativeActions>()
                .progress
                .as_ref()
                .unwrap()
                .started
        );
        assert_eq!(state_hash(app.world_mut()), before);
        run_ticks(app.world_mut(), 1);
        tick(app.world_mut());
        assert_eq!(message(&app), "Planting a tree cutting…");
        journal::record_effect(
            app.world_mut(),
            Some(who.id),
            "HabitatRestored",
            &json!({"patch":target,"work":"plant_tree"}),
        );
        events::harvest(app.world_mut());
        app.insert_resource(AgentIoState::default());
        let next = actor(app.world()).unwrap();
        assert_ne!(who.epoch, next.epoch);
        app.world_mut().resource_mut::<NativeActions>().feedback = Some("Resumed island".into());
        update(app.world_mut(), Some(next), true);
        assert!(app.world().resource::<NativeActions>().progress.is_none());
        assert_eq!(message(&app), "Resumed island");
    }

    #[test]
    fn restoration_dispatch_completion_is_not_a_physical_outcome() {
        let (mut app, who, target, _) = restoration_fixture();
        let seq = events::last_seq(app.world());
        track(
            app.world_mut(),
            who,
            Some(target),
            &json!({"verb":"restore_habitat","handle":123}),
            seq,
        );
        journal::record(app.world_mut(), Some(who.id), || {
            JournalKind::ExternalAction {
                handle: 123,
                verb: "restore_habitat".into(),
                status: "done".into(),
                latency_ticks: None,
            }
        });
        events::harvest(app.world_mut());
        update(app.world_mut(), Some(who), true);
        assert_eq!(message(&app), "Habitat restoration requested.");
        assert!(app.world().resource::<NativeActions>().progress.is_some());
        journal::record_effect(
            app.world_mut(),
            Some(who.id),
            "LandscapeWorkCancelled",
            &json!({"target":target,"verb":"restore_habitat"}),
        );
        events::harvest(app.world_mut());
        update(app.world_mut(), Some(who), true);
        assert_eq!(
            message(&app),
            "Habitat restoration stopped before completion."
        );
        assert!(app.world().resource::<NativeActions>().progress.is_none());
    }

    fn publish_log(app: &mut App, who: ActorStamp) {
        economy::grant(app.world_mut(), who.entity, Item::new("log"));
        act(
            app.world_mut(),
            ActStep {
                verb: Some("offer_goods".into()),
                args: Some(json!({"kind":"log","quantity":1})),
                ..default()
            },
        )
        .unwrap();
        run_ticks(app.world_mut(), 1);
        tick(app.world_mut());
        assert_eq!(message(app), "Your goods are listed for sale.");
        assert!(app.world().resource::<NativeActions>().progress.is_none());
    }

    fn resident_buys_log(app: &mut App, who: ActorStamp) -> u32 {
        use island_content::village::{VillageAct, Wallet};
        let buyer = app
            .world()
            .resource::<island_content::IslandIds>()
            .id("Jun");
        let buyer_entity = app.world().resource::<SimIndex>().get(buyer).unwrap();
        let at = *app.world().get::<SimPos>(who.entity).unwrap();
        // Isolate feedback from finding a buyer: execute the resident-side
        // shared buy action used by NPC work, never a player sell request.
        VillageAct::new(buyer, "stop_work")
            .perform(app.world_mut())
            .unwrap();
        app.world_mut().entity_mut(buyer_entity).insert(at);
        app.world_mut()
            .get_mut::<Wallet>(buyer_entity)
            .unwrap()
            .coins = 1000;
        let before = app.world().get::<Wallet>(who.entity).unwrap().coins;
        let stock = economy::count(app.world(), who.entity, "log");
        let mut purchase = VillageAct::new(buyer, "buy");
        purchase.target = Some(who.id);
        purchase.kind = "log".into();
        purchase.quantity = 1;
        purchase.perform(app.world_mut()).unwrap();
        assert_eq!(economy::count(app.world(), who.entity, "log"), stock - 1);
        let earned = app.world().get::<Wallet>(who.entity).unwrap().coins - before;
        assert!(earned > 0);
        earned
    }

    #[test]
    fn posted_goods_later_bought_by_a_resident_report_actual_income_once() {
        let (mut app, who, _) = fixture();
        publish_log(&mut app, who);
        let coins = resident_buys_log(&mut app, who);
        // The publication command is over; its later own sale receipt is enough.
        run_ticks(app.world_mut(), 21);
        tick(app.world_mut());
        assert_eq!(message(&app), format!("Sold 1 × log for {coins} coins."));
        app.world_mut().resource_mut::<NativeActions>().feedback = Some("New selection".into());
        for _ in 0..3 {
            tick(app.world_mut());
        }
        assert_eq!(
            message(&app),
            "New selection",
            "one receipt must not reset later feedback"
        );
    }

    #[test]
    fn passive_income_waits_for_work_and_ignores_foreign_receipts_and_old_epochs() {
        let (mut app, who, source) = fixture();
        publish_log(&mut app, who);
        gather(&mut app, source);
        run_ticks(app.world_mut(), 1);
        tick(app.world_mut());
        let coins = resident_buys_log(&mut app, who);
        let other = app
            .world()
            .resource::<island_content::IslandIds>()
            .id("Ada");
        journal::record_effect(
            app.world_mut(),
            Some(other),
            "VillageSaleCompleted",
            &json!({"kind":"log","quantity":99,"coins":99999}),
        );
        tick(app.world_mut());
        assert_eq!(message(&app), "Gathering supplies…");
        let ends = app
            .world()
            .get::<island_content::village::Gathering>(who.entity)
            .unwrap()
            .ends;
        while app.world().resource::<SimClock>().tick <= ends {
            run_ticks(app.world_mut(), 1);
            tick(app.world_mut());
        }
        assert_eq!(message(&app), "Supplies gathered.");
        run_ticks(app.world_mut(), 21);
        tick(app.world_mut());
        assert_eq!(message(&app), format!("Sold 1 × log for {coins} coins."));
        // Even a retained old event buffer cannot replay income into a new epoch.
        app.insert_resource(AgentIoState::default());
        app.world_mut().resource_mut::<NativeActions>().feedback = Some("Resumed island".into());
        tick(app.world_mut());
        assert_eq!(message(&app), "Resumed island");
        clear_view(app.world_mut());
        app.world_mut().resource_mut::<NativeActions>().feedback = Some("Continued island".into());
        tick(app.world_mut());
        assert_eq!(message(&app), "Continued island");
    }

    #[test]
    fn gathering_feedback_distinguishes_request_running_and_actual_own_completion() {
        let (mut app, who, source) = fixture();
        let before = economy::count(app.world(), who.entity, "log");
        gather(&mut app, source);
        assert_eq!(message(&app), "Gathering requested.");
        run_ticks(app.world_mut(), 1);
        tick(app.world_mut());
        assert_eq!(message(&app), "Gathering supplies…");
        assert_eq!(economy::count(app.world(), who.entity, "log"), before);
        let ends = app
            .world()
            .get::<island_content::village::Gathering>(who.entity)
            .unwrap()
            .ends;
        while app.world().resource::<SimClock>().tick <= ends {
            run_ticks(app.world_mut(), 1);
            tick(app.world_mut());
        }
        assert_eq!(economy::count(app.world(), who.entity, "log"), before + 1);
        assert_eq!(message(&app), "Supplies gathered.");
        assert!(app.world().resource::<NativeActions>().progress.is_none());
    }

    #[test]
    fn unrelated_events_and_replaced_epochs_cannot_complete_current_feedback() {
        let (mut app, who, source) = fixture();
        gather(&mut app, source);
        let other = app
            .world()
            .resource::<island_content::IslandIds>()
            .id("Ada");
        journal::record_effect(
            app.world_mut(),
            Some(other),
            "VillageGathered",
            &json!({"patch":source,"quantity":1}),
        );
        journal::record_effect(
            app.world_mut(),
            Some(who.id),
            "VillageGathered",
            &json!({"patch":u32::MAX,"quantity":1}),
        );
        events::harvest(app.world_mut());
        let before = state_hash(app.world_mut());
        update(app.world_mut(), Some(who), true);
        assert_eq!(message(&app), "Gathering requested.");
        assert_eq!(state_hash(app.world_mut()), before);
        app.insert_resource(AgentIoState::default());
        let new_actor = actor(app.world()).unwrap();
        assert_ne!(new_actor.epoch, who.epoch);
        update(app.world_mut(), Some(new_actor), true);
        assert!(app.world().resource::<NativeActions>().progress.is_none());
        assert!(!message(&app).contains("gathered"));
    }

    #[test]
    fn application_refusal_is_plain_and_cannot_report_a_completed_request() {
        let (mut app, who, source) = fixture();
        gather(&mut app, source);
        // A world change between acceptance and application is a genuine refusal.
        app.world_mut()
            .entity_mut(who.entity)
            .insert(SimPos::new(30000, -30000));
        run_ticks(app.world_mut(), 1);
        tick(app.world_mut());
        let text = message(&app);
        assert!(text.contains("Could not start"), "{text}");
        assert!(
            !text.contains("VillageAct") && !text.contains("tick") && !text.contains("gathered")
        );
        assert!(app.world().resource::<NativeActions>().progress.is_none());
        assert!(
            app.world()
                .get::<island_content::village::Gathering>(who.entity)
                .is_none()
        );
    }

    #[test]
    fn an_external_completion_requires_the_accepted_handle_and_actor() {
        let (mut app, who, _) = fixture();
        let response = json!({"verb":"craft:village_saw","handle":123});
        let seq = events::last_seq(app.world());
        track(app.world_mut(), who, None, &response, seq);
        let other = app
            .world()
            .resource::<island_content::IslandIds>()
            .id("Ada");
        for (unit, handle) in [(who.id, 122), (other, 123)] {
            journal::record(app.world_mut(), Some(unit), || {
                JournalKind::ExternalAction {
                    handle,
                    verb: "craft:village_saw".into(),
                    status: "done".into(),
                    latency_ticks: None,
                }
            });
        }
        events::harvest(app.world_mut());
        update(app.world_mut(), Some(who), true);
        assert!(message(&app).contains("requested"));
        journal::record(app.world_mut(), Some(who.id), || {
            JournalKind::ExternalAction {
                handle: 123,
                verb: "craft:village_saw".into(),
                status: "done".into(),
                latency_ticks: None,
            }
        });
        events::harvest(app.world_mut());
        update(app.world_mut(), Some(who), true);
        assert!(message(&app).starts_with("Finished making plank"));
    }
}
