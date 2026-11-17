//! Live mode (regional profile only): what makes one played life worth
//! living among the residents.
//!
//! - **Sleep to morning.** The played character goes to bed and sleeps to the
//!   next morning. The simulation skips nothing: a host may run it faster
//!   while [`asleep_until`] names a tick. At every morning each played
//!   character gets a [`DaySummary`] of the day before.
//! - **Repeat orders.** The same gathering again and again until the count
//!   is used up or it cannot be done.
//! - **Bonds.** A score between a played character and each resident, raised
//!   by talk, trade and gifts and lowered by neglect, in tiers from data with
//!   real effects: a discount, a gift back, an invitation.
//! - **Homestead tiers and goals.** Upgrades paid in goods and work, and a
//!   chain of goals from data that unlocks them.
//! - **Gatherings and mourning.** Calendar feasts the residents walk to, and
//!   grief when somebody is laid to rest.
//!
//! Everything here is hashed state changed by recorded commands
//! ([`LiveAct`]) or by the tick; the data is `economy.live`.
use super::colony::{self, Household, SocialLife};
use super::demography::Departed;
use super::frontier::economy_spec;
use super::frontier::life_spec::{GoalSpec, HomesteadTier, LiveSpec};
use super::{Gathering, Role, Season, Villager, Wallet, catalog, economy as econ, living};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::Crafting;
use npc_framework::interaction::Item;
use npc_framework::kernel::rng::sim_rand;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Goods by key.
pub type Goods = BTreeMap<String, u32>;

/// `sim_rand` purposes of this module.
const TASTE_PURPOSE: u32 = 0x4c49_5601;
const NEEDS: [&str; 4] = ["Hunger", "Thirst", "Energy", "Warmth"];
/// A resident this near its settlement's square attends a gathering.
const GATHERING_REACH_MM: i64 = 12_000;
/// A resident greets the played character from at most this far (the
/// tree's `life.player_within` guard narrows it).
pub const GREETING_REACH_MM: i64 = 12_000;
/// Work on the homestead counts while its owner is this near the door.
const HOMESTEAD_REACH_MM: i64 = 4_000;

/// The bond of a played character with one resident.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Bond {
    /// 0 to 100000.
    pub score: i32,
    /// Calendar day of the last talk, trade or gift.
    pub last_contact_day: u64,
    pub talked_day: Option<u64>,
    pub traded_day: Option<u64>,
    pub gifted_day: Option<u64>,
    pub gift_back_day: Option<u64>,
    pub greeted_day: Option<u64>,
    /// Invited to the next gathering.
    pub invited: bool,
    /// Calendar day the resident last helped on the homestead.
    #[serde(default)]
    pub helped_day: Option<u64>,
}

/// What one played character did in one day.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct DaySummary {
    pub day: u64,
    pub earned: u32,
    pub spent: u32,
    /// Units that came into the pack while gathering, crafting, or otherwise
    /// (bought or given).
    pub gathered: BTreeMap<String, u32>,
    pub made: BTreeMap<String, u32>,
    pub received: BTreeMap<String, u32>,
    /// Residents talked to, traded with or given a gift.
    pub met: Vec<SimId>,
    /// The lowest each need fell to.
    pub low: BTreeMap<String, i32>,
    /// Slept in a bed to the morning that ended this day.
    pub slept: bool,
    /// Units bought and sold by trade this day, by good, and their coins.
    #[serde(default)]
    pub bought: BTreeMap<String, u32>,
    #[serde(default)]
    pub sold: BTreeMap<String, u32>,
    #[serde(default)]
    pub bought_coins: u32,
    #[serde(default)]
    pub sold_coins: u32,
}

/// The running account of a day.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct DayLedger {
    pub summary: DaySummary,
    pub coins: u32,
    pub pack: BTreeMap<String, u32>,
    /// Gathering or crafting on the tick before: the goods of a finished
    /// job arrive on the tick after the work ends.
    #[serde(default)]
    pub gathering: bool,
    #[serde(default)]
    pub crafting: bool,
}

/// The same work again until `remaining` is zero: a gathering at a resource
/// patch, a recipe at a workshop (`recipe`), or a garden verb on a homestead
/// plot (`task`: `plant`, `water_crop` or `harvest`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct RepeatOrder {
    pub target: SimId,
    pub remaining: u32,
    pub done: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recipe: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub task: String,
}

/// A homestead upgrade under way: a private building site at the cottage,
/// paid from the owner's escrow (the civic fund pays for village sites).
/// Materials are delivered from the owner's pack, carried there by a hauling
/// resident for an order paid from escrow, or brought by a friend; work is
/// done at the door by the owner, a friend or a hired builder. Nothing
/// happens at a distance: see [`jobs`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Upgrade {
    pub key: String,
    /// Work ticks still to do.
    pub remaining_ticks: u64,
    /// Materials the upgrade needs, and those delivered so far.
    #[serde(default)]
    pub cost: Goods,
    #[serde(default)]
    pub delivered: Goods,
    /// Units ordered from residents and not yet brought.
    #[serde(default)]
    pub ordered: Goods,
    /// Coins held from the owner's wallet for orders and a hired builder.
    #[serde(default)]
    pub escrow: u32,
    /// The hired builder and the stages paid for and not yet worked.
    #[serde(default)]
    pub builder: Option<SimId>,
    #[serde(default)]
    pub stages: u32,
    /// Deliveries on their way, by hauler or friend: what it carries or is
    /// buying for the homestead (`loaded` once in the pack).
    #[serde(default)]
    pub incoming: BTreeMap<SimId, super::civic::Hold>,
    /// The part of the escrow that pays for the open orders, by good: the
    /// quote and carrying fee of the day the order was placed.
    #[serde(default)]
    pub order_coins: Goods,
    /// The hired builder's claim while it walks to the door or works there.
    #[serde(default)]
    pub builder_until: u64,
    /// Tick the last order was placed; `None` before the first.
    #[serde(default)]
    pub ordered_at: Option<u64>,
}
impl Upgrade {
    /// Units of `kind` still to be delivered.
    pub fn missing(&self, kind: &str) -> u32 {
        self.cost
            .get(kind)
            .copied()
            .unwrap_or(0)
            .saturating_sub(self.delivered.get(kind).copied().unwrap_or(0))
    }
    pub fn supplied(&self) -> bool {
        self.cost.keys().all(|kind| self.missing(kind) == 0)
    }
    /// Coins of the escrow that pay a hired builder: what the orders do not
    /// hold.
    pub fn wages_held(&self) -> u32 {
        self.escrow.saturating_sub(
            self.order_coins
                .values()
                .copied()
                .fold(0, u32::saturating_add),
        )
    }
}

/// A friend's errand on a played character's homestead (`LiveState.helpers`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Help {
    /// The played character helped.
    pub owner: SimId,
    /// `material` (bring one unit the upgrade misses), `work` (one stage at
    /// the door) or `harvest` (a ripe bed of the owner's plot).
    pub how: String,
    /// The material of a `material` errand.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    /// The store the friend buys the material at, when it carries none.
    #[serde(default)]
    pub store: Option<SimId>,
    /// The plot and bed of a `harvest` errand.
    #[serde(default)]
    pub plot: Option<SimId>,
    #[serde(default)]
    pub bed: u32,
    /// The errand lapses at this tick if it is not done.
    pub until: u64,
}

/// One stage of work at a homestead upgrade, done at the door by a hired
/// builder or a helping friend. Credited when it ends; abandoned (not
/// credited) when the worker is called away.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct UpgradeWork {
    /// The played character whose upgrade it is.
    pub owner: SimId,
    pub ends: u64,
    pub started: u64,
    /// Work ticks the stage takes off the upgrade.
    pub ticks: u32,
    /// Paid from the escrow (the hired builder), or a friend's help.
    pub hired: bool,
}
impl SimHash for UpgradeWork {
    const NAME: &'static str = "regional_upgrade_work_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.owner.0);
        h.u64(self.ends);
        h.u64(self.started);
        h.u32(self.ticks);
        h.u8(u8::from(self.hired));
    }
}

/// A gathering going on, and the last ones held.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct HeldGathering {
    pub key: String,
    pub day: u64,
    pub attended: Vec<SimId>,
}

/// Grief of one resident for somebody laid to rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Mourning {
    pub mourner: SimId,
    pub name: String,
    pub until_day: u64,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct LiveState {
    /// Played character, then resident.
    pub bonds: BTreeMap<SimId, BTreeMap<SimId, Bond>>,
    pub ledgers: BTreeMap<SimId, DayLedger>,
    pub summaries: BTreeMap<SimId, DaySummary>,
    /// Asleep until this tick.
    pub sleeping: BTreeMap<SimId, u64>,
    pub repeats: BTreeMap<SimId, RepeatOrder>,
    /// Goal counters by played character: `harvest`, `gift`, `sleep`,
    /// `gathering`, `trade`, `talk`.
    pub counters: BTreeMap<SimId, BTreeMap<String, u32>>,
    /// Homestead tiers finished, in order.
    pub tiers: BTreeMap<SimId, Vec<String>>,
    pub upgrades: BTreeMap<SimId, Upgrade>,
    pub gathering: Option<HeldGathering>,
    /// The last sixteen gatherings, oldest first.
    pub held: Vec<HeldGathering>,
    pub mourning: Vec<Mourning>,
    pub last_day: Option<u64>,
    /// The homestead chest of each played character.
    #[serde(default)]
    pub chests: BTreeMap<SimId, Goods>,
    /// Friends on an errand to a homestead, by friend.
    #[serde(default)]
    pub helpers: BTreeMap<SimId, Help>,
    /// The latest gathering or rest of each played character: when it began
    /// and ends, for `view::activity_progress`.
    #[serde(default)]
    pub timers: BTreeMap<SimId, Timer>,
}

/// A timed activity of a played character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Timer {
    /// `gather` or `rest`.
    pub kind: String,
    pub from: u64,
    pub until: u64,
}

/// `(done, total)` ticks of a played character's gathering or rest, when
/// its start is known.
pub fn timer_progress(world: &World, unit: SimId, kind: &str, until: u64) -> Option<(u64, u64)> {
    let timer = world.get_resource::<LiveState>()?.timers.get(&unit)?;
    if timer.kind != kind || timer.until != until {
        return None;
    }
    let now = world.resource::<SimClock>().tick;
    let total = until.saturating_sub(timer.from).max(1);
    Some((now.saturating_sub(timer.from).min(total), total))
}
impl SimHashResource for LiveState {
    const NAME: &'static str = "regional_live_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        // Plain data with ordered maps: its canonical text is its identity.
        h.str(&serde_json::to_string(self).unwrap_or_default());
    }
    fn snapshot_fns() -> Option<npc_framework::snapshot::ResourceSnapshotFns> {
        Some(npc_framework::snapshot::serde_resource_fns::<Self>())
    }
}

/// Regional profile only.
pub fn register(app: &mut App) {
    app.init_resource::<LiveState>()
        .register_sim_resource::<LiveState>()
        .register_sim_component::<UpgradeWork>()
        .register_command::<LiveAct>();
}

mod jobs;
pub use jobs::{HomesteadTrip, UpgradeWorkerView, help_trip, homestead_trip, work_target};
pub(crate) use jobs::{
    build as build_step, cancel_work, haul as haul_step, help as help_step, order_demand,
};

fn spec(world: &World) -> Option<&LiveSpec> {
    economy_spec::registry(world)
        .map(|registry| &registry.spec.live)
        .filter(|live| live.enabled)
}

