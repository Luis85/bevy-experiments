//! The resident's granular leaves. Each takes one bounded step through the
//! same validated actions a player uses, holds for one decision interval (or
//! until the physical work it started ends) and then succeeds, so the tree is
//! decided again from the top. A leaf that cannot begin fails at once and the
//! tree tries the next option in the same think.
use super::super::behavior::{self, LifeDecision, LifeGoal};
use super::super::{VillageAct, Villager, Wallet, ai, catalog, colony, economy as econ};
use super::super::{goap, market, production};
use super::{Context, record};
use bevy_ecs::prelude::*;
use npc_framework::ai::StartFn;
use npc_framework::prelude::*;
use serde_json::json;

/// What a leaf parameter may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    /// No parameter (`"None"`).
    None,
    /// One of the listed names (`{"Name": ..}`).
    Names(&'static [&'static str]),
    /// One of the listed names, or the key of a good.
    NamesOrGood(&'static [&'static str]),
    /// Nothing, or the key of a food good.
    OptionalFood,
    /// Nothing, or the key of a recipe.
    OptionalRecipe,
    /// A built-in or authored place name, or a position (`{"Pos": [x, z]}`).
    Place,
}

pub struct ActionInfo {
    pub name: &'static str,
    pub start: StartFn,
    pub param: Param,
    /// The leaf starts whenever the resident is alive, so a guard that aborts
    /// lower-priority work may lead to it without further promises.
    pub always_starts: bool,
    pub doc: &'static str,
}

const BUY_CLASSES: &[&str] = &[
    "water",
    "food",
    "fuel",
    "provisions",
    "clothes",
    "reserves",
    "inputs",
    "tool",
];
const GATHER_CLASSES: &[&str] = &["water", "food", "fuel", "outputs"];
const SELL_CLASSES: &[&str] = &["surplus"];
const HAUL_CLASSES: &[&str] = &["fuel", "materials", "surplus", "restock", "boat"];

/// A leaf start measured under the profiler row `start.<action>` (S4
/// measurement; hash-neutral, see `village::probe`).
macro_rules! timed {
    ($name:literal, $start:path) => {{
        fn timed_start(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
            let _probe = super::super::probe::start(world, concat!("start.", $name));
            $start(world, actor, param)
        }
        timed_start
    }};
}

