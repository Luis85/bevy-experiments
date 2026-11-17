//! The gun tracker (A6-13): staged reloads, shell inserts, the bolt and the
//! dry trigger, read from the attempt after every tick. Every case runs at
//! speeds 1x and 4x (one and four ticks per frame).

use horde_audio::{
    bank::{BankId, GunStage},
    cue::Cue,
    events::{EventContext, event_requests},
    gun::{
        BOLT_DELAY_TICKS, DRY_FIRE_REPEAT_TICKS, GunTracker, MAX_SHELL_INSERTS,
        dry_fire_repeat_ticks, stage_speed,
    },
    mix::{Bus, GateGroup, JitterBounds, duck, mix},
    pan::{Places, positional},
    select::CueGate,
    voices::{Family, family},
    voicing::EnemyVoiceIndex,
};
use horde_core::{
    content::Mode,
    sim::{BattleId, Game, Input},
    weapons::WeaponClass,
};

const SPEEDS: [u32; 2] = [1, 4];
/// The cues this row owns, plus the reload's finishing stage.
const GUN_CUES: [Cue; 6] = [
    Cue::DryFire,
    Cue::ReloadOut,
    Cue::ReloadIn,
    Cue::ShellInsert,
    Cue::BoltCycle,
    Cue::Reload,
];

fn deployed(mode: Mode, class: WeaponClass) -> Game {
    let mut game = Game::default();
    game.set_seed(1);
    game.campaign_mut().attempts = 1;
    // An NPC defense needs the companion path; the mode alone is enough.
    game.deploy(0, Mode::Personal).unwrap();
    game.set_paused(false);
    let attempt = game.staging().attempt().unwrap();
    attempt.mode = mode;
    attempt.profile.weapon.class = class;
    attempt.profile.personal.reload = 2.0;
    game
}

/// A battle and its tracker, driven as the runner drives them: per tick the
/// events through `event_requests`, then the tracker; `speed` ticks share
/// one frame's cues and places.
struct Rig {
    game: Game,
    tracker: GunTracker,
    voices: EnemyVoiceIndex,
    speed: u32,
    /// The gun cues of every frame so far.
    frames: Vec<Vec<Cue>>,
    /// The places of the last frame.
    places: Places,
}
impl Rig {
    fn new(game: Game, speed: u32) -> Self {
        let mut rig = Self {
            game,
            tracker: GunTracker::default(),
            voices: EnemyVoiceIndex::default(),
            speed,
            frames: Vec::new(),
            places: Places::default(),
        };
        // The first observation primes the tracker.
        rig.frame(&Input::default());
        assert_eq!(rig.heard(), vec![]);
        rig
    }
    fn frame(&mut self, input: &Input) {
        let (mut cues, mut places) = (Vec::new(), Places::default());
        for _ in 0..self.speed {
            self.game.tick(input.clone());
            let records: Vec<_> = self.game.drain_events().collect();
            let Some(attempt) = self.game.attempt() else {
                continue;
            };
            self.voices.sync(BattleId::Root, attempt);
            let ctx = EventContext {
                battle: BattleId::Root,
                width_yards: Some(attempt.width_yards),
                distant: false,
                corpses: &attempt.corpses,
            };
            for record in records {
                event_requests(
                    &mut cues,
                    &mut places,
                    &mut self.voices,
                    &record.event,
                    &ctx,
                );
            }
            self.tracker
                .observe(BattleId::Root, attempt, input, &mut cues, &mut places);
        }
        cues.retain(|cue| GUN_CUES.contains(cue));
        self.frames.push(cues);
        self.places = places;
    }
    /// Frames until `ticks` more ticks have run.
    fn run(&mut self, input: &Input, ticks: u32) {
        for _ in 0..ticks.div_ceil(self.speed) {
            self.frame(input);
        }
    }
    /// Every gun cue heard so far, in order.
    fn heard(&self) -> Vec<Cue> {
        self.frames.iter().flatten().copied().collect()
    }
    fn count(&self, cue: Cue) -> usize {
        self.heard().iter().filter(|heard| **heard == cue).count()
    }
    fn attempt(&mut self) -> &mut horde_core::sim::Attempt {
        self.game.staging().attempt().unwrap()
    }
    /// One pressed reload, then the whole reload and a second on top.
    fn reload(&mut self) {
        let ticks = self.game.attempt().unwrap().profile.personal.reload * 60.0;
        self.frame(&Input {
            reload: true,
            ..Input::default()
        });
        self.run(&Input::default(), ticks as u32 + 60);
    }
}

