//! The exhaustive event mapping (A6-12): every `SimEvent` variant has an
//! explicit decision, enemies sound by catalogue id, bosses by kind, a
//! defender's shot by weapon class and gun quality.

use horde_audio::{
    bank::{BOSS_KINDS, BankId, BossStage, EnemyStage, EnemyVoice},
    cue::Cue,
    events::{EventContext, event_requests},
    mix::mix,
    pan::Places,
    select::{CueGate, SUBSUMES, select_sounds},
    voicing::{
        CAPSTONE_GAIN_DB, CAPSTONE_SPEED, ELITE_GAIN_DB, ELITE_SPEED, ENEMY_VOICES,
        EnemyVoiceIndex, EnemyVoiceRef, Tone, Voicing, boss_voicing, kind_voice, quality_tone,
        voice_row,
    },
};
use horde_core::{
    content::{BossKind, EnemyKind, Mode, StructureKind},
    enemy_catalog::enemy_catalog,
    ordnance::OrdnanceKind,
    progression::BossRank,
    quality::Quality,
    sim::{
        Actor, BattleId, CorpseRecord, DetonationTrigger, Game, Input, Outcome, SimEvent, Strike,
    },
    survivors::EquipmentVisualItem,
    weapons::WeaponClass,
};
use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const ROOT: EventContext<'static> = EventContext {
    battle: BattleId::Root,
    width_yards: Some(30.0),
    distant: false,
    corpses: &[],
};

/// The cues and places of one event in `ctx`.
fn requests(
    voices: &mut EnemyVoiceIndex,
    event: &SimEvent,
    ctx: &EventContext,
) -> (Vec<Cue>, Places) {
    let (mut cues, mut places) = (Vec::new(), Places::default());
    event_requests(&mut cues, &mut places, voices, event, ctx);
    (cues, places)
}
/// The cues of one event with nothing indexed.
fn plain(event: &SimEvent) -> Vec<Cue> {
    requests(&mut EnemyVoiceIndex::default(), event, &ROOT).0
}

fn strike(attacker: Actor, target_id: Option<u64>) -> Strike {
    Strike {
        attacker,
        target_id,
        lane: None,
        from: [12.0, 0.5],
        to: [0.5, 0.5],
        value: 1,
    }
}
const DEFENDER: Actor = Actor::Defender {
    mode: Mode::Personal,
    weapon: WeaponClass::Rifle,
};
const ENEMY: Actor = Actor::Enemy { id: 7 };
fn shot(shooter: Actor) -> SimEvent {
    SimEvent::Shot {
        shooter,
        target_id: None,
        lane: None,
        from: [0.5, 0.5],
        to: [15.0, 0.5],
    }
}
fn windup(structure_id: Option<u64>, x: f32) -> SimEvent {
    SimEvent::Windup {
        enemy_id: 7,
        structure_id,
        lane: None,
        duration_ticks: 30,
        from: [x, 0.5],
        to: [0.5, 0.5],
    }
}
fn kill(enemy_id: u64) -> SimEvent {
    SimEvent::Kill {
        enemy_id,
        lane: None,
        at: [10.0, 0.5],
        coins: 0,
    }
}
fn exposure(exposed: bool) -> SimEvent {
    SimEvent::BossExposure {
        boss_id: 7,
        exposed,
        lane: None,
        duration_ticks: 60,
        at: [10.0, 0.5],
    }
}