fn players(world: &World) -> Vec<(SimId, Entity)> {
    let Some(mut query) = world.try_query::<(Entity, &SimId, &Villager, &Wallet)>() else {
        return Vec::new();
    };
    let mut out: Vec<(SimId, Entity)> = query
        .iter(world)
        .filter(|(entity, _, person, _)| person.role == Role::Visitor && living(world, *entity))
        .map(|(entity, id, _, _)| (*id, entity))
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

fn is_player(world: &World, actor: Entity) -> bool {
    world
        .get::<Villager>(actor)
        .is_some_and(|person| person.role == Role::Visitor)
        && world.get::<Wallet>(actor).is_some()
}

fn is_resident(world: &World, actor: Entity) -> bool {
    living(world, actor)
        && world.get::<SocialLife>(actor).is_some()
        && world
            .get::<Villager>(actor)
            .is_some_and(|person| person.role != Role::Visitor)
}

fn entity(world: &World, id: SimId) -> Option<Entity> {
    world.resource::<SimIndex>().get(id)
}

fn pack(world: &World, actor: Entity) -> BTreeMap<String, u32> {
    catalog::trade_kinds(world)
        .iter()
        .filter_map(|kind| {
            let held = econ::count(world, actor, kind);
            (held > 0).then(|| ((*kind).to_owned(), held))
        })
        .collect()
}

/// Name of the tier a score has reached, and its index.
pub fn tier_of(world: &World, score: i32) -> (usize, String) {
    let Some(spec) = spec(world) else {
        return (0, String::new());
    };
    spec.tiers
        .iter()
        .enumerate()
        .rev()
        .find(|(_, tier)| score >= tier.score)
        .map_or((0, String::new()), |(index, tier)| {
            (index, tier.name.clone())
        })
}

/// The goods one resident likes and dislikes as a gift: seeded, for life.
pub fn tastes(world: &World, resident: SimId) -> Option<(String, String)> {
    let goods = &spec(world)?.gift_goods;
    if goods.len() < 2 {
        return None;
    }
    let seed = world.resource::<WorldSeed>().0;
    let liked = sim_rand(seed, resident, 0, TASTE_PURPOSE) as usize % goods.len();
    let step = 1 + sim_rand(seed, resident, 1, TASTE_PURPOSE) as usize % (goods.len() - 1);
    Some((
        goods[liked].clone(),
        goods[(liked + step) % goods.len()].clone(),
    ))
}

fn bond_mut(world: &mut World, player: SimId, resident: SimId) -> Mut<'_, LiveState> {
    let mut state = world.resource_mut::<LiveState>();
    state
        .bonds
        .entry(player)
        .or_default()
        .entry(resident)
        .or_default();
    state
}

fn change_bond(world: &mut World, player: SimId, resident: SimId, points: i32, why: &str) {
    let day = super::calendar(world).0;
    let before = score(world, player, resident);
    let after = (before + points).clamp(0, 100_000);
    {
        let mut guard = bond_mut(world, player, resident);
        let state = &mut *guard;
        let bond = state
            .bonds
            .get_mut(&player)
            .and_then(|bonds| bonds.get_mut(&resident))
            .expect("bond");
        bond.score = after;
        if points > 0 {
            bond.last_contact_day = day;
        }
        if let Some(ledger) = state.ledgers.get_mut(&player)
            && points > 0
            && !ledger.summary.met.contains(&resident)
        {
            ledger.summary.met.push(resident);
            ledger.summary.met.sort_unstable();
        }
    }
    let (old, _) = tier_of(world, before);
    let (new, name) = tier_of(world, after);
    if old != new {
        record_effect(
            world,
            Some(player),
            "VillageBondTier",
            &json!({"with":resident,"tier":name,"score":after,"reason":why,"up":new > old}),
        );
    }
}

/// The bond score of a played character with a resident.
pub fn score(world: &World, player: SimId, resident: SimId) -> i32 {
    world
        .get_resource::<LiveState>()
        .and_then(|state| state.bonds.get(&player)?.get(&resident))
        .map_or(0, |bond| bond.score)
}

/// Percent a seller takes off for a buyer it is fond of. 0 between residents
/// and outside the regional profile.
pub fn discount_percent(world: &World, buyer: Entity, seller: Entity) -> u32 {
    let Some(spec) = spec(world) else {
        return 0;
    };
    if !is_player(world, buyer) {
        return 0;
    }
    let (Some(player), Some(resident)) = (
        world.get::<SimId>(buyer).copied(),
        world.get::<SimId>(seller).copied(),
    ) else {
        return 0;
    };
    let (tier, _) = tier_of(world, score(world, player, resident));
    spec.tiers
        .get(tier)
        .map_or(0, |tier| tier.discount_percent.min(90))
}

fn bump(world: &mut World, player: SimId, counter: &str) {
    *world
        .resource_mut::<LiveState>()
        .counters
        .entry(player)
        .or_default()
        .entry(counter.into())
        .or_default() += 1;
}

/// Called after a recorded village action of a played character succeeded.
pub(super) fn acted(
    world: &mut World,
    unit: SimId,
    verb: &str,
    target: Option<SimId>,
    kind: &str,
    quantity: u32,
) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let Some(actor) = entity(world, unit).filter(|actor| is_player(world, *actor)) else {
        return;
    };
    let _ = actor;
    if verb == "buy" {
        booked(world, unit, Book::Received, kind, quantity);
    }
    let day = super::calendar(world).0;
    let resident = target.filter(|id| entity(world, *id).is_some_and(|e| is_resident(world, e)));
    match (verb, resident) {
        ("talk", Some(resident)) => {
            let first = {
                let mut state = bond_mut(world, unit, resident);
                let bond = state
                    .bonds
                    .get_mut(&unit)
                    .and_then(|bonds| bonds.get_mut(&resident))
                    .expect("bond");
                let first = bond.talked_day != Some(day);
                bond.talked_day = Some(day);
                first
            };
            if first {
                change_bond(world, unit, resident, spec.talk_points, "talk");
                bump(world, unit, "talk");
                gift_back(world, &spec, unit, resident, day);
            }
        }
        ("buy" | "sell", Some(resident)) => {
            let first = {
                let mut state = bond_mut(world, unit, resident);
                let bond = state
                    .bonds
                    .get_mut(&unit)
                    .and_then(|bonds| bonds.get_mut(&resident))
                    .expect("bond");
                let first = bond.traded_day != Some(day);
                bond.traded_day = Some(day);
                first
            };
            if first {
                change_bond(world, unit, resident, spec.trade_points, "trade");
            }
            bump(world, unit, "trade");
        }
        _ => {}
    }
}

/// Called after a recorded homestead action of a played character succeeded.
pub(super) fn homestead_acted(world: &mut World, unit: SimId, verb: &str) {
    if spec(world).is_none() || verb != "harvest" {
        return;
    }
    if entity(world, unit).is_some_and(|actor| is_player(world, actor)) {
        bump(world, unit, "harvest");
    }
}

/// A fond resident gives something of its own back, once in a while.
fn gift_back(world: &mut World, spec: &LiveSpec, player: SimId, resident: SimId, day: u64) {
    let (tier, _) = tier_of(world, score(world, player, resident));
    let Some(rule) = spec.tiers.get(tier).filter(|tier| tier.gift_back_days > 0) else {
        return;
    };
    let due = world
        .resource::<LiveState>()
        .bonds
        .get(&player)
        .and_then(|bonds| bonds.get(&resident))
        .and_then(|bond| bond.gift_back_day)
        .is_none_or(|last| day >= last + rule.gift_back_days);
    let (Some(giver), Some(taker)) = (entity(world, resident), entity(world, player)) else {
        return;
    };
    if !due || !econ::near(world, giver, taker) {
        return;
    }
    // Something the resident offers for sale anyway, that fits the pack.
    let kind = catalog::trade_kinds(world).iter().copied().find(|kind| {
        super::market::available_to_sell(world, giver, kind) > 0
            && super::civic::storable(world, kind)
            && econ::fit(world, taker, &[Item::new(*kind)]).is_ok()
    });
    let Some(kind) = kind else {
        return;
    };
    if econ::consume(world, giver, kind, 1).is_err() {
        return;
    }
    econ::grant(world, taker, Item::new(kind));
    booked(world, player, Book::Received, kind, 1);
    if let Some(bond) = world
        .resource_mut::<LiveState>()
        .bonds
        .get_mut(&player)
        .and_then(|bonds| bonds.get_mut(&resident))
    {
        bond.gift_back_day = Some(day);
    }
    record_effect(
        world,
        Some(resident),
        "VillageGiftReturned",
        &json!({"to":player,"kind":kind,"quantity":1}),
    );
}

/// A recorded command of live mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveAct {
    pub unit: u32,
    /// `sleep`, `wake`, `gift`, `repeat`, `stop_repeat` or `upgrade`.
    pub verb: String,
    /// The resident of a gift; the resource patch of a repeat order.
    #[serde(default)]
    pub target: Option<SimId>,
    /// The good of a gift; the tier of an upgrade.
    #[serde(default)]
    pub kind: String,
    /// Units of a gift (1 to 5); gatherings of a repeat order (1 to 50);
    /// units to deliver, order, store or take (1 to 20).
    #[serde(default = "one")]
    pub quantity: u32,
    /// Repeat a recipe at the workshop `target`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recipe: String,
    /// Repeat a garden verb (`plant`, `water_crop`, `harvest`) on the
    /// homestead plot `target`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub task: String,
}
fn one() -> u32 {
    1
}
/// Garden verbs a repeat order may use.
const GARDEN_TASKS: [&str; 3] = ["plant", "water_crop", "harvest"];
impl LiveAct {
    pub fn new(unit: SimId, verb: &str) -> Self {
        Self {
            unit: unit.0,
            verb: verb.into(),
            target: None,
            kind: String::new(),
            quantity: 1,
            recipe: String::new(),
            task: String::new(),
        }
    }

    fn actor(&self, world: &World) -> Result<Entity, String> {
        let actor = entity(world, SimId(self.unit)).ok_or("the actor no longer exists")?;
        if !living(world, actor) || !is_player(world, actor) {
            return Err("only a living played character can do this".into());
        }
        Ok(actor)
    }

