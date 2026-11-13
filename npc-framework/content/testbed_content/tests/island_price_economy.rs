//! Phase 1 of the island roadmap: an economy that answers to prices.
//! Every behaviour here is regional (`living_world`) only; the older profiles
//! are asserted unchanged where a shared code path was touched.
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::crafting::Durability;
use npc_framework::needs::{NeedRegistry, Needs};
use npc_framework::prelude::*;
use testbed_content::{
    TestbedIds,
    village::{
        self, ResourcePatch, Role, VillageAct, VillageState, Villager, Vitality, Wallet, catalog,
        colony, economy,
        frontier::{
            labor::{self, Employment},
            settlements,
        },
        goap, living,
        market::{Commodity, MarketState},
    },
};

fn scene() -> App {
    testbed_content::new_sim_app(7, "living_world")
}
fn person(app: &App, name: &str) -> Entity {
    let id = app.world().resource::<TestbedIds>().id(name);
    app.world().resource::<SimIndex>().get(id).unwrap()
}
fn money(app: &App) -> u64 {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .map(|entity| {
            u64::from(app.world().get::<Wallet>(*entity).map_or(0, |w| w.coins))
                + u64::from(
                    app.world()
                        .get::<village::CoinPurse>(*entity)
                        .map_or(0, |p| p.coins),
                )
        })
        .sum::<u64>()
        + u64::from(app.world().resource::<VillageState>().fund)
}
fn set_need(app: &mut App, actor: Entity, name: &str, value: i32) {
    let id = app.world().resource::<NeedRegistry>().id(name).unwrap();
    app.world_mut()
        .get_mut::<Needs>(actor)
        .unwrap()
        .get_mut(id)
        .value = value;
}
fn set_price(app: &mut App, kind: &str, coins: u32) {
    let mut market = app.world_mut().resource_mut::<MarketState>();
    let reference_milli = coins * 1000;
    market
        .commodities
        .entry(kind.into())
        .and_modify(|commodity| commodity.reference_milli = reference_milli)
        .or_insert(Commodity {
            reference_milli,
            pending_units: 0,
            pending_turnover: 0,
            total_units: 0,
            total_turnover: 0,
            history: Vec::new(),
        });
}
fn residents(app: &App) -> Vec<Entity> {
    app.world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|entity| app.world().get::<labor::ResidentProfile>(*entity).is_some())
        .collect()
}
fn tools(app: &App, actor: Entity, kind: &str) -> Vec<Entity> {
    economy::free_items(app.world(), actor)
        .into_iter()
        .filter(|(_, item)| item.kind == kind)
        .filter_map(|(id, _)| app.world().resource::<SimIndex>().get(id))
        .collect()
}

#[test]
fn levy_is_proportional_keeps_profit_and_pays_a_destitution_floor() {
    let mut app = scene();
    let total = money(&app);
    let rich = person(&app, "Mira");
    let poor = person(&app, "Jun");
    // Move real coins: ten neighbours each pay Mira 100, Jun pays her 195.
    let donors: Vec<_> = residents(&app)
        .into_iter()
        .filter(|actor| *actor != rich && *actor != poor)
        .take(10)
        .collect();
    for donor in donors {
        app.world_mut().get_mut::<Wallet>(donor).unwrap().coins -= 100;
        app.world_mut().get_mut::<Wallet>(rich).unwrap().coins += 100;
    }
    app.world_mut().get_mut::<Wallet>(poor).unwrap().coins -= 195;
    app.world_mut().get_mut::<Wallet>(rich).unwrap().coins += 195;
    assert_eq!(app.world().get::<Wallet>(rich).unwrap().coins, 1395);
    assert_eq!(money(&app), total);
    // The daily settlement is due on the next ordinary tick.
    let day = village::calendar(app.world()).0;
    app.world_mut()
        .resource_mut::<VillageState>()
        .last_dividend_day = day - 1;
    run_ticks(app.world_mut(), 1);
    assert_eq!(
        app.world().resource::<VillageState>().last_dividend_day,
        day
    );
    // 10% of the 1195 above the 200 threshold; the old cap left exactly 200.
    let levy = (1395 - village::LEVY_THRESHOLD) * village::LEVY_PERCENT / 100;
    assert_eq!(levy, 119);
    assert_eq!(app.world().get::<Wallet>(rich).unwrap().coins, 1395 - levy);
    assert_eq!(
        app.world().get::<Wallet>(poor).unwrap().coins,
        village::DESTITUTION_FLOOR
    );
    // The fund keeps what the floor did not need; below its reserve it pays no dividend.
    assert_eq!(
        app.world().resource::<VillageState>().fund,
        levy - (village::DESTITUTION_FLOOR - 5)
    );
    assert_eq!(money(&app), total);
}