/// One sample of every variant with the cues it maps to when nothing is
/// indexed.
fn samples() -> Vec<(&'static str, SimEvent, Vec<Cue>)> {
    let at = [10.0, 0.5];
    vec![
        ("Shot", shot(DEFENDER), vec![Cue::Rifle]),
        (
            "Hit",
            SimEvent::Hit(strike(DEFENDER, Some(7))),
            vec![Cue::Impact],
        ),
        (
            "WeakPoint",
            SimEvent::WeakPoint(strike(DEFENDER, Some(7))),
            vec![Cue::Impact],
        ),
        (
            "Armor",
            SimEvent::Armor(strike(DEFENDER, Some(7))),
            vec![Cue::Armor],
        ),
        // The tracer is a miss's twin.
        ("Miss", SimEvent::Miss(strike(DEFENDER, Some(7))), vec![]),
        // The defender defended.
        (
            "Defended",
            SimEvent::Defended(strike(ENEMY, None)),
            vec![Cue::Block],
        ),
        (
            "Damage",
            SimEvent::Damage(strike(ENEMY, None)),
            vec![Cue::Damage],
        ),
        (
            "Melee",
            SimEvent::Melee(strike(DEFENDER, Some(7))),
            vec![Cue::Melee],
        ),
        // Nothing resolves enemy 7: the legacy kill.
        ("Kill", kill(7), vec![Cue::Kill]),
        ("Reload", SimEvent::Reload, vec![Cue::Reload]),
        (
            "Spawn",
            SimEvent::Spawn {
                enemy_id: 7,
                lane: None,
                at,
            },
            vec![Cue::Incoming],
        ),
        // No kind resolves: the legacy boss.
        (
            "Boss",
            SimEvent::Boss {
                enemy_id: 7,
                lane: None,
                at,
            },
            vec![Cue::Boss],
        ),
        (
            "StructureLost",
            SimEvent::StructureLost { at },
            vec![Cue::Structure],
        ),
        (
            "Ability",
            SimEvent::Ability { healed: 3 },
            vec![Cue::Ability],
        ),
        (
            "Contact",
            SimEvent::Contact {
                enemy_id: 7,
                structure_id: None,
                lane: None,
                at,
            },
            vec![Cue::EnemyContact],
        ),
        ("Windup", windup(None, 12.0), vec![Cue::EnemyWindup]),
        (
            "BossWarning",
            SimEvent::BossWarning {
                seconds_left: 10,
                lane: None,
                duration_ticks: 60,
            },
            vec![Cue::Wave],
        ),
        ("BossExposure", exposure(true), vec![Cue::BossExposed]),
        (
            "Reinforcements",
            SimEvent::Reinforcements {
                lane: None,
                duration_ticks: 60,
            },
            vec![Cue::Wave],
        ),
        (
            "Throw",
            SimEvent::Throw {
                projectile_id: 1,
                ordnance: OrdnanceKind::Grenade,
                lane: None,
                from: [0.5, 0.5],
                to: at,
                flight_ticks: 30,
            },
            vec![Cue::Throw],
        ),
        (
            "Explosion",
            SimEvent::Explosion {
                projectile_id: 1,
                ordnance: OrdnanceKind::Molotov,
                lane: None,
                at,
                radius: 2.0,
            },
            vec![Cue::Molotov],
        ),
        // The fire bed covers a burn; no cue per event.
        (
            "Burn",
            SimEvent::Burn {
                projectile_id: 1,
                lane: None,
                at,
                radius: 2.0,
                duration_ticks: 60,
            },
            vec![],
        ),
        ("CooldownReady", SimEvent::CooldownReady, vec![Cue::Ready]),
        (
            "RuleResolved",
            SimEvent::RuleResolved {
                attacker: DEFENDER,
                target_id: Some(7),
                summary: String::new(),
            },
            vec![],
        ),
        (
            "AttemptFinished",
            SimEvent::AttemptFinished {
                attempt_id: 1,
                outcome: Outcome::Withdrawal,
                mode: Mode::Personal,
                cause: String::new(),
            },
            vec![Cue::Retreat],
        ),
        // A ranged enemy's strike is its contact.
        (
            "Spit",
            SimEvent::Spit(strike(ENEMY, None)),
            vec![Cue::EnemyContact],
        ),
        (
            "Detonation",
            SimEvent::Detonation {
                enemy_id: 7,
                trigger: DetonationTrigger::Death,
                lane: None,
                at,
                radius: 2.0,
            },
            vec![Cue::Explosion],
        ),
        // The parent's death and the children's arrival sound; the split
        // itself has no cue.
        (
            "Split",
            SimEvent::Split {
                enemy_id: 7,
                lane: None,
                at,
                count: 2,
            },
            vec![],
        ),
        (
            "BossPhase",
            SimEvent::BossPhase {
                boss_id: 7,
                kind: BossKind::Caller,
                phase: 1,
                lane: None,
                at,
            },
            vec![Cue::BossPhase],
        ),
    ]
}

