use super::{recording::*, replay::*, *};
use crate::{
    profile::ProfilePaths,
    runner::{ControlFrame, RuntimeCtx, flush, step},
    settings::Settings,
};
use horde_core::{
    campaign::SaleBlock,
    command::{Command, CommandError, CommandOutcome},
    content::{Mode, StructureKind},
    sim::Input,
};

/// A scratch folder removed on drop (the runner may autosave into it).
struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("lastlight-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn camp_game() -> Game {
    horde_core::catalog::init_catalogs().unwrap();
    let mut game = Game::default();
    game.campaign_mut().legacy_features = true;
    game.campaign_mut()
        .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
        .unwrap();
    game.campaign_mut()
        .choose_survivor(horde_core::survivors::SurvivorId::Rook)
        .unwrap();
    game.campaign_mut().choose_start_country("DEU").unwrap();
    game
}

/// Drives the real runner passes for a scripted session and returns the final game.
fn scripted_session(game: &mut Game, runtime: &mut Runtime, scratch: &Scratch) -> Vec<u64> {
    let settings = Settings::default();
    let paths = ProfilePaths::in_directory(scratch.0.clone());
    let (mut notices, mut cues) = (Vec::new(), Vec::new());
    let mut pass = |game: &mut Game, runtime: &mut Runtime, frame: u32| {
        runtime.begin_frame(frame);
        let mut ctx = RuntimeCtx {
            game,
            runtime,
            settings: &settings,
            paths: &paths,
            notices: &mut notices,
            cues: &mut cues,
            game_changed: false,
            perf: None,
        };
        flush(&mut ctx);
        step(&mut ctx);
    };
    let mut frame = 0;
    let mut next = |game: &mut Game, runtime: &mut Runtime| {
        frame += 1;
        pass(game, runtime, frame);
    };
    runtime.enqueue(
        Command::Deploy {
            stage: 0,
            mode: Mode::Personal,
        },
        "",
    );
    runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    let mut ticks_after = Vec::new();
    for tick in 0..240u32 {
        match tick {
            10 => {
                runtime.enqueue(
                    Command::Place {
                        kind: StructureKind::Wall,
                        lane: 1,
                        column: 4,
                    },
                    "",
                );
            }
            // Pause, then single-step three ticks through the runner.
            60 => {
                runtime.enqueue(Command::SetPaused(true), "");
            }
            62..=64 => runtime.request_step(),
            70 => {
                runtime.enqueue(Command::SetPaused(false), "");
            }
            _ => {}
        }
        // Hold fire from tick 20 and sweep the aim, so input lines vary.
        let held = Input {
            fire: tick >= 20,
            aim: [15.0 + (tick / 30) as f32, 5.5],
            ..Input::default()
        };
        runtime.publish(ControlFrame {
            enabled: true,
            held,
            ..ControlFrame::default()
        });
        next(game, runtime);
        ticks_after.push(runtime.debug_snapshot().simulated_ticks);
    }
    ticks_after
}

#[test]
fn a_session_recorded_through_the_runner_replays_to_the_same_digest() {
    let scratch = Scratch::new("record-replay");
    let path = scratch.0.join("session.jsonl");
    let mut game = camp_game();
    let mut runtime = Runtime::default();
    runtime.start_recording(Recorder::create(&path, &game, 0).unwrap());

    let ticks_after = scripted_session(&mut game, &mut runtime, &scratch);
    let added = |frame: usize| ticks_after[frame] - ticks_after[frame - 1];
    assert_eq!(added(61), 0, "paused: no tick");
    for frame in 62..=64 {
        assert_eq!(
            added(frame),
            1,
            "F5 step at frame {frame} adds exactly one tick"
        );
    }
    assert_eq!(added(65), 0, "re-paused after the step");
    assert!(
        game.attempt().is_some_and(|a| a.tick > 100),
        "the scripted session really simulated ticks"
    );
    assert!(runtime.stop_recording(&game, "test"));
    let expected = game.state_digest();
    runtime.finish_storage_for_test();

    let lines = read_lines(&path).unwrap();
    assert!(matches!(lines.first(), Some(Line::Header(_))));
    let commands = lines
        .iter()
        .filter(|l| matches!(l, Line::Command { .. }))
        .count();
    let inputs = lines
        .iter()
        .filter(|l| matches!(l, Line::Input { .. }))
        .count();
    assert!(
        commands >= 6,
        "deploy, place, pauses and step toggles: {commands}"
    );
    assert!(
        (3..120).contains(&inputs),
        "inputs are written only when they change, not every tick: {inputs}"
    );

    let report = replay_file(&path).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.digest, expected);
    assert!(report.matches_recording());
    assert!(report.ticks > 100);
}