#[test]
fn fund_above_its_reserve_returns_as_an_equal_dividend_and_conserves_coins() {
    let mut app = scene();
    let people = residents(&app);
    let count = people.len() as u32;
    // A declared starting position: every resident has paid 50 into the fund.
    for actor in &people {
        app.world_mut().get_mut::<Wallet>(*actor).unwrap().coins -= 50;
    }
    app.world_mut().resource_mut::<VillageState>().fund += 50 * count;
    let total = money(&app);
    let day = village::calendar(app.world()).0;
    app.world_mut()
        .resource_mut::<VillageState>()
        .last_dividend_day = day - 1;
    npc_framework::kernel::journal::set_journal(app.world_mut(), true, false);
    run_ticks(app.world_mut(), 1);
    // The day's settlement also tops a sole inn's keeper up to the service
    // wage floor from the fund (ADR 0041): journaled, paid on top.
    let mut wages: std::collections::BTreeMap<u64, u32> = Default::default();
    for entry in &app.world().resource::<Journal>().entries {
        if let JournalKind::Effect {
            effect,
            applied: true,
        } = &entry.kind
            && effect.get("type").and_then(|kind| kind.as_str()) == Some("VillageServiceWage")
        {
            let field = |key: &str| effect.get(key).and_then(|value| value.as_u64());
            *wages.entry(field("keeper").expect("keeper")).or_default() +=
                field("paid").expect("paid") as u32;
        }
    }
    let share = 50 - village::FUND_RESERVE_PER_RESIDENT;
    for actor in &people {
        let id = u64::from(app.world().get::<SimId>(*actor).unwrap().0);
        assert_eq!(
            app.world().get::<Wallet>(*actor).unwrap().coins,
            150 + share + wages.get(&id).copied().unwrap_or(0)
        );
    }
    assert_eq!(
        app.world().resource::<VillageState>().fund,
        count * village::FUND_RESERVE_PER_RESIDENT - wages.values().sum::<u32>()
    );
    assert_eq!(money(&app), total);
}

#[test]
fn founding_gatherers_carry_durable_tools_and_tools_set_gathering_speed() {
    let mut app = scene();
    for (name, tool, good) in [
        ("Oak", "axe", "log"),
        ("Piper", "axe", "log"),
        ("Bram", "pickaxe", "ore"),
        ("Ada", catalog::HOE, catalog::GRAIN),
    ] {
        let actor = person(&app, name);
        let owned = tools(&app, actor, tool);
        assert_eq!(owned.len(), 1, "{name} should start with one {tool}");
        assert_eq!(
            app.world().get::<Durability>(owned[0]).map(|d| d.current),
            Some(catalog::TOOL_DURABILITY)
        );
        assert_eq!(catalog::gather_tool(good), Some(tool));
    }
    let oak = person(&app, "Oak");
    app.world_mut()
        .get_mut::<colony::SocialLife>(oak)
        .unwrap()
        .morale = 60_000;
    // 115 - 60000 * 25 / 100000 = 100: a neutral pace isolates the tool effect.
    assert_eq!(colony::work_pace_percent(app.world(), oak), 100);
    assert_eq!(living::gather_ticks(app.world(), oak, "log", 240), 180);
    assert_eq!(
        living::gather_ticks(app.world(), oak, catalog::WATER, 100),
        100
    );
    let axe = tools(&app, oak, "axe")[0];
    app.world_mut().get_mut::<Durability>(axe).unwrap().current = 0;
    assert_eq!(living::gather_ticks(app.world(), oak, "log", 240), 360);
    // Morale moves the same work within the bounded 90%..115% band.
    for (morale, pace) in [(0, 115), (100_000, 90), (250_000, 90), (-5, 115)] {
        app.world_mut()
            .get_mut::<colony::SocialLife>(oak)
            .unwrap()
            .morale = morale;
        assert_eq!(colony::work_pace_percent(app.world(), oak), pace);
        assert_eq!(
            living::gather_ticks(app.world(), oak, catalog::WATER, 100),
            pace
        );
    }
    // The older colony profile keeps its exact base durations.
    let old = testbed_content::new_sim_app(7, "island_colony");
    let resident = old
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .find(|entity| {
            old.world()
                .get::<Villager>(*entity)
                .is_some_and(|v| v.role == Role::Woodcutter)
        })
        .unwrap();
    assert_eq!(colony::work_pace_percent(old.world(), resident), 100);
    assert_eq!(living::gather_ticks(old.world(), resident, "log", 240), 240);
}

