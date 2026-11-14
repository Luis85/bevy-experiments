//! Named conditions and scorers of the resident tree. Conditions take no
//! parameter (the tree runtime has none), so every threshold they read comes
//! from data: the `island_ai` policy, the schedule, or the resident's traits.
use super::super::behavior;
use super::super::{Villager, Wallet, ai, catalog, colony, economy as econ, policy, production};
use super::BlockKind;
use bevy_ecs::prelude::*;
use npc_framework::ai::{CondFn, ScoreFn};
use npc_framework::prelude::*;

pub struct ConditionInfo {
    pub name: &'static str,
    pub test: CondFn,
    /// When the condition holds, at least one of these `(action, parameter)`
    /// leaves can start. A guard that aborts lower-priority work needs all of
    /// them among the first leaves of its branch. Empty: no promise.
    pub covers: &'static [(&'static str, &'static str)],
    /// Registered by the engine, not by this module.
    pub external: bool,
    pub doc: &'static str,
}

const DRINKS: &[(&str, &str)] = &[
    ("life.drink", ""),
    ("life.buy", "water"),
    ("life.gather", "water"),
];
const MEALS: &[(&str, &str)] = &[
    ("life.eat", ""),
    ("life.buy", "food"),
    ("life.gather", "food"),
];

macro_rules! condition {
    ($name:literal, $test:expr, $covers:expr, $doc:literal) => {
        ConditionInfo {
            name: $name,
            test: $test,
            covers: $covers,
            external: false,
            doc: $doc,
        }
    };
}

