//! Writers for checked-in save fixtures. Each writer is `#[ignore]`d and does
//! nothing unless its environment variable names the file to create; it
//! refuses to overwrite an existing file and asserts the save format it cuts.
//!
//! `LASTLIGHT_WRITE_SAVE_FIXTURE=<path>` cuts `fixtures/save_v6.json` (see the
//! `save_migration.rs` header for what it holds). The state is reached through
//! `Game::apply` with the seeded bot inputs of the `--smoke` exercise
//! (`smoke_input`). The staging seam is used only for: the game seed and the
//! strategy seed, coins (so the recipe buys instead of grinding), and to
//! restore the root report that deploying the final attempt clears
//! (`lifecycle.rs:234`), so one file holds both a settled Mara report and a
//! live attempt.
//!
//! `LASTLIGHT_WRITE_SAVE_V7_FIXTURE=<path>` cuts `fixtures/save_v7.json`, the
//! schema-freeze file (W1-63, schema-v7.md section 11), with
//! [`stage_v7_scenario`]: a short campaign played through `Game::apply` and
//! the smoke bot, then the v7 fields wave 1 has no writer for yet staged
//! through the test-support seams (`Staging::stage_takeover` for the
//! recruit defense; the staging handle and the `*_mut` accessors for enemy
//! state, receipts and summaries). It must validate as a save.
//!
//! Run: `cargo test --locked -p horde_core --test save_fixtures -- --ignored`.
#![allow(clippy::print_stderr)] // the writers report progress and the file size
use horde_core::balance::{BalancePreset, BalanceSettings};
use horde_core::campaign::{AutoSellRule, AutoSoldLine, AutoSoldReceipt, NpcTraining};
use horde_core::command::{Command, CommandOutcome};
use horde_core::content::{EnemyKind, Mode, Slot, StructureKind, content};
use horde_core::defenders::DefenderId;
use horde_core::enemy_catalog::{DeathEffect, EliteMark};
use horde_core::progression::BossRank;
use horde_core::quality::Quality;
use horde_core::rules::Rng;
use horde_core::sim::{
    BossPhaseProfile, BossState, DEFENSE_LINE, EnemyPhase, ExpeditionSummary, Game, Input,
    MilestoneReward, Outcome, ProgressionRewards, SAVE_VERSION, smoke_input,
};
use horde_core::skill_catalog::{ClassModifiers, skills};
use horde_core::survivors::SurvivorId;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Seed of the v6 fixture generation.
const SEED: u64 = 0x5AFE_0006;
/// Ten simulated minutes: the longest a staged encounter may run.
const ENCOUNTER_CAP: u32 = 60 * 600;

fn apply(g: &mut Game, command: Command) -> CommandOutcome {
    g.apply(&command)
        .unwrap_or_else(|error| panic!("{command:?} rejected: {error}"))
}

fn id(outcome: CommandOutcome) -> u64 {
    let CommandOutcome::Id(id) = outcome else {
        panic!("expected a new identity, got {outcome:?}");
    };
    id
}

/// Staged funds: the recipe buys its way to a strong defender instead of grinding.
fn fund(g: &mut Game, coins: u64) {
    let campaign = g.staging().campaign();
    campaign.coins = campaign.coins.max(coins);
}

/// Buys every skill node the campaign may purchase now, in catalogue order.
fn buy_nodes(g: &mut Game) {
    loop {
        let mut bought = false;
        for node in &skills().nodes {
            if !g.campaign().has_node(&node.id) {
                fund(g, node.price + 10_000);
                bought |= g.apply(&Command::BuySkillNode(node.id.clone())).is_ok();
            }
        }
        if !bought {
            break;
        }
    }
}

/// Buys and equips the best unlocked item per slot that `mode` may use.
fn buy_gear(g: &mut Game, mode: Mode) {
    for slot in Slot::ALL {
        let best = content()
            .items
            .iter()
            .enumerate()
            .filter(|(n, item)| {
                item.slot == slot
                    && item.tier > 0
                    && item.tier <= g.campaign().unlocked()
                    && g.campaign().can_use_weapon(mode, *n)
            })
            .max_by_key(|(n, item)| (item.tier, item.buy_value, std::cmp::Reverse(*n)))
            .map(|(n, item)| (n, item.tier));
        let Some((definition, tier)) = best else {
            continue;
        };
        let current = g.campaign().equipped(mode, slot);
        let level = {
            let loadout = match mode {
                Mode::Personal => g.campaign().player.loadout,
                Mode::Npc => g.campaign().npc.loadout,
            };
            g.campaign()
                .item(loadout[slot.index()])
                .map_or(0, |i| i.level)
        };
        if current.tier > tier || (current.tier == tier && level >= g.campaign().shop_item_level())
        {
            continue;
        }
        fund(g, g.campaign().buy_price(definition).unwrap() + 10_000);
        let item = id(apply(
            g,
            Command::BuyItem(horde_core::content::ItemDefId::at(definition)),
        ));
        apply(g, Command::Equip { mode, id: item });
    }
}

