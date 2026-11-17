//! The resident inspector: the right column's panel on the resident selected
//! while overseeing. A header with who they are, a status badge (shape,
//! colour and word) with what they are doing, and four tabs: Overview, Day,
//! Why, and Home and job.
//!
//! Everything is read from existing simulation data: needs, wallet, pack,
//! `ResidentTraits`, the world's schedule blocks and the resident's decision
//! record (the journal's `VillageDecision`, or the live `resident::explain`
//! when the journal holds none for the running leaf). The card, its Follow
//! toggle and the decision log are view state: nothing here is saved, hashed
//! or read by the simulation, and no command is recorded.
pub(crate) mod model;
#[cfg(test)]
mod tests;

use crate::colony_panel::{Action as LedgerAct, Entry, Extra};
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale;
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as kit, Reach, Tone};
use crate::island_shell::model::{Column, column_width};
use crate::{IslandMode, game_ui as ui};
use bevy::input_focus::InputFocus;
use bevy::input_focus::tab_navigation::TabGroup;
use bevy::prelude::*;
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, ScrollArea};
use island_content::village::{self, Role, Villager, Wallet, catalog, colony, goap, resident};
use model::{NeedLevel, PlanView, Slot, WhyContext};
use npc_framework::kernel::journal::{Journal, JournalKind};
use npc_framework::prelude::{SimId, SimIndex, SimPos};
use npc_framework::view_helpers::{Selected, ViewSystems, VisualIndex, sim_to_world};
use serde_json::Value;
use std::collections::BTreeMap;

/// Seconds between two refreshes of an unchanged card.
const REFRESH_SECS: f32 = 0.25;
/// Camera distance when a roster row or the card focuses a resident.
pub(crate) const FOCUS_DISTANCE: f32 = 14.0;

fn slot_color(slot: Slot) -> Color {
    match slot {
        Slot::Free => t::PARCHMENT_300,
        Slot::Work => Color::srgb(0.36, 0.50, 0.66),
        Slot::Meal => t::HONEY_500,
        Slot::Leisure => Color::srgb(0.47, 0.72, 0.53),
        Slot::Sleep => Color::srgb(0.30, 0.27, 0.48),
    }
}

/// The inspector's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    Overview,
    Day,
    Why,
    HomeAndJob,
}

impl Tab {
    pub(crate) const ALL: [Self; 4] = [Self::Overview, Self::Day, Self::Why, Self::HomeAndJob];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Day => "Day",
            Self::Why => "Why",
            Self::HomeAndJob => "Home and job",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Request {
    Close,
    Follow,
    Tab(Tab),
    /// Back to the roster the resident was chosen from.
    People,
}

/// View state of the panel and the decision log behind its Why tab.
#[derive(Resource, Default)]
pub(crate) struct Inspector {
    follow: bool,
    requests: Vec<Request>,
    /// The resident the panel last showed.
    shown: Option<SimId>,
    elapsed: f32,
    tab: Tab,
    /// The resident chosen from the People page, while they are the subject:
    /// the header then offers the way back to the roster.
    roster: Option<SimId>,
    rows: BTreeMap<String, Entity>,
    /// What the header shows, so its icon and badge rebuild on change only.
    avatar: Option<u8>,
    status: Option<(Status, String)>,
    manual_serial: u64,
    epoch: Option<u64>,
    cursor: Option<(u64, u32)>,
    /// Latest decision record per resident, and the latest one that is not
    /// "carry on": what began the activity a resident is finishing.
    latest: BTreeMap<SimId, Value>,
    origin: BTreeMap<SimId, Value>,
    /// Each resident's latest job change: the job's key and whether the
    /// player's order made it (2.7).
    job_change: BTreeMap<SimId, (String, bool)>,
}

#[derive(Component)]
pub(crate) struct InspectorRoot;
#[derive(Component)]
struct InspectorBody;
#[derive(Component, Clone, Copy)]
struct InspectorAction(Request);
/// The header's way back to the People page.
#[derive(Component)]
struct BackButton;
#[derive(Component)]
struct NameText;
#[derive(Component)]
struct SubtitleText;
#[derive(Component)]
struct ActivityText;
/// Holds the status badge: shape, colour and word.
#[derive(Component)]
struct StatusSlot;
/// The ring in the trade's marker colour, and the trade's icon inside it.
#[derive(Component)]
struct AvatarRing;
#[derive(Component)]
struct AvatarSlot;

pub(crate) fn plugin(app: &mut App) {
    ui::install(app);
    crate::panel_scroll::install(app);
    app.init_resource::<Inspector>()
        .init_resource::<crate::island_camera::Follow>()
        .add_observer(activate)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            keys.in_set(ViewSystems::OverlayInput)
                .before(crate::island_shell::ShellInput),
        )
        .add_systems(
            Update,
            (track, process, refresh, follow)
                .chain()
                .in_set(ViewSystems::Present)
                .after(crate::colony_panel::refresh)
                .before(crate::island_alerts::track)
                .before(crate::island_camera::CameraFraming),
        );
}

/// The resident the card is about: the selection, while overseeing.
pub(crate) fn subject(world: &World) -> Option<(SimId, Entity)> {
    if !world.contains_resource::<Inspector>()
        || world.get_resource::<IslandMode>() != Some(&IslandMode::Inspector)
        || crate::player_hud::active(world)
        // The right column holds one thing: a ledger page hides the card.
        || (crate::island_shell::enabled(world) && crate::colony_panel::open(world))
    {
        return None;
    }
    let id = world.get_resource::<Selected>()?.0?;
    let entity = world.get_resource::<SimIndex>()?.get(id)?;
    world.get::<Villager>(entity).map(|_| (id, entity))
}

/// Whether the card is on screen. The alert feed folds to its header and the
/// marker key hides while it is.
pub(crate) fn open(world: &World) -> bool {
    subject(world).is_some()
}

/// Close the card: the selection is the card's only "open" state.
pub(crate) fn close(world: &mut World) {
    if let Some(mut selected) = world.get_resource_mut::<Selected>() {
        selected.0 = None;
    }
    if let Some(mut state) = world.get_resource_mut::<Inspector>() {
        state.follow = false;
        state.elapsed = REFRESH_SECS;
    }
}

