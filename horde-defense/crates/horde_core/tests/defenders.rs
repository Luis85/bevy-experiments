//! Defender identity (W1-10): the public `DefenderId`, the saved
//! `DefenderSeat`, `resolve`, the recruit identity table and the plain labels
//! that replace Rust `Debug` names in player text; and (W1-11) the appended
//! `RecruitStatus::Defending` with `reserve_defender` / `release_defender`;
//! and (W1-12) the typed `DefenseInProgress` / `DefenderUnavailable` errors
//! with the Defending pre-checks in `Game::apply`; and (W1-13) the saved
//! `"defender"` seat on `Attempt` and `Report` with their `defender()`
//! accessors; and (W1-15) the recruit seat and appearance stamped on worker
//! attempts at both deploy sites, with the worker-seat save validation; and
//! (W1-16) the saved defender schema fields `Campaign.next_defender` and
//! `reserved_recruit`, `ParallelBattle.summary`, `StrategyState.folded_outcomes`
//! and `Recruit.containments`, all skipped at their defaults; and (W1-17)
//! the D7 seat, Defending and reservation validation, the expedition summary
//! bounds and the test-support `stage_takeover` seam.
mod support;
use horde_core::{
    campaign::{Campaign, NpcTraining},
    command::{Command, CommandError},
    content::{Mode, Policy},
    defenders::{
        DefenderBlocker, DefenderId, DefenderSeat, OriginTrait, RECRUIT_IDENTITIES,
        recruit_identity, resolve,
    },
    sim::{ExpeditionSummary, Game, Outcome, ParallelBattle, SAVE_VERSION, SaveError},
    skill_catalog::skills,
    strategy::RecruitStatus,
    survivors::Appearance,
    world::world,
};
use serde_json::json;
use support::{advance, enemy, strategic_game};

#[test]
fn defender_id_serde_shape() {
    for (id, shape) in [
        (DefenderId::Player, json!("Player")),
        (DefenderId::Mara, json!("Mara")),
        (DefenderId::Recruit(3), json!({"Recruit": 3})),
    ] {
        assert_eq!(serde_json::to_value(id).unwrap(), shape);
        assert_eq!(serde_json::from_value::<DefenderId>(shape).unwrap(), id);
    }
}

#[test]
fn seat_serde_shape_matches_defender_id_recruit() {
    let seat = DefenderSeat::Recruit(7);
    assert_eq!(
        serde_json::to_value(seat).unwrap(),
        serde_json::to_value(DefenderId::Recruit(7)).unwrap()
    );
    assert_eq!(
        serde_json::from_value::<DefenderSeat>(json!({"Recruit": 7})).unwrap(),
        seat
    );
    assert_eq!(DefenderSeat::default(), DefenderSeat::ModeOwner);
    assert!(DefenderSeat::ModeOwner.is_mode_owner());
    assert!(!seat.is_mode_owner());
}

#[test]
fn resolve_table() {
    assert_eq!(
        resolve(Mode::Personal, DefenderSeat::ModeOwner),
        DefenderId::Player
    );
    assert_eq!(
        resolve(Mode::Npc, DefenderSeat::ModeOwner),
        DefenderId::Mara
    );
    assert_eq!(
        resolve(Mode::Npc, DefenderSeat::Recruit(4)),
        DefenderId::Recruit(4)
    );
    // Personal with a recruit seat is invalid saved state (validation rejects
    // it); resolving it still names the player, never a recruit.
    assert_eq!(
        resolve(Mode::Personal, DefenderSeat::Recruit(4)),
        DefenderId::Player
    );
}

#[test]
fn defender_id_round_trips_through_mode_and_seat() {
    for id in [
        DefenderId::Player,
        DefenderId::Mara,
        DefenderId::Recruit(1),
        DefenderId::Recruit(u64::MAX),
    ] {
        assert_eq!(resolve(id.mode(), id.seat()), id);
    }
    assert_eq!(DefenderId::Player.mode(), Mode::Personal);
    assert_eq!(DefenderId::Mara.mode(), Mode::Npc);
    assert_eq!(DefenderId::Recruit(2).mode(), Mode::Npc);
    assert_eq!(DefenderId::Recruit(2).recruit(), Some(2));
    assert_eq!(DefenderId::Mara.recruit(), None);
    assert_eq!(DefenderId::from_mode(Mode::Personal), DefenderId::Player);
    assert_eq!(DefenderId::from_mode(Mode::Npc), DefenderId::Mara);
}

#[test]
fn recruit_identities_keep_todays_names() {
    use Appearance::{Female as F, Male as M};
    let today = [
        ("Alex Rowan", M),
        ("Sam Okoro", M),
        ("Noor Aziz", F),
        ("Ren Mori", M),
        ("Lena Fischer", F),
        ("Ari Silva", F),
        ("Mika Chen", F),
        ("Imani Cole", F),
        ("Sasha Volk", M),
        ("Kai Morgan", M),
        ("Dara Nouri", F),
        ("Morgan Reyes", M),
    ];
    assert_eq!(RECRUIT_IDENTITIES, today);
    for (index, entry) in today.iter().enumerate() {
        let id = index as u64 + 1;
        assert_eq!(recruit_identity(id), *entry);
        assert_eq!(recruit_identity(id + 12), *entry, "the table wraps");
    }
}

/// `StrategyState::recruit` names recruit `id` from `recruit_identity(id)`:
/// ids 1 to 12 take the bare table name, and an id past the table adds the
/// id as a suffix, on both the roster entry and its npc.
#[test]
fn recruits_take_the_table_name_and_a_suffix_past_it() {
    let names = |game: &Game, id: u64| {
        let r = game.strategy().roster.iter().find(|r| r.id == id).unwrap();
        assert_eq!(r.name, r.npc.name, "recruit {id}");
        r.name.clone()
    };
    let mut game = liberated_game(4);
    let country = game.campaign().world.cleared[0].clone();
    for (index, (name, _)) in RECRUIT_IDENTITIES.iter().enumerate() {
        let id = recruit(&mut game, &country);
        assert_eq!(id, index as u64 + 1);
        assert_eq!(names(&game, id), *name, "recruit {id}");
    }
    for (next, expected) in [(13, "Alex Rowan 13"), (24, "Morgan Reyes 24")] {
        let mut game = liberated_game(4);
        game.staging().strategy().next_recruit_id = next;
        let id = recruit(&mut game, &country);
        assert_eq!((id, names(&game, id).as_str()), (next, expected));
    }
}