    pub fn check(&self, world: &World) -> Result<(), String> {
        let spec = spec(world).ok_or("live mode is not part of this world")?;
        let actor = self.actor(world)?;
        let unit = SimId(self.unit);
        let now = world.resource::<SimClock>().tick;
        let asleep = super::indoors::asleep(world, actor);
        match self.verb.as_str() {
            "wake" => {
                if asleep {
                    Ok(())
                } else {
                    Err("not asleep".into())
                }
            }
            _ if asleep => Err("asleep until morning; wake first".into()),
            "sleep" => {
                let home = colony::home_of(world, unit)
                    .and_then(|house| entity(world, house))
                    .and_then(|house| world.get::<Household>(house))
                    .ok_or("you have no bed of your own; rent one at the inn")?;
                let here = world.get::<SimPos>(actor).ok_or("no position")?;
                if here.dist2(home.door) > HOMESTEAD_REACH_MM.pow(2) {
                    return Err("go to the door of your home first".into());
                }
                if super::resident::day_permille(world) < spec.sleep_from_permille as i32 {
                    return Err("it is too early to sleep until morning".into());
                }
                if world
                    .get::<Villager>(actor)
                    .is_some_and(|person| person.resting_until > now)
                {
                    return Err("already resting".into());
                }
                Ok(())
            }
            "gift" => {
                let resident = self
                    .target
                    .and_then(|id| entity(world, id))
                    .filter(|resident| is_resident(world, *resident))
                    .ok_or("choose a living resident")?;
                if !econ::near(world, actor, resident) {
                    return Err("move within 1.5 m of the resident".into());
                }
                if !(1..=5).contains(&self.quantity) {
                    return Err("a gift is 1 to 5 units".into());
                }
                if !super::civic::storable(world, &self.kind) {
                    return Err("tools, clothing and untraded goods cannot be given".into());
                }
                if econ::count(world, actor, &self.kind) < self.quantity {
                    return Err("you do not carry that many".into());
                }
                econ::fit(
                    world,
                    resident,
                    &[Item::new(&self.kind).stack(self.quantity)],
                )
                .map_err(|_| "the resident cannot carry it".to_owned())
            }
            "repeat" => {
                if !(1..=50).contains(&self.quantity) {
                    return Err("repeat 1 to 50 times".into());
                }
                if !self.recipe.is_empty() {
                    let station = self
                        .target
                        .and_then(|id| entity(world, id))
                        .ok_or("choose a workshop")?;
                    if world
                        .get::<npc_framework::crafting::Workstation>(station)
                        .is_none()
                    {
                        return Err("that is not a workshop".into());
                    }
                    return npc_framework::crafting::can_craft(
                        world,
                        actor,
                        &self.recipe,
                        self.target,
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                }
                if !self.task.is_empty() {
                    if !GARDEN_TASKS.contains(&self.task.as_str()) {
                        return Err("repeat plant, water_crop or harvest".into());
                    }
                    let plot = self
                        .target
                        .and_then(|id| entity(world, id))
                        .ok_or("choose a homestead plot")?;
                    let owned = world
                        .get::<super::homestead::HomesteadPlot>(plot)
                        .ok_or("that is not a homestead plot")?
                        .owner;
                    if owned != Some(unit) {
                        return Err("work your own plot".into());
                    }
                    return Ok(());
                }
                let patch = self
                    .target
                    .and_then(|id| entity(world, id))
                    .ok_or("choose a resource to gather")?;
                if world.get::<super::ResourcePatch>(patch).is_none() {
                    return Err("that is not a resource".into());
                }
                Ok(())
            }
            "stop_repeat" => Ok(()),
            "upgrade" => upgrade_check(world, spec, unit, actor, &self.kind),
            "cancel_upgrade" => {
                if world.resource::<LiveState>().upgrades.contains_key(&unit) {
                    Ok(())
                } else {
                    Err("no upgrade is under way".into())
                }
            }
            "deliver" => {
                let upgrade = site(world, unit)?;
                at_door(world, unit, actor)?;
                quantity_check(self.quantity)?;
                if upgrade.missing(&self.kind) < self.quantity {
                    return Err("the upgrade does not need that many of this material".into());
                }
                if econ::count(world, actor, &self.kind) < self.quantity {
                    return Err("you do not carry that many".into());
                }
                Ok(())
            }
            "order" => {
                let upgrade = site(world, unit)?;
                quantity_check(self.quantity)?;
                let open = upgrade
                    .missing(&self.kind)
                    .saturating_sub(upgrade.ordered.get(&self.kind).copied().unwrap_or(0));
                if open < self.quantity {
                    return Err(
                        "that many of this material are not wanted or already ordered".into(),
                    );
                }
                let total = order_total(world, spec, &self.kind, self.quantity)
                    .ok_or("this good is not traded here")?;
                if world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins) < total {
                    return Err(format!("the order and its carrying fee cost {total} coins"));
                }
                Ok(())
            }
            "hire" => {
                let upgrade = site(world, unit)?;
                if upgrade.builder.is_some() {
                    return Err("a builder is already hired".into());
                }
                builder_for(world, unit).ok_or("nobody in the settlement can be hired")?;
                let cost = hire_cost(world, spec, upgrade.remaining_ticks);
                if world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins) < cost {
                    return Err(format!("hiring a builder for the rest costs {cost} coins"));
                }
                Ok(())
            }
            "store" | "take" => {
                at_door(world, unit, actor)?;
                quantity_check(self.quantity)?;
                let capacity = chest_capacity(world, unit);
                if capacity == 0 {
                    return Err("build a storage chest first".into());
                }
                let chest = world
                    .resource::<LiveState>()
                    .chests
                    .get(&unit)
                    .cloned()
                    .unwrap_or_default();
                if self.verb == "store" {
                    if !super::civic::storable(world, &self.kind) {
                        return Err("tools, clothing and untraded goods cannot be stored".into());
                    }
                    if econ::count(world, actor, &self.kind) < self.quantity {
                        return Err("you do not carry that many".into());
                    }
                    let used: u32 = chest.values().sum();
                    if used + self.quantity > capacity {
                        return Err(format!(
                            "the chest holds {capacity}; it has room for {}",
                            capacity.saturating_sub(used)
                        ));
                    }
                    Ok(())
                } else {
                    if chest.get(&self.kind).copied().unwrap_or(0) < self.quantity {
                        return Err("the chest does not hold that many".into());
                    }
                    econ::fit(world, actor, &[Item::new(&self.kind).stack(self.quantity)])
                        .map_err(|_| "your pack has no room for it".to_owned())
                }
            }
            _ => Err("unknown live action".into()),
        }
    }

    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        let actor = self.actor(world)?;
        let unit = SimId(self.unit);
        let now = world.resource::<SimClock>().tick;
        match self.verb.as_str() {
            "wake" => {
                super::indoors::wake(world, actor);
                world.resource_mut::<LiveState>().sleeping.remove(&unit);
            }
            "sleep" => {
                let day = world.resource::<SimConfig>().day_length.max(1);
                let until = (now / day + 1) * day;
                super::ai::stop_work(world, actor, "going to bed");
                super::indoors::sleep_until(world, actor, until);
                world
                    .resource_mut::<LiveState>()
                    .sleeping
                    .insert(unit, until);
                record_effect(world, Some(unit), "VillageSleep", &json!({"until":until}));
            }
            "gift" => {
                let resident_id = self.target.expect("checked resident");
                let resident = entity(world, resident_id).expect("checked resident");
                econ::consume(world, actor, &self.kind, self.quantity)?;
                econ::grant(world, resident, Item::new(&self.kind).stack(self.quantity));
                let spec = spec(world).cloned().expect("checked spec");
                let day = super::calendar(world).0;
                let taste = tastes(world, resident_id);
                let (points, liked) = match &taste {
                    Some((liked, _)) if *liked == self.kind => (spec.gift_liked_points, "liked"),
                    Some((_, disliked)) if *disliked == self.kind => {
                        (spec.gift_disliked_points, "disliked")
                    }
                    _ => (spec.gift_plain_points, "plain"),
                };
                // One gift a day counts; a second is still handed over.
                let first = {
                    let mut state = bond_mut(world, unit, resident_id);
                    let bond = state
                        .bonds
                        .get_mut(&unit)
                        .and_then(|bonds| bonds.get_mut(&resident_id))
                        .expect("bond");
                    let first = bond.gifted_day != Some(day);
                    bond.gifted_day = Some(day);
                    first
                };
                if first {
                    change_bond(world, unit, resident_id, points, "gift");
                }
                bump(world, unit, "gift");
                record_effect(
                    world,
                    Some(unit),
                    "VillageGift",
                    &json!({"to":resident_id,"kind":self.kind,"quantity":self.quantity,
                        "taste":liked,"counted":first,"score":score(world, unit, resident_id)}),
                );
            }
            "repeat" => {
                world.resource_mut::<LiveState>().repeats.insert(
                    unit,
                    RepeatOrder {
                        target: self.target.expect("checked target"),
                        remaining: self.quantity,
                        done: 0,
                        recipe: self.recipe.clone(),
                        task: self.task.clone(),
                    },
                );
            }
            "stop_repeat" => {
                world.resource_mut::<LiveState>().repeats.remove(&unit);
            }
            "upgrade" => {
                let tier = spec(world)
                    .and_then(|spec| spec.homestead.iter().find(|tier| tier.key == self.kind))
                    .cloned()
                    .expect("checked tier");
                let mut cost = Goods::new();
                for stack in &tier.cost {
                    *cost.entry(stack.good.clone()).or_default() += stack.count;
                }
                world.resource_mut::<LiveState>().upgrades.insert(
                    unit,
                    Upgrade {
                        key: tier.key.clone(),
                        remaining_ticks: u64::from(tier.work_ticks),
                        cost: cost.clone(),
                        delivered: Goods::new(),
                        ordered: Goods::new(),
                        escrow: 0,
                        builder: None,
                        stages: 0,
                        incoming: BTreeMap::new(),
                        order_coins: Goods::new(),
                        builder_until: 0,
                        ordered_at: None,
                    },
                );
                record_effect(
                    world,
                    Some(unit),
                    "VillageHomesteadUpgradeStarted",
                    &json!({"tier":tier.key,"name":tier.name,"work_ticks":tier.work_ticks,
                        "cost":cost}),
                );
                deliver_from_pack(world, unit, actor, None);
            }
            "cancel_upgrade" => cancel_upgrade(world, unit, actor),
            "deliver" => {
                deliver_from_pack(world, unit, actor, Some((&self.kind, self.quantity)));
            }
            "order" => {
                let spec = spec(world).cloned().expect("checked spec");
                let total =
                    order_total(world, &spec, &self.kind, self.quantity).expect("checked price");
                world
                    .get_mut::<Wallet>(actor)
                    .expect("checked wallet")
                    .coins -= total;
                {
                    let mut state = world.resource_mut::<LiveState>();
                    let upgrade = state.upgrades.get_mut(&unit).expect("checked site");
                    upgrade.escrow += total;
                    *upgrade.ordered.entry(self.kind.clone()).or_default() += self.quantity;
                    *upgrade.order_coins.entry(self.kind.clone()).or_default() += total;
                    upgrade.ordered_at = Some(now);
                }
                record_effect(
                    world,
                    Some(unit),
                    "VillageHomesteadOrder",
                    &json!({"kind":self.kind,"quantity":self.quantity,"escrow":total}),
                );
            }
            "hire" => {
                let spec = spec(world).cloned().expect("checked spec");
                let builder = builder_for(world, unit).expect("checked builder");
                let remaining = site(world, unit).expect("checked site").remaining_ticks;
                let cost = hire_cost(world, &spec, remaining);
                let stages = stage_count(world, remaining);
                world
                    .get_mut::<Wallet>(actor)
                    .expect("checked wallet")
                    .coins -= cost;
                {
                    let mut state = world.resource_mut::<LiveState>();
                    let upgrade = state.upgrades.get_mut(&unit).expect("checked site");
                    upgrade.escrow += cost;
                    upgrade.builder = Some(builder);
                    upgrade.stages = stages;
                    upgrade.builder_until = 0;
                }
                record_effect(
                    world,
                    Some(unit),
                    "VillageHomesteadBuilderHired",
                    &json!({"builder":builder,"coins":cost,"wage":spec.builder_wage}),
                );
            }
            "store" => {
                econ::consume(world, actor, &self.kind, self.quantity)?;
                *world
                    .resource_mut::<LiveState>()
                    .chests
                    .entry(unit)
                    .or_default()
                    .entry(self.kind.clone())
                    .or_default() += self.quantity;
                record_effect(
                    world,
                    Some(unit),
                    "VillageChestStored",
                    &json!({"kind":self.kind,"quantity":self.quantity}),
                );
            }
            "take" => {
                {
                    let mut state = world.resource_mut::<LiveState>();
                    let chest = state.chests.entry(unit).or_default();
                    let held = chest.get_mut(&self.kind).expect("checked chest");
                    *held -= self.quantity;
                    if *held == 0 {
                        chest.remove(&self.kind);
                    }
                }
                econ::grant(world, actor, Item::new(&self.kind).stack(self.quantity));
                record_effect(
                    world,
                    Some(unit),
                    "VillageChestTaken",
                    &json!({"kind":self.kind,"quantity":self.quantity}),
                );
            }
            _ => return Err("unknown live action".into()),
        }
        Ok(())
    }
}
impl ContentCommand for LiveAct {
    const NAME: &'static str = "LiveAct";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world)
            .map_err(|reason| CommandError::Unavailable { reason })
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
    fn unit(&self) -> Option<SimId> {
        Some(SimId(self.unit))
    }
}

/// The homestead of a played character: its cottage's household and door.
fn homestead(world: &World, owner: SimId) -> Option<(Entity, SimPos)> {
    let house = entity(world, colony::home_of(world, owner)?)?;
    world.get::<super::homestead::Cottage>(house)?;
    Some((house, world.get::<Household>(house)?.door))
}