/// F: follow the inspected resident, or stop following.
pub(crate) fn toggle_follow(world: &mut World) {
    if let Some(mut state) = world.get_resource_mut::<Inspector>() {
        state.requests.push(Request::Follow);
    }
}

/// The resident was chosen from the roster: their inspector offers the way back.
pub(crate) fn from_roster(world: &mut World, id: SimId) {
    if let Some(mut state) = world.get_resource_mut::<Inspector>() {
        state.roster = Some(id);
    }
}

/// What the header's People button does, for the roster's own tests.
#[cfg(test)]
pub(crate) fn back_to_people(world: &mut World) {
    let offered = {
        let state = world.resource::<Inspector>();
        state.roster.is_some() && state.roster == subject(world).map(|(id, _)| id)
    };
    assert!(
        offered,
        "the way back is offered for a resident chosen from the roster"
    );
    world
        .resource_mut::<Inspector>()
        .requests
        .push(Request::People);
    process(world);
}

/// Show one of the panel's tabs: a link on another tab leads here.
pub(crate) fn show_tab(world: &mut World, tab: Tab) {
    if let Some(mut state) = world.get_resource_mut::<Inspector>() {
        state.requests.push(Request::Tab(tab));
    }
}

/// Select a resident, open the card on them and bring the camera there.
pub(crate) fn inspect(world: &mut World, id: SimId) {
    world.get_resource_or_init::<Selected>().0 = Some(id);
    if let Some(mut state) = world.get_resource_mut::<Inspector>() {
        state.elapsed = REFRESH_SECS;
    }
    let position = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(id))
        .and_then(|entity| world.get::<SimPos>(entity))
        .copied();
    if let Some(position) = position {
        crate::island_camera::focus(world, sim_to_world(position), FOCUS_DISTANCE);
    }
}

fn good_name(world: &World, kind: &str) -> String {
    catalog::display_in(world, kind).map_or_else(
        || {
            kind.strip_prefix("village_")
                .unwrap_or(kind)
                .replace('_', " ")
        },
        str::to_owned,
    )
}

/// Display name of a good or a recipe key.
fn display_name(world: &World, key: &str) -> String {
    village::frontier::spec(world)
        .and_then(|spec| spec.economy.recipe(key))
        .map_or_else(|| good_name(world, key), |recipe| recipe.display.clone())
}

/// The rest leaves that are themselves a rest and keep their own words:
/// at home, warming up, asleep at home or asleep at the inn (a rented bed).
pub(crate) fn own_rest_label(leaf: &str) -> Option<&str> {
    matches!(
        leaf,
        "resting at home" | "warming up at home" | "asleep at home" | "asleep at the inn"
    )
    .then_some(leaf)
}

/// What a resident is doing, in plain words: the running leaf's own label,
/// made concrete by the physical action where the leaf only says "work" or
/// "carry on". Shared by the card, the roster and the caption in the world.
pub(crate) fn activity_label(world: &World, entity: Entity) -> String {
    let (physical, _, _) = village::activity(world, entity);
    match physical {
        "dead" => return "died".into(),
        "awaiting_arrival" => return "not yet arrived".into(),
        _ => {}
    }
    // The simulation's own label of the running leaf; the older island has
    // none and names the goal instead.
    let leaf = village::view::activity_label(world, entity)
        .filter(|_| true)
        .or_else(|| village::behavior::decision(world, entity));
    let generic = matches!(
        leaf,
        None | Some(
            "finishing the current activity"
                | "finish current activity"
                | "working to a plan"
                | "gathering"
                | "crafting at the workplace"
                | "supply the village"
        )
    );
    // A rest outlasts the leaf that led to it: somebody asleep is not "going
    // home". The two leaves that are themselves a rest keep their own words.
    if physical == "resting" {
        if let Some(own) = leaf.and_then(own_rest_label) {
            return own.to_owned();
        }
        // Asleep or resting awake is the simulation's call, not the hour's.
        return match village::view::repose(world, entity) {
            village::view::Repose::Asleep => "sleeping",
            _ => "resting",
        }
        .to_owned();
    }
    // Home already, and staying there.
    if leaf == Some("going home") && physical == "idle" {
        return "at home".to_owned();
    }
    // Building and carrying for the village name the site, the store or the
    // stall: "building Harbour cottage 2", "carrying planks to ...".
    if let Some(leaf) = leaf
        && let Some(errand) = crate::island_build::errand(world, entity, leaf)
    {
        return errand;
    }
    if !generic {
        return leaf.unwrap_or(physical).to_owned();
    }
    let concrete = match physical {
        "gathering" => world
            .get::<village::Gathering>(entity)
            .and_then(|gathering| world.get_resource::<SimIndex>()?.get(gathering.patch))
            .and_then(|patch| world.get::<village::ResourcePatch>(patch))
            .map(|patch| format!("gathering {}", good_name(world, &patch.kind))),
        "crafting" => world
            .get::<npc_framework::crafting::Crafting>(entity)
            .map(|craft| display_name(world, &craft.recipe).to_lowercase()),
        // A stage of work on a village site, or the resident's own cottage.
        "building" => Some(
            crate::island_build::building_site(world, entity).map_or_else(
                || "building a cottage".to_owned(),
                |site| format!("building {site}"),
            ),
        ),
        "restoring" => Some("tending the land".to_owned()),
        "hunting" => Some("hunting".to_owned()),
        _ => None,
    };
    if let Some(concrete) = concrete {
        return concrete;
    }
    let job = village::production::job_summary(world, entity).map(|job| job.to_lowercase());
    match (leaf, job) {
        (Some("working to a plan" | "supply the village"), Some(job)) => {
            format!("on the way to {job}")
        }
        (Some(leaf), _) if !leaf.starts_with("finish") => leaf.to_owned(),
        (_, Some(job)) => format!("on the way to {job}"),
        _ => match physical {
            "blocked" => "waiting for the way to clear".to_owned(),
            other => other.to_owned(),
        },
    }
}

/// One need bar: label, milli-units and how pressing it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NeedBar {
    pub(crate) label: &'static str,
    pub(crate) value: i32,
    pub(crate) level: NeedLevel,
}