#[test]
fn gathering_wears_the_tool_and_a_spent_tool_breaks() {
    let mut app = scene();
    let oak = person(&app, "Oak");
    let id = *app.world().get::<SimId>(oak).unwrap();
    let axe = tools(&app, oak, "axe")[0];
    app.world_mut().get_mut::<Durability>(axe).unwrap().current = 1;
    let (patch_id, patch) = app
        .world()
        .resource::<SimIndex>()
        .map
        .iter()
        .find(|(_, entity)| {
            app.world()
                .get::<ResourcePatch>(**entity)
                .is_some_and(|patch| patch.kind == "log" && patch.remaining > 0)
                && settlements::member(app.world(), **entity)
                    == settlements::member(app.world(), oak)
        })
        .map(|(id, entity)| (*id, *entity))
        .unwrap();
    let at = *app.world().get::<SimPos>(patch).unwrap();
    app.world_mut().entity_mut(oak).insert(at);
    let logs = economy::count(app.world(), oak, "log");
    let mut gather = VillageAct::new(id, "gather");
    gather.target = Some(patch_id);
    gather.perform(app.world_mut()).unwrap();
    let ends = app.world().get::<village::Gathering>(oak).unwrap().ends;
    let now = app.world().resource::<SimClock>().tick;
    // With the axe: 75% of the patch's base time, scaled by morale pace.
    let base = app
        .world()
        .get::<ResourcePatch>(patch)
        .unwrap()
        .gather_ticks;
    let pace = colony::work_pace_percent(app.world(), oak);
    assert_eq!(ends - now, u64::from(base * 75 * pace / 10_000));
    run_ticks(app.world_mut(), ends - now + 1);
    assert!(economy::count(app.world(), oak, "log") > logs);
    assert!(
        tools(&app, oak, "axe").is_empty(),
        "the spent axe should break"
    );
    assert!(app.world().get_entity(axe).is_err());
}

#[test]
fn a_missing_tool_becomes_funded_smith_work() {
    let mut app = scene();
    let oak = person(&app, "Oak");
    economy::consume(app.world_mut(), oak, "axe", 1).unwrap();
    village::production::refresh(app.world_mut());
    assert_eq!(
        village::production::desired_stock(app.world(), oak, "axe"),
        1
    );
    let region = settlements::member(app.world(), oak);
    let smith = residents(&app)
        .into_iter()
        .find(|actor| {
            settlements::member(app.world(), *actor) == region
                && app
                    .world()
                    .get::<Villager>(*actor)
                    .is_some_and(|v| v.role == Role::Smith)
        })
        .unwrap();
    assert!(
        village::production::production_demand(app.world(), smith, "axe") > 0,
        "the smith should hold a funded axe commission"
    );
    assert!(catalog::recipes_for(app.world(), Role::Smith).contains(&catalog::FORGE_AXE));
}

#[test]
fn a_smith_forges_a_durable_axe_from_an_ingot_and_a_plank() {
    use npc_framework::crafting::{CraftSource, Crafting, begin_craft, complete_craft};
    let mut app = scene();
    let ivo = person(&app, "Ivo");
    let smithy = app.world().get::<Villager>(ivo).unwrap().workplace;
    let at = *app
        .world()
        .get::<SimPos>(app.world().resource::<SimIndex>().get(smithy).unwrap())
        .unwrap();
    app.world_mut().entity_mut(ivo).insert(at);
    for kind in ["ingot", "plank"] {
        let held = economy::count(app.world(), ivo, kind);
        if held > 0 {
            economy::consume(app.world_mut(), ivo, kind, held).unwrap();
        }
        economy::grant(app.world_mut(), ivo, Item::new(kind));
    }
    assert!(tools(&app, ivo, "axe").is_empty());
    begin_craft(
        app.world_mut(),
        ivo,
        catalog::FORGE_AXE,
        Some(smithy),
        CraftSource::Command,
        None,
    )
    .unwrap();
    let ends = app.world().get::<Crafting>(ivo).unwrap().ends;
    app.world_mut().resource_mut::<SimClock>().tick = ends;
    assert!(complete_craft(app.world_mut(), ivo).is_some());
    let forged = tools(&app, ivo, "axe");
    assert_eq!(forged.len(), 1);
    assert_eq!(
        app.world()
            .get::<Durability>(forged[0])
            .map(|d| (d.current, d.max)),
        Some((catalog::TOOL_DURABILITY, catalog::TOOL_DURABILITY))
    );
    assert_eq!(economy::count(app.world(), ivo, "ingot"), 0);
    assert_eq!(economy::count(app.world(), ivo, "plank"), 0);
}