/// Every condition a resident tree may name, sorted by name.
pub const CONDITIONS: &[ConditionInfo] = &[
    ConditionInfo {
        name: HAS_UNIT_ORDER,
        test: |_, _| false,
        covers: &[(EXECUTE_ORDER_ACTION, "")],
        external: true,
        doc: "A player order is queued for this resident.",
    },
    condition!(
        "life.at_home",
        at_home,
        &[],
        "Within reach of the own front door."
    ),
    condition!(
        "life.at_work",
        at_work,
        &[],
        "Within reach of the own workplace."
    ),
    condition!(
        "life.bedtime",
        bedtime,
        &[("life.go_to", "home")],
        "Not busy, inside a sleep block, and the hearth does not still need fuel fetched."
    ),
    condition!(
        "life.busy",
        behavior::busy,
        &[("life.continue", "")],
        "Physical work is in progress: gathering, crafting, hunting, building, an interaction \
         or a rest."
    ),
    condition!(
        "life.carrying_fuel",
        carrying_fuel,
        &[],
        "Carries at least one unit of the home's hearth fuel."
    ),
    condition!(
        "life.coat_season",
        coat_season,
        &[],
        "Living world: the resident neither wears nor carries a coat, and it is winter or \n         the last `tuning.coat_lead_days` of autumn, or a coat is on offer that it (or the \n         civic fund above its reserve) can pay: buying one is a priority purchase."
    ),
    condition!(
        "life.cold",
        cold,
        &[("life.warm_up", "")],
        "Not busy, no coat, cold in the open (night, winter or snow) and Warmth below \
         `recovery_threshold`: the home shelters, lit hearth or not."
    ),
    condition!(
        "life.company_nearby",
        company_nearby,
        &[("life.socialise", "")],
        "A free neighbour is within walking distance and the meeting cooldown has passed."
    ),
    condition!(
        "life.critical_hunger",
        critical_hunger,
        MEALS,
        "Hunger below `critical_need` and food is carried, affordable or forageable. \
         Interrupts everything except a rest that was not started for recovery."
    ),
    condition!(
        "life.critical_thirst",
        critical_thirst,
        DRINKS,
        "Thirst below `critical_need` and water is carried, affordable or gatherable."
    ),
    condition!(
        "life.diligent",
        |world, actor| super::traits(world, actor).diligence >= 60,
        &[],
        "Diligence trait is 60 or more."
    ),
    condition!(
        "life.elder",
        |world, actor| {
            super::super::demography::stage(world, actor)
                == super::super::demography::LifeStage::Elder
        },
        &[],
        "The resident is an elder: past `demography.elder_age_seasons`."
    ),
    condition!(
        "life.field_needs_harvest",
        |world, actor| field_task(world, actor, super::super::fields::FieldTask::Harvest),
        &[("life.harvest", "")],
        "A farmer whose settlement has a field with a ripe plot nobody else is working, and \
         room in the pack for the crop."
    ),
    condition!(
        "life.field_needs_sowing",
        |world, actor| field_task(world, actor, super::super::fields::FieldTask::Sow),
        &[("life.sow", "")],
        "A farmer whose settlement has a fallow plot that may be sown now: a sowing season, \
         time to ripen before winter, and seed in the field or the pack."
    ),
    condition!(
        "life.field_needs_tending",
        |world, actor| field_task(world, actor, super::super::fields::FieldTask::Tend),
        &[("life.tend", "")],
        "A farmer whose settlement has a growing plot that has not been tended for the crop's \
         interval."
    ),
    condition!(
        "life.food_available",
        |world, actor| production::food_choice(world, actor).is_some(),
        &[],
        "Food is carried or an affordable seller offers some."
    ),
    condition!(
        "life.fuel_patch_nearer",
        fuel_patch_nearer,
        &[],
        "A patch of the home's hearth fuel is nearer than the nearest seller of it."
    ),
    condition!(
        "life.freezing",
        freezing_first,
        &[("life.warm_up", "")],
        "Living world: Warmth below `tuning.freezing_warmth` without a coat, and neither          Thirst nor Hunger is below `critical_need` and lower still: getting warm comes          before every other need."
    ),
    condition!(
        "life.has_coins",
        |world, actor| world.get::<Wallet>(actor).is_some_and(|w| w.coins > 0),
        &[],
        "Owns at least one coin."
    ),
    condition!(
        "life.has_commission",
        |world, actor| colony::construction_intent(world, actor).is_some(),
        &[],
        "Holds a building commission."
    ),
    condition!(
        "life.has_food",
        has_food,
        &[("life.eat", "")],
        "Carries something edible."
    ),
    condition!(
        "life.has_partner",
        |world, actor| {
            world
                .get::<super::super::demography::Life>(actor)
                .is_some_and(|life| life.partner.is_some())
        },
        &[],
        "Has a living partner."
    ),
    condition!(
        "life.has_spare_coins",
        has_spare_coins,
        &[],
        "Owns more coins than the thrift reserve (`thrift` x `thrift_reserve_coins` / 100)."
    ),
    condition!(
        "life.has_surplus",
        |world, actor| super::super::civic::surplus_trip(world, actor).is_some(),
        &[("life.haul", "surplus")],
        "Holds surplus of an own product worth a trip and a store of the settlement has room."
    ),
    condition!(
        "life.has_water",
        |world, actor| econ::count(world, actor, super::super::roles::roles(world).drink) > 0,
        &[("life.drink", "")],
        "Carries water."
    ),
    condition!(
        "life.hungry",
        hungry,
        MEALS,
        "Not busy, Hunger below `hunger_threshold`, and food is carried, affordable or \
         forageable."
    ),
    condition!(
        "life.in_leisure_window",
        |world, actor| super::in_block(world, actor, BlockKind::Leisure),
        &[],
        "Inside a leisure block, shifted by chronotype and delayed by diligence."
    ),
    condition!(
        "life.in_meal_window",
        |world, actor| super::in_block(world, actor, BlockKind::Meal),
        &[],
        "Inside a meal block, shifted by chronotype."
    ),
    condition!(
        "life.in_sleep_window",
        |world, actor| super::in_block(world, actor, BlockKind::Sleep),
        &[],
        "Inside a sleep block, shifted by chronotype."
    ),
    condition!(
        "life.in_work_window",
        |world, actor| super::in_block(world, actor, BlockKind::Work),
        &[],
        "Inside a work block, shifted by chronotype."
    ),
    condition!(
        "life.needs_fuel",
        needs_fuel,
        &[],
        "Cold without a coat, the home hearth is out and too little fuel is carried."
    ),
    condition!(
        "life.needs_tool",
        needs_tool,
        &[],
        "The profession uses a tool and no usable one is carried."
    ),
    condition!(
        "life.night",
        |world, _| super::super::calendar(world).2,
        &[],
        "It is night by the world clock (not shifted by chronotype)."
    ),
    condition!(
        "life.raining",
        |world, _| super::super::weather::raining(world),
        &[],
        "Today's weather is rain."
    ),
    condition!(
        "life.site_needs_builder",
        |world, actor| super::super::civic::site_needs_builder(world, actor),
        &[("life.build", "")],
        "A building site of the settlement has all its materials and no builder."
    ),
    condition!(
        "life.site_needs_materials",
        |world, actor| super::super::civic::material_trip(world, actor).is_some()
            || super::super::live::homestead_trip(world, actor).is_some(),
        &[("life.haul", "materials")],
        "A building site of the settlement waits for materials this resident carries spare or \
         can buy at a store, and the civic fund can pay for them; or a played character's \
         homestead upgrade of the settlement has an order this resident can fill."
    ),
    condition!(
        "life.snowing",
        |world, _| super::super::weather::snowing(world),
        &[],
        "Today's weather is snow."
    ),
    condition!(
        "life.sociable",
        sociable,
        &[],
        "Sociability trait is at least `traits.sociable_min`."
    ),
    condition!(
        "life.stall_needs_stock",
        |world, actor| super::super::civic::restock_trip(world, actor).is_some(),
        &[("life.haul", "restock")],
        "A stall of the settlement is short of a good its storage holds, and this resident can \
         carry it over."
    ),
    condition!(
        "life.stock_available",
        |world, actor| !ai::procurement(world, actor).is_empty(),
        &[],
        "An affordable seller offers something the funded plan wants."
    ),
    condition!(
        "life.storm",
        |world, _| super::super::weather::storm(world),
        &[],
        "Today's weather is a storm: outdoor work stops."
    ),
    condition!(
        "life.thirsty",
        thirsty,
        DRINKS,
        "Not busy, Thirst below `thirst_threshold`, and water is carried, affordable or \
         gatherable."
    ),
    condition!(
        "life.thrifty",
        |world, actor| super::traits(world, actor).thrift >= 60,
        &[],
        "Thrift trait is 60 or more."
    ),
    condition!(
        "life.tired",
        tired,
        &[("life.sleep", "")],
        "Not busy and Energy below `recovery_threshold`, or inside a sleep block with Energy \
         below `night_recovery_threshold`."
    ),
    condition!(
        "life.wants_leisure",
        wants_leisure,
        &[],
        "Not busy, inside a leisure block, and not out in the cold (night or winter) without a          coat. Everybody has an evening; sociability decides whether it is spent in company."
    ),
    condition!(
        "life.gathering",
        gathering,
        &[],
        "The village is gathering and the resident is free to go: no critical need, not in \
         its sleep block, not out in the cold without a coat."
    ),
    condition!(
        "life.meal_due",
        meal_due,
        &[],
        "Not busy, inside a meal block on a day without a sit-down meal yet, carrying food, and the \
         village is not gathering."
    ),
    condition!(
        "life.family_at_home",
        family_at_home,
        &[],
        "Has a partner or a child, and a home, and the village is not gathering."
    ),
    condition!(
        "life.wants_meal",
        wants_meal,
        &[("life.eat", "")],
        "Not busy, inside a meal block, carrying food, and Hunger below \
         `schedule.meal_hunger_threshold`."
    ),
    condition!(
        "life.water_available",
        |world, actor| {
            econ::count(world, actor, super::super::roles::roles(world).drink) > 0
                || ai::market_supplier(world, actor, super::super::roles::roles(world).drink)
                    .is_some()
        },
        &[],
        "Water is carried or an affordable seller offers some."
    ),
    condition!(
        "life.winter",
        |world, _| super::super::calendar(world).1 == super::super::Season::Winter,
        &[],
        "The season is winter."
    ),
    condition!(
        "life.workplace_open",
        workplace_open,
        &[],
        "The workplace is reachable and the job is feasible (tool obtainable, a source exists)."
    ),
];

