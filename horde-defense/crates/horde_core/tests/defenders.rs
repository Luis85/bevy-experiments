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
//! bounds and the test-support `stage_takeover` seam; and (W2-10) the
//! liberation-based advancement allowance (10 levels by the 6th liberation)
//! with the indexed `liberated_count`; and (W2-11) the takeover core: a
//! Ready recruit deploys on the main run with its own loadout, tactics and
//! identity, progresses and is credited on its roster entry, and finish
//! releases it and its items; and (W2-12) the saved next defender with
//! `SetNextDefender` and `BeginDefense`, the defender-aware continue, retry
//! and repeat, the dispatch reset, and the `available_defenders`,
//! `defender_card`, `defender_loadout`, `active_defender`,
//! `battle_defender_card` and `continue_blocker` reads; and (W2-13) the live
//! `SetRecruitTactics` for a Ready or Defending recruit; and (W2-19)
//! `SelectCountry` with the pure `outbreak_plan` and `country_choice_preview`
//! reads; and (W2-21) the pure `defender_power` on the deploy's shared
//! inputs, the derived `service_record` and the `Recruit.containments`
//! writer; and (W2-44) the core `Slot::label` and `DamageKind::label` that
//! replace the slot and damage-kind `Debug` names.
mod support;
use horde_core::{
    campaign::{Campaign, Npc, NpcTraining},
    command::{Command, CommandError, CommandOutcome, MAX_RECALL_PERCENT, NpcTactics},
    content::{Mode, Policy, Slot, content},
    defenders::{
        DefenderBlocker, DefenderId, DefenderOption, DefenderRole, DefenderSeat, GuardDuty,
        MAX_ADVANCEMENT_LEVEL, OriginTrait, PowerBand, RECRUIT_IDENTITIES, ServiceRecord,
        recruit_identity, resolve,
    },
    rules::DamageKind,
    sim::{
        DeploymentInputs, ExpeditionSummary, Game, Outcome, ParallelBattle, SAVE_VERSION, SaveError,
    },
    skill_catalog::skills,
    stats::RunRecord,
    strategy::{OutbreakOutcome, RecruitStatus},
    survivors::Appearance,
    world::world,
};
use serde_json::json;
use std::collections::BTreeSet;
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
    labels.extend(Slot::ALL.iter().map(|s| s.label().to_string()));
    labels.extend(DamageKind::ALL.iter().map(|k| k.label().to_string()));
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
    let slots: Vec<_> = Slot::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(slots, ["Ranged", "Melee", "Outfit", "Accessory"]);
    let kinds: Vec<_> = DamageKind::ALL.iter().map(|k| k.label()).collect();
    assert_eq!(
        kinds,
        [
            "Crushing",
            "Cutting",
            "Piercing",
            "Impaling",
            "Small piercing",
            "Large piercing"
        ]
    );
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

/// Sets the battle's current stage up to end in victory on the next tick.
fn arm_stage_win(g: &mut Game, id: u64) {
    let b = g.battles_mut().iter_mut().find(|b| b.id == id).unwrap();
    enemy(b.game_mut(), 5, 20.0, true);
    let a = b.game_mut().attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies[0].injury.unconscious = true;
}

/// Ends the battle's current stage in victory on the next tick.
fn win_stage(g: &mut Game, id: u64) {
    arm_stage_win(g, id);
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

/// The op-chain sync copies the ledgers as the pass's settlement leaves
/// them (ruling E1): a stage victory that takes the campaign to rank 5 is
/// claimed at the end of the pass, after the next stage is deployed, and the
/// continuing worker still receives the claim.
#[test]
fn country_operation_sync_copies_the_ledgers_the_settlement_wrote() {
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
    c.victories = 4;
    c.milestones_claimed.clear();
    win_stage(&mut g, operation);
    assert!(g.take_settlement_issues().is_empty());
    let b = battle(&g, operation);
    assert!(!b.settled && b.stage == 1, "the operation continues");
    let (parent, worker) = (g.campaign(), b.game().campaign());
    assert_eq!(parent.progression_rank(), 5);
    assert_eq!(
        parent.milestones_claimed,
        [5].into(),
        "the pass claimed rank 5"
    );
    assert_eq!(worker.milestones_claimed, parent.milestones_claimed);
    assert_eq!(
        worker.player.level_grant_paid,
        parent.player.level_grant_paid
    );
}

/// Two country operations whose stage victories settle in the same idle
/// pass: operation A is credited and re-armed first, then B's credit levels
/// the player and reaches rank 5, which the end-of-pass settlement pays. The
/// ledger copy onto the continuing A never exceeds the level and rank A was
/// synced at, so every save of the pass still validates.
#[test]
fn country_operation_ledger_copy_stays_within_the_workers_synced_rank() {
    let (mut g, home, first) = game_with_alex();
    let second = recruit(&mut g, &home);
    let countries: Vec<String> = world()
        .countries
        .iter()
        .filter(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .take(2)
        .map(|c| c.id.clone())
        .collect();
    assert_eq!(countries.len(), 2);
    let a = g.deploy_country_operation(first, &countries[0]).unwrap();
    let b = g.deploy_country_operation(second, &countries[1]).unwrap();
    let position = |g: &Game, id: u64| g.battles().iter().position(|x| x.id == id).unwrap();
    assert!(position(&g, a) < position(&g, b), "A settles before B");
    let c = g.campaign_mut();
    c.threat_revision = 1;
    c.victories = 3;
    c.milestones_claimed.clear();
    // One victory leaves the player one XP short of the next level; the
    // second levels it.
    c.player.xp = 0;
    while c.player.next_level_xp() <= c.completion_xp_preview() + 1 {
        c.player.level += 1;
        if c.legacy_features {
            c.player.points += 1;
        }
    }
    c.player.level_grant_paid = c.player.level;
    let victory_xp = c.completion_xp_preview();
    assert!(victory_xp > 0 && victory_xp < c.player.next_level_xp());
    c.player.xp = c.player.next_level_xp() - victory_xp - 1;
    let level = c.player.level;
    arm_stage_win(&mut g, a);
    arm_stage_win(&mut g, b);
    advance(&mut g, 1);
    assert!(g.take_settlement_issues().is_empty());
    let parent = g.campaign();
    assert_eq!(parent.player.level, level + 1, "B's credit levelled up");
    assert_eq!(parent.progression_rank(), 5);
    assert_eq!(parent.player.level_grant_paid, level + 1);
    assert!(parent.milestones_claimed.contains(&5), "the pass claimed 5");
    g.to_save_json()
        .unwrap_or_else(|e| panic!("save after the pass: {e}"));
    for id in [a, b] {
        let op = battle(&g, id);
        assert!(!op.settled && op.stage == 1, "operation {id} continues");
        let worker = op.game().campaign();
        assert!(
            worker.player.level_grant_paid <= worker.player.level,
            "operation {id} grant ledger"
        );
        assert!(
            worker
                .milestones_claimed
                .last()
                .is_none_or(|rank| *rank <= worker.progression_rank()),
            "operation {id} claims"
        );
    }
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
    ] {
        let saved = serde_json::to_value(game).unwrap();
        assert_eq!(defender_schema_keys(&saved), Vec::<String>::new(), "{name}");
    }
    // W2-15: settlement writes the settled expedition's outcome notice, and
    // only that field.
    let saved = serde_json::to_value(&settled).unwrap();
    assert_eq!(defender_schema_keys(&saved), ["battles[0].summary"]);
    assert_eq!(
        battle(&settled, live).summary,
        Some(ExpeditionSummary {
            outcome: Some(Outcome::Withdrawal),
            ..ExpeditionSummary::default()
        })
    );

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

// ---- W2-10: the liberation-based advancement allowance ----

/// A legacy campaign that liberated `countries` (in order, the first is the
/// start) and first-cleared the first `first_clears` catalogue levels.
fn allowance_campaign(countries: &[&str], first_clears: usize) -> Campaign {
    let mut c = Campaign {
        legacy_features: true,
        ..Campaign::default()
    };
    if let Some(start) = countries.first() {
        c.choose_start_country(start).unwrap();
    }
    for id in countries {
        let levels = world().country(id).unwrap().level_count();
        c.world.country_levels.insert((*id).into(), levels);
        c.world.clear_high_water.insert((*id).into(), levels);
        c.world.cleared.push((*id).into());
    }
    for level in content().levels.iter().take(first_clears) {
        c.first_clears.insert(level.id.clone());
    }
    assert_eq!(c.first_clears.len(), first_clears, "catalogue too short");
    assert_eq!(c.liberated_count() as usize, countries.len());
    c
}

fn country_ids(filter: impl Fn(usize) -> bool, take: usize) -> Vec<&'static str> {
    let ids: Vec<&'static str> = world()
        .countries
        .iter()
        .filter(|c| filter(c.level_count()))
        .take(take)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(ids.len(), take, "not enough matching countries");
    ids
}

fn legacy_allowance(c: &Campaign) -> u32 {
    12 + 8 * c.first_clears.len() as u32
}

#[test]
fn allowance_reaches_level_ten_by_sixth_liberation() {
    let ids = country_ids(|_| true, 8);
    // The CD5 table (first_clears below the curve): L = 0..=6, then capped.
    for (l, cap, allowance) in [
        (0, 1, 12),
        (1, 3, 28),
        (2, 4, 36),
        (3, 6, 52),
        (4, 7, 60),
        (5, 9, 76),
        (6, 10, 84),
        (7, 10, 84),
        (8, 10, 84),
    ] {
        let c = allowance_campaign(&ids[..l], 0);
        assert_eq!(c.allowance(), allowance, "allowance at L = {l}");
        assert_eq!(c.advancement_cap_level(), cap, "cap level at L = {l}");
    }
    // Today a sixth liberation of three-stage fronts leaves the allowance at
    // 36 (12 + 8 x 3 first clears); it now reaches level 10.
    let c = allowance_campaign(&country_ids(|n| n == 3, 6), 3);
    assert_eq!(c.allowance(), 84);
    assert_eq!(c.advancement_cap_level(), MAX_ADVANCEMENT_LEVEL);
    // A defender awarded the whole allowance stands at level 10.
    let mut npc = c.npc.clone();
    for (awarded, level) in [(0, 1), (12, 1), (13, 2), (28, 3), (76, 9), (84, 10)] {
        npc.awarded = awarded;
        assert_eq!(npc.advancement_level(), level, "awarded {awarded}");
    }
}

#[test]
fn three_stage_only_route_keeps_raising_the_allowance() {
    // A route of three-stage countries first-clears only catalogue levels
    // 0..3, so the legacy allowance stops at 36 after the first liberation.
    let ids = country_ids(|n| n == 3, 6);
    let allowances: Vec<u32> = (0..=6)
        .map(|l| allowance_campaign(&ids[..l], 3).allowance())
        .collect();
    assert_eq!(allowances, [36, 36, 36, 52, 60, 76, 84]);
    assert!(allowances.windows(2).all(|w| w[0] <= w[1]));
    assert!(allowances[3..].iter().all(|a| *a > 36));
}

#[test]
fn allowance_never_below_legacy_and_monotonic() {
    let ids = country_ids(|_| true, 12);
    let levels = content().levels.len();
    for fc in 0..=levels {
        let mut previous = 0;
        for l in 0..=ids.len() {
            let c = allowance_campaign(&ids[..l], fc);
            let allowance = c.allowance();
            assert!(allowance >= legacy_allowance(&c), "fc {fc}, L {l}");
            if l == 0 {
                assert_eq!(allowance, legacy_allowance(&c), "L = 0 is legacy");
            }
            assert!(allowance >= previous, "monotonic in L: fc {fc}, L {l}");
            if fc > 0 {
                let fewer = allowance_campaign(&ids[..l], fc - 1).allowance();
                assert!(allowance >= fewer, "monotonic in first clears");
            }
            previous = allowance;
        }
    }
    // An outbreak that reopens a liberated country keeps the high-water rank.
    let mut c = allowance_campaign(&ids[..3], 0);
    let before = c.allowance();
    assert!(c.world.reopen(ids[1]).unwrap());
    assert_eq!(c.liberated_count(), 3);
    assert_eq!(c.allowance(), before);
}

#[test]
fn allowance_stops_growing_at_the_max_level_whatever_the_first_clears() {
    // Content is append-only: a catalogue of 10 or more levels gives a
    // campaign 10 or more first clears, and the clears term alone must not
    // carry the cap past MAX_ADVANCEMENT_LEVEL.
    let max = u32::from(MAX_ADVANCEMENT_LEVEL);
    for clears in [9, 10, 12, 40] {
        let mut c = Campaign {
            legacy_features: true,
            ..Campaign::default()
        };
        c.first_clears = (0..clears).map(|i| format!("appended_level_{i}")).collect();
        let level = (1 + clears).min(max);
        assert_eq!(
            u32::from(c.advancement_cap_level()),
            level,
            "{clears} first clears"
        );
        assert_eq!(c.allowance(), 12 + 8 * (level - 1), "{clears} first clears");
    }
}

/// The pre-W2-10 definition: a scan of every catalogue country.
fn full_scan_liberated(c: &Campaign) -> u32 {
    world()
        .countries
        .iter()
        .filter(|country| {
            c.world.cleared.contains(&country.id)
                || c.world
                    .clear_high_water
                    .get(&country.id)
                    .copied()
                    .unwrap_or(0)
                    == country.level_count()
        })
        .count() as u32
}

#[test]
fn liberated_count_matches_full_scan() {
    let countries = &world().countries;
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = |bound: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize % bound.max(1)
    };
    for case in 0..400 {
        let mut c = Campaign::default();
        for _ in 0..next(12) {
            let country = &countries[next(countries.len())];
            // Duplicates are possible on purpose: the scan counts them once.
            c.world.cleared.push(country.id.clone());
        }
        for _ in 0..next(16) {
            let country = &countries[next(countries.len())];
            let count = country.level_count();
            // Below, at and (invalid) above the country's stage count.
            let value = match next(4) {
                0 => count,
                1 => count + 1,
                _ => next(count),
            };
            c.world.clear_high_water.insert(country.id.clone(), value);
        }
        if case % 7 == 0 {
            // Ids outside the catalogue never count.
            c.world.cleared.push("ZZZ".into());
            c.world.clear_high_water.insert("ZZY".into(), 3);
        }
        assert_eq!(c.liberated_count(), full_scan_liberated(&c), "case {case}");
    }
    assert_eq!(Campaign::default().liberated_count(), 0);
    let all = allowance_campaign(&country_ids(|_| true, countries.len()), 0);
    assert_eq!(all.liberated_count(), full_scan_liberated(&all));
}