/// Every action a resident tree may name, sorted by name.
pub const ACTIONS: &[ActionInfo] = &[
    ActionInfo {
        name: "life.build",
        start: timed!("life.build", build),
        param: Param::None,
        always_starts: false,
        doc: "Work on the cottage commission this resident was given, else on a building site \
              of the settlement whose materials are all delivered: walk there and work one \
              stage. Fails without a commission or a supplied site.",
    },
    ActionInfo {
        name: "life.buy",
        start: timed!("life.buy", buy),
        param: Param::NamesOrGood(BUY_CLASSES),
        always_starts: false,
        doc: "Walk to the best affordable seller and buy. `water` and `food` buy one unit to \
              consume; `fuel` buys hearth fuel when the home needs it; `provisions` (water and \
              food), `clothes` and `reserves` (both) stock what the funded plan wants; `inputs` \
              buys work materials; `tool` replaces the profession's tool; a good key buys one \
              more of that good. Fails when nobody affordable sells it.",
    },
    ActionInfo {
        name: "life.change_job",
        start: timed!("life.change_job", change_job),
        param: Param::None,
        always_starts: false,
        doc: "Take a requested or better-paid vacant workplace at an idle moment. Fails when \
              nothing changes, which is the usual case.",
    },
    ActionInfo {
        name: "life.collect_found",
        start: timed!("life.collect_found", collect_found),
        param: Param::None,
        always_starts: false,
        doc: "Pick up a dropped coin purse or diary within reach. Fails when none is near.",
    },
    ActionInfo {
        name: "life.continue",
        start: timed!("life.continue", carry_on),
        param: Param::None,
        always_starts: false,
        doc: "Keep doing the physical work already in progress (gathering, crafting, hunting, \
              building, resting). Fails when nothing is in progress.",
    },
    ActionInfo {
        name: "life.craft",
        start: timed!("life.craft", craft),
        param: Param::OptionalRecipe,
        always_starts: false,
        doc: "Walk to the workplace and start the best-scoring funded recipe whose inputs are \
              in the pack; with a recipe key, only that recipe. Fails when no recipe is ready.",
    },
    ActionInfo {
        name: "life.drink",
        start: timed!("life.drink", drink),
        param: Param::None,
        always_starts: false,
        doc: "Drink one carried water. Fails when none is carried.",
    },
    ActionInfo {
        name: "life.eat",
        start: timed!("life.eat", eat),
        param: Param::OptionalFood,
        always_starts: false,
        doc: "Eat one carried food: the preferred food, else the alternative, else berries; \
              with a food key, that food. Fails when none is carried.",
    },
    ActionInfo {
        name: "life.equip",
        start: timed!("life.equip", equip),
        param: Param::None,
        always_starts: false,
        doc: "Put on a carried coat. Fails when one is already worn or none is carried.",
    },
    ActionInfo {
        name: "life.gather",
        start: timed!("life.gather", gather),
        param: Param::NamesOrGood(GATHER_CLASSES),
        always_starts: false,
        doc: "Walk to the nearest patch and gather one unit. `water` and `food` (berries) are \
              subsistence; `fuel` is hearth fuel when the home needs it; `outputs` is the \
              profession's raw good with the most funded demand; a good key gathers that good. \
              Fails when no patch has any left or the pack is full.",
    },
    ActionInfo {
        name: "life.go_to",
        start: timed!("life.go_to", go_to),
        param: Param::Place,
        always_starts: false,
        doc: "Walk to `home`, `work`, `centre` (the settlement centre), a place from \
              `behavior.places`, or a position, and stay there. `home` always starts; any other \
              destination fails when the way is blocked.",
    },
    ActionInfo {
        name: "life.harvest",
        start: timed!("life.harvest", harvest),
        param: Param::None,
        always_starts: false,
        doc: "A farmer walks to a field of the settlement with a ripe plot and harvests it. \
              The crop goes into the pack; the plot's seed stays in the field. Fails when no \
              plot is ripe or the pack is full.",
    },
    ActionInfo {
        name: "life.haul",
        start: timed!("life.haul", haul),
        param: Param::Names(HAUL_CLASSES),
        always_starts: false,
        doc: "`fuel` carries hearth fuel home, where the household lights the hearth. \
              `materials` brings a building site of the settlement what it still needs: spare \
              goods from the pack, else goods bought at a store, sold to the site at the quote. \
              `surplus` lays the resident's own surplus in a store or stall of the settlement. \
              `restock` buys from storage what the stall is short of and lays it on the stall. \
              `boat` (living world) carries unsold stock the merchant boat buys to the trading \
              post while the boat is in or due that day. Fails when there is nothing to carry.",
    },
    ActionInfo {
        name: "life.help",
        start: timed!("life.help", help),
        param: Param::None,
        always_starts: false,
        doc: "Lend a played friend a hand in free time (living world, at most once a day each, \
              only with every need above `live.help_needs_above`): walk to the homestead and \
              bring one material its upgrade misses (from the pack, or bought at a store on \
              the way), work one stage at the door, or harvest a ripe bed of the friend's plot. \
              Fails when no friend's homestead needs a hand.",
    },
    ActionInfo {
        name: "life.hunt",
        start: timed!("life.hunt", hunt),
        param: Param::None,
        always_starts: false,
        doc: "A hunter with a spear and funded demand approaches the nearest prey and hunts. \
              Fails otherwise.",
    },
    ActionInfo {
        name: "life.sit_down",
        start: timed!("life.sit_down", sit_down),
        param: Param::None,
        always_starts: false,
        doc: "Sit down to the meal: eats the first food carried when hunger is below \
              `schedule.meal_hunger_threshold` (nobody eats past hunger), and started in a meal \
              block it marks the day's sit-down meal as had (one a day). Fails for a resident \
              who is not alive.",
    },
    ActionInfo {
        name: "life.idle",
        start: timed!("life.idle", idle),
        param: Param::None,
        always_starts: true,
        doc: "Stand still for one decision. Always starts; put it last.",
    },
    ActionInfo {
        name: "life.pursue_goal",
        start: timed!("life.pursue_goal", pursue_goal),
        param: Param::None,
        always_starts: false,
        doc: "GOAP: plan a sequence of buy, gather and craft steps towards the funded \
              production goal and take its first step. The plan is cached while it stays \
              valid. Fails when there is no funded goal or no plan.",
    },
    ActionInfo {
        name: "life.sell",
        start: timed!("life.sell", sell),
        param: Param::NamesOrGood(SELL_CLASSES),
        always_starts: false,
        doc: "Carry goods offered for sale to the nearest neighbour with funded demand and \
              sell there. `surplus` picks any offered good; a good key sells that good. Fails \
              when nobody wants it. Not used by the shipped tree.",
    },
    ActionInfo {
        name: "life.sleep",
        start: timed!("life.sleep", sleep),
        param: Param::None,
        always_starts: true,
        doc: "Walk home and rest there (300 ticks per rest). Always starts.",
    },
    ActionInfo {
        name: "life.socialise",
        start: timed!("life.socialise", socialise),
        param: Param::None,
        always_starts: false,
        doc: "Walk to a free neighbour nearby for a meeting. Fails when nobody is free.",
    },
    ActionInfo {
        name: "life.sow",
        start: timed!("life.sow", sow),
        param: Param::None,
        always_starts: false,
        doc: "A farmer walks to a field with a fallow plot and sows it, in a sowing season with \
              time left to ripen before winter. Seed comes from the field, else from the pack. \
              Fails when no plot can be sown.",
    },
    ActionInfo {
        name: "life.tend",
        start: timed!("life.tend", tend),
        param: Param::None,
        always_starts: false,
        doc: "A farmer walks to a field and tends a growing plot that has not been tended for \
              the crop's interval. A neglected plot grows slower. Fails when none needs it.",
    },
    ActionInfo {
        name: "life.warm_up",
        start: timed!("life.warm_up", warm_up),
        param: Param::None,
        always_starts: true,
        doc: "Walk home and stay by the hearth until warm (at most 1500 ticks). Always starts.",
    },
];