fn goal_reached(world: &World, unit: SimId, goal: &GoalSpec) -> (u32, bool) {
    let state = world.resource::<LiveState>();
    let counter = |name: &str| {
        state
            .counters
            .get(&unit)
            .and_then(|counters| counters.get(name))
            .copied()
            .unwrap_or(0)
    };
    let progress = match goal.kind.as_str() {
        "own_cottage" => u32::from(homestead(world, unit).is_some()),
        "friend" => {
            let friend = spec(world).map_or(i32::MAX, |spec| {
                spec.tiers
                    .get(goal.tier as usize)
                    .map_or(i32::MAX, |tier| tier.score)
            });
            state.bonds.get(&unit).map_or(0, |bonds| {
                bonds.values().filter(|bond| bond.score >= friend).count() as u32
            })
        }
        "upgrade" => state.tiers.get(&unit).map_or(0, |tiers| tiers.len() as u32),
        "homestead" => u32::from(
            state
                .tiers
                .get(&unit)
                .is_some_and(|tiers| tiers.contains(&goal.homestead)),
        ),
        "coins" => entity(world, unit)
            .and_then(|actor| world.get::<Wallet>(actor))
            .map_or(0, |wallet| wallet.coins),
        other => counter(other),
    };
    (progress.min(goal.count), progress >= goal.count)
}

fn upgrade_check(
    world: &World,
    spec: &LiveSpec,
    unit: SimId,
    actor: Entity,
    key: &str,
) -> Result<(), String> {
    let tier = spec
        .homestead
        .iter()
        .find(|tier| tier.key == key)
        .ok_or("unknown homestead tier")?;
    let reason = tier_blocked(world, spec, unit, tier);
    if !reason.is_empty() {
        return Err(reason);
    }
    let (_, door) = homestead(world, unit).ok_or("build your cottage first")?;
    let here = world.get::<SimPos>(actor).ok_or("no position")?;
    if here.dist2(door) > HOMESTEAD_REACH_MM.pow(2) {
        return Err("go to the door of your cottage first".into());
    }
    Ok(())
}

/// Why a tier cannot be started now; empty when it can.
fn tier_blocked(world: &World, spec: &LiveSpec, unit: SimId, tier: &HomesteadTier) -> String {
    let state = world.resource::<LiveState>();
    let done = state.tiers.get(&unit).cloned().unwrap_or_default();
    if done.contains(&tier.key) {
        return "already built".into();
    }
    if state.upgrades.contains_key(&unit) {
        return "another upgrade is under way".into();
    }
    if homestead(world, unit).is_none() {
        return "build your cottage first".into();
    }
    if !tier.after.is_empty() && !done.contains(&tier.after) {
        let name = spec
            .homestead
            .iter()
            .find(|other| other.key == tier.after)
            .map_or(tier.after.as_str(), |other| other.name.as_str());
        return format!("build {name} first");
    }
    if !tier.goal.is_empty()
        && let Some(goal) = spec.goals.iter().find(|goal| goal.key == tier.goal)
        && !goal_reached(world, unit, goal).1
    {
        return format!("reach the goal \"{}\" first", goal.title);
    }
    if entity(world, unit).is_none() {
        return "the owner is gone".into();
    }
    if !tier.building.is_empty()
        && let Some((house, door)) = homestead(world, unit)
    {
        let (centre, body, _) = super::homestead::cottage_body(world, door, Some(&tier.building));
        if let Err(reason) = super::homestead::body_fits(world, centre, body, Some(house)) {
            return format!("the cottage cannot grow here: {reason}");
        }
    }
    String::new()
}

/// The settlement a played character's cottage belongs to: the nearest
/// settlement centre to its door.
pub fn homestead_settlement(world: &World, unit: SimId) -> Option<SimId> {
    let (house, door) = homestead(world, unit)?;
    super::frontier::settlements::member(world, house).or_else(|| {
        super::frontier::settlements::all(world)
            .into_iter()
            .filter_map(|(id, _)| {
                Some((
                    super::frontier::settlements::definition(world, id)?
                        .center
                        .dist2(door),
                    id,
                ))
            })
            .min()
            .map(|(_, id)| id)
    })
}

/// The catalogue home the cottage grows into, if a tier names one.
pub fn larger_building(world: &World) -> Option<String> {
    spec(world)?
        .homestead
        .iter()
        .find(|tier| !tier.building.is_empty())
        .map(|tier| tier.building.clone())
}

fn quantity_check(quantity: u32) -> Result<(), String> {
    if (1..=20).contains(&quantity) {
        Ok(())
    } else {
        Err("quantity must be 1 to 20".into())
    }
}

/// The upgrade under way of a played character.
fn site(world: &World, unit: SimId) -> Result<Upgrade, String> {
    world
        .resource::<LiveState>()
        .upgrades
        .get(&unit)
        .cloned()
        .ok_or_else(|| "no upgrade is under way".to_owned())
}

fn at_door(world: &World, unit: SimId, actor: Entity) -> Result<(), String> {
    let (_, door) = homestead(world, unit).ok_or("build your cottage first")?;
    let here = world.get::<SimPos>(actor).ok_or("no position")?;
    if here.dist2(door) > HOMESTEAD_REACH_MM.pow(2) {
        return Err("go to the door of your cottage first".into());
    }
    Ok(())
}

/// Price and carrying fee of an order: today's quote and `haul_fee_percent`.
fn order_total(world: &World, spec: &LiveSpec, kind: &str, quantity: u32) -> Option<u32> {
    let goods = super::market::unit_price(world, kind)?.checked_mul(quantity)?;
    let fee = (u64::from(goods) * u64::from(spec.haul_fee_percent)).div_ceil(100) as u32;
    goods.checked_add(fee)
}

fn stage_ticks(world: &World) -> u64 {
    economy_spec::tuning(world).map_or(200, |tuning| u64::from(tuning.build_stage_ticks.max(1)))
}

fn stage_count(world: &World, remaining: u64) -> u32 {
    remaining.div_ceil(stage_ticks(world)) as u32
}

fn hire_cost(world: &World, spec: &LiveSpec, remaining: u64) -> u32 {
    stage_count(world, remaining).saturating_mul(spec.builder_wage)
}

/// The resident a played character can hire to build: a living resident of
/// the homestead's settlement, a carpenter first, then by identity.
fn builder_for(world: &World, unit: SimId) -> Option<SimId> {
    let settlement = homestead_settlement(world, unit)?;
    super::demography::members(world, settlement)
        .into_iter()
        .filter(|(id, actor)| *id != unit && is_resident(world, *actor))
        .min_by_key(|(id, actor)| {
            (
                world
                    .get::<Villager>(*actor)
                    .is_none_or(|person| person.role != super::roles::roles(world).builder),
                *id,
            )
        })
        .map(|(id, _)| id)
}

/// Units the homestead chest of a played character holds: the storage of its
/// finished tiers.
pub fn chest_capacity(world: &World, unit: SimId) -> u32 {
    let (Some(spec), Some(state)) = (spec(world), world.get_resource::<LiveState>()) else {
        return 0;
    };
    let done = state.tiers.get(&unit).cloned().unwrap_or_default();
    spec.homestead
        .iter()
        .filter(|tier| done.contains(&tier.key))
        .map(|tier| tier.storage)
        .fold(0, u32::saturating_add)
}

/// Units of `kind` lying in homestead upgrade sites (delivered, not yet
/// spent) and homestead chests: part of every goods count.
pub fn goods_in_custody(world: &World, kind: &str) -> u32 {
    world.get_resource::<LiveState>().map_or(0, |state| {
        let sites: u32 = state
            .upgrades
            .values()
            .map(|upgrade| upgrade.delivered.get(kind).copied().unwrap_or(0))
            .sum();
        let chests: u32 = state
            .chests
            .values()
            .map(|chest| chest.get(kind).copied().unwrap_or(0))
            .sum();
        sites + chests
    })
}

/// Coins held in escrow for homestead orders and builders: part of every
/// conservation sum.
pub fn escrow(world: &World) -> u64 {
    world.get_resource::<LiveState>().map_or(0, |state| {
        state
            .upgrades
            .values()
            .map(|upgrade| u64::from(upgrade.escrow))
            .sum()
    })
}

/// Hands carried materials over to the upgrade: `only` one good and count,
/// or everything it still misses.
fn deliver_from_pack(world: &mut World, unit: SimId, actor: Entity, only: Option<(&str, u32)>) {
    let Some(upgrade) = world.resource::<LiveState>().upgrades.get(&unit).cloned() else {
        return;
    };
    let mut handed = Goods::new();
    for kind in upgrade.cost.keys() {
        if only.is_some_and(|(good, _)| good != kind) {
            continue;
        }
        let wanted = only.map_or(upgrade.missing(kind), |(_, count)| count);
        let count = wanted
            .min(upgrade.missing(kind))
            .min(econ::count(world, actor, kind));
        if count > 0 && econ::consume(world, actor, kind, count).is_ok() {
            handed.insert(kind.clone(), count);
        }
    }
    if handed.is_empty() {
        return;
    }
    let refund = {
        let mut state = world.resource_mut::<LiveState>();
        let upgrade = state.upgrades.get_mut(&unit).expect("upgrade");
        let mut refund = 0;
        for (kind, count) in &handed {
            *upgrade.delivered.entry(kind.clone()).or_default() += count;
            // A delivery from the pack also answers an open order: the units
            // nobody needs to bring any more, and their coins, are returned.
            let still = upgrade.missing(kind);
            let ordered = upgrade.ordered.get(kind).copied().unwrap_or(0);
            if ordered > still {
                let coins = jobs::order_pay(upgrade, kind, ordered - still);
                refund += coins;
                upgrade.escrow -= coins;
                if let Some(held) = upgrade.order_coins.get_mut(kind) {
                    *held -= coins;
                }
                upgrade.ordered.insert(kind.clone(), still);
            }
        }
        upgrade.ordered.retain(|_, units| *units > 0);
        let ordered = upgrade.ordered.clone();
        upgrade
            .order_coins
            .retain(|kind, coins| *coins > 0 && ordered.contains_key(kind));
        refund
    };
    if refund > 0
        && let Some(mut wallet) = world.get_mut::<Wallet>(actor)
    {
        wallet.coins += refund;
    }
    record_effect(
        world,
        Some(unit),
        "VillageHomesteadDelivered",
        &json!({"by":unit,"goods":handed,"refunded":refund}),
    );
}

/// Gives back what an abandoned upgrade received: goods to the owner's pack
/// (or the ground at the door when it is full) and the escrow to the wallet.
/// Workers leave the door unpaid for the unfinished stage, friends drop
/// their errand, and a hauler on the way keeps what it carries: it bought
/// those goods with its own coins (or they were its own stock) and nothing
/// was paid from the escrow, so the purchase stays with it.
fn cancel_upgrade(world: &mut World, unit: SimId, actor: Entity) {
    let Some(upgrade) = world.resource_mut::<LiveState>().upgrades.remove(&unit) else {
        return;
    };
    let released = jobs::release(world, unit);
    let door = homestead(world, unit).map(|(_, door)| door);
    let mut dropped = Goods::new();
    for (kind, count) in &upgrade.delivered {
        if *count == 0 {
            continue;
        }
        if econ::fit(world, actor, &[Item::new(kind).stack(*count)]).is_ok() {
            econ::grant(world, actor, Item::new(kind).stack(*count));
        } else if let Some(door) = door {
            spawn_sim(
                world,
                (
                    door,
                    Item::new(kind).stack(*count),
                    Perceivable::new(crate::world::kinds::ITEM),
                ),
            );
            dropped.insert(kind.clone(), *count);
        }
    }
    if let Some(mut wallet) = world.get_mut::<Wallet>(actor) {
        wallet.coins = wallet.coins.saturating_add(upgrade.escrow);
    }
    record_effect(
        world,
        Some(unit),
        "VillageHomesteadUpgradeCancelled",
        &json!({"tier":upgrade.key,"returned":upgrade.delivered,"dropped":dropped,
            "refunded":upgrade.escrow,"released":released,
            "kept_by_haulers":upgrade.incoming.iter()
                .filter(|(_, hold)| hold.loaded)
                .map(|(hauler, hold)| json!({"hauler":hauler,"kind":hold.kind,"quantity":hold.count}))
                .collect::<Vec<_>>()}),
    );
}