// ---------------------------------------------------------------------------
// W2-11: the takeover core (D6 + D8). A Ready recruit defends the main run
// with its own loadout, tactics and identity. Its roster entry is the
// battle's npc, so progress and credit land there, and finish releases it.
// ---------------------------------------------------------------------------

/// Recruit 1 (Alex Rowan, a Male identity) carrying a bought rifle Mara does
/// not hold, in a strategic game between defenses.
fn takeover_ready() -> (Game, u64) {
    let (mut g, _, id) = game_with_alex();
    let rifle = g.campaign_mut().add_item(4).unwrap();
    g.apply(&Command::EquipRecruit { id, item: rifle }).unwrap();
    assert_ne!(
        roster_npc(&g, id).loadout,
        g.campaign().npc.loadout,
        "the recruit's loadout differs from Mara's"
    );
    (g, id)
}

fn roster_npc(g: &Game, id: u64) -> &Npc {
    &g.strategy().roster.iter().find(|r| r.id == id).unwrap().npc
}

fn roster_npc_mut(g: &mut Game, id: u64) -> &mut Npc {
    &mut g
        .staging()
        .strategy()
        .roster
        .iter_mut()
        .find(|r| r.id == id)
        .unwrap()
        .npc
}

/// Deploys recruit `id` on the active country's first stage, unpaused.
fn take_over(g: &mut Game, id: u64) {
    g.deploy_country_stage_as(0, DefenderId::Recruit(id))
        .unwrap_or_else(|e| panic!("takeover deploy: {e}"));
    g.set_paused(false);
}

/// The same game with Mara replaced by a copy of recruit `id` and deployed
/// as Mara: what the takeover must equal in battle.
fn mara_as_recruit(g: &Game, id: u64) -> Game {
    let mut twin = g.clone();
    twin.campaign_mut().npc = roster_npc(g, id).clone();
    twin.deploy_country_stage(0, Mode::Npc).unwrap();
    twin.set_paused(false);
    twin
}

/// An attempt's saved form without the seat and the stamped appearance,
/// the only two fields a takeover adds to a Mara deploy of the same npc.
fn without_identity(g: &Game) -> serde_json::Value {
    let mut value = serde_json::to_value(g.attempt().expect("live attempt")).unwrap();
    value.as_object_mut().unwrap().remove("defender");
    value["profile"]["appearance"] = serde_json::Value::Null;
    value
}

/// Saves and loads through validation.
fn reload(g: &Game) -> Game {
    let payload = g
        .to_save_json()
        .unwrap_or_else(|e| panic!("the save does not validate: {e}"));
    Game::from_payload(serde_json::from_str(&payload).unwrap(), SAVE_VERSION)
        .unwrap_or_else(|e| panic!("the save does not load: {e}"))
}

/// Ends the main run in victory on the next tick.
fn win_main(g: &mut Game) {
    enemy(g, 0, 20.0, true);
    let a = g.attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies.last_mut().unwrap().injury.unconscious = true;
    advance(g, 1);
    assert_eq!(g.last_result().unwrap().outcome, Outcome::Victory);
}

/// The Mara fields a takeover must never touch.
fn mara_ledger(g: &Game) -> (u64, u64, u64, u64, BTreeSet<String>) {
    let npc = &g.campaign().npc;
    (
        npc.attempts,
        npc.victories,
        npc.successful_hits,
        npc.combat_ticks,
        npc.first_clears.clone(),
    )
}

#[test]
fn recruit_takeover_deploys_with_recruit_loadout_and_identity() {
    let (mut g, id) = takeover_ready();
    let mara = g.campaign().npc.clone();
    let recruit = roster_npc(&g, id).clone();
    let twin = mara_as_recruit(&g, id);
    take_over(&mut g, id);

    let a = g.attempt().unwrap();
    assert_eq!(a.defender(), DefenderId::Recruit(id));
    assert_eq!(a.mode, Mode::Npc);
    assert_eq!(a.profile.appearance, recruit_identity(id).1);
    assert_eq!(
        twin.attempt().unwrap().profile.appearance,
        Appearance::Female
    );
    assert_eq!(
        without_identity(&g),
        without_identity(&twin),
        "the takeover deploys exactly what Mara would with the recruit's npc and loadout"
    );
    let names: Vec<_> = a.gear_snapshot.iter().map(|r| r.name.clone()).collect();
    let expected: Vec<_> = recruit
        .loadout
        .iter()
        .map(|item| g.campaign().item(*item).unwrap().definition().name.clone())
        .collect();
    assert_eq!(
        names, expected,
        "the gear snapshot is the recruit's loadout"
    );

    assert_eq!(status(&g, id), RecruitStatus::Defending);
    let c = g.campaign();
    assert_eq!(c.reserved, Some(Mode::Npc));
    assert_eq!(c.reserved_recruit, Some(id));
    for item in recruit.loadout {
        assert!(c.item_reserved(item), "recruit item {item}");
    }
    for item in mara.loadout {
        assert!(!c.item_reserved(item), "Mara's idle item {item}");
    }
    assert_eq!(c.npc, mara, "Mara is untouched");
    let saved = serde_json::to_value(&g).unwrap();
    assert_eq!(saved["attempt"]["defender"], json!({"Recruit": id}));
    let loaded = reload(&g);
    assert_eq!(
        loaded.attempt().unwrap().defender(),
        DefenderId::Recruit(id)
    );
}

#[test]
fn takeover_preflight_rejects_without_a_change() {
    let (mut g, id) = takeover_ready();
    let country = g.campaign().world.cleared[0].clone();
    let guard = recruit(&mut g, &country);
    g.apply(&Command::AssignGuard { id: guard, country })
        .unwrap();
    let before = serde_json::to_value(&g).unwrap();
    for defender in [DefenderId::Recruit(99), DefenderId::Recruit(guard)] {
        let mut attempt = g.clone();
        assert!(
            attempt.deploy_country_stage_as(0, defender).is_err(),
            "{defender:?}"
        );
        assert_eq!(
            serde_json::to_value(&attempt).unwrap(),
            before,
            "{defender:?}"
        );
    }
    // A defense in progress blocks a second one, also for the recruit.
    let mut live = g.clone();
    live.deploy_country_stage(0, Mode::Npc).unwrap();
    let held = serde_json::to_value(&live).unwrap();
    assert!(
        live.deploy_country_stage_as(0, DefenderId::Recruit(id))
            .is_err()
    );
    assert_eq!(serde_json::to_value(&live).unwrap(), held);
    // Mode owners keep their mode-based deploy.
    let mut mara = g.clone();
    mara.deploy_country_stage_as(0, DefenderId::Mara).unwrap();
    let mut legacy = g.clone();
    legacy.deploy_country_stage(0, Mode::Npc).unwrap();
    assert_eq!(
        serde_json::to_value(&mara).unwrap(),
        serde_json::to_value(&legacy).unwrap()
    );
}

#[test]
fn takeover_uses_recruit_recall_not_maras() {
    for (recruit_recall, mara_recall, recalled) in [(50, 0, true), (0, 50, false)] {
        let (mut g, id) = takeover_ready();
        g.campaign_mut().npc.recall_percent = mara_recall;
        roster_npc_mut(&mut g, id).recall_percent = recruit_recall;
        take_over(&mut g, id);
        g.attempt_mut().unwrap().hp = 1;
        advance(&mut g, 60);
        let label = format!("recruit {recruit_recall}%, Mara {mara_recall}%");
        if recalled {
            assert!(g.attempt().is_none(), "{label}");
            let report = g.last_result().unwrap();
            assert_eq!(report.outcome, Outcome::Withdrawal, "{label}");
            assert_eq!(
                report.cause, "Survivor recalled at the configured health threshold",
                "{label}"
            );
            assert_eq!(report.defender(), DefenderId::Recruit(id), "{label}");
        } else {
            assert!(
                g.attempt().is_some(),
                "{label}: Mara's threshold is not the recruit's"
            );
        }
    }
}

#[test]
fn takeover_battle_follows_the_recruits_tactics_tick_for_tick() {
    let (mut g, id) = takeover_ready();
    // Mara's tactics differ from the recruit's in every field.
    let mara = &mut g.campaign_mut().npc;
    mara.policy = Policy::Guardian;
    mara.prefer_aim = false;
    mara.early_reload = true;
    mara.allow_risk = true;
    let r = roster_npc_mut(&mut g, id);
    r.policy = Policy::Marksman;
    r.prefer_aim = true;
    r.early_reload = false;
    r.allow_risk = false;
    let mut twin = mara_as_recruit(&g, id);
    take_over(&mut g, id);
    enemy(&mut g, 0, 30.0, false);
    enemy(&mut twin, 0, 30.0, false);
    for step in 0..30 {
        advance(&mut g, 60);
        advance(&mut twin, 60);
        let (Some(_), Some(_)) = (g.attempt(), twin.attempt()) else {
            panic!("both runs end together at step {step}");
        };
        assert_eq!(without_identity(&g), without_identity(&twin), "step {step}");
    }
}

#[test]
fn takeover_progress_accrues_to_roster_recruit_only() {
    let (mut g, id) = takeover_ready();
    // One npc turn short of the combat-time objective.
    roster_npc_mut(&mut g, id).combat_ticks = 7200 - 60;
    let mara = mara_ledger(&g);
    let mara_awarded = g.campaign().npc.awarded;
    let mut twin = mara_as_recruit(&g, id);
    take_over(&mut g, id);
    enemy(&mut g, 0, 30.0, false);
    enemy(&mut twin, 0, 30.0, false);
    advance(&mut g, 1200);
    advance(&mut twin, 1200);
    let (recruit, twin_npc) = (roster_npc(&g, id), &twin.campaign().npc);
    assert!(recruit.combat_ticks >= 7200, "{}", recruit.combat_ticks);
    assert!(
        recruit.objectives_claimed[1],
        "the recruit claims its objective"
    );
    assert!(g.attempt().unwrap().hits > 0, "the run lands hits");
    assert_eq!(
        (
            recruit.successful_hits,
            recruit.combat_ticks,
            recruit.objectives_claimed,
            recruit.awarded,
            recruit.unspent,
        ),
        (
            twin_npc.successful_hits,
            twin_npc.combat_ticks,
            twin_npc.objectives_claimed,
            twin_npc.awarded,
            twin_npc.unspent,
        ),
        "the recruit progresses exactly as Mara would in its place"
    );
    assert_eq!(mara_ledger(&g), mara, "Mara gains nothing");
    assert_eq!(g.campaign().npc.awarded, mara_awarded);
    // The progress is saved on the roster entry and survives a reload.
    assert_eq!(roster_npc(&reload(&g), id), roster_npc(&g, id));
}