fn is_human(label: &str) -> bool {
    let mut chars = label.chars();
    let starts_upper = chars.next().is_some_and(char::is_uppercase);
    let camel = label
        .as_bytes()
        .windows(2)
        .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
    starts_upper && !camel && !label.contains("::") && !label.contains('{')
}

#[test]
fn recruit_status_labels_cover_all_variants() {
    let labels: Vec<_> = RecruitStatus::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(labels, ["Ready", "Guarding", "On expedition", "Defending"]);
}

#[test]
fn labels_are_human_over_all() {
    let mut labels: Vec<String> = Vec::new();
    labels.extend(Policy::ALL.iter().map(|p| p.label().to_string()));
    labels.extend(RecruitStatus::ALL.iter().map(|s| s.label().to_string()));
    for t in OriginTrait::ALL {
        labels.push(t.label().into());
        labels.push(t.blurb().into());
    }
    let node = &skills().nodes[0];
    let country = &world().countries[0];
    for blocker in [
        DefenderBlocker::Locked {
            node_id: node.id.clone(),
        },
        DefenderBlocker::Guarding {
            country_id: country.id.clone(),
        },
        DefenderBlocker::OnExpedition { battle: 3 },
        DefenderBlocker::Defending,
        DefenderBlocker::InvalidLoadout {
            reason: "Missing ranged weapon".into(),
        },
    ] {
        labels.push(blocker.label());
    }
    for label in &labels {
        assert!(is_human(label), "not a player-facing label: {label:?}");
    }
    assert_eq!(Policy::BossFocus.label(), "Boss focus");
    let locked = DefenderBlocker::Locked {
        node_id: node.id.clone(),
    }
    .label();
    assert!(locked.contains(&node.name), "{locked}");
    let guarding = DefenderBlocker::Guarding {
        country_id: country.id.clone(),
    }
    .label();
    assert!(guarding.contains(&country.name), "{guarding}");
}

#[test]
fn origin_traits_cover_every_world_biome_without_fallback() {
    for country in &world().countries {
        assert!(
            OriginTrait::from_biome(&country.biome).is_some(),
            "{} has unmapped biome {}",
            country.id,
            country.biome
        );
    }
    assert_eq!(
        OriginTrait::for_biome("Tropical"),
        OriginTrait::MonsoonHardened
    );
    assert_eq!(OriginTrait::for_biome("Arid"), OriginTrait::DustWalker);
    assert_eq!(OriginTrait::for_biome("Boreal"), OriginTrait::FrostSteady);
    assert_eq!(
        OriginTrait::for_biome("Alpine"),
        OriginTrait::HighCountryClimber
    );
    assert_eq!(
        OriginTrait::for_biome("Temperate"),
        OriginTrait::HedgerowForager
    );
    assert_eq!(
        OriginTrait::for_biome("Lunar"),
        OriginTrait::HedgerowForager,
        "unknown biomes fall back to Temperate"
    );
}

/// A legacy game with `count` liberated countries, recruitment and garrisons
/// unlocked, plenty of coins and no live attempt (so preparation is open).
fn liberated_game(count: usize) -> Game {
    let mut c = Campaign {
        legacy_features: true,
        ..Campaign::default()
    };
    let ids: Vec<_> = world()
        .countries
        .iter()
        .take(count)
        .map(|c| c.id.clone())
        .collect();
    c.choose_start_country(&ids[0]).unwrap();
    for id in ids {
        let levels = world().country(&id).unwrap().level_count();
        c.world.country_levels.insert(id.clone(), levels);
        c.world.cleared.push(id);
    }
    c.coins = 10_000;
    let mut game = Game::default();
    *game.staging().campaign() = c;
    game
}

fn recruit(game: &mut Game, country: &str) -> u64 {
    match game.apply(&Command::Recruit(country.into())) {
        Ok(horde_core::command::CommandOutcome::Id(id)) => id,
        other => panic!("recruiting failed: {other:?}"),
    }
}

fn status(game: &Game, id: u64) -> RecruitStatus {
    game.strategy()
        .roster
        .iter()
        .find(|r| r.id == id)
        .unwrap()
        .status
}

#[test]
fn reserve_and_release_defender_round_trip() {
    let mut game = liberated_game(4);
    let country = game.campaign().world.cleared[0].clone();
    let defender = recruit(&mut game, &country);
    let guard = recruit(&mut game, &country);
    let away = recruit(&mut game, &country);
    game.apply(&Command::AssignGuard {
        id: guard,
        country: country.clone(),
    })
    .unwrap();
    game.staging().strategy().reserve_recruit(away).unwrap();
    let before = serde_json::to_value(game.strategy()).unwrap();

    let s = game.staging().strategy();
    assert!(s.reserve_defender(99).is_err(), "unknown survivor");
    assert!(s.reserve_defender(guard).is_err(), "Guarding is not Ready");
    assert!(s.reserve_defender(away).is_err(), "Deployed is not Ready");
    assert!(
        s.release_defender(defender).is_err(),
        "Ready is not Defending"
    );
    assert!(
        s.release_defender(away).is_err(),
        "Deployed is not Defending"
    );
    assert!(
        s.release_defender(guard).is_err(),
        "Guarding is not Defending"
    );
    assert_eq!(serde_json::to_value(&*s).unwrap(), before);

    s.reserve_defender(defender).unwrap();
    assert_eq!(status(&game, defender), RecruitStatus::Defending);
    let s = game.staging().strategy();
    assert!(s.reserve_defender(defender).is_err(), "already defending");
    let npc = s
        .roster
        .iter()
        .find(|r| r.id == defender)
        .unwrap()
        .npc
        .clone();
    assert!(
        s.release_recruit(defender, npc).is_err(),
        "release_recruit stays Deployed-only"
    );
    s.release_defender(defender).unwrap();
    assert_eq!(status(&game, defender), RecruitStatus::Ready);
    assert!(
        game.staging()
            .strategy()
            .release_defender(defender)
            .is_err()
    );
    assert_eq!(serde_json::to_value(game.strategy()).unwrap(), before);
}

