//! Variety (A6-10): the per-bank round-robin that picks a start's variant and
//! the jitter of its speed and gain. Both come from presentation counters (a
//! bank's own trigger count), never from the simulation: the same bank plays
//! the same sequence whatever the battle's seed.

use horde_audio::{
    bank::BankId,
    cue::Cue,
    mix::{GateGroup, JitterBounds, Sliders, mix},
    render::{self, VoicePolicy, VoiceStart},
    select::{BANK_SLOTS, Jitter, RoundRobin, Variety, jitter, splitmix64},
};
use horde_core::{content::Mode, sim::Game};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const TRIGGERS: usize = 1_000;

/// No immediate repeat, and every variant plays at least half its even share.
fn assert_varied(label: &str, variants: u64, picks: &[u64]) {
    assert_eq!(picks.len(), TRIGGERS, "{label}");
    assert!(variants >= 2, "{label} has {variants} variant(s)");
    assert!(picks.iter().all(|pick| *pick < variants), "{label}");
    if let Some(at) = picks.windows(2).position(|pair| pair[0] == pair[1]) {
        panic!(
            "{label} repeats variant {} at trigger {}",
            picks[at],
            at + 1
        );
    }
    let floor = TRIGGERS / 2 / variants as usize;
    for variant in 0..variants {
        let used = picks.iter().filter(|pick| **pick == variant).count();
        assert!(
            used >= floor,
            "{label} variant {variant}: {used} of {TRIGGERS}"
        );
    }
}

/// G8.2: no bank plays the same variant twice in a row in 1,000 triggers,
/// with every bank triggered in turn, and the Smg end to end through the
/// policy at its gate cadence.
#[test]
fn round_robin_never_repeats_in_1000_triggers() {
    let banks = BankId::shipped();
    assert!(banks.len() > 24, "{}", banks.len());
    let mut variety = Variety::default();
    let mut picks: Vec<Vec<u64>> = vec![Vec::new(); banks.len()];
    for _ in 0..TRIGGERS {
        for (bank, picks) in banks.iter().zip(&mut picks) {
            picks.push(variety.pick(*bank, JitterBounds::NONE).variant);
        }
    }
    for (bank, picks) in banks.iter().zip(&picks) {
        assert_varied(&format!("{bank:?}"), bank.variants(), picks);
    }
    // Banks do not march in step: two five-variant banks differ.
    let five: Vec<&Vec<u64>> = banks
        .iter()
        .zip(&picks)
        .filter(|(bank, _)| bank.variants() == 5)
        .map(|(_, picks)| picks)
        .collect();
    assert_ne!(five[0], five[1]);

    // End to end: 1,000 Smg requests, one per 55 ms cooldown.
    let audible = Sliders::MAX.bus_gains();
    let cooldown = mix(Cue::Smg).cooldown(false);
    assert_eq!(cooldown, 55);
    let mut policy = VoicePolicy::default();
    let starts: Vec<VoiceStart> = (0..TRIGGERS as u64)
        .flat_map(|n| {
            policy
                .frame(vec![Cue::Smg], n * cooldown, false, &audible, &[])
                .starts
        })
        .collect();
    let played: Vec<u64> = starts.iter().map(|start| start.variant).collect();
    assert_varied(
        "Smg through the policy",
        Cue::Smg.bank().variants(),
        &played,
    );
    // The policy plays the bank's own sequence.
    let mut alone = Variety::default();
    let expected: Vec<u64> = (0..TRIGGERS)
        .map(|_| alone.pick_cue(Cue::Smg).variant)
        .collect();
    assert_eq!(played, expected);
}

#[test]
fn round_robin_formula_and_first_pick() {
    // After load there is no last variant: the first pick is `h % n`.
    for h in 0..40_u64 {
        let mut fresh = RoundRobin::default();
        assert_eq!(fresh.next(5, h), h % 5);
        // Then `next = (last + 1 + h % (n - 1)) % n`, which never equals `last`.
        let last = h % 5;
        for h2 in 0..40_u64 {
            let mut bank = fresh;
            let next = bank.next(5, h2);
            assert_eq!(next, (last + 1 + h2 % 4) % 5);
            assert_ne!(next, last);
        }
    }
    // One variant is all a one-variant bank can play; two alternate.
    let mut single = RoundRobin::default();
    assert_eq!([single.next(1, 7), single.next(1, 8)], [0, 0]);
    let mut pair = RoundRobin::default();
    let first = pair.next(2, 3);
    assert_eq!([pair.next(2, 9), pair.next(2, 4)], [1 - first, first]);
    // The hash is SplitMix64 (its published first output for state 0), keyed
    // by the bank's ordinal and its own trigger count.
    assert_eq!(splitmix64(0), 0xe220_a839_7b1d_cdaf);
    let bank = Cue::Impact.bank();
    let mut variety = Variety::default();
    let first = variety.pick(bank, JitterBounds::IMPACTS);
    let h = splitmix64(bank.ordinal() << 32);
    assert_eq!(first.variant, h % bank.variants());
    assert_eq!(first.jitter, jitter(JitterBounds::IMPACTS, h));
    let second = variety.pick(bank, JitterBounds::IMPACTS);
    assert_eq!(
        second.jitter,
        jitter(
            JitterBounds::IMPACTS,
            splitmix64((bank.ordinal() << 32) ^ 1)
        )
    );
}