#[test]
fn takeover_victory_credits_recruit_not_mara() {
    let (mut g, id) = takeover_ready();
    let mara = mara_ledger(&g);
    let victories = g.campaign().victories;
    let player = (g.campaign().player.level, g.campaign().player.xp);
    let mut twin = mara_as_recruit(&g, id);
    take_over(&mut g, id);
    advance(&mut g, 120);
    advance(&mut twin, 120);
    win_main(&mut g);
    win_main(&mut twin);

    let report = g.last_result().unwrap();
    assert!(report.first_clear);
    assert_eq!(report.defender(), DefenderId::Recruit(id));
    assert!(report.completion_xp > 0);
    let level_id = content().levels[report.level].id.clone();
    let recruit = roster_npc(&g, id);
    assert!(recruit.first_clears.contains(&level_id));
    assert_eq!(
        recruit,
        &twin.campaign().npc,
        "the recruit is credited exactly as Mara would be in its place"
    );
    assert_eq!(recruit.victories, 1);
    assert_eq!(recruit.attempts, 1);
    assert_eq!(mara_ledger(&g), mara, "Mara is not credited");
    // The player's completion credit is the same for every defender.
    let c = g.campaign();
    assert_eq!(c.victories, victories + 1);
    assert_ne!((c.player.level, c.player.xp), player);
    assert_eq!(
        (c.player.level, c.player.xp),
        (twin.campaign().player.level, twin.campaign().player.xp)
    );
    assert!(c.first_clears.contains(&level_id));
    let record = c.run_history.last().unwrap();
    assert_eq!(record.recruit_id, Some(id));
    assert_eq!(record.worker_id, None);
    assert_eq!(record.mode, Mode::Npc);
    assert_eq!(record.defender_name, recruit_identity(id).0);
    assert_eq!(
        twin.campaign().run_history.last().unwrap().recruit_id,
        None,
        "a Mara record names no recruit"
    );
    assert_eq!(status(&g, id), RecruitStatus::Ready);
    reload(&g);
}

#[test]
fn takeover_defeat_and_withdraw_return_recruit_ready_and_release_items() {
    for outcome in [Outcome::Defeat, Outcome::Withdrawal] {
        let (mut g, id) = takeover_ready();
        let mara = mara_ledger(&g);
        let loadout = roster_npc(&g, id).loadout;
        let attempts = g.campaign().attempts;
        take_over(&mut g, id);
        advance(&mut g, 120);
        if outcome == Outcome::Defeat {
            g.attempt_mut().unwrap().injury.unconscious = true;
            advance(&mut g, 1);
        } else {
            g.apply(&Command::Withdraw).unwrap();
        }
        let label = format!("{outcome:?}");
        assert!(g.attempt().is_none(), "{label}");
        let report = g.last_result().unwrap();
        assert_eq!(report.outcome, outcome, "{label}");
        assert_eq!(report.defender(), DefenderId::Recruit(id), "{label}");
        assert_eq!(status(&g, id), RecruitStatus::Ready, "{label}");
        let c = g.campaign();
        assert_eq!(c.reserved, None, "{label}");
        assert_eq!(c.reserved_recruit, None, "{label}");
        for item in loadout {
            assert!(!c.item_reserved(item), "{label}: item {item} released");
        }
        assert!(c.reserved_items.is_empty(), "{label}");
        assert_eq!(c.attempts, attempts + 1, "{label}");
        assert_eq!(roster_npc(&g, id).attempts, 1, "{label}");
        assert_eq!(roster_npc(&g, id).victories, 0, "{label}");
        assert_eq!(mara_ledger(&g), mara, "{label}: Mara is untouched");
        let record = c.run_history.last().unwrap();
        assert_eq!(
            (record.recruit_id, record.outcome),
            (Some(id), outcome),
            "{label}"
        );
        reload(&g);
        // Released for real: the recruit can be re-equipped and defends again.
        let mut again = g.clone();
        let spare = again.campaign_mut().add_item(4).unwrap();
        again
            .apply(&Command::EquipRecruit { id, item: spare })
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        take_over(&mut again, id);
        assert_eq!(status(&again, id), RecruitStatus::Defending, "{label}");
    }
}

#[test]
fn takeover_save_reload_mid_battle_continues_identically() {
    let (mut g, id) = takeover_ready();
    take_over(&mut g, id);
    enemy(&mut g, 1, 40.0, false);
    advance(&mut g, 900);
    assert!(g.attempt().is_some());
    let mut loaded = reload(&g);
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&g).unwrap()
    );
    for step in 0..40 {
        advance(&mut g, 60);
        advance(&mut loaded, 60);
        assert_eq!(
            serde_json::to_value(&loaded).unwrap(),
            serde_json::to_value(&g).unwrap(),
            "step {step}"
        );
    }
    g.apply(&Command::Withdraw).unwrap();
    loaded.apply(&Command::Withdraw).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&g).unwrap()
    );
    assert_eq!(status(&loaded, id), RecruitStatus::Ready);
}

const FIXTURE_V7: &str = include_str!("fixtures/save_v7.json");

#[test]
fn v7_fixture_withdraw_returns_defending_recruit_ready_and_releases_items() {
    let mut g = Game::decode_save_generation(FIXTURE_V7.as_bytes()).expect("v7 fixture loads");
    let id = 1;
    assert_eq!(g.attempt().unwrap().defender(), DefenderId::Recruit(id));
    assert_eq!(status(&g, id), RecruitStatus::Defending);
    let loadout = roster_npc(&g, id).loadout;
    assert!(loadout.iter().all(|item| g.campaign().item_reserved(*item)));
    let mara = mara_ledger(&g);
    let attempts = roster_npc(&g, id).attempts;
    let records = g.campaign().run_history.len();
    let mut expedition_items: Vec<u64> = g
        .battles()
        .iter()
        .filter(|b| !b.settled)
        .flat_map(|b| b.loadout)
        .collect();
    expedition_items.sort_unstable();

    g.apply(&Command::Withdraw).unwrap();
    assert!(g.attempt().is_none());
    assert_eq!(g.last_result().unwrap().defender(), DefenderId::Recruit(id));
    assert_eq!(status(&g, id), RecruitStatus::Ready);
    let c = g.campaign();
    assert_eq!((c.reserved, c.reserved_recruit), (None, None));
    for item in loadout {
        assert!(!c.item_reserved(item), "item {item} released");
    }
    let mut still = c.reserved_items.clone();
    still.sort_unstable();
    assert_eq!(
        still, expedition_items,
        "only the live expedition keeps its items"
    );
    assert_eq!(roster_npc(&g, id).attempts, attempts + 1);
    assert_eq!(mara_ledger(&g), mara, "Mara is untouched");
    assert_eq!(c.run_history.len(), records + 1);
    let record = c.run_history.last().unwrap();
    assert_eq!((record.recruit_id, record.worker_id), (Some(id), None));
    assert_eq!(record.defender_name, roster_npc(&g, id).name);
    reload(&g);
}

/// One digest over mode-owner runs that reach every finish path: a Mara
/// victory, a Mara recall, a Mara defeat and a Personal withdrawal.
fn mode_owner_digest() -> u64 {
    let mut digest = support::fnv1a(b"mode-owner");
    let mut fold = |name: &str, g: &Game| {
        assert!(g.attempt().is_none(), "{name} finished");
        digest = support::fnv1a_extend(digest, name.as_bytes());
        digest = support::fnv1a_extend(digest, &g.state_digest().to_le_bytes());
    };
    let mut victory = support::smoke_run(1, Mode::Npc, 900, |_| {});
    win_main(&mut victory);
    fold("mara victory", &victory);

    let mut recall = support::smoke_run(2, Mode::Npc, 600, |_| {});
    recall.campaign_mut().npc.recall_percent = 50;
    recall.attempt_mut().unwrap().hp = 1;
    advance(&mut recall, 60);
    fold("mara recall", &recall);

    let mut defeat = support::smoke_run(3, Mode::Npc, 600, |_| {});
    defeat.attempt_mut().unwrap().injury.unconscious = true;
    advance(&mut defeat, 1);
    fold("mara defeat", &defeat);

    let mut personal = support::smoke_run(1, Mode::Personal, 900, |_| {});
    personal.apply(&Command::Withdraw).unwrap();
    fold("personal withdrawal", &personal);
    digest
}

/// Pin: the mode-owner deploy, tick and finish paths are byte-identical to
/// the pre-takeover code (the digest was recorded on its parent commit).
/// Re-recorded at W2-08 (content, class C): the 39 loot-only items join the
/// drop pool of attempts deployed from v7 on, so the drops these runs bank
/// change definition; the takeover code did not move.
/// Re-recorded at W2-52 (format, class F): a player level-up keeps the
/// neutral `level_grant_paid` ledger at the level, so the runs that level
/// the player now save it; the takeover code did not move.
/// Re-recorded at W2-09 (gameplay, class G): the weighted DropTable picks
/// the drops these runs bank (same one draw, other definitions); with
/// neutral weights the old digest returns, and the takeover code did not
/// move.
/// Re-recorded at W2-54 (content): `progression.json` authors the level
/// grant (0.25) and the rank milestones, so the Personal runs that level the
/// player owe `level_grant_paid` mid-run and the withdrawal pays the grant
/// (coins, ledger and receipt); with a neutral rewards block the W2-09
/// digest returns, and the takeover code did not move.
#[test]
fn mode_owner_paths_are_byte_identical() {
    assert_eq!(mode_owner_digest(), MODE_OWNER_DIGEST);
}
const MODE_OWNER_DIGEST: u64 = 6_823_137_652_472_446_824;

/// Pin: Mara's `attempts` count moves exactly as the legacy
/// `finish_attempt` moved it: one per finished Mara run of any outcome,
/// none for a Personal run, and a worker's run counts on the worker's copy,
/// which settlement hands back to the recruit.
#[test]
fn mara_finish_matches_legacy_attempt_counting() {
    let mut g = support::game(Mode::Npc);
    assert_eq!(g.campaign().npc.attempts, 0);
    g.apply(&Command::Withdraw).unwrap();
    assert_eq!(g.campaign().npc.attempts, 1, "withdrawal");
    g.deploy(0, Mode::Npc).unwrap();
    g.set_paused(false);
    g.attempt_mut().unwrap().injury.unconscious = true;
    advance(&mut g, 1);
    assert_eq!(g.last_result().unwrap().outcome, Outcome::Defeat);
    assert_eq!(g.campaign().npc.attempts, 2, "defeat");
    g.deploy(0, Mode::Npc).unwrap();
    g.set_paused(false);
    win_main(&mut g);
    assert_eq!(g.campaign().npc.attempts, 3, "victory");
    g.deploy(0, Mode::Personal).unwrap();
    g.apply(&Command::Withdraw).unwrap();
    assert_eq!(g.campaign().npc.attempts, 3, "a Personal run");
    assert_eq!(g.campaign().attempts, 1 + 4);

    let (mut g, country, id) = game_with_alex();
    let b = g.deploy_survivor(id, &country, 0).unwrap();
    win_stage(&mut g, b);
    assert!(battle(&g, b).settled);
    assert_eq!(battle(&g, b).game().campaign().npc.attempts, 1);
    assert_eq!(
        roster_npc(&g, id).attempts,
        1,
        "settled back to the recruit"
    );
    assert_eq!(
        g.campaign().npc.attempts,
        0,
        "Mara never counts a worker run"
    );
}

// ---------------------------------------------------------------------------
// W2-12: choosing the next defender (D9). `Campaign.next_defender` is saved;
// `Game::next_defender()` falls back to the last report's defender.
// `SetNextDefender` and `BeginDefense` are appended commands; `Deploy` keeps
// its recorded mode-owner meaning. Dispatching the chosen recruit elsewhere
// hands the next defense back to the player with a note.
// ---------------------------------------------------------------------------

/// The strategic note a dispatch leaves when it takes the next defender.
fn left_note(name: &str) -> String {
    format!("{name} left the next defense; you will defend next.")
}

fn apply_ok(g: &mut Game, command: Command) {
    g.apply(&command)
        .unwrap_or_else(|e| panic!("{command:?}: {e}"));
}

/// A front the recruit can open a country operation on.
fn operation_front(g: &Game) -> String {
    world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id)
                && !g.campaign().world.cleared.contains(&c.id)
                && c.level_count() > 1
        })
        .expect("an enterable front")
        .id
        .clone()
}

#[test]
fn next_defender_persists_through_save_load_and_continue_deploys_it() {
    let (mut g, id) = takeover_ready();
    apply_ok(
        &mut g,
        Command::Deploy {
            stage: 0,
            mode: Mode::Personal,
        },
    );
    apply_ok(&mut g, Command::Withdraw);
    assert_eq!(
        g.next_defender(),
        DefenderId::Player,
        "the report's defender"
    );
    assert_eq!(
        g.apply(&Command::SetNextDefender(DefenderId::Recruit(id))),
        Ok(CommandOutcome::Applied)
    );
    assert_eq!(g.campaign().next_defender, Some(DefenderId::Recruit(id)));
    let saved = serde_json::to_value(&g).unwrap();
    assert_eq!(saved["campaign"]["next_defender"], json!({"Recruit": id}));

    let mut g = reload(&g);
    assert_eq!(g.campaign().next_defender, Some(DefenderId::Recruit(id)));
    assert_eq!(g.next_defender(), DefenderId::Recruit(id));
    assert_eq!(
        g.apply(&Command::ContinueJourney),
        Ok(CommandOutcome::Applied)
    );
    let a = g.attempt().expect("continue deployed");
    assert_eq!(a.defender(), DefenderId::Recruit(id));
    assert_eq!(a.profile.appearance, recruit_identity(id).1);
    assert!(!a.paused && !g.global_paused(), "continue unpauses");
    assert_eq!(status(&g, id), RecruitStatus::Defending);
    assert_eq!(g.campaign().next_defender, None, "cleared on success");
    assert!(
        serde_json::to_value(&g).unwrap()["campaign"]
            .get("next_defender")
            .is_none(),
        "None is not written"
    );
    apply_ok(&mut g, Command::Withdraw);
    assert_eq!(
        g.next_defender(),
        DefenderId::Recruit(id),
        "None falls back to the recruit's report"
    );
    reload(&g);
}

