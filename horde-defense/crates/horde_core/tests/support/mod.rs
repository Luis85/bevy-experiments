//! Helpers shared by the integration test crates. Each test binary uses a
//! different subset, so unused items are expected.
#![allow(dead_code)]
use horde_core::balance::{BalancePreset, BalanceSettings};
use horde_core::campaign::{Campaign, NpcTraining};
use horde_core::command::{Command, CommandOutcome};
use horde_core::content::{EnemyKind, ItemDefId, Mode, Policy, Slot, StructureKind, content};
use horde_core::defenders::DefenderId;
use horde_core::sim::{Game, Input, TICKS_PER_SECOND, construction_lane_order, smoke_input};
use horde_core::skill_catalog::skills;
use horde_core::world::world;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;

/// FNV-1a (64-bit) of `bytes`, as used by save checksums and pinned digests.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    fnv1a_extend(0xcbf2_9ce4_8422_2325, bytes)
}

/// Continue an FNV-1a hash over more `bytes`.
pub fn fnv1a_extend(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A default game with the legacy (fixed, unscaled) balance and features.
pub fn legacy_game() -> Game {
    let mut game = Game::default();
    game.campaign_mut().legacy_features = true;
    game.campaign_mut().balance_settings = BalanceSettings::legacy();
    game
}

/// A legacy game with an unpaused attempt deployed in `mode`.
pub fn game(mode: Mode) -> Game {
    let mut g = legacy_game();
    g.campaign_mut().attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    g
}

/// Spawn a Shambler (or boss) in `lane` and move it to `x`.
pub fn enemy(g: &mut Game, lane: u8, x: f32, boss: bool) {
    let mut a = g.attempt_slot_mut().take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, lane, boss);
    a.enemies.last_mut().unwrap().x = x;
    *g.attempt_slot_mut() = Some(a);
}

/// Tick with default (idle) input `count` times.
pub fn advance(game: &mut Game, count: u32) {
    for _ in 0..count {
        game.tick(Input::default());
    }
}

/// The catalogue price of skill node `id`.
pub fn node_price(id: &str) -> u64 {
    skills().node(id).unwrap().price
}

/// The summed catalogue price of the named skill nodes.
pub fn nodes_price(nodes: &[&str]) -> u64 {
    nodes.iter().map(|id| node_price(id)).sum()
}

/// A fresh campaign that bought the named skill nodes and still holds
/// `spare` coins.
pub fn trained_campaign(spare: u64, nodes: &[&str]) -> Campaign {
    let mut campaign = Campaign::default();
    campaign.reward(spare + nodes_price(nodes), 0, Mode::Personal);
    for node in nodes {
        campaign.purchase_node(node).unwrap();
    }
    campaign
}

/// A default game that started its campaign in `country`.
pub fn country_game(country: &str) -> Game {
    let mut game = Game::default();
    game.campaign_mut().choose_start_country(country).unwrap();
    game
}

/// Decode a JSON game value as a v1 save envelope with a valid checksum, the
/// way an old build's save would reach migration.
pub fn decode_value(value: serde_json::Value) -> Result<Game, String> {
    let payload = serde_json::to_string(&value).unwrap();
    let checksum = fnv1a(payload.as_bytes());
    Ok(Game::decode_save_generation(
        &serde_json::to_vec(&serde_json::json!({
            "version": 1, "checksum": checksum, "payload": payload,
        }))
        .unwrap(),
    )?)
}

/// A legacy game with four cleared countries, 10k coins and a started campaign,
/// ready to recruit survivors and deploy parallel expeditions.
pub fn strategic_game() -> Game {
    let mut g = legacy_game();
    let countries: Vec<_> = horde_core::world::world()
        .countries
        .iter()
        .take(4)
        .map(|c| (c.id.clone(), c.level_count()))
        .collect();
    g.campaign_mut()
        .choose_start_country(&countries[0].0)
        .unwrap();
    for (id, count) in countries {
        g.campaign_mut()
            .world
            .country_levels
            .insert(id.clone(), count);
        g.campaign_mut().world.cleared.push(id);
    }
    g.campaign_mut().coins = 10000;
    g.campaign_mut().attempts = 1;
    g
}

