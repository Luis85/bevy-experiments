use super::{apply::*, command::*, persistence, runtime::*, *};
use crate::{
    audio::Cue as SoundCue,
    profile::{ProfilePaths, load_profile, profile_directory},
    settings::Settings,
};
use bevy::prelude::*;
use horde_core::{
    command::{Command, CommandOutcome},
    content::{Mode, StructureKind},
    ordnance::OrdnanceKind,
    sim::{Game, Input, SettlementIssue},
    skill_catalog::skills,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn country_command_requires_confirmed_survivor_and_retains_cosmetic_identity() {
    use horde_core::survivors::{Appearance, SurvivorId};
    let mut game = Game::default();
    let before = serde_json::to_value(&game).unwrap();
    assert!(game.apply(&Command::ChooseStart("DEU".into())).is_err());
    assert_eq!(serde_json::to_value(&game).unwrap(), before);
    game.apply(&Command::ChooseSurvivor(SurvivorId::June))
        .unwrap();
    game.apply(&Command::ChooseStart("DEU".into())).unwrap();
    assert_eq!(
        game.attempt().unwrap().profile.appearance,
        Appearance::Female
    );
}
#[test]
fn authoritative_events_select_weapon_and_ordnance_sounds_without_mutating_the_game() {
    use horde_core::sim::{Actor, SimEvent};
    let mut host = Host::new();
    host.deploy();
    let before = serde_json::to_value(&host.game).unwrap();
    let a = host.game.attempt().unwrap();
    let shot = SimEvent::Shot {
        shooter: Actor::Defender {
            mode: a.mode,
            weapon: a.profile.weapon.class,
        },
        target_id: None,
        lane: None,
        from: horde_core::sim::DEFENDER,
        to: [15.0, 5.5],
    };
    event_sound(&mut host.cues, &shot);
    assert_eq!(host.cues, vec![SoundCue::Pistol]);
    host.cues.clear();
    let throw = SimEvent::Throw {
        projectile_id: 7,
        ordnance: OrdnanceKind::Grenade,
        lane: Some(5),
        from: horde_core::sim::DEFENDER,
        to: [15.0, 5.5],
        flight_ticks: 30,
    };
    event_sound(&mut host.cues, &throw);
    assert_eq!(host.cues, vec![SoundCue::Throw]);
    host.cues.clear();
    for (ordnance, cue) in [
        (OrdnanceKind::Grenade, SoundCue::Explosion),
        (OrdnanceKind::Molotov, SoundCue::Molotov),
    ] {
        event_sound(
            &mut host.cues,
            &SimEvent::Explosion {
                projectile_id: 7,
                ordnance,
                lane: Some(5),
                at: [15.0, 5.5],
                radius: 1.5,
            },
        );
        assert_eq!(host.cues, vec![cue]);
        host.cues.clear();
    }
    assert_eq!(serde_json::to_value(&host.game).unwrap(), before);
}
#[test]
fn a_sound_emitted_on_the_finishing_tick_is_played() {
    let mut host = Host::new();
    host.deploy();
    let a = host.game.attempt_mut().unwrap();
    // The reload completes on the same tick the overrun defender retreats.
    a.hp = 0;
    a.reload_left = 1;
    host.runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    step(&mut host.ctx());
    assert!(host.game.attempt().is_none());
    assert!(host.cues.contains(&SoundCue::Reload));
    assert!(
        host.notices
            .iter()
            .any(|n| matches!(n, RuntimeNotice::SimulationFinished))
    );
}
#[test]
fn melee_readiness_event_plays_the_ready_cue() {
    let mut host = Host::new();
    host.deploy();
    host.game.attempt_mut().unwrap().melee_cooldown = 1;
    host.runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    step(&mut host.ctx());
    assert!(host.cues.contains(&SoundCue::Ready));
}
#[test]
fn kill_and_attempt_finished_events_map_to_their_cues() {
    use horde_core::sim::{Outcome, SimEvent};
    let mut cues = Vec::new();
    event_sound(
        &mut cues,
        &SimEvent::Kill {
            enemy_id: 3,
            lane: Some(1),
            at: [4.0, 5.0],
            coins: 18,
        },
    );
    assert_eq!(cues, vec![SoundCue::Kill]);
    for (outcome, cue) in [
        (Outcome::Victory, SoundCue::Victory),
        (Outcome::Defeat, SoundCue::Defeat),
        (Outcome::Withdrawal, SoundCue::Retreat),
    ] {
        cues.clear();
        event_sound(
            &mut cues,
            &SimEvent::AttemptFinished {
                attempt_id: 1,
                outcome,
                mode: Mode::Personal,
                cause: String::new(),
            },
        );
        assert_eq!(cues, vec![cue]);
    }
}
#[test]
fn boss_kill_on_the_finishing_tick_plays_kill_and_victory() {
    let mut host = Host::new();
    host.deploy();
    let mut attempt = host.game.attempt_slot_mut().take().unwrap();
    attempt.boss_spawned = true;
    host.game
        .spawn_enemy(&mut attempt, horde_core::content::EnemyKind::Brute, 5, true);
    attempt.enemies.last_mut().unwrap().hp = 0;
    *host.game.attempt_slot_mut() = Some(attempt);
    host.runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    step(&mut host.ctx());
    assert!(host.game.attempt().is_none());
    assert!(host.cues.contains(&SoundCue::Kill), "{:?}", host.cues);
    assert!(host.cues.contains(&SoundCue::Victory), "{:?}", host.cues);
    assert!(!host.cues.contains(&SoundCue::Defeat));
}
#[test]
fn overrun_on_the_finishing_tick_plays_the_defeat_stinger() {
    let mut host = Host::new();
    host.deploy();
    host.game.attempt_mut().unwrap().hp = 0;
    host.runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    step(&mut host.ctx());
    assert!(host.game.attempt().is_none());
    assert!(host.cues.contains(&SoundCue::Defeat), "{:?}", host.cues);
    assert!(!host.cues.contains(&SoundCue::Victory));
}
#[test]
fn withdrawal_plays_retreat_once_without_waiting_for_a_tick() {
    let mut host = Host::new();
    host.deploy();
    host.runtime.begin_frame(1);
    assert!(host.runtime.enqueue(Command::Withdraw, "Recalled"));
    flush_for_test(&mut host.ctx());
    assert!(host.game.attempt().is_none());
    assert_eq!(host.cues, vec![SoundCue::Retreat]);
    // The terminal event was drained with the command: later camp ticks
    // cannot replay it.
    host.cues.clear();
    host.game.set_paused(false);
    host.runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    for _ in 0..3 {
        step(&mut host.ctx());
    }
    assert!(!host.cues.contains(&SoundCue::Retreat), "{:?}", host.cues);
}
#[test]
fn successful_purchases_confirm_and_rejected_ones_stay_silent() {
    let mut host = Host::new();
    host.game.campaign_mut().coins = 100;
    host.runtime.begin_frame(1);
    host.runtime
        .enqueue(Command::BuySkillNode("steady_hands".into()), "Learned");
    flush_for_test(&mut host.ctx());
    assert_eq!(host.cues, vec![SoundCue::Confirm]);
    host.cues.clear();
    host.runtime.begin_frame(2);
    host.runtime
        .enqueue(Command::BuySkillNode("steady_hands".into()), "Learned");
    flush_for_test(&mut host.ctx());
    assert!(host.cues.is_empty(), "{:?}", host.cues);
}
#[test]
fn authored_node_purchase_command_charges_once_and_preserves_id_ownership() {
    let mut game = Game::default();
    game.campaign_mut().coins = 100;
    game.apply(&Command::BuySkillNode("steady_hands".into()))
        .unwrap();
    assert!(game.campaign().has_node("steady_hands"));
    assert_eq!(
        game.campaign().coins,
        100 - skills().node("steady_hands").unwrap().price
    );
    let before = game.clone();
    assert!(
        game.apply(&Command::BuySkillNode("steady_hands".into()))
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(game).unwrap(),
        serde_json::to_value(before).unwrap()
    );
}
#[test]
fn runtime_consumes_throw_edge_once_even_at_observation_speed_four() {
    use horde_core::ordnance::{OrdnanceKind, ThrowRequest};
    let request = ThrowRequest {
        kind: OrdnanceKind::Grenade,
        cell: [20, 5],
    };
    let mut runtime = Runtime::default();
    runtime.publish(ControlFrame {
        enabled: true,
        speed: 4,
        pressed: Input {
            throw: Some(request),
            ..Input::default()
        },
        held: Input {
            throw: Some(request),
            ..Input::default()
        },
        ..ControlFrame::default()
    });
    assert_eq!(runtime.input().throw, Some(request));
    assert!(runtime.input().throw.is_none());
    assert!(runtime.input().throw.is_none());
}
#[test]
fn npc_defender_shot_uses_weapon_class_cue() {
    use horde_core::sim::{Actor, SimEvent};
    use horde_core::weapons::WeaponClass;
    // NpcShot is retired: a defender's shot sounds like its weapon in either
    // mode, so an NPC with a rifle is heard as a rifle.
    for (weapon, expected) in [
        (WeaponClass::Pistol, SoundCue::Pistol),
        (WeaponClass::Smg, SoundCue::Smg),
        (WeaponClass::Shotgun, SoundCue::Shotgun),
        (WeaponClass::Rifle, SoundCue::Rifle),
        (WeaponClass::Marksman, SoundCue::Marksman),
        (WeaponClass::MachineGun, SoundCue::MachineGun),
    ] {
        for mode in [Mode::Npc, Mode::Personal] {
            let mut cues = Vec::new();
            event_sound(
                &mut cues,
                &SimEvent::Shot {
                    shooter: Actor::Defender { mode, weapon },
                    target_id: None,
                    lane: Some(0),
                    from: horde_core::sim::DEFENDER,
                    to: [15.0, 5.5],
                },
            );
            assert_eq!(cues, vec![expected], "{mode:?} {weapon:?}");
            assert!(cues.iter().all(|cue| !cue.retired()), "{mode:?} {weapon:?}");
        }
    }
}
#[test]
fn trap_shots_never_fabricate_a_turret_and_missing_sources_stay_silent() {
    use horde_core::sim::{Actor, SimEvent, Strike};
    let mut cues = Vec::new();
    let shot = |shooter| SimEvent::Shot {
        shooter,
        target_id: None,
        lane: Some(0),
        from: [4.2, 0.5],
        to: [5.0, 0.5],
    };
    let trap = Actor::Structure {
        id: 9000,
        kind: StructureKind::Trap,
    };
    event_sound(&mut cues, &shot(trap));
    assert_eq!(cues, vec![SoundCue::Trap]);
    cues.clear();
    event_sound(
        &mut cues,
        &shot(Actor::Structure {
            id: 9000,
            kind: StructureKind::Turret,
        }),
    );
    assert_eq!(cues, vec![SoundCue::TurretShot]);
    cues.clear();
    event_sound(&mut cues, &shot(Actor::Unknown { id: 9000 }));
    assert!(cues.is_empty());
    event_sound(
        &mut cues,
        &SimEvent::Miss(Strike {
            attacker: trap,
            target_id: Some(1),
            lane: Some(0),
            from: [4.2, 0.5],
            to: [5.0, 0.5],
            value: 0,
        }),
    );
    assert_eq!(cues, vec![SoundCue::Trap]);
}

/// Owned runner state; `ctx()` lends it to the pure runner functions.
struct Host {
    game: Game,
    runtime: Runtime,
    settings: Settings,
    directory: PathBuf,
    paths: ProfilePaths,
    notices: Vec<RuntimeNotice>,
    cues: Vec<SoundCue>,
}
impl Host {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "lastlight-runtime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut game = Game::default();
        game.campaign_mut().legacy_features = true;
        // Runtime fixtures below explicitly preserve the legacy11×30 geometry.
        game.campaign_mut()
            .set_balance_settings(horde_core::balance::BalanceSettings::legacy())
            .unwrap();
        game.campaign_mut()
            .choose_survivor(horde_core::survivors::SurvivorId::Rook)
            .unwrap();
        Self {
            game,
            runtime: Runtime::default(),
            settings: Settings::default(),
            paths: ProfilePaths::in_directory(directory.clone()),
            directory,
            notices: vec![],
            cues: vec![],
        }
    }
    fn ctx(&mut self) -> RuntimeCtx<'_> {
        RuntimeCtx {
            game: &mut self.game,
            runtime: &mut self.runtime,
            settings: &self.settings,
            paths: &self.paths,
            notices: &mut self.notices,
            cues: &mut self.cues,
            game_changed: false,
            perf: None,
        }
    }
    fn deploy(&mut self) {
        self.game
            .campaign_mut()
            .choose_start_country("DEU")
            .unwrap();
        self.game.deploy_country_stage(0, Mode::Personal).unwrap();
        self.game.set_paused(false);
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.runtime.finish_storage_for_test();
        let archives = self.directory.join("journey-archives");
        if let (Ok(base), Ok(target)) = (self.directory.canonicalize(), archives.canonicalize())
            && target.starts_with(&base)
            && target != base
        {
            let _ = std::fs::remove_dir_all(target);
        }

        for notice in &self.notices {
            if let RuntimeNotice::RecoveryCopies(paths) = notice {
                for path in paths {
                    if path.parent() == Some(self.directory.as_path()) {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
        }
        for file in [
            "campaign.json",
            "campaign.bak",
            "campaign.tmp",
            "settings.json",
            "settings.tmp",
            "not_directory",
        ] {
            let _ = std::fs::remove_file(self.directory.join(file));
        }
        let _ = std::fs::remove_dir(&self.directory);
    }
}
fn state(game: &Game) -> serde_json::Value {
    serde_json::to_value(game).unwrap()
}

#[test]
fn repeated_ui_layout_pass_cannot_repeat_a_purchase() {
    let mut host = Host::new();
    host.game.campaign_mut().coins = 100;
    host.runtime.begin_frame(20);
    assert!(
        host.runtime
            .enqueue(Command::BuySkillNode("quick_reload".into()), "Learned")
    );
    host.runtime.begin_frame(20);
    assert!(
        !host
            .runtime
            .enqueue(Command::BuySkillNode("quick_reload".into()), "Learned")
    );
    assert!(!flush_for_test(&mut host.ctx()));
    assert_eq!(
        host.game.campaign().coins,
        100 - skills().node("quick_reload").unwrap().price
    );
    assert!(host.game.campaign().has_node("quick_reload"));
    assert_eq!(
        host.notices
            .iter()
            .filter(|n| matches!(n, RuntimeNotice::CommandCompleted { .. }))
            .count(),
        1
    );
    assert!(host.directory.join("campaign.json").exists());
}
#[test]
fn rejected_commands_and_settlement_failures_reach_notices_as_text() {
    let mut host = Host::new();
    let tactics = horde_core::command::NpcTactics {
        policy: host.game.campaign().npc.policy,
        prefer_aim: false,
        early_reload: false,
        allow_risk: false,
        recall_percent: 76,
    };
    host.runtime.enqueue(Command::SetNpcPolicy(tactics), "");
    let countries: Vec<_> = horde_core::world::world()
        .countries
        .iter()
        .take(4)
        .map(|c| (c.id.clone(), c.level_count()))
        .collect();
    host.game
        .campaign_mut()
        .choose_start_country(&countries[0].0)
        .unwrap();
    for (id, count) in countries {
        host.game
            .campaign_mut()
            .world
            .country_levels
            .insert(id.clone(), count);
        host.game.campaign_mut().world.cleared.push(id);
    }
    host.game.campaign_mut().coins = 10_000;
    host.game.campaign_mut().attempts = 1;
    let country = host.game.campaign().world.cleared[0].clone();
    let recruit = host
        .game
        .with_strategy(|strategy, campaign| strategy.recruit(campaign, &country))
        .unwrap();
    let battle = host.game.deploy_survivor(recruit, &country, 0).unwrap();
    host.game.campaign_mut().next_item_id = u64::MAX;
    host.game.battles_mut()[0]
        .game_mut()
        .campaign_mut()
        .add_item_with_quality(4, 9, None)
        .unwrap();
    host.runtime.enqueue(Command::RecallSurvivor(battle), "");
    flush(&mut host.ctx());
    assert!(host.notices.iter().any(|n| matches!(
        n,
        RuntimeNotice::CommandCompleted { result: Err(error), .. }
            if error == "Recall threshold must be between 0 and 75 percent"
    )));
    assert!(host.notices.iter().any(|n| matches!(
        n,
        RuntimeNotice::CommandCompleted {
            command: AppCommand::Game(Command::RecallSurvivor(_)),
            result: Ok(AppOutcome::Command(CommandOutcome::Applied)),
            ..
        }
    )));
    let issues: Vec<_> = host
        .notices
        .iter()
        .filter_map(|n| match n {
            RuntimeNotice::SettlementIssues(issues) => Some(issues.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(issues.len(), 1);
    assert!(matches!(
        issues[0].as_slice(),
        [SettlementIssue::LootLost { battle: b, .. }] if *b == battle
    ));
    assert!(host.game.take_settlement_issues().is_empty());
}
#[test]
fn pressed_reload_is_consumed_once_across_fixed_ticks_and_speed() {
    let mut host = Host::new();
    host.deploy();
    host.game.attempt_mut().unwrap().ammo = 0;
    host.runtime.publish(ControlFrame {
        enabled: true,
        speed: 4,
        held: Input {
            fire: true,
            auto_reload: false,
            ..Input::default()
        },
        pressed: Input {
            reload: true,
            auto_reload: false,
            ..Input::default()
        },
        pressed_aims: AttackAims::default(),
    });
    for _ in 0..125 {
        step(&mut host.ctx());
    }
    let attempt = host.game.attempt().unwrap();
    assert_eq!(attempt.tick, 500);
    assert_eq!(attempt.shots, attempt.magazine);
    assert_eq!(attempt.ammo, 0);
    assert_eq!(attempt.reload_left, 0);
}
#[test]
fn pause_freezes_main_parallel_and_strategy_without_catch_up() {
    let mut host = Host::new();
    host.deploy();
    host.game.set_paused(true);
    host.runtime.publish(ControlFrame {
        enabled: true,
        speed: 4,
        held: Input {
            fire: true,
            ..Input::default()
        },
        pressed: Input {
            reload: true,
            ..Input::default()
        },
        pressed_aims: AttackAims::default(),
    });
    let before = state(&host.game);
    for _ in 0..120 {
        step(&mut host.ctx());
    }
    assert_eq!(before, state(&host.game));
    assert_eq!(host.runtime.simulated_ticks, 0);
    host.game.set_paused(false);
    step(&mut host.ctx());
    assert_eq!(host.game.attempt().unwrap().tick, 4);
    assert_eq!(host.game.attempt().unwrap().shots, 0);
}
#[test]
fn observation_speed_changes_only_number_of_whole_ticks() {
    let mut normal = Host::new();
    normal.deploy();
    let mut fast = Host::new();
    fast.game = normal.game.clone();
    normal.runtime.publish(ControlFrame {
        enabled: true,
        speed: 1,
        ..ControlFrame::default()
    });
    fast.runtime.publish(ControlFrame {
        enabled: true,
        speed: 4,
        ..ControlFrame::default()
    });
    for _ in 0..240 {
        step(&mut normal.ctx());
    }
    for _ in 0..60 {
        step(&mut fast.ctx());
    }
    assert_eq!(state(&normal.game), state(&fast.game));
    assert_eq!(normal.runtime.simulated_ticks, fast.runtime.simulated_ticks);
}
#[test]
fn quit_is_acknowledged_only_after_durable_current_state_commit() {
    let mut host = Host::new();
    host.deploy();
    host.game.attempt_mut().unwrap().reload_left = 37;
    host.runtime.request_save(SaveIntent::Quit);
    assert!(flush_for_test(&mut host.ctx()));
    let restored = Game::load(&host.directory.join("campaign.json")).unwrap();
    assert_eq!(state(&host.game), state(&restored));
    assert!(host.notices.iter().any(|n| matches!(
        n,
        RuntimeNotice::Saved {
            intent: SaveIntent::Quit,
            result: Ok(())
        }
    )));
}
#[test]
fn failed_commit_never_acknowledges_quit_or_runs_a_hidden_tick() {
    let mut host = Host::new();
    host.deploy();
    std::fs::create_dir_all(&host.directory).unwrap();
    std::fs::write(
        host.directory.join("campaign.json"),
        b"invalid existing save",
    )
    .unwrap();
    // A directory at the temporary-file path makes the atomic write fail.
    std::fs::create_dir(host.directory.join("campaign.tmp")).unwrap();
    let before = state(&host.game);
    host.runtime.request_save(SaveIntent::Quit);
    assert!(!flush_for_test(&mut host.ctx()));
    assert_eq!(before, state(&host.game));
    assert!(host.notices.iter().any(|n| matches!(
        n,
        RuntimeNotice::Saved {
            intent: SaveIntent::Quit,
            result: Err(_)
        }
    )));
    std::fs::remove_dir(host.directory.join("campaign.tmp")).unwrap();
}
#[test]
fn terminal_main_attempt_stops_remaining_fixed_catch_up_until_new_input() {
    let mut host = Host::new();
    host.deploy();
    let mut attempt = host.game.attempt_slot_mut().take().unwrap();
    attempt.boss_spawned = true;
    host.game
        .spawn_enemy(&mut attempt, horde_core::content::EnemyKind::Brute, 5, true);
    attempt.enemies.last_mut().unwrap().hp = 0;
    *host.game.attempt_slot_mut() = Some(attempt);
    host.runtime.publish(ControlFrame {
        enabled: true,
        speed: 4,
        ..ControlFrame::default()
    });
    step(&mut host.ctx());
    assert!(host.game.attempt().is_none());
    assert_eq!(host.runtime.simulated_ticks, 1);
    for _ in 0..10 {
        step(&mut host.ctx());
    }
    assert_eq!(host.runtime.simulated_ticks, 1);
    assert_eq!(
        host.notices
            .iter()
            .filter(|notice| matches!(notice, RuntimeNotice::SimulationFinished))
            .count(),
        1
    );
}
#[test]
fn corrupt_primary_and_backup_are_uniquely_preserved_before_new_campaign_autosave() {
    let mut host = Host::new();
    std::fs::create_dir_all(&host.directory).unwrap();
    let original = b"unreadable primary bytes";
    let backup = b"unreadable backup bytes";
    std::fs::write(host.directory.join("campaign.json"), original).unwrap();
    std::fs::write(host.directory.join("campaign.bak"), backup).unwrap();
    let loaded = load_profile(host.directory.clone(), Default::default());
    assert!(loaded.warning.contains("Original files remain unchanged"));
    host.game = loaded.game;
    host.runtime.begin_frame(1);
    host.runtime.enqueue(
        Command::ChooseSurvivor(horde_core::survivors::SurvivorId::Rook),
        "Survivor selected",
    );
    host.runtime
        .enqueue(Command::ChooseStart("DEU".into()), "New country selected");
    assert!(!flush_for_test(&mut host.ctx()));
    let copies: Vec<_> = host
        .notices
        .iter()
        .filter_map(|notice| {
            if let RuntimeNotice::RecoveryCopies(paths) = notice {
                Some(paths)
            } else {
                None
            }
        })
        .flatten()
        .collect();
    assert_eq!(copies.len(), 2);
    assert_ne!(copies[0], copies[1]);
    let bytes: Vec<_> = copies
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect();
    assert!(bytes.iter().any(|data| data == original));
    assert!(bytes.iter().any(|data| data == backup));
    let restored = Game::load(&host.directory.join("campaign.json")).unwrap();
    assert_eq!(restored.campaign().world.start.as_deref(), Some("DEU"));
}
#[test]
fn practice_profiles_save_and_resume_without_changing_the_ordinary_campaign() {
    let mut main = Host::new();
    main.game.campaign_mut().coins = 7;
    main.runtime.request_save(SaveIntent::Manual);
    flush_for_test(&mut main.ctx());
    let normal_bytes = std::fs::read(main.directory.join("campaign.json")).unwrap();
    let directory = profile_directory(main.directory.clone(), Some("operations"), true);
    {
        let mut practice = Host::new();
        practice.directory = directory.clone();
        practice.paths = ProfilePaths::in_directory(directory.clone());
        practice
            .game
            .campaign_mut()
            .choose_start_country("DEU")
            .unwrap();
        practice.game.campaign_mut().coins = 300;
        practice.runtime.request_save(SaveIntent::Manual);
        flush_for_test(&mut practice.ctx());
        let loaded = load_profile(directory, Default::default());
        assert_eq!(loaded.game.campaign().coins, 300);
        assert_eq!(loaded.game.campaign().world.start.as_deref(), Some("DEU"));
        assert_eq!(
            std::fs::read(main.directory.join("campaign.json")).unwrap(),
            normal_bytes
        );
    }
    std::fs::remove_dir(main.directory.join("practice")).unwrap();
}
#[test]
fn named_fixed_schedule_advances_once_per_invocation_without_egui() {
    let mut host = Host::new();
    host.deploy();
    host.runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    let mut app = App::new();
    app.insert_resource(GameSession {
        game: std::mem::take(&mut host.game),
    })
    .insert_resource(std::mem::take(&mut host.runtime))
    .insert_resource(host.settings.clone())
    .insert_resource(host.paths.clone())
    .add_plugins(SimRuntimePlugin);
    app.world_mut().run_schedule(FixedUpdate);
    app.world_mut().run_schedule(FixedUpdate);
    assert_eq!(
        app.world()
            .resource::<GameSession>()
            .game
            .attempt()
            .unwrap()
            .tick,
        2
    );
    app.world_mut()
        .resource_mut::<Runtime>()
        .finish_storage_for_test();
}
#[test]
fn paused_fixed_steps_report_no_game_change() {
    let mut host = Host::new();
    host.deploy();
    host.game.set_paused(true);
    host.runtime.publish(ControlFrame {
        enabled: true,
        ..ControlFrame::default()
    });
    let mut ctx = host.ctx();
    step(&mut ctx);
    assert!(!ctx.game_changed, "paused steps must not dirty GameSession");
    ctx.game.set_paused(false);
    step(&mut ctx);
    assert!(ctx.game_changed);
}
#[test]
fn confirmed_new_journey_archives_the_exact_current_game_and_preserves_preferences() {
    let mut host = Host::new();
    host.deploy();
    host.game.set_paused(true);
    host.game.campaign_mut().coins = 123;
    host.settings.scale = 1.17;
    let path = host.directory.join("campaign.json");
    host.game.save(&path).unwrap();
    write_settings(&host.settings, &host.directory.join("settings.json")).unwrap();
    let settings = std::fs::read(host.directory.join("settings.json")).unwrap();
    // A Custom balance edited from Hard keeps the Hard extras (W1-29 review).
    let mut hard_edit =
        horde_core::balance::BalanceSettings::for_preset(horde_core::balance::BalancePreset::Hard);
    hard_edit.supply = 7;
    host.game.apply(&Command::SetBalance(hard_edit)).unwrap();
    assert_eq!(
        host.game.campaign().balance_extras_preset(),
        horde_core::balance::BalancePreset::Hard
    );
    host.game.save(&path).unwrap();
    let before = state(&host.game);
    let balance = host.game.campaign().balance_settings.clone();
    host.runtime.enqueue(AppCommand::NewJourney, "");
    assert!(!flush_for_test(&mut host.ctx()));
    let archive = host
        .notices
        .iter()
        .find_map(|n| match n {
            RuntimeNotice::CommandCompleted {
                command: AppCommand::NewJourney,
                result: Ok(AppOutcome::JourneyArchived(Some(path))),
                ..
            } => Some(path.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(state(&Game::load(&archive).unwrap()), before);
    assert!(host.game.campaign().selected_survivor.is_none());
    assert!(host.game.campaign().world.start.is_none());
    assert!(host.game.battles().is_empty());
    assert!(host.game.attempt().is_none());
    assert_eq!(host.game.campaign().coins, 0);
    assert_eq!(host.game.campaign().attempts, 0);
    assert!(!host.game.campaign().journey_active);
    assert!(!host.game.campaign().legacy_features);
    assert_eq!(host.game.campaign().balance_settings, balance);
    assert_eq!(
        host.game.campaign().balance_extras_preset(),
        horde_core::balance::BalancePreset::Hard
    );
    let reloaded = Game::load(&path).unwrap();
    assert_eq!(
        reloaded.campaign().balance_extras_basis,
        horde_core::balance::BalancePreset::Hard
    );
    assert_eq!(state(&reloaded), state(&host.game));
    assert_eq!(
        std::fs::read(host.directory.join("settings.json")).unwrap(),
        settings
    );
    assert_eq!(host.settings.scale, 1.17);
    assert!(!host.runtime.controls.enabled);
}
#[test]
fn failed_new_journey_archive_or_commit_preserves_the_current_live_and_saved_generation() {
    for archive_failure in [true, false] {
        let mut host = Host::new();
        host.deploy();
        host.game.set_paused(true);
        let path = host.directory.join("campaign.json");
        host.game.save(&path).unwrap();
        let saved = std::fs::read(&path).unwrap();
        let before = state(&host.game);
        if archive_failure {
            std::fs::write(host.directory.join("journey-archives"), b"blocked").unwrap();
        } else {
            std::fs::create_dir(path.with_extension("tmp")).unwrap();
        }
        host.runtime.enqueue(AppCommand::NewJourney, "");
        assert!(!flush_for_test(&mut host.ctx()));
        assert_eq!(state(&host.game), before);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert!(host.notices.iter().any(|n| matches!(
            n,
            RuntimeNotice::CommandCompleted {
                command: AppCommand::NewJourney,
                result: Err(_),
                ..
            }
        )));
        if archive_failure {
            std::fs::remove_file(host.directory.join("journey-archives")).unwrap();
        } else {
            std::fs::remove_dir(path.with_extension("tmp")).unwrap();
        }
    }
}
/// W1-35 review: the stress review never saves (`Runtime::forbid_saves`), so
/// New Journey from its menu is refused before the storage reset: no archive,
/// no fresh campaign.json, the live game unchanged and a failed completion.
#[cfg(feature = "review")]
#[test]
fn new_journey_on_an_unsaveable_review_writes_nothing() {
    let mut host = Host::new();
    host.deploy();
    host.game.set_paused(true);
    host.runtime.forbid_saves();
    let path = host.directory.join("campaign.json");
    let before = state(&host.game);
    host.runtime.enqueue(AppCommand::NewJourney, "");
    assert!(!flush_for_test(&mut host.ctx()));
    assert!(!path.exists(), "no campaign.json was written");
    assert!(!host.directory.join("journey-archives").exists());
    assert!(!host.runtime.reset_in_progress());
    assert_eq!(state(&host.game), before);
    assert!(host.notices.iter().any(|n| matches!(
        n,
        RuntimeNotice::CommandCompleted {
            command: AppCommand::NewJourney,
            result: Err(_),
            ..
        }
    )));
}
#[test]
fn leaving_journey_is_acknowledged_only_after_durable_save_and_never_loses_earned_state() {
    let mut host = Host::new();
    host.deploy();
    host.game.withdraw();
    host.game.set_paused(true);
    let coins = host.game.campaign().coins;
    let records = host.game.campaign().run_history.len();
    host.runtime.enqueue(Command::LeaveJourney, "");
    flush_for_test(&mut host.ctx());
    let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
    assert!(!saved.campaign().journey_active);
    assert_eq!(saved.campaign().coins, coins);
    assert_eq!(saved.campaign().run_history.len(), records);
    assert!(host.notices.iter().any(|n| matches!(
        n,
        RuntimeNotice::Saved {
            intent: SaveIntent::Menu,
            result: Ok(())
        }
    )));
}
#[test]
fn stale_quit_cannot_exit_a_new_journey_and_reset_blocks_old_domain_commands_and_ticks() {
    let mut host = Host::new();
    host.deploy();
    host.game.campaign_mut().coins = 100;
    let (service, gate) = persistence::gated();
    host.runtime.storage = service;
    host.runtime.request_save(SaveIntent::Quit);
    assert!(!flush(&mut host.ctx()));
    gate.started();
    let original = state(&host.game);
    host.runtime.enqueue(AppCommand::NewJourney, "");
    assert!(!flush(&mut host.ctx()));
    host.runtime
        .enqueue(Command::BuySkillNode("steady_hands".into()), "");
    assert!(!flush(&mut host.ctx()));
    host.runtime.publish(ControlFrame {
        enabled: true,
        speed: 4,
        ..Default::default()
    });
    step(&mut host.ctx());
    assert_eq!(state(&host.game), original);
    assert_eq!(host.runtime.simulated_ticks, 0);
    gate.release();
    persistence::wait_for_test(&mut host.runtime.storage);
    assert!(!persist_pending(&mut host.ctx()));
    assert!(gate.started().reset);
    assert_eq!(state(&host.game), original);
    gate.release();
    persistence::wait_for_test(&mut host.runtime.storage);
    assert!(!persist_pending(&mut host.ctx()));
    assert!(host.game.campaign().world.start.is_none());
    assert_eq!(host.game.campaign().coins, 0);
    assert!(!host.notices.iter().any(|n| matches!(
        n,
        RuntimeNotice::Saved {
            intent: SaveIntent::Quit,
            result: Ok(())
        }
    )));
}
#[test]
fn manual_and_navigation_requests_never_acknowledge_an_older_inflight_snapshot() {
    for intent in [
        SaveIntent::Manual,
        SaveIntent::Menu,
        SaveIntent::Quit,
        SaveIntent::SettingsBack,
    ] {
        let mut host = Host::new();
        let (service, gate) = persistence::gated();
        host.runtime.storage = service;
        host.runtime.request_save(SaveIntent::Autosave);
        assert!(!persist_pending(&mut host.ctx()));
        gate.started();
        host.game.campaign_mut().coins = 77;
        host.settings.master = 0.41;
        host.runtime.request_save(intent);
        assert!(!persist_pending(&mut host.ctx()));
        assert!(gate.no_next_job());
        assert!(!host.notices.iter().any(
            |n| matches!(n,RuntimeNotice::Saved{intent:found,result:Ok(())} if *found==intent)
        ));
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert!(!persist_pending(&mut host.ctx()));
        assert_eq!(gate.started().coins, 77);
        assert!(!host.notices.iter().any(
            |n| matches!(n,RuntimeNotice::Saved{intent:found,result:Ok(())} if *found==intent)
        ));
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert_eq!(persist_pending(&mut host.ctx()), intent == SaveIntent::Quit);
        let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
        assert_eq!(saved.campaign().coins, 77);
        assert_eq!(
            Settings::load(&host.directory.join("settings.json")).master,
            0.41
        );
        assert!(host.notices.iter().any(
            |n| matches!(n,RuntimeNotice::Saved{intent:found,result:Ok(())} if *found==intent)
        ));
    }
}
/// `persist_pending` hands the storage worker `Game::save_snapshot`, not a
/// full clone: the live game's undrained event records stay with the
/// frontend and never cross to the worker.
#[test]
fn a_save_hands_storage_the_snapshot_without_transient_events() {
    let pending = |game: &Game| game.clone().drain_events().count();
    let mut host = Host::new();
    host.deploy();
    for _ in 0..600 {
        if pending(&host.game) > 0 {
            break;
        }
        host.game.tick(Input::default());
    }
    let live = pending(&host.game);
    assert!(live > 0, "the live game holds undrained records");
    let (service, gate) = persistence::gated();
    host.runtime.storage = service;
    host.runtime.request_save(SaveIntent::Manual);
    assert!(!persist_pending(&mut host.ctx()));
    assert_eq!(
        gate.started().events,
        0,
        "the worker was handed {live} transient records"
    );
    assert_eq!(pending(&host.game), live, "the live outbox is untouched");
    gate.release();
    persistence::wait_for_test(&mut host.runtime.storage);
    assert!(!persist_pending(&mut host.ctx()));
}
#[test]
fn critical_save_barrier_rejects_legal_mutations_and_saves_the_ordered_paused_generation() {
    for intent in [SaveIntent::Menu, SaveIntent::Quit, SaveIntent::SettingsBack] {
        let mut host = Host::new();
        host.game
            .campaign_mut()
            .choose_start_country("DEU")
            .unwrap();
        host.game.campaign_mut().coins = 77;
        assert!(
            host.game
                .campaign()
                .can_purchase_node("steady_hands")
                .is_ok()
        );
        let (service, gate) = persistence::gated();
        host.runtime.storage = service;
        host.runtime.enqueue(Command::SetPaused(true), "");
        host.runtime.enqueue(AppCommand::Save(intent), "");
        assert!(host.runtime.durable_transition_pending());
        assert!(!flush(&mut host.ctx()));
        gate.started();
        assert!(host.game.global_paused());
        let state_before = state(&host.game);
        assert!(
            !host
                .runtime
                .enqueue(Command::BuySkillNode("steady_hands".into()), "")
        );
        assert!(!host.runtime.enqueue(
            Command::Deploy {
                stage: 0,
                mode: Mode::Personal
            },
            ""
        ));
        host.runtime.publish(ControlFrame {
            enabled: true,
            speed: 4,
            ..Default::default()
        });
        step(&mut host.ctx());
        assert_eq!(state(&host.game), state_before);
        assert_eq!(host.runtime.simulated_ticks, 0);
        assert!(!flush(&mut host.ctx()));
        assert!(gate.no_next_job());
        gate.release();
        persistence::wait_for_test(&mut host.runtime.storage);
        assert_eq!(persist_pending(&mut host.ctx()), intent == SaveIntent::Quit);
        assert_eq!(
            state(&Game::load(&host.directory.join("campaign.json")).unwrap()),
            state_before
        );
    }
}
#[test]
fn quit_requested_during_reset_is_recaptured_after_fresh_state_and_pause_are_applied() {
    let mut host = Host::new();
    host.game.campaign_mut().coins = 77;
    let (service, gate) = persistence::gated();
    host.runtime.storage = service;
    host.runtime.enqueue(AppCommand::NewJourney, "");
    assert!(!flush(&mut host.ctx()));
    assert!(gate.started().reset);
    host.runtime.enqueue(AppCommand::Save(SaveIntent::Quit), "");
    assert!(!flush(&mut host.ctx()));
    gate.release();
    persistence::wait_for_test(&mut host.runtime.storage);
    assert!(!persist_pending(&mut host.ctx()));
    assert_eq!(host.game.campaign().coins, 0);
    assert!(host.runtime.durable_transition_pending());
    assert!(!flush(&mut host.ctx()));
    let next = gate.started();
    assert!(!next.reset);
    assert_eq!(next.coins, 0);
    assert!(host.game.global_paused());
    gate.release();
    persistence::wait_for_test(&mut host.runtime.storage);
    assert!(persist_pending(&mut host.ctx()));
    let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
    assert!(saved.global_paused());
    assert_eq!(saved.campaign().coins, 0);
}
#[test]
fn same_batch_new_journey_then_native_close_preserves_quit_for_the_fresh_paused_generation() {
    let mut host = Host::new();
    host.game.campaign_mut().coins = 77;
    let (service, gate) = persistence::gated();
    host.runtime.storage = service;
    assert!(host.runtime.enqueue(AppCommand::NewJourney, ""));
    // Native close normally enqueues these together. A queued reset may
    // reject this old-state pause; the deferred fresh pause is mandatory.
    host.runtime.enqueue(Command::SetPaused(true), "");
    assert!(host.runtime.enqueue(AppCommand::Save(SaveIntent::Quit), ""));
    assert!(!flush(&mut host.ctx()));
    assert!(gate.started().reset);
    gate.release();
    persistence::wait_for_test(&mut host.runtime.storage);
    assert!(!persist_pending(&mut host.ctx()));
    assert_eq!(host.game.campaign().coins, 0);
    assert!(host.runtime.durable_transition_pending());
    assert!(!flush(&mut host.ctx()));
    let saved = gate.started();
    assert!(!saved.reset);
    assert_eq!(saved.coins, 0);
    assert!(host.game.global_paused());
    gate.release();
    persistence::wait_for_test(&mut host.runtime.storage);
    assert!(persist_pending(&mut host.ctx()));
    let saved = Game::load(&host.directory.join("campaign.json")).unwrap();
    assert_eq!(saved.campaign().coins, 0);
    assert!(saved.global_paused());
    assert!(saved.campaign().selected_survivor.is_none());
}

/// W3-06 (UX-08): "Open folder" runs in the runner, through the injectable
/// opener, and shows the profile directory.
#[test]
fn open_save_folder_uses_profile_directory() {
    let mut host = Host::new();
    let opener = RecordingOpener::default();
    host.runtime.set_folder_opener(opener.clone());
    std::fs::create_dir_all(&host.directory).unwrap();
    let before = state(&host.game);
    host.runtime.begin_frame(1);
    assert!(host.runtime.enqueue(AppCommand::OpenSaveFolder, ""));
    assert!(!flush(&mut host.ctx()));
    assert_eq!(host.paths.directory(), host.directory);
    assert_eq!(opener.opened(), vec![host.directory.clone()]);
    assert!(
        matches!(
            host.notices.as_slice(),
            [RuntimeNotice::SaveFolderOpened(Ok(()))]
        ),
        "{:?}",
        host.notices
    );
    // Showing the folder is not gameplay and not a save.
    assert_eq!(state(&host.game), before);
    assert!(!host.runtime.commands_applied_since_load());
    assert!(!host.runtime.saved_since_load());
    assert!(!host.runtime.durable_transition_pending());

    // An opener that cannot start reports why; nothing else changes.
    host.notices.clear();
    host.runtime.set_folder_opener(RecordingOpener::failing(
        "explorer.exe could not be started: not found",
    ));
    host.runtime.begin_frame(2);
    assert!(host.runtime.enqueue(AppCommand::OpenSaveFolder, ""));
    flush(&mut host.ctx());
    assert!(
        matches!(
            host.notices.as_slice(),
            [RuntimeNotice::SaveFolderOpened(Err(error))] if error.contains("not found")
        ),
        "{:?}",
        host.notices
    );

    // Before the first save creates the profile directory, its nearest
    // existing ancestor is shown instead of a path the file manager lacks.
    let mut fresh = Host::new();
    assert!(!fresh.directory.exists());
    let opener = RecordingOpener::default();
    fresh.runtime.set_folder_opener(opener.clone());
    fresh.runtime.begin_frame(1);
    fresh.runtime.enqueue(AppCommand::OpenSaveFolder, "");
    flush(&mut fresh.ctx());
    assert_eq!(
        opener.opened(),
        vec![fresh.directory.parent().unwrap().to_path_buf()]
    );
    assert!(
        !fresh.directory.exists(),
        "showing a folder creates nothing"
    );
}

/// W3-06: `ReloadProfile` reads the save again only while neither the
/// in-memory journey nor the files changed since the load.
#[test]
fn reload_profile_reads_the_save_until_the_journey_or_the_files_change() {
    let reload = |host: &mut Host, frame: u32| -> Result<(), String> {
        host.notices.clear();
        host.runtime.begin_frame(frame);
        assert!(host.runtime.enqueue(AppCommand::ReloadProfile, ""));
        flush(&mut host.ctx());
        match host.notices.as_slice() {
            [RuntimeNotice::ProfileReloaded(result)] => result.clone(),
            other => panic!("{other:?}"),
        }
    };
    let mut host = Host::new();
    std::fs::create_dir_all(&host.directory).unwrap();
    let path = host.directory.join("campaign.json");
    // Unreadable: the in-memory journey stays, and nothing is written.
    std::fs::write(&path, b"not a save").unwrap();
    let before = state(&host.game);
    assert!(reload(&mut host, 1).is_err());
    assert_eq!(state(&host.game), before);
    assert_eq!(std::fs::read(&path).unwrap(), b"not a save");
    // Readable: it replaces the in-memory journey.
    std::fs::remove_file(&path).unwrap();
    let mut saved = Game::default();
    saved.campaign_mut().coins = 77;
    saved.save(&path).unwrap();
    assert_eq!(reload(&mut host, 2), Ok(()));
    assert_eq!(host.game.campaign().coins, 77);
    assert!(host.runtime.reload_allowed());
    // A pause is not progress.
    host.runtime.begin_frame(3);
    host.runtime.enqueue(Command::SetPaused(true), "");
    host.runtime.enqueue(Command::SetPaused(false), "");
    flush(&mut host.ctx());
    assert!(!host.runtime.commands_applied_since_load());
    // A gameplay command is: the save is not read over it.
    host.runtime.begin_frame(4);
    host.runtime.enqueue(
        Command::ChooseSurvivor(horde_core::survivors::SurvivorId::Rook),
        "",
    );
    flush_for_test(&mut host.ctx());
    assert!(host.runtime.commands_applied_since_load());
    assert!(
        host.runtime.saved_since_load(),
        "the autosave replaced the file"
    );
    assert!(!host.runtime.reload_allowed());
    let played = state(&host.game);
    assert_eq!(reload(&mut host, 5), Err(RELOAD_REFUSED.into()));
    assert_eq!(state(&host.game), played);

    // A save alone closes the door too, with no command applied.
    let mut host = Host::new();
    host.runtime.begin_frame(1);
    host.runtime
        .enqueue(AppCommand::Save(SaveIntent::Manual), "");
    flush_for_test(&mut host.ctx());
    assert!(!host.runtime.commands_applied_since_load());
    assert!(host.runtime.saved_since_load());
    assert_eq!(reload(&mut host, 2), Err(RELOAD_REFUSED.into()));
}

/// Lane pan (A6-07): the root battle's positional cues carry the pan place
/// of their event, the place depends on the event's x alone, and the
/// defender's own shots stay centred.
#[test]
fn root_battle_cues_carry_their_lane_pan_place() {
    use horde_audio::pan::{Places, lane_position, positional};
    let mut game = Game::default();
    game.deploy(0, Mode::Personal).unwrap();
    game.set_paused(false);
    let width = game.attempt().unwrap().width_yards;
    let mut heard = std::collections::BTreeMap::new();
    for _ in 0..60 * 60 {
        if game.attempt().is_none() {
            break;
        }
        let input = horde_core::sim::smoke_input(&game);
        game.tick(input);
        let (mut cues, mut places) = (Vec::new(), Places::default());
        tick_sounds(&mut game, None, &mut cues, &mut places);
        for cue in cues {
            let place = places.of(cue);
            assert!(positional(cue) || place.is_none(), "{cue:?} is centred");
            if let Some(place) = place {
                assert!((0.0..=1.0).contains(&place), "{cue:?} at {place}");
                heard.entry(format!("{cue:?}")).or_insert(place);
            }
        }
    }
    // Enemies enter at the far end; kills and impacts happen on the field.
    let incoming = heard["Incoming"];
    assert!(incoming > lane_position(width - 3.0, width), "{heard:?}");
    assert!(
        heard.contains_key("Kill") && heard.contains_key("Impact"),
        "{heard:?}"
    );
    assert!(heard["Kill"] < incoming, "{heard:?}");
}