#[test]
fn set_next_defender_typed_rejections_leave_state_unchanged() {
    let unavailable = |defender, blocker| CommandError::DefenderUnavailable { defender, blocker };
    let (ready, id) = takeover_ready();
    let country = ready.campaign().world.cleared[0].clone();
    let mut rows: Vec<(&str, Game, DefenderId, CommandError)> = Vec::new();

    let mut live = ready.clone();
    live.deploy_country_stage(0, Mode::Personal).unwrap();
    for defender in [
        DefenderId::Player,
        DefenderId::Mara,
        DefenderId::Recruit(id),
    ] {
        rows.push((
            "live defense",
            live.clone(),
            defender,
            CommandError::DefenseInProgress,
        ));
    }
    rows.push((
        "Mara before Companion",
        Game::default(),
        DefenderId::Mara,
        unavailable(
            DefenderId::Mara,
            DefenderBlocker::Locked {
                node_id: "companion_contact".into(),
            },
        ),
    ));
    let mut locked = ready.clone();
    locked.campaign_mut().legacy_features = false;
    rows.push((
        "recruit before Recruitment",
        locked,
        DefenderId::Recruit(id),
        unavailable(
            DefenderId::Recruit(id),
            DefenderBlocker::Locked {
                node_id: "recruitment_network".into(),
            },
        ),
    ));
    let mut guarding = ready.clone();
    apply_ok(
        &mut guarding,
        Command::AssignGuard {
            id,
            country: country.clone(),
        },
    );
    rows.push((
        "guarding recruit",
        guarding,
        DefenderId::Recruit(id),
        unavailable(
            DefenderId::Recruit(id),
            DefenderBlocker::Guarding {
                country_id: country.clone(),
            },
        ),
    ));
    let mut away = ready.clone();
    let battle = away.deploy_survivor(id, &country, 0).unwrap();
    rows.push((
        "recruit on an expedition",
        away,
        DefenderId::Recruit(id),
        unavailable(
            DefenderId::Recruit(id),
            DefenderBlocker::OnExpedition { battle },
        ),
    ));
    let mut broken = ready.clone();
    roster_npc_mut(&mut broken, id).loadout[0] = 999_999;
    rows.push((
        "recruit with an invalid loadout",
        broken,
        DefenderId::Recruit(id),
        unavailable(
            DefenderId::Recruit(id),
            DefenderBlocker::InvalidLoadout {
                reason: "Preset references sold equipment".into(),
            },
        ),
    ));
    rows.push((
        "unknown recruit",
        ready.clone(),
        DefenderId::Recruit(99),
        CommandError::UnknownSurvivor,
    ));

    for (label, mut g, defender, error) in rows {
        let before = serde_json::to_value(&g).unwrap();
        assert_eq!(
            g.apply(&Command::SetNextDefender(defender)),
            Err(error.clone()),
            "{label}"
        );
        assert_eq!(serde_json::to_value(&g).unwrap(), before, "{label}");
        // BeginDefense rejects the same blocked defender without a change.
        if !matches!(error, CommandError::DefenseInProgress) {
            g.staging().campaign().next_defender = Some(defender);
            let staged = serde_json::to_value(&g).unwrap();
            assert_eq!(
                g.apply(&Command::BeginDefense { stage: 0 }),
                Err(error),
                "{label}: BeginDefense"
            );
            assert_eq!(serde_json::to_value(&g).unwrap(), staged, "{label}");
        }
    }
    // Control: between defenses each of them is accepted.
    for defender in [
        DefenderId::Player,
        DefenderId::Mara,
        DefenderId::Recruit(id),
    ] {
        let mut g = ready.clone();
        assert_eq!(
            g.apply(&Command::SetNextDefender(defender)),
            Ok(CommandOutcome::Applied),
            "{defender:?}"
        );
        assert_eq!(g.campaign().next_defender, Some(defender));
    }
}

#[test]
fn begin_defense_for_mara_matches_legacy_deploy_digest() {
    let (base, _) = takeover_ready();
    for (defender, mode) in [
        (DefenderId::Mara, Mode::Npc),
        (DefenderId::Player, Mode::Personal),
    ] {
        let mut legacy = base.clone();
        apply_ok(&mut legacy, Command::Deploy { stage: 0, mode });
        let mut chosen = base.clone();
        apply_ok(&mut chosen, Command::SetNextDefender(defender));
        assert_eq!(
            chosen.apply(&Command::BeginDefense { stage: 0 }),
            Ok(CommandOutcome::Applied)
        );
        assert_eq!(chosen.campaign().next_defender, None, "{defender:?}");
        assert_eq!(chosen.state_digest(), legacy.state_digest(), "{defender:?}");
        enemy(&mut legacy, 1, 40.0, false);
        enemy(&mut chosen, 1, 40.0, false);
        advance(&mut legacy, 600);
        advance(&mut chosen, 600);
        assert_eq!(chosen.state_digest(), legacy.state_digest(), "{defender:?}");
    }
    // With nothing chosen and no report, BeginDefense is the player's deploy.
    let mut legacy = base.clone();
    apply_ok(
        &mut legacy,
        Command::Deploy {
            stage: 0,
            mode: Mode::Personal,
        },
    );
    let mut fallback = base.clone();
    apply_ok(&mut fallback, Command::BeginDefense { stage: 0 });
    assert_eq!(fallback.state_digest(), legacy.state_digest());
}

#[test]
fn deploy_command_stays_player_or_mara_with_next_defender_set() {
    let (mut g, id) = takeover_ready();
    apply_ok(&mut g, Command::SetNextDefender(DefenderId::Recruit(id)));
    for (mode, defender) in [
        (Mode::Npc, DefenderId::Mara),
        (Mode::Personal, DefenderId::Player),
    ] {
        apply_ok(&mut g, Command::Deploy { stage: 0, mode });
        assert_eq!(g.attempt().unwrap().defender(), defender, "{mode:?}");
        assert_eq!(status(&g, id), RecruitStatus::Ready, "{mode:?}");
        assert_eq!(g.campaign().reserved_recruit, None, "{mode:?}");
        assert_eq!(
            g.campaign().next_defender,
            Some(DefenderId::Recruit(id)),
            "Deploy never touches next_defender ({mode:?})"
        );
        apply_ok(&mut g, Command::Withdraw);
        assert_eq!(g.campaign().next_defender, Some(DefenderId::Recruit(id)));
        assert_eq!(g.next_defender(), DefenderId::Recruit(id));
    }
}

#[test]
fn dispatching_the_next_defender_resets_it_to_player_with_note() {
    let (ready, id) = takeover_ready();
    let country = ready.campaign().world.cleared[0].clone();
    let front = operation_front(&ready);
    let name = roster_npc(&ready, id).name.clone();
    let dispatches = |id: u64| {
        [
            Command::AssignGuard {
                id,
                country: country.clone(),
            },
            Command::DeploySurvivor {
                id,
                country: country.clone(),
                stage: 0,
            },
            Command::CountryOperation {
                id,
                country: front.clone(),
            },
        ]
    };
    for command in dispatches(id) {
        let mut g = ready.clone();
        apply_ok(&mut g, Command::SetNextDefender(DefenderId::Recruit(id)));
        apply_ok(&mut g, command.clone());
        assert_eq!(
            g.campaign().next_defender,
            Some(DefenderId::Player),
            "{command:?}"
        );
        assert_eq!(g.next_defender(), DefenderId::Player, "{command:?}");
        assert_eq!(
            g.strategy().log.last(),
            Some(&left_note(&name)),
            "{command:?}"
        );
        reload(&g);
    }

    // The fallback counts: the last report's recruit is the next defender.
    let mut g = ready.clone();
    take_over(&mut g, id);
    apply_ok(&mut g, Command::Withdraw);
    assert_eq!(
        (g.campaign().next_defender, g.next_defender()),
        (None, DefenderId::Recruit(id))
    );
    apply_ok(&mut g, dispatches(id)[0].clone());
    assert_eq!(g.campaign().next_defender, Some(DefenderId::Player));
    assert_eq!(g.strategy().log.last(), Some(&left_note(&name)));

    // Dispatching any other recruit leaves the choice and adds no note.
    let mut g = ready.clone();
    let other = recruit(&mut g, &country);
    apply_ok(&mut g, Command::SetNextDefender(DefenderId::Recruit(id)));
    for command in dispatches(other).into_iter().take(2) {
        let mut g = g.clone();
        let log = g.strategy().log.len();
        apply_ok(&mut g, command.clone());
        assert_eq!(
            g.campaign().next_defender,
            Some(DefenderId::Recruit(id)),
            "{command:?}"
        );
        assert!(
            !g.strategy().log[log.saturating_sub(1)..]
                .iter()
                .any(|line| line.contains("left the next defense")),
            "{command:?}"
        );
    }
}

#[test]
fn recruit_on_main_run_cannot_be_dispatched_trained_or_equipped() {
    let (mut g, id) = takeover_ready();
    let country = g.campaign().world.cleared[0].clone();
    let front = operation_front(&g);
    let spare = g.campaign_mut().add_item(4).unwrap();
    apply_ok(&mut g, Command::BuyRecruitPoint(id));
    apply_ok(&mut g, Command::SetNextDefender(DefenderId::Recruit(id)));
    apply_ok(&mut g, Command::BeginDefense { stage: 0 });
    assert_eq!(g.attempt().unwrap().defender(), DefenderId::Recruit(id));
    assert_eq!(status(&g, id), RecruitStatus::Defending);
    let before = serde_json::to_value(&g).unwrap();
    let commands = [
        Command::AssignGuard {
            id,
            country: country.clone(),
        },
        Command::DeploySurvivor {
            id,
            country: country.clone(),
            stage: 0,
        },
        Command::CountryOperation { id, country: front },
        Command::TrainRecruit {
            id,
            training: NpcTraining::Guns,
        },
        Command::EquipRecruit { id, item: spare },
        Command::BuyRecruitPoint(id),
    ];
    for command in &commands {
        assert_eq!(
            g.apply(command),
            Err(CommandError::DefenderUnavailable {
                defender: DefenderId::Recruit(id),
                blocker: DefenderBlocker::Defending,
            }),
            "{command:?}"
        );
        assert_eq!(serde_json::to_value(&g).unwrap(), before, "{command:?}");
    }
    assert_eq!(
        g.apply(&Command::SetNextDefender(DefenderId::Player)),
        Err(CommandError::DefenseInProgress)
    );
    assert_eq!(
        g.apply(&Command::BeginDefense { stage: 0 }),
        Err(CommandError::Rejected(
            "Finish or recall the active deployment first".into()
        ))
    );
    assert_eq!(serde_json::to_value(&g).unwrap(), before);
}

const FIXTURE_V6: &str = include_str!("fixtures/save_v6.json");

#[test]
fn migrated_v6_continues_with_mara() {
    let mut g = Game::decode_save_generation(FIXTURE_V6.as_bytes()).expect("v6 fixture loads");
    assert_eq!(g.campaign().next_defender, None);
    // The live Personal defense ends; the settled Mara report it followed is
    // the one the next run repeats.
    let report = g.last_result().cloned().expect("a settled report");
    assert_eq!(report.defender(), DefenderId::Mara);
    g.withdraw();
    *g.staging().last_result() = Some(report);
    assert_eq!(g.next_defender(), DefenderId::Mara);

    let mut retried = g.clone();
    retried.retry().expect("retry");
    let mut repeated = g.clone();
    repeated.start_repeat(2).expect("repeat");
    for (label, run) in [("retry", &retried), ("repeat", &repeated)] {
        let a = run.attempt().unwrap_or_else(|| panic!("{label} deployed"));
        assert_eq!(a.defender(), DefenderId::Mara, "{label}");
        assert_eq!(
            (a.country_id.as_deref(), a.stage),
            (Some("HUN"), 2),
            "{label} replays the reported stage"
        );
        let card = run.active_defender().expect("a live defense");
        assert_eq!(card.id, DefenderId::Mara, "{label}");
        assert_eq!(card.name, run.campaign().npc.name, "{label}");
        assert_eq!(card.role, DefenderRole::Companion, "{label}");
        assert_eq!(run.campaign().next_defender, None, "{label}");
    }
}