// ---------------------------------------------------------------------------
// Determinism tooling (tests/determinism.rs). These helpers explain a digest
// move; they never decide whether one is allowed.
// ---------------------------------------------------------------------------

/// The first-stage encounter the `smoke` command plays: a default game with
/// `seed`, deployed in `mode` (Npc with the legacy features and the BossFocus
/// policy) and driven by `smoke_input` for `ticks` ticks. `observe` sees the
/// game after every tick.
pub fn smoke_run(seed: u64, mode: Mode, ticks: u32, mut observe: impl FnMut(&Game)) -> Game {
    let mut g = Game::default();
    g.set_seed(seed);
    g.campaign_mut().attempts = 1;
    if mode == Mode::Npc {
        g.campaign_mut().legacy_features = true;
        g.campaign_mut().npc.policy = Policy::BossFocus;
    }
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    for _ in 0..ticks {
        let input = smoke_input(&g);
        g.tick(input);
        observe(&g);
    }
    g
}

// ---------------------------------------------------------------------------
// The progressed determinism runs (W4-09; design-save-determinism section 9,
// "Extra coverage (wave 4)"). `tests/determinism.rs` pins their state digests
// (PROGRESSED_GOLDEN) and gameplay traces (PROGRESSED_TRACE).
// ---------------------------------------------------------------------------

/// The two progressed campaigns `PROGRESSED_GOLDEN` pins, per seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressedRun {
    /// The player defends the Capstone (last) stage of the front on the
    /// Hard preset.
    HardCapstone,
    /// A trained, equipped recruit takes the main run over (Npc mode) on the
    /// front's first stage, on the default preset.
    RecruitTakeover,
}

impl ProgressedRun {
    pub const ALL: [Self; 2] = [Self::HardCapstone, Self::RecruitTakeover];
    /// The attempt mode the run deploys in.
    pub fn mode(self) -> Mode {
        match self {
            Self::HardCapstone => Mode::Personal,
            Self::RecruitTakeover => Mode::Npc,
        }
    }
}

/// The campaign threat rank (`Campaign::victories`) of the progressed runs.
pub const PROGRESSED_RANK: u64 = 12;
/// How many ticks the smoke bot drives a progressed run.
pub const PROGRESSED_TICKS: u32 = 3600;
/// The staged attempt clock starts this many ticks before the boss is due,
/// so the window holds the late wave, the boss, its phases and (when the
/// defender wins) the settlement.
pub const PROGRESSED_BOSS_LEAD: u64 = 1800;
/// The recruit takeover's lead (W4-14): the Npc rules meet half the
/// arrivals since the Npc arrival factor fell to 0.5, so its window starts
/// earlier to keep an elite, the new archetypes and the boss in it.
pub const PROGRESSED_TAKEOVER_BOSS_LEAD: u64 = 3000;
/// The coins the progressed campaigns are staged with before they shop.
pub const PROGRESSED_PURSE: u64 = 400_000;
/// The liberated countries (three stages each) the campaigns are staged with:
/// they unlock Companion, Recruitment and Parallel, and the recruit comes
/// from the second.
pub const PROGRESSED_LIBERATED: [&str; 4] = ["ALA", "ALB", "ASM", "AND"];
/// The active front (three stages): its Capstone is stage 2.
pub const PROGRESSED_FRONT: &str = "AIA";

/// The highest-tier catalogue item for `slot` the campaign can buy now (the
/// last of equal tiers in catalogue order); NPC-only items only for an npc.
fn best_purchasable(g: &Game, slot: Slot, npc: bool) -> Option<usize> {
    let unlocked = g.campaign().unlocked();
    content()
        .items
        .iter()
        .enumerate()
        .filter(|(_, i)| {
            i.slot == slot && i.purchasable() && i.tier <= unlocked && (npc || !i.npc_only)
        })
        .max_by_key(|(_, i)| i.tier)
        .map(|(n, _)| n)
}