#[test]
fn deep_cold_drains_health_only_in_the_regional_profile() {
    let mut app = scene();
    let jun = person(&app, "Jun");
    set_need(&mut app, jun, "Warmth", 2_000);
    let before = app.world().get::<Vitality>(jun).unwrap().health;
    living::climate_tick(app.world_mut());
    assert_eq!(
        app.world().get::<Vitality>(jun).unwrap().health,
        before - living::COLD_HEALTH_DRAIN
    );
    let warm = person(&app, "Mira");
    set_need(
        &mut app,
        warm,
        "Warmth",
        living::COLD_HEALTH_THRESHOLD + 5_000,
    );
    let before = app.world().get::<Vitality>(warm).unwrap().health;
    living::climate_tick(app.world_mut());
    assert_eq!(app.world().get::<Vitality>(warm).unwrap().health, before);

    let mut old = testbed_content::new_sim_app(7, "island_colony");
    let resident = old
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .find(|entity| {
            old.world()
                .get::<Villager>(*entity)
                .is_some_and(|v| v.role != Role::Visitor)
        })
        .unwrap();
    set_need(&mut old, resident, "Warmth", 2_000);
    let before = old.world().get::<Vitality>(resident).unwrap().health;
    living::climate_tick(old.world_mut());
    assert_eq!(
        old.world().get::<Vitality>(resident).unwrap().health,
        before
    );
}

#[test]
fn winter_hearths_burn_a_log_in_a_third_of_a_day() {
    let mut app = scene();
    let jun = person(&app, "Jun");
    let day = app.world().resource::<SimConfig>().day_length;
    assert_eq!(village::calendar(app.world()).1, village::Season::Autumn);
    assert_eq!(
        colony::home_heating(app.world(), jun).unwrap().period_ticks,
        day / 2
    );
    // The living calendar starts on day 19; winter begins on day 21.
    app.world_mut().resource_mut::<SimClock>().tick = 2 * day;
    assert_eq!(village::calendar(app.world()).1, village::Season::Winter);
    assert_eq!(
        colony::home_heating(app.world(), jun).unwrap().period_ticks,
        day / 3
    );
}

#[test]
fn persistent_margin_gap_attracts_labour_and_shortens_the_cooldown() {
    let mut app = scene();
    // The test pins the margin hysteresis it was written for (the shipped
    // default is now 6000 ticks; the rule is the same).
    {
        let mut registry = app
            .world_mut()
            .resource_mut::<testbed_content::village::frontier::economy_spec::EconomyRegistry>(
        );
        registry.spec.tuning.margin_persist_ticks = labor::MARGIN_PERSIST_TICKS;
        registry.spec.tuning.margin_cooldown_ticks = labor::MARGIN_PERSIST_TICKS;
    }
    let piper = person(&app, "Piper");
    let total = money(&app);
    assert_eq!(labor::margin_permille(app.world(), Role::Baker), 0);
    assert_eq!(labor::margin_permille(app.world(), Role::Woodcutter), 0);
    // A sustained bread shortage, expressed as its learned market price.
    set_price(&mut app, "bread", 12);
    assert!(labor::margin_permille(app.world(), Role::Baker) >= 1000);
    app.world_mut().resource_mut::<SimClock>().tick = 300;
    labor::tick(app.world_mut());
    let since = app
        .world()
        .get::<Employment>(piper)
        .unwrap()
        .margin_gap_since;
    assert_eq!(since, Some(300));
    // The gap must persist before anyone moves.
    app.world_mut().resource_mut::<SimClock>().tick = 300 + labor::MARGIN_PERSIST_TICKS;
    labor::tick(app.world_mut());
    assert!(labor::consider_assignment(app.world_mut(), piper));
    let person_now = app.world().get::<Villager>(piper).unwrap().clone();
    assert_eq!(person_now.role, Role::Baker);
    assert!(
        app.world()
            .get::<Employment>(piper)
            .unwrap()
            .last_reason
            .contains("margin")
    );
    let changed = app.world().resource::<SimClock>().tick;
    // The market turns: bread is glutted, logs are scarce. The ordinary 6000-tick
    // cooldown would hold Piper; a persistent gap releases them after 2400.
    set_price(&mut app, "bread", 2);
    set_price(&mut app, "log", 9);
    app.world_mut().resource_mut::<SimClock>().tick = changed + 300;
    labor::tick(app.world_mut());
    assert!(!labor::consider_assignment(app.world_mut(), piper));
    app.world_mut().resource_mut::<SimClock>().tick = changed + 300 + labor::MARGIN_PERSIST_TICKS;
    labor::tick(app.world_mut());
    assert!(
        app.world().resource::<SimClock>().tick < changed + 6000,
        "still inside the ordinary cooldown"
    );
    assert!(labor::consider_assignment(app.world_mut(), piper));
    assert_eq!(
        app.world().get::<Villager>(piper).unwrap().role,
        Role::Woodcutter
    );
    assert_eq!(money(&app), total);
    village::integrity::validate_colony(app.world()).unwrap();
}