/// Food, water, warmth, rest and morale, against the world's own thresholds:
/// a bar is "low" below the level at which the resident acts on it and
/// "critical" below the level that interrupts everything else. Morale has no
/// simulation threshold; it is low under a half and critical under a quarter.
pub(crate) fn needs(world: &World, entity: Entity) -> [NeedBar; 5] {
    let levels = |policy: &village::policy::IslandAiPolicy| {
        (
            policy.critical_need,
            policy.hunger_threshold,
            policy.thirst_threshold,
            policy.recovery_threshold,
        )
    };
    let (critical, hunger, thirst, recovery) = village::policy::active(world).map_or_else(
        || levels(&village::policy::IslandAiPolicy::default()),
        levels,
    );
    let bar = |label, name: &str, low: i32| {
        let value = village::economy::need(world, entity, name);
        NeedBar {
            label,
            value,
            level: model::need_level(value, low, critical),
        }
    };
    let morale = world
        .get::<colony::SocialLife>(entity)
        .map_or(75_000, |social| social.morale);
    [
        bar("Food", "Hunger", hunger),
        bar("Water", "Thirst", thirst),
        bar("Warmth", "Warmth", recovery),
        bar("Rest", "Energy", recovery),
        NeedBar {
            label: "Morale",
            value: morale,
            level: model::need_level(morale, 50_000, 25_000),
        },
    ]
}

/// The most pressing of the four bodily needs, for the roster's indicator.
pub(crate) fn worst_need(world: &World, entity: Entity) -> NeedLevel {
    needs(world, entity)[..4]
        .iter()
        .map(|bar| bar.level)
        .max()
        .unwrap_or_default()
}

pub(crate) fn settlement_name(world: &World, entity: Entity) -> Option<String> {
    let id = village::frontier::settlements::member(world, entity)?;
    let settlement = world.get_resource::<SimIndex>()?.get(id)?;
    world
        .get::<village::frontier::Settlement>(settlement)
        .map(|place| place.label.clone())
}

/// The resident's work plan in plain words for Home and job: what it
/// supplies and its next steps, or that it follows the daily routine. The
/// planner's own trace (raw ids, states searched) stays out of the panel.
pub(crate) fn plan_words(world: &World, entity: Entity) -> String {
    // The simulation's readable plan line; the raw trace (ids, states
    // searched) is for the developer view only.
    let readable = village::view::plan_lines(world, entity).readable;
    if !readable.trim().is_empty() {
        return readable;
    }
    live_plan(world, entity).map_or_else(
        || "No work plan right now: the daily routine and the village's needs decide what comes next.".to_owned(),
        |plan| model::plan_line(&plan),
    )
}

/// The plan the simulation holds for the resident now, in display words.
fn live_plan(world: &World, entity: Entity) -> Option<PlanView> {
    let memo = world.get::<goap::PlanMemo>(entity)?;
    let steps = memo
        .plan
        .steps
        .iter()
        .take(2)
        .map(|step| match step {
            goap::PlanStep::Buy { kind, .. } => model::plan_step("buy", &good_name(world, kind)),
            goap::PlanStep::Gather { kind, .. } => {
                model::plan_step("gather", &good_name(world, kind))
            }
            goap::PlanStep::Craft { recipe, .. } => {
                model::plan_step("craft", &display_name(world, recipe))
            }
        })
        .collect();
    Some(PlanView {
        goal: good_name(world, &memo.plan.goal),
        steps,
        remaining: memo.plan.steps.len(),
    })
}

/// The routine (behaviour tree) the resident runs, in words: its data
/// name with spaces ("island life").
pub(crate) fn routine_name(world: &World, entity: Entity) -> Option<String> {
    let id = island_content::village::resident::tree_for(world, entity)?;
    let registry = world.get_resource::<npc_framework::ai::AiRegistry>()?;
    Some(registry.tree(id).name.replace('_', " "))
}

/// The "Why" text for one resident: what they chose, what would have come
/// first and why it did not, and the plan they are working to.
#[cfg(test)]
pub(crate) fn why_text(world: &World, id: SimId, entity: Entity) -> String {
    match why_view(world, id, entity) {
        Ok(why) => why.text(),
        Err(sentence) => sentence,
    }
}

/// Today's sit-down meal on the Day tab, read from the resident's
/// `DayRhythm` (the day of their last sit-down meal) and their own meal
/// blocks. The day is the simulation's (`tick / day_length`), as the meal
/// rule counts it. The played character runs no resident routine and sits
/// down to no such meal, so the line is left out for them.
fn todays_meal(
    world: &World,
    id: SimId,
    entity: Entity,
    blocks: &[(resident::BlockKind, i32, i32)],
    permille: i32,
) -> Option<String> {
    if world
        .get_resource::<village::VillageState>()
        .is_some_and(|state| state.played == id)
    {
        return None;
    }
    let meals: Vec<(i32, i32)> = blocks
        .iter()
        .filter(|(kind, _, _)| *kind == resident::BlockKind::Meal)
        .map(|(_, from, to)| (*from, *to))
        .collect();
    let day_length = world
        .get_resource::<npc_framework::prelude::SimConfig>()
        .map_or(1, |config| config.day_length.max(1));
    let day = world
        .get_resource::<npc_framework::prelude::SimClock>()
        .map_or(0, |clock| clock.tick / day_length);
    let eaten = world
        .get::<resident::DayRhythm>(entity)
        .is_some_and(|rhythm| rhythm.last_meal == Some(day));
    model::meal_line(eaten, &meals, permille)
}

/// The parts of "Why": what they chose, what was passed over, what was
/// checked and fine, and the plan. `Err` is one sentence where there is no
/// decision to explain.
pub(crate) fn why_view(world: &World, id: SimId, entity: Entity) -> Result<model::Why, String> {
    let mut why = why_parts(world, id, entity)?;
    // A plan that begins with a purchase says where it will be made: the
    // simulation's own choice of seller for this resident, now.
    if let Some(line) = purchase_line(world, entity) {
        why.plan = Some(match why.plan.take() {
            Some(plan) => format!("{plan} {line}"),
            None => line,
        });
    }
    Ok(why)
}