/// Scorers for utility selectors: `(name, function, description)`.
pub const SCORERS: &[(&str, ScoreFn, &str)] = &[
    (
        "life.urgency_hunger",
        |world, actor| 100_000 - econ::need(world, actor, "Hunger"),
        "100000 minus Hunger.",
    ),
    (
        "life.urgency_rest",
        |world, actor| 100_000 - econ::need(world, actor, "Energy"),
        "100000 minus Energy.",
    ),
    (
        "life.urgency_thirst",
        |world, actor| 100_000 - econ::need(world, actor, "Thirst"),
        "100000 minus Thirst.",
    ),
    (
        "life.urgency_warmth",
        |world, actor| 100_000 - econ::need(world, actor, "Warmth"),
        "100000 minus Warmth.",
    ),
];

pub(super) fn find(name: &str) -> Option<&'static ConditionInfo> {
    CONDITIONS.iter().find(|info| info.name == name)
}

pub(super) fn names() -> impl Iterator<Item = &'static str> {
    CONDITIONS.iter().map(|info| info.name)
}

fn critical(world: &World) -> i32 {
    policy::active(world).map_or(15_000, |policy| policy.critical_need)
}

fn critical_thirst(world: &World, actor: Entity) -> bool {
    behavior::can_interrupt(world, actor) && behavior::thirsty(world, actor, critical(world))
}