#[test]
fn replay_detects_a_tampered_recording() {
    let scratch = Scratch::new("record-tamper");
    let path = scratch.0.join("session.jsonl");
    let mut game = camp_game();
    let mut runtime = Runtime::default();
    runtime.start_recording(Recorder::create(&path, &game, 0).unwrap());
    scripted_session(&mut game, &mut runtime, &scratch);
    runtime.stop_recording(&game, "test");
    runtime.finish_storage_for_test();

    let mut lines = read_lines(&path).unwrap();
    lines.retain(|l| {
        !matches!(
            l,
            Line::Command {
                command: Command::Deploy { .. },
                ..
            }
        )
    });
    let report = replay(lines).unwrap();
    assert!(
        !report.matches_recording(),
        "dropping the deploy command changes the outcome"
    );
}

#[test]
fn header_mismatches_warn_and_garbage_is_an_error() {
    horde_core::catalog::init_catalogs().unwrap();
    let game = Game::default();
    let scratch = Scratch::new("record-header");
    let path = scratch.0.join("empty.jsonl");
    let recorder = Recorder::create(&path, &game, 0).unwrap();
    recorder.finish(0, &game, "test");
    let mut lines = read_lines(&path).unwrap();
    let Some(Line::Header(header)) = lines.first_mut() else {
        panic!("header first");
    };
    header.content_fingerprint = hex(1);
    header.ruleset += 1;
    let report = replay(lines).unwrap();
    assert_eq!(report.warnings.len(), 2, "{:?}", report.warnings);
    assert!(report.matches_recording());

    assert!(replay(vec![]).is_err());
    std::fs::write(&path, "not json\n").unwrap();
    assert!(read_lines(&path).is_err());
}

#[test]
fn digest_is_stable_and_state_sensitive() {
    horde_core::catalog::init_catalogs().unwrap();
    let mut game = Game::default();
    let first = game.state_digest();
    assert_eq!(first, game.clone().state_digest());
    game.set_seed(game.seed() ^ 1);
    assert_ne!(first, game.state_digest());
}

#[test]
fn an_unusable_step_request_is_dropped_and_the_ordinary_tick_still_runs() {
    let scratch = Scratch::new("record-step");
    let mut game = camp_game();
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    game.set_paused(false);
    let mut runtime = Runtime::default();
    runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    runtime.request_step();
    let settings = Settings::default();
    let paths = ProfilePaths::in_directory(scratch.0.clone());
    let (mut notices, mut cues) = (Vec::new(), Vec::new());
    let mut ctx = RuntimeCtx {
        game: &mut game,
        runtime: &mut runtime,
        settings: &settings,
        paths: &paths,
        notices: &mut notices,
        cues: &mut cues,
        game_changed: false,
        perf: None,
    };
    step(&mut ctx);
    assert_eq!(
        runtime.debug_snapshot().simulated_ticks,
        1,
        "ordinary tick ran"
    );
    assert_eq!(
        runtime.debug_snapshot().step_requests,
        0,
        "request consumed"
    );
    assert_eq!(game.attempt().unwrap().tick, 1);
}

#[test]
fn panel_reports_the_live_attempt() {
    let mut game = camp_game();
    game.deploy_country_stage(0, Mode::Personal).unwrap();
    let text = sim_panel_lines(&game, &Runtime::default(), false).join("\n");
    for needle in [
        "seed",
        "attempt",
        "enemies",
        "corpses",
        "event seq",
        "recording",
    ] {
        assert!(text.contains(needle), "{needle} in {text}");
    }
}

#[test]
fn dump_writes_the_save_payload() {
    let scratch = Scratch::new("record-dump");
    horde_core::catalog::init_catalogs().unwrap();
    let game = Game::default();
    let path = dump_game(&game, &scratch.0).unwrap();
    assert!(path.starts_with(scratch.0.join("dumps")));
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&game).unwrap());
}