/// Where the next step of a resident's plan will buy its good.
fn purchase_line(world: &World, entity: Entity) -> Option<String> {
    let memo = world.get::<goap::PlanMemo>(entity)?;
    let goap::PlanStep::Buy { kind, .. } = memo.plan.steps.first()? else {
        return None;
    };
    let seller = village::view::supplier(world, entity, kind).and_then(|id| {
        let seller = world.get_resource::<SimIndex>()?.get(id)?;
        world
            .get::<village::VillagePlace>(seller)
            .map(|place| place.label.clone())
            .or_else(|| {
                world
                    .get::<Villager>(seller)
                    .map(|person| person.name.clone())
            })
    });
    Some(model::purchase_sentence(
        &good_name(world, kind),
        seller.as_deref(),
    ))
}

fn why_parts(world: &World, id: SimId, entity: Entity) -> Result<model::Why, String> {
    if !village::living(world, entity) {
        return Err("Their story has ended.".into());
    }
    if world
        .get::<Villager>(entity)
        .is_some_and(|person| person.role == Role::Visitor)
    {
        return Err("This is your character: you decide what they do.".into());
    }
    let state = world.get_resource::<Inspector>();
    let latest = state.and_then(|state| state.latest.get(&id));
    let live = resident::explain(world, entity);
    // The record written when the running leaf was chosen knows why an
    // option could not start. The simulation keeps it and says whether it
    // still stands for that leaf (`recorded`).
    let recorded = village::view::latest_decision(world, entity)
        .filter(|decision| decision.recorded)
        .map(|decision| model::record_of(&decision));
    let record = match (&live, &recorded, latest) {
        (Some(_), Some(recorded), _) => recorded,
        // A session whose journal the simulation did not trace: the panel's
        // own copy of the record, while it names the same leaf.
        (Some(live), None, Some(latest))
            if latest["node"] == live["node"] && latest["param"] == live["param"] =>
        {
            latest
        }
        (Some(live), ..) => live,
        // Between two decisions no leaf runs: the last choice still explains
        // what is on screen.
        (None, _, Some(latest)) => latest,
        (None, _, None) => {
            return Err(match village::behavior::decision(world, entity) {
                // A world whose residents do not run the data-authored tree.
                Some(goal) => format!("Current aim: {goal}."),
                None => "Between tasks: about to decide what to do next.".into(),
            });
        }
    };
    let origin = state.and_then(|state| state.origin.get(&id));
    let bars = needs(world, entity);
    let names = |key: &str| display_name(world, key);
    let context = WhyContext {
        names: &names,
        hunter: world
            .get::<Villager>(entity)
            .is_some_and(|person| person.role == Role::Hunter),
        hunger: bars[0].level,
        thirst: bars[1].level,
        warmth: bars[2].level,
        origin,
        plan: live_plan(world, entity),
        activity: Some(activity_label(world, entity)),
    };
    Ok(model::why(record, &context))
}

/// Family and bed of a resident, from the simulation's household view: who
/// they belong to and where they sleep. Nothing in a world without families.
fn family_entries(world: &World, id: SimId, entity: Entity) -> Vec<Entry> {
    let named = |person: &village::view::PersonView| {
        if person.child.is_some() {
            format!("{} ({})", person.name, person.age_years)
        } else {
            person.name.clone()
        }
    };
    let mut lines = Vec::new();
    if let Some(home) = village::view::household(world, id) {
        lines.extend(model::family_lines(&model::Family {
            label: village::view::age(world, id)
                .map(|age| age.label)
                .unwrap_or_default(),
            stage: home.person.stage.to_owned(),
            age_years: home.person.age_years,
            partner: home.partner.as_ref().map(named),
            courting: home.courting.as_ref().map(named),
            children: home.children.iter().map(named).collect(),
            parents: home.parents.iter().map(named).collect(),
        }));
    }
    if let Some((house, bed)) = village::view::bed_of(world, id)
        && let Some(home) = village::view::home(world, house)
    {
        lines.push(model::bed_line(
            &home.label,
            bed,
            home.beds.len() as u32,
            village::view::repose(world, entity),
        ));
    }
    if lines.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![
        Entry::caption("family_title", "Family and bed").with_tip(Tooltip::new(
            "Family and bed",
            "Who they live with and where they sleep, from the village census.",
        )),
    ];
    rows.extend(
        lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| Entry::text(&format!("family{index}"), line, false)),
    );
    rows
}

/// The resident's closest bonds, from the simulation's relationships:
/// five at most, strongest first, each with its tier.
fn friends_entries(world: &World, id: SimId) -> Vec<Entry> {
    let relations = village::view::relationships(world, id);
    // The played character can appear twice: the bond the game keeps with
    // them and the resident's own fondness. The game's bond is the one shown.
    let mut seen = Vec::new();
    let mut listed = Vec::new();
    for relation in relations
        .iter()
        .filter(|relation| relation.with_player)
        .chain(relations.iter().filter(|relation| !relation.with_player))
    {
        if !seen.contains(&relation.other) {
            seen.push(relation.other);
            listed.push((
                relation.score,
                (
                    relation.name.clone(),
                    relation.tier.clone(),
                    relation.with_player,
                ),
            ));
        }
    }
    // Strongest first again, as the simulation lists them.
    listed.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    let lines = model::friend_lines(&listed.into_iter().map(|(_, line)| line).collect::<Vec<_>>());
    if lines.is_empty() {
        return Vec::new();
    }
    let mut rows = vec![Entry::caption("friends_title", "Friends").with_tip(Tooltip::new(
        "Friends",
        "Their closest bonds, strongest first. Talk, sales and gifts bring people closer; time apart fades a bond.",
    ))];
    rows.extend(
        lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| Entry::text(&format!("friend{index}"), line, false)),
    );
    rows
}

/// What the resident carries, most first: `(catalogue key, name, count)`.
fn carried(world: &World, entity: Entity) -> (u32, Vec<(String, String, u32)>) {
    let coins = world.get::<Wallet>(entity).map_or(0, |wallet| wallet.coins);
    let mut goods: BTreeMap<String, u32> = BTreeMap::new();
    for (_, item) in village::economy::free_items(world, entity) {
        if item.kind != catalog::COINS {
            *goods.entry(item.kind.clone()).or_default() += item.stack;
        }
    }
    let mut goods: Vec<(String, String, u32)> = goods
        .into_iter()
        .map(|(kind, count)| {
            let name = good_name(world, &kind);
            (kind, name, count)
        })
        .collect();
    goods.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    (coins, goods)
}