fn critical_hunger(world: &World, actor: Entity) -> bool {
    // A child of the home with nothing left to eat is as urgent as the
    // resident's own hunger: it wakes a parent.
    behavior::can_interrupt(world, actor)
        && (behavior::hungry(world, actor, critical(world)) || fetch_for_child(world, actor, 1))
}

/// A child of the home needs food, nobody at home carries any, and this
/// adult can buy some now.
fn fetch_for_child(world: &World, actor: Entity, below: i32) -> bool {
    super::super::demography::child_needs_food(world, actor, below)
        && [
            super::super::roles::roles(world).baked,
            super::super::roles::roles(world).cooked,
            super::super::roles::roles(world).foraged,
        ]
        .iter()
        .any(|kind| ai::market_supplier(world, actor, kind).is_some())
}

fn thirsty(world: &World, actor: Entity) -> bool {
    !behavior::busy(world, actor)
        && behavior::thirsty(
            world,
            actor,
            policy::active(world).map_or(55_000, |policy| policy.thirst_threshold),
        )
}

fn hungry(world: &World, actor: Entity) -> bool {
    // A hungry child at home with an empty larder sends an adult for food,
    // whatever the adult's own hunger and purse.
    !behavior::busy(world, actor)
        && (behavior::hungry(
            world,
            actor,
            policy::active(world).map_or(50_000, |policy| policy.hunger_threshold),
        ) || fetch_for_child(world, actor, 60_000))
}

fn has_food(world: &World, actor: Entity) -> bool {
    catalog::trade_kinds(world).iter().any(|kind| {
        catalog::food_restore_in(world, kind).is_some() && econ::count(world, actor, kind) > 0
    })
}

/// A resident with a family to spend the evening with: a partner or a child,
/// and a home. A gathering of the calendar wins: the family goes too.
fn family_at_home(world: &World, actor: Entity) -> bool {
    if super::super::live::attending(world, actor) {
        return false;
    }
    let Some(id) = world.get::<SimId>(actor).copied() else {
        return false;
    };
    let Some(home) = super::super::colony::home_of(world, id) else {
        return false;
    };
    world
        .get::<super::super::demography::Life>(actor)
        .is_some_and(|life| life.partner.is_some())
        || !super::super::demography::children_of(world, home).is_empty()
}

/// A gathering of the calendar is a village event: it ranks above tiredness
/// that is not critical, the meal and the chores (shopping, stocking up,
/// storing, restocking). Critical needs, the night and the cold still win.
fn gathering(world: &World, actor: Entity) -> bool {
    super::super::live::attending(world, actor)
        && !behavior::busy(world, actor)
        && !super::in_block(world, actor, BlockKind::Sleep)
        && !(super::super::living::cold(world) && !super::super::living::has_coat(world, actor))
}

/// A meal block on a day without a sit-down meal yet: everybody stops for
/// one meal a day, hungry or not (S3.3). A gathering of the calendar wins
/// (hunger is still answered by the hunger branch).
fn meal_due(world: &World, actor: Entity) -> bool {
    let Some(day) = super::meal_block(world, actor) else {
        return false;
    };
    if super::super::live::attending(world, actor) {
        return false;
    }
    super::super::living(world, actor)
        && !behavior::busy(world, actor)
        && has_food(world, actor)
        && world
            .get::<super::DayRhythm>(actor)
            .is_none_or(|rhythm| rhythm.last_meal != Some(day))
}

fn wants_meal(world: &World, actor: Entity) -> bool {
    let Some(spec) = super::spec(world) else {
        return false;
    };
    super::super::living(world, actor)
        && !behavior::busy(world, actor)
        && econ::need(world, actor, "Hunger") < spec.schedule.meal_hunger_threshold
        && super::in_block(world, actor, BlockKind::Meal)
        && has_food(world, actor)
}

fn tired(world: &World, actor: Entity) -> bool {
    if behavior::busy(world, actor) {
        return false;
    }
    let policy = policy::active(world);
    let energy = econ::need(world, actor, "Energy");
    energy < policy.map_or(30_000, |policy| policy.recovery_threshold)
        || (energy < policy.map_or(85_000, |policy| policy.night_recovery_threshold)
            && super::in_block(world, actor, BlockKind::Sleep))
}