/// Mara spends her allowance on Guns first, then Dexterity and Health.
fn train_mara(g: &mut Game) {
    while g.campaign().npc.awarded < g.campaign().allowance() {
        fund(g, 10_000);
        apply(g, Command::BuyNpcPoint);
    }
    for training in [
        NpcTraining::Guns,
        NpcTraining::Dexterity,
        NpcTraining::Health,
    ] {
        while g.apply(&Command::TrainNpc(training)).is_ok() {}
    }
}

fn prepare(g: &mut Game, mode: Mode) {
    buy_nodes(g);
    buy_gear(g, mode);
    if mode == Mode::Npc {
        train_mara(g);
    }
}

/// Plays the live root attempt to its end with the smoke bot.
fn play_out(g: &mut Game) -> Outcome {
    for _ in 0..ENCOUNTER_CAP {
        if g.attempt().is_none() {
            break;
        }
        let input = smoke_input(g);
        g.tick(input);
    }
    assert!(g.attempt().is_none(), "encounter exceeded ten minutes");
    let report = g.last_result().unwrap();
    eprintln!(
        "{:?} {:?} stage {} level {} rank {} -> {:?} in {:.0}s, {} kills, {} loot, player level {}",
        report.mode,
        report.country_id,
        report.stage + 1,
        report.level,
        g.campaign().progression_rank(),
        report.outcome,
        report.elapsed,
        report.kills,
        report.loot,
        g.campaign().player.level
    );
    report.outcome
}

/// Rook liberates `country`: entering deploys its next stage, and every
/// settled round continues the journey (a defeat retries the same stage).
fn liberate(g: &mut Game, country: &str) {
    prepare(g, Mode::Personal);
    apply(g, Command::EnterCountry(country.into()));
    for _ in 0..12 {
        play_out(g);
        prepare(g, Mode::Personal);
        if apply(g, Command::ContinueJourney) == CommandOutcome::CountryLiberated {
            assert!(g.campaign().world.cleared.iter().any(|c| c == country));
            return;
        }
    }
    panic!("{country} not liberated in twelve rounds");
}

/// Places traps, then turrets, then walls in the live attempt while supply lasts.
/// With `battle`, fortifies that expedition's attempt instead.
fn fortify(g: &mut Game, battle: Option<u64>) {
    let lanes = match battle {
        Some(id) => {
            g.active_battle(id)
                .unwrap()
                .game()
                .attempt()
                .unwrap()
                .lane_count
        }
        None => g.attempt().unwrap().lane_count,
    };
    for (kind, column) in [
        (StructureKind::Trap, 2),
        (StructureKind::Turret, 1),
        (StructureKind::Wall, 0),
    ] {
        for lane in 0..lanes {
            let _ = g.apply(&match battle {
                Some(battle) => Command::PlaceExpedition {
                    battle,
                    kind,
                    lane,
                    column,
                },
                None => Command::Place { kind, lane, column },
            });
        }
    }
}

/// Buys the best Npc gear for recruit `id` and spends its allowance on Guns,
/// Dexterity and Health.
fn outfit_recruit(g: &mut Game, recruit: u64) {
    for slot in Slot::ALL {
        let best = content()
            .items
            .iter()
            .enumerate()
            .filter(|(n, item)| {
                item.slot == slot
                    && item.tier > 0
                    && item.tier <= g.campaign().unlocked()
                    && g.campaign().can_use_weapon(Mode::Npc, *n)
            })
            .max_by_key(|(n, item)| (item.tier, item.buy_value, std::cmp::Reverse(*n)))
            .map(|(n, _)| n);
        let Some(definition) = best else {
            continue;
        };
        fund(g, g.campaign().buy_price(definition).unwrap() + 10_000);
        let item = id(apply(
            g,
            Command::BuyItem(horde_core::content::ItemDefId::at(definition)),
        ));
        apply(g, Command::EquipRecruit { id: recruit, item });
    }
    loop {
        fund(g, 10_000);
        if g.apply(&Command::BuyRecruitPoint(recruit)).is_err() {
            break;
        }
    }
    for training in [
        NpcTraining::Guns,
        NpcTraining::Dexterity,
        NpcTraining::Health,
    ] {
        while g
            .apply(&Command::TrainRecruit {
                id: recruit,
                training,
            })
            .is_ok()
        {}
    }
}

/// Unpaused idle ticks (no root attempt) until `done` holds.
fn idle_until(g: &mut Game, what: &str, done: impl Fn(&Game) -> bool) {
    apply(g, Command::SetPaused(false));
    for _ in 0..ENCOUNTER_CAP {
        if done(g) {
            return;
        }
        g.tick(Input::default());
    }
    panic!("{what} did not happen within ten minutes");
}

fn battle(g: &Game, id: u64) -> &horde_core::sim::ParallelBattle {
    g.battles().iter().find(|b| b.id == id).unwrap()
}