/// The home and the workplace of a resident, where each has a card to open.
fn places(world: &World, id: SimId, person: &Villager) -> Vec<(&'static str, Icon, String, SimId)> {
    let mut found = Vec::new();
    let home = world.get_resource::<SimIndex>().and_then(|index| {
        index.map.iter().find_map(|(house, entity)| {
            let home = world.get::<colony::Household>(*entity)?;
            home.residents
                .contains(&id)
                .then(|| (*house, home.label.clone()))
        })
    });
    if let Some((house, label)) = home
        && crate::island_build::card(world, house).is_some()
    {
        found.push(("Home", Icon::Homes, label, house));
    }
    let work = world
        .get_resource::<SimIndex>()
        .and_then(|index| index.get(person.workplace))
        .and_then(|place| world.get::<village::VillagePlace>(place))
        .map(|place| place.label.clone());
    if let Some(label) = work
        && person.role != Role::Visitor
        && crate::island_build::card(world, person.workplace).is_some()
    {
        found.push(("Work", Icon::Craft, label, person.workplace));
    }
    found
}

/// The rows of the open tab for the inspected resident.
pub(crate) fn entries(world: &World) -> Vec<Entry> {
    let Some((id, entity)) = subject(world) else {
        return Vec::new();
    };
    let Some(person) = world.get::<Villager>(entity) else {
        return Vec::new();
    };
    let alive = village::living(world, entity);
    let resident = person.role != Role::Visitor && alive;
    let tab = world.resource::<Inspector>().tab;
    let mut rows = Vec::new();
    match tab {
        Tab::Overview => {
            if alive {
                for (index, bar) in needs(world, entity).into_iter().enumerate() {
                    let percent = model::percent_of(bar.value).clamp(0, 100) as u8;
                    let status = crate::colony_panel::need_status(bar.level);
                    let picture = [
                        Icon::Food,
                        Icon::Water,
                        Icon::Warmth,
                        Icon::Rest,
                        Icon::Morale,
                    ][index.min(4)];
                    rows.push(
                        Entry::text(&format!("need{index}"), bar.label, false)
                            .with_icon(Some(picture))
                            .with(Extra::Meter {
                                percent,
                                status,
                                value: format!("{percent}%"),
                            })
                            .with_tip(
                                Tooltip::new(bar.label, model::need_sentence(bar.level))
                                    .icon(picture)
                                    .value(format!("{percent}%")),
                            ),
                    );
                }
            }
            // Who they belong to and where they sleep, before what they are like.
            if resident {
                rows.extend(family_entries(world, id, entity));
                rows.extend(friends_entries(world, id));
            }
            if resident && let Some(spec) = resident::spec(world) {
                let words =
                    model::trait_words(resident::traits(world, entity), spec.traits.sociable_min);
                rows.push(
                    Entry::caption("traits_title", "Traits").with_tip(Tooltip::new(
                        "Traits",
                        "What this resident is like: it shapes their routine and choices.",
                    )),
                );
                rows.push(
                    Entry::text("traits", "Traits", false).with(Extra::Chips(
                        words
                            .iter()
                            .map(|word| (None, model::capitalise(word)))
                            .collect(),
                    )),
                );
            }
            let (coins, goods) = carried(world, entity);
            let mut chips = vec![(Some(Icon::Coins), coins.to_string())];
            chips.extend(goods.iter().take(5).map(|(kind, name, count)| {
                (
                    Icon::of_good(kind),
                    crate::game_ui::words::count(u64::from(*count), name),
                )
            }));
            if goods.len() > 5 {
                chips.push((None, format!("{} more", goods.len() - 5)));
            }
            rows.push(
                Entry::caption("carrying_title", "Carrying").with_tip(Tooltip::new(
                    "Carrying",
                    "Coins and goods in their pack, most first.",
                )),
            );
            rows.push(Entry::text("carrying", "Carrying", false).with(Extra::Chips(chips)));
            rows.push(
                Entry::caption("why_title", "Why now").with_tip(Tooltip::new(
                    "Why now",
                    "What they are doing, and why. The Why tab has the whole decision.",
                )),
            );
            match why_view(world, id, entity) {
                Ok(why) => {
                    rows.push(Entry::strong("why_now", why.chosen));
                    rows.push(Entry::link(
                        "why_more",
                        "See what was passed over and the plan",
                        Icon::Goal,
                        LedgerAct::InspectorTab(Tab::Why),
                    ));
                }
                Err(sentence) => rows.push(Entry::strong("why_now", sentence)),
            }
            for (word, picture, label, place) in places(world, id, person) {
                rows.push(Entry::link(
                    format!("place_{word}"),
                    format!("{word} \u{b7} {label}"),
                    picture,
                    LedgerAct::Site(place),
                ));
            }
            if resident {
                rows.push(Entry::link(
                    "manage_link",
                    "Change job or home",
                    Icon::Homes,
                    LedgerAct::InspectorTab(Tab::HomeAndJob),
                ));
            }
        }
        Tab::Day => match resident::spec(world).filter(|_| resident) {
            Some(_) => {
                // The simulation's own blocks for this resident, already
                // moved by their hours.
                let blocks: Vec<_> = village::view::schedule_blocks(world, entity)
                    .into_iter()
                    .map(|block| (block.kind, block.from_permille, block.to_permille))
                    .collect();
                let plan = model::plan_of_blocks(&blocks);
                let permille = resident::day_permille(world);
                rows.push(
                    Entry::text("day_strip", "Day", false).with(Extra::Strip(
                        plan.iter()
                            .map(|segment| {
                                let color = slot_color(segment.slot).to_srgba();
                                (
                                    (segment.to - segment.from) as f32 / 10.0,
                                    [color.red, color.green, color.blue],
                                )
                            })
                            .collect(),
                        permille as f32 / 10.0,
                    )),
                );
                // "Meal now · work 13:42 · meal 20:54": one change per line.
                for (index, part) in model::schedule_caption(&plan, permille)
                    .split(" \u{b7} ")
                    .enumerate()
                {
                    let line = model::capitalise(part);
                    rows.push(if index == 0 {
                        Entry::strong("day_now", line)
                    } else {
                        Entry::text(&format!("day_next{index}"), line, false)
                    });
                }
                if let Some(line) = todays_meal(world, id, entity, &blocks, permille) {
                    rows.push(Entry::text("day_meal", line, false));
                }
            }
            None => rows.push(Entry::text(
                "day_none",
                "No daily plan: this is not a working resident.",
                false,
            )),
        },
        Tab::Why => match why_view(world, id, entity) {
            Ok(why) => {
                // Residents may run different routines (by job or age).
                if let Some(routine) = routine_name(world, entity) {
                    rows.push(Entry::caption(
                        "why_routine",
                        format!(
                            "Daily routine: {}.",
                            crate::game_ui::words::capital(&routine)
                        ),
                    ));
                }
                rows.push(Entry::choices(
                    "why_behaviour",
                    vec![
                        (
                            "Save behaviour to file".to_owned(),
                            LedgerAct::BehaviourFile(crate::BehaviourFile::Save),
                        ),
                        (
                            "Reload behaviour from file".to_owned(),
                            LedgerAct::BehaviourFile(crate::BehaviourFile::Reload),
                        ),
                    ],
                ));
                rows.push(Entry::text("why_chose_title", "Chose", true));
                rows.push(Entry::strong("why_chose", why.chosen));
                if !why.alternatives.is_empty() {
                    rows.push(Entry::text("why_passed_title", "Passed over", true));
                    for (index, line) in why.alternatives.iter().enumerate() {
                        let (title, reason) = model::title_and_reason(line);
                        rows.push(Entry::strong(&format!("why_passed{index}"), title));
                        if let Some(reason) = reason {
                            rows.push(Entry::text(
                                &format!("why_passed_reason{index}"),
                                reason,
                                false,
                            ));
                        }
                    }
                }
                if let Some(ruled_out) = &why.ruled_out {
                    rows.push(Entry::text("why_checked_title", "Checked and fine", true));
                    rows.push(
                        Entry::text("why_checked", "Checked", false).with(Extra::Chips(
                            model::checked_chips(ruled_out)
                                .into_iter()
                                .map(|chip| (None, chip))
                                .collect(),
                        )),
                    );
                }
                if let Some(plan) = why.plan {
                    rows.push(Entry::text("why_plan_title", "Plan", true));
                    rows.push(Entry::text("why_plan", plan, false));
                }
            }
            Err(sentence) => rows.push(Entry::strong("why_none", sentence)),
        },
        Tab::HomeAndJob => {
            if resident {
                rows.extend(crate::colony_panel::manage_entries(world, id, entity));
            } else {
                rows.push(Entry::text(
                    "manage_none",
                    "Nothing to arrange: this is not a working resident.",
                    false,
                ));
            }
        }
    }
    rows
}