/// Every shipped bank has its own round-robin slot.
#[test]
fn every_bank_has_its_own_slot() {
    let mut seen = [false; BANK_SLOTS];
    for bank in BankId::shipped() {
        let slot = bank.ordinal() as usize;
        assert!(slot < BANK_SLOTS, "{bank:?}");
        assert!(!std::mem::replace(&mut seen[slot], true), "{bank:?}");
    }
    for cue in Cue::ALL {
        assert!((cue.bank().ordinal() as usize) < BANK_SLOTS, "{cue:?}");
    }
}

fn db(gain: f32) -> f32 {
    20.0 * gain.log10()
}

/// Jitter stays inside its group's bounds, uses the range, and a group
/// without jitter plays untouched.
#[test]
fn jitter_bounded() {
    let groups = [
        ("guns", JitterBounds::GUNS, 0.02, 0.75),
        ("enemy vocals", JitterBounds::ENEMY_VOCALS, 0.04, 1.0),
        ("impacts and deaths", JitterBounds::IMPACTS, 0.03, 1.0),
        ("boss", JitterBounds::BOSS, 0.01, 0.0),
        ("ui", JitterBounds::UI, 0.0, 0.3),
        ("stingers", JitterBounds::NONE, 0.0, 0.0),
    ];
    for (label, bounds, speed, gain_db) in groups {
        assert_eq!((bounds.speed, bounds.gain_db), (speed, gain_db), "{label}");
        let mut variety = Variety::default();
        let (mut speeds, mut gains) = ([f32::MAX, f32::MIN], [f32::MAX, f32::MIN]);
        for bank in BankId::shipped() {
            for _ in 0..200 {
                let Jitter { speed: s, gain: g } = variety.pick(bank, bounds).jitter;
                assert!(
                    (1.0 - speed..=1.0 + speed).contains(&s),
                    "{label}: speed {s}"
                );
                assert!(db(g).abs() <= gain_db + 1e-4, "{label}: gain {} dB", db(g));
                speeds = [speeds[0].min(s), speeds[1].max(s)];
                gains = [gains[0].min(db(g)), gains[1].max(db(g))];
            }
        }
        if speed == 0.0 {
            assert_eq!(speeds, [1.0, 1.0], "{label}");
        } else {
            assert!(
                speeds[0] < 1.0 - 0.9 * speed && speeds[1] > 1.0 + 0.9 * speed,
                "{label}: {speeds:?}"
            );
        }
        if gain_db == 0.0 {
            assert_eq!(gains, [0.0, 0.0], "{label}");
        } else {
            assert!(
                gains[0] < -0.9 * gain_db && gains[1] > 0.9 * gain_db,
                "{label}: {gains:?}"
            );
        }
    }
    assert_eq!(
        jitter(JitterBounds::NONE, u64::MAX),
        Jitter {
            speed: 1.0,
            gain: 1.0
        }
    );
    assert_eq!(Jitter::NONE, jitter(JitterBounds::NONE, 12345));
    // Speed and gain move independently.
    let mut variety = Variety::default();
    let picks: Vec<Jitter> = (0..200)
        .map(|_| variety.pick_cue(Cue::Impact).jitter)
        .collect();
    assert!(picks.iter().any(|j| j.speed > 1.0 && j.gain < 1.0));
    assert!(picks.iter().any(|j| j.speed > 1.0 && j.gain > 1.0));
    assert!(picks.iter().any(|j| j.speed < 1.0 && j.gain < 1.0));
    assert!(picks.iter().any(|j| j.speed < 1.0 && j.gain > 1.0));
}