/// Buys the best item for every slot and returns the new item ids.
fn buy_best_gear(g: &mut Game, npc: bool) -> Vec<u64> {
    let mut ids = Vec::new();
    for slot in Slot::ALL {
        if let Some(n) = best_purchasable(g, slot, npc) {
            match g.apply(&Command::BuyItem(ItemDefId::at(n))) {
                Ok(CommandOutcome::Id(id)) => ids.push(id),
                other => panic!("buying {}: {other:?}", content().items[n].name),
            }
        }
    }
    ids
}

/// The game a progressed run starts from, at [`PROGRESSED_RANK`]: see
/// [`progressed_determinism_game_at`].
pub fn progressed_determinism_game(seed: u64, run: ProgressedRun) -> Game {
    progressed_determinism_game_at(seed, run, PROGRESSED_RANK)
}

/// A progressed campaign at threat `rank`, deployed and with its attempt
/// clock [`PROGRESSED_BOSS_LEAD`] ticks before the boss for
/// [`ProgressedRun::HardCapstone`] and [`PROGRESSED_TAKEOVER_BOSS_LEAD`]
/// ticks before it for [`ProgressedRun::RecruitTakeover`] (W4-14 halved the
/// Npc arrivals, so the takeover's window starts earlier to keep an elite,
/// the new archetypes and the boss phase in it). Built here, not by the
/// review render fixtures.
///
/// Staged through the test-support seams, because no short legal command
/// sequence reaches them: the world (the [`PROGRESSED_LIBERATED`] countries
/// liberated, [`PROGRESSED_FRONT`] the active front at its Capstone for
/// [`ProgressedRun::HardCapstone`] and at its first stage for
/// [`ProgressedRun::RecruitTakeover`]), the victories (the threat rank), the
/// attempt count, the purse and, after the deploy, the attempt clock.
///
/// Played through `Game::apply`: the Hard preset (HardCapstone only), every
/// skill node the purse buys in catalogue order, the best gear for the
/// player (equipped), and for RecruitTakeover a recruit from the second
/// liberated country in the best gear, trained until no training or point
/// is left to buy, chosen as the next defender. Then `BeginDefense`, and the
/// defender builds walls, turrets and traps lane by lane while supply lasts.
pub fn progressed_determinism_game_at(seed: u64, run: ProgressedRun, rank: u64) -> Game {
    let mut g = Game::default();
    g.set_seed(seed);
    if run == ProgressedRun::HardCapstone {
        g.apply(&Command::SetBalance(BalanceSettings::for_preset(
            BalancePreset::Hard,
        )))
        .expect("the Hard preset applies to a new campaign");
    }
    let front_count = world()
        .country(PROGRESSED_FRONT)
        .expect("the progressed front exists")
        .level_count();
    let stage = match run {
        ProgressedRun::HardCapstone => front_count - 1,
        ProgressedRun::RecruitTakeover => 0,
    };
    {
        let c = g.staging().campaign();
        c.choose_start_country(PROGRESSED_FRONT)
            .expect("the progressed front can be chosen");
        c.world
            .country_levels
            .insert(PROGRESSED_FRONT.into(), stage);
        for id in PROGRESSED_LIBERATED {
            let count = world().country(id).expect("a staged country").level_count();
            c.world.country_levels.insert(id.into(), count);
            c.world.cleared.push(id.into());
            if !c.world.unlocked.iter().any(|u| u == id) {
                c.world.unlocked.push(id.into());
            }
        }
        c.victories = rank;
        c.attempts = rank;
        c.coins = PROGRESSED_PURSE;
    }
    while let Some(id) = skills()
        .nodes
        .iter()
        .find(|n| g.campaign().can_purchase_node(&n.id).is_ok())
        .map(|n| n.id.clone())
    {
        g.apply(&Command::BuySkillNode(id))
            .expect("a purchasable node buys");
    }
    for id in buy_best_gear(&mut g, false) {
        g.apply(&Command::Equip {
            mode: Mode::Personal,
            id,
        })
        .expect("the player equips bought gear");
    }
    let defender = match run {
        ProgressedRun::HardCapstone => DefenderId::Player,
        ProgressedRun::RecruitTakeover => {
            let id = match g.apply(&Command::Recruit(PROGRESSED_LIBERATED[1].into())) {
                Ok(CommandOutcome::Id(id)) => id,
                other => panic!("recruiting: {other:?}"),
            };
            for item in buy_best_gear(&mut g, true) {
                g.apply(&Command::EquipRecruit { id, item })
                    .expect("the recruit equips bought gear");
            }
            loop {
                let mut trained = false;
                for training in NpcTraining::ALL {
                    trained |= g.apply(&Command::TrainRecruit { id, training }).is_ok();
                }
                if !trained && g.apply(&Command::BuyRecruitPoint(id)).is_err() {
                    break;
                }
            }
            DefenderId::Recruit(id)
        }
    };
    g.apply(&Command::SetNextDefender(defender))
        .expect("the defender is selectable");
    g.apply(&Command::BeginDefense { stage })
        .expect("the progressed defense deploys");
    let lanes = g.attempt().expect("deployed").lane_count;
    for (kind, column) in [
        (StructureKind::Wall, 0),
        (StructureKind::Turret, 1),
        (StructureKind::Trap, 3),
    ] {
        for lane in construction_lane_order(lanes) {
            // Supply runs out part-way; a refused placement changes nothing.
            let _ = g.apply(&Command::Place { kind, lane, column });
        }
    }
    let a = g.staging().attempt().expect("deployed");
    let boss_tick = (a.boss_time() * TICKS_PER_SECOND as f32).ceil() as u64;
    a.tick = boss_tick.saturating_sub(match run {
        ProgressedRun::HardCapstone => PROGRESSED_BOSS_LEAD,
        ProgressedRun::RecruitTakeover => PROGRESSED_TAKEOVER_BOSS_LEAD,
    });
    g
}