/// Units of gathering time a played character needs, in percent of the
/// ordinary time: its best finished tool tier. 100 without one.
pub fn gather_percent(world: &World, actor: Entity) -> u32 {
    let (Some(spec), Some(unit)) = (spec(world), world.get::<SimId>(actor).copied()) else {
        return 100;
    };
    let Some(done) = world
        .get_resource::<LiveState>()
        .and_then(|state| state.tiers.get(&unit))
    else {
        return 100;
    };
    spec.homestead
        .iter()
        .filter(|tier| done.contains(&tier.key) && tier.gather_percent > 0)
        .map(|tier| tier.gather_percent)
        .min()
        .unwrap_or(100)
}

/// The finished homestead tier that gives a played character its better
/// tools (the one with the lowest `gather_percent`), if any.
pub fn tool_tier(world: &World, actor: Entity) -> Option<String> {
    let spec = spec(world)?;
    let unit = world.get::<SimId>(actor)?;
    let done = world.get_resource::<LiveState>()?.tiers.get(unit)?;
    spec.homestead
        .iter()
        .filter(|tier| done.contains(&tier.key) && tier.gather_percent > 0)
        .min_by_key(|tier| tier.gather_percent)
        .map(|tier| tier.key.clone())
}

/// Homestead plots a played character may own: one, plus finished tiers.
pub fn plot_allowance(world: &World, owner: SimId) -> u32 {
    let Some(spec) = spec(world) else {
        return 1;
    };
    let done = world
        .get_resource::<LiveState>()
        .and_then(|state| state.tiers.get(&owner))
        .cloned()
        .unwrap_or_default();
    1 + spec
        .homestead
        .iter()
        .filter(|tier| done.contains(&tier.key))
        .map(|tier| tier.plots)
        .sum::<u32>()
}

fn finish_upgrade(world: &mut World, unit: SimId, actor: Entity, tier: &HomesteadTier) {
    let Some(upgrade) = world.resource_mut::<LiveState>().upgrades.remove(&unit) else {
        return;
    };
    jobs::release(world, unit);
    // What was held and not spent goes back.
    if let Some(mut wallet) = world.get_mut::<Wallet>(actor) {
        wallet.coins = wallet.coins.saturating_add(upgrade.escrow);
    }
    if let Some((house, door)) = homestead(world, unit) {
        let mut beds = None;
        if !tier.building.is_empty() {
            // The body grows into the catalogue home: the door stays.
            let (centre, body, capacity) =
                super::homestead::cottage_body(world, door, Some(&tier.building));
            world
                .entity_mut(house)
                .insert((centre, GridBlocker(body), GridOccluder(body)));
            beds = Some(capacity);
        }
        if let Some(mut home) = world.get_mut::<Household>(house) {
            home.capacity = match beds {
                Some(capacity) => capacity.max(home.capacity),
                None => home.capacity + tier.beds,
            };
        }
    }
    world
        .resource_mut::<LiveState>()
        .tiers
        .entry(unit)
        .or_default()
        .push(tier.key.clone());
    record_effect(
        world,
        Some(unit),
        "VillageHomesteadUpgraded",
        &json!({"tier":tier.key,"name":tier.name,"beds":tier.beds,"plots":tier.plots,
            "gather_percent":tier.gather_percent,"storage":tier.storage,
            "building":tier.building,"refunded":upgrade.escrow}),
    );
    super::colony_demand_changed(world);
}

/// The gathering of the calendar that is on at this moment.
fn gathering_now(world: &World) -> Option<&super::frontier::life_spec::GatheringSpec> {
    let spec = spec(world)?;
    let (day, season, _) = super::calendar(world);
    let of_season = day % super::days_per_season(world) + 1;
    let at = super::resident::day_permille(world);
    spec.gatherings.iter().find(|gathering| {
        season_key(season) == gathering.season
            && gathering.day.min(super::days_per_season(world)) == of_season
            && (gathering.from_permille as i32..gathering.to_permille as i32).contains(&at)
    })
}

fn season_key(season: Season) -> &'static str {
    match season {
        Season::Spring => "spring",
        Season::Summer => "summer",
        Season::Autumn => "autumn",
        Season::Winter => "winter",
    }
}

/// Whether the village is gathering now and this resident is free to go: a
/// resident whose need is at or below `gathering_needs_above` sees to it first.
pub fn attending(world: &World, actor: Entity) -> bool {
    let Some(spec) = spec(world) else {
        return false;
    };
    gathering_now(world).is_some()
        && is_resident(world, actor)
        && NEEDS
            .iter()
            .all(|need| econ::need(world, actor, need) > spec.gathering_needs_above)
}

/// Grief when somebody is laid to rest: the household and those fond of the
/// dead lose heart for some days.
pub(super) fn mourn(world: &mut World, gone: &Departed) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let Some(dead) = gone.resident else {
        return;
    };
    let fond = spec.tiers.get(1).map_or(i32::MAX, |tier| tier.score);
    let Some(mut query) = world.try_query::<(Entity, &SimId, &SocialLife)>() else {
        return;
    };
    let mut mourners: Vec<(SimId, Entity)> = query
        .iter(world)
        .filter(|(actor, id, social)| {
            living(world, *actor)
                && (social.affinity.get(&dead).copied().unwrap_or(0) >= fond
                    || (gone.household.is_some() && colony::home_of(world, **id) == gone.household))
        })
        .map(|(actor, id, _)| (*id, actor))
        .collect();
    mourners.sort_unstable_by_key(|(id, _)| *id);
    let until_day = super::calendar(world).0 + spec.mourning_days;
    for (id, actor) in &mourners {
        if let Some(mut social) = world.get_mut::<SocialLife>(*actor) {
            social.morale = (social.morale - spec.mourning_morale).max(0);
        }
        world.resource_mut::<LiveState>().mourning.push(Mourning {
            mourner: *id,
            name: gone.name.clone(),
            until_day,
        });
    }
    record_effect(
        world,
        None,
        "VillageMourning",
        &json!({"for":gone.name,"mourners":mourners.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            "until_day":until_day}),
    );
}

/// Once per tick in the regional Effects pass.
pub fn tick(world: &mut World) {
    let Some(spec) = spec(world).cloned() else {
        return;
    };
    let now = world.resource::<SimClock>().tick;
    let day = super::calendar(world).0;
    let played = players(world);
    // Stages of work that ended at the doors, before the sites look at
    // themselves.
    jobs::tick(world, &spec);
    for (unit, actor) in &played {
        account(world, *unit, *actor, day);
        let asleep = super::indoors::asleep(world, *actor);
        if !asleep && world.resource::<LiveState>().sleeping.contains_key(unit) {
            let mut state = world.resource_mut::<LiveState>();
            let until = state.sleeping.remove(unit).unwrap_or(0);
            // Slept through: the morning came.
            if now >= until {
                if let Some(ledger) = state.ledgers.get_mut(unit) {
                    ledger.summary.slept = true;
                }
                *state
                    .counters
                    .entry(*unit)
                    .or_default()
                    .entry("sleep".into())
                    .or_default() += 1;
            }
        }
        work_on_upgrade(world, &spec, *unit, *actor);
        if now.is_multiple_of(20) && !asleep {
            repeat(world, *unit, *actor);
        }
    }
    if now.is_multiple_of(50) {
        gather(world, &played, day);
    }
    let last = world.resource::<LiveState>().last_day;
    if last == Some(day) {
        return;
    }
    world.resource_mut::<LiveState>().last_day = Some(day);
    if last.is_none() {
        return;
    }
    morning(world, &spec, &played, day);
}

/// Keeps the running account of a played character's day.
fn account(world: &mut World, unit: SimId, actor: Entity, day: u64) {
    // When a timed activity began, for its progress bar: a gathering and a
    // rest record only their end.
    let now = world.resource::<SimClock>().tick;
    let timer = if let Some(gathering) = world.get::<Gathering>(actor) {
        Some(("gather", gathering.ends))
    } else {
        world
            .get::<Villager>(actor)
            .filter(|person| person.resting_until > now)
            .map(|person| ("rest", person.resting_until))
    };
    if let Some((kind, until)) = timer {
        let state = world.resource::<LiveState>();
        let known = state
            .timers
            .get(&unit)
            .is_some_and(|timer| timer.kind == kind && timer.until == until);
        if !known {
            world.resource_mut::<LiveState>().timers.insert(
                unit,
                Timer {
                    kind: kind.into(),
                    from: now,
                    until,
                },
            );
        }
    }
    let coins = world.get::<Wallet>(actor).map_or(0, |wallet| wallet.coins);
    let carried = BTreeMap::new();
    let gathering = world.get::<Gathering>(actor).is_some();
    let crafting = world.get::<Crafting>(actor).is_some();
    let (world_gathering, world_crafting) = (gathering, crafting);
    let needs: Vec<(String, i32)> = NEEDS
        .iter()
        .map(|need| ((*need).to_owned(), econ::need(world, actor, need)))
        .collect();
    let mut state = world.resource_mut::<LiveState>();
    let ledger = state.ledgers.entry(unit).or_insert_with(|| DayLedger {
        summary: DaySummary {
            day,
            ..Default::default()
        },
        coins,
        pack: carried.clone(),
        gathering,
        crafting,
    });
    let (gathering, crafting) = (gathering || ledger.gathering, crafting || ledger.crafting);
    ledger.gathering = world_gathering;
    ledger.crafting = world_crafting;
    if coins > ledger.coins {
        ledger.summary.earned += coins - ledger.coins;
    } else {
        ledger.summary.spent += ledger.coins - coins;
    }
    ledger.coins = coins;
    // Goods are booked by the actions themselves (`booked`), not from the
    // difference of the pack.
    let _ = (gathering, crafting);
    ledger.pack = carried;
    for (need, value) in needs {
        let low = ledger.summary.low.entry(need).or_insert(value);
        *low = (*low).min(value);
    }
}

/// Books a trade of a played character in its day summary: what it bought
/// or sold and for how many coins. Called by `market::record_trade`.
pub(super) fn trade_booked(
    world: &mut World,
    buyer: Entity,
    seller: Entity,
    kind: &str,
    count: u32,
    coins: u32,
) {
    if spec(world).is_none() || count == 0 {
        return;
    }
    for (actor, bought) in [(buyer, true), (seller, false)] {
        if !is_player(world, actor) {
            continue;
        }
        let Some(unit) = world.get::<SimId>(actor).copied() else {
            continue;
        };
        let mut state = world.resource_mut::<LiveState>();
        let Some(ledger) = state.ledgers.get_mut(&unit) else {
            continue;
        };
        let summary = &mut ledger.summary;
        let (goods, total) = if bought {
            (&mut summary.bought, &mut summary.bought_coins)
        } else {
            (&mut summary.sold, &mut summary.sold_coins)
        };
        *goods.entry(kind.to_owned()).or_default() += count;
        *total = total.saturating_add(coins);
    }
}

/// Book of the day summary a unit of goods goes into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Book {
    Gathered,
    Made,
    Received,
}