fn cold(world: &World, actor: Entity) -> bool {
    // Indoors a home shelters even with a cold hearth, so a chilled resident
    // without a coat goes in to warm up whether or not there is fuel.
    !behavior::busy(world, actor)
        && (behavior::warming_need(world, actor).is_some() || behavior::freezing(world, actor))
}

/// The emergency of the cold: the lowest critical need, when it is Warmth.
fn freezing_first(world: &World, actor: Entity) -> bool {
    let Some(threshold) =
        super::super::frontier::economy_spec::tuning(world).map(|tuning| tuning.freezing_warmth)
    else {
        return false;
    };
    let warmth = econ::need(world, actor, "Warmth");
    if warmth >= threshold || super::super::living::has_coat(world, actor) {
        return false;
    }
    let critical = policy::active(world).map_or_else(
        || policy::IslandAiPolicy::default().critical_need,
        |policy| policy.critical_need,
    );
    !["Thirst", "Hunger"].iter().any(|need| {
        let value = econ::need(world, actor, need);
        value < critical && value < warmth
    })
}

fn coat_season(world: &World, actor: Entity) -> bool {
    super::super::living::coat_wanted_now(world, actor) && !behavior::busy(world, actor)
}

fn needs_fuel(world: &World, actor: Entity) -> bool {
    behavior::needs_hearth_fuel(world, actor)
}

fn bedtime(world: &World, actor: Entity) -> bool {
    !behavior::busy(world, actor)
        && super::in_block(world, actor, BlockKind::Sleep)
        && !needs_fuel(world, actor)
}

fn fuel_patch_nearer(world: &World, actor: Entity) -> bool {
    colony::home_heating(world, actor)
        .is_some_and(|home| ai::patch_nearer_than_seller(world, actor, home.fuel_kind))
}

fn carrying_fuel(world: &World, actor: Entity) -> bool {
    colony::home_heating(world, actor).is_some_and(|home| home.carried_fuel > 0)
}

fn has_spare_coins(world: &World, actor: Entity) -> bool {
    world
        .get::<Wallet>(actor)
        .is_some_and(|wallet| wallet.coins > super::thrift_reserve(world, actor))
}

fn sociable(world: &World, actor: Entity) -> bool {
    super::spec(world)
        .is_some_and(|spec| super::traits(world, actor).sociability >= spec.traits.sociable_min)
}

fn wants_leisure(world: &World, actor: Entity) -> bool {
    // A gathering of the calendar is everybody's free time.
    if !behavior::busy(world, actor) && super::super::live::attending(world, actor) {
        return !(super::super::living::cold(world)
            && !super::super::living::has_coat(world, actor));
    }
    // Everybody stops working in the leisure block (review fix npc-ai#3);
    // `life.sociable` decides whether the evening is spent in company.
    !behavior::busy(world, actor)
        && super::in_block(world, actor, BlockKind::Leisure)
        // Nobody strolls in the cold without a coat: that time goes to the
        // work that clothes the village.
        && !(super::super::living::cold(world) && !super::super::living::has_coat(world, actor))
}

fn company_nearby(world: &World, actor: Entity) -> bool {
    colony::social_target(world, actor).is_some()
}

fn needs_tool(world: &World, actor: Entity) -> bool {
    world.get::<Villager>(actor).is_some_and(|person| {
        catalog::role_tool_in(world, person.role)
            .is_some_and(|tool| super::super::living::usable_tool(world, actor, tool).is_none())
    })
}

fn workplace_open(world: &World, actor: Entity) -> bool {
    super::super::frontier::labor::work_feasible(world, actor)
}

fn field_task(world: &World, actor: Entity, task: super::super::fields::FieldTask) -> bool {
    !behavior::busy(world, actor) && super::super::fields::target(world, actor, task).is_some()
}

fn near(world: &World, actor: Entity, target: Option<SimPos>) -> bool {
    let (Some(here), Some(target)) = (world.get::<SimPos>(actor), target) else {
        return false;
    };
    here.dist2(target) <= 1_500_i64.pow(2)
}

fn at_home(world: &World, actor: Entity) -> bool {
    near(
        world,
        actor,
        world.get::<Villager>(actor).map(|person| person.home),
    )
}

fn at_work(world: &World, actor: Entity) -> bool {
    let site = world
        .get::<Villager>(actor)
        .and_then(|person| world.resource::<SimIndex>().get(person.workplace))
        .and_then(|site| world.get::<SimPos>(site).copied());
    near(world, actor, site)
}