/// `tests/fixtures/recording_v6_equipped_sale.jsonl`: a fresh new game
/// recorded through the runner on the v6 build (parent commit 3d492cd). Rook
/// starts in Germany and fights with aim quantised to whole yards until the
/// banked coins buy the cheapest outfit, retreats, buys it, equips it and
/// sells the equipped item (accepted under ruleset 1), then the recording
/// ends with its digest.
const EQUIPPED_SALE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/recording_v6_equipped_sale.jsonl"
);

/// The cheapest tier-1 outfit a fresh campaign may buy.
fn cheapest_outfit(game: &Game) -> (horde_core::content::ItemDefId, u64) {
    horde_core::content::content()
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.tier == 1 && item.purchasable() && item.slot == horde_core::content::Slot::Outfit
        })
        .map(|(n, item)| (item.id.clone(), game.campaign().buy_price(n).unwrap()))
        .min_by_key(|(id, price)| (*price, id.as_str().to_owned()))
        .unwrap()
}

#[test]
#[ignore = "writes tests/fixtures/recording_v6_equipped_sale.jsonl; set LASTLIGHT_WRITE_RECORDING_FIXTURE"]
fn write_equipped_sale_recording() {
    let Some(path) = std::env::var_os("LASTLIGHT_WRITE_RECORDING_FIXTURE").map(PathBuf::from)
    else {
        return;
    };
    assert!(!path.exists(), "refusing to overwrite {}", path.display());
    assert_eq!(
        horde_core::sim::SAVE_VERSION,
        6,
        "this recording is cut on v6"
    );
    horde_core::catalog::init_catalogs().unwrap();
    let scratch = Scratch::new("record-equipped-sale");
    let mut game = Game::default();
    let mut runtime = Runtime::default();
    runtime.start_recording(Recorder::create(&path, &game, 0).unwrap());
    let settings = Settings::default();
    let paths = ProfilePaths::in_directory(scratch.0.clone());
    let (mut notices, mut cues) = (Vec::new(), Vec::new());
    let mut frame = 0;
    let mut pass = |game: &mut Game, runtime: &mut Runtime, held: Input| {
        frame += 1;
        runtime.begin_frame(frame);
        runtime.publish(ControlFrame {
            enabled: true,
            held,
            ..ControlFrame::default()
        });
        let mut ctx = RuntimeCtx {
            game,
            runtime,
            settings: &settings,
            paths: &paths,
            notices: &mut notices,
            cues: &mut cues,
            game_changed: false,
            perf: None,
        };
        flush(&mut ctx);
        step(&mut ctx);
    };
    let (outfit, price) = cheapest_outfit(&game);
    for command in [
        Command::ChooseSurvivor(horde_core::survivors::SurvivorId::Rook),
        Command::ChooseStart("DEU".into()),
        Command::SetPaused(false),
    ] {
        runtime.enqueue(command, "");
    }
    pass(&mut game, &mut runtime, Input::default());
    while game.campaign().coins < price {
        assert!(
            game.attempt().is_some(),
            "the defense ended before the outfit was affordable"
        );
        let mut held = horde_core::sim::smoke_input(&game);
        held.aim = [held.aim[0].round(), held.aim[1].floor() + 0.5];
        pass(&mut game, &mut runtime, held);
    }
    runtime.enqueue(Command::Withdraw, "");
    runtime.enqueue(Command::BuyItem(outfit), "");
    pass(&mut game, &mut runtime, Input::default());
    let item = game.campaign().items.last().unwrap().id;
    runtime.enqueue(
        Command::Equip {
            mode: Mode::Personal,
            id: item,
        },
        "",
    );
    pass(&mut game, &mut runtime, Input::default());
    assert_eq!(
        game.campaign().player.loadout[2],
        item,
        "the outfit is equipped"
    );
    runtime.enqueue(Command::SellItem(item), "");
    pass(&mut game, &mut runtime, Input::default());
    assert!(
        game.campaign().item(item).is_none(),
        "ruleset 1 sells the equipped outfit"
    );
    assert!(runtime.stop_recording(&game, "fixture"));
    runtime.finish_storage_for_test();
}