/// Books goods a played character got by an action: a gathering, a craft, a
/// harvest, a purchase or a gift. Called where the goods are handed over.
pub(super) fn booked(world: &mut World, unit: SimId, book: Book, kind: &str, count: u32) {
    if spec(world).is_none() || count == 0 {
        return;
    }
    if !entity(world, unit).is_some_and(|actor| is_player(world, actor)) {
        return;
    }
    let mut state = world.resource_mut::<LiveState>();
    let Some(ledger) = state.ledgers.get_mut(&unit) else {
        return;
    };
    let entry = match book {
        Book::Gathered => &mut ledger.summary.gathered,
        Book::Made => &mut ledger.summary.made,
        Book::Received => &mut ledger.summary.received,
    };
    *entry.entry(kind.to_owned()).or_default() += count;
}

/// The site of a homestead upgrade, once per tick: deliveries from the pack
/// while the owner stands at the door, the owner's work, and completion. The
/// hired builder and friends work at the door in stages ([`jobs`]).
fn work_on_upgrade(world: &mut World, spec: &LiveSpec, unit: SimId, actor: Entity) {
    let Some(upgrade) = world.resource::<LiveState>().upgrades.get(&unit).cloned() else {
        return;
    };
    let Some(tier) = spec.homestead.iter().find(|tier| tier.key == upgrade.key) else {
        cancel_upgrade(world, unit, actor);
        return;
    };
    let now = world.resource::<SimClock>().tick;
    let near = at_door(world, unit, actor).is_ok()
        && world
            .get::<Villager>(actor)
            .is_some_and(|person| person.resting_until <= now);
    if near && !upgrade.supplied() {
        deliver_from_pack(world, unit, actor, None);
    }
    let mut upgrade = world.resource::<LiveState>().upgrades[&unit].clone();
    if !upgrade.supplied() {
        return;
    }
    if near && upgrade.remaining_ticks > 0 {
        upgrade.remaining_ticks -= 1;
        world
            .resource_mut::<LiveState>()
            .upgrades
            .insert(unit, upgrade.clone());
    }
    if upgrade.remaining_ticks == 0 {
        finish_upgrade(world, unit, actor, tier);
    }
}

/// Why a repeat order stopped: a stable code and a sentence.
fn stop_repeat(world: &mut World, unit: SimId, order: &RepeatOrder, code: &str, detail: &str) {
    world.resource_mut::<LiveState>().repeats.remove(&unit);
    record_effect(
        world,
        Some(unit),
        "VillageRepeatStopped",
        &json!({"reason":code,"detail":detail,"done":order.done,"left":order.remaining,
            "recipe":order.recipe,"task":order.task}),
    );
}

/// The bed of a plot the garden verb `task` can be done on now.
fn garden_bed(world: &World, plot: Entity, task: &str) -> Option<u32> {
    let estate = world.get::<super::homestead::HomesteadPlot>(plot)?;
    let now = world.resource::<SimClock>().tick;
    estate
        .beds
        .iter()
        .position(|bed| match (task, bed) {
            ("plant", None) => true,
            ("water_crop", Some(crop)) => crop.watered_at.is_none(),
            ("harvest", Some(crop)) => crop.ready_at.is_some_and(|ready| ready <= now),
            _ => false,
        })
        .map(|bed| bed as u32)
}

fn repeat(world: &mut World, unit: SimId, actor: Entity) {
    let Some(order) = world.resource::<LiveState>().repeats.get(&unit).cloned() else {
        return;
    };
    let now = world.resource::<SimClock>().tick;
    let garden = !order.task.is_empty();
    // A gathering order walks to its patch itself; any other walk is the
    // player's and the order waits for it.
    let to_patch = order.recipe.is_empty()
        && !garden
        && world.get::<MoveIntent>(actor).is_some_and(|walk| {
            entity(world, order.target)
                .and_then(|patch| world.get::<SimPos>(patch))
                .is_some_and(|patch| walk.goal.dist2(*patch) <= 2_250_000)
        });
    let idle = world.get::<Gathering>(actor).is_none()
        && world.get::<Crafting>(actor).is_none()
        && (garden || to_patch || world.get::<MoveIntent>(actor).is_none())
        && world
            .get::<Villager>(actor)
            .is_some_and(|person| person.resting_until <= now);
    if !idle {
        return;
    }
    let critical = super::policy::active(world).map_or_else(
        || super::policy::IslandAiPolicy::default().critical_need,
        |policy| policy.critical_need,
    );
    if let Some(need) = NEEDS
        .iter()
        .find(|need| econ::need(world, actor, need) < critical)
    {
        stop_repeat(
            world,
            unit,
            &order,
            "need_critical",
            &format!("{need} is critical"),
        );
        return;
    }
    let Some(target) = entity(world, order.target) else {
        stop_repeat(world, unit, &order, "target_gone", "the target is gone");
        return;
    };
    let started: Result<(), (&str, String)> = if !order.recipe.is_empty() {
        use npc_framework::crafting::CraftError;
        match npc_framework::crafting::can_craft(world, actor, &order.recipe, Some(order.target)) {
            Err(
                error @ (CraftError::Missing { .. }
                | CraftError::MissingInput { .. }
                | CraftError::MissingTool { .. }),
            ) => Err(("inputs_gone", error.to_string())),
            Err(error) => Err(("target_gone", error.to_string())),
            Ok(_) => {
                let outputs: Vec<Item> = world
                    .resource::<npc_framework::crafting::RecipeRegistry>()
                    .recipes
                    .get(&order.recipe)
                    .map(|recipe| {
                        recipe
                            .outputs
                            .iter()
                            .map(|output| Item::new(&output.kind).stack(output.count))
                            .collect()
                    })
                    .unwrap_or_default();
                if let Err(reason) = econ::fit(world, actor, &outputs) {
                    Err(("pack_full", reason))
                } else {
                    npc_framework::crafting::begin_craft(
                        world,
                        actor,
                        &order.recipe,
                        Some(order.target),
                        npc_framework::crafting::CraftSource::Command,
                        None,
                    )
                    .map_err(|error| ("target_gone", error.to_string()))
                }
            }
        }
    } else if garden {
        match garden_bed(world, target, &order.task) {
            None => Err(("target_gone", format!("no bed needs {}", order.task))),
            Some(bed) => {
                let place = super::homestead::bed_position(world, target, bed);
                if let Some(place) = place
                    && !super::ai::travel(world, actor, place)
                {
                    // Walking to the bed: the next round works it.
                    return;
                }
                let act = super::homestead::HomesteadAct {
                    bed,
                    ..super::homestead::HomesteadAct::new(unit, &order.task, order.target)
                };
                match act.check(world) {
                    Err(reason) if reason.contains("seed") || reason.contains("carry") => {
                        Err(("inputs_gone", reason))
                    }
                    Err(reason) if reason.contains("room") || reason.contains("carry more") => {
                        Err(("pack_full", reason))
                    }
                    Err(reason) => {
                        // The crop needs water or seed that is not in the pack.
                        let inputs = (order.task == "water_crop"
                            && econ::count(world, actor, super::roles::roles(world).drink) == 0)
                            || (order.task == "plant"
                                && econ::count(world, actor, super::roles::roles(world).grain)
                                    == 0);
                        Err((if inputs { "inputs_gone" } else { "target_gone" }, reason))
                    }
                    Ok(()) => {
                        let result = act.perform(world);
                        if result.is_ok() {
                            homestead_acted(world, unit, &order.task);
                        }
                        result.map_err(|reason| ("target_gone", reason))
                    }
                }
            }
        }
    } else {
        let kind = world
            .get::<super::ResourcePatch>(target)
            .filter(|patch| patch.remaining > 0)
            .map(|patch| patch.kind.clone());
        match kind {
            None => Err(("target_gone", "the resource is used up".to_owned())),
            Some(kind) => {
                if let Err(reason) = econ::fit(world, actor, &[Item::new(&kind)]) {
                    Err(("pack_full", reason))
                } else if !econ::near(world, actor, target) {
                    // Out of reach: walk there (the rule of a single
                    // gathering); the next round gathers.
                    super::ai::approach_work(world, actor, target);
                    if world.get::<MoveIntent>(actor).is_none() {
                        Err((
                            "target_gone",
                            "the way to the resource is blocked".to_owned(),
                        ))
                    } else {
                        return;
                    }
                } else {
                    let mut again = super::VillageAct::new(unit, "gather");
                    again.target = Some(order.target);
                    again
                        .perform(world)
                        .map_err(|reason| ("target_gone", reason))
                }
            }
        }
    };
    match started {
        Ok(()) => {
            let finished = {
                let mut state = world.resource_mut::<LiveState>();
                let order = state.repeats.get_mut(&unit).expect("order");
                order.remaining -= 1;
                order.done += 1;
                order.remaining == 0
            };
            if finished {
                let order = world.resource::<LiveState>().repeats[&unit].clone();
                stop_repeat(
                    world,
                    unit,
                    &order,
                    "count_done",
                    "every repetition is done",
                );
            }
        }
        Err((code, reason)) => stop_repeat(world, unit, &order, code, &reason),
    }
}

/// The played character a resident may notice: arrived, alive, and not the
/// resident itself. One index lookup, no scan.
fn noticed_player(world: &World, actor: Entity) -> Option<(SimId, Entity, SimPos)> {
    let id = world.get_resource::<super::VillageState>()?.played;
    let player = entity(world, id)?;
    if player == actor || !is_player(world, player) || !living(world, player) {
        return None;
    }
    Some((id, player, *world.get::<SimPos>(player)?))
}

/// `life.player_within`: the played character is alive and within
/// `reach_mm` of `actor` (squared integer distance).
pub fn player_within(world: &World, actor: Entity, reach_mm: i64) -> bool {
    let (Some((_, _, at)), Some(here)) = (noticed_player(world, actor), world.get::<SimPos>(actor))
    else {
        return false;
    };
    here.dist2(at) <= reach_mm.saturating_mul(reach_mm)
}

/// Whether the played character sold a good `resident` makes today, while it
/// stands in the resident's settlement.
fn busy_at_trade(world: &World, player: SimId, at: SimPos, resident: Entity, day: u64) -> bool {
    let Some(person) = world.get::<Villager>(resident) else {
        return false;
    };
    let home = super::frontier::settlements::member(world, resident);
    if home.is_none() || super::frontier::settlements::nearest(world, at) != home {
        return false;
    }
    let outputs = catalog::outputs_for(world, person.role);
    world
        .resource::<LiveState>()
        .ledgers
        .get(&player)
        .filter(|ledger| ledger.summary.day == day)
        .is_some_and(|ledger| {
            ledger
                .summary
                .sold
                .iter()
                .any(|(kind, count)| *count > 0 && outputs.contains(&kind.as_str()))
        })
}