/// Actions registered by the engine that a resident tree may also name.
const EXTERNAL_ACTIONS: &[&str] = &[EXECUTE_ORDER_ACTION];

/// Whether the leaf `(action, parameter)` starts for every living resident.
pub(super) fn always_starts(action: &str, param: &str) -> bool {
    ACTIONS
        .iter()
        .any(|info| info.name == action && info.always_starts)
        || (action == "life.go_to" && param == "home")
}

pub(super) fn always_starting() -> impl Iterator<Item = &'static str> {
    ACTIONS
        .iter()
        .filter(|info| info.always_starts)
        .map(|info| info.name)
}

/// Checks an action name and its parameter against the world's data.
pub(super) fn check(action: &str, param: &ParamValue, context: &Context) -> Result<(), String> {
    if EXTERNAL_ACTIONS.contains(&action) {
        return Ok(());
    }
    let Some(info) = ACTIONS.iter().find(|info| info.name == action) else {
        return Err(format!(
            "unknown action `{action}`; known actions: {}",
            super::list(
                ACTIONS
                    .iter()
                    .map(|info| info.name)
                    .chain(EXTERNAL_ACTIONS.iter().copied())
            )
        ));
    };
    let name = match param {
        ParamValue::Name(name) => Some(name.as_str()),
        _ => None,
    };
    let good = |key: &str| context.economy.good(key).is_some();
    let ok = match info.param {
        Param::None => *param == ParamValue::None,
        Param::Names(names) => name.is_some_and(|name| names.contains(&name)),
        Param::NamesOrGood(names) => name.is_some_and(|name| names.contains(&name) || good(name)),
        Param::OptionalFood => {
            *param == ParamValue::None
                || name.is_some_and(|name| {
                    context
                        .economy
                        .good(name)
                        .is_some_and(|good| good.food_value.is_some())
                })
        }
        Param::OptionalRecipe => {
            *param == ParamValue::None
                || name.is_some_and(|name| context.economy.recipe(name).is_some())
        }
        Param::Place => match param {
            ParamValue::Pos(x, z) => context
                .bounds
                .contains(CellCoord::new(x.div_euclid(1000), z.div_euclid(1000))),
            ParamValue::Name(name) => {
                super::BUILTIN_PLACES.contains(&name.as_str())
                    || context.places.contains(name.as_str())
            }
            _ => false,
        },
    };
    if ok {
        return Ok(());
    }
    let expected = match info.param {
        Param::None => "no parameter (\"None\")".to_owned(),
        Param::Names(names) => format!("{{\"Name\": ..}} with one of: {}", names.join(", ")),
        Param::NamesOrGood(names) => format!(
            "{{\"Name\": ..}} with one of: {}, or the key of a good",
            names.join(", ")
        ),
        Param::OptionalFood => "\"None\" or {\"Name\": <food good key>}".to_owned(),
        Param::OptionalRecipe => "\"None\" or {\"Name\": <recipe key>}".to_owned(),
        Param::Place => format!(
            "{{\"Name\": ..}} with one of: {}, or {{\"Pos\": [x, z]}} inside the map",
            super::list(
                super::BUILTIN_PLACES
                    .iter()
                    .copied()
                    .chain(context.places.iter().copied())
            )
        ),
    };
    Err(format!(
        "action `{action}` does not accept parameter {}; expected {expected}",
        serde_json::to_string(param).unwrap_or_default()
    ))
}

/// Present-tense label of a running leaf.
pub(super) fn label(action: &str, param: &ParamValue) -> Option<&'static str> {
    let name = match param {
        ParamValue::Name(name) => name.as_str(),
        _ => "",
    };
    Some(match (action, name) {
        ("life.build", _) => "building",
        ("life.buy", "water") => "buying water",
        ("life.buy", "food") => "buying food",
        ("life.buy", "fuel") => "buying hearth fuel",
        ("life.buy", "provisions" | "reserves") => "stocking provisions",
        ("life.buy", "clothes") => "buying warm clothes",
        ("life.buy", "inputs") => "buying work materials",
        ("life.buy", "tool") => "buying a tool",
        ("life.buy", _) => "buying goods",
        ("life.change_job", _) => "taking a new job",
        ("life.collect_found", _) => "picking up lost property",
        ("life.continue", _) => "finishing the current activity",
        ("life.craft", _) => "crafting at the workplace",
        ("life.drink", _) => "drinking",
        ("life.eat", _) => "eating",
        ("life.equip", _) => "putting on a coat",
        ("life.gather", "water") => "fetching water",
        ("life.gather", "food") => "foraging",
        ("life.gather", "fuel") => "gathering hearth fuel",
        ("life.gather", _) => "gathering",
        ("life.go_to", "home") => "going home",
        ("life.go_to", "work") => "going to work",
        ("life.go_to", "centre") => "strolling to the square",
        ("life.go_to", _) => "taking a walk",
        ("life.harvest", _) => "harvesting the field",
        ("life.haul", "materials") => "carrying building materials",
        ("life.haul", "surplus") => "storing surplus goods",
        ("life.haul", "restock") => "restocking the stall",
        ("life.haul", "boat") => "carrying goods to the boat",
        ("life.haul", _) => "carrying fuel home",
        ("life.help", _) => "helping a friend",
        ("life.hunt", _) => "hunting",
        ("life.idle", _) => "idle",
        ("life.sit_down", _) => "sitting at the table",
        ("life.pursue_goal", _) => "working to a plan",
        ("life.sell", _) => "delivering goods",
        ("life.sleep", _) => "resting at home",
        ("life.socialise", _) => "meeting a neighbour",
        ("life.sow", _) => "sowing the field",
        ("life.tend", _) => "tending the field",
        ("life.warm_up", _) => "warming up at home",
        (name, _) if name == EXECUTE_ORDER_ACTION => "following orders",
        _ => return None,
    })
}