#[test]
fn manual_reload_plays_out_in_finish_once() {
    for speed in SPEEDS {
        let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Rifle), speed);
        rig.attempt().ammo = 1;
        rig.reload();
        assert_eq!(
            rig.heard(),
            vec![Cue::ReloadOut, Cue::ReloadIn, Cue::Reload],
            "speed {speed}"
        );
        let attempt = rig.game.attempt().unwrap();
        assert_eq!(attempt.ammo, attempt.magazine);
        // The magazine goes in at the half of the reload.
        let frame_of = |cue: Cue| rig.frames.iter().position(|frame| frame.contains(&cue));
        let (out, r#in, finish) = (
            frame_of(Cue::ReloadOut).unwrap(),
            frame_of(Cue::ReloadIn).unwrap(),
            frame_of(Cue::Reload).unwrap(),
        );
        let half = 60 / speed as usize;
        assert!((r#in - out).abs_diff(half) <= 1, "speed {speed}");
        assert!((finish - r#in).abs_diff(half) <= 1, "speed {speed}");
    }
}

/// A melee strike cancels the reload: the stages heard so far stand and no
/// finish follows.
#[test]
fn canceled_reload_plays_no_later_stage() {
    for speed in SPEEDS {
        let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Rifle), speed);
        rig.attempt().ammo = 1;
        rig.frame(&Input {
            reload: true,
            ..Input::default()
        });
        rig.run(&Input::default(), 12);
        // What a melee strike does to a reload (`manual.rs`).
        rig.attempt().reload_left = 0;
        rig.run(&Input::default(), 200);
        assert_eq!(rig.heard(), vec![Cue::ReloadOut], "speed {speed}");
    }
}

#[test]
fn shotgun_reload_plays_n_shell_inserts_capped_6() {
    for speed in SPEEDS {
        for (magazine, ammo, shells) in [(8, 6, 2), (8, 3, 5), (8, 0, 6), (12, 1, 6)] {
            let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Shotgun), speed);
            // 210 ticks: at most seven parts of 30 ticks, a frame apart at 4x.
            rig.attempt().profile.personal.reload = 3.5;
            rig.attempt().magazine = magazine;
            rig.attempt().ammo = ammo;
            rig.reload();
            let mut expected = vec![Cue::ReloadOut];
            expected.extend(std::iter::repeat_n(Cue::ShellInsert, shells));
            expected.push(Cue::Reload);
            // Shells replace the magazine: a shotgun has no ReloadIn.
            assert_eq!(rig.heard(), expected, "speed {speed}, {ammo}/{magazine}");
            assert!(shells <= MAX_SHELL_INSERTS as usize);
        }
    }
}