/// `life.greet player`: a free resident turns to the played character and
/// speaks first, once a day each. The line is the bond tier's greeting, or a
/// word about its trade when the played character sold one of its goods in
/// its settlement today. The bond gains half a talk.
pub(crate) fn greet_player(world: &mut World, resident: Entity) -> Result<(), &'static str> {
    let Some(spec) = spec(world).cloned() else {
        return Err("this world keeps no bonds with a played character");
    };
    let Some((unit, player, at)) = noticed_player(world, resident) else {
        return Err("no played character is about");
    };
    let Some(here) = world.get::<SimPos>(resident).copied() else {
        return Err("no played character is about");
    };
    if here.dist2(at) > GREETING_REACH_MM.pow(2) {
        return Err("the played character is too far away to greet");
    }
    // A resident at home greets a passer-by from the door; a played
    // character asleep, or indoors where the resident is not, is not greeted.
    let indoors = super::indoors::inside_entity(world, player);
    if super::indoors::asleep(world, player)
        || (indoors.is_some() && indoors != super::indoors::inside_entity(world, resident))
    {
        return Err("the played character cannot be greeted from here");
    }
    if world.get::<colony::Chatting>(resident).is_some() {
        return Err("in a meeting");
    }
    let Some(id) = world.get::<SimId>(resident).copied() else {
        return Err("no played character is about");
    };
    let day = super::calendar(world).0;
    let already = world
        .resource::<LiveState>()
        .bonds
        .get(&unit)
        .and_then(|bonds| bonds.get(&id))
        .is_some_and(|bond| bond.greeted_day == Some(day));
    if already {
        return Err("already greeted the played character today");
    }
    let trade = busy_at_trade(world, unit, at, resident, day);
    let line = if trade {
        let name = world
            .get::<Villager>(player)
            .map(|person| person.name.clone())
            .unwrap_or_default();
        format!("You've been busy at my trade, {name}.")
    } else {
        let (tier, _) = tier_of(world, score(world, unit, id));
        spec.greetings
            .get(tier.min(spec.greetings.len().saturating_sub(1)))
            .cloned()
            .unwrap_or_else(|| "Good day.".to_owned())
    };
    if let Some(mut facing) = world.get_mut::<Facing>(resident) {
        let towards = Facing::towards(here, at);
        if *facing != towards {
            *facing = towards;
        }
    }
    world.entity_mut(resident).remove::<MoveIntent>();
    if super::speech::validate_text(&line).is_ok() {
        // Addressed: the played character hears it, nobody is scanned for.
        super::speech::speak(world, id, Some(unit), &line);
    }
    {
        let mut state = bond_mut(world, unit, id);
        if let Some(bond) = state
            .bonds
            .get_mut(&unit)
            .and_then(|bonds| bonds.get_mut(&id))
        {
            bond.greeted_day = Some(day);
        }
    }
    change_bond(world, unit, id, spec.talk_points / 2, "greeting");
    record_effect(
        world,
        Some(id),
        "VillageGreeting",
        &json!({"to":unit,"text":line,"trade":trade}),
    );
    Ok(())
}

/// A resident resting at home in the day, free to attend, gets up for the
/// gathering; a night's sleep is not cut short.
fn wake_for_gathering(world: &mut World) {
    let now = world.resource::<SimClock>().tick;
    let Some(mut query) = world.try_query::<(Entity, &SimId, &Villager)>() else {
        return;
    };
    let mut resting: Vec<(SimId, Entity)> = query
        .iter(world)
        .filter(|(_, _, person)| person.resting_until > now)
        .map(|(actor, id, _)| (*id, actor))
        .collect();
    resting.sort_unstable_by_key(|(id, _)| *id);
    for (id, actor) in resting {
        if !attending(world, actor)
            || super::resident::in_block(world, actor, super::resident::BlockKind::Sleep)
            || (super::living::cold(world) && !super::living::has_coat(world, actor))
        {
            continue;
        }
        if let Some(mut person) = world.get_mut::<Villager>(actor) {
            person.resting_until = now;
        }
        record_effect(world, Some(id), "VillageGatheringWoke", &json!({}));
    }
}

/// Attendance of the gathering that is on, and its end.
fn gather(world: &mut World, played: &[(SimId, Entity)], day: u64) {
    let on = gathering_now(world).cloned();
    if on.is_some() {
        wake_for_gathering(world);
    }
    let running = world.resource::<LiveState>().gathering.clone();
    match (on, running) {
        (Some(spec), running) => {
            if running
                .as_ref()
                .is_none_or(|held| held.key != spec.key || held.day != day)
            {
                world.resource_mut::<LiveState>().gathering = Some(HeldGathering {
                    key: spec.key.clone(),
                    day,
                    attended: Vec::new(),
                });
                record_effect(
                    world,
                    None,
                    "VillageGatheringBegan",
                    &json!({"key":spec.key,"name":spec.name,"day":day}),
                );
            }
            let centres: Vec<(SimId, SimPos)> = super::frontier::settlements::all(world)
                .into_iter()
                .filter_map(|(id, _)| {
                    Some((
                        id,
                        super::frontier::settlements::definition(world, id)?.center,
                    ))
                })
                .collect();
            let Some(mut query) = world.try_query::<(Entity, &SimId, &SimPos, &Villager)>() else {
                return;
            };
            let mut here: Vec<(SimId, Entity)> = query
                .iter(world)
                .filter(|(actor, _, pos, _)| {
                    living(world, *actor)
                        && centres
                            .iter()
                            .any(|(_, centre)| pos.dist2(*centre) <= GATHERING_REACH_MM.pow(2))
                })
                .map(|(actor, id, _, _)| (*id, actor))
                .collect();
            here.sort_unstable_by_key(|(id, _)| *id);
            for (id, actor) in here {
                let known = world
                    .resource::<LiveState>()
                    .gathering
                    .as_ref()
                    .is_some_and(|held| held.attended.contains(&id));
                if known {
                    continue;
                }
                if let Some(mut social) = world.get_mut::<SocialLife>(actor) {
                    social.morale = (social.morale + spec.morale).min(100_000);
                }
                let mut state = world.resource_mut::<LiveState>();
                if let Some(held) = state.gathering.as_mut() {
                    held.attended.push(id);
                    held.attended.sort_unstable();
                }
                if played.iter().any(|(unit, _)| *unit == id) {
                    *state
                        .counters
                        .entry(id)
                        .or_default()
                        .entry("gathering".into())
                        .or_default() += 1;
                }
            }
        }
        (None, Some(held)) => {
            let name = spec(world)
                .and_then(|spec| spec.gatherings.iter().find(|g| g.key == held.key))
                .map_or_else(String::new, |gathering| gathering.name.clone());
            record_effect(
                world,
                None,
                "VillageGathering",
                &json!({"key":held.key,"name":name,"day":held.day,"attended":held.attended.len()}),
            );
            let mut state = world.resource_mut::<LiveState>();
            state.gathering = None;
            state.held.push(held);
            if state.held.len() > 16 {
                state.held.remove(0);
            }
        }
        (None, None) => {}
    }
}

/// The daily pass: summaries, neglect, invitations and the end of mourning.
fn morning(world: &mut World, spec: &LiveSpec, played: &[(SimId, Entity)], day: u64) {
    for (unit, actor) in played {
        let coins = world.get::<Wallet>(*actor).map_or(0, |wallet| wallet.coins);
        let carried = pack(world, *actor);
        let summary = {
            let mut state = world.resource_mut::<LiveState>();
            let ledger = state.ledgers.remove(unit);
            state.ledgers.insert(
                *unit,
                DayLedger {
                    summary: DaySummary {
                        day,
                        ..Default::default()
                    },
                    coins,
                    pack: carried,
                    gathering: false,
                    crafting: false,
                },
            );
            ledger.map(|ledger| ledger.summary)
        };
        if let Some(summary) = summary {
            record_effect(
                world,
                Some(*unit),
                "VillageDaySummary",
                &serde_json::to_value(&summary).unwrap_or_default(),
            );
            world
                .resource_mut::<LiveState>()
                .summaries
                .insert(*unit, summary);
        }
    }
    // Neglect: a bond nobody tends fades; a close one earns an invitation to
    // the next gathering.
    let invite_day = spec.gatherings.iter().any(|gathering| {
        let (_, season, _) = super::calendar(world);
        season_key(season) == gathering.season
            && gathering.day.min(super::days_per_season(world))
                == day % super::days_per_season(world) + 1
    });
    let mut invitations = Vec::new();
    {
        let mut state = world.resource_mut::<LiveState>();
        for (player, bonds) in &mut state.bonds {
            for (resident, bond) in bonds.iter_mut() {
                if day > bond.last_contact_day + spec.neglect_after_days {
                    bond.score = (bond.score - spec.neglect_points_per_day).max(0);
                }
                let invites = spec
                    .tiers
                    .iter()
                    .rev()
                    .find(|tier| bond.score >= tier.score)
                    .is_some_and(|tier| tier.invites);
                if invite_day && invites && !bond.invited {
                    bond.invited = true;
                    invitations.push((*player, *resident));
                } else if !invite_day {
                    bond.invited = false;
                }
            }
        }
        state.mourning.retain(|grief| grief.until_day > day);
    }
    for (player, resident) in invitations {
        record_effect(
            world,
            Some(resident),
            "VillageInvitation",
            &json!({"to":player,"day":day}),
        );
    }
    // Among residents an old fondness fades a little every day too.
    if spec.resident_neglect_points_per_day > 0 {
        let Some(mut query) = world.try_query::<(Entity, &SimId, &SocialLife)>() else {
            return;
        };
        let people: Vec<Entity> = {
            let mut people: Vec<(SimId, Entity)> = query
                .iter(world)
                .map(|(actor, id, _)| (*id, actor))
                .collect();
            people.sort_unstable_by_key(|(id, _)| *id);
            people.into_iter().map(|(_, actor)| actor).collect()
        };
        for actor in people {
            if let Some(mut social) = world.get_mut::<SocialLife>(actor) {
                for affinity in social.affinity.values_mut() {
                    *affinity = (*affinity - spec.resident_neglect_points_per_day).max(0);
                }
            }
        }
    }
}

/// Tick the played character sleeps until; `None` when awake. A host may run
/// the simulation faster until then.
pub fn asleep_until(world: &World, unit: SimId) -> Option<u64> {
    let actor = entity(world, unit)?;
    let until = *world.get_resource::<LiveState>()?.sleeping.get(&unit)?;
    super::indoors::asleep(world, actor).then_some(until)
}

/// One relationship of a resident or a played character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RelationView {
    pub other: SimId,
    pub name: String,
    /// 0 to 100000.
    pub score: i32,
    pub tier: String,
    pub tier_index: u32,
    /// `true` when the other is a played character or this is one.
    pub with_player: bool,
    /// Percent off when the played character buys from the resident.
    pub discount_percent: u32,
    pub invited: bool,
    /// The partner or a member of the same household.
    pub household: bool,
    /// Between a played character and a resident: whether the played
    /// character has learned the resident's gift tastes (after a first gift,
    /// or once the bond reaches a tier that helps).
    pub tastes_known: bool,
    /// The resident's `(liked, disliked)` gift, once learned.
    pub tastes: Option<(String, String)>,
}

/// The relationships of a resident or of a played character, strongest first.
pub fn relationships(world: &World, of: SimId) -> Vec<RelationView> {
    let Some(spec) = spec(world) else {
        return Vec::new();
    };
    let Some(state) = world.get_resource::<LiveState>() else {
        return Vec::new();
    };
    let name = |id: SimId| {
        entity(world, id)
            .and_then(|actor| world.get::<Villager>(actor))
            .map_or_else(String::new, |person| person.name.clone())
    };
    let home = colony::home_of(world, of);
    let view = |other: SimId,
                score: i32,
                with_player: bool,
                invited: bool,
                resident: Option<(SimId, bool)>| {
        let (tier_index, tier) = tier_of(world, score);
        let tastes_known = resident.is_some_and(|(_, gifted)| {
            gifted || spec.tiers.get(tier_index).is_some_and(|tier| tier.helps)
        });
        RelationView {
            tastes_known,
            tastes: resident
                .filter(|_| tastes_known)
                .and_then(|(resident, _)| tastes(world, resident)),
            other,
            name: name(other),
            score,
            tier,
            tier_index: tier_index as u32,
            with_player,
            discount_percent: if with_player {
                spec.tiers
                    .get(tier_index)
                    .map_or(0, |tier| tier.discount_percent)
            } else {
                0
            },
            invited,
            household: home.is_some() && colony::home_of(world, other) == home,
        }
    };
    let mut out: Vec<RelationView> = Vec::new();
    if let Some(bonds) = state.bonds.get(&of) {
        out.extend(bonds.iter().map(|(resident, bond)| {
            view(
                *resident,
                bond.score,
                true,
                bond.invited,
                Some((*resident, bond.gifted_day.is_some())),
            )
        }));
    }
    for (player, bonds) in &state.bonds {
        if let Some(bond) = bonds.get(&of) {
            out.push(view(
                *player,
                bond.score,
                true,
                bond.invited,
                Some((of, bond.gifted_day.is_some())),
            ));
        }
    }
    if let Some(social) = entity(world, of).and_then(|actor| world.get::<SocialLife>(actor)) {
        out.extend(
            social
                .affinity
                .iter()
                .filter(|(other, score)| **score > 0 && entity(world, **other).is_some())
                .map(|(other, score)| view(*other, *score, false, false, None)),
        );
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.other.cmp(&b.other)));
    out
}