type Step = Result<(), &'static str>;

/// Leaves whose step keeps a resident outdoors for a while: in the cold
/// without a coat, below `tuning.exposure_warmth`, none of them starts.
const OUTDOOR_ERRANDS: &[&str] = &[
    "life.build",
    "life.craft",
    "life.gather",
    "life.go_to",
    "life.harvest",
    "life.haul",
    "life.help",
    "life.hunt",
    "life.pursue_goal",
    "life.sell",
    "life.socialise",
    "life.sow",
    "life.tend",
];

/// Shared start: run one step, then hold the leaf for a decision interval.
fn begin(
    world: &mut World,
    actor: Entity,
    action: &'static str,
    param: &ParamValue,
    goal: LifeGoal,
    step: impl FnOnce(&mut World, Entity) -> Step,
) -> Status {
    if !super::super::living(world, actor)
        || world
            .get::<Villager>(actor)
            .is_none_or(|person| person.role == super::super::Role::Visitor)
    {
        return Status::Failure;
    }
    // Out in the cold without a coat: no errand that keeps the resident
    // outdoors (living world, `tuning.exposure_warmth`). Buying food, water,
    // fuel or a coat, eating, resting and going home are not errands.
    if OUTDOOR_ERRANDS.contains(&action)
        && !(action == "life.go_to" && name_of(param) == "home")
        && super::super::living::exposed(world, actor)
    {
        record::tried(
            world,
            actor,
            action,
            param,
            "too cold to go out without a coat",
        );
        return Status::Failure;
    }
    // Anything but rest and the leaf that carries a rest on wakes a sleeper.
    if !matches!(action, "life.sleep" | "life.warm_up" | "life.continue") {
        super::super::indoors::wake(world, actor);
    }
    record::clear_travel_failure(world);
    let outcome = step(world, actor).and_then(|()| {
        if record::take_travel_failure(world) {
            Err("the route was abandoned after repeated blockage")
        } else {
            Ok(())
        }
    });
    if let Err(reason) = outcome {
        record::tried(world, actor, action, param, reason);
        return Status::Failure;
    }
    let reconsider_at = world.resource::<SimClock>().tick
        + super::super::policy::active(world).map_or(20, |policy| policy.decision_ticks);
    world.entity_mut(actor).insert(LifeDecision {
        goal,
        reconsider_at,
        pending: false,
    });
    if let Some(mut person) = world.get_mut::<Villager>(actor) {
        person.next_decision = reconsider_at;
    }
    record::started(world, actor);
    Status::Running
}

fn id_of(world: &World, actor: Entity) -> SimId {
    *world.get::<SimId>(actor).expect("sim actor")
}

fn name_of(param: &ParamValue) -> &str {
    match param {
        ParamValue::Name(name) => name,
        _ => "",
    }
}

/// The foods a resident reaches for, in order.
fn foods(world: &World, actor: Entity) -> [&'static str; 3] {
    let preferred = production::food_preference(world, actor);
    let alternate = if preferred == super::super::roles::roles(world).baked {
        super::super::roles::roles(world).cooked
    } else {
        super::super::roles::roles(world).baked
    };
    [
        preferred,
        alternate,
        super::super::roles::roles(world).foraged,
    ]
}

/// Hearth fuel this resident still has to bring home, if any.
fn fuel_needed(world: &World, actor: Entity) -> Option<(&'static str, u32)> {
    behavior::warming_need(world, actor)
        .filter(|home| !home.heated && home.carried_fuel < home.fuel_required)
        .map(|home| (home.fuel_kind, home.fuel_required))
}

/// Walk to the best affordable seller and buy one unit to consume.
fn buy_one(world: &mut World, actor: Entity, kind: &str) -> Step {
    let Some(seller) = ai::market_supplier(world, actor, kind) else {
        return Err("no affordable seller offers it");
    };
    // A thirsty resident does not walk past a well to a far seller (closing
    // fix: two residents walked over 100 m to buy water and fell to Thirst
    // 6,373): when a patch of the good is nearer than the seller, the
    // selector's next leaf fetches it there.
    if let (Some(here), Some(at)) = (
        world.get::<SimPos>(actor).copied(),
        world.get::<SimPos>(seller).copied(),
    ) && let Some((_, patch)) = ai::gather_source(world, actor, kind)
        && let Some(well) = world.get::<SimPos>(patch).copied()
        && here.dist2(well) < here.dist2(at)
    {
        return Err("a patch of it is nearer than any seller");
    }
    if !ai::approach_work(world, actor, seller) {
        return Ok(());
    }
    let mut command = VillageAct::new(id_of(world, actor), "buy");
    command.target = world.get::<SimId>(seller).copied();
    command.kind = kind.into();
    command.limit_price = market::asking_price(world, seller, kind).map(|price| {
        production::purchase_bid_limit(world, actor, kind).map_or(price, |limit| limit.min(price))
    });
    command
        .perform(world)
        .map_err(|_| "the seller refused the trade")
}