#[test]
fn dry_trigger_auto_reload_off_plays_dry_fire_once_per_350ms() {
    let dry = Input {
        fire: true,
        auto_reload: false,
        ..Input::default()
    };
    assert_eq!(mix(Cue::DryFire).cooldown_ms, 350);
    assert_eq!(dry_fire_repeat_ticks(false), DRY_FIRE_REPEAT_TICKS);
    // Normal effects click every 400 ms; reduced effects every 3/2 of the
    // gate (525 ms) or a little more, never a request lost to the gate.
    for (reduced, gap) in [(false, 350..=400), (true, 525..=600)] {
        for speed in SPEEDS {
            let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Pistol), speed);
            rig.tracker.set_reduced_effects(reduced);
            rig.attempt().ammo = 0;
            rig.frame(&Input {
                auto_reload: false,
                ..Input::default()
            });
            let from = rig.frames.len();
            // Two seconds of real time with the trigger held.
            for _ in 0..120 {
                rig.frame(&dry);
            }
            let case = format!("speed {speed}, reduced {reduced}");
            assert_eq!(rig.count(Cue::DryFire), rig.heard().len(), "{case}");
            // The click is on the press itself.
            assert_eq!(rig.frames[from], vec![Cue::DryFire], "{case}");
            // The tracker asks once per repeat interval of simulated time ...
            let requests: Vec<usize> = (from..rig.frames.len())
                .filter(|frame| rig.frames[*frame].contains(&Cue::DryFire))
                .collect();
            let ticks = 120 * speed as u64;
            assert_eq!(
                requests.len() as u64,
                ticks.div_ceil(dry_fire_repeat_ticks(reduced)),
                "{case}"
            );
            // ... and the cue's gate lets one through per cooldown of real time.
            let mut gate = CueGate::default();
            let starts: Vec<u64> = requests
                .iter()
                .map(|frame| (frame - from) as u64 * 1000 / 60)
                .filter(|now_ms| gate.accept(Cue::DryFire, *now_ms, reduced))
                .collect();
            assert!(starts.len() >= 3 && starts.len() <= 6, "{case}: {starts:?}");
            assert!(
                starts
                    .windows(2)
                    .all(|pair| gap.contains(&(pair[1] - pair[0]))),
                "{case}: {starts:?}"
            );
            assert_eq!(starts[0], 0);
            // At 1x, the real cadence: every request starts.
            if speed == 1 {
                assert_eq!(starts.len(), requests.len(), "{case}: {starts:?}");
            }

            // Releasing and pressing again clicks again at once.
            rig.frame(&Input {
                auto_reload: false,
                ..Input::default()
            });
            assert_eq!(rig.frames.last().unwrap(), &vec![]);
            rig.frame(&dry);
            assert_eq!(rig.frames.last().unwrap(), &vec![Cue::DryFire]);

            // No dry fire with rounds left, with auto reload on (it reloads
            // instead), or in a melee commit.
            let heard = rig.count(Cue::DryFire);
            rig.attempt().melee_commit = 600;
            rig.frame(&Input {
                auto_reload: false,
                ..Input::default()
            });
            rig.run(&dry, 60);
            assert_eq!(rig.count(Cue::DryFire), heard, "speed {speed}: melee");
            rig.attempt().melee_commit = 0;
            rig.run(
                &Input {
                    fire: true,
                    ..Input::default()
                },
                8,
            );
            assert_eq!(rig.count(Cue::DryFire), heard, "speed {speed}: auto reload");
            assert_eq!(rig.count(Cue::ReloadOut), 1, "speed {speed}");
        }
    }
}

/// The last round out of the magazine is a shot, not a click, on its tick.
#[test]
fn the_emptying_shot_is_not_a_dry_fire() {
    let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Pistol), 1);
    rig.attempt().ammo = 1;
    rig.attempt().shot_cooldown = 0;
    rig.frame(&Input {
        fire: true,
        auto_reload: false,
        ..Input::default()
    });
    assert_eq!(rig.game.attempt().unwrap().ammo, 0);
    assert_eq!(rig.heard(), vec![]);
}

/// An NPC defender does not pull a dry trigger: the cue is the player's.
#[test]
fn npc_defender_never_dry_fires() {
    for speed in SPEEDS {
        let mut rig = Rig::new(deployed(Mode::Npc, WeaponClass::Rifle), speed);
        rig.attempt().ammo = 0;
        rig.run(
            &Input {
                fire: true,
                auto_reload: false,
                ..Input::default()
            },
            30,
        );
        assert_eq!(rig.count(Cue::DryFire), 0, "speed {speed}");
    }
}