/// A recording made by an older build loads its initial state the way that
/// build's save would load here: migrated, reconciled and validated through
/// `Game::from_payload`. A current-version initial state is taken as recorded.
#[test]
fn v6_recording_initial_state_goes_through_from_payload() {
    use horde_core::sim::{RULESET_VERSION, SAVE_VERSION};
    horde_core::catalog::init_catalogs().unwrap();
    let game = Game::default();
    let header = |version: u32, initial: serde_json::Value| {
        Line::Header(Header {
            format: FORMAT,
            seed: game.seed(),
            version,
            ruleset: RULESET_VERSION,
            content_fingerprint: hex(horde_core::catalog::content_fingerprint()),
            start_tick: 0,
            initial,
        })
    };
    // A pre-journey save: reconciling it on load derives the journey flags,
    // so the loaded and the raw state differ.
    let mut initial = serde_json::to_value(&game).unwrap();
    initial["campaign"]["journey_revision"] = 0.into();
    let loaded = Game::from_payload(initial.clone(), 6).unwrap();
    let raw: Game = serde_json::from_value(initial.clone()).unwrap();
    assert_ne!(loaded.state_digest(), raw.state_digest());
    let old = replay(vec![header(6, initial.clone())]).unwrap();
    assert_eq!((old.ticks, old.digest), (0, loaded.state_digest()));
    let current = replay(vec![header(SAVE_VERSION, initial)]).unwrap();
    assert_eq!(current.digest, raw.state_digest());
    // An old initial state that cannot be migrated is an error.
    let mut broken = serde_json::to_value(&game).unwrap();
    broken["battles"] = serde_json::json!([{"game": {}}]);
    let error = replay(vec![header(6, broken)]).unwrap_err();
    assert!(error.contains("recorded initial state"), "{error}");
}

