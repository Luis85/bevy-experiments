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
//! Run: `cargo test --locked -p horde_core --test save_fixtures -- --ignored`.
#![allow(clippy::print_stderr)] // the writers report progress and the file size
use horde_core::balance::{BalancePreset, BalanceSettings};
use horde_core::campaign::NpcTraining;
use horde_core::command::{Command, CommandOutcome};
use horde_core::content::{Mode, Slot, StructureKind, content};
use horde_core::quality::Quality;
use horde_core::rules::Rng;
use horde_core::sim::{Game, Input, Outcome, SAVE_VERSION, smoke_input};
use horde_core::skill_catalog::skills;
use horde_core::survivors::SurvivorId;
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