#[test]
fn retry_and_repeat_redeploy_the_reports_recruit() {
    let (mut g, id) = takeover_ready();
    take_over(&mut g, id);
    win_main(&mut g);
    assert_eq!(g.last_result().unwrap().defender(), DefenderId::Recruit(id));
    // A Mara choice for the next defense does not redirect a retry or repeat.
    apply_ok(&mut g, Command::SetNextDefender(DefenderId::Mara));
    let mut retried = g.clone();
    retried.retry().expect("retry");
    let mut repeated = g.clone();
    repeated.start_repeat(2).expect("repeat");
    for (label, run) in [("retry", &retried), ("repeat", &repeated)] {
        assert_eq!(
            run.attempt().unwrap().defender(),
            DefenderId::Recruit(id),
            "{label}"
        );
        assert_eq!(status(run, id), RecruitStatus::Defending, "{label}");
        assert_eq!(
            run.campaign().next_defender,
            Some(DefenderId::Mara),
            "{label} keeps the choice"
        );
    }
}

/// One verdict per defender: a guarding recruit with Recruitment locked is
/// refused for the same first reason by `SetNextDefender`, `BeginDefense`,
/// `ContinueJourney` and retry (repeats share retry's redeploy).
#[test]
fn every_entry_point_gives_a_blocked_recruit_the_same_reason() {
    let (mut g, id) = takeover_ready();
    take_over(&mut g, id);
    apply_ok(&mut g, Command::Withdraw);
    assert_eq!(g.last_result().unwrap().defender(), DefenderId::Recruit(id));
    let country = g.campaign().world.cleared[0].clone();
    apply_ok(&mut g, Command::AssignGuard { id, country });
    g.campaign_mut().legacy_features = false;
    let expected = CommandError::DefenderUnavailable {
        defender: DefenderId::Recruit(id),
        blocker: DefenderBlocker::Locked {
            node_id: "recruitment_network".into(),
        },
    };
    assert_eq!(
        g.clone()
            .apply(&Command::SetNextDefender(DefenderId::Recruit(id))),
        Err(expected.clone()),
        "SetNextDefender"
    );
    g.staging().campaign().next_defender = Some(DefenderId::Recruit(id));
    assert_eq!(
        g.clone().apply(&Command::BeginDefense { stage: 0 }),
        Err(expected.clone()),
        "BeginDefense"
    );
    assert_eq!(
        g.clone().apply(&Command::ContinueJourney),
        Err(expected.clone()),
        "ContinueJourney"
    );
    assert_eq!(g.clone().retry(), Err(expected.to_string()), "retry");
}

/// `continue_blocker` is `ContinueJourney`'s own refusal, read without a
/// change, so Results can disable Continue and say why.
#[test]
fn continue_blocker_matches_continue_journey_without_a_change() {
    let unavailable = |blocker| CommandError::DefenderUnavailable {
        defender: DefenderId::Recruit(1),
        blocker,
    };
    let (mut ready, id) = takeover_ready();
    assert_eq!(id, 1);
    apply_ok(
        &mut ready,
        Command::Deploy {
            stage: 0,
            mode: Mode::Personal,
        },
    );
    apply_ok(&mut ready, Command::Withdraw);
    let country = ready.campaign().world.cleared[0].clone();

    let mut rows: Vec<(&str, Game, Option<CommandError>)> = Vec::new();
    let mut guarding = ready.clone();
    apply_ok(
        &mut guarding,
        Command::AssignGuard {
            id,
            country: country.clone(),
        },
    );
    guarding.staging().campaign().next_defender = Some(DefenderId::Recruit(id));
    rows.push((
        "guarding next defender",
        guarding,
        Some(unavailable(DefenderBlocker::Guarding {
            country_id: country,
        })),
    ));
    let mut locked = ready.clone();
    locked.staging().campaign().next_defender = Some(DefenderId::Recruit(id));
    locked.campaign_mut().legacy_features = false;
    rows.push((
        "Recruitment locked",
        locked.clone(),
        Some(unavailable(DefenderBlocker::Locked {
            node_id: "recruitment_network".into(),
        })),
    ));
    let mut live = ready.clone();
    live.deploy_country_stage(0, Mode::Personal).unwrap();
    rows.push((
        "live defense",
        live,
        Some(CommandError::Rejected(
            "Finish the current round before continuing".into(),
        )),
    ));
    rows.push((
        "no journey",
        Game::default(),
        Some(CommandError::Rejected(
            "Choose a country to begin a journey".into(),
        )),
    ));
    // Controls: a defender who can defend, and a liberation (which deploys
    // nobody, so the blocked next defender does not stop it).
    rows.push(("ready recruit", ready.clone(), None));
    let mut won = ready.clone();
    won.deploy_country_stage(0, Mode::Npc).unwrap();
    won.set_paused(false);
    win_main(&mut won);
    won.staging().campaign().next_defender = Some(DefenderId::Recruit(id));
    won.campaign_mut().legacy_features = false;
    rows.push(("liberation", won, None));

    for (label, g, expected) in rows {
        let before = serde_json::to_value(&g).unwrap();
        assert_eq!(g.continue_blocker(), expected, "{label}");
        assert_eq!(serde_json::to_value(&g).unwrap(), before, "{label}: pure");
        let mut applied = g.clone();
        let result = applied.apply(&Command::ContinueJourney);
        assert_eq!(result.clone().err(), expected, "{label}: apply");
        if result.is_err() {
            assert_eq!(
                serde_json::to_value(&applied).unwrap(),
                before,
                "{label}: refused without a change"
            );
        }
    }
}

#[test]
fn new_defender_commands_round_trip_serde() {
    let rows = [
        (
            Command::SetNextDefender(DefenderId::Player),
            json!({"SetNextDefender": "Player"}),
        ),
        (
            Command::SetNextDefender(DefenderId::Mara),
            json!({"SetNextDefender": "Mara"}),
        ),
        (
            Command::SetNextDefender(DefenderId::Recruit(3)),
            json!({"SetNextDefender": {"Recruit": 3}}),
        ),
        (
            Command::BeginDefense { stage: 2 },
            json!({"BeginDefense": {"stage": 2}}),
        ),
    ];
    for (command, shape) in rows {
        assert_eq!(serde_json::to_value(&command).unwrap(), shape);
        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
    }
}

#[test]
fn available_defenders_lists_player_mara_recruits_with_blockers() {
    let fresh = Game::default();
    let list = fresh.available_defenders();
    let ids: Vec<_> = list.iter().map(|o| o.card.id).collect();
    assert_eq!(ids, [DefenderId::Player, DefenderId::Mara]);
    assert_eq!(list[0].selectable, Ok(()));
    assert!(list[0].selected, "the player defends a fresh campaign");
    assert_eq!(
        list[1].selectable,
        Err(DefenderBlocker::Locked {
            node_id: "companion_contact".into()
        }),
        "Mara is listed, locked, before Companion"
    );
    assert!(!list[1].selected);

    let (mut g, ready) = takeover_ready();
    let country = g.campaign().world.cleared[0].clone();
    let guard = recruit(&mut g, &country);
    let away = recruit(&mut g, &country);
    apply_ok(
        &mut g,
        Command::AssignGuard {
            id: guard,
            country: country.clone(),
        },
    );
    let battle = g.deploy_survivor(away, &country, 0).unwrap();
    apply_ok(&mut g, Command::SetNextDefender(DefenderId::Recruit(ready)));
    let before = serde_json::to_value(&g).unwrap();
    let list = g.available_defenders();
    assert_eq!(serde_json::to_value(&g).unwrap(), before, "pure");
    let rows: Vec<_> = list
        .iter()
        .map(|o| (o.card.id, o.selectable.clone(), o.selected))
        .collect();
    assert_eq!(
        rows,
        [
            (DefenderId::Player, Ok(()), false),
            (DefenderId::Mara, Ok(()), false),
            (DefenderId::Recruit(ready), Ok(()), true),
            (
                DefenderId::Recruit(guard),
                Err(DefenderBlocker::Guarding {
                    country_id: country.clone()
                }),
                false
            ),
            (
                DefenderId::Recruit(away),
                Err(DefenderBlocker::OnExpedition { battle }),
                false
            ),
        ]
    );
    // Selectable means SetNextDefender accepts it, and a blocker is the
    // command's own typed rejection.
    for option in &list {
        let mut probe = g.clone();
        let result = probe.apply(&Command::SetNextDefender(option.card.id));
        match &option.selectable {
            Ok(()) => assert_eq!(result, Ok(CommandOutcome::Applied)),
            Err(blocker) => assert_eq!(
                result,
                Err(CommandError::DefenderUnavailable {
                    defender: option.card.id,
                    blocker: blocker.clone(),
                })
            ),
        }
    }
    // During a defense the rows keep the between-defenses verdict, while
    // every SetNextDefender is DefenseInProgress (the documented contract).
    let mut live = g.clone();
    live.deploy_country_stage(0, Mode::Personal).unwrap();
    let verdicts = |list: &[DefenderOption]| -> Vec<_> {
        list.iter()
            .map(|o| (o.card.id, o.selectable.clone()))
            .collect()
    };
    assert_eq!(verdicts(&live.available_defenders()), verdicts(&list));
    for option in &list {
        assert_eq!(
            live.clone()
                .apply(&Command::SetNextDefender(option.card.id)),
            Err(CommandError::DefenseInProgress),
            "{:?}",
            option.card.id
        );
    }
}

/// The items each defender would carry (design §1.4; Briefing reads
/// `defender_loadout(next_defender())`).
#[test]
fn defender_loadout_names_each_defenders_items() {
    let (mut g, id) = takeover_ready();
    let before = serde_json::to_value(&g).unwrap();
    assert_eq!(
        g.defender_loadout(DefenderId::Player),
        Some(g.campaign().player.loadout)
    );
    assert_eq!(
        g.defender_loadout(DefenderId::Mara),
        Some(g.campaign().npc.loadout)
    );
    assert_eq!(
        g.defender_loadout(DefenderId::Recruit(id)),
        Some(roster_npc(&g, id).loadout),
        "the recruit's own items, not Mara's"
    );
    assert_eq!(g.defender_loadout(DefenderId::Recruit(99)), None);
    assert_eq!(serde_json::to_value(&g).unwrap(), before, "pure");
    apply_ok(&mut g, Command::SetNextDefender(DefenderId::Recruit(id)));
    assert_eq!(
        g.defender_loadout(g.next_defender()),
        Some(roster_npc(&g, id).loadout)
    );
}

#[test]
fn defender_cards_name_each_defender() {
    let (mut g, id) = takeover_ready();
    let before = serde_json::to_value(&g).unwrap();
    let player = g.defender_card(DefenderId::Player).unwrap();
    let survivor = g
        .campaign()
        .selected_survivor
        .unwrap_or(horde_core::survivors::SurvivorId::Rook);
    assert_eq!(player.name, survivor.name());
    assert_eq!(player.hud_name, survivor.name().to_uppercase());
    assert_eq!(player.appearance, survivor.appearance());
    assert_eq!(
        (player.role, player.mode, player.origin.clone()),
        (DefenderRole::Survivor, Mode::Personal, None)
    );
    assert_eq!(player.level, g.campaign().player.level);

    let mara = g.defender_card(DefenderId::Mara).unwrap();
    assert_eq!(
        (mara.name.as_str(), mara.hud_name.as_str()),
        ("Mara Vale", "MARA")
    );
    assert_eq!(
        (mara.appearance, mara.role, mara.mode),
        (Appearance::Female, DefenderRole::Companion, Mode::Npc)
    );
    assert_eq!(mara.level, u32::from(g.campaign().npc.advancement_level()));
    assert_eq!(
        mara.level_cap,
        u32::from(g.campaign().advancement_cap_level())
    );

    let card = g.defender_card(DefenderId::Recruit(id)).unwrap();
    let r = g.strategy().roster.iter().find(|r| r.id == id).unwrap();
    assert_eq!(
        (card.name.as_str(), card.hud_name.as_str()),
        ("Alex Rowan", "ALEX")
    );
    assert_eq!(
        (card.appearance, card.role, card.mode),
        (Appearance::Male, DefenderRole::Recruit, Mode::Npc)
    );
    let origin = card.origin.clone().expect("recruits have an origin");
    let home = world().country(&r.origin).unwrap();
    assert_eq!(
        (origin.country_id.as_str(), origin.country_name.as_str()),
        (home.id.as_str(), home.name.as_str())
    );
    assert_eq!(origin.origin_trait, OriginTrait::for_biome(&home.biome));
    assert_eq!(card.level, u32::from(r.npc.advancement_level()));
    assert!(g.defender_card(DefenderId::Recruit(99)).is_none());
    assert!(g.active_defender().is_none(), "no live defense");
    assert_eq!(serde_json::to_value(&g).unwrap(), before, "pure");

    // The live main attempt and an expedition name their recruit.
    take_over(&mut g, id);
    assert_eq!(g.active_defender(), Some(card.clone()));
    apply_ok(&mut g, Command::Withdraw);
    let country = g.campaign().world.cleared[0].clone();
    let battle = g.deploy_survivor(id, &country, 0).unwrap();
    let before = serde_json::to_value(&g).unwrap();
    assert_eq!(g.battle_defender_card(battle), Some(card));
    assert_eq!(g.battle_defender_card(battle + 100), None);
    assert!(g.active_defender().is_none());
    assert_eq!(serde_json::to_value(&g).unwrap(), before, "pure");
}