#[test]
fn a_sole_essential_worker_ignores_a_margin_gap() {
    let mut app = scene();
    let nell = person(&app, "Nell");
    assert_eq!(
        app.world().get::<Villager>(nell).unwrap().role,
        Role::WaterKeeper
    );
    set_price(&mut app, "bread", 12);
    for tick in [300, 300 + labor::MARGIN_PERSIST_TICKS, 9000] {
        app.world_mut().resource_mut::<SimClock>().tick = tick;
        labor::tick(app.world_mut());
        assert!(!labor::consider_assignment(app.world_mut(), nell));
    }
    assert_eq!(
        app.world().get::<Villager>(nell).unwrap().role,
        Role::WaterKeeper
    );
}

fn woodcutters(app: &App) -> usize {
    residents(app)
        .into_iter()
        .filter(|actor| {
            app.world()
                .get::<Villager>(*actor)
                .is_some_and(|v| v.role == Role::Woodcutter)
        })
        .count()
}
fn produced(app: &App, kind: &str) -> u64 {
    app.world()
        .resource::<VillageState>()
        .produced
        .get(kind)
        .copied()
        .unwrap_or(0)
}

const LABOUR_RUN: u64 = 6_000;

/// End to end, a glut answers through the plan and the market: stock already on
/// offer replaces new gathering, and the learned price and realised margin fall.
/// (Labour leaving a glutted trade is covered by the price-signal test above; the
/// surplus woodcutters also leave in the undisturbed control run.)
#[test]
fn a_sustained_glut_lowers_output_price_and_realised_margin() {
    let mut control = scene();
    let mut glut = scene();
    // The declared disruption: large real stocks of fiber and logs at their producers.
    for actor in residents(&glut) {
        let role = glut.world().get::<Villager>(actor).map(|v| v.role);
        if role == Some(Role::Farmer) {
            economy::grant(
                glut.world_mut(),
                actor,
                Item::new(catalog::FIBER).stack(120),
            );
        }
        if role == Some(Role::Woodcutter) {
            economy::grant(glut.world_mut(), actor, Item::new("log").stack(60));
        }
    }
    run_ticks(control.world_mut(), LABOUR_RUN);
    run_ticks(glut.world_mut(), LABOUR_RUN);
    let log_margin = labor::good_margin_permille(glut.world(), Role::Woodcutter, "log");
    println!(
        "GLUT control_fiber={} glut_fiber={} control_log_price={:?} glut_log_price={:?} log_margin={log_margin} woodcutters={}/{}",
        produced(&control, catalog::FIBER),
        produced(&glut, catalog::FIBER),
        village::market::unit_price(control.world(), "log"),
        village::market::unit_price(glut.world(), "log"),
        woodcutters(&glut),
        woodcutters(&control),
    );
    assert!(produced(&control, catalog::FIBER) > 0);
    assert!(
        produced(&glut, catalog::FIBER) < produced(&control, catalog::FIBER),
        "a glut should reduce new fiber output"
    );
    // Logs are in surplus even in the undisturbed world (every resident starts
    // with two and few are bought), so the control price is already depressed.
    assert!(
        village::market::unit_price(glut.world(), "log") < catalog::price("log"),
        "unsold surplus should hold the learned log price below its seed"
    );
    assert!(
        log_margin <= -labor::MARGIN_GAP_PERMILLE,
        "the realised woodcutting margin should fall by at least the switching gap"
    );
    assert!(woodcutters(&glut) <= woodcutters(&control));
    for actor in residents(&glut) {
        assert!(village::living(glut.world(), actor));
    }
}