/// An NPC with a one-turn reload refills without a `Reload` event
/// (`npc.rs`); the tracker hears the refill.
#[test]
fn npc_one_turn_refill_plays_reload() {
    for speed in SPEEDS {
        let mut rig = Rig::new(deployed(Mode::Npc, WeaponClass::Rifle), speed);
        // What the one-turn refill does: no reload time, the magazine full.
        rig.attempt().ammo = 0;
        rig.frame(&Input::default());
        let attempt = rig.attempt();
        attempt.reload_left = 0;
        attempt.ammo = attempt.magazine;
        rig.frame(&Input::default());
        assert_eq!(rig.frames.last().unwrap(), &vec![Cue::Reload], "{speed}");
        assert_eq!(rig.heard(), vec![Cue::Reload], "speed {speed}");
        // The same rise in Personal mode is not a reload (no such refill).
        let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Rifle), speed);
        rig.attempt().ammo = 1;
        rig.frame(&Input::default());
        let attempt = rig.attempt();
        attempt.ammo = attempt.magazine;
        rig.frame(&Input::default());
        assert_eq!(rig.heard(), vec![], "speed {speed}");
    }
}

/// The real NPC turn: a defender whose profile reloads in one turn.
#[test]
fn npc_one_turn_reload_through_the_core_plays_reload_once() {
    let mut rig = Rig::new(deployed(Mode::Npc, WeaponClass::Rifle), 1);
    rig.attempt().profile.npc.reload_turns = 1;
    rig.attempt().ammo = 0;
    // No enemy in reach: the NPC's turn reloads only when it has a target,
    // so run until the magazine is full again.
    let mut ticks = 0;
    while rig.game.attempt().is_some_and(|a| a.ammo == 0) && ticks < 3_600 {
        rig.frame(&Input::default());
        ticks += 1;
    }
    let attempt = rig.game.attempt().expect("the attempt outlives the reload");
    assert_eq!(attempt.ammo, attempt.magazine, "after {ticks} ticks");
    assert_eq!(rig.count(Cue::Reload), 1);
    assert_eq!(rig.count(Cue::ReloadOut), 0);
}

#[test]
fn loaded_mid_reload_emits_no_stage_burst() {
    for speed in SPEEDS {
        let mut game = deployed(Mode::Personal, WeaponClass::Shotgun);
        game.staging().attempt().unwrap().ammo = 0;
        game.tick(Input {
            reload: true,
            ..Input::default()
        });
        for _ in 0..20 {
            game.tick(Input::default());
        }
        assert!(game.attempt().unwrap().reload_left > 0);
        game.drain_events().for_each(drop);
        // A load: a new tracker meets the attempt mid-reload.
        let saved = serde_json::to_string(&game).unwrap();
        let loaded: Game = serde_json::from_str(&saved).unwrap();
        let mut rig = Rig::new(loaded, speed);
        rig.run(&Input::default(), 200);
        // Only the finish, which the core's own event announces.
        assert_eq!(rig.heard(), vec![Cue::Reload], "speed {speed}");

        // The same tracker meeting another attempt primes again.
        let mut other = deployed(Mode::Personal, WeaponClass::Shotgun);
        other.staging().attempt().unwrap().id += 1;
        other.staging().attempt().unwrap().reload_left = 90;
        let (mut cues, mut places) = (Vec::new(), Places::default());
        rig.tracker.observe(
            BattleId::Root,
            other.attempt().unwrap(),
            &Input::default(),
            &mut cues,
            &mut places,
        );
        assert_eq!(cues, vec![], "speed {speed}");
    }
}