/// Replaces `v6_fresh_recording_reproduces_before_strict_sale` (W1-20): a
/// ruleset 1 recording that sells an equipped item is not expected to
/// reproduce under ruleset 2 (strict `SellItem`). Replay still loads the v6
/// initial state, applies every command and reaches the end line; the
/// recorded sale is now a typed rejection, so the digest differs, and the
/// warnings name both the older ruleset and the older save version.
#[test]
fn pre_strict_recording_diverges_with_ruleset_warning() {
    horde_core::catalog::init_catalogs().unwrap();
    let lines = read_lines(Path::new(EQUIPPED_SALE)).unwrap();
    let Some(Line::Header(header)) = lines.first() else {
        panic!("header first");
    };
    assert_eq!((header.format, header.version, header.ruleset), (1, 6, 1));
    let commands: Vec<_> = lines
        .iter()
        .filter_map(|line| match line {
            Line::Command { command, .. } => Some(command.clone()),
            _ => None,
        })
        .collect();
    let [
        ..,
        Command::BuyItem(bought),
        Command::Equip {
            mode: equip_mode,
            id: equipped,
        },
        Command::SellItem(sold),
    ] = commands.as_slice()
    else {
        panic!("the recording ends buy, equip, sell: {commands:?}");
    };
    assert_eq!(equipped, sold, "the sold item is the equipped one");
    let Some(Line::End { digest, .. }) = lines.last() else {
        panic!("the recording ends with its digest");
    };
    let recorded = digest.clone();
    let sold = *sold;
    let mut without_sale = lines.clone();
    let sale = without_sale
        .iter()
        .rposition(|line| {
            matches!(
                line,
                Line::Command {
                    command: Command::SellItem(_),
                    ..
                }
            )
        })
        .unwrap();
    without_sale.remove(sale);
    let (game, report) = replay_game(lines).unwrap();
    assert_eq!(
        report.commands,
        commands.len(),
        "replay applies every command"
    );
    assert_eq!(report.recorded_digest.as_ref(), Some(&recorded));
    assert_ne!(
        hex(report.digest),
        recorded,
        "ruleset 2 refuses the equipped sale"
    );
    assert!(!report.matches_recording());
    // Ruleset 2 refuses a sale as a typed SaleBlocked rejection that changes
    // nothing. Since W2-39 the replayed defender falls before the recorded
    // withdrawal: Germany, the recording's start, plays difficulty 1.04
    // instead of 1.06 (the stage-count bands of design-enemy-balance §8,
    // content), so the defeat pays 23 coins, the recorded buy of a 60-coin
    // padded vest is refused for want of coins, and the recorded Equip and
    // SellItem name an item that was never bought.
    let mut again = game.clone();
    assert_eq!(
        again.apply(&Command::SellItem(sold)),
        Err(CommandError::SaleBlocked(SaleBlock::Unknown))
    );
    assert_eq!(again.state_digest(), report.digest);
    assert!(
        game.campaign().item(sold).is_none(),
        "the recorded buy was refused"
    );
    // The recorded buy, equip and sale on that state, with the coins the
    // buy needs: ruleset 2 refuses the equipped sale and keeps the item.
    again.campaign_mut().coins += 1_000;
    let Ok(CommandOutcome::Id(vest)) = again.apply(&Command::BuyItem(bought.clone())) else {
        panic!("the recorded {bought} can be bought with coins in hand");
    };
    again
        .apply(&Command::Equip {
            mode: *equip_mode,
            id: vest,
        })
        .unwrap();
    let before = again.state_digest();
    assert_eq!(
        again.apply(&Command::SellItem(vest)),
        Err(CommandError::SaleBlocked(SaleBlock::EquippedPersonal))
    );
    assert_eq!(again.state_digest(), before);
    assert!(
        again.campaign().item(vest).is_some(),
        "the equipped item is kept"
    );
    // The play before the sale is pinned here, because the recorded End
    // digest cannot vouch for it. Rebuilding the ruleset 1 sale legally
    // (equip the personal starter outfit, then sell) reaches
    // 107ff7ceb7fde8d3, the same state as redoing the old unequip-and-replace
    // sale by hand, and not the recorded 9308db57c71cb2f0. So the 2,592
    // recorded combat ticks no longer replay as they did on v6. The expected
    // cause is W1-69 (enemies never block each other), whose re-bless clause
    // names this replay. With this pin, any further drift in the replayed
    // combat, retreat, buy or equip fails the test, not only the sale.
    // W2-05 (per-archetype telemetry, class T) moved the pin from
    // 81559980e3a904d7: the report now carries the archetype maps. With those
    // three maps removed the replayed state digests to 81559980e3a904d7
    // again, so no combat moved. W2-06 (drop tracking, class F) moved it from
    // 0da44f5dc0a31d2c: attempts save their frozen drop_tier and drop_ids,
    // and reports their drop_ids and loot_by_quality. With those four keys
    // left unserialized the replayed state digests to 0da44f5dc0a31d2c again.
    // W2-07 (relative drop quality) moved it from dfcb186697a88f45: the
    // replayed deploys take the drop_tier Some path, so a drop of an
    // Uncommon-base item rolls its quality relative to that base. With
    // drop_quality returning the absolute roll the replayed state digests to
    // dfcb186697a88f45 again, so no combat moved. W2-08 (39 loot-only items,
    // class C) moved it from 3894ac12174eb3f7: the replayed deploys' drops
    // pick from the larger catalogue. With the Some path limited to the 29
    // v1 definitions the replayed state digests to 3894ac12174eb3f7 again.
    // W2-39 (difficulty bands and the start clamp, class C) moved it from
    // fa528d88f6639802: the replayed Germany start plays difficulty 1.04
    // instead of 1.06, so the combat, its defeat and the refused buy differ.
    // With the parent world.json and the raw difficulty in the deploy terms
    // the replayed state digests to fa528d88f6639802 again.
    assert_eq!(
        hex(replay(without_sale).unwrap().digest),
        "711acf17d72357c2",
        "the replay of the recording up to its sale is pinned"
    );
    // Only compatibility warnings, never a format or seed mismatch. A content
    // fingerprint warning is allowed: the fixture's fingerprint covers item
    // ids, and appending content moves it without changing this property.
    let unexpected: Vec<&String> = report
        .warnings
        .iter()
        .filter(|w| {
            !(w.contains("ruleset 1") || w.contains("save version 6") || w.contains("fingerprint"))
        })
        .collect();
    assert!(unexpected.is_empty(), "{:?}", report.warnings);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("ruleset 1") && w.contains("may diverge")),
        "{:?}",
        report.warnings
    );
    assert!(
        report.warnings.iter().any(|w| w.contains("save version 6")),
        "{:?}",
        report.warnings
    );
}