/// The inspector for a test that walks the Tab ring.
#[cfg(test)]
pub(crate) fn spawn_for_tests(world: &mut World) {
    world.init_resource::<Inspector>();
    spawn(world);
}

fn spawn(world: &mut World) {
    let root = world
        .spawn((
            Name::new("Resident inspector"),
            InspectorRoot,
            crate::game_ui::sound::PanelSound::default(),
            crate::island_shell::focus::FocusRegion::Inspector,
            TabGroup::new(5),
            GlobalZIndex(t::layer::PANEL),
            kit::panel(Node {
                position_type: PositionType::Absolute,
                top: px(t::TOP_BAR + t::SCREEN_MARGIN),
                right: px(t::SCREEN_MARGIN),
                width: px(t::COLUMN_INSPECTOR),
                max_width: percent(94),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            }),
        ))
        .id();
    // Header: the trade's ring and icon, the name, trade and village.
    let header = world
        .spawn((
            ChildOf(root),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(10),
                min_height: px(52),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    let ring = world
        .spawn((
            ChildOf(header),
            AvatarRing,
            kit::Swatch::Fill,
            Tone::new(Surface::Paper, kit::Role::Sunken),
            Node {
                width: px(44),
                height: px(44),
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(22)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::NONE),
            BorderColor::all(t::BORDER_CONTROL),
        ))
        .id();
    world.spawn((
        ChildOf(ring),
        AvatarSlot,
        Node {
            width: px(24),
            height: px(24),
            ..default()
        },
    ));
    let who = world
        .spawn((
            ChildOf(header),
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_width: px(0),
                overflow: Overflow::clip_x(),
                ..default()
            },
        ))
        .id();
    let name = kit::label(world, who, "", t::HEADING, Tone::text(Surface::Paper));
    world.entity_mut(name).insert(NameText);
    let subtitle = kit::label(world, who, "", t::DENSE, Tone::secondary(Surface::Paper));
    world.entity_mut(subtitle).insert(SubtitleText);
    let back = kit::icon_button(
        world,
        header,
        "Back to People",
        Icon::People,
        Surface::Paper,
        Reach::Ring,
        (InspectorAction(Request::People), BackButton),
    );
    world.entity_mut(back).insert(
        Tooltip::new(
            "People",
            "Back to the roster this resident was chosen from.",
        )
        .icon(Icon::People)
        .key("F3"),
    );
    kit::show(world, back, false);
    let follow = kit::icon_button(
        world,
        header,
        "Follow",
        Icon::Follow,
        Surface::Paper,
        Reach::Ring,
        InspectorAction(Request::Follow),
    );
    world.entity_mut(follow).insert(
        Tooltip::new("Follow", "Keep the camera on this resident.")
            .icon(Icon::Follow)
            .key("F")
            .detail(["Following ends when you move the camera yourself."]),
    );
    let close = kit::icon_button(
        world,
        header,
        "Close inspector",
        Icon::Close,
        Surface::Paper,
        Reach::Ring,
        InspectorAction(Request::Close),
    );
    world
        .entity_mut(close)
        .insert(Tooltip::new("Close", "Close this card.").key("Esc"));
    // Status: shape, colour and word, then what they are doing.
    let status = world
        .spawn((
            ChildOf(root),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    world.spawn((
        ChildOf(status),
        StatusSlot,
        bevy::picking::hover::Hovered::default(),
        Node {
            flex_shrink: 0.0,
            ..default()
        },
    ));
    let activity = kit::paragraph(world, status, "", t::DENSE, Tone::text(Surface::Paper));
    world.entity_mut(activity).insert(ActivityText);
    let tabs = kit::tabs(world, root);
    for tab in Tab::ALL {
        let entity = kit::tab(
            world,
            tabs,
            tab.label(),
            None,
            Reach::Ring,
            InspectorAction(Request::Tab(tab)),
        );
        world.get_mut::<Node>(entity).expect("tab").padding = UiRect::horizontal(px(4));
    }
    let body = world
        .spawn((
            ChildOf(root),
            InspectorBody,
            ScrollArea,
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
        ))
        .id();
    crate::panel_scroll::attach_rail(world, root, body, "Inspector");
}

fn activate(
    event: On<Activate>,
    actions: Query<(&InspectorAction, Has<InteractionDisabled>)>,
    mut state: ResMut<Inspector>,
) {
    if let Ok((action, false)) = actions.get(event.entity) {
        state.requests.push(action.0);
    }
}

/// Esc closes the card before it reaches the ledger or the pause menu.
pub(crate) fn keys(world: &mut World) {
    if !open(world) {
        return;
    }
    let focused = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    if focused.is_some_and(|entity| world.get::<bevy::text::EditableText>(entity).is_some()) {
        return;
    }
    let escape = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape));
    if !escape {
        return;
    }
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset(KeyCode::Escape);
    // Focus on one of the card's controls goes with the card.
    if focused.is_some_and(|entity| world.get::<InspectorAction>(entity).is_some()) {
        world.resource_mut::<InputFocus>().clear();
    }
    close(world);
}

/// Keep the latest decision record of every resident. The journal is a
/// bounded ring, so one pass over its unread tail is bounded too.
pub(crate) fn track(world: &mut World) {
    let epoch = npc_framework::agent_io::api::epoch(world);
    if world.resource::<Inspector>().epoch != Some(epoch) {
        let mut state = world.resource_mut::<Inspector>();
        state.epoch = Some(epoch);
        state.cursor = None;
        state.latest.clear();
        state.origin.clear();
        state.job_change.clear();
        state.follow = false;
        state.avatar = None;
        state.status = None;
    }
    let cursor = world.resource::<Inspector>().cursor;
    let Some(journal) = world.get_resource::<Journal>() else {
        return;
    };
    let newest = journal.entries.back().map(|entry| (entry.tick, entry.seq));
    if newest == cursor {
        return;
    }
    // A journal that moved backwards belongs to another timeline.
    let rewound = matches!((newest, cursor), (Some(newest), Some(cursor)) if newest < cursor);
    let unread = |entry: &&npc_framework::kernel::journal::JournalEntry| {
        rewound || cursor.is_none_or(|cursor| (entry.tick, entry.seq) > cursor)
    };
    let mut fresh: Vec<(SimId, Value)> = journal
        .entries
        .iter()
        .rev()
        .take_while(unread)
        .filter_map(|entry| match &entry.kind {
            JournalKind::Effect {
                effect,
                applied: true,
            } if effect.get("type").and_then(Value::as_str) == Some("VillageDecision") => {
                Some((entry.unit?, serde_json::to_value(effect).ok()?))
            }
            _ => None,
        })
        .collect();
    fresh.reverse();
    let mut changes: Vec<(SimId, (String, bool))> = journal
        .entries
        .iter()
        .rev()
        .take_while(unread)
        .filter_map(|entry| match &entry.kind {
            JournalKind::Effect {
                effect,
                applied: true,
            } if effect.get("type").and_then(Value::as_str)
                == Some("RegionalOccupationChanged") =>
            {
                Some((
                    entry.unit?,
                    (
                        effect.get("to")?.as_str()?.to_owned(),
                        effect.get("by_order").and_then(Value::as_bool) == Some(true),
                    ),
                ))
            }
            _ => None,
        })
        .collect();
    changes.reverse();
    let mut state = world.resource_mut::<Inspector>();
    if rewound {
        state.latest.clear();
        state.origin.clear();
    }
    state.cursor = newest;
    for (id, change) in changes {
        state.job_change.insert(id, change);
    }
    for (id, record) in fresh {
        if record["action"].as_str() != Some("life.continue") {
            state.origin.insert(id, record.clone());
        }
        state.latest.insert(id, record);
    }
}

pub(crate) fn process(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<Inspector>().requests);
    for request in requests {
        match request {
            Request::Close => {
                close(world);
                world.resource_mut::<InputFocus>().clear();
            }
            Request::Tab(tab) => {
                let mut state = world.resource_mut::<Inspector>();
                state.tab = tab;
                state.elapsed = REFRESH_SECS;
                for mut scroll in world
                    .query_filtered::<&mut ScrollPosition, With<InspectorBody>>()
                    .iter_mut(world)
                {
                    scroll.0 = Vec2::ZERO;
                }
            }
            Request::People => {
                // The page hides the inspector; the resident stays selected.
                crate::colony_panel::open_page(
                    world,
                    crate::island_shell::model::LedgerPage::People,
                );
            }
            Request::Follow => {
                let following = {
                    let mut state = world.resource_mut::<Inspector>();
                    state.follow = !state.follow;
                    state.elapsed = REFRESH_SECS;
                    state.follow
                };
                // Following starts from a close look at the resident.
                if following && let Some((id, _)) = subject(world) {
                    inspect(world, id);
                }
            }
        }
    }
}