#[test]
fn marksman_bolt_cycles_300ms_after_each_shot() {
    assert_eq!(BOLT_DELAY_TICKS, 18);
    for speed in SPEEDS {
        let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Marksman), speed);
        rig.attempt().shot_cooldown = 0;
        let from = rig.frames.len();
        rig.frame(&Input {
            fire: true,
            ..Input::default()
        });
        assert_eq!(rig.game.attempt().unwrap().shots, 1);
        rig.run(&Input::default(), 60);
        assert_eq!(rig.heard(), vec![Cue::BoltCycle], "speed {speed}");
        let at = rig
            .frames
            .iter()
            .position(|frame| frame.contains(&Cue::BoltCycle));
        let frames = (BOLT_DELAY_TICKS / speed) as usize;
        assert!((at.unwrap() - from).abs_diff(frames) <= 1, "speed {speed}");

        // No other class has a bolt.
        let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Rifle), speed);
        rig.attempt().shot_cooldown = 0;
        rig.frame(&Input {
            fire: true,
            ..Input::default()
        });
        assert_eq!(rig.game.attempt().unwrap().shots, 1);
        rig.run(&Input::default(), 60);
        assert_eq!(rig.heard(), vec![], "speed {speed}");
    }
}

/// The shot that empties the magazine goes into the reload, whose stages
/// chamber the next round: no bolt on top.
#[test]
fn a_reload_replaces_the_pending_bolt() {
    for speed in SPEEDS {
        let mut rig = Rig::new(deployed(Mode::Personal, WeaponClass::Marksman), speed);
        rig.attempt().shot_cooldown = 0;
        rig.attempt().ammo = 1;
        rig.frame(&Input {
            fire: true,
            ..Input::default()
        });
        rig.run(&Input::default(), 200);
        assert_eq!(
            rig.heard(),
            vec![Cue::ReloadOut, Cue::ReloadIn, Cue::Reload],
            "speed {speed}"
        );
    }
}

#[test]
fn stage_speed_follows_weapon_class() {
    let speeds = [
        (WeaponClass::Pistol, 1.08),
        (WeaponClass::Smg, 1.04),
        (WeaponClass::Rifle, 1.00),
        (WeaponClass::Marksman, 0.95),
        (WeaponClass::Shotgun, 0.92),
        (WeaponClass::MachineGun, 0.90),
    ];
    for speed in SPEEDS {
        for (class, expected) in speeds {
            assert_eq!(stage_speed(class), expected);
            let mut rig = Rig::new(deployed(Mode::Personal, class), speed);
            rig.attempt().ammo = 1;
            rig.frame(&Input {
                reload: true,
                ..Input::default()
            });
            let out = rig.places.voicing(Cue::ReloadOut);
            assert_eq!(out.bank, BankId::Gun(GunStage::ReloadOut), "{class:?}");
            assert_eq!((out.speed, out.gain_db), (expected, 0.0), "{class:?}");
            // The finishing stage is the v1 Reload bank at the class's speed.
            while rig.count(Cue::Reload) == 0 {
                rig.frame(&Input::default());
            }
            let finish = rig.places.voicing(Cue::Reload);
            assert_eq!(finish.bank, BankId::Legacy(Cue::Reload), "{class:?}");
            assert_eq!(finish.speed, expected, "{class:?}");
            // The dry trigger too.
            let mut rig = Rig::new(deployed(Mode::Personal, class), speed);
            rig.attempt().ammo = 0;
            rig.frame(&Input {
                auto_reload: false,
                ..Input::default()
            });
            rig.frame(&Input {
                fire: true,
                auto_reload: false,
                ..Input::default()
            });
            let click = rig.places.voicing(Cue::DryFire);
            assert_eq!(click.bank, BankId::Gun(GunStage::DryFire), "{class:?}");
            assert_eq!(click.speed, expected, "{class:?}");
        }
    }
}

