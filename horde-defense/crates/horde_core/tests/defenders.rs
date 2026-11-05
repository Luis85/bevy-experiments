//! Defender identity (W1-10): the public `DefenderId`, the saved
//! `DefenderSeat`, `resolve`, the recruit identity table and the plain labels
//! that replace Rust `Debug` names in player text; and (W1-11) the appended
//! `RecruitStatus::Defending` with `reserve_defender` / `release_defender`;
//! and (W1-12) the typed `DefenseInProgress` / `DefenderUnavailable` errors
//! with the Defending pre-checks in `Game::apply`; and (W1-13) the saved
//! `"defender"` seat on `Attempt` and `Report` with their `defender()`
//! accessors; and (W1-15) the recruit seat and appearance stamped on worker
//! attempts at both deploy sites, with the worker-seat save validation.
mod support;
use horde_core::{
    campaign::{Campaign, NpcTraining},
    command::{Command, CommandError},
    content::{Mode, Policy},
    defenders::{
        DefenderBlocker, DefenderId, DefenderSeat, OriginTrait, RECRUIT_IDENTITIES,
        recruit_identity, resolve,
    },
    sim::{Game, ParallelBattle, SAVE_VERSION, SaveError},
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
    // Control: every command is accepted while the recruit is Ready, so the
    // rejections below come from the status gate and nothing else.
    for command in &commands {
        let mut game = ready.clone();
        game.apply(command)
            .unwrap_or_else(|e| panic!("{command:?} on a Ready recruit: {e}"));
    }
    {
        let mut game = ready.clone();
        let (s, c) = game.staging().strategy_and_campaign();
        s.respec_recruit(c, id).unwrap();
        s.reserve_recruit(id).unwrap();
    }

    let mut defending = ready.clone();
    defending.staging().strategy().reserve_defender(id).unwrap();
    let before = serde_json::to_value(&defending).unwrap();
    for command in &commands {
        assert!(
            defending.apply(command).is_err(),
            "{command:?} accepted for a Defending recruit"
        );
        assert_eq!(
            serde_json::to_value(&defending).unwrap(),
            before,
            "{command:?}"
        );
    }
    let (s, c) = defending.staging().strategy_and_campaign();
    assert!(s.respec_recruit(c, id).is_err());
    assert!(s.reserve_recruit(id).is_err());
    assert_eq!(serde_json::to_value(&defending).unwrap(), before);
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