/// One goal of the chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoalView {
    pub key: String,
    pub title: String,
    pub progress: u32,
    pub count: u32,
    pub done: bool,
    /// What reaching it unlocks: names of homestead tiers.
    pub reward: Vec<String>,
}

/// The goal chain of a played character.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GoalsView {
    pub done: Vec<GoalView>,
    pub current: Option<GoalView>,
    pub next: Option<GoalView>,
    /// Number of the current goal, from 1; `total + 1` when all are done.
    pub step: u32,
    /// Goals in the chain.
    pub total: u32,
}

/// The goal chain: goals are worked through in the order of the data.
pub fn goals(world: &World, unit: SimId) -> GoalsView {
    let Some(spec) = spec(world) else {
        return GoalsView::default();
    };
    let mut out = GoalsView::default();
    for goal in &spec.goals {
        let (progress, done) = goal_reached(world, unit, goal);
        let view = GoalView {
            key: goal.key.clone(),
            title: goal.title.clone(),
            progress,
            count: goal.count,
            done: done && out.current.is_none(),
            reward: spec
                .homestead
                .iter()
                .filter(|tier| tier.goal == goal.key)
                .map(|tier| tier.name.clone())
                .collect(),
        };
        if view.done {
            out.done.push(view);
        } else if out.current.is_none() {
            out.current = Some(view);
        } else if out.next.is_none() {
            out.next = Some(view);
        }
    }
    out.total = spec.goals.len() as u32;
    out.step = out.done.len() as u32 + 1;
    out
}

/// One homestead tier and whether it can be built now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TierView {
    pub key: String,
    pub name: String,
    pub done: bool,
    pub available: bool,
    /// Why it cannot be started; empty when it can or is done.
    pub reason: String,
    pub cost: Vec<(String, u32)>,
    pub work_ticks: u32,
    /// Work left of the upgrade under way; `None` when it is not this one.
    pub remaining_ticks: Option<u64>,
    pub beds: u32,
    pub plots: u32,
    pub gather_percent: u32,
    /// Chest units the tier adds, and the catalogue home the cottage grows into.
    pub storage: u32,
    pub building: String,
}

/// The homestead tiers of a played character, in the order of the data.
pub fn homestead_tiers(world: &World, unit: SimId) -> Vec<TierView> {
    let Some(spec) = spec(world) else {
        return Vec::new();
    };
    let state = world.resource::<LiveState>();
    let done = state.tiers.get(&unit).cloned().unwrap_or_default();
    spec.homestead
        .iter()
        .map(|tier| {
            let finished = done.contains(&tier.key);
            let under_way = state
                .upgrades
                .get(&unit)
                .filter(|upgrade| upgrade.key == tier.key)
                .map(|upgrade| upgrade.remaining_ticks);
            let reason = if finished || under_way.is_some() {
                String::new()
            } else {
                tier_blocked(world, spec, unit, tier)
            };
            TierView {
                key: tier.key.clone(),
                name: tier.name.clone(),
                done: finished,
                available: !finished && under_way.is_none() && reason.is_empty(),
                reason,
                cost: tier
                    .cost
                    .iter()
                    .map(|stack| (stack.good.clone(), stack.count))
                    .collect(),
                work_ticks: tier.work_ticks,
                remaining_ticks: under_way,
                beds: tier.beds,
                plots: tier.plots,
                gather_percent: tier.gather_percent,
                storage: tier.storage,
                building: tier.building.clone(),
            }
        })
        .collect()
}

/// One gathering of the calendar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GatheringView {
    pub key: String,
    pub name: String,
    pub season: String,
    /// Day of the season, from 1.
    pub day: u64,
    pub from_permille: u32,
    pub to_permille: u32,
    /// Calendar day of the next one (today while it has not ended).
    pub next_day: u64,
    pub active: bool,
    /// Morale each attendant gains.
    pub morale: i32,
    /// How many came the last time; `None` before the first.
    pub last_attendance: Option<u32>,
}

/// The calendar of gatherings, soonest first.
pub fn gatherings(world: &World) -> Vec<GatheringView> {
    let Some(spec) = spec(world) else {
        return Vec::new();
    };
    let state = world.resource::<LiveState>();
    let (today, _, _) = super::calendar(world);
    let season_days = super::days_per_season(world);
    let year = season_days * 4;
    let at = super::resident::day_permille(world);
    let mut out: Vec<GatheringView> = spec
        .gatherings
        .iter()
        .map(|gathering| {
            let season = ["spring", "summer", "autumn", "winter"]
                .iter()
                .position(|key| *key == gathering.season)
                .unwrap_or(0) as u64;
            let of_year = season * season_days + gathering.day.min(season_days) - 1;
            let mut next_day = today - today % year + of_year;
            if next_day < today || (next_day == today && at >= gathering.to_permille as i32) {
                next_day += year;
            }
            GatheringView {
                key: gathering.key.clone(),
                name: gathering.name.clone(),
                season: gathering.season.clone(),
                day: gathering.day.min(season_days),
                from_permille: gathering.from_permille,
                to_permille: gathering.to_permille,
                next_day,
                active: state
                    .gathering
                    .as_ref()
                    .is_some_and(|held| held.key == gathering.key),
                morale: gathering.morale,
                last_attendance: state
                    .held
                    .iter()
                    .rev()
                    .find(|held| held.key == gathering.key)
                    .map(|held| held.attended.len() as u32),
            }
        })
        .collect();
    out.sort_by(|a, b| a.next_day.cmp(&b.next_day).then(a.key.cmp(&b.key)));
    out
}

/// The last finished day of a played character; `None` before its first
/// morning.
pub fn day_summary(world: &World, unit: SimId) -> Option<DaySummary> {
    world
        .get_resource::<LiveState>()?
        .summaries
        .get(&unit)
        .cloned()
}

/// The day so far.
pub fn day_so_far(world: &World, unit: SimId) -> Option<DaySummary> {
    world
        .get_resource::<LiveState>()?
        .ledgers
        .get(&unit)
        .map(|ledger| ledger.summary.clone())
}

/// The repeat order of a played character: `(resource, left, done)`.
pub fn repeat_order(world: &World, unit: SimId) -> Option<(SimId, u32, u32)> {
    world
        .get_resource::<LiveState>()?
        .repeats
        .get(&unit)
        .map(|order| (order.target, order.remaining, order.done))
}

/// Residents in mourning: `(mourner, for whom, until day)`.
pub fn mourning(world: &World) -> Vec<(SimId, String, u64)> {
    world
        .get_resource::<LiveState>()
        .map(|state| {
            state
                .mourning
                .iter()
                .map(|grief| (grief.mourner, grief.name.clone(), grief.until_day))
                .collect()
        })
        .unwrap_or_default()
}

/// Checks the state against the world: everybody named still makes sense.
pub fn validate(world: &World) -> Result<(), String> {
    let Some(state) = world.get_resource::<LiveState>() else {
        return Ok(());
    };
    let known = |set: BTreeSet<&String>| {
        spec(world).is_none_or(|spec| {
            set.into_iter()
                .all(|key| spec.homestead.iter().any(|tier| tier.key == *key))
        })
    };
    if !known(state.tiers.values().flatten().collect())
        || state
            .bonds
            .values()
            .flat_map(BTreeMap::values)
            .any(|bond| !(0..=100_000).contains(&bond.score))
        || state.held.len() > 16
    {
        return Err("Live-mode state is inconsistent".into());
    }
    Ok(())
}

/// The homestead chest of a played character.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ChestView {
    pub capacity: u32,
    pub used: u32,
    pub goods: Vec<(String, u32)>,
}

/// The chest of a played character; `None` before a storage tier stands.
pub fn chest(world: &World, unit: SimId) -> Option<ChestView> {
    let capacity = chest_capacity(world, unit);
    if capacity == 0 {
        return None;
    }
    let goods: Vec<(String, u32)> = world
        .get_resource::<LiveState>()?
        .chests
        .get(&unit)
        .map(|chest| {
            chest
                .iter()
                .map(|(kind, count)| (kind.clone(), *count))
                .collect()
        })
        .unwrap_or_default();
    Some(ChestView {
        capacity,
        used: goods.iter().map(|(_, count)| count).sum(),
        goods,
    })
}

/// The homestead upgrade site of a played character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UpgradeSiteView {
    pub tier: String,
    pub name: String,
    pub cost: Vec<(String, u32)>,
    pub delivered: Vec<(String, u32)>,
    /// Units ordered from residents and not yet brought.
    pub ordered: Vec<(String, u32)>,
    pub escrow: u32,
    pub supplied: bool,
    pub work_ticks: u32,
    pub remaining_ticks: u64,
    pub builder: Option<SimId>,
    /// Coins an order of one unit of each missing good would cost now.
    pub order_prices: Vec<(String, u32)>,
    /// Coins hiring a builder for the rest would cost now.
    pub hire_cost: u32,
    /// Who is on the way to the homestead or working there now, by id: a
    /// hauler bringing an order, the hired builder, a helping friend.
    pub workers: Vec<UpgradeWorkerView>,
    /// Tick the last order was placed; `None` before the first.
    pub ordered_at: Option<u64>,
}

/// The stage of work under way at `unit`'s homestead upgrade: `(ticks left,
/// ticks in the stage)`, for a 0..1 bar on the house; the hired builder's
/// stage first, else a friend's. `None` while nobody works at the door (the
/// owner's own work is the whole upgrade's `remaining_ticks`).
pub fn upgrade_stage(world: &World, unit: SimId) -> Option<(u64, u64)> {
    let now = world.resource::<SimClock>().tick;
    let mut query = world.try_query::<(&SimId, &UpgradeWork)>()?;
    let mut stages: Vec<(bool, SimId, UpgradeWork)> = query
        .iter(world)
        .filter(|(_, work)| work.owner == unit)
        .map(|(id, work)| (!work.hired, *id, *work))
        .collect();
    stages.sort_unstable_by_key(|(friend, id, _)| (*friend, *id));
    let (_, _, work) = stages.into_iter().next()?;
    let total = work.ends.saturating_sub(work.started).max(1);
    Some((work.ends.saturating_sub(now).min(total), total))
}

/// The upgrade under way of a played character.
pub fn upgrade_site(world: &World, unit: SimId) -> Option<UpgradeSiteView> {
    let spec = spec(world)?;
    let upgrade = world.get_resource::<LiveState>()?.upgrades.get(&unit)?;
    let tier = spec.homestead.iter().find(|tier| tier.key == upgrade.key)?;
    let list = |goods: &Goods| {
        goods
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>()
    };
    Some(UpgradeSiteView {
        tier: tier.key.clone(),
        name: tier.name.clone(),
        cost: list(&upgrade.cost),
        delivered: list(&upgrade.delivered),
        ordered: list(&upgrade.ordered),
        escrow: upgrade.escrow,
        supplied: upgrade.supplied(),
        work_ticks: tier.work_ticks,
        remaining_ticks: upgrade.remaining_ticks,
        builder: upgrade.builder,
        order_prices: upgrade
            .cost
            .keys()
            .filter(|kind| upgrade.missing(kind) > 0)
            .filter_map(|kind| Some((kind.clone(), order_total(world, spec, kind, 1)?)))
            .collect(),
        hire_cost: hire_cost(world, spec, upgrade.remaining_ticks),
        workers: jobs::workers(world, unit),
        ordered_at: upgrade.ordered_at,
    })
}

/// Garden plots a played character may own (living world): one, plus the
/// plots of its finished tiers. `None` outside live mode.
pub fn garden_allowance(world: &World, owner: SimId) -> Option<u32> {
    spec(world)?;
    Some(plot_allowance(world, owner))
}
