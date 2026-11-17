//! Staged campaign and battle fixtures the scenario matrix builds on.
pub(super) mod production;

use crate::settings::Settings;
use crate::ui::{FrontendParts, Screen, Tab, battle};
use horde_core::{
    campaign::{BodyAttribute, NpcTraining, Skill},
    content::{Mode, Slot, StructureKind, content},
    ordnance::{OrdnanceKind, ThrowRequest},
    sim::{Game, Input, Outcome},
    world::world,
};

/// Whether an isolated review scene keeps its audio: only a perf run that
/// asks for it (`LASTLIGHT_PERF_AUDIO` with `LASTLIGHT_PERF_REPORT`), so a
/// live battle's `LASTLIGHT_REVIEW_CUE_LOG` records real starts for
/// `replay_cue_log` (A6-18). Every other review scene is muted.
pub(super) fn keeps_audio(launch: &crate::launch::LaunchConfig) -> bool {
    launch.perf().is_some_and(|perf| perf.audio)
}

/// Strict opt-in copied checkpoint; production and source files are never targets.
pub(super) fn prepare_copied_npc(f: &mut FrontendParts) {
    let launch = f.launch.clone();
    assert!(
        !launch.practice,
        "npc-stutter requires isolated non-practice review"
    );
    let input = launch
        .review()
        .and_then(|r| r.input_save.clone())
        .expect("npc-stutter requires LASTLIGHT_REVIEW_INPUT_SAVE copied generation");
    let canonical = input
        .canonicalize()
        .expect("copied NPC checkpoint must exist");
    let copied_settings = input
        .parent()
        .expect("checkpoint parent")
        .join("settings.json");
    for source in [&canonical, &copied_settings] {
        if let Ok(source) = source.canonicalize() {
            for target in [
                &f.paths.campaign(),
                &f.paths.campaign().with_extension("bak"),
                &f.paths.campaign().with_extension("tmp"),
                &f.paths.settings(),
            ] {
                if let Ok(target) = target.canonicalize() {
                    assert_ne!(
                        source, target,
                        "Copied inputs must differ from every review output generation"
                    );
                }
            }
        }
    }
    let bytes = std::fs::read(&canonical).expect("read copied NPC checkpoint");
    f.game = Game::decode_save_generation(&bytes)
        .expect("Copied NPC generation invalid; no fallback is permitted");
    assert!(
        f.game.attempt().is_some_and(|a| a.mode == Mode::Npc),
        "Copied checkpoint must contain the active NPC battle"
    );
    f.settings = Settings::load(&copied_settings);
    if !keeps_audio(&launch) {
        f.settings.master = 0.0;
    }
    if let Some(scale) = launch.review().and_then(|r| r.scale) {
        f.settings.scale = scale;
    }
    f.game.set_paused(false);
    f.campaign.mode = Mode::Npc;
    f.screen = Screen::Battle;
    f.battle.speed = launch.review().and_then(|r| r.perf.npc_speed).unwrap_or(1);
    assert!(
        matches!(f.battle.speed, 1 | 2 | 4),
        "NPC diagnostic speed must be1,2or4"
    );
    let a = f.game.attempt().unwrap();
    f.battle.aim = [a.width_yards * 0.6, a.defender_position()[1]];
    f.status.message = "Copied checkpoint/resumed NPC battle · isolated performance review".into();
}

/// Staged near-defeated boss; real core settlement supplies the report/rewards.
pub(super) fn prepare_journey_victory(f: &mut FrontendParts, liberated: bool) {
    let count = f
        .game
        .campaign()
        .world
        .active_country()
        .expect("review country")
        .level_count();
    let stage = if liberated { count - 1 } else { 0 };
    for n in 0..stage {
        f.game
            .staging()
            .campaign()
            .record_country_stage_clear(n)
            .expect("explicit earlier-country review milestones");
    }
    f.game
        .deploy_country_stage(stage, Mode::Personal)
        .expect("journey outcome fixture");
    let mut a = f.game.staging().attempt_slot().take().unwrap();
    a.boss_spawned = true;
    let lane = a.lane_count / 2;
    f.game
        .spawn_enemy(&mut a, horde_core::content::EnemyKind::Brute, lane, true);
    let target = a.enemies.last_mut().unwrap();
    target.x = a.width_yards * 0.55;
    target.hp = 1;
    target.armor = 0;
    let aim = [target.x, target.y()];
    *f.game.staging().attempt_slot() = Some(a);
    f.game.set_paused(false);
    f.game.tick(Input {
        fire: true,
        aim,
        ..Default::default()
    });
    assert!(f.game.attempt().is_none());
    assert_eq!(f.game.last_result().unwrap().outcome, Outcome::Victory);
    f.game.set_paused(true);
    f.campaign.mode = Mode::Personal;
    f.act(|c| c.focus_next_upgrade());
    f.screen = Screen::Results;
}