fn v6_recipe() -> Game {
    let mut g = Game::default();
    g.set_seed(SEED);
    g.staging().strategy().rng = Rng::new(SEED ^ u64::from_be_bytes(*b"STRATEGY"));

    // An earlier Custom-preset round: Normal with gentler hostiles.
    let mut custom = BalanceSettings::for_preset(BalancePreset::Normal);
    custom.enemy_health = 0.9;
    apply(&mut g, Command::SetBalance(custom));
    assert_eq!(g.campaign().balance_settings.preset, BalancePreset::Custom);
    apply(&mut g, Command::ChooseSurvivor(SurvivorId::Rook));
    prepare(&mut g, Mode::Personal);
    apply(&mut g, Command::ChooseStart("MDA".into()));
    apply(&mut g, Command::SetPaused(false));
    assert_eq!(play_out(&mut g), Outcome::Victory);

    // Hard from here on. Moldova, then Romania, unlock the companion,
    // recruitment and parallel expeditions.
    apply(
        &mut g,
        Command::SetBalance(BalanceSettings::for_preset(BalancePreset::Hard)),
    );
    prepare(&mut g, Mode::Personal);
    assert_eq!(
        apply(&mut g, Command::ContinueJourney),
        CommandOutcome::Applied
    );
    play_out(&mut g);
    prepare(&mut g, Mode::Personal);
    assert_eq!(
        apply(&mut g, Command::ContinueJourney),
        CommandOutcome::Applied
    );
    play_out(&mut g);
    prepare(&mut g, Mode::Personal);
    assert_eq!(
        apply(&mut g, Command::ContinueJourney),
        CommandOutcome::CountryLiberated
    );
    buy_nodes(&mut g);
    let guard = id(apply(&mut g, Command::Recruit("MDA".into())));
    let veteran = id(apply(&mut g, Command::Recruit("MDA".into())));
    let scout = id(apply(&mut g, Command::Recruit("MDA".into())));
    liberate(&mut g, "ROU");
    buy_nodes(&mut g);

    // Rook takes Ukraine's five stages (every catalogue level); then the
    // veteran, in the best gear, clears Hungary's first stage on an expedition
    // dispatched under the Normal preset (each deployment keeps its own copy).
    liberate(&mut g, "UKR");
    outfit_recruit(&mut g, veteran);
    apply(
        &mut g,
        Command::SetBalance(BalanceSettings::for_preset(BalancePreset::Normal)),
    );
    let expedition = id(apply(
        &mut g,
        Command::DeploySurvivor {
            id: veteran,
            country: "HUN".into(),
            stage: 0,
        },
    ));
    fortify(&mut g, Some(expedition));
    apply(
        &mut g,
        Command::SetBalance(BalanceSettings::for_preset(BalancePreset::Hard)),
    );
    idle_until(&mut g, "the expedition settling", |g| {
        battle(g, expedition).settled
    });
    let report = battle(&g, expedition).game().last_result().unwrap();
    assert_eq!(report.outcome, Outcome::Victory, "{}", report.cause);

    // Rook takes Hungary's second stage; Mara liberates it behind traps,
    // turrets and walls. Fourteen victories: milestones 5 and 10, not 15.
    prepare(&mut g, Mode::Personal);
    apply(&mut g, Command::EnterCountry("HUN".into()));
    while play_out(&mut g) != Outcome::Victory {
        prepare(&mut g, Mode::Personal);
        apply(&mut g, Command::ContinueJourney);
    }
    loop {
        prepare(&mut g, Mode::Npc);
        apply(
            &mut g,
            Command::Deploy {
                stage: 2,
                mode: Mode::Npc,
            },
        );
        fortify(&mut g, None);
        if play_out(&mut g) == Outcome::Victory {
            break;
        }
    }
    assert_eq!(g.campaign().world.cleared.len(), 4);
    assert_eq!(g.campaign().progression_rank(), 14);
    buy_nodes(&mut g);
    apply(
        &mut g,
        Command::AssignGuard {
            id: guard,
            country: "MDA".into(),
        },
    );
    let mara = g.last_result().unwrap().clone();
    assert!(mara.mode == Mode::Npc && !mara.loot_events.is_empty());

    // The armory: keep an Epic (locked) and a Rare (favourite), save a preset,
    // then sell everything unprotected. The veteran keeps the bought kit it
    // wore home, so the stash holds recruit-equipped items.
    let drop = |g: &Game, quality: Quality| {
        g.campaign()
            .items
            .iter()
            .rev()
            .find(|item| item.quality == Some(quality) && !item.locked && !item.favorite)
            .map(|item| item.id)
            .unwrap_or_else(|| panic!("no {quality:?} drop"))
    };
    let epic = drop(&g, Quality::Epic);
    apply(
        &mut g,
        Command::Lock {
            id: epic,
            locked: true,
        },
    );
    let rare = drop(&g, Quality::Rare);
    apply(
        &mut g,
        Command::Favorite {
            id: rare,
            favorite: true,
        },
    );
    apply(
        &mut g,
        Command::SavePreset {
            name: "Night line".into(),
            mode: Mode::Personal,
        },
    );
    apply(&mut g, Command::SellUnprotected);

    // The scout leaves on a long expedition into Poland and is still out
    // there, with loot already synced home, when the save is cut.
    outfit_recruit(&mut g, scout);
    let live = id(apply(
        &mut g,
        Command::DeploySurvivor {
            id: scout,
            country: "POL".into(),
            stage: 0,
        },
    ));
    fortify(&mut g, Some(live));
    idle_until(&mut g, "a synced expedition drop", |g| {
        g.active_battle(live)
            .and_then(|b| b.game().attempt())
            .is_some_and(|a| a.loot > 0)
    });

    // Rook opens Belarus: the live root attempt, at the current rank, stops
    // at the first tick from 420 that holds enemies, a corpse and a drop.
    prepare(&mut g, Mode::Personal);
    apply(&mut g, Command::EnterCountry("BLR".into()));
    loop {
        let a = g.attempt().unwrap();
        if a.tick >= 420
            && !a.loot_events.is_empty()
            && !a.corpses.is_empty()
            && !a.enemies.is_empty()
        {
            break;
        }
        assert!(a.tick < 3600, "no drop in the first minute");
        let input = smoke_input(&g);
        g.tick(input);
    }
    let a = g.attempt().unwrap();
    eprintln!(
        "root attempt tick {} rank {} enemies {} corpses {} loot {} level {} xp {}",
        a.tick,
        a.progression.rank,
        a.enemies.len(),
        a.corpses.len(),
        a.loot_events.len(),
        g.campaign().player.level,
        g.campaign().player.xp
    );
    assert!(!a.loot_events.is_empty() && !a.enemies.is_empty());
    *g.staging().last_result() = Some(mara);
    g
}