// W2-13: `SetRecruitTactics` (D12). Live tactics for a recruit: a Ready
// recruit's roster npc, or the Defending recruit's roster npc mid-battle,
// which is the battle's npc (D6), so the change acts at its next npc turn.
// Guarding and expedition recruits refuse it; `SetNpcPolicy` still edits
// Mara and `SetRecruitPolicy` stays Ready-only.
// ---------------------------------------------------------------------------

/// The five tactics fields of `npc`, as one command payload.
fn tactics_of(npc: &Npc) -> NpcTactics {
    NpcTactics {
        policy: npc.policy,
        prefer_aim: npc.prefer_aim,
        early_reload: npc.early_reload,
        allow_risk: npc.allow_risk,
        recall_percent: npc.recall_percent,
    }
}

/// Tactics that differ from `npc`'s in every field but the recall
/// threshold, which is `recall_percent`.
fn other_tactics(npc: &Npc, recall_percent: u32) -> NpcTactics {
    NpcTactics {
        policy: *Policy::ALL.iter().find(|p| **p != npc.policy).unwrap(),
        prefer_aim: !npc.prefer_aim,
        early_reload: !npc.early_reload,
        allow_risk: !npc.allow_risk,
        recall_percent,
    }
}

/// `g` with recruit `id`'s roster tactics written directly: the only change
/// an accepted `SetRecruitTactics` may make.
fn with_roster_tactics(g: &Game, id: u64, t: &NpcTactics) -> serde_json::Value {
    let mut expected = g.clone();
    let npc = roster_npc_mut(&mut expected, id);
    npc.policy = t.policy;
    npc.prefer_aim = t.prefer_aim;
    npc.early_reload = t.early_reload;
    npc.allow_risk = t.allow_risk;
    npc.recall_percent = t.recall_percent;
    serde_json::to_value(&expected).unwrap()
}

#[test]
fn set_recruit_tactics_edits_ready_and_defending_only() {
    let (ready, id) = takeover_ready();
    let country = ready.campaign().world.cleared[0].clone();
    let current = roster_npc(&ready, id).clone();
    let wanted = other_tactics(&current, MAX_RECALL_PERCENT);
    assert_ne!(wanted.recall_percent, current.recall_percent);
    let set = |tactics: NpcTactics| Command::SetRecruitTactics { id, tactics };

    // The command is appended data with a stable serde shape.
    let command = set(wanted.clone());
    let shape = serde_json::to_value(&command).unwrap();
    assert_eq!(shape["SetRecruitTactics"]["id"], json!(id));
    assert_eq!(
        shape["SetRecruitTactics"]["tactics"],
        serde_json::to_value(&wanted).unwrap()
    );
    let text = serde_json::to_string(&command).unwrap();
    assert_eq!(serde_json::from_str::<Command>(&text).unwrap(), command);

    let mut defending = ready.clone();
    take_over(&mut defending, id);
    for (label, g) in [("Ready", ready.clone()), ("Defending", defending)] {
        let mara = g.campaign().npc.clone();
        let expected = with_roster_tactics(&g, id, &wanted);
        let mut edited = g.clone();
        assert_eq!(
            edited.apply(&set(wanted.clone())),
            Ok(CommandOutcome::Applied),
            "{label}"
        );
        assert_eq!(serde_json::to_value(&edited).unwrap(), expected, "{label}");
        assert_eq!(tactics_of(roster_npc(&edited, id)), wanted, "{label}");
        assert_eq!(edited.campaign().npc, mara, "{label}: Mara is untouched");
        let loaded = reload(&edited);
        assert_eq!(tactics_of(roster_npc(&loaded, id)), wanted, "{label}");

        // A threshold above the cap is refused, with no change.
        let before = serde_json::to_value(&g).unwrap();
        let mut over = g.clone();
        let mut high = wanted.clone();
        high.recall_percent = MAX_RECALL_PERCENT + 1;
        assert_eq!(
            over.apply(&set(high)),
            Err(CommandError::RecallThresholdOutOfRange(
                MAX_RECALL_PERCENT + 1
            )),
            "{label}"
        );
        assert_eq!(serde_json::to_value(&over).unwrap(), before, "{label}");
    }

    // On the main run, SetNpcPolicy still edits Mara and SetRecruitPolicy
    // stays Ready-only.
    let mut live = ready.clone();
    take_over(&mut live, id);
    let mara_wanted = other_tactics(&live.campaign().npc, 30);
    apply_ok(&mut live, Command::SetNpcPolicy(mara_wanted.clone()));
    assert_eq!(tactics_of(&live.campaign().npc), mara_wanted);
    assert_eq!(roster_npc(&live, id), &current, "the recruit is untouched");
    let before = serde_json::to_value(&live).unwrap();
    assert_eq!(
        live.apply(&Command::SetRecruitPolicy {
            id,
            policy: wanted.policy,
        }),
        Err(CommandError::DefenderUnavailable {
            defender: DefenderId::Recruit(id),
            blocker: DefenderBlocker::Defending,
        })
    );
    assert_eq!(serde_json::to_value(&live).unwrap(), before);

    // Guarding and expedition recruits, and unknown ids, are refused with
    // no change.
    let mut guarding = ready.clone();
    apply_ok(
        &mut guarding,
        Command::AssignGuard {
            id,
            country: country.clone(),
        },
    );
    let mut away = ready.clone();
    away.deploy_survivor(id, &country, 0).unwrap();
    assert_eq!(status(&away, id), RecruitStatus::Deployed);
    let rows = [
        ("Guarding", guarding, id, CommandError::RecruitNotReady),
        ("on an expedition", away, id, CommandError::RecruitNotReady),
        ("unknown", ready.clone(), 99, CommandError::UnknownSurvivor),
    ];
    for (label, mut g, target, error) in rows {
        let before = serde_json::to_value(&g).unwrap();
        assert_eq!(
            g.apply(&Command::SetRecruitTactics {
                id: target,
                tactics: wanted.clone(),
            }),
            Err(error),
            "{label}"
        );
        assert_eq!(serde_json::to_value(&g).unwrap(), before, "{label}");
    }
}

#[test]
fn mid_battle_tactics_change_takes_effect_next_turn_deterministically() {
    // A recall threshold raised between npc turns acts at the next turn,
    // not before it.
    let (mut g, id) = takeover_ready();
    roster_npc_mut(&mut g, id).recall_percent = 0;
    take_over(&mut g, id);
    advance(&mut g, 90);
    g.attempt_mut().unwrap().hp = 1;
    let tick = g.attempt().unwrap().tick;
    assert_ne!(tick % 60, 0, "the change lands between npc turns");
    let mut untouched = g.clone();
    let raised = NpcTactics {
        recall_percent: 50,
        ..tactics_of(roster_npc(&g, id))
    };
    apply_ok(
        &mut g,
        Command::SetRecruitTactics {
            id,
            tactics: raised,
        },
    );
    let to_turn = (60 - tick % 60) as u32;
    advance(&mut g, to_turn - 1);
    assert!(g.attempt().is_some(), "no recall before the next npc turn");
    advance(&mut g, 1);
    assert!(g.attempt().is_none(), "recalled at the next npc turn");
    let report = g.last_result().unwrap();
    assert_eq!(report.outcome, Outcome::Withdrawal);
    assert_eq!(
        report.cause,
        "Survivor recalled at the configured health threshold"
    );
    assert_eq!(report.defender(), DefenderId::Recruit(id));
    assert_eq!(status(&g, id), RecruitStatus::Ready);
    advance(&mut untouched, to_turn);
    assert!(
        untouched.attempt().is_some(),
        "without the change the recruit holds"
    );

    // A policy change mid-battle plays exactly like the same change made to
    // Mara through SetNpcPolicy, and replays and reloads identically.
    let (mut g, id) = takeover_ready();
    let mut twin = mara_as_recruit(&g, id);
    take_over(&mut g, id);
    for game in [&mut g, &mut twin] {
        enemy(game, 0, 30.0, false);
        enemy(game, 2, 45.0, false);
        advance(game, 90);
    }
    let changed = other_tactics(roster_npc(&g, id), roster_npc(&g, id).recall_percent);
    let mut unchanged = g.clone();
    let mut replay = g.clone();
    for game in [&mut g, &mut replay] {
        apply_ok(
            game,
            Command::SetRecruitTactics {
                id,
                tactics: changed.clone(),
            },
        );
    }
    apply_ok(&mut twin, Command::SetNpcPolicy(changed.clone()));
    let mut loaded = reload(&g);
    let mut diverged = false;
    for step in 0..30 {
        for game in [&mut g, &mut twin, &mut replay, &mut loaded, &mut unchanged] {
            advance(game, 60);
        }
        if g.attempt().is_none() {
            assert!(twin.attempt().is_none(), "both runs end together");
            break;
        }
        assert_eq!(without_identity(&g), without_identity(&twin), "step {step}");
        let state = serde_json::to_value(&g).unwrap();
        assert_eq!(serde_json::to_value(&replay).unwrap(), state, "step {step}");
        assert_eq!(serde_json::to_value(&loaded).unwrap(), state, "step {step}");
        diverged |= unchanged
            .attempt()
            .is_none_or(|_| without_identity(&unchanged) != without_identity(&g));
    }
    assert!(diverged, "the new tactics change the battle");
}

// ---------------------------------------------------------------------------
// W2-19: SelectCountry chooses a country without deploying (CD6), and the
// pure outbreak_plan and country_choice_preview reads behind "Plan response"
// and the country dossier.
// ---------------------------------------------------------------------------

/// A country the strategic game can enter but has not liberated.
fn open_front(g: &Game) -> String {
    world()
        .countries
        .iter()
        .find(|c| {
            g.campaign().world.can_enter(&c.id) && !g.campaign().world.cleared.contains(&c.id)
        })
        .expect("an open front")
        .id
        .clone()
}

/// A catalogue country the strategic game cannot enter yet.
fn locked_country(g: &Game) -> String {
    world()
        .countries
        .iter()
        .find(|c| !g.campaign().world.can_enter(&c.id))
        .expect("a locked country")
        .id
        .clone()
}

/// `g` as it would be had only `world.active` moved to `id` (the country is
/// also remembered as unlocked, as entering it always did).
fn only_selected(g: &Game, id: &str) -> serde_json::Value {
    let mut expected = g.clone();
    let world = &mut expected.campaign_mut().world;
    world.active = Some(id.into());
    if !world.unlocked.iter().any(|c| c == id) {
        world.unlocked.push(id.into());
    }
    serde_json::to_value(&expected).unwrap()
}

#[test]
fn select_country_never_creates_an_attempt() {
    let g = strategic_game();
    let front = open_front(&g);
    let liberated = g.campaign().world.cleared[1].clone();
    for (country, outcome) in [
        (front.clone(), CommandOutcome::Applied),
        (liberated, CommandOutcome::CountryLiberated),
    ] {
        let mut selected = g.clone();
        let expected = only_selected(&g, &country);
        assert_eq!(
            selected.apply(&Command::SelectCountry(country.clone())),
            Ok(outcome),
            "{country}"
        );
        assert!(selected.attempt().is_none(), "{country}: no attempt");
        assert_eq!(selected.next_attempt_id(), g.next_attempt_id());
        assert_eq!(
            selected.campaign().world.active_country().unwrap().id,
            country
        );
        assert_eq!(
            serde_json::to_value(&selected).unwrap(),
            expected,
            "{country}"
        );
        // Reselecting is idempotent.
        assert_eq!(
            selected.apply(&Command::SelectCountry(country.clone())),
            Ok(outcome)
        );
        assert_eq!(serde_json::to_value(&selected).unwrap(), expected);
    }
    // The frozen legacy EnterCountry still deploys at once.
    let mut entered = g.clone();
    entered.apply(&Command::EnterCountry(front)).unwrap();
    assert!(entered.attempt().is_some());
    assert_eq!(entered.next_attempt_id(), g.next_attempt_id() + 1);
}