/// These pre-existing advanced view fixtures explicitly retain legacy ledgers
/// (character points, body ranks). Idempotent; it must run before any staged
/// clear whose levels should grant those points.
pub(super) fn enable_legacy_ledgers(game: &mut Game) {
    game.staging().campaign().legacy_features = true;
}

pub(super) fn prepare_progression(game: &mut Game) {
    // Explicit screenshot entitlements; use model methods for every owned item,
    // skill and point investment. This does not represent earned campaign income.
    enable_legacy_ledgers(game);
    let campaign = game.staging().campaign();
    // 600 more than before the W4-p repricing, which is what the six nodes
    // bought below cost more, so the fixture ends with the same wallet.
    campaign.coins = 2400;
    for stage in 0..5 {
        campaign.first_clear(stage, Mode::Personal);
    }
    campaign.finish_attempt();
    campaign.reward(0, 300, Mode::Personal);
    for attribute in [
        BodyAttribute::Vitality,
        BodyAttribute::Strength,
        BodyAttribute::Endurance,
    ] {
        campaign
            .allocate_body(attribute)
            .expect("fixture body investment");
    }
    for skill in [Skill::QuickReload, Skill::SecondWind, Skill::Engineering] {
        campaign.purchase_skill(skill).expect("fixture skill");
    }
    campaign
        .purchase_node("rifle_training")
        .expect("fixture firearm proficiency");
    campaign
        .purchase_node("grenade_training")
        .expect("fixture grenade handling");
    campaign
        .purchase_node("molotov_training")
        .expect("fixture incendiary handling");
    for kind in StructureKind::ALL {
        campaign
            .purchase_blueprint(kind)
            .expect("fixture blueprint");
    }
    for slot in Slot::ALL {
        let definition = content()
            .items
            .iter()
            .enumerate()
            .find(|(_, item)| item.slot == slot && item.tier == 1)
            .map(|(n, _)| n)
            .expect("starter-tier shop option");
        let id = campaign
            .buy_item(definition)
            .expect("fixture shop equipment");
        campaign
            .equip(Mode::Personal, id)
            .expect("fixture personal equipment");
        campaign
            .equip(Mode::Npc, id)
            .expect("fixture alternate equipment");
    }
    campaign.record_npc_progress(20, 7200);
    for _ in 0..10 {
        campaign
            .buy_training_point()
            .expect("fixture training service");
    }
    campaign.train_npc(NpcTraining::Guns).expect("fixture guns");
    campaign
        .train_npc(NpcTraining::Melee)
        .expect("fixture melee");
    let definition = content()
        .items
        .iter()
        .position(|item| item.slot == Slot::Ranged && item.tier == 1)
        .unwrap();
    let id = campaign
        .add_item(definition)
        .expect("fixture recovered spare");
    campaign.set_favorite(id, true).expect("fixture favorite");
    campaign
        .save_preset("Prepared patrol".into(), Mode::Personal)
        .expect("fixture preset");
}
pub(super) fn prepare_history(f: &mut FrontendParts) {
    // Real model deployments/inputs/settlements, never authored plot points.
    for mode in [Mode::Personal, Mode::Npc, Mode::Personal] {
        f.game
            .deploy_country_stage(0, mode)
            .expect("history review deployment");
        f.game.set_paused(false);
        for _ in 0..60 * 18 {
            if f.game.attempt().is_none() {
                break;
            }
            let input = if mode == Mode::Personal {
                horde_core::sim::smoke_input(&f.game)
            } else {
                Input::default()
            };
            f.game.tick(input);
        }
        if f.game.attempt().is_some() {
            f.game.withdraw();
        }
    }
    f.nav.history_return = Screen::Camp;
    f.nav.history_world_paused = true;
    f.screen = Screen::History;
}
pub(super) fn prepare_battle(f: &mut FrontendParts, screen: &str) {
    let mode = if screen == "npc" {
        Mode::Npc
    } else {
        Mode::Personal
    };
    f.game
        .deploy_country_stage(0, mode)
        .expect("review deployment");
    let lanes = f.game.attempt().unwrap().lane_count;
    let center = lanes / 2;
    // A legal mixed starting plan in the actual frozen rows.
    for (kind, lane, column) in [
        (StructureKind::Wall, center.saturating_sub(1), 0),
        (StructureKind::Wall, (center + 1).min(lanes - 1), 0),
        (StructureKind::Turret, center, 1),
        (StructureKind::Trap, center, 2),
    ] {
        f.game
            .place(kind, lane, column)
            .expect("fixture affordable placement");
    }
    f.game.set_paused(false);
    let target = match screen {
        "progressed-battle" => 60 * 15,
        "results" => 60 * 480,
        "boss" => ((f.game.attempt().unwrap().boss_time() + 12.0) * 60.0).ceil() as u32,
        _ => 60 * 38,
    };
    for _ in 0..target {
        if f.game.attempt().is_none() {
            break;
        }
        let input = if matches!(screen, "boss" | "results") {
            horde_core::sim::smoke_input(&f.game)
        } else {
            Input::default()
        };
        f.game.tick(input);
        if !matches!(screen, "boss" | "results")
            && f.game
                .attempt()
                .is_some_and(|a| a.elapsed() > 6.0 && a.hp < a.max_hp * 2 / 3)
        {
            break;
        }
        if screen == "boss" && f.game.attempt().is_some_and(|a| a.boss_spawned) {
            for _ in 0..60 * 8 {
                if f.game.attempt().is_some() {
                    f.game.tick(Input::default());
                }
            }
            break;
        }
    }
    if screen == "ordnance" && f.game.attempt().is_some() {
        let a = f.game.attempt().unwrap();
        let molotov_cell = [(a.width_yards * 0.6).floor() as u8, a.lane_count / 2];
        let grenade_cell = [
            (a.width_yards * 0.75).floor() as u8,
            (a.lane_count / 2 + 1).min(a.lane_count - 1),
        ];
        f.game.tick(Input {
            throw: Some(ThrowRequest {
                kind: OrdnanceKind::Molotov,
                cell: molotov_cell,
            }),
            ..Input::default()
        });
        for _ in 0..=horde_core::ordnance::catalog().molotov.flight_ticks {
            f.game.tick(Input::default());
        }
        f.game.tick(Input {
            throw: Some(ThrowRequest {
                kind: OrdnanceKind::Grenade,
                cell: grenade_cell,
            }),
            ..Input::default()
        });
        f.battle.attack_selection = battle::AttackSelection::Molotov;
        f.battle.aim = [
            f.game.attempt().unwrap().width_yards * 0.6,
            f.game.attempt().unwrap().defender_position()[1],
        ];
    }
    if screen == "results" {
        if f.game.attempt().is_some() {
            f.game.withdraw();
        }
        f.screen = Screen::Results;
    } else {
        assert!(
            f.game.attempt().is_some(),
            "review battle should still be active"
        );
        f.screen = Screen::Battle;
    }
    f.campaign.mode = mode;
    f.battle.set_construction(screen == "construction");
    if let Some(enemy) = f.game.attempt().and_then(|a| {
        a.enemies
            .iter()
            .find(|e| e.boss)
            .or_else(|| a.enemies.first())
    }) {
        f.battle.aim = [enemy.x, enemy.y()];
    }
}