fn drink(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.drink",
        param,
        LifeGoal::Thirst,
        |world, actor| {
            VillageAct::new(id_of(world, actor), "drink")
                .perform(world)
                .map_err(|_| "no water carried")
        },
    )
}

fn eat(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.eat",
        param,
        LifeGoal::Hunger,
        |world, actor| {
            let named = name_of(param);
            let kind = if named.is_empty() {
                foods(world, actor)
                    .into_iter()
                    .find(|kind| econ::count(world, actor, kind) > 0)
                    .ok_or("no food carried")?
            } else {
                named
            };
            let mut command = VillageAct::new(id_of(world, actor), "eat");
            command.kind = kind.into();
            command.perform(world).map_err(|_| "no food carried")
        },
    )
}

fn buy(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    let class = name_of(param);
    let goal = match class {
        "water" => LifeGoal::Thirst,
        "food" => LifeGoal::Hunger,
        _ => LifeGoal::Work,
    };
    begin(world, actor, "life.buy", param, goal, |world, actor| {
        let listed = |world: &mut World, wanted: fn(u8) -> bool| -> Step {
            let procure = record::procurement(world, actor);
            if ai::buy_listed(world, actor, &procure, wanted) {
                Ok(())
            } else {
                Err("nothing wanted is on offer at an affordable price")
            }
        };
        match class {
            "water" => buy_one(world, actor, super::super::roles::roles(world).drink),
            "food" => {
                let kind = foods(world, actor)
                    .into_iter()
                    .find(|kind| ai::market_supplier(world, actor, kind).is_some())
                    .ok_or("no affordable seller offers food")?;
                buy_one(world, actor, kind)
            }
            "fuel" => {
                let (kind, wanted) =
                    fuel_needed(world, actor).ok_or("the hearth does not need fuel")?;
                if ai::buy_market_input(world, actor, kind, wanted) {
                    Ok(())
                } else {
                    Err("no affordable seller offers hearth fuel")
                }
            }
            "provisions" => listed(world, |priority| priority < 2),
            "clothes" => {
                // The grant is paid only for a coat that is then bought: with
                // no coat on offer the coins go back to the fund.
                let grant = coat_grant(world, actor);
                let bought = listed(world, |priority| priority == 2).or_else(|reason| {
                    // The plan listed no coat for a resident who had no
                    // coins at its last refresh: in the coat season it buys
                    // one directly.
                    if super::super::living::coat_wanted_now(world, actor)
                        && ai::buy_market_input(
                            world,
                            actor,
                            super::super::roles::roles(world).garment,
                            1,
                        )
                    {
                        Ok(())
                    } else {
                        Err(reason)
                    }
                });
                if let Some((coins, price)) = grant {
                    if bought.is_ok() {
                        let id = id_of(world, actor);
                        record_effect(
                            world,
                            Some(id),
                            "VillageCoatGrant",
                            &json!({"resident":id,"coins":coins,"price":price}),
                        );
                    } else {
                        world.get_mut::<Wallet>(actor).expect("wallet").coins -= coins;
                        world.resource_mut::<super::super::VillageState>().fund += coins;
                    }
                }
                bought
            }
            "reserves" => listed(world, |priority| priority < 3),
            "inputs" => listed(world, |priority| priority == 3),
            "tool" => {
                let role = world.get::<Villager>(actor).expect("villager").role;
                let tool =
                    catalog::role_tool_in(world, role).ok_or("this profession uses no tool")?;
                if super::super::living::usable_tool(world, actor, tool).is_some() {
                    return Err("a usable tool is already carried");
                }
                if ai::buy_market_input(world, actor, tool, 1) {
                    Ok(())
                } else {
                    Err("no affordable seller offers the tool")
                }
            }
            kind => {
                let wanted = econ::count(world, actor, kind).saturating_add(1);
                if ai::buy_market_input(world, actor, kind, wanted) {
                    Ok(())
                } else {
                    Err("no affordable seller offers it")
                }
            }
        }
    })
}