#[test]
fn defending_recruit_rejects_every_ready_gated_mutation() {
    let mut ready = liberated_game(4);
    let country = ready.campaign().world.cleared[1].clone();
    let id = recruit(&mut ready, &country);
    for _ in 0..4 {
        ready.apply(&Command::BuyRecruitPoint(id)).unwrap();
    }
    let item = ready.strategy().roster[0].npc.loadout[0];
    let commands = [
        Command::EquipRecruit { id, item },
        Command::TrainRecruit {
            id,
            training: NpcTraining::Guns,
        },
        Command::BuyRecruitPoint(id),
        Command::AssignGuard {
            id,
            country: country.clone(),
        },
        Command::SetRecruitPolicy {
            id,
            policy: Policy::Guardian,
        },
    ];
    // Control: every command and strategy call is accepted while the recruit
    // is Ready, so the rejections below come from the Defending status alone.
    // Through `Game::apply` they come from W1-12's typed pre-check (pinned by
    // `defending_recruit_commands_return_typed_defender_unavailable`); the
    // direct strategy calls after it reach each method's own Ready gate.
    for command in &commands {
        let mut game = ready.clone();
        game.apply(command)
            .unwrap_or_else(|e| panic!("{command:?} on a Ready recruit: {e}"));
    }
    type Call = fn(
        &mut horde_core::strategy::StrategyState,
        &mut Campaign,
        u64,
        u64,
        &str,
    ) -> Result<(), String>;
    let calls: [(&str, Call, &str); 6] = [
        (
            "equip_recruit",
            |s, c, id, item, _| s.equip_recruit(c, id, item),
            "Recall this survivor before changing equipment",
        ),
        (
            "train_recruit",
            |s, c, id, _, _| s.train_recruit(c, id, NpcTraining::Guns),
            "Recall this survivor before training",
        ),
        (
            "buy_training_point",
            |s, c, id, _, _| s.buy_training_point(c, id),
            "Recall this survivor before training",
        ),
        (
            "assign_guard",
            |s, c, id, _, country| s.assign_guard(c, id, country),
            "Survivor is already on duty",
        ),
        (
            "respec_recruit",
            |s, c, id, _, _| s.respec_recruit(c, id),
            "Recall this survivor before redistributing advancement",
        ),
        (
            "reserve_recruit",
            |s, _, id, _, _| s.reserve_recruit(id).map(drop),
            "Survivor is already deployed or guarding a country",
        ),
    ];
    for (name, call, _) in calls {
        let mut game = ready.clone();
        let (s, c) = game.staging().strategy_and_campaign();
        call(s, c, id, item, &country).unwrap_or_else(|e| panic!("{name} on a Ready recruit: {e}"));
    }

    let mut defending = ready.clone();
    defending.staging().strategy().reserve_defender(id).unwrap();
    let before = serde_json::to_value(&defending).unwrap();
    for command in &commands {
        assert_eq!(
            defending.apply(command),
            Err(CommandError::DefenderUnavailable {
                defender: DefenderId::Recruit(id),
                blocker: DefenderBlocker::Defending,
            }),
            "{command:?}"
        );
        assert_eq!(
            serde_json::to_value(&defending).unwrap(),
            before,
            "{command:?}"
        );
    }
    for (name, call, gate) in calls {
        let (s, c) = defending.staging().strategy_and_campaign();
        assert_eq!(call(s, c, id, item, &country), Err(gate.into()), "{name}");
        assert_eq!(serde_json::to_value(&defending).unwrap(), before, "{name}");
    }
    assert_eq!(status(&defending, id), RecruitStatus::Defending);
}

#[test]
fn world_tick_does_not_claim_for_defending_or_deployed_recruits() {
    let mut game = liberated_game(4);
    let country = game.campaign().world.cleared[0].clone();
    let ready = recruit(&mut game, &country);
    let defending = recruit(&mut game, &country);
    let away = recruit(&mut game, &country);
    let (s, c) = game.staging().strategy_and_campaign();
    s.reserve_defender(defending).unwrap();
    s.reserve_recruit(away).unwrap();
    // Every recruit has an unclaimed hit objective and allowance to spare.
    for r in &mut s.roster {
        r.npc.successful_hits = 20;
        assert!(!r.npc.objectives_claimed[0]);
        assert!(r.npc.awarded < c.allowance());
    }
    let before: Vec<_> = s.roster.iter().map(|r| r.npc.clone()).collect();
    s.tick_world(c);
    let npc = |id: u64| &s.roster.iter().find(|r| r.id == id).unwrap().npc;
    let was = |id: u64| &before[s.roster.iter().position(|r| r.id == id).unwrap()];
    assert_eq!(npc(ready).awarded, was(ready).awarded + 1, "Ready claims");
    assert_eq!(npc(defending), was(defending), "Defending must not claim");
    assert_eq!(npc(away), was(away), "Deployed must not claim");
}

fn every_blocker() -> Vec<DefenderBlocker> {
    vec![
        DefenderBlocker::Locked {
            node_id: skills().nodes[0].id.clone(),
        },
        DefenderBlocker::Guarding {
            country_id: world().countries[0].id.clone(),
        },
        DefenderBlocker::OnExpedition { battle: 3 },
        DefenderBlocker::Defending,
        DefenderBlocker::InvalidLoadout {
            reason: "Missing ranged weapon".into(),
        },
    ]
}

#[test]
fn defender_blockers_have_plain_display() {
    let mut texts = vec![CommandError::DefenseInProgress.to_string()];
    for blocker in every_blocker() {
        assert_eq!(blocker.to_string(), blocker.label());
        for defender in [DefenderId::Player, DefenderId::Mara, DefenderId::Recruit(2)] {
            let error = CommandError::DefenderUnavailable {
                defender,
                blocker: blocker.clone(),
            };
            let text = error.to_string();
            assert!(text.contains(&blocker.label()), "{text}");
            texts.push(text);
        }
    }
    for text in &texts {
        assert!(is_human(text), "not a player-facing message: {text:?}");
    }
    assert_eq!(
        CommandError::DefenseInProgress.to_string(),
        "Only possible between defenses"
    );
}