#[test]
#[ignore = "writes fixtures/save_v6.json; set LASTLIGHT_WRITE_SAVE_FIXTURE"]
fn write_save_fixture() {
    let Some(path) = std::env::var_os("LASTLIGHT_WRITE_SAVE_FIXTURE").map(PathBuf::from) else {
        return;
    };
    assert!(!path.exists(), "refusing to overwrite {}", path.display());
    assert_eq!(SAVE_VERSION, 6, "this recipe cuts the v6 fixture");
    let game = v6_recipe();
    game.save(&path).unwrap();
    let _ = std::fs::remove_file(path.with_extension("bak"));
    let bytes = std::fs::read(&path).unwrap();
    eprintln!("wrote {} ({} bytes)", path.display(), bytes.len());
}

// ---------------------------------------------------------------------------
// save_v7.json (W1-63): the schema-freeze file.
// ---------------------------------------------------------------------------

/// Seed of the v7 fixture generation.
const SEED_V7: u64 = 0x5AFE_0007;

/// The sorted, deduplicated item ids of `events`.
fn drop_ids(events: &[horde_core::quality::LootEvent]) -> Vec<u64> {
    let mut ids: Vec<u64> = events.iter().map(|e| e.item_id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The per-quality count of `events`.
fn by_quality(
    events: &[horde_core::quality::LootEvent],
) -> horde_core::kind_map::KindMap<Quality, u32> {
    let mut counts = horde_core::kind_map::KindMap::default();
    for event in events {
        counts[event.quality] += 1;
    }
    counts
}

/// Spawns a regular `kind` hostile in `lane` at `x` and returns its index.
fn spawn(
    g: &mut Game,
    a: &mut horde_core::sim::Attempt,
    kind: EnemyKind,
    lane: u8,
    x: f32,
) -> usize {
    let before = a.enemies.len();
    g.spawn_enemy(a, kind, lane, false);
    assert_eq!(
        a.enemies.len(),
        before + 1,
        "the field has room for {kind:?}"
    );
    a.enemies[before].x = x;
    before
}

/// Ticks the live root defense under the smoke bot until `done` holds; an
/// expedition settles each of its ticks into the parent as it goes.
fn fight_until(g: &mut Game, what: &str, done: impl Fn(&Game) -> bool) {
    while !done(g) {
        let a = g
            .attempt()
            .unwrap_or_else(|| panic!("Mara's defense ended before {what}"));
        assert!(a.tick < 10_800, "no {what} in the first three minutes");
        let input = smoke_input(g);
        g.tick(input);
    }
}

/// The v7 schema-freeze scenario (schema-v7.md section 11). Played through
/// `Game::apply` and the smoke bot: Rook liberates Moldova and Romania, Mara
/// replays a stage, three recruits join, the second wins an expedition into
/// Ukraine and the third is still out in Hungary with a synced drop, the
/// preset becomes Custom with Hard's extras, auto-sell is switched on, and
/// Mara holds Romania's first stage until she has kills, a corpse and a
/// drop. Then `stage_takeover` hands that defense to the first recruit, and
/// every v7 field wave 1 has no writer for is staged at a valid non-default
/// value (wave 2 owns the writers): drop tracking, receipts, summaries,
/// ledgers, the snapshot, the skill modifiers, telemetry and the new
/// enemy-side state.
fn stage_v7_scenario() -> Game {
    let mut g = Game::default();
    g.set_seed(SEED_V7);
    g.staging().strategy().rng = Rng::new(SEED_V7 ^ u64::from_be_bytes(*b"STRATEGY"));

    // Rook liberates Moldova: the companion and recruitment open.
    apply(&mut g, Command::ChooseSurvivor(SurvivorId::Rook));
    prepare(&mut g, Mode::Personal);
    apply(&mut g, Command::ChooseStart("MDA".into()));
    apply(&mut g, Command::SetPaused(false));
    let mut liberated = false;
    for _ in 0..12 {
        play_out(&mut g);
        prepare(&mut g, Mode::Personal);
        if apply(&mut g, Command::ContinueJourney) == CommandOutcome::CountryLiberated {
            liberated = true;
            break;
        }
    }
    assert!(liberated, "Moldova not liberated in twelve rounds");
    buy_nodes(&mut g);
    let recruit = |g: &mut Game| {
        fund(g, 10_000);
        id(apply(g, Command::Recruit("MDA".into())))
    };
    let defender = recruit(&mut g);
    let veteran = recruit(&mut g);
    let scout = recruit(&mut g);

    // Mara replays Moldova's first stage: the main-run Npc record and the
    // settled report (cleared by the final deployment and restored by the
    // staging, `lifecycle.rs:234`) that are later staged, together, as the
    // veteran's main-run takeover with its receipts.
    prepare(&mut g, Mode::Npc);
    apply(
        &mut g,
        Command::Deploy {
            stage: 0,
            mode: Mode::Npc,
        },
    );
    fortify(&mut g, None);
    play_out(&mut g);
    let mara = g.last_result().unwrap().clone();

    // Rook liberates Romania: parallel expeditions open.
    liberate(&mut g, "ROU");
    buy_nodes(&mut g);

    // The veteran clears Ukraine's first stage on an expedition.
    outfit_recruit(&mut g, veteran);
    let settled = id(apply(
        &mut g,
        Command::DeploySurvivor {
            id: veteran,
            country: "UKR".into(),
            stage: 0,
        },
    ));
    fortify(&mut g, Some(settled));
    idle_until(&mut g, "the expedition settling", |g| {
        battle(g, settled).settled
    });
    let report = battle(&g, settled).game().last_result().unwrap();
    eprintln!(
        "expedition {settled}: {:?}, {} kills, {} loot",
        report.outcome, report.kills, report.loot
    );
    assert!(report.loot > 0 && report.kills > 0, "{}", report.cause);
    // The receipt staged later sells the expedition's first drop at the
    // value of its synced stash copy; then the stash is cleared of every
    // unprotected drop, as a player would, to keep the file small.
    let sold = report.loot_events[0].clone();
    let copy = g
        .campaign()
        .items
        .iter()
        .find(|item| {
            item.definition == sold.definition
                && item.level == sold.level
                && item.quality == Some(sold.quality)
        })
        .expect("the synced copy of the expedition's first drop");
    let receipt_coins = g.campaign().item_sale_value(copy.id).unwrap();
    apply(&mut g, Command::SellUnprotected);

    // The scout leaves for Hungary and is still out there, with a drop
    // already synced home, when the save is cut.
    outfit_recruit(&mut g, scout);
    let synced_from = g.campaign().next_item_id;
    let live = id(apply(
        &mut g,
        Command::DeploySurvivor {
            id: scout,
            country: "HUN".into(),
            stage: 0,
        },
    ));
    fortify(&mut g, Some(live));
    idle_until(&mut g, "a synced expedition drop", |g| {
        g.campaign().next_item_id > synced_from
    });
    let synced_to = g.campaign().next_item_id;

    // Hard, then edited: a Custom preset on Hard's extras.
    apply(
        &mut g,
        Command::SetBalance(BalanceSettings::for_preset(BalancePreset::Hard)),
    );
    let mut custom = BalanceSettings::for_preset(BalancePreset::Hard);
    custom.enemy_health = 1.1;
    apply(&mut g, Command::SetBalance(custom));
    assert_eq!(g.campaign().balance_settings.preset, BalancePreset::Custom);
    assert_eq!(g.campaign().balance_extras_basis, BalancePreset::Hard);
    apply(
        &mut g,
        Command::SetAutoSell(AutoSellRule {
            enabled: true,
            sell_below: Quality::Rare,
            slots: [true, true, false, true],
            keep_upgrades: true,
            apply_to_npc_runs: false,
        }),
    );

    // Mara holds Romania's first stage behind the fortifications until she
    // has kills, a corpse and a drop; then the first recruit takes over.
    prepare(&mut g, Mode::Npc);
    apply(
        &mut g,
        Command::Deploy {
            stage: 0,
            mode: Mode::Npc,
        },
    );
    fortify(&mut g, None);
    fight_until(&mut g, "a drop", |g| {
        g.attempt()
            .is_some_and(|a| a.kills > 0 && !a.loot_events.is_empty() && !a.corpses.is_empty())
    });
    assert!(
        !battle(&g, live).settled,
        "the Hungarian expedition is still out"
    );
    g.staging().stage_takeover(defender).unwrap();
    *g.staging().last_result() = Some(mara);

    stage_root_report(&mut g, veteran);
    stage_root_attempt(&mut g);
    stage_expeditions(&mut g, settled, live, synced_from..synced_to, receipt_coins);
    stage_campaign(&mut g, veteran);
    trim_logs(&mut g);
    let a = g.attempt().unwrap();
    eprintln!(
        "root attempt tick {} rank {} enemies {} corpses {} loot {} level {}",
        a.tick,
        a.progression.rank,
        a.enemies.len(),
        a.corpses.len(),
        a.loot_events.len(),
        g.campaign().player.level
    );
    g
}

/// The restored root report, staged as the veteran's main-run takeover (the
/// seat its run record names, see [`stage_campaign`]): its drops, their
/// qualities, an auto-sold, level-up and milestone receipt, a Regional boss
/// rank and per-archetype telemetry.
fn stage_root_report(g: &mut Game, veteran: u64) {
    let level = g.campaign().player.level;
    let report = g.staging().last_result().as_mut().unwrap();
    assert!(!report.loot_events.is_empty() && report.kills > 0);
    assert_eq!(report.mode, Mode::Npc);
    // The seat has no test-support writer; it is set through the saved form.
    let mut saved = serde_json::to_value(&*report).unwrap();
    saved["defender"] = serde_json::json!({ "Recruit": veteran });
    *report = serde_json::from_value(saved).unwrap();
    report.drop_ids = drop_ids(&report.loot_events);
    report.loot_by_quality = by_quality(&report.loot_events);
    let granted = report.loot_events[0].clone();
    report.progression_rewards = ProgressionRewards {
        level_from: level - 1,
        level_to: level,
        level_coins: 120,
        milestones: vec![MilestoneReward {
            rank: 5,
            coins: 250,
            item_id: Some(granted.item_id),
            item_definition: Some(granted.definition),
        }],
    };
    let sold = report.loot_events[1].clone();
    let mut sold_by_quality = horde_core::kind_map::KindMap::default();
    sold_by_quality[sold.quality] = 1;
    report.auto_sold = AutoSoldReceipt {
        count: 1,
        coins: TAKEOVER_SOLD_COINS,
        by_quality: sold_by_quality,
        lines: vec![AutoSoldLine {
            definition: sold.definition,
            level: sold.level,
            quality: sold.quality,
            coins: TAKEOVER_SOLD_COINS,
        }],
        kept_on_overflow: 0,
    };
    report.boss_rank = BossRank::Regional;
    let shambler = horde_core::enemy_catalog::resolve_kind(EnemyKind::Shambler).archetype_id;
    let t = &mut report.telemetry;
    t.spawned_by_archetype =
        BTreeMap::from([(shambler.clone(), report.kills + 2), ("spitter".into(), 1)]);
    t.kills_by_archetype = BTreeMap::from([(shambler.clone(), report.kills)]);
    t.damage_taken_by_archetype = BTreeMap::from([(shambler, 40), ("spitter".into(), 12)]);
    t.elites_spawned = 2;
    t.elites_killed = 1;
    t.hazard_damage_taken = 12;
    t.boss_phase_ticks = vec![2400, 3000];
}

/// The live takeover defense: its drops at the frozen drop tier, a Capstone
/// snapshot on wave tier 11, per-class skill modifiers, the new hostile
/// kinds (an armored, stagger-immune elite, a spitter mid-windup, a splitter
/// and its child, a group of exploders) and a phased boss, with their
/// telemetry.
fn stage_root_attempt(g: &mut Game) {
    let mut a = g.staging().attempt_slot().take().unwrap();
    a.drop_ids = drop_ids(&a.loot_events);
    a.drop_tier = Some(content().max_drop_tier());

    let rank = a.progression.rank;
    let p = &mut a.progression;
    p.elite_rate_bp = (200 + 65 * rank.saturating_sub(3)).min(1500) * 150 / 100;
    p.front_ramp = 1.125;
    p.boss_rank = BossRank::Capstone;
    p.boss_phases = 2;
    p.boss_armor_multiplier = 1.5;
    p.npc_boss_armor_add = 2;
    p.boss_health_multiplier = 1.4;
    p.boss_reward_multiplier = 1.4;
    p.boss_damage_multiplier = 1.25;
    p.wave_tier = 11;

    let class = a.profile.weapon.class;
    let m = &mut a.profile.modifiers;
    m.class_modifiers = BTreeMap::from([(
        class,
        ClassModifiers {
            damage_multiplier: 1.25,
            reload_multiplier: 0.875,
            cadence_multiplier: 0.9375,
            weak_bonus_add: 0.125,
        },
    )]);
    m.armor_penetration = 4;
    m.hazard_damage_multiplier = 0.75;

    let tick = a.tick;
    let width = a.width_yards;
    let x = |n: f32| DEFENSE_LINE + (width - DEFENSE_LINE - 1.0) * (0.45 + 0.07 * n);
    let elite = spawn(g, &mut a, EnemyKind::Shambler, 1, x(0.0));
    {
        let r = a.enemies[elite].resolved.as_mut().unwrap();
        r.elite = Some(EliteMark {
            affix_id: "armored".into(),
            name: format!("Armored {}", r.name),
        });
        r.armor += 2;
        r.ranged_armor = 6;
        r.npc_ranged_armor = 2;
        r.stagger_immune = true;
        r.tags = vec!["elite".into(), "heavy".into()];
    }
    let shambler = a.enemies[elite]
        .resolved
        .as_ref()
        .unwrap()
        .archetype_id
        .clone();
    let spitter = spawn(g, &mut a, EnemyKind::Shambler, 2, DEFENSE_LINE + 6.0);
    {
        let e = &mut a.enemies[spitter];
        let r = e.resolved.as_mut().unwrap();
        r.archetype_id = "spitter".into();
        r.name = "Spitter".into();
        r.reach = 6.0;
        r.tags = vec!["ranged".into()];
        e.contact_since = None;
        e.attack_phase = EnemyPhase::Windup;
        e.windup_total = 90;
        e.attack_left = 40;
    }
    let splitter = spawn(g, &mut a, EnemyKind::Brute, 3, x(1.0));
    {
        let r = a.enemies[splitter].resolved.as_mut().unwrap();
        r.archetype_id = "splitter".into();
        r.name = "Splitter".into();
        r.death_effect = Some(DeathEffect::Split {
            archetype_id: "splitling".into(),
            count: 2,
        });
    }
    let child = spawn(g, &mut a, EnemyKind::Runner, 3, x(2.0));
    {
        let e = &mut a.enemies[child];
        e.split_child = true;
        let r = e.resolved.as_mut().unwrap();
        r.archetype_id = "splitling".into();
        r.name = "Splitling".into();
    }
    let exploder = spawn(g, &mut a, EnemyKind::Runner, 4, x(3.0));
    {
        let r = a.enemies[exploder].resolved.as_mut().unwrap();
        r.archetype_id = "exploder".into();
        r.name = "Exploder".into();
        r.group_size = 3;
        r.death_effect = Some(DeathEffect::Burst {
            radius: 1.5,
            damage: 40,
            structure_damage: 60,
            npc_dice: 2,
            npc_add: 2,
            fuse: true,
        });
    }
    if !a.boss_spawned {
        let center = a.lane_count / 2;
        g.spawn_enemy(&mut a, EnemyKind::Brute, center, true);
        a.enemies.last_mut().expect("the boss arrives").x = a.width_yards - 0.6;
        a.boss_spawned = true;
        a.telemetry.boss_arrival_tick = Some(tick);
    }
    let phase_tick = tick - 30;
    let boss = a.enemies.iter_mut().find(|e| e.boss).unwrap();
    boss.boss_state = Some(BossState {
        profile: BossPhaseProfile {
            name: "Second wind".into(),
            speed_mult: 1.25,
            armor_mult: 0.75,
            call_interval_mult: 0.5,
            siege_recovery_mult: 0.5,
            hold_ticks: 60,
        },
        transitions: 2,
        entered: 1,
        hold_until: phase_tick + 60,
    });

    let mut spawned: BTreeMap<String, u32> = BTreeMap::new();
    for e in a.enemies.iter().filter(|e| !e.boss) {
        if let Some(r) = &e.resolved {
            *spawned.entry(r.archetype_id.clone()).or_default() += 1;
        }
    }
    *spawned.entry(shambler.clone()).or_default() += a.kills;
    let t = &mut a.telemetry;
    t.spawned_by_archetype = spawned;
    t.kills_by_archetype = BTreeMap::from([(shambler.clone(), a.kills)]);
    t.damage_taken_by_archetype = BTreeMap::from([(shambler, 25), ("spitter".into(), 9)]);
    t.elites_spawned = 2;
    t.elites_killed = 1;
    t.hazard_damage_taken = 9;
    t.boss_phase_ticks = vec![phase_tick];
    // The staged spawns queued presentation events; like the core review
    // fixtures, the staged field starts with an empty queue.
    a.events.clear();
    *g.staging().attempt_slot() = Some(a);
}

/// Coins of the takeover run's one auto-sold drop (its report and record).
const TAKEOVER_SOLD_COINS: u64 = 18;

/// The settled expedition's receipt and unacknowledged notice, and the live
/// one's pending auto-sale and a pending record awaiting its sync. That
/// record is the live expedition's own: Hungary at its stage, under an
/// attempt id reserved from the shared counter (no game has issued it, so no
/// record or report holds it), unstamped (the sync stamps the worker and the
/// recruit), and a Withdrawal, so the sync appends it and clears nothing.
fn stage_expeditions(
    g: &mut Game,
    settled: u64,
    live: u64,
    synced: std::ops::Range<u64>,
    coins: u64,
) {
    let template = g
        .campaign()
        .run_history
        .iter()
        .rev()
        .find(|r| r.worker_id == Some(settled))
        .cloned()
        .expect("the expedition's run record");
    // The expedition's synced drops: issued while it ran, not the main run's.
    let root_drops = g.attempt().unwrap().drop_ids.clone();
    let pending: Vec<u64> = g
        .campaign()
        .items
        .iter()
        .map(|item| item.id)
        .filter(|id| synced.contains(id) && !root_drops.contains(id))
        .collect();
    assert!(!pending.is_empty(), "a synced drop is still in the stash");
    let sold = battle(g, settled).game().last_result().unwrap().loot_events[0].clone();
    let attempt_id = g.next_attempt_id();
    g.set_next_attempt_id(attempt_id + 1);
    let battles = g.staging().battles();

    let b = battles.iter_mut().find(|b| b.id == settled).unwrap();
    let report = b.game_mut().last_result_mut().as_mut().unwrap();
    report.drop_ids = drop_ids(&report.loot_events);
    report.loot_by_quality = by_quality(&report.loot_events);
    let mut sold_by_quality = horde_core::kind_map::KindMap::default();
    sold_by_quality[sold.quality] = 1;
    report.auto_sold = AutoSoldReceipt {
        count: 1,
        coins,
        by_quality: sold_by_quality,
        lines: vec![AutoSoldLine {
            definition: sold.definition,
            level: sold.level,
            quality: sold.quality,
            coins,
        }],
        kept_on_overflow: 0,
    };
    let summary = ExpeditionSummary {
        outcome: Some(report.outcome),
        stages_cleared: u32::from(report.outcome == Outcome::Victory),
        liberated: false,
        fronts: Vec::new(),
        coins: report.coins,
        loot_count: report.loot,
        acknowledged: false,
    };
    b.summary = Some(summary);

    let b = battles.iter_mut().find(|b| b.id == live).unwrap();
    b.pending_auto_sell = pending;
    let a = b.game().attempt().expect("the live expedition's attempt");
    let mut record = template;
    record.id = 0;
    record.attempt_id = attempt_id;
    record.worker_id = None;
    record.recruit_id = None;
    record.outcome = Outcome::Withdrawal;
    record.cause = "Recalled to camp with all banked rewards".into();
    record.country_id = Some(b.country_id.clone());
    record.country_name = a.country_name.clone();
    record.stage = b.stage;
    record.template = a.level;
    record.lane_count = a.lane_count;
    record.width_yards = a.width_yards;
    record.completion_xp = 0;
    record.auto_sold_count = 1;
    record.auto_sold_coins = coins;
    record.level_grant_coins = 60;
    record.milestone_coins = 150;
    record.elite_kills = 1;
    record.boss_ttk_ticks = Some(record.duration_ticks / 2);
    record.boss_rank = BossRank::Regional;
    b.game_mut().pending_runs_mut().push(record);
}

/// The campaign ledgers, the next defender, a main-run takeover record with
/// its receipts (those of the restored root report, the same attempt), a
/// folded outcome and a containment.
fn stage_campaign(g: &mut Game, veteran: u64) {
    let rank = g.campaign().progression_rank();
    let name = g
        .strategy()
        .roster
        .iter()
        .find(|r| r.id == veteran)
        .unwrap()
        .npc
        .name
        .clone();
    let report = g.last_result().unwrap().clone();
    let c = g.staging().campaign();
    c.next_defender = Some(DefenderId::Recruit(veteran));
    c.milestones_claimed = [5, 10, 15, 20].into_iter().filter(|r| *r <= rank).collect();
    assert!(!c.milestones_claimed.is_empty(), "rank {rank}");
    assert!(c.player.level >= 2);
    c.player.level_grant_paid = c.player.level;
    let mut records = c.run_history.to_vec();
    let r = records
        .iter_mut()
        .find(|r| r.worker_id.is_none() && r.attempt_id == report.attempt_id)
        .expect("the root report's main-run record");
    assert_eq!(r.mode, Mode::Npc);
    assert!(r.loot > 0 && r.kills > 0);
    r.recruit_id = Some(veteran);
    r.defender_name = name;
    r.auto_sold_count = report.auto_sold.count;
    r.auto_sold_coins = report.auto_sold.coins;
    r.level_grant_coins = report.progression_rewards.level_coins;
    r.milestone_coins = report
        .progression_rewards
        .milestones
        .iter()
        .map(|m| m.coins)
        .sum();
    r.elite_kills = report.telemetry.elites_killed;
    r.boss_ttk_ticks = Some(r.duration_ticks / 3);
    r.boss_rank = report.boss_rank;
    c.run_history = records.into();
    let strategy = g.staging().strategy();
    strategy.folded_outcomes = 1;
    strategy
        .roster
        .iter_mut()
        .find(|r| r.id == veteran)
        .unwrap()
        .containments = 2;
}

/// Combat log lines kept per attempt and report. The Npc logs (80 lines
/// each) are presentation history no rule reads; trimming them keeps the
/// file within its 96 KB budget.
const V7_LOG_LINES: usize = 4;

fn trim_logs(g: &mut Game) {
    fn trim(log: &mut Vec<String>) {
        let excess = log.len().saturating_sub(V7_LOG_LINES);
        log.drain(..excess);
    }
    fn trim_game(g: &mut Game) {
        if let Some(a) = g.attempt_mut() {
            trim(&mut a.log);
        }
        if let Some(r) = g.last_result_mut() {
            trim(&mut r.log);
        }
    }
    trim_game(g);
    for b in g.battles_mut() {
        trim_game(b.game_mut());
    }
}

#[test]
#[ignore = "writes fixtures/save_v7.json; set LASTLIGHT_WRITE_SAVE_V7_FIXTURE"]
fn write_save_v7_fixture() {
    let Some(path) = std::env::var_os("LASTLIGHT_WRITE_SAVE_V7_FIXTURE").map(PathBuf::from) else {
        return;
    };
    assert!(!path.exists(), "refusing to overwrite {}", path.display());
    assert_eq!(SAVE_VERSION, 7, "this recipe cuts the v7 fixture");
    let game = stage_v7_scenario();
    game.save(&path).unwrap();
    let _ = std::fs::remove_file(path.with_extension("bak"));
    let bytes = std::fs::read(&path).unwrap();
    // The cut loads, validates and saves back to the very same payload.
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let loaded = Game::decode_save_generation(&bytes).unwrap();
    assert_eq!(
        loaded.to_save_json().unwrap(),
        envelope["payload"].as_str().unwrap()
    );
    eprintln!("wrote {} ({} bytes)", path.display(), bytes.len());
}