#[test]
fn select_country_typed_rejections_leave_state_unchanged() {
    fn rejects(g: &mut Game, country: &str, error: CommandError) {
        let before = serde_json::to_value(&*g).unwrap();
        assert_eq!(
            g.apply(&Command::SelectCountry(country.into())),
            Err(error.clone()),
            "{country}"
        );
        assert_eq!(serde_json::to_value(&*g).unwrap(), before, "{country}");
        assert!(is_human(&error.to_string()), "{error}");
    }
    // No starting country: SelectCountry never chooses the start.
    let mut fresh = Game::default();
    rejects(&mut fresh, "DEU", CommandError::StartNotChosen);
    fresh
        .apply(&Command::ChooseSurvivor(
            horde_core::survivors::SurvivorId::Rook,
        ))
        .unwrap();
    rejects(&mut fresh, "DEU", CommandError::StartNotChosen);

    let mut g = strategic_game();
    let locked = locked_country(&g);
    rejects(&mut g, &locked, CommandError::CountryLocked(locked.clone()));
    rejects(
        &mut g,
        "NOT_A_COUNTRY",
        CommandError::CountryLocked("NOT_A_COUNTRY".into()),
    );

    // A live defense refuses every selection, checked first.
    let liberated = g.campaign().world.cleared[1].clone();
    let front = open_front(&g);
    g.deploy_country_stage(0, Mode::Personal).unwrap();
    for country in [liberated, locked, front] {
        rejects(&mut g, &country, CommandError::DefenseInProgress);
    }
}

/// A strategic game with an active outbreak in its second liberated country,
/// returning the game and that country.
fn outbreak_game() -> (Game, String) {
    let mut g = strategic_game();
    let country = g.campaign().world.cleared[1].clone();
    g.with_strategy(|s, c| s.trigger_outbreak(c, &country))
        .unwrap();
    (g, country)
}

#[test]
fn outbreak_plan_targets_final_stage_and_lists_ready_guards() {
    let (mut g, country) = outbreak_game();
    let home = g.campaign().world.cleared[0].clone();
    let guard = recruit(&mut g, &home);
    let away = recruit(&mut g, &home);
    let ready = recruit(&mut g, &home);
    apply_ok(
        &mut g,
        Command::AssignGuard {
            id: guard,
            country: country.clone(),
        },
    );
    apply_ok(
        &mut g,
        Command::DeploySurvivor {
            id: away,
            country: home.clone(),
            stage: 0,
        },
    );
    advance(&mut g, 30);
    let before = serde_json::to_value(&g).unwrap();
    let plan = g
        .outbreak_plan(&country)
        .expect("an active outbreak has a plan");
    assert_eq!(
        serde_json::to_value(&g).unwrap(),
        before,
        "outbreak_plan is pure"
    );
    let definition = world().country(&country).unwrap();
    let outbreak = g
        .strategy()
        .outbreaks
        .iter()
        .find(|o| o.country_id == country)
        .unwrap();
    assert_eq!(plan.country_id, country);
    assert_eq!(plan.country_name, definition.name);
    assert_eq!(plan.stage, definition.level_count() - 1, "the final stage");
    assert_eq!(plan.ticks_left, outbreak.deadline_tick - g.strategy().tick);
    assert!(
        plan.ticks_left > 0 && plan.ticks_left < world().strategy_config.outbreak_duration_ticks,
        "the clock ran: {}",
        plan.ticks_left
    );
    assert_eq!(plan.strength, outbreak.strength);
    assert_eq!(plan.guards, vec![guard]);
    assert_eq!(
        plan.guard_capacity,
        world().strategy_config.max_guards_per_country
    );
    // Only the Ready recruit can still be assigned: the guard is on duty and
    // the other is away on an expedition.
    assert_eq!(status(&g, away), RecruitStatus::Deployed);
    assert_eq!(plan.eligible_guards, vec![ready]);
    for id in &plan.eligible_guards {
        let mut assigned = g.clone();
        apply_ok(
            &mut assigned,
            Command::AssignGuard {
                id: *id,
                country: country.clone(),
            },
        );
    }
    // A full garrison lists nobody.
    let mut full = g.clone();
    let extra = recruit(&mut full, &home);
    for id in [ready, extra] {
        apply_ok(
            &mut full,
            Command::AssignGuard {
                id,
                country: country.clone(),
            },
        );
    }
    let fourth = recruit(&mut full, &home);
    let plan = full.outbreak_plan(&country).unwrap();
    assert_eq!(plan.guards, vec![guard, ready, extra]);
    assert!(
        plan.eligible_guards.is_empty(),
        "recruit {fourth} cannot join a full garrison"
    );
    // No active outbreak, or no such country: no plan.
    assert!(g.outbreak_plan(&home).is_none());
    assert!(g.outbreak_plan("NOT_A_COUNTRY").is_none());
}

#[test]
fn begin_defense_from_plan_replays_final_stage_and_contains_the_outbreak() {
    let (mut g, country) = outbreak_game();
    let attempts = g.next_attempt_id();
    // "Plan response": select the country, read the plan, then begin.
    assert_eq!(
        g.apply(&Command::SelectCountry(country.clone())),
        Ok(CommandOutcome::CountryLiberated),
        "an outbreak country is still liberated"
    );
    assert!(g.attempt().is_none());
    assert_eq!(g.next_attempt_id(), attempts);
    let plan = g.outbreak_plan(&country).unwrap();
    apply_ok(&mut g, Command::BeginDefense { stage: plan.stage });
    let a = g.attempt().expect("BeginDefense deploys");
    assert_eq!(a.country_id.as_deref(), Some(country.as_str()));
    assert_eq!(a.stage, plan.stage);
    assert_eq!(a.defender(), DefenderId::Player);
    assert!(!g.global_paused());
    assert_eq!(g.next_attempt_id(), attempts + 1);
    // The player's boss falls on the next tick.
    enemy(&mut g, 0, 20.0, true);
    let a = g.attempt_mut().unwrap();
    a.boss_spawned = true;
    a.enemies.last_mut().unwrap().hp = 0;
    advance(&mut g, 1);
    let report = g.last_result().expect("the defense ended");
    assert_eq!(report.outcome, Outcome::Victory);
    assert_eq!(report.stage, plan.stage);
    let outbreak = g
        .strategy()
        .outbreaks
        .iter()
        .find(|o| o.country_id == country)
        .unwrap();
    assert_eq!(outbreak.outcome, OutbreakOutcome::Contained);
    assert_eq!(g.strategy().active_outbreaks(), 0);
    assert!(g.outbreak_plan(&country).is_none());
    assert!(g.campaign().world.cleared.contains(&country));
}

#[test]
fn country_choice_preview_names_stage_count_and_mara_timing() {
    // A fresh campaign at the start choice: Mara can be met once the first
    // country is liberated.
    let mut fresh = Game::default();
    fresh
        .apply(&Command::ChooseSurvivor(
            horde_core::survivors::SurvivorId::Rook,
        ))
        .unwrap();
    let before = serde_json::to_value(&fresh).unwrap();
    for country in &world().countries {
        let preview = fresh.country_choice_preview(&country.id).unwrap();
        assert_eq!(preview.stage_count, country.level_count(), "{}", country.id);
        assert!(preview.companion_after_liberation, "{}", country.id);
    }
    assert_eq!(
        serde_json::to_value(&fresh).unwrap(),
        before,
        "the preview is pure"
    );
    assert!(fresh.country_choice_preview("NOT_A_COUNTRY").is_none());
    // With Mara already in camp the dossier has no Mara line.
    let g = strategic_game();
    assert!(g.campaign().npc_available());
    let preview = g.country_choice_preview(&open_front(&g)).unwrap();
    assert!(!preview.companion_after_liberation);
}

#[test]
fn country_choice_preview_has_no_mara_line_once_her_liberation_is_met() {
    // One liberation already meets the "Meet Mara" node's minimum_liberated,
    // so no further liberation (of a new country, a liberated one, or one an
    // outbreak reopened after every stage was cleared) is what brings her,
    // even while her node is still unbought.
    let mut g = Game::default();
    g.apply(&Command::ChooseSurvivor(
        horde_core::survivors::SurvivorId::Rook,
    ))
    .unwrap();
    let countries = &world().countries;
    let liberated = countries[0].id.clone();
    let reopened = countries[1].id.clone();
    let world_state = &mut g.campaign_mut().world;
    world_state.cleared.push(liberated.clone());
    world_state
        .clear_high_water
        .insert(reopened.clone(), countries[1].level_count());
    assert_eq!(g.campaign().liberated_count(), 2);
    assert!(!g.campaign().npc_available());
    for country in countries {
        let preview = g.country_choice_preview(&country.id).unwrap();
        assert!(!preview.companion_after_liberation, "{}", country.id);
    }
}

#[test]
fn legacy_campaign_meets_mara_after_its_first_defense_not_a_liberation() {
    // A legacy campaign (legacy_features) gets the Companion feature after
    // its first defense with no node (Campaign::has_feature), so liberating
    // the chosen country is not what brings Mara.
    let mut g = support::legacy_game();
    g.apply(&Command::ChooseSurvivor(
        horde_core::survivors::SurvivorId::Rook,
    ))
    .unwrap();
    assert_eq!(g.campaign().attempts, 0);
    assert!(!g.campaign().npc_available());
    for country in &world().countries {
        let preview = g.country_choice_preview(&country.id).unwrap();
        assert!(!preview.companion_after_liberation, "{}", country.id);
    }
    g.campaign_mut().attempts = 1;
    assert!(g.campaign().npc_available(), "Mara joins after one defense");
}

// ---------------------------------------------------------------------------
// W2-21: the pure defender_power read (design 1.11) on the inputs a deploy
// uses, the derived service_record (1.10) and the Recruit.containments
// writer at the guard-containment site.
// ---------------------------------------------------------------------------

#[test]
fn fresh_starter_power_is_100_at_threat_zero_in_both_models() {
    let g = Game::default();
    assert_eq!(g.next_threat(), 0, "a fresh campaign faces threat 0");
    for (defender, mode) in [
        (DefenderId::Player, Mode::Personal),
        (DefenderId::Mara, Mode::Npc),
    ] {
        assert_eq!(defender.mode(), mode);
        let power = g.defender_power(defender, 0).unwrap();
        assert_eq!(power.defender, defender);
        assert_eq!(power.threat, 0);
        assert_eq!(power.rating, 100, "{defender:?}: {power:?}");
        assert_eq!(power.band, PowerBand::Even);
        assert!(
            power.kills_per_min.is_finite() && power.kills_per_min > 0.0,
            "{power:?}"
        );
        assert!(
            power.survival_s.is_finite() && power.survival_s > 0.0,
            "{power:?}"
        );
    }
    // Every row of the picker carries the power at the next threat, Mara's
    // included while she is still locked.
    for option in g.available_defenders() {
        assert_eq!(
            option.power,
            g.defender_power(option.card.id, g.next_threat()).unwrap()
        );
    }
    // Bands: Weak below 85, Even 85-115, Strong above 115.
    for (rating, band) in [
        (1, PowerBand::Weak),
        (84, PowerBand::Weak),
        (85, PowerBand::Even),
        (115, PowerBand::Even),
        (116, PowerBand::Strong),
        (999, PowerBand::Strong),
    ] {
        assert_eq!(PowerBand::for_rating(rating), band, "{rating}");
        assert!(is_human(band.label()));
    }
}

#[test]
fn defender_power_drops_as_threat_rises() {
    let (g, id) = takeover_ready();
    for defender in [
        DefenderId::Player,
        DefenderId::Mara,
        DefenderId::Recruit(id),
    ] {
        let powers: Vec<_> = (0..=40)
            .map(|threat| g.defender_power(defender, threat).unwrap())
            .collect();
        for pair in powers.windows(2) {
            let (low, high) = (&pair[0], &pair[1]);
            assert!(
                high.kills_per_min <= low.kills_per_min
                    && high.survival_s <= low.survival_s
                    && high.rating <= low.rating,
                "{defender:?}: {low:?} then {high:?}"
            );
            assert_eq!(high.band, PowerBand::for_rating(high.rating));
        }
        assert!(
            powers[40].rating < powers[0].rating,
            "{defender:?}: {} at threat 0, {} at threat 40",
            powers[0].rating,
            powers[40].rating
        );
    }
}