#[test]
fn reduced_effects_keeps_dry_fire_and_bolt_at_reduced_gain() {
    // The tracker asks in either setting (the cases above; only the dry
    // trigger's repeat follows the longer gate), and the rows soften the
    // cues without removing them.
    for cue in [
        Cue::DryFire,
        Cue::BoltCycle,
        Cue::ShellInsert,
        Cue::ReloadOut,
        Cue::ReloadIn,
    ] {
        let row = mix(cue);
        assert!(!row.retired, "{cue:?}");
        assert_eq!(row.level(true), row.gain * 0.65, "{cue:?}");
        assert!(row.level(true) > 0.0, "{cue:?}");
        assert_eq!(row.cooldown(true), u64::from(row.cooldown_ms) * 3 / 2);
    }
    assert_eq!(mix(Cue::DryFire).cooldown(true), 525);
}

/// The rows of the gun-handling cues: linear gain, cooldown in ms, priority.
#[test]
fn gun_stage_rows() {
    let rows = [
        (Cue::DryFire, GunStage::DryFire, 0.26, 350, 6),
        (Cue::ReloadOut, GunStage::ReloadOut, 0.44, 300, 5),
        (Cue::ReloadIn, GunStage::ReloadIn, 0.44, 300, 5),
        (Cue::ShellInsert, GunStage::ShellInsert, 0.44, 80, 5),
        (Cue::BoltCycle, GunStage::BoltCycle, 0.44, 300, 6),
    ];
    assert_eq!(rows.map(|row| row.0)[..], Cue::ALL[37..42]);
    let db = |cue: Cue| 20.0 * (mix(cue).gain / mix(Cue::Reload).gain).log10();
    for (cue, stage, gain, cooldown_ms, priority) in rows {
        let row = mix(cue);
        assert_eq!(
            (row.gain, row.cooldown_ms, row.priority),
            (gain, cooldown_ms, priority),
            "{cue:?}"
        );
        assert_eq!(cue.bank(), BankId::Gun(stage), "{cue:?}");
        assert_eq!(cue.legacy_family(), Cue::Reload, "{cue:?}");
        assert_eq!(row.bus, Bus::Effects, "{cue:?}");
        assert_eq!(row.gate, GateGroup::General, "{cue:?}");
        assert_eq!(row.jitter, JitterBounds::GUNS, "{cue:?}");
        assert_eq!(row.warning, None, "{cue:?}");
        // The defender's own handling is centred and ducks nothing.
        assert!(!positional(cue), "{cue:?}");
        assert_eq!(duck(cue), None, "{cue:?}");
        assert!(!cue.retired());
    }
    // The design's table: the stages 2 dB under the Reload finish (-7.5
    // against -5.5), the dry trigger 6.5 dB under it (-12).
    for cue in [
        Cue::ReloadOut,
        Cue::ReloadIn,
        Cue::ShellInsert,
        Cue::BoltCycle,
    ] {
        assert!((db(cue) + 2.0).abs() < 0.1, "{cue:?}");
    }
    assert!((db(Cue::DryFire) + 6.5).abs() < 0.1);
    // One reload-stage voice at a time; the bolt and the click are their own.
    for cue in [Cue::Reload, Cue::ReloadOut, Cue::ReloadIn, Cue::ShellInsert] {
        assert_eq!(family(cue), Some(Family::Reload), "{cue:?}");
    }
    assert_eq!(Family::Reload.cap(), 1);
    assert_eq!(family(Cue::BoltCycle), None);
    assert_eq!(family(Cue::DryFire), None);
}

/// The tracker keeps one battle's gun per key and forgets the others.
#[test]
fn tracker_forgets_battles_that_are_not_kept() {
    let game = deployed(Mode::Personal, WeaponClass::Rifle);
    let mut tracker = GunTracker::default();
    let (mut cues, mut places) = (Vec::new(), Places::default());
    let attempt = game.attempt().unwrap();
    let input = Input::default();
    tracker.observe(BattleId::Root, attempt, &input, &mut cues, &mut places);
    assert!(tracker.tracks(BattleId::Root));
    tracker.retain(|battle| battle != BattleId::Root);
    assert!(!tracker.tracks(BattleId::Root));
    assert_eq!(cues, vec![]);
}