/// The variant names of `SimEvent`, read from the core's source.
fn sim_event_variants() -> Vec<String> {
    let source =
        std::fs::read_to_string(workspace().join("crates/horde_core/src/sim/events.rs")).unwrap();
    let body = source
        .split_once("pub enum SimEvent {")
        .expect("the SimEvent enum")
        .1;
    let body = body.split_once("\n}\n").unwrap().0;
    body.lines()
        // A variant sits one level in; its fields and comments further in or
        // behind `//`.
        .filter(|line| line.starts_with("    ") && !line.starts_with("     "))
        .map(|line| line.trim())
        .filter(|line| line.starts_with(|first: char| first.is_ascii_uppercase()))
        .map(|line| {
            line.split(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .unwrap()
                .to_owned()
        })
        .collect()
}

/// Every `SimEvent` variant is mapped by name: the mapping has no wildcard
/// arm, so a new variant is a compile error until someone decides its sound,
/// and each variant's decision is pinned here.
#[test]
fn sim_event_mapping_is_exhaustive() {
    let variants = sim_event_variants();
    // The 25 of the design, Spit, Detonation, Split and BossPhase.
    assert_eq!(variants.len(), 29, "{variants:?}");
    let samples = samples();
    assert_eq!(
        samples.iter().map(|sample| sample.0).collect::<Vec<_>>(),
        variants,
        "one sample per variant, in the enum's order"
    );
    for (name, event, cues) in &samples {
        assert_eq!(&plain(event), cues, "{name}");
    }
    // The mapping names every variant and has no wildcard arm at all.
    let source =
        std::fs::read_to_string(workspace().join("crates/horde_audio/src/events.rs")).unwrap();
    let mapping = source
        .split_once("pub fn event_requests(")
        .expect("event_requests")
        .1;
    let mapping = mapping.split_once("\n}\n").unwrap().0;
    for name in &variants {
        assert!(
            mapping.contains(&format!("SimEvent::{name}")),
            "event_requests does not name SimEvent::{name}"
        );
    }
    for line in mapping.lines() {
        let arm = line.trim_start();
        assert!(
            !arm.starts_with("_ =>") && !arm.starts_with("| _ =>") && !arm.contains("| _ =>"),
            "event_requests has a wildcard arm: {line}"
        );
    }
}

/// The exposure cue plays when the weak point opens; its closing is silent.
#[test]
fn boss_exposure_closing_is_silent() {
    let mut voices = EnemyVoiceIndex::default();
    let (cues, places) = requests(&mut voices, &exposure(true), &ROOT);
    assert_eq!(cues, vec![Cue::BossExposed]);
    // The Ready bank, lower; the mix row carries the +2 dB.
    let voicing = places.voicing(Cue::BossExposed);
    assert_eq!(voicing.bank, Cue::Ready.bank());
    assert_eq!(voicing.speed, 0.9);
    assert!(mix(Cue::BossExposed).gain > mix(Cue::Ready).gain);
    assert_eq!(plain(&exposure(false)), vec![]);
}

/// An NPC defender's shot sounds like its weapon class, whatever the mode.
#[test]
fn npc_defender_shotgun_plays_shotgun_family() {
    for mode in [Mode::Npc, Mode::Personal] {
        let event = shot(Actor::Defender {
            mode,
            weapon: WeaponClass::Shotgun,
        });
        let (cues, places) = requests(&mut EnemyVoiceIndex::default(), &event, &ROOT);
        assert_eq!(cues, vec![Cue::Shotgun], "{mode:?}");
        assert_eq!(places.voicing(Cue::Shotgun).bank, Cue::Shotgun.bank());
        assert!(!cues.iter().any(|cue| cue.retired()));
    }
    // Enemies, ordnance and vanished shooters fire no gun.
    for shooter in [
        ENEMY,
        Actor::Ordnance {
            id: 1,
            kind: OrdnanceKind::Grenade,
        },
        Actor::Unknown { id: 1 },
        Actor::Structure {
            id: 1,
            kind: StructureKind::Wall,
        },
    ] {
        assert_eq!(plain(&shot(shooter)), vec![], "{shooter:?}");
    }
}

fn deployed(mode: Mode) -> Game {
    let mut game = Game::default();
    game.set_seed(1);
    game.campaign_mut().attempts = 1;
    game.deploy(0, mode).unwrap();
    game.set_paused(false);
    game
}
fn equip(game: &mut Game, quality: Option<Quality>) {
    let attempt = game.staging().attempt().unwrap();
    attempt.profile.equipment_visual.ranged = quality.map(|quality| EquipmentVisualItem {
        definition: Default::default(),
        level: 1,
        quality,
        style: String::new(),
    });
}

/// The quality tone follows the gun in the attempt's profile, so a recruit
/// who took over the defense with a rare shotgun sounds rare.
#[test]
fn takeover_recruit_rare_shotgun_gets_rare_tone() {
    let event = shot(Actor::Defender {
        mode: Mode::Npc,
        weapon: WeaponClass::Shotgun,
    });
    let tone_of = |quality: Option<Quality>| {
        let mut game = deployed(Mode::Personal);
        equip(&mut game, quality);
        let mut voices = EnemyVoiceIndex::default();
        voices.sync(BattleId::Root, game.attempt().unwrap());
        let (cues, places) = requests(&mut voices, &event, &ROOT);
        assert_eq!(cues, vec![Cue::Shotgun]);
        places.voicing(Cue::Shotgun)
    };
    let rare = tone_of(Some(Quality::Rare));
    assert_eq!(
        rare,
        Voicing::toned(Cue::Shotgun, quality_tone(Quality::Rare))
    );
    assert_eq!(
        (rare.speed, rare.gain_db, rare.lowpass_hz),
        (1.005, 0.3, None)
    );
    // An empty ranged slot counts as Common.
    assert_eq!(
        tone_of(None),
        Voicing::toned(Cue::Shotgun, quality_tone(Quality::Common))
    );
    // The table: duller and lower below Uncommon, brighter and louder above.
    let table = Quality::ALL.map(|quality| {
        let tone = quality_tone(quality);
        (tone.speed, tone.lowpass_hz, tone.gain_db)
    });
    assert_eq!(
        table,
        [
            (0.985, Some(6000.0), -0.5),
            (1.0, Some(9000.0), 0.0),
            (1.0, None, 0.0),
            (1.005, None, 0.3),
            (1.010, None, 0.5),
            (1.015, None, 0.7),
        ]
    );
    // A small offset: within 1.5% of pitch and 1 dB.
    for (speed, _, gain_db) in table {
        assert!((speed - 1.0).abs() <= 0.015 && gain_db.abs() <= 1.0);
    }
}

/// A shot on an expedition watched from camp is distant: lower, duller and
/// quieter, whatever the gun's quality, and never the player's lane.
#[test]
fn observed_expedition_shot_uses_distant_tone() {
    let mut game = deployed(Mode::Personal);
    equip(&mut game, Some(Quality::Legendary));
    let battle = BattleId::Expedition(4);
    let mut voices = EnemyVoiceIndex::default();
    voices.sync(battle, game.attempt().unwrap());
    let ctx = EventContext {
        battle,
        width_yards: None,
        distant: true,
        corpses: &[],
    };
    let event = shot(Actor::Defender {
        mode: Mode::Npc,
        weapon: WeaponClass::Rifle,
    });
    let (cues, places) = requests(&mut voices, &event, &ctx);
    assert_eq!(cues, vec![Cue::Rifle]);
    assert_eq!(
        places.voicing(Cue::Rifle),
        Voicing::toned(Cue::Rifle, Tone::DISTANT)
    );
    assert_eq!(
        (
            Tone::DISTANT.speed,
            Tone::DISTANT.lowpass_hz,
            Tone::DISTANT.gain_db
        ),
        (0.97, Some(3500.0), -4.0)
    );
    assert_eq!(places.player_shot(), None);
    // The root battle's own shot takes the player-weapon lane.
    let (_, places) = requests(&mut voices, &event, &ROOT);
    assert_eq!(places.player_shot(), Some(Cue::Rifle));
}

/// The windup warning sounds for a windup against the defender. A windup
/// against a structure stays visual (its ring).
#[test]
fn windup_cue_only_for_defender_targeted_windups() {
    assert_eq!(plain(&windup(None, 12.0)), vec![Cue::EnemyWindup]);
    assert_eq!(plain(&windup(Some(9000), 12.0)), vec![]);
    assert!(mix(Cue::EnemyWindup).warning.is_some());
    // Of a frame's windups, the one nearest the defender stands.
    let mut voices = EnemyVoiceIndex::default();
    let (mut cues, mut places) = (Vec::new(), Places::default());
    for x in [20.0, 6.0, 12.0] {
        event_requests(&mut cues, &mut places, &mut voices, &windup(None, x), &ROOT);
    }
    assert_eq!(cues, vec![Cue::EnemyWindup]);
    assert_eq!(
        places.of(Cue::EnemyWindup),
        Some(horde_audio::pan::lane_position(6.0, 30.0))
    );
}

/// Every archetype of the catalogue has its own voice row, every row names
/// a catalogue archetype, and every bank a row can play ships.
#[test]
fn every_catalog_archetype_has_explicit_voice_row_with_existing_files() {
    let catalog = enemy_catalog();
    let ids: Vec<&str> = catalog.archetype_ids().collect();
    assert!(ids.len() >= 18, "{ids:?}");
    let assets = workspace().join("assets");
    for id in &ids {
        let row = voice_row(id).unwrap_or_else(|| panic!("{id} has no ENEMY_VOICES row"));
        assert!((0.8..=1.1).contains(&row.speed), "{id}");
        assert!((-3.0..=1.5).contains(&row.gain_db), "{id}");
        for stage in EnemyStage::ALL {
            let bank = BankId::Enemy(row.voice, stage);
            for variant in 0..bank.variants() {
                let path = bank.variant_path(variant);
                assert!(assets.join(&path).is_file(), "{id}: missing {path}");
            }
        }
    }
    for row in ENEMY_VOICES {
        assert!(
            ids.contains(&row.archetype_id),
            "{} is no archetype",
            row.archetype_id
        );
        assert_eq!(
            ENEMY_VOICES
                .iter()
                .filter(|other| other.archetype_id == row.archetype_id)
                .count(),
            1,
            "{}",
            row.archetype_id
        );
    }
    // Every voice is used, so no bank ships unheard.
    for voice in EnemyVoice::ALL {
        assert!(
            ENEMY_VOICES.iter().any(|row| row.voice == voice),
            "{voice:?}"
        );
    }
    // An id without a row falls back to its legacy kind's voice.
    for kind in EnemyKind::ALL {
        let fallback = EnemyVoiceRef::resolve(Some("not-in-the-table"), kind, false);
        assert_eq!((fallback.voice, fallback.speed), (kind_voice(kind), 1.0));
        // A kind's default archetype has that voice.
        let default = voice_row(catalog.default_for_kind(kind)).unwrap();
        assert_eq!(default.voice, kind_voice(kind), "{kind:?}");
    }
    // An elite plays its family's bank lower and louder.
    let plain = EnemyVoiceRef::resolve(Some("runner"), EnemyKind::Runner, false);
    let elite = EnemyVoiceRef::resolve(Some("runner"), EnemyKind::Runner, true);
    assert_eq!(elite.voice, plain.voice);
    assert_eq!(elite.speed, plain.speed * ELITE_SPEED);
    assert_eq!(elite.gain_db, plain.gain_db + ELITE_GAIN_DB);
    assert_eq!((ELITE_SPEED, ELITE_GAIN_DB), (0.94, 1.0));
}

/// Each boss kind has arrival, phase and death banks that ship; a boss of a
/// known kind plays them, the capstone lower and louder on the same bank,
/// and a boss without a kind falls back to the legacy cues.
#[test]
fn every_boss_kind_has_arrival_phase_death_banks_and_capstone_uses_kind_bank() {
    let assets = workspace().join("assets");
    for kind in BOSS_KINDS {
        for stage in BossStage::ALL {
            let bank = BankId::Boss(kind, stage);
            for variant in 0..bank.variants() {
                let path = bank.variant_path(variant);
                assert!(assets.join(&path).is_file(), "missing {path}");
            }
            let plain = boss_voicing(kind, stage, false);
            let capstone = boss_voicing(kind, stage, true);
            assert_eq!(plain, Voicing::bank(bank));
            assert_eq!(capstone.bank, bank);
            assert_eq!(
                (capstone.speed, capstone.gain_db),
                (CAPSTONE_SPEED, CAPSTONE_GAIN_DB)
            );
        }
    }
    assert_eq!((CAPSTONE_SPEED, CAPSTONE_GAIN_DB), (0.94, 1.0));
    // A live boss of each kind, in a plain and in a capstone attempt.
    for capstone in [false, true] {
        for kind in BOSS_KINDS {
            let mut game = deployed(Mode::Personal);
            for _ in 0..600 {
                if !game.attempt().unwrap().enemies.is_empty() {
                    break;
                }
                game.tick(Input::default());
            }
            let attempt = game.staging().attempt().unwrap();
            if capstone {
                attempt.progression.boss_rank = BossRank::Capstone;
            }
            let boss = &mut attempt.enemies[0];
            boss.boss = true;
            boss.boss_kind = Some(kind);
            let id = boss.id;
            let mut voices = EnemyVoiceIndex::default();
            voices.sync(BattleId::Root, game.attempt().unwrap());
            let at = [10.0, 0.5];
            let cases = [
                (
                    SimEvent::Boss {
                        enemy_id: id,
                        lane: None,
                        at,
                    },
                    Cue::BossArrival,
                    BossStage::Arrival,
                ),
                (
                    SimEvent::BossPhase {
                        boss_id: id,
                        kind,
                        phase: 1,
                        lane: None,
                        at,
                    },
                    Cue::BossPhase,
                    BossStage::Phase,
                ),
                // Last: the kill takes the boss out of the index.
                (kill(id), Cue::BossDeath, BossStage::Death),
            ];
            for (event, cue, stage) in cases {
                let (cues, places) = requests(&mut voices, &event, &ROOT);
                assert_eq!(cues, vec![cue], "{kind:?} {stage:?}");
                assert_eq!(
                    places.voicing(cue),
                    boss_voicing(kind, stage, capstone),
                    "{kind:?} {stage:?} capstone {capstone}"
                );
            }
        }
    }
    // No kind: the legacy boss and kill cues.
    let at = [10.0, 0.5];
    assert_eq!(
        plain(&SimEvent::Boss {
            enemy_id: 7,
            lane: None,
            at
        }),
        vec![Cue::Boss]
    );
    assert_eq!(plain(&kill(7)), vec![Cue::Kill]);
}

/// The index is not saved. After a load the first sync rebuilds it from the
/// attempt, so the next kill still dies in its own voice.
#[test]
fn kill_after_load_resolves_from_rebuilt_index() {
    let mut game = deployed(Mode::Personal);
    for _ in 0..900 {
        if game.attempt().unwrap().enemies.len() >= 2 {
            break;
        }
        game.tick(Input::default());
    }
    let attempt = game.attempt().unwrap();
    assert!(attempt.enemies.len() >= 2);
    // A fresh index, as after a load.
    let mut voices = EnemyVoiceIndex::default();
    assert_eq!(voices.len(BattleId::Root), 0);
    voices.sync(BattleId::Root, attempt);
    assert_eq!(voices.len(BattleId::Root), attempt.enemies.len());
    let enemy = &attempt.enemies[0];
    let voice = EnemyVoiceRef::of(enemy);
    assert_eq!(
        Some(voice.voice),
        voice_row(&enemy.resolved.as_ref().unwrap().archetype_id).map(|row| row.voice)
    );
    let (cues, places) = requests(&mut voices, &kill(enemy.id), &ROOT);
    assert_eq!(cues, vec![Cue::EnemyDeath]);
    assert_eq!(
        places.voicing(Cue::EnemyDeath),
        voice.voicing(EnemyStage::Death)
    );
    assert_eq!(
        places.voicing(Cue::EnemyDeath).bank,
        BankId::Enemy(voice.voice, EnemyStage::Death)
    );
    assert_eq!(
        places.of(Cue::EnemyDeath),
        Some(horde_audio::pan::lane_position(10.0, 30.0))
    );
    // The kill took it out; a second kill of the id resolves nothing.
    assert_eq!(voices.len(BattleId::Root), attempt.enemies.len() - 1);
    assert_eq!(plain(&kill(enemy.id)), vec![Cue::Kill]);
    // Another attempt under the same battle rebuilds the battle's voices.
    let mut other = deployed(Mode::Personal);
    other.staging().attempt().unwrap().id += 1;
    voices.sync(BattleId::Root, other.attempt().unwrap());
    assert_eq!(
        voices.len(BattleId::Root),
        other.attempt().unwrap().enemies.len()
    );
    assert_eq!(voices.get(BattleId::Root, attempt.enemies[1].id), None);
}

/// An enemy that spawned and died inside one tick was never indexed: its
/// corpse record gives the kind, and the kind the voice.
#[test]
fn same_tick_spawn_kill_resolves_from_corpse_kind() {
    let corpse = |id, kind, boss_kind| CorpseRecord {
        id,
        kind,
        boss_kind,
        at: [10.0, 0.5],
        visual: None,
        tick: 1,
    };
    let corpses = [
        corpse(41, EnemyKind::Brute, None),
        corpse(42, EnemyKind::Demolisher, Some(BossKind::SiegeBrute)),
    ];
    let ctx = EventContext {
        corpses: &corpses,
        ..ROOT
    };
    let mut voices = EnemyVoiceIndex::default();
    let (cues, places) = requests(&mut voices, &kill(41), &ctx);
    assert_eq!(cues, vec![Cue::EnemyDeath]);
    assert_eq!(
        places.voicing(Cue::EnemyDeath),
        Voicing::bank(BankId::Enemy(EnemyVoice::Brute, EnemyStage::Death))
    );
    // A boss's corpse keeps its kind.
    let (cues, places) = requests(&mut voices, &kill(42), &ctx);
    assert_eq!(cues, vec![Cue::BossDeath]);
    assert_eq!(
        places.voicing(Cue::BossDeath).bank,
        BankId::Boss(BossKind::SiegeBrute, BossStage::Death)
    );
    // No index entry and no corpse: the legacy kill.
    assert_eq!(requests(&mut voices, &kill(43), &ctx).0, vec![Cue::Kill]);
}

/// Every enemy death shares the kill's 160 ms gate and its voice cap, and a
/// death, like a kill, subsumes the impact that caused it.
#[test]
fn death_cues_share_kill_gate_and_subsume_impact() {
    assert_eq!(Cue::EnemyDeath.gate_key(), Cue::Kill);
    assert_eq!(mix(Cue::EnemyDeath).cooldown_ms, mix(Cue::Kill).cooldown_ms);
    assert_eq!(mix(Cue::Kill).cooldown_ms, 160);
    let mut gate = CueGate::default();
    assert!(gate.accept(Cue::Kill, 0, false));
    assert!(!gate.accept(Cue::EnemyDeath, 100, false));
    assert!(gate.accept(Cue::EnemyDeath, 160, false));
    assert!(!gate.accept(Cue::Kill, 300, false));
    assert!(gate.accept(Cue::Kill, 320, false));
    // An impact has its own gate.
    assert!(gate.accept(Cue::Impact, 320, false));
    assert_eq!(
        horde_audio::voices::family(Cue::EnemyDeath),
        horde_audio::voices::family(Cue::Kill)
    );
    assert!(SUBSUMES.contains(&(Cue::EnemyDeath, Cue::Impact)));
    assert!(SUBSUMES.contains(&(Cue::Kill, Cue::Impact)));
    assert_eq!(
        select_sounds(vec![Cue::Impact, Cue::EnemyDeath], 0),
        vec![Cue::EnemyDeath]
    );
    // The windup and contact gates of the design.
    assert_eq!(mix(Cue::EnemyWindup).cooldown_ms, 300);
    assert_eq!(mix(Cue::EnemyContact).cooldown_ms, 260);
    assert_eq!(mix(Cue::BossArrival).cooldown_ms, 3000);
}

/// A contact and a spit sound in the attacker's voice, where it stands.
#[test]
fn contact_and_spit_play_the_attackers_voice() {
    let mut game = deployed(Mode::Personal);
    for _ in 0..600 {
        if !game.attempt().unwrap().enemies.is_empty() {
            break;
        }
        game.tick(Input::default());
    }
    let attempt = game.attempt().unwrap();
    let enemy = &attempt.enemies[0];
    let voice = EnemyVoiceRef::of(enemy);
    let mut voices = EnemyVoiceIndex::default();
    voices.sync(BattleId::Root, attempt);
    for structure_id in [None, Some(9000)] {
        let contact = SimEvent::Contact {
            enemy_id: enemy.id,
            structure_id,
            lane: None,
            at: [3.0, 0.5],
        };
        let (cues, places) = requests(&mut voices, &contact, &ROOT);
        assert_eq!(cues, vec![Cue::EnemyContact]);
        assert_eq!(
            places.voicing(Cue::EnemyContact),
            voice.voicing(EnemyStage::Contact)
        );
        assert_eq!(
            places.of(Cue::EnemyContact),
            Some(horde_audio::pan::lane_position(3.0, 30.0))
        );
    }
    let spit = SimEvent::Spit(strike(Actor::Enemy { id: enemy.id }, None));
    let (cues, places) = requests(&mut voices, &spit, &ROOT);
    assert_eq!(cues, vec![Cue::EnemyContact]);
    assert_eq!(
        places.voicing(Cue::EnemyContact),
        voice.voicing(EnemyStage::Contact)
    );
    let wind = SimEvent::Windup {
        enemy_id: enemy.id,
        structure_id: None,
        lane: None,
        duration_ticks: 30,
        from: [8.0, 0.5],
        to: [0.5, 0.5],
    };
    let (_, places) = requests(&mut voices, &wind, &ROOT);
    assert_eq!(
        places.voicing(Cue::EnemyWindup),
        voice.voicing(EnemyStage::Windup)
    );
}

/// A trap's strike is the trap's snap, whatever came of it; the defender's
/// own defense is a block on the Armor bank, lower.
#[test]
fn trap_strikes_and_blocks() {
    let trap = Actor::Structure {
        id: 9000,
        kind: StructureKind::Trap,
    };
    assert_eq!(
        plain(&SimEvent::Miss(strike(trap, Some(7)))),
        vec![Cue::Trap]
    );
    assert_eq!(
        plain(&SimEvent::Defended(strike(trap, Some(7)))),
        vec![Cue::Trap]
    );
    assert_eq!(
        plain(&SimEvent::Hit(strike(trap, Some(7)))),
        vec![Cue::Trap, Cue::Impact]
    );
    // An enemy that defended the defender's strike is silent.
    assert_eq!(
        plain(&SimEvent::Defended(strike(DEFENDER, Some(7)))),
        vec![]
    );
    let (cues, places) = requests(
        &mut EnemyVoiceIndex::default(),
        &SimEvent::Defended(strike(ENEMY, None)),
        &ROOT,
    );
    assert_eq!(cues, vec![Cue::Block]);
    let voicing = places.voicing(Cue::Block);
    assert_eq!((voicing.bank, voicing.speed), (Cue::Armor.bank(), 0.85));
    assert!(mix(Cue::Block).gain < mix(Cue::Armor).gain);
    let (_, places) = requests(
        &mut EnemyVoiceIndex::default(),
        &SimEvent::Ability { healed: 0 },
        &ROOT,
    );
    let voicing = places.voicing(Cue::Ability);
    assert_eq!((voicing.bank, voicing.speed), (Cue::Ready.bank(), 0.92));
}