fn set_text<M: Component>(world: &mut World, value: &str) {
    for mut text in world.query_filtered::<&mut Text, With<M>>().iter_mut(world) {
        if text.0 != value {
            text.0 = value.into();
        }
    }
}

fn show<M: Component>(world: &mut World, shown: bool) {
    let display = if shown { Display::Flex } else { Display::None };
    for mut node in world.query_filtered::<&mut Node, With<M>>().iter_mut(world) {
        if node.display != display {
            node.display = display;
        }
    }
}

/// The badge of a resident: fine, in need or in danger, and of what.
/// How a resident is, as the badge says it: the shape's status and the words.
pub(crate) fn status_of(world: &World, entity: Entity) -> (Status, String) {
    if !village::living(world, entity) {
        return (Status::Idle, "Life ended".to_owned());
    }
    let bars = needs(world, entity);
    model::status_words(&bars.map(|bar| (bar.label, bar.level)))
}

/// The resident the panel shows has left the world (a body is removed a day
/// after a death): their name, from the record of the departed.
fn gone(world: &World) -> Option<Option<String>> {
    let shown = world.get_resource::<Inspector>()?.shown?;
    if world.get_resource::<Selected>()?.0 != Some(shown)
        || world.get_resource::<SimIndex>()?.get(shown).is_some()
    {
        return None;
    }
    Some(
        village::view::departed(world)
            .into_iter()
            .find(|record| record.resident == Some(shown))
            .map(|record| record.name),
    )
}