#[test]
fn defending_recruit_commands_return_typed_defender_unavailable() {
    let mut ready = liberated_game(4);
    // Expeditions need the introductory attempt behind the player.
    ready.staging().campaign().attempts = 1;
    let origin = ready.campaign().world.cleared[1].clone();
    let id = recruit(&mut ready, &origin);
    for _ in 0..4 {
        ready.apply(&Command::BuyRecruitPoint(id)).unwrap();
    }
    let item = ready.strategy().roster[0].npc.loadout[0];
    let frontier = world()
        .countries
        .iter()
        .find(|c| {
            ready.campaign().world.can_enter(&c.id)
                && !ready.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .expect("an enterable front")
        .id
        .clone();
    let commands = [
        Command::AssignGuard {
            id,
            country: origin.clone(),
        },
        Command::DeploySurvivor {
            id,
            country: origin.clone(),
            stage: 0,
        },
        Command::CountryOperation {
            id,
            country: frontier,
        },
        Command::TrainRecruit {
            id,
            training: NpcTraining::Guns,
        },
        Command::EquipRecruit { id, item },
        Command::BuyRecruitPoint(id),
        Command::SetRecruitPolicy {
            id,
            policy: Policy::Guardian,
        },
    ];
    // Control: each command is accepted for the same recruit while Ready.
    for command in &commands {
        let mut game = ready.clone();
        game.apply(command)
            .unwrap_or_else(|e| panic!("{command:?} on a Ready recruit: {e}"));
    }

    let mut defending = ready.clone();
    defending.staging().strategy().reserve_defender(id).unwrap();
    let before = serde_json::to_value(&defending).unwrap();
    for command in &commands {
        assert_eq!(
            defending.apply(command),
            Err(CommandError::DefenderUnavailable {
                defender: DefenderId::Recruit(id),
                blocker: DefenderBlocker::Defending,
            }),
            "{command:?}"
        );
        assert_eq!(
            serde_json::to_value(&defending).unwrap(),
            before,
            "{command:?}"
        );
    }
    // Other recruits keep their existing errors: an unknown id is still
    // UnknownSurvivor, not a defender error.
    assert_eq!(
        defending.apply(&Command::SetRecruitPolicy {
            id: id + 100,
            policy: Policy::Guardian,
        }),
        Err(CommandError::UnknownSurvivor)
    );
    assert_eq!(serde_json::to_value(&defending).unwrap(), before);
}

/// A fresh game with a live stage-0 attempt in `mode`.
fn deployed(mode: Mode) -> Game {
    let mut game = Game::default();
    let campaign = game.staging().campaign();
    campaign.legacy_features = true;
    campaign.attempts = 1; // past the introduction, so Mara can deploy
    game.deploy(0, mode).unwrap();
    game
}

/// Rewrites the live attempt's saved `"defender"` key through serde: the seat
/// is crate-private and has no setter, so tests stage it as a save would.
fn with_attempt_seat(game: &Game, seat: serde_json::Value) -> Game {
    let mut value = serde_json::to_value(game).unwrap();
    value["attempt"]
        .as_object_mut()
        .expect("live attempt")
        .insert("defender".into(), seat);
    serde_json::from_value(value).unwrap()
}

fn has_defender_key(value: &serde_json::Value, slot: &str) -> bool {
    value[slot]
        .as_object()
        .unwrap_or_else(|| panic!("{slot} is an object"))
        .contains_key("defender")
}

#[test]
fn npc_report_without_key_reads_mara_personal_reads_player() {
    for (mode, owner) in [
        (Mode::Personal, DefenderId::Player),
        (Mode::Npc, DefenderId::Mara),
    ] {
        let mut game = deployed(mode);
        let saved = serde_json::to_value(&game).unwrap();
        assert!(!has_defender_key(&saved, "attempt"), "{mode:?}");
        let loaded: Game = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded.attempt().unwrap().defender(), owner, "{mode:?}");
        assert_eq!(game.attempt().unwrap().defender(), owner, "{mode:?}");

        game.apply(&Command::Withdraw).unwrap();
        let saved = serde_json::to_value(&game).unwrap();
        assert!(!has_defender_key(&saved, "last_result"), "{mode:?}");
        let loaded: Game = serde_json::from_value(saved).unwrap();
        assert_eq!(loaded.last_result().unwrap().defender(), owner, "{mode:?}");
        assert_eq!(game.last_result().unwrap().defender(), owner, "{mode:?}");
    }
    // A keyed Npc attempt names the recruit; Personal always names the
    // player (a Personal recruit seat is invalid state, never a recruit).
    let npc = with_attempt_seat(&deployed(Mode::Npc), json!({"Recruit": 5}));
    assert_eq!(npc.attempt().unwrap().defender(), DefenderId::Recruit(5));
    let personal = with_attempt_seat(&deployed(Mode::Personal), json!({"Recruit": 5}));
    assert_eq!(personal.attempt().unwrap().defender(), DefenderId::Player);
}

#[test]
fn report_copies_attempt_seat_at_finish() {
    let mut game = with_attempt_seat(&deployed(Mode::Npc), json!({"Recruit": 3}));
    let saved = serde_json::to_value(&game).unwrap();
    assert_eq!(saved["attempt"]["defender"], json!({"Recruit": 3}));
    game.apply(&Command::Withdraw).unwrap();
    assert!(game.attempt().is_none());
    assert_eq!(
        game.last_result().unwrap().defender(),
        DefenderId::Recruit(3)
    );
    let saved = serde_json::to_value(&game).unwrap();
    assert_eq!(saved["last_result"]["defender"], json!({"Recruit": 3}));
    let loaded: Game = serde_json::from_value(saved).unwrap();
    assert_eq!(
        loaded.last_result().unwrap().defender(),
        DefenderId::Recruit(3)
    );
}

/// A strategic game with recruit 1 (Alex Rowan, a Male identity) on the
/// roster, so the stamped appearance differs from the Female Npc default.
fn game_with_alex() -> (Game, String, u64) {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let id = g
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    assert_eq!(id, 1);
    assert_eq!(recruit_identity(id), ("Alex Rowan", Appearance::Male));
    (g, country, id)
}

fn battle(g: &Game, id: u64) -> &ParallelBattle {
    g.battles().iter().find(|b| b.id == id).unwrap()
}

/// Ends the battle's current stage in victory on the next tick.
fn win_stage(g: &mut Game, id: u64) {
    let b = g.battles_mut().iter_mut().find(|b| b.id == id).unwrap();
    enemy(b.game_mut(), 5, 20.0, true);
    let a = b.game_mut().attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
    advance(g, 1);
}

#[test]
fn expedition_attempt_carries_recruit_seat_and_appearance() {
    let (mut g, country, id) = game_with_alex();
    let b = g.deploy_survivor(id, &country, 0).unwrap();
    let a = battle(&g, b).game().attempt().unwrap();
    assert_eq!(a.defender(), DefenderId::Recruit(id));
    assert_eq!(a.profile.appearance, Appearance::Male);
    let saved = serde_json::to_value(&g).unwrap();
    let attempt = &saved["battles"][0]["game"]["attempt"];
    assert_eq!(attempt["defender"], json!({"Recruit": id}));
    assert_eq!(attempt["profile"]["appearance"], json!("Male"));
    let loaded = Game::from_payload(saved, SAVE_VERSION).unwrap();
    let a = battle(&loaded, b).game().attempt().unwrap();
    assert_eq!(a.defender(), DefenderId::Recruit(id));
    assert_eq!(a.profile.appearance, Appearance::Male);
}

#[test]
fn country_operation_next_stage_attempt_keeps_recruit_seat_and_appearance() {
    let (mut g, _, id) = game_with_alex();
    let country = world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .unwrap()
        .clone();
    let operation = g.deploy_country_operation(id, &country.id).unwrap();
    for stage in 0..2 {
        let b = battle(&g, operation);
        assert_eq!(b.stage, stage);
        assert!(!b.settled);
        let a = b.game().attempt().unwrap();
        assert_eq!(a.defender(), DefenderId::Recruit(id), "stage {stage}");
        assert_eq!(a.profile.appearance, Appearance::Male, "stage {stage}");
        Game::from_payload(serde_json::to_value(&g).unwrap(), SAVE_VERSION)
            .unwrap_or_else(|e| panic!("stage {stage} save: {e}"));
        win_stage(&mut g, operation);
        assert!(g.take_settlement_issues().is_empty());
    }
}

/// W1-24: the operation-chain sync that re-arms a country operation's next
/// stage copies the parent's progression ledgers with the level it copies,
/// so the worker's claims never exceed its synced rank.
#[test]
fn country_operation_sync_copies_the_progression_ledgers() {
    let (mut g, _, id) = game_with_alex();
    let country = world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .unwrap()
        .clone();
    let operation = g.deploy_country_operation(id, &country.id).unwrap();
    let c = g.campaign_mut();
    c.victories = c.victories.max(10);
    c.milestones_claimed = [5, 10].into();
    c.player.level_grant_paid = c.player.level;
    let worker = battle(&g, operation).game().campaign();
    assert!(worker.milestones_claimed.is_empty());
    win_stage(&mut g, operation);
    assert!(g.take_settlement_issues().is_empty());
    let b = battle(&g, operation);
    assert!(!b.settled && b.stage == 1);
    let (parent, worker) = (g.campaign(), b.game().campaign());
    assert_eq!(worker.milestones_claimed, parent.milestones_claimed);
    assert_eq!(worker.player.level, parent.player.level);
    assert_eq!(
        worker.player.level_grant_paid,
        parent.player.level_grant_paid
    );
    Game::from_payload(serde_json::to_value(&g).unwrap(), SAVE_VERSION).unwrap();
}

#[test]
fn worker_battle_seat_mismatch_is_invalid() {
    let (mut g, country, id) = game_with_alex();
    let b = g.deploy_survivor(id, &country, 0).unwrap();
    let live = serde_json::to_value(&g).unwrap();
    g.recall_survivor(b).unwrap();
    assert!(battle(&g, b).settled);
    let settled = serde_json::to_value(&g).unwrap();
    for (base, slot) in [(&live, "attempt"), (&settled, "last_result")] {
        Game::from_payload(base.clone(), SAVE_VERSION)
            .unwrap_or_else(|e| panic!("{slot} baseline: {e}"));
        for seat in [None, Some(json!({"Recruit": id + 1}))] {
            let mut value = base.clone();
            let target = value["battles"][0]["game"][slot]
                .as_object_mut()
                .unwrap_or_else(|| panic!("worker {slot}"));
            match &seat {
                None => target.remove("defender"),
                Some(seat) => target.insert("defender".into(), seat.clone()),
            };
            match Game::from_payload(value, SAVE_VERSION) {
                Err(SaveError::Invalid(_)) => {}
                other => panic!("{slot} seat {seat:?} loaded: {:?}", other.err()),
            }
        }
    }
}

/// Pin: inside a worker the victory credit lands on the worker's own
/// `campaign.npc` copy, which settlement hands back to the roster entry.
#[test]
fn worker_victory_still_credits_the_worker_npc_copy() {
    let (mut g, country, id) = game_with_alex();
    let roster_victories = |g: &Game| {
        g.strategy()
            .roster
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .npc
            .victories
    };
    let before = roster_victories(&g);
    let b = g.deploy_survivor(id, &country, 0).unwrap();
    assert_eq!(battle(&g, b).game().campaign().npc.victories, before);
    win_stage(&mut g, b);
    let settled = battle(&g, b);
    assert!(settled.settled);
    assert_eq!(settled.game().campaign().npc.victories, before + 1);
    assert_eq!(roster_victories(&g), before + 1);
    assert_eq!(status(&g, id), RecruitStatus::Ready);
}

/// Every saved location of a W1-16 field in `value` that is present, as
/// `path.key`, over the root game and each worker game.
fn defender_schema_keys(value: &serde_json::Value) -> Vec<String> {
    fn check(object: &serde_json::Value, path: String, key: &str, found: &mut Vec<String>) {
        if object.as_object().is_some_and(|o| o.contains_key(key)) {
            found.push(format!("{path}.{key}"));
        }
    }
    fn game(value: &serde_json::Value, at: &str, found: &mut Vec<String>) {
        for key in ["next_defender", "reserved_recruit"] {
            check(&value["campaign"], format!("{at}campaign"), key, found);
        }
        let strategy = &value["strategy"];
        check(strategy, format!("{at}strategy"), "folded_outcomes", found);
        let roster = strategy["roster"].as_array().into_iter().flatten();
        for (n, recruit) in roster.enumerate() {
            let path = format!("{at}strategy.roster[{n}]");
            check(recruit, path, "containments", found);
        }
        let battles = value["battles"].as_array().into_iter().flatten();
        for (n, battle) in battles.enumerate() {
            check(battle, format!("{at}battles[{n}]"), "summary", found);
            game(&battle["game"], &format!("{at}battles[{n}].game."), found);
        }
    }
    let mut found = Vec::new();
    game(value, "", &mut found);
    found
}

#[test]
fn defender_fields_serialize_only_when_set() {
    let mut personal = deployed(Mode::Personal);
    personal.apply(&Command::Withdraw).unwrap();
    let mut mara = deployed(Mode::Npc);
    mara.apply(&Command::Withdraw).unwrap();
    let (mut expedition, country, id) = game_with_alex();
    let live = expedition.deploy_survivor(id, &country, 0).unwrap();
    let mut settled = expedition.clone();
    settled.recall_survivor(live).unwrap();
    assert!(battle(&settled, live).settled);
    for (name, game) in [
        ("fresh", &Game::default()),
        ("personal report", &personal),
        ("mara report", &mara),
        ("live expedition", &expedition),
        ("settled expedition", &settled),
    ] {
        let saved = serde_json::to_value(game).unwrap();
        assert_eq!(defender_schema_keys(&saved), Vec::<String>::new(), "{name}");
    }

    let summary = ExpeditionSummary {
        outcome: Some(Outcome::Withdrawal),
        stages_cleared: 1,
        liberated: true,
        fronts: vec!["front-a".into(), "front-b".into()],
        coins: 40,
        loot_count: 2,
        acknowledged: false,
    };
    let campaign = settled.staging().campaign();
    campaign.next_defender = Some(DefenderId::Recruit(id));
    campaign.reserved_recruit = Some(id);
    settled.staging().strategy().folded_outcomes = 3;
    settled.staging().strategy().roster[0].containments = 2;
    settled.staging().battles()[0].summary = Some(summary.clone());
    let saved = serde_json::to_value(&settled).unwrap();
    assert_eq!(
        defender_schema_keys(&saved),
        [
            "campaign.next_defender",
            "campaign.reserved_recruit",
            "strategy.folded_outcomes",
            "strategy.roster[0].containments",
            "battles[0].summary",
        ]
    );
    assert_eq!(saved["campaign"]["next_defender"], json!({"Recruit": id}));
    assert_eq!(saved["campaign"]["reserved_recruit"], json!(id));
    assert_eq!(saved["strategy"]["folded_outcomes"], json!(3));
    assert_eq!(saved["strategy"]["roster"][0]["containments"], json!(2));
    let loaded: Game = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(
        loaded.campaign().next_defender,
        Some(DefenderId::Recruit(id))
    );
    assert_eq!(loaded.campaign().reserved_recruit, Some(id));
    assert_eq!(loaded.strategy().folded_outcomes, 3);
    assert_eq!(loaded.strategy().roster[0].containments, 2);
    assert_eq!(battle(&loaded, live).summary, Some(summary));
    assert_eq!(serde_json::to_value(&loaded).unwrap(), saved);

    // The summary has a container default: missing keys read as neutral.
    let mut sparse = saved;
    sparse["battles"][0]["summary"] = json!({"coins": 7});
    let loaded: Game = serde_json::from_value(sparse).unwrap();
    assert_eq!(
        battle(&loaded, live).summary,
        Some(ExpeditionSummary {
            coins: 7,
            ..ExpeditionSummary::default()
        })
    );
}

/// During a takeover `reserved == Some(Npc)` names the recruit's run, so
/// only the recruit's items (in `reserved_items`) are reserved: Mara's idle
/// loadout stays free for sale checks and other deployments.
#[test]
fn takeover_does_not_reserve_maras_idle_loadout() {
    let mut game = liberated_game(1);
    let country = game.campaign().world.cleared[0].clone();
    let id = recruit(&mut game, &country);
    let recruit_loadout = game.strategy().roster[0].npc.loadout;
    let mara_loadout = game.campaign().npc.loadout;
    let campaign = game.staging().campaign();
    campaign.reserve_external(recruit_loadout).unwrap();
    campaign.reserved = Some(Mode::Npc);
    campaign.reserved_recruit = Some(id);
    for item in mara_loadout {
        assert!(!campaign.item_reserved(item), "Mara's item {item}");
    }
    for item in recruit_loadout {
        assert!(campaign.item_reserved(item), "recruit item {item}");
    }
    // Another deployment may take Mara's idle loadout during the takeover.
    let mut other = campaign.clone();
    other.reserve_external(mara_loadout).unwrap();
    assert!(mara_loadout.iter().all(|item| other.item_reserved(*item)));

    // Without a recruit in the seat, Some(Npc) is Mara's own run.
    campaign.reserved_recruit = None;
    for item in mara_loadout {
        assert!(campaign.item_reserved(item), "Mara's item {item}");
    }
    assert!(campaign.clone().reserve_external(mara_loadout).is_err());
}

// ---------------------------------------------------------------------------
// W1-17: D7 seat, Defending and reservation validation, the D10 summary
// bounds and the `stage_takeover` test-support seam.
// ---------------------------------------------------------------------------

/// The same main-run state saved before and after `stage_takeover`, with
/// four recruits on the roster: `defender` (who takes the main run),
/// `ready`, `expedition` (on the live battle 0) and `guard` (garrisoned).
struct TakeoverSaves {
    mara: serde_json::Value,
    takeover: serde_json::Value,
    defender: u64,
    ready: u64,
    expedition: u64,
    guard: u64,
}

fn takeover_saves() -> TakeoverSaves {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[0].clone();
    let [defender, ready, expedition, guard] = [(); 4].map(|()| {
        g.with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
            .unwrap()
    });
    g.apply(&Command::AssignGuard {
        id: guard,
        country: country.clone(),
    })
    .unwrap();
    g.deploy_survivor(expedition, &country, 0).unwrap();
    g.deploy(0, Mode::Npc).unwrap();
    let mara = serde_json::to_value(&g).unwrap();
    g.staging().stage_takeover(defender).unwrap();
    TakeoverSaves {
        mara,
        takeover: serde_json::to_value(&g).unwrap(),
        defender,
        ready,
        expedition,
        guard,
    }
}

fn roster_entry(value: &mut serde_json::Value, id: u64) -> &mut serde_json::Value {
    value["strategy"]["roster"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == json!(id))
        .unwrap_or_else(|| panic!("recruit {id} on the roster"))
}

fn set_status(value: &mut serde_json::Value, id: u64, status: &str) {
    roster_entry(value, id)["status"] = json!(status);
}

fn saved_loadout(value: &mut serde_json::Value, id: u64) -> serde_json::Value {
    roster_entry(value, id)["npc"]["loadout"].clone()
}

/// Drops `loadout`'s items from the root `reserved_items`.
fn unreserve(value: &mut serde_json::Value, loadout: &serde_json::Value) {
    let items = loadout.as_array().unwrap().clone();
    value["campaign"]["reserved_items"]
        .as_array_mut()
        .unwrap()
        .retain(|id| !items.contains(id));
}

/// Loads every row and fails with the names of the rows that loaded or
/// failed with anything but `SaveError::Invalid`.
fn assert_each_invalid(rows: Vec<(&str, serde_json::Value)>) {
    let loaded: Vec<String> = rows
        .into_iter()
        .filter_map(
            |(name, value)| match Game::from_payload(value, SAVE_VERSION) {
                Err(SaveError::Invalid(_)) => None,
                Ok(_) => Some(format!("{name}: loaded")),
                Err(error) => Some(format!("{name}: {error}")),
            },
        )
        .collect();
    assert_eq!(
        loaded,
        Vec::<String>::new(),
        "rows that did not fail validation"
    );
}

#[test]
fn staged_recruit_defending_main_run_validates() {
    let mut saves = takeover_saves();
    let id = saves.defender;
    Game::from_payload(saves.mara.clone(), SAVE_VERSION).expect("Mara's defense loads");
    let value = saves.takeover.clone();
    assert_eq!(value["attempt"]["defender"], json!({"Recruit": id}));
    assert_eq!(value["campaign"]["reserved"], json!("Npc"));
    assert_eq!(value["campaign"]["reserved_recruit"], json!(id));
    let loadout = saved_loadout(&mut saves.takeover, id);
    for item in loadout.as_array().unwrap() {
        assert!(
            value["campaign"]["reserved_items"]
                .as_array()
                .unwrap()
                .contains(item),
            "recruit item {item} is reserved"
        );
    }
    let loaded = Game::from_payload(value.clone(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("staged takeover: {e}"));
    assert_eq!(status(&loaded, id), RecruitStatus::Defending);
    let a = loaded.attempt().unwrap();
    assert_eq!(a.defender(), DefenderId::Recruit(id));
    assert_eq!(a.profile.appearance, recruit_identity(id).1);
    assert_eq!(serde_json::to_value(&loaded).unwrap(), value);

    // The seam refuses without a change: no Mara defense, or a recruit that
    // is unknown or not Ready.
    let mut idle = strategic_game();
    let country = idle.campaign().world.cleared[0].clone();
    let recruit = idle
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    let mut personal = idle.clone();
    personal.deploy(0, Mode::Personal).unwrap();
    let mut taken = loaded.clone();
    let busy = Game::from_payload(saves.mara, SAVE_VERSION).unwrap();
    for (name, game, recruit) in [
        ("no attempt", &mut idle, recruit),
        ("personal attempt", &mut personal, recruit),
        ("already taken over", &mut taken, saves.ready),
        ("guarding recruit", &mut busy.clone(), saves.guard),
        ("expedition recruit", &mut busy.clone(), saves.expedition),
        ("unknown recruit", &mut busy.clone(), 99),
    ] {
        let before = serde_json::to_value(&*game).unwrap();
        assert!(
            game.staging().stage_takeover(recruit).is_err(),
            "{name} was staged"
        );
        assert_eq!(serde_json::to_value(&*game).unwrap(), before, "{name}");
    }

    // A refusal after the recruit was already reserved rolls it back: a Ready
    // recruit whose loadout Mara's live defense holds passes reserve_defender
    // (Ready to Defending), then fails reserve_external, and must be Ready
    // again with nothing else changed.
    let mut clash = busy.clone();
    let mara_loadout = clash.campaign().npc.loadout;
    clash
        .staging()
        .strategy()
        .roster
        .iter_mut()
        .find(|r| r.id == saves.ready)
        .unwrap()
        .npc
        .loadout = mara_loadout;
    let before = serde_json::to_value(&clash).unwrap();
    assert!(
        clash.staging().stage_takeover(saves.ready).is_err(),
        "a recruit holding Mara's reserved loadout was staged"
    );
    assert_eq!(status(&clash, saves.ready), RecruitStatus::Ready);
    assert_eq!(
        serde_json::to_value(&clash).unwrap(),
        before,
        "reserved loadout"
    );
}

#[test]
fn seat_validation_rejects_each_inconsistency() {
    let mut saves = takeover_saves();
    let (a, b, c) = (saves.defender, saves.ready, saves.expedition);
    let mara = saves.mara.clone();
    let takeover = saves.takeover.clone();
    let loadout_a = saved_loadout(&mut saves.takeover, a);

    // Personal defenses with a recruit on the roster, live and reported.
    let mut personal = strategic_game();
    let country = personal.campaign().world.cleared[0].clone();
    let recruit = personal
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    personal.deploy(0, Mode::Personal).unwrap();
    let live_personal = serde_json::to_value(&personal).unwrap();
    personal.apply(&Command::Withdraw).unwrap();
    let personal_report = serde_json::to_value(&personal).unwrap();
    // Mara's finished defense, with no attempt and an Npc report.
    let mut mara_game = Game::from_payload(mara.clone(), SAVE_VERSION).unwrap();
    mara_game.apply(&Command::Withdraw).unwrap();
    let mara_report = serde_json::to_value(&mara_game).unwrap();
    for (name, base) in [
        ("mara", &mara),
        ("takeover", &takeover),
        ("personal", &live_personal),
        ("personal report", &personal_report),
        ("mara report", &mara_report),
    ] {
        Game::from_payload(base.clone(), SAVE_VERSION)
            .unwrap_or_else(|e| panic!("{name} baseline: {e}"));
    }
    // An Npc report may name any roster recruit, whatever its status.
    let mut known_report = mara_report.clone();
    known_report["last_result"]["defender"] = json!({"Recruit": a});
    Game::from_payload(known_report, SAVE_VERSION).expect("a report naming a roster recruit");

    let row = |base: &serde_json::Value, edit: &dyn Fn(&mut serde_json::Value)| {
        let mut value = base.clone();
        edit(&mut value);
        value
    };
    assert_each_invalid(vec![
        (
            "personal attempt with a recruit seat",
            row(&live_personal, &|v| {
                v["attempt"]["defender"] = json!({"Recruit": recruit});
            }),
        ),
        (
            "personal report with a recruit seat",
            row(&personal_report, &|v| {
                v["last_result"]["defender"] = json!({"Recruit": recruit});
            }),
        ),
        (
            "report seat names no roster recruit",
            row(&mara_report, &|v| {
                v["last_result"]["defender"] = json!({"Recruit": 99});
            }),
        ),
        (
            "Defending without a seat",
            row(&mara, &|v| set_status(v, b, "Defending")),
        ),
        (
            "Defending without an attempt",
            row(&mara_report, &|v| set_status(v, b, "Defending")),
        ),
        (
            "reserved_recruit without a takeover",
            row(&mara, &|v| v["campaign"]["reserved_recruit"] = json!(b)),
        ),
        (
            "seat with a Ready recruit",
            row(&takeover, &|v| set_status(v, a, "Ready")),
        ),
        (
            "two Defending",
            row(&takeover, &|v| set_status(v, b, "Defending")),
        ),
        (
            "seat names no roster recruit",
            row(&takeover, &|v| {
                v["attempt"]["defender"] = json!({"Recruit": 99});
            }),
        ),
        (
            "reserved_recruit names another recruit",
            row(&takeover, &|v| v["campaign"]["reserved_recruit"] = json!(b)),
        ),
        (
            "reserved_recruit missing",
            row(&takeover, &|v| {
                v["campaign"]
                    .as_object_mut()
                    .unwrap()
                    .remove("reserved_recruit");
            }),
        ),
        (
            "loadout not reserved",
            row(&takeover, &|v| unreserve(v, &loadout_a)),
        ),
        (
            "worker seat != recruit_id",
            row(&takeover, &|v| {
                v["battles"][0]["game"]["attempt"]["defender"] = json!({"Recruit": b});
            }),
        ),
        (
            "worker with next_defender",
            row(&takeover, &|v| {
                v["battles"][0]["game"]["campaign"]["next_defender"] = json!("Mara");
            }),
        ),
        (
            "worker with reserved_recruit",
            row(&takeover, &|v| {
                v["battles"][0]["game"]["campaign"]["reserved_recruit"] = json!(c);
            }),
        ),
    ]);
}

/// D7 rule 2, pinned by the check that rejects each row. A Defending
/// recruit cannot also guard or hold an unsettled battle: the garrison and
/// expedition checks demand Guarding and Deployed first. What is left to the
/// takeover check itself is equipment shared with another duty.
#[test]
fn takeover_duties_fail_at_the_check_that_owns_them() {
    let mut saves = takeover_saves();
    let (a, c, d) = (saves.defender, saves.expedition, saves.guard);
    let takeover = saves.takeover.clone();
    let loadout_a = saved_loadout(&mut saves.takeover, a);
    let loadout_c = saved_loadout(&mut saves.takeover, c);
    let loadout_d = saved_loadout(&mut saves.takeover, d);
    let row = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut value = takeover.clone();
        edit(&mut value);
        value
    };
    let rows = [
        (
            "Defending plus guard",
            row(&|v| {
                v["strategy"]["guards"][0]["recruit_id"] = json!(a);
                set_status(v, d, "Ready");
                unreserve(v, &loadout_d);
            }),
            "Invalid garrison reservation",
        ),
        (
            "Defending plus unsettled battle",
            row(&|v| {
                let battle = &mut v["battles"][0];
                battle["recruit_id"] = json!(a);
                battle["loadout"] = loadout_a.clone();
                battle["game"]["attempt"]["defender"] = json!({"Recruit": a});
                set_status(v, c, "Ready");
                unreserve(v, &loadout_c);
            }),
            "Expedition reservation is inconsistent",
        ),
        (
            "Defending shares the guard's equipment",
            row(&|v| {
                roster_entry(v, d)["npc"]["loadout"] = loadout_a.clone();
            }),
            "Defending survivor shares equipment with another duty",
        ),
    ];
    let wrong: Vec<String> = rows
        .into_iter()
        .filter_map(
            |(name, value, expected)| match Game::from_payload(value, SAVE_VERSION) {
                Err(SaveError::Invalid(message)) if message == expected => None,
                Ok(_) => Some(format!("{name}: loaded")),
                Err(error) => Some(format!("{name}: {error}")),
            },
        )
        .collect();
    assert_eq!(wrong, Vec::<String>::new(), "rows rejected elsewhere");
}

#[test]
fn next_defender_must_name_a_roster_recruit() {
    let saves = takeover_saves();
    for (name, base) in [("mara", &saves.mara), ("takeover", &saves.takeover)] {
        for defender in [
            json!("Player"),
            json!("Mara"),
            json!({"Recruit": saves.ready}),
            json!({"Recruit": saves.guard}),
            json!({"Recruit": saves.defender}),
        ] {
            let mut value = base.clone();
            value["campaign"]["next_defender"] = defender.clone();
            let loaded = Game::from_payload(value, SAVE_VERSION)
                .unwrap_or_else(|e| panic!("{name} next_defender {defender}: {e}"));
            assert_eq!(
                serde_json::to_value(loaded.campaign().next_defender).unwrap(),
                defender
            );
        }
        let mut value = base.clone();
        value["campaign"]["next_defender"] = json!({"Recruit": 99});
        match Game::from_payload(value, SAVE_VERSION) {
            Err(SaveError::Invalid(_)) => {}
            other => panic!("{name} unknown next_defender loaded: {:?}", other.err()),
        }
    }
}

#[test]
fn summary_bounds_are_validated() {
    let (mut g, country, id) = game_with_alex();
    let live = g.deploy_survivor(id, &country, 0).unwrap();
    let running = serde_json::to_value(&g).unwrap();
    g.recall_survivor(live).unwrap();
    let settled = serde_json::to_value(&g).unwrap();
    let fronts = |n: usize| -> Vec<String> { (0..n).map(|n| format!("front-{n}")).collect() };
    let with = |base: &serde_json::Value, summary: serde_json::Value| {
        let mut value = base.clone();
        value["battles"][0]["summary"] = summary;
        value
    };
    for (name, value) in [
        (
            "running, partial",
            with(&running, json!({"coins": 5, "loot_count": 1})),
        ),
        (
            "settled, full",
            with(
                &settled,
                json!({"outcome": "Withdrawal", "acknowledged": true, "fronts": fronts(32)}),
            ),
        ),
        (
            "settled, unacknowledged",
            with(&settled, json!({"outcome": "Victory"})),
        ),
    ] {
        Game::from_payload(value, SAVE_VERSION).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    assert_each_invalid(vec![
        (
            "acknowledged while running",
            with(&running, json!({"acknowledged": true})),
        ),
        (
            "outcome while running",
            with(&running, json!({"outcome": "Defeat"})),
        ),
        (
            "33 fronts while running",
            with(&running, json!({"fronts": fronts(33)})),
        ),
        (
            "33 fronts when settled",
            with(
                &settled,
                json!({"outcome": "Victory", "fronts": fronts(33)}),
            ),
        ),
    ]);
}

#[test]
fn run_record_takeover_requires_npc_mode() {
    let mut personal = deployed(Mode::Personal);
    personal.apply(&Command::Withdraw).unwrap();
    let mut mara = deployed(Mode::Npc);
    mara.apply(&Command::Withdraw).unwrap();
    // The last main-run record, rewritten as a recruit's main-run takeover.
    let takeover_record = |game: &Game, mode: &str| {
        let mut value = serde_json::to_value(game).unwrap();
        let record = value["campaign"]["run_history"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .expect("a main-run record");
        assert_eq!(record["worker_id"], json!(null));
        record["recruit_id"] = json!(1);
        record["mode"] = json!(mode);
        value
    };
    Game::from_payload(takeover_record(&mara, "Npc"), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("Npc takeover record: {e}"));
    assert_each_invalid(vec![
        (
            "Personal takeover record",
            takeover_record(&personal, "Personal"),
        ),
        (
            "Personal-mode takeover record of an Npc run",
            takeover_record(&mara, "Personal"),
        ),
    ]);
}