/// The progressed run of `seed` and `run`: [`progressed_determinism_game`]
/// driven by `smoke_input` for [`PROGRESSED_TICKS`] ticks. `observe` sees
/// the game after every tick.
pub fn progressed_run(seed: u64, run: ProgressedRun, observe: impl FnMut(&Game)) -> Game {
    smoke_drive(
        progressed_determinism_game(seed, run),
        PROGRESSED_TICKS,
        observe,
    )
}

/// Drives `g` with `smoke_input` for `ticks` ticks; `observe` sees the game
/// after every tick.
pub fn smoke_drive(mut g: Game, ticks: u32, mut observe: impl FnMut(&Game)) -> Game {
    for _ in 0..ticks {
        let input = smoke_input(&g);
        g.tick(input);
        observe(&g);
    }
    g
}

/// The attempt ticks at which [`OutcomeSummary`] samples a run.
pub const OUTCOME_SAMPLE_TICKS: [u64; 3] = [600, 1200, 1800];

/// The gameplay outcome of a live attempt at one tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeSample {
    pub tick: u64,
    pub kills: u32,
    pub coins: u64,
    pub hp: i32,
    pub loot: u32,
    pub spawned: u32,
    pub rng: u64,
}

/// Kills, coins, hp, loot, spawned and `rng.state` of a run at each of
/// [`OUTCOME_SAMPLE_TICKS`]. Feed it every tick with [`OutcomeSummary::observe`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutcomeSummary {
    pub samples: Vec<OutcomeSample>,
}

impl OutcomeSummary {
    /// Record the live attempt if it stands on a sample tick.
    pub fn observe(&mut self, g: &Game) {
        let Some(a) = g.attempt() else { return };
        if OUTCOME_SAMPLE_TICKS.contains(&a.tick) {
            self.samples.push(OutcomeSample {
                tick: a.tick,
                kills: a.kills,
                coins: a.coins,
                hp: a.hp,
                loot: a.loot,
                spawned: a.telemetry.spawned,
                rng: a.rng.state,
            });
        }
    }
    /// FNV-1a of the printed summary.
    pub fn digest(&self) -> u64 {
        fnv1a(self.to_string().as_bytes())
    }
}