#[test]
fn a_sustained_log_shortage_raises_output_price_and_margin_and_keeps_its_labour() {
    let mut control = scene();
    let mut short = scene();
    // The declared disruption: nobody starts with any logs.
    for actor in residents(&short) {
        let logs = economy::count(short.world(), actor, "log");
        if logs > 0 {
            economy::consume(short.world_mut(), actor, "log", logs).unwrap();
        }
    }
    run_ticks(control.world_mut(), LABOUR_RUN);
    run_ticks(short.world_mut(), LABOUR_RUN);
    println!(
        "LOG_SHORTAGE control_logs={} short_logs={} control_woodcutters={} short_woodcutters={} log_price={:?} log_margin={}",
        produced(&control, "log"),
        produced(&short, "log"),
        woodcutters(&control),
        woodcutters(&short),
        village::market::unit_price(short.world(), "log"),
        labor::margin_permille(short.world(), Role::Woodcutter),
    );
    assert!(
        village::market::unit_price(short.world(), "log")
            > village::market::unit_price(control.world(), "log"),
        "scarcity should lift the learned log price above the surplus world's"
    );
    assert!(
        labor::margin_permille(short.world(), Role::Woodcutter)
            > labor::margin_permille(control.world(), Role::Woodcutter),
        "scarcity should lift the realised woodcutting margin"
    );
    assert!(
        produced(&short, "log") > produced(&control, "log"),
        "a shortage should raise log output"
    );
    assert!(
        woodcutters(&short) >= woodcutters(&control),
        "a shortage should not lose woodcutting labour"
    );
}

#[test]
fn cached_plan_is_reused_until_its_first_step_precondition_fails() {
    let mut app = scene();
    let mut found = None;
    for _ in 0..200 {
        run_ticks(app.world_mut(), 20);
        found =
            residents(&app)
                .into_iter()
                .find_map(|actor| match goap::recall(app.world(), actor) {
                    goap::Recall::Hit(plan) => Some((actor, plan)),
                    _ => None,
                });
        if found.is_some() {
            break;
        }
    }
    let (actor, plan) = found.expect("some resident should hold a reusable plan");
    let memo = app.world().get::<goap::PlanMemo>(actor).unwrap().clone();
    assert_eq!(memo.plan, plan);
    // Reuse is a pure read: asking again changes nothing.
    assert_eq!(
        goap::recall(app.world(), actor),
        goap::Recall::Hit(plan.clone())
    );
    assert_eq!(app.world().get::<goap::PlanMemo>(actor), Some(&memo));
    let step = plan.steps[0].clone();
    let target = app
        .world()
        .resource::<SimIndex>()
        .get(step.target())
        .unwrap();
    let reason = match &step {
        goap::PlanStep::Gather { .. } => {
            app.world_mut()
                .get_mut::<ResourcePatch>(target)
                .unwrap()
                .remaining = 0;
            "source is depleted"
        }
        goap::PlanStep::Buy { .. } => {
            app.world_mut().get_mut::<Wallet>(actor).unwrap().coins = 0;
            "offer is no longer affordable"
        }
        goap::PlanStep::Craft { recipe, .. } => {
            let input = app
                .world()
                .resource::<npc_framework::crafting::RecipeRegistry>()
                .recipes
                .get(recipe)
                .unwrap()
                .inputs[0]
                .key
                .clone();
            let held = economy::count(app.world(), actor, &input);
            economy::consume(app.world_mut(), actor, &input, held).unwrap();
            "craft inputs are missing"
        }
    };
    assert_eq!(
        goap::recall(app.world(), actor),
        goap::Recall::Failed(reason)
    );
    // A different world profile never caches.
    let old = testbed_content::new_sim_app(7, "island_colony");
    let resident = *old
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .next()
        .unwrap();
    assert_eq!(goap::recall(old.world(), resident), goap::Recall::Miss);
}