#[test]
fn defender_power_is_pure() {
    let (mut g, id) = takeover_ready();
    let (mut outbreak, country) = outbreak_game();
    let home = outbreak.campaign().world.cleared[0].clone();
    recruit(&mut outbreak, &home);
    advance(&mut g, 90);
    take_over(&mut g, id);
    advance(&mut g, 30);
    let guard = horde_core::progression::catalog().threat_guard_rank;
    for game in [&g, &outbreak] {
        let before = serde_json::to_value(game).unwrap();
        let twin = game.clone();
        let mut ids = vec![DefenderId::Player, DefenderId::Mara];
        ids.extend(
            game.strategy()
                .roster
                .iter()
                .map(|r| DefenderId::Recruit(r.id)),
        );
        for defender in ids {
            for threat in [0, 7, game.next_threat(), guard] {
                let power = game.defender_power(defender, threat);
                assert!(power.is_ok(), "{defender:?} at {threat}: {power:?}");
                assert_eq!(power, game.defender_power(defender, threat));
                assert_eq!(power, twin.defender_power(defender, threat));
            }
            assert!(matches!(
                game.defender_power(defender, guard + 1),
                Err(CommandError::Rejected(_))
            ));
        }
        assert_eq!(
            game.defender_power(DefenderId::Recruit(99), 0),
            Err(CommandError::UnknownSurvivor)
        );
        let _ = game.available_defenders();
        let _ = game.outbreak_plan(&country);
        assert_eq!(serde_json::to_value(game).unwrap(), before, "pure");
    }
}

#[test]
fn outbreak_plan_recommends_the_strongest_selectable_defender() {
    let (mut g, country) = outbreak_game();
    let home = g.campaign().world.cleared[0].clone();
    let hardy = recruit(&mut g, &home);
    let hardier = recruit(&mut g, &home);
    // Hardier than everyone else: the hardier one is guarding, so the
    // other is the strongest defender that can be chosen.
    roster_npc_mut(&mut g, hardy).ht += 10;
    roster_npc_mut(&mut g, hardier).ht += 16;
    apply_ok(
        &mut g,
        Command::AssignGuard {
            id: hardier,
            country: country.clone(),
        },
    );
    let options = g.available_defenders();
    let rating = |id| {
        options
            .iter()
            .find(|o| o.card.id == id)
            .unwrap()
            .power
            .rating
    };
    assert!(rating(DefenderId::Recruit(hardier)) > rating(DefenderId::Recruit(hardy)));
    for other in [DefenderId::Player, DefenderId::Mara] {
        assert!(
            rating(DefenderId::Recruit(hardy)) > rating(other),
            "{other:?}"
        );
    }
    let plan = g.outbreak_plan(&country).unwrap();
    assert_eq!(plan.recommended_defender, DefenderId::Recruit(hardy));
    // On a tie the earlier row wins, so the player is the fallback.
    let fresh = outbreak_game().0;
    let top = fresh
        .available_defenders()
        .into_iter()
        .filter(|o| o.selectable.is_ok())
        .map(|o| o.power.rating)
        .max()
        .unwrap();
    let first = fresh
        .available_defenders()
        .into_iter()
        .find(|o| o.selectable.is_ok() && o.power.rating == top)
        .unwrap();
    assert_eq!(
        fresh.outbreak_plan(&country).unwrap().recommended_defender,
        first.card.id
    );
}

/// A liberated country whose modifiers leave the defender's profile alone,
/// so a deployed attempt's profile is exactly the deploy's shared inputs.
fn profile_neutral_country(g: &Game) -> String {
    g.campaign()
        .world
        .cleared
        .iter()
        .find(|id| {
            let m = world().country(id).unwrap().modifiers;
            m.personal_damage == 1.0 && m.npc_skill == 0
        })
        .expect("a liberated profile-neutral country")
        .clone()
}

#[test]
fn defender_power_inputs_match_the_deployed_attempt() {
    let (mut g, id) = takeover_ready();
    // The recruit's own record differs from Mara's beyond its loadout.
    roster_npc_mut(&mut g, id).ht += 2;
    // Off-default balance and a raised threat, so both scale the inputs.
    let balance = &mut g.campaign_mut().balance_settings;
    balance.player_damage = 1.3;
    balance.defender_health = 0.8;
    balance.enemy_health = 1.2;
    balance.enemy_damage = 1.1;
    g.campaign_mut().victories += 9;
    let country = profile_neutral_country(&g);
    apply_ok(&mut g, Command::SelectCountry(country));
    let threat = g.next_threat();
    assert!(threat > 0);
    for defender in [
        DefenderId::Player,
        DefenderId::Mara,
        DefenderId::Recruit(id),
    ] {
        let mut deployed = g.clone();
        let inputs = deployed.deployment_inputs(defender, threat).unwrap();
        let power = deployed.defender_power(defender, threat).unwrap();
        assert_eq!(inputs.defender, defender);
        assert_eq!(inputs.threat, threat);
        assert_eq!(inputs.power(), power, "{defender:?}");
        deployed
            .deploy_country_stage_as(0, defender)
            .unwrap_or_else(|e| panic!("{defender:?}: {e}"));
        let a = deployed.attempt().unwrap();
        assert_eq!(a.defender(), defender);
        assert_eq!(a.profile, inputs.profile, "{defender:?} profile");
        assert_eq!(a.max_hp, inputs.max_hp, "{defender:?} max_hp");
        assert_eq!(a.progression, inputs.snapshot, "{defender:?} snapshot");
        assert_eq!(a.balance, inputs.balance, "{defender:?} balance");
        // The power read from the deployed attempt's own inputs is the same.
        let from_attempt = DeploymentInputs {
            defender: a.defender(),
            threat,
            profile: a.profile.clone(),
            snapshot: a.progression.clone(),
            max_hp: a.max_hp,
            balance: a.balance.clone(),
        };
        assert_eq!(from_attempt.power(), power, "{defender:?}");
    }
}

/// Recruit `id`'s saved containment count.
fn containments(g: &Game, id: u64) -> u32 {
    g.strategy()
        .roster
        .iter()
        .find(|r| r.id == id)
        .unwrap()
        .containments
}

/// A guarded outbreak with one infected group left, contained under
/// strategy seed `seed`. Returns the game, the guard, an idle recruit, and
/// whether the guard's own hit ended it (it is credited with a victory);
/// otherwise the group fell to its wounds on its own turn.
fn contained_by_guard(seed: u64) -> (Game, u64, u64, bool) {
    let (mut g, country) = outbreak_game();
    let home = g.campaign().world.cleared[0].clone();
    let guard = recruit(&mut g, &home);
    let idle = recruit(&mut g, &home);
    apply_ok(
        &mut g,
        Command::AssignGuard {
            id: guard,
            country: country.clone(),
        },
    );
    let outcome = |g: &Game| {
        g.strategy()
            .outbreaks
            .iter()
            .find(|o| o.country_id == country)
            .unwrap()
            .outcome
    };
    let strategy = g.staging().strategy();
    strategy.rng = horde_core::rules::Rng::new(seed);
    strategy
        .outbreaks
        .iter_mut()
        .find(|o| o.country_id == country)
        .unwrap()
        .strength = 1;
    let victories = roster_npc(&g, guard).victories;
    let mut turns = 0;
    while outcome(&g) == OutbreakOutcome::Active {
        assert_eq!(status(&g, guard), RecruitStatus::Guarding, "seed {seed}");
        advance(&mut g, 60);
        turns += 1;
        assert!(turns < 400, "seed {seed}: the guard never contained it");
    }
    assert_eq!(outcome(&g), OutbreakOutcome::Contained, "seed {seed}");
    let hit = roster_npc(&g, guard).victories > victories;
    (g, guard, idle, hit)
}

#[test]
fn guard_containment_increments_recruit_containments_and_survives_guard_removal() {
    // Both places a guarded outbreak becomes Contained credit the guard:
    // its own decisive hit, and the last group falling to its wounds.
    let mut sites = BTreeSet::new();
    for seed in 1..=24 {
        let (mut g, guard, idle, hit) = contained_by_guard(seed);
        sites.insert(hit);
        assert_eq!(containments(&g, guard), 1, "seed {seed} (hit {hit})");
        assert_eq!(containments(&g, idle), 0, "only the guard is credited");
        // The guard assignment goes; the recruit's containment stays.
        if status(&g, guard) == RecruitStatus::Guarding {
            apply_ok(&mut g, Command::ReleaseGuard(guard));
        }
        assert!(g.strategy().guards.iter().all(|a| a.recruit_id != guard));
        assert_eq!(containments(&g, guard), 1);
        assert_eq!(g.service_record(guard).unwrap().containments, 1);
        let loaded = reload(&g);
        assert_eq!(containments(&loaded, guard), 1);
        assert_eq!(loaded.service_record(guard).unwrap().containments, 1);
    }
    assert_eq!(
        sites,
        BTreeSet::from([false, true]),
        "the seeds reach both containment sites"
    );
}

/// A copy of `template` with the fields a service record reads.
#[allow(clippy::too_many_arguments)]
fn service_run(
    template: &RunRecord,
    recruit_id: Option<u64>,
    worker_id: Option<u64>,
    outcome: Outcome,
    country: &str,
    stage: usize,
    kills: u32,
    coins: u64,
) -> RunRecord {
    let mut record = template.clone();
    record.recruit_id = recruit_id;
    record.worker_id = worker_id;
    record.outcome = outcome;
    record.country_id = Some(country.into());
    record.country_name = world().country(country).unwrap().name.clone();
    record.stage = stage;
    record.kills = kills;
    record.coins = coins;
    record
}

#[test]
fn service_record_is_derived_from_run_history_and_containments() {
    let (mut g, id) = takeover_ready();
    let home = g.campaign().world.cleared[0].clone();
    let other = recruit(&mut g, &home);
    take_over(&mut g, id);
    win_main(&mut g);
    let record = g.service_record(id).unwrap();
    let won = g.campaign().run_history.last().unwrap().clone();
    assert_eq!((won.recruit_id, won.worker_id), (Some(id), None));
    assert_eq!(record.recruit_id, id);
    assert_eq!(
        (record.deployments, record.takeovers, record.expeditions),
        (1, 1, 0)
    );
    assert_eq!(
        (record.victories, record.defeats, record.withdrawals),
        (1, 0, 0)
    );
    assert_eq!(record.kills, u64::from(won.kills));
    assert_eq!(record.coins, won.coins);
    assert_eq!(record.last_outcome, Some(Outcome::Victory));
    assert_eq!(record.npc_attempts, roster_npc(&g, id).attempts);
    assert_eq!(record.npc_victories, roster_npc(&g, id).victories);
    assert_eq!(record.containments, 0);
    assert_eq!(record.guard, None);

    // A history of expeditions and takeovers, beside other defenders' runs.
    let cleared = g.campaign().world.cleared.clone();
    let (a, b, c) = (&cleared[0], &cleared[1], &cleared[2]);
    let final_b = world().country(b).unwrap().level_count() - 1;
    assert!(final_b > 0);
    let history = vec![
        service_run(&won, Some(id), None, Outcome::Victory, a, 0, 10, 5),
        service_run(&won, Some(id), Some(7), Outcome::Victory, a, 0, 4, 3),
        service_run(&won, None, None, Outcome::Victory, b, final_b, 50, 50),
        service_run(&won, Some(id), Some(7), Outcome::Victory, b, final_b, 6, 2),
        service_run(&won, Some(id), Some(8), Outcome::Defeat, b, 0, 1, 0),
        service_run(&won, Some(other), Some(9), Outcome::Defeat, c, 0, 9, 9),
        service_run(&won, Some(id), None, Outcome::Withdrawal, c, 0, 0, 0),
        service_run(&won, Some(other), None, Outcome::Victory, c, 0, 9, 9),
    ];
    g.staging().campaign().run_history = std::sync::Arc::new(history);
    g.staging()
        .strategy()
        .roster
        .iter_mut()
        .find(|r| r.id == id)
        .unwrap()
        .containments = 2;
    let guarded = cleared[3].clone();
    apply_ok(
        &mut g,
        Command::AssignGuard {
            id,
            country: guarded.clone(),
        },
    );
    advance(&mut g, 120);
    let duty_ticks = g
        .strategy()
        .guards
        .iter()
        .find(|a| a.recruit_id == id)
        .unwrap()
        .duty_ticks;
    let before = serde_json::to_value(&g).unwrap();
    let record = g.service_record(id).unwrap();
    assert_eq!(serde_json::to_value(&g).unwrap(), before, "pure");
    assert_eq!(
        record,
        ServiceRecord {
            recruit_id: id,
            deployments: 5,
            takeovers: 2,
            expeditions: 2,
            victories: 3,
            defeats: 1,
            withdrawals: 1,
            kills: 21,
            coins: 10,
            stages_cleared: 2,
            liberations: 1,
            countries: vec![a.clone(), b.clone(), c.clone()],
            last_outcome: Some(Outcome::Withdrawal),
            npc_attempts: roster_npc(&g, id).attempts,
            npc_victories: roster_npc(&g, id).victories,
            guard: Some(GuardDuty {
                country_id: guarded,
                duty_ticks,
            }),
            containments: 2,
        }
    );
    // The other recruit sees only its own runs; an unknown id is typed.
    let theirs = g.service_record(other).unwrap();
    assert_eq!((theirs.deployments, theirs.takeovers), (2, 1));
    assert_eq!(theirs.countries, vec![c.clone()]);
    assert_eq!(theirs.last_outcome, Some(Outcome::Victory));
    assert_eq!(g.service_record(99), Err(CommandError::UnknownSurvivor));
}