impl fmt::Display for OutcomeSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, s) in self.samples.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(
                f,
                "t{} kills {} coins {} hp {} loot {} spawned {} rng {:#018x}",
                s.tick, s.kills, s.coins, s.hp, s.loot, s.spawned, s.rng
            )?;
        }
        Ok(())
    }
}

/// The outcome summary of the [`smoke_run`] of `seed` and `mode` to the last
/// sample tick.
pub fn outcome_summary(seed: u64, mode: Mode) -> OutcomeSummary {
    let mut summary = OutcomeSummary::default();
    let ticks = u32::try_from(OUTCOME_SAMPLE_TICKS[2]).unwrap();
    smoke_run(seed, mode, ticks, |g| summary.observe(g));
    summary
}

/// Object keys whose value is a map keyed by data (level, country and
/// archetype IDs), not by field names: `Campaign::country_layouts`,
/// `first_clear_claimed`, `WorldProgress::country_levels`,
/// `clear_high_water` and the per-archetype `CombatTelemetry` maps
/// `spawned_by_archetype`, `kills_by_archetype` and
/// `damage_taken_by_archetype` (W1-27). Their entries are state, so
/// [`json_key_paths`] reports their children as one `*` segment.
pub const DATA_KEYED_MAPS: [&str; 7] = [
    "country_layouts",
    "first_clear_claimed",
    "country_levels",
    "clear_high_water",
    "spawned_by_archetype",
    "kills_by_archetype",
    "damage_taken_by_archetype",
];

/// Every JSON-pointer path of an object key in `value` (no leaf values),
/// with array indices stripped: array elements add their keys under the
/// array's own path, so element count and order never change the set. The
/// entries of a [`DATA_KEYED_MAPS`] map collapse to one `*` segment, so
/// adding or removing an entry never changes the set either.
pub fn json_key_paths(value: &Value) -> BTreeSet<String> {
    fn walk(value: &Value, path: &str, data_keyed: bool, paths: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let child_path = if data_keyed {
                        format!("{path}/*")
                    } else {
                        let escaped = key.replace('~', "~0").replace('/', "~1");
                        format!("{path}/{escaped}")
                    };
                    let child_keyed = !data_keyed && DATA_KEYED_MAPS.contains(&key.as_str());
                    walk(child, &child_path, child_keyed, paths);
                    paths.insert(child_path);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, path, data_keyed, paths);
                }
            }
            _ => {}
        }
    }
    let mut paths = BTreeSet::new();
    walk(value, "", false, &mut paths);
    paths
}

/// Where an expedition's worker game sits in a save: `ParallelBattle.game`
/// inside the root `battles` array. [`game_key_paths`] folds it onto the root.
pub const WORKER_GAME_PREFIX: &str = "/battles/game";

/// [`json_key_paths`] of a saved game with every worker game path folded
/// onto the root path of the same field (`/battles/game/attempt/drop_ids`
/// becomes `/attempt/drop_ids`), since a worker game is a `Game` too.
pub fn game_key_paths(value: &Value) -> BTreeSet<String> {
    json_key_paths(value)
        .into_iter()
        .map(|path| match path.strip_prefix(WORKER_GAME_PREFIX) {
            Some(rest) if rest.starts_with('/') => rest.to_owned(),
            _ => path,
        })
        .collect()
}

/// Every value at the key `path` (as [`json_key_paths`] spells it: no array
/// indices), walking into every element of each array on the way. Empty when
/// no object along the way holds the key.
pub fn values_at_key_path<'a>(value: &'a Value, path: &str) -> Vec<&'a Value> {
    fn walk<'a>(value: &'a Value, keys: &[&str], found: &mut Vec<&'a Value>) {
        let Some((key, rest)) = keys.split_first() else {
            found.push(value);
            return;
        };
        match value {
            Value::Array(items) => items.iter().for_each(|item| walk(item, keys, found)),
            Value::Object(map) => {
                if let Some(child) = map.get(*key) {
                    walk(child, rest, found);
                }
            }
            _ => {}
        }
    }
    let keys: Vec<&str> = path.split('/').skip(1).collect();
    let mut found = Vec::new();
    walk(value, &keys, &mut found);
    found
}