fn sell(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.sell",
        param,
        LifeGoal::Work,
        |world, actor| {
            let only = name_of(param);
            let here = *world.get::<SimPos>(actor).ok_or("no position")?;
            let kinds: Vec<&'static str> = catalog::trade_kinds(world)
                .iter()
                .copied()
                .filter(|kind| {
                    (only == "surplus" || *kind == only)
                        && market::available_to_sell(world, actor, kind) > 0
                })
                .collect();
            if kinds.is_empty() {
                return Err("nothing is offered for sale from the pack");
            }
            let mut best: Option<(i64, SimId, Entity, &'static str)> = None;
            for kind in kinds {
                let Some(price) = market::asking_price(world, actor, kind) else {
                    continue;
                };
                for (id, buyer) in &world.resource::<SimIndex>().map {
                    let Some(person) = world.get::<Villager>(*buyer) else {
                        continue;
                    };
                    if *buyer == actor
                        || !super::super::living(world, *buyer)
                        || person.role == super::super::Role::Visitor
                        || !catalog::wants_for(world, person.role, kind)
                        || production::purchase_target(world, *buyer, kind)
                            <= econ::count(world, *buyer, kind)
                        || world.get::<Wallet>(*buyer).is_none_or(|w| w.coins < price)
                        || production::purchase_bid_limit(world, *buyer, kind)
                            .is_some_and(|limit| price > limit)
                    {
                        continue;
                    }
                    let Some(pos) = world.get::<SimPos>(*buyer).copied() else {
                        continue;
                    };
                    if super::super::routing::avoiding(world, actor, pos, Some(*id)) {
                        continue;
                    }
                    let Some(route) = super::super::frontier::routes::estimate(world, here, pos)
                    else {
                        continue;
                    };
                    let candidate = (i64::from(route.distance_mm), *id, *buyer, kind);
                    if best.is_none_or(|(distance, best_id, ..)| {
                        (candidate.0, candidate.1) < (distance, best_id)
                    }) {
                        best = Some(candidate);
                    }
                }
            }
            let (_, buyer_id, buyer, kind) = best.ok_or("no neighbour has funded demand for it")?;
            production::intent(world, actor, "sell", kind, "", Some(buyer_id));
            if !ai::approach_work(world, actor, buyer) {
                return Ok(());
            }
            let quantity = market::available_to_sell(world, actor, kind)
                .min(
                    production::purchase_target(world, buyer, kind)
                        .saturating_sub(econ::count(world, buyer, kind)),
                )
                .min(8);
            let mut command = VillageAct::new(id_of(world, actor), "sell");
            command.target = Some(buyer_id);
            command.kind = kind.into();
            for count in (1..=quantity).rev() {
                command.quantity = count;
                if command.perform(world).is_ok() {
                    return Ok(());
                }
            }
            Err("the buyer refused the trade")
        },
    )
}

fn gather(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    let class = name_of(param);
    let goal = match class {
        "water" => LifeGoal::Thirst,
        "food" => LifeGoal::Hunger,
        _ => LifeGoal::Work,
    };
    begin(world, actor, "life.gather", param, goal, |world, actor| {
        let done = match class {
            "water" => ai::gather_job(world, actor, super::super::roles::roles(world).drink),
            "food" => ai::gather_job(world, actor, super::super::roles::roles(world).foraged),
            "fuel" => {
                let (kind, _) = fuel_needed(world, actor).ok_or("the hearth does not need fuel")?;
                ai::gather_job(world, actor, kind)
            }
            "outputs" => ai::gather_outputs(world, actor),
            kind => ai::gather_job(world, actor, kind),
        };
        if done {
            Ok(())
        } else if class == "outputs" {
            Err("no funded demand for a raw good, or no patch has any left")
        } else {
            Err("no reachable patch has any left, or the pack is full")
        }
    })
}

fn craft(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.craft",
        param,
        LifeGoal::Work,
        |world, actor| {
            let only = name_of(param);
            let mut seen = Vec::new();
            let started =
                ai::craft_best(world, actor, (!only.is_empty()).then_some(only), &mut seen);
            if !started {
                return Err("no funded recipe has its inputs in the pack");
            }
            record::detail(world, actor, |_| {
                json!({"candidates": seen.iter().take(3).map(|(score, recipe)| {
                json!({"recipe": recipe, "score": score})
            }).collect::<Vec<_>>()})
            });
            Ok(())
        },
    )
}

fn hunt(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.hunt",
        param,
        LifeGoal::Work,
        |world, actor| {
            if ai::hunt(world, actor) {
                Ok(())
            } else {
                Err("not a hunter with a spear, funded demand and prey in reach")
            }
        },
    )
}

fn equip(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.equip",
        param,
        LifeGoal::Work,
        |world, actor| {
            if ai::equip_coat(world, actor) {
                Ok(())
            } else {
                Err("a coat is already worn or none is carried")
            }
        },
    )
}

fn build(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.build",
        param,
        LifeGoal::Work,
        |world, actor| {
            if ai::construction_step(world, actor) {
                Ok(())
            } else {
                super::super::civic::build_step(world, actor)
                    .map_err(|_| "no building commission and no supplied building site")
            }
        },
    )
}

fn pursue_goal(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.pursue_goal",
        param,
        LifeGoal::Work,
        |world, actor| {
            if !ai::planned_work(world, actor) {
                return Err("no funded production goal has an executable plan");
            }
            record::detail(world, actor, |world| {
                world.get::<goap::PlanMemo>(actor).map_or_else(
                    || json!({}),
                    |memo| {
                        json!({"plan": {
                            "goal": memo.plan.goal,
                            "steps": memo.plan.steps.len(),
                            "first": memo.plan.steps.first().map(goap::PlanStep::label),
                            "cost": memo.plan.cost,
                            "expanded": memo.plan.expanded,
                        }})
                    },
                )
            });
            Ok(())
        },
    )
}