/// The mix rows name the jitter group of the design's table.
#[test]
fn jitter_rows_follow_the_group_table() {
    for cue in Cue::ALL {
        let row = mix(cue);
        let expected = match cue {
            _ if row.gate == GateGroup::Fire => JitterBounds::GUNS,
            // Handling a weapon varies like firing it.
            Cue::Reload | Cue::Throw => JitterBounds::GUNS,
            Cue::Impact
            | Cue::Armor
            | Cue::Kill
            | Cue::Damage
            | Cue::Melee
            | Cue::Explosion
            | Cue::Molotov
            | Cue::Structure
            | Cue::Trap => JitterBounds::IMPACTS,
            Cue::Boss => JitterBounds::BOSS,
            Cue::Confirm => JitterBounds::UI,
            // Stingers, the radio warnings, the ready chirp and the fire bed
            // keep their pitch and level.
            _ => JitterBounds::NONE,
        };
        assert_eq!(row.jitter, expected, "{cue:?}");
    }
    for stinger in [Cue::Victory, Cue::Defeat, Cue::Retreat] {
        assert_eq!(mix(stinger).jitter, JitterBounds::NONE);
    }
}

/// The policy hands the executor the bank's pick, in normal and reduced
/// effects alike (jitter adds no intensity), and leaves the row gain alone.
#[test]
fn policy_starts_carry_the_banks_pick() {
    let audible = Sliders::MAX.bus_gains();
    for reduced in [false, true] {
        let mut policy = VoicePolicy::default();
        let mut variety = Variety::default();
        for n in 0..50_u64 {
            let actions = policy.frame(
                vec![Cue::Explosion, Cue::Victory],
                n * 5_000,
                reduced,
                &audible,
                &[],
            );
            assert_eq!(actions.starts.len(), 2);
            for start in actions.starts {
                let pick = variety.pick_cue(start.cue);
                assert_eq!((start.variant, start.jitter), (pick.variant, pick.jitter));
                assert_eq!(start.gain, mix(start.cue).level(reduced));
                if start.cue == Cue::Victory {
                    assert_eq!(start.jitter, Jitter::NONE);
                }
            }
        }
    }
    // A cue the gate holds back does not advance its bank.
    let mut policy = VoicePolicy::default();
    let first = policy.frame(vec![Cue::Explosion], 0, false, &audible, &[]);
    assert!(
        policy
            .frame(vec![Cue::Explosion], 10, false, &audible, &[])
            .starts
            .is_empty()
    );
    let second = policy.frame(vec![Cue::Explosion], 5_000, false, &audible, &[]);
    let mut variety = Variety::default();
    assert_eq!(
        [first.starts[0].variant, second.starts[0].variant],
        [
            variety.pick_cue(Cue::Explosion).variant,
            variety.pick_cue(Cue::Explosion).variant
        ]
    );
}

/// A pitched clip is the clip read at `speed`: shorter when faster, never
/// louder, and the clip itself at speed 1.
#[test]
fn pitched_clip_is_resampled_by_its_speed() {
    let clip: Vec<f32> = (0..1_000).map(|n| (n as f32 * 0.05).sin() * 0.5).collect();
    assert_eq!(render::pitched(&clip, 1.0), clip);
    let fast = render::pitched(&clip, 1.04);
    let slow = render::pitched(&clip, 0.96);
    assert_eq!(fast.len(), 961);
    assert_eq!(slow.len(), 1_041);
    let peak = |samples: &[f32]| samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
    assert!(peak(&fast) <= peak(&clip) && peak(&slow) <= peak(&clip));
    // Sample 500 of the fast clip is the source at 520.
    assert!((fast[500] - clip[520]).abs() < 1e-6);
    assert!(render::pitched(&[], 1.02).is_empty());
}

fn battle(seed: u64) -> Game {
    let mut game = Game::default();
    game.set_seed(seed);
    game.campaign_mut().attempts = 1;
    game.deploy(0, Mode::Personal).unwrap();
    game.set_paused(false);
    game
}

/// The starts of a battle's first 40 s through the policy, per bank.
fn plans(seed: u64) -> (Vec<Vec<Cue>>, BTreeMap<u64, Vec<(u64, Jitter)>>) {
    let frames = render::battle_frames(&mut battle(seed), 2_400);
    let audible = Sliders::MAX.bus_gains();
    let mut policy = VoicePolicy::default();
    let mut plans: BTreeMap<u64, Vec<(u64, Jitter)>> = BTreeMap::new();
    for (index, frame) in frames.iter().enumerate() {
        let now_ms = index as u64 * 1000 / 60;
        let actions = policy.frame_requests(
            frame.cues.clone(),
            frame.places.player_shot(),
            now_ms,
            false,
            &audible,
            &[],
        );
        for start in actions.starts {
            plans
                .entry(start.cue.bank().ordinal())
                .or_default()
                .push((start.variant, start.jitter));
        }
    }
    (frames.into_iter().map(|frame| frame.cues).collect(), plans)
}