/// Every field save format 7 added (schema-v7.md section 3), as
/// `(section, key path)`. Paths are [`game_key_paths`]: no array indices, and
/// a worker game's field shares its root path. A type saved at several places
/// (telemetry on the attempt and the report, run records in the history and
/// the pending list) has one row per place. Each field is serde-defaulted and
/// skipped at its default, so a v6 save and the GOLDEN runs write none of
/// them. `tests/schema_v7.rs` proves the list complete against the types.
pub const V7_SAVED_KEYS: &[(&str, &str)] = &[
    // 3.1 Campaign
    ("3.1 Campaign", "/campaign/auto_sell"),
    ("3.1 Campaign", "/campaign/next_defender"),
    ("3.1 Campaign", "/campaign/reserved_recruit"),
    ("3.1 Campaign", "/campaign/milestones_claimed"),
    ("3.1 Campaign", "/campaign/balance_extras_basis"),
    // 3.2 Player
    ("3.2 Player", "/campaign/player/level_grant_paid"),
    // 3.3 Attempt (the seat is saved as "defender")
    ("3.3 Attempt", "/attempt/defender"),
    ("3.3 Attempt", "/attempt/drop_ids"),
    ("3.3 Attempt", "/attempt/drop_tier"),
    // 3.4 Report
    ("3.4 Report", "/last_result/defender"),
    ("3.4 Report", "/last_result/drop_ids"),
    ("3.4 Report", "/last_result/loot_by_quality"),
    ("3.4 Report", "/last_result/auto_sold"),
    ("3.4 Report", "/last_result/progression_rewards"),
    ("3.4 Report", "/last_result/boss_rank"),
    // 3.5 RunRecord, in the run history and the pending list
    ("3.5 RunRecord", "/campaign/run_history/auto_sold_count"),
    ("3.5 RunRecord", "/campaign/run_history/auto_sold_coins"),
    ("3.5 RunRecord", "/campaign/run_history/level_grant_coins"),
    ("3.5 RunRecord", "/campaign/run_history/milestone_coins"),
    ("3.5 RunRecord", "/campaign/run_history/elite_kills"),
    ("3.5 RunRecord", "/campaign/run_history/boss_ttk_ticks"),
    ("3.5 RunRecord", "/campaign/run_history/boss_rank"),
    ("3.5 RunRecord", "/pending_runs/auto_sold_count"),
    ("3.5 RunRecord", "/pending_runs/auto_sold_coins"),
    ("3.5 RunRecord", "/pending_runs/level_grant_coins"),
    ("3.5 RunRecord", "/pending_runs/milestone_coins"),
    ("3.5 RunRecord", "/pending_runs/elite_kills"),
    ("3.5 RunRecord", "/pending_runs/boss_ttk_ticks"),
    ("3.5 RunRecord", "/pending_runs/boss_rank"),
    // 3.6 ParallelBattle
    ("3.6 ParallelBattle", "/battles/summary"),
    ("3.6 ParallelBattle", "/battles/pending_auto_sell"),
    // 3.7 StrategyState and Recruit
    ("3.7 StrategyState", "/strategy/folded_outcomes"),
    ("3.7 Recruit", "/strategy/roster/containments"),
    // 3.8 Enemy
    ("3.8 Enemy", "/attempt/enemies/split_child"),
    ("3.8 Enemy", "/attempt/enemies/boss_state"),
    // 3.9 ResolvedEnemy
    ("3.9 ResolvedEnemy", "/attempt/enemies/resolved/reach"),
    (
        "3.9 ResolvedEnemy",
        "/attempt/enemies/resolved/ranged_armor",
    ),
    (
        "3.9 ResolvedEnemy",
        "/attempt/enemies/resolved/npc_ranged_armor",
    ),
    (
        "3.9 ResolvedEnemy",
        "/attempt/enemies/resolved/stagger_immune",
    ),
    ("3.9 ResolvedEnemy", "/attempt/enemies/resolved/group_size"),
    (
        "3.9 ResolvedEnemy",
        "/attempt/enemies/resolved/death_effect",
    ),
    ("3.9 ResolvedEnemy", "/attempt/enemies/resolved/tags"),
    ("3.9 ResolvedEnemy", "/attempt/enemies/resolved/elite"),
    // 3.10 ProgressionSnapshot
    (
        "3.10 ProgressionSnapshot",
        "/attempt/progression/elite_rate_bp",
    ),
    (
        "3.10 ProgressionSnapshot",
        "/attempt/progression/front_ramp",
    ),
    ("3.10 ProgressionSnapshot", "/attempt/progression/boss_rank"),
    (
        "3.10 ProgressionSnapshot",
        "/attempt/progression/boss_phases",
    ),
    (
        "3.10 ProgressionSnapshot",
        "/attempt/progression/boss_armor_multiplier",
    ),
    (
        "3.10 ProgressionSnapshot",
        "/attempt/progression/npc_boss_armor_add",
    ),
    (
        "3.10 ProgressionSnapshot",
        "/attempt/progression/boss_health_multiplier",
    ),
    (
        "3.10 ProgressionSnapshot",
        "/attempt/progression/boss_reward_multiplier",
    ),
    (
        "3.10 ProgressionSnapshot",
        "/attempt/progression/boss_damage_multiplier",
    ),
    // 3.11 SkillModifiers (saved inside the attempt's defender profile)
    (
        "3.11 SkillModifiers",
        "/attempt/profile/modifiers/class_modifiers",
    ),
    (
        "3.11 SkillModifiers",
        "/attempt/profile/modifiers/armor_penetration",
    ),
    (
        "3.11 SkillModifiers",
        "/attempt/profile/modifiers/hazard_damage_multiplier",
    ),
    // 3.12 CombatTelemetry, on the live attempt and on the report
    (
        "3.12 CombatTelemetry",
        "/attempt/telemetry/spawned_by_archetype",
    ),
    (
        "3.12 CombatTelemetry",
        "/attempt/telemetry/kills_by_archetype",
    ),
    (
        "3.12 CombatTelemetry",
        "/attempt/telemetry/damage_taken_by_archetype",
    ),
    ("3.12 CombatTelemetry", "/attempt/telemetry/elites_spawned"),
    ("3.12 CombatTelemetry", "/attempt/telemetry/elites_killed"),
    (
        "3.12 CombatTelemetry",
        "/attempt/telemetry/hazard_damage_taken",
    ),
    (
        "3.12 CombatTelemetry",
        "/attempt/telemetry/boss_phase_ticks",
    ),
    (
        "3.12 CombatTelemetry",
        "/last_result/telemetry/spawned_by_archetype",
    ),
    (
        "3.12 CombatTelemetry",
        "/last_result/telemetry/kills_by_archetype",
    ),
    (
        "3.12 CombatTelemetry",
        "/last_result/telemetry/damage_taken_by_archetype",
    ),
    (
        "3.12 CombatTelemetry",
        "/last_result/telemetry/elites_spawned",
    ),
    (
        "3.12 CombatTelemetry",
        "/last_result/telemetry/elites_killed",
    ),
    (
        "3.12 CombatTelemetry",
        "/last_result/telemetry/hazard_damage_taken",
    ),
    (
        "3.12 CombatTelemetry",
        "/last_result/telemetry/boss_phase_ticks",
    ),
];