fn change_job(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.change_job",
        param,
        LifeGoal::Work,
        |world, actor| {
            if super::super::frontier::labor::consider_assignment(world, actor) {
                Ok(())
            } else {
                Err("no requested or better-paid vacancy")
            }
        },
    )
}

fn collect_found(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.collect_found",
        param,
        LifeGoal::Work,
        |world, actor| {
            if ai::recover_found(world, actor) {
                Ok(())
            } else {
                Err("nothing lost within reach")
            }
        },
    )
}

fn carry_on(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.continue",
        param,
        LifeGoal::Busy,
        |world, actor| {
            if behavior::busy(world, actor) {
                Ok(())
            } else {
                Err("nothing is in progress")
            }
        },
    )
}

/// Walk home and rest there. In the sleep block that is the night's sleep,
/// to the end of the block. By day it is a short rest; a cold resident
/// without a coat stays by the hearth until warm.
/// The keeper of a staffed inn with a free bed at most half as far as home,
/// when home is at least `tuning.inn_detour_m` away and the resident can pay
/// for a bed and keep its destitution floor (living world).
/// The inn a tired resident far from home sleeps at: its keeper (who is paid)
/// and its entrance (where the bed is rented).
fn inn_on_the_way(world: &World, actor: Entity) -> Option<(SimId, SimPos)> {
    use super::super::{indoors, living};
    let tuning = super::super::frontier::economy_spec::tuning(world)?;
    if tuning.inn_detour_m == 0 {
        return None;
    }
    let here = *world.get::<SimPos>(actor)?;
    let home = world.get::<Villager>(actor)?.home;
    let far = here.dist2(home);
    if far < i64::from(tuning.inn_detour_m).pow(2) * 1_000_000 {
        return None;
    }
    let coins = world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins);
    if coins < catalog::bed_price(world).saturating_add(tuning.destitution_floor) {
        return None;
    }
    let mut keepers: Vec<(i64, SimId, SimPos)> = indoors::inns(world)
        .into_iter()
        .filter(|inn| indoors::free_guest_bed(world, inn.inn).is_some())
        .filter_map(|inn| {
            let keeper = inn.keeper?;
            let body = world.resource::<SimIndex>().get(keeper)?;
            if !living(world, body) {
                return None;
            }
            let distance = inn.entrance.dist2(here);
            (distance * 4 <= far).then_some((distance, keeper, inn.entrance))
        })
        .collect();
    keepers.sort_unstable_by_key(|(distance, keeper, _)| (*distance, *keeper));
    keepers
        .first()
        .map(|(_, keeper, entrance)| (*keeper, *entrance))
}

fn rest_at_home(world: &mut World, actor: Entity) -> Step {
    let home = world.get::<Villager>(actor).expect("villager").home;
    if super::super::indoors::asleep(world, actor) {
        return Ok(());
    }
    // Far from home, a bed at a nearer inn.
    if let Some((keeper, entrance)) = inn_on_the_way(world, actor) {
        if ai::travel(world, actor, entrance) {
            let mut rent = VillageAct::new(id_of(world, actor), "rest");
            rent.target = Some(keeper);
            if rent.perform(world).is_ok() {
                return Ok(());
            }
        } else {
            record::clear_travel_failure(world);
            return Ok(());
        }
    }
    if ai::travel(world, actor, home) {
        let warming = behavior::warming_rest_ticks(world, actor);
        if VillageAct::new(id_of(world, actor), "rest_home")
            .perform(world)
            .is_ok()
            && !super::super::indoors::fall_asleep(world, actor)
        {
            let nap = super::super::frontier::economy_spec::tuning(world)
                .map_or(super::super::HOME_REST_RATE, |tuning| {
                    tuning.nap_energy_per_tick as i32
                });
            ai::start_rest(
                world,
                actor,
                warming.unwrap_or(super::super::commands::HOME_REST_TICKS),
                nap,
            );
        }
    }
    // A blocked way home is retried at the next decision, never a failure.
    record::clear_travel_failure(world);
    Ok(())
}

fn sleep(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.sleep",
        param,
        LifeGoal::Recovery,
        rest_at_home,
    )
}

fn warm_up(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.warm_up",
        param,
        LifeGoal::Recovery,
        rest_at_home,
    )
}

/// Whether a resident stands in the square of its settlement.
fn at_square(world: &World, actor: Entity) -> bool {
    let centre = super::super::frontier::settlements::member(world, actor)
        .and_then(|settlement| super::super::frontier::settlements::definition(world, settlement))
        .map(|definition| definition.center);
    match (centre, world.get::<SimPos>(actor)) {
        (Some(centre), Some(here)) => here.dist2(centre) <= 10_000_i64.pow(2),
        _ => true,
    }
}

fn socialise(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.socialise",
        param,
        LifeGoal::Leisure,
        |world, actor| {
            production::clear_intent(world, actor);
            // During a gathering everybody walks to the square instead.
            if super::super::live::attending(world, actor) && !at_square(world, actor) {
                return Err("the village is gathering in the square");
            }
            if ai::socialise(world, actor) {
                Ok(())
            } else {
                Err("nobody nearby is free to meet")
            }
        },
    )
}