/// Two battles with different seeds request different cues, and every bank
/// still plays the same variants at the same jitter, start for start: the
/// plan reads the bank's trigger count and nothing of the game.
#[test]
fn plans_independent_of_game_seed() {
    let (cues_a, plans_a) = plans(1);
    let (cues_b, plans_b) = plans(2);
    assert_ne!(cues_a, cues_b, "the two seeds play the same battle");
    let mut compared = 0;
    for (bank, a) in &plans_a {
        let Some(b) = plans_b.get(bank) else { continue };
        let shared = a.len().min(b.len());
        assert_eq!(a[..shared], b[..shared], "bank {bank}");
        compared += shared;
    }
    assert!(compared >= 50, "{compared} starts compared");
}

// ----- structure -----

fn source_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// What reaches the simulation's randomness or a clock-seeded generator.
const RNG_NEEDLES: &[&str] = &[
    ".rng",
    "rng(",
    "Rng",
    "hash::keyed",
    "set_seed",
    "rand::",
    "getrandom",
    "RandomState",
    "catalog().roll(",
    ".roll_relative(",
    "drop_quality(",
];
/// The modules that decide variety may not name the core crate at all.
const CORE_FREE: &[&str] = &["select.rs", "mix.rs", "voices.rs", "pan.rs"];

/// `text` without its `//` comments (no string in this crate holds `//`
/// followed by a needle).
fn code(text: &str) -> String {
    text.lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn scan(path: &str, text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (number, line) in code(text).lines().enumerate() {
        for needle in RNG_NEEDLES {
            if line.contains(needle) {
                hits.push(format!("{path}:{}: {needle}", number + 1));
            }
        }
        if CORE_FREE.contains(&path) && line.contains("horde_core") {
            hits.push(format!("{path}:{}: horde_core", number + 1));
        }
    }
    hits
}

fn sources(directory: &Path, prefix: &str, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}{name}");
        if entry.file_type().unwrap().is_dir() {
            sources(&entry.path(), &format!("{path}/"), out);
        } else if name.ends_with(".rs") {
            out.push((path, std::fs::read_to_string(entry.path()).unwrap()));
        }
    }
}

/// ISO-8: nothing in `horde_audio` reads the core RNG, a keyed core hash or
/// any other generator; variety has only its own counters.
#[test]
fn horde_audio_never_reads_core_rng() {
    // The scanner catches planted reads, so a clean scan means something.
    for (path, text, needle) in [
        (
            "render.rs",
            "let n = game.attempt().unwrap().rng.below(3);\n",
            ".rng",
        ),
        (
            "select.rs",
            "let h = horde_core::hash::keyed(seed, a, b, TAG);\n",
            "hash::keyed",
        ),
        ("select.rs", "use horde_core::sim::Game;\n", "horde_core"),
        (
            "synth/family.rs",
            "let x = rand::random::<u64>();\n",
            "rand::",
        ),
        ("render.rs", "game.set_seed(7);\n", "set_seed"),
    ] {
        let hits = scan(path, text);
        assert!(
            hits.iter()
                .any(|hit| hit.starts_with(&format!("{path}:1:")) && hit.ends_with(needle)),
            "the scanner missed a planted `{needle}`: {hits:?}"
        );
    }
    assert!(scan("select.rs", "// reads no core RNG: .rng, rand::\n").is_empty());
    assert!(scan("render.rs", "use horde_core::sim::Game;\n").is_empty());

    let mut files = Vec::new();
    sources(&source_directory(), "", &mut files);
    for expected in ["select.rs", "render.rs", "mix.rs", "synth/family.rs"] {
        assert!(
            files.iter().any(|(path, _)| path == expected),
            "the scan never read {expected}"
        );
    }
    let hits: Vec<String> = files
        .iter()
        .flat_map(|(path, text)| scan(path, text))
        .collect();
    assert!(
        hits.is_empty(),
        "horde_audio reaches core randomness:\n{}",
        hits.join("\n")
    );
    // The manifest adds no generator crate either.
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    for line in code(&manifest.replace('#', "//")).lines() {
        assert!(
            !line.starts_with("rand")
                && !line.starts_with("getrandom")
                && !line.starts_with("fastrand"),
            "{line}"
        );
    }
}