fn refresh(world: &mut World) {
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    // Somebody who was laid to rest cannot be inspected any more: let the
    // selection go and say why, once.
    if let Some(name) = gone(world) {
        close(world);
        crate::island_shell::notice(world, model::gone_sentence(name.as_deref()));
    }
    let current = subject(world);
    {
        let mut state = world.resource_mut::<Inspector>();
        state.elapsed += delta;
        let changed = state.shown != current.map(|(id, _)| id);
        if changed {
            // Following and the open tab belong to one resident.
            state.follow = false;
            state.tab = Tab::Overview;
            state.shown = current.map(|(id, _)| id);
        }
        if !changed && state.elapsed < REFRESH_SECS {
            return;
        }
        state.elapsed = 0.0;
    }
    show::<InspectorRoot>(world, current.is_some());
    let Some((id, entity)) = current else {
        return;
    };
    // The right column: under the bar, above the docks, at the spec's width.
    let layout = scale::layout(world);
    let top = crate::island_shell::content_top(world);
    let bottom = crate::island_shell::content_bottom(world);
    let width = column_width(Column::Inspector, layout.class);
    for mut node in world
        .query_filtered::<&mut Node, With<InspectorRoot>>()
        .iter_mut(world)
    {
        if node.top != px(top) {
            node.top = px(top);
        }
        if node.bottom != px(bottom) {
            node.bottom = px(bottom);
        }
        if node.width != px(width) {
            node.width = px(width);
        }
    }
    let Some(person) = world.get::<Villager>(entity).cloned() else {
        return;
    };
    set_text::<NameText>(world, &person.name);
    let from_roster = world.resource::<Inspector>().roster == Some(id);
    show::<BackButton>(world, from_roster);
    // "Farmer · Harbour": the trade and the village; the home is a link below.
    let trade = if person.role == Role::Visitor {
        "Your character".to_owned()
    } else {
        crate::colony_panel::trade_display(world, person.role)
    };
    // A job the player's order gave them says so (2.7).
    let ordered = world
        .resource::<Inspector>()
        .job_change
        .get(&id)
        .is_some_and(|(job, by_order)| *by_order && job == person.role.name());
    let subtitle = model::job_line(&trade, settlement_name(world, entity).as_deref(), ordered);
    set_text::<SubtitleText>(world, &subtitle);
    set_text::<ActivityText>(world, &model::capitalise(&activity_label(world, entity)));
    // The ring wears the trade's marker colour; the icon names the trade too.
    let tag = person.role.tag();
    if world.resource::<Inspector>().avatar != Some(tag) {
        let [r, g, b] = crate::island_atmosphere::role_color(person.role);
        for mut border in world
            .query_filtered::<&mut BorderColor, With<AvatarRing>>()
            .iter_mut(world)
        {
            *border = BorderColor::all(Color::srgb(r, g, b));
        }
        let slots: Vec<Entity> = world
            .query_filtered::<Entity, With<AvatarSlot>>()
            .iter(world)
            .collect();
        for slot in slots {
            world.entity_mut(slot).despawn_children();
            icon::icon(
                world,
                slot,
                Icon::of_trade(person.role.name()).unwrap_or(Icon::Resident),
                24.0,
                Tone::text(Surface::Paper),
            );
        }
        world.resource_mut::<Inspector>().avatar = Some(tag);
    }
    let status = status_of(world, entity);
    if world.resource::<Inspector>().status.as_ref() != Some(&status) {
        let slots: Vec<Entity> = world
            .query_filtered::<Entity, With<StatusSlot>>()
            .iter(world)
            .collect();
        for slot in slots {
            world.entity_mut(slot).despawn_children();
            kit::badge(world, slot, Surface::Paper, status.0, &status.1);
            world.entity_mut(slot).insert(
                Tooltip::new(status.1.clone(), model::status_sentence(status.0)).detail([
                    "A circle is fine, a triangle a need they will see to, a diamond a danger.",
                ]),
            );
        }
        world.resource_mut::<Inspector>().status = Some(status);
    }
    let (tab, following) = {
        let state = world.resource::<Inspector>();
        (state.tab, state.follow)
    };
    let controls: Vec<(Entity, Request)> = world
        .query::<(Entity, &InspectorAction)>()
        .iter(world)
        .map(|(entity, action)| (entity, action.0))
        .collect();
    for (control, request) in controls {
        let on = match request {
            Request::Tab(wanted) => wanted == tab,
            Request::Follow => following,
            Request::Close | Request::People => false,
        };
        kit::set_state(world, control, on, false);
    }
    let Some(body) = world
        .query_filtered::<Entity, With<InspectorBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let _ = id;
    let rows = entries(world);
    let old = std::mem::take(&mut world.resource_mut::<Inspector>().rows);
    let live = crate::colony_panel::reconcile_rows(world, body, old, rows);
    world.resource_mut::<Inspector>().rows = live;
}

/// While Follow is on, hand the camera the resident's drawn position every
/// frame. Panning by hand lets go; zoom and orbit stay the viewer's.
fn follow(world: &mut World) {
    let target = (|| {
        let (_, entity) = subject(world)?;
        if !world.resource::<Inspector>().follow {
            return None;
        }
        let manual = world.get_resource::<crate::camera::CameraManualIntent>();
        let panned = manual.is_some_and(|manual| {
            manual.active && manual.serial != world.resource::<Inspector>().manual_serial
        });
        if panned {
            return None;
        }
        world
            .get_resource::<VisualIndex>()
            .and_then(|index| index.map.get(&entity))
            .and_then(|visual| world.get::<GlobalTransform>(*visual))
            .map(GlobalTransform::translation)
            .or_else(|| world.get::<SimPos>(entity).map(|at| sim_to_world(*at)))
    })();
    let serial = world
        .get_resource::<crate::camera::CameraManualIntent>()
        .map_or(0, |manual| manual.serial);
    let mut state = world.resource_mut::<Inspector>();
    state.manual_serial = serial;
    if target.is_none() && state.follow {
        state.follow = false;
        state.elapsed = REFRESH_SECS;
    }
    world.resource_mut::<crate::island_camera::Follow>().0 = target;
}