fn field_work(
    world: &mut World,
    actor: Entity,
    action: &'static str,
    param: &ParamValue,
    task: super::super::fields::FieldTask,
) -> Status {
    begin(
        world,
        actor,
        action,
        param,
        LifeGoal::Work,
        move |world, actor| super::super::fields::step(world, actor, task),
    )
}

fn harvest(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    field_work(
        world,
        actor,
        "life.harvest",
        param,
        super::super::fields::FieldTask::Harvest,
    )
}

fn sow(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    field_work(
        world,
        actor,
        "life.sow",
        param,
        super::super::fields::FieldTask::Sow,
    )
}

fn tend(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    field_work(
        world,
        actor,
        "life.tend",
        param,
        super::super::fields::FieldTask::Tend,
    )
}

fn haul(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.haul",
        param,
        LifeGoal::Work,
        |world, actor| {
            match name_of(param) {
                "materials" => return super::super::civic::haul_materials(world, actor),
                "surplus" => return super::super::civic::haul_surplus(world, actor),
                "restock" => return super::super::civic::haul_restock(world, actor),
                "boat" => return super::super::civic::haul_boat(world, actor),
                _ => {}
            }
            let hearth = colony::home_heating(world, actor).ok_or("no home hearth")?;
            if hearth.carried_fuel == 0 {
                return Err("no hearth fuel is carried");
            }
            if hearth.heated || !hearth.cold {
                return Err("the hearth does not need fuel now");
            }
            let home = world.get::<Villager>(actor).expect("villager").home;
            ai::travel(world, actor, home);
            Ok(())
        },
    )
}

fn help(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.help",
        param,
        LifeGoal::Leisure,
        |world, actor| {
            if super::super::live::attending(world, actor) {
                return Err("the village is gathering in the square");
            }
            super::super::live::help_step(world, actor)
        },
    )
}

/// A resident without a coat in the coat season who cannot pay for one is
/// given the price by the civic fund when the fund may spend it. Returns
/// `(coins granted, price)`; the caller journals it once the purchase
/// starts, or hands the coins back.
fn coat_grant(world: &mut World, actor: Entity) -> Option<(u32, u32)> {
    use super::super::{VillageState, living, market};
    if !living::coat_wanted_now(world, actor) {
        return None;
    }
    let price = market::unit_price(world, super::super::roles::roles(world).garment)?;
    let coins = world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins);
    // In the coat season it may draw on the reserve; otherwise only on the
    // balance above it (`living::coat_grant_room`).
    if coins >= price || living::coat_grant_room(world) < price - coins {
        return None;
    }
    let grant = price - coins;
    world.resource_mut::<VillageState>().fund -= grant;
    world.get_mut::<Wallet>(actor).expect("wallet").coins += grant;
    Some((grant, price))
}

fn go_to(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    let home = name_of(param) == "home";
    let goal = if home {
        LifeGoal::Home
    } else {
        LifeGoal::Leisure
    };
    begin(world, actor, "life.go_to", param, goal, |world, actor| {
        let Some(target) = super::place(world, actor, param) else {
            return if home {
                Ok(())
            } else {
                Err("this resident has no such place")
            };
        };
        if !home {
            production::clear_intent(world, actor);
        }
        let arrived = ai::travel(world, actor, target);
        if home {
            record::clear_travel_failure(world);
        } else if !arrived && world.get::<MoveIntent>(actor).is_none() {
            return Err("the way there is blocked");
        }
        Ok(())
    })
}

fn sit_down(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.sit_down",
        param,
        LifeGoal::Leisure,
        |world, actor| {
            let meal = super::meal_block(world, actor);
            world
                .entity_mut(actor)
                .remove::<npc_framework::prelude::MoveIntent>();
            let threshold =
                super::spec(world).map_or(60_000, |spec| spec.schedule.meal_hunger_threshold);
            if econ::need(world, actor, "Hunger") < threshold
                && let Some(kind) = foods(world, actor)
                    .into_iter()
                    .find(|kind| econ::count(world, actor, kind) > 0)
            {
                let mut command = VillageAct::new(id_of(world, actor), "eat");
                command.kind = kind.into();
                // A meal that cannot be eaten is still a meal sat down to.
                let _ = command.perform(world);
            }
            if let Some(meal) = meal {
                world.entity_mut(actor).insert(super::DayRhythm {
                    last_meal: Some(meal),
                });
            }
            Ok(())
        },
    )
}

fn idle(world: &mut World, actor: Entity, param: &ParamValue) -> Status {
    begin(
        world,
        actor,
        "life.idle",
        param,
        LifeGoal::Leisure,
        |world, actor| {
            production::clear_intent(world, actor);
            // Nothing to do in the cold without a coat: wait at home.
            if super::super::living::exposed(world, actor) {
                let home = world.get::<Villager>(actor).expect("villager").home;
                ai::travel(world, actor, home);
                return Ok(());
            }
            world.entity_mut(actor).remove::<MoveIntent>();
            Ok(())
        },
    )
}