/// FNV-1a of the sorted [`json_key_paths`], one per line: moves only when
/// the serialized shape gains, loses or renames a field key (entries of
/// arrays and [`DATA_KEYED_MAPS`] maps are data, not shape).
pub fn json_key_paths_fnv(value: &Value) -> u64 {
    json_key_paths(value).iter().fold(fnv1a(b""), |hash, path| {
        fnv1a_extend(fnv1a_extend(hash, path.as_bytes()), b"\n")
    })
}

/// Object keys whose whole subtree is telemetry or a receipt: what a run
/// measured or handed out, never what the next tick reads. A move confined to
/// these subtrees is class T.
pub const TELEMETRY_SUBTREES: [&str; 8] = [
    "telemetry",
    "dps",
    "loot_events",
    "loot_quality_counts",
    "gear_snapshot",
    "run_history",
    "pending_runs",
    "progression_rewards",
];

/// `value` with every [`TELEMETRY_SUBTREES`] key removed at any depth.
pub fn without_telemetry(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !TELEMETRY_SUBTREES.contains(&key.as_str()))
                .map(|(key, child)| (key.clone(), without_telemetry(child)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_telemetry).collect()),
        other => other.clone(),
    }
}

/// FNV-1a of the definition, quality and level of every `campaign.items`
/// entry, in order. Panics when the payload has no `campaign.items` array, so
/// a renamed field cannot silently hide content moves.
pub fn items_fnv(payload: &Value) -> u64 {
    let items = payload
        .pointer("/campaign/items")
        .and_then(Value::as_array)
        .expect("the payload has a campaign.items array");
    items.iter().fold(fnv1a(b""), |hash, item| {
        let line = format!(
            "{} {} {}\n",
            item["definition"], item["quality"], item["level"]
        );
        fnv1a_extend(hash, line.as_bytes())
    })
}