pub(super) fn prepare_gesture_battle(f: &mut FrontendParts, melee: bool) {
    prepare_battle(f, "battle");
    f.battle.fire_latched = false;
    f.battle.speed = 1;
    f.battle.pending = Input::default();
    // Remove automated damage using the actual legal salvage operation. The
    // single contact placement below is initial fixture staging, never an
    // invented action event, damage result, cooldown or later-frame teleport.
    let structures: Vec<_> = f
        .game
        .attempt()
        .unwrap()
        .structures
        .iter()
        .map(|structure| structure.id)
        .collect();
    for id in structures {
        f.game
            .salvage(id)
            .expect("isolated gesture fixture salvage");
    }
    let attempt = f.game.staging().attempt().unwrap();
    if melee {
        let lane = attempt.lane_count / 2;
        let target = attempt
            .enemies
            .iter_mut()
            .find(|enemy| {
                !enemy.boss
                    && enemy.hp > 0
                    && enemy.kind != horde_core::content::EnemyKind::Demolisher
            })
            .expect("existing live non-explosive contact target");
        target.lane = lane;
        target.x = horde_core::sim::DEFENSE_LINE;
        target.pass_offset = 0.;
    }
    f.battle.aim = [
        attempt.width_yards * 0.6,
        f32::from(attempt.lane_count / 2) + 0.5,
    ];
    f.campaign.mode = Mode::Personal;
    f.screen = Screen::Battle;
}
pub(super) fn prepare_operations(f: &mut FrontendParts) {
    f.nav.tab = Tab::Operations;
    for id in ["DEU", "FRA", "CHE", "AUT"] {
        f.game.staging().campaign().world.cleared.push(id.into());
        f.game
            .staging()
            .campaign()
            .world
            .country_levels
            .insert(id.into(), world().country(id).unwrap().level_count());
    }
    let (strategy, campaign) = f.game.staging().strategy_and_campaign();
    let first = strategy.recruit(campaign, "DEU").expect("review guard");
    let (strategy, campaign) = f.game.staging().strategy_and_campaign();
    let second = strategy
        .recruit(campaign, "FRA")
        .expect("review expedition");
    let (strategy, campaign) = f.game.staging().strategy_and_campaign();
    strategy
        .assign_guard(campaign, first, "DEU")
        .expect("review garrison");
    f.game
        .deploy_survivor(second, "POL", 0)
        .expect("review parallel battle");
    f.game.set_paused(false);
    for _ in 0..60 * 20 {
        f.game.tick(Input::default());
    }
    let (strategy, campaign) = f.game.staging().strategy_and_campaign();
    strategy
        .trigger_outbreak(campaign, "DEU")
        .expect("review plague alert");
}