/// What a run's digests are made of, for classifying a move between two
/// builds (`determinism_anatomy` prints one per run).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anatomy {
    /// `Game::state_digest`.
    pub golden: u64,
    /// The gameplay trace hash.
    pub trace: u64,
    /// [`json_key_paths_fnv`] of the payload.
    pub keys: u64,
    /// [`items_fnv`] of the payload.
    pub items: u64,
    /// FNV-1a of the payload [`without_telemetry`].
    pub core: u64,
    /// [`OutcomeSummary::digest`].
    pub outcome: u64,
}

impl Anatomy {
    pub fn of(payload: &Value, golden: u64, trace: u64, outcome: u64) -> Self {
        Self {
            golden,
            trace,
            keys: json_key_paths_fnv(payload),
            items: items_fnv(payload),
            core: fnv1a(without_telemetry(payload).to_string().as_bytes()),
            outcome,
        }
    }
}

impl fmt::Display for Anatomy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "golden {:#018x} trace {:#018x} keys {:#018x} items {:#018x} core {:#018x} outcome {:#018x}",
            self.golden, self.trace, self.keys, self.items, self.core, self.outcome
        )
    }
}

/// The class of a digest move (determinism protocol in
/// design-save-determinism section 9; ledger ruling P3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveClass {
    /// Format: the serialized key paths moved (a key gained, lost or
    /// renamed) with no gameplay, content or telemetry-only explanation.
    F,
    /// Content: campaign items moved, the outcome did not.
    C,
    /// Telemetry: only telemetry and receipt subtrees moved.
    T,
    /// Gameplay: the outcome summary or the gameplay trace moved, or core
    /// state moved with nothing else to explain it (the conservative class).
    G,
}

/// Classify the move from `before` to `after`; None when nothing moved.
///
/// In precedence order: G when the outcome or the trace moved; T when the
/// payload without telemetry is unchanged; C when the items moved; F when the
/// key paths moved; otherwise G, because a value-only core move (a campaign
/// counter, recruit XP, a field the trace does not cover) is a state move,
/// never format. The digests cannot tell whether values also moved beside a
/// key move, so an F still needs its written reason checked by hand.
pub fn classify(before: &Anatomy, after: &Anatomy) -> Option<MoveClass> {
    if before == after {
        None
    } else if before.outcome != after.outcome || before.trace != after.trace {
        Some(MoveClass::G)
    } else if before.core == after.core {
        Some(MoveClass::T)
    } else if before.items != after.items {
        Some(MoveClass::C)
    } else if before.keys != after.keys {
        Some(MoveClass::F)
    } else {
        Some(MoveClass::G)
    }
}
