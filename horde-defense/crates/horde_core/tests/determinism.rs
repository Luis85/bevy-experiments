#![allow(clippy::print_stdout)] // prints digests for re-blessing
//! Golden determinism: fixed seeds driven through the headless bot must reach
//! exactly the pinned `Game::state_digest()` after a fixed number of ticks.
//!
//! The digest hashes the canonical serialized save payload, so ANY change to
//! gameplay outcomes, RNG consumption, or the serialized shape of the game
//! changes it. A failure here means one of those happened.
//!
//! Intentionally changing outcomes (or the save format) is a rebaseline and
//! needs explicit approval. The tooling:
//!
//! - `cargo test -p horde_core --test determinism -- --nocapture` prints the
//!   six `GOLDEN` lines, the six `GAMEPLAY_TRACE` lines and the six outcome
//!   summaries (kills, coins, hp, loot, spawned and `rng.state` at ticks 600,
//!   1200 and 1800) of the current build.
//! - `cargo test -p horde_core --test determinism -- --ignored --nocapture
//!   determinism_anatomy` prints each run's anatomy (golden, trace, key-path,
//!   item, telemetry-stripped and outcome hashes). Run it on the build before
//!   and after a move; `support::classify` labels each move F (format),
//!   C (content), T (telemetry and receipts) or G (gameplay).
//!
//! **Provisional-pin ledger (ruling P3).** A commit that moves a digest does
//! not edit `GOLDEN` or `GAMEPLAY_TRACE`. It sets `PROVISIONAL` to the six new
//! digests of each table (GOLDEN order) and appends its cause to
//! `PROVISIONAL_CAUSES` (commit tag, class, reason), so every commit stays
//! green and the cause is recorded where it happens. The tests then pin the
//! provisional digests, and they accept `PROVISIONAL` only with causes. The
//! wave's dedicated save-determinism re-bless commit folds `PROVISIONAL` into
//! `GOLDEN` and `GAMEPLAY_TRACE`, adds one row per moved digest to the log
//! below, and resets `PROVISIONAL` to None and `PROVISIONAL_CAUSES` to empty.
//! The wave-close critic checks that `PROVISIONAL` is None.
//!
//! `TRACE_RULESET` must equal `RULESET_VERSION`: a ruleset bump states in the
//! same commit what it does to `GAMEPLAY_TRACE`.
//!
//! Save v5 re-bless (format-only, approved): the ordnance counters became
//! kind-keyed maps, which changes the serialized payload of every run but no
//! gameplay; `GAMEPLAY_TRACE` pins the unchanged outcomes of the same runs.
//!
//! Save v6 re-bless (format-only): level progress and level references are
//! keyed by stable level ID; `GAMEPLAY_TRACE` is again unchanged.
//!
//! Re-bless log:
//!
//! | wave | commit | seed/mode | GOLDEN old->new | TRACE old->new | class | cause |
//! |---|---|---|---|---|---|---|
//! | pre-pass | 9b6454e | 0x1 Personal | 0x0b18ef9e9f3fdb8f->0x2e203b0ecbb0a2df | pinned 0xa6caa6de7fd92b1a | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0x1 Npc | 0x13e5aa1af9024db9->0x666dd9785ba38601 | pinned 0x296610f8ded8a91f | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0x1a571167 Personal | 0x2175368a9d8d176a->0x4da12071d6c2a94a | pinned 0x8beb3e2e0553347a | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0x1a571167 Npc | 0x8441e44894115e96->0xa0d031d13edd1996 | pinned 0x85ec9e5263ea48af | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0xdeadbeef2026 Personal | 0x65799cfc9dd3de72->0x9900ad25467328e2 | pinned 0x46a2adc6720f8678 | F | save v5 ordnance kind maps |
//! | pre-pass | 9b6454e | 0xdeadbeef2026 Npc | 0x57c75e07a11dc660->0x72f7f039a9dd7280 | pinned 0x1fda7a0649d44756 | F | save v5 ordnance kind maps |
//! | pre-pass | 8103a97 | 0x1 Personal | 0x2e203b0ecbb0a2df->0x303ea36fac6a070b | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0x1 Npc | 0x666dd9785ba38601->0xeede2391290bb381 | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0x1a571167 Personal | 0x4da12071d6c2a94a->0x3be131386490e3b2 | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0x1a571167 Npc | 0xa0d031d13edd1996->0x6a2ba00f7e4018d8 | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0xdeadbeef2026 Personal | 0x9900ad25467328e2->0x5aaf1f080eb1da92 | unchanged | F | save v6 level IDs |
//! | pre-pass | 8103a97 | 0xdeadbeef2026 Npc | 0x72f7f039a9dd7280->0xc1bb9ef207925afc | unchanged | F | save v6 level IDs |
mod support;
use horde_core::content::Mode;
use horde_core::sim::{Game, RULESET_VERSION};
use support::{Anatomy, MoveClass, OutcomeSummary, advance, smoke_run};

const TICKS: u32 = 1800;
const SEEDS: [u64; 3] = [1, 0x1A57_1167, 0xDEAD_BEEF_2026];

/// `(seed, mode)` -> digest after [`TICKS`] ticks of the headless bot.
/// Re-blessed for save v6: level progress (`first_clears`, NPC claims) is
/// serialized as level-ID sets/maps, the redundant `unlocked` frontier is no
/// longer saved and attempt/report levels serialize as level IDs. The payload
/// of every run changed, no gameplay did: `GAMEPLAY_TRACE` is unchanged.
const GOLDEN: [(u64, Mode, u64); 6] = [
    (1, Mode::Personal, 0x303e_a36f_ac6a_070b),
    (1, Mode::Npc, 0xeede_2391_290b_b381),
    (0x1A57_1167, Mode::Personal, 0x3be1_3138_6490_e3b2),
    (0x1A57_1167, Mode::Npc, 0x6a2b_a00f_7e40_18d8),
    (0xDEAD_BEEF_2026, Mode::Personal, 0x5aaf_1f08_0eb1_da92),
    (0xDEAD_BEEF_2026, Mode::Npc, 0xc1bb_9ef2_0792_5afc),
];

/// `(seed, mode)` -> FNV-1a of the per-tick gameplay trace (tick, hp, coins,
/// enemy ids/hp/positions and the event queue; no serialized format) recorded
/// on the v4 build (commit c6f518d) before the ordnance arrays became kind
/// maps. The save-v5 and save-v6 re-blesses of `GOLDEN` are format-only: this trace proves
/// the gameplay outcomes of the same six runs did not move.
const GAMEPLAY_TRACE: [(u64, Mode, u64); 6] = [
    (1, Mode::Personal, 0xa6ca_a6de_7fd9_2b1a),
    (1, Mode::Npc, 0x2966_10f8_ded8_a91f),
    (0x1A57_1167, Mode::Personal, 0x8beb_3e2e_0553_347a),
    (0x1A57_1167, Mode::Npc, 0x85ec_9e52_63ea_48af),
    (0xDEAD_BEEF_2026, Mode::Personal, 0x46a2_adc6_720f_8678),
    (0xDEAD_BEEF_2026, Mode::Npc, 0x1fda_7a06_49d4_4756),
];

/// The ruleset `GAMEPLAY_TRACE` was recorded under. It follows
/// `RULESET_VERSION` in the commits that bump it (ruling P4).
const TRACE_RULESET: u32 = 1;

/// Provisional `(GOLDEN, GAMEPLAY_TRACE)` digests, in `GOLDEN` order, set by
/// a digest-moving commit until the wave's re-bless folds them (ruling P3).
const PROVISIONAL: Option<([u64; 6], [u64; 6])> = None;

/// Why `PROVISIONAL` moved: `(commit tag, class, reason)`, one per moving
/// commit. Empty exactly when `PROVISIONAL` is None.
const PROVISIONAL_CAUSES: &[(&str, MoveClass, &str)] = &[];

/// The ledger rules: provisional digests carry causes, every cause names its
/// commit and reason, and causes never outlive the provisional pin.
fn check_ledger(
    provisional: Option<([u64; 6], [u64; 6])>,
    causes: &[(&str, MoveClass, &str)],
) -> Result<(), String> {
    for (tag, class, reason) in causes {
        if tag.trim().is_empty() || reason.trim().is_empty() {
            return Err(format!(
                "PROVISIONAL_CAUSES entry {tag:?} {class:?} {reason:?} needs a commit tag and a reason"
            ));
        }
    }
    match provisional {
        None if causes.is_empty() => Ok(()),
        None => Err(
            "PROVISIONAL is None but PROVISIONAL_CAUSES is not empty: the re-bless that \
             folded the pins moves the causes into the re-bless log"
                .into(),
        ),
        Some(_) if causes.is_empty() => Err(
            "PROVISIONAL is set without a cause: append (commit tag, class, reason) to \
             PROVISIONAL_CAUSES"
                .into(),
        ),
        Some((golden, trace)) => {
            if golden == GOLDEN.map(|(_, _, d)| d) && trace == GAMEPLAY_TRACE.map(|(_, _, d)| d) {
                Err("PROVISIONAL equals GOLDEN and GAMEPLAY_TRACE: reset it to None".into())
            } else {
                Ok(())
            }
        }
    }
}

/// The digests the tests pin: `PROVISIONAL` when set, the tables otherwise.
fn pinned(table: &[(u64, Mode, u64); 6], provisional: Option<[u64; 6]>) -> [(u64, Mode, u64); 6] {
    let mut pins = *table;
    if let Some(digests) = provisional {
        for (pin, digest) in pins.iter_mut().zip(digests) {
            pin.2 = digest;
        }
    }
    pins
}

fn run(seed: u64, mode: Mode) -> Game {
    smoke_run(seed, mode, TICKS, |_| {})
}

/// One tick of the gameplay trace: tick, hp, coins, enemy ids/hp/positions
/// and the event queue.
fn trace_line(g: &Game) -> String {
    let a = g.attempt().unwrap();
    format!(
        "t{} hp{} coins{} e{:?} ev{:?}\n",
        a.tick,
        a.hp,
        a.coins,
        a.enemies
            .iter()
            .map(|e| (e.id, e.hp, e.x.to_bits(), e.y().to_bits()))
            .collect::<Vec<_>>(),
        a.events
    )
}

/// The smoke run of `seed` and `mode` with its gameplay trace hash and
/// outcome summary, from one pass.
fn observed_run(seed: u64, mode: Mode) -> (Game, u64, OutcomeSummary) {
    let mut hash = support::fnv1a(b"");
    let mut summary = OutcomeSummary::default();
    let g = smoke_run(seed, mode, TICKS, |g| {
        hash = support::fnv1a_extend(hash, trace_line(g).as_bytes());
        summary.observe(g);
    });
    (g, hash, summary)
}

#[test]
fn seeded_headless_runs_keep_their_pinned_gameplay_traces() {
    let mut mismatches = Vec::new();
    for (seed, mode, expected) in pinned(&GAMEPLAY_TRACE, PROVISIONAL.map(|p| p.1)) {
        let (_, hash, summary) = observed_run(seed, mode);
        println!("({seed:#x}, Mode::{mode:?}, {hash:#018x}),");
        println!("// {seed:#x} {mode:?}: {summary}");
        if hash != expected {
            mismatches.push(format!(
                "seed {seed:#x} {mode:?} gameplay moved: expected {expected:#018x}, \
                 got {hash:#018x}\n  outcome now: {summary}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "Gameplay diverged from the pinned traces:\n{}\n\n\
         If this change is intentional, set PROVISIONAL and append its cause to \
         PROVISIONAL_CAUSES in tests/determinism.rs (see the file header).",
        mismatches.join("\n")
    );
}

#[test]
fn seeded_headless_runs_reach_the_pinned_state_digests() {
    let mut mismatches = Vec::new();
    for (seed, mode, expected) in pinned(&GOLDEN, PROVISIONAL.map(|p| p.0)) {
        let g = run(seed, mode);
        let digest = g.state_digest();
        println!("({seed:#x}, Mode::{mode:?}, {digest:#018x}),");
        if digest != expected {
            mismatches.push(format!(
                "seed {seed:#x} {mode:?}: expected {expected:#018x}, got {digest:#018x}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "Simulation state diverged from the pinned determinism digests:\n{}\n\n\
         If this change is intentional, set PROVISIONAL and append its cause to \
         PROVISIONAL_CAUSES in tests/determinism.rs (see the file header). \
         Rebaselining gameplay requires explicit approval.",
        mismatches.join("\n")
    );
}

#[test]
fn gameplay_trace_is_recorded_under_the_current_ruleset() {
    assert_eq!(
        TRACE_RULESET, RULESET_VERSION,
        "RULESET_VERSION moved: state what the bump does to GAMEPLAY_TRACE and \
         move TRACE_RULESET in the same commit"
    );
}

#[test]
fn provisional_digests_name_their_causes() {
    check_ledger(PROVISIONAL, PROVISIONAL_CAUSES).unwrap();

    let golden = GOLDEN.map(|(_, _, d)| d);
    let trace = GAMEPLAY_TRACE.map(|(_, _, d)| d);
    let mut moved = golden;
    moved[0] ^= 1;
    let cause = [("W9-99", MoveClass::F, "a planted format move")];
    assert!(check_ledger(None, &[]).is_ok());
    assert!(check_ledger(Some((moved, trace)), &cause).is_ok());
    assert!(
        check_ledger(Some((moved, trace)), &[]).is_err(),
        "provisional digests without a cause"
    );
    assert!(
        check_ledger(None, &cause).is_err(),
        "a cause left behind after the re-bless"
    );
    assert!(
        check_ledger(Some((golden, trace)), &cause).is_err(),
        "a provisional pin that moves nothing"
    );
    for blank in [("", MoveClass::F, "a reason"), ("W9-99", MoveClass::F, " ")] {
        assert!(
            check_ledger(Some((moved, trace)), &[blank]).is_err(),
            "a cause without a commit tag or reason: {blank:?}"
        );
    }
}

/// A small payload with the real shape: campaign items, a receipt list and an
/// attempt with telemetry.
fn planted_payload() -> serde_json::Value {
    serde_json::json!({
        "campaign": {
            "coins": 10,
            "items": [
                {"id": 1, "definition": "pistol", "quality": null, "level": 1, "locked": false},
                {"id": 2, "definition": "bat", "quality": "Rare", "level": 2, "locked": true},
            ],
            "run_history": [{"id": 1, "kills": 3}],
            "first_clear_claimed": {"level-1": 1},
        },
        "attempt": {
            "hp": 30,
            "telemetry": {"misses": 2, "kills_by_kind": {"Shambler": 3}},
        },
    })
}

fn planted(payload: &serde_json::Value, trace: u64, outcome: u64) -> Anatomy {
    let golden = support::fnv1a(payload.to_string().as_bytes());
    Anatomy::of(payload, golden, trace, outcome)
}

#[test]
fn anatomy_classifier_labels_planted_changes() {
    let base = planted_payload();
    let before = planted(&base, 7, 9);
    let classify = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut after = base.clone();
        edit(&mut after);
        support::classify(&before, &planted(&after, 7, 9))
    };

    assert_eq!(support::classify(&before, &before), None);
    assert_eq!(
        classify(&|p| p["campaign"]["new_counter"] = 0.into()),
        Some(MoveClass::F),
        "an extra key is a format move"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["coins"] = 11.into()),
        Some(MoveClass::G),
        "a value-only state move is never format; it is conservatively gameplay"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["first_clear_claimed"]["level-2"] = 1.into()),
        Some(MoveClass::G),
        "a new entry in a data-keyed map is a state move, not a format move"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["items"][0]["definition"] = "rifle".into()),
        Some(MoveClass::C),
        "a changed item definition is a content move"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["items"][1]["level"] = 3.into()),
        Some(MoveClass::C),
        "a changed item level is a content move"
    );
    assert_eq!(
        classify(&|p| p["attempt"]["telemetry"]["misses"] = 3.into()),
        Some(MoveClass::T),
        "a changed telemetry subtree is a telemetry move"
    );
    assert_eq!(
        classify(&|p| p["attempt"]["telemetry"]["trap_damage"] = 0.into()),
        Some(MoveClass::T),
        "a new telemetry field stays inside the telemetry subtree"
    );
    assert_eq!(
        classify(&|p| p["campaign"]["run_history"][0]["kills"] = 4.into()),
        Some(MoveClass::T),
        "a changed receipt is a telemetry move"
    );
    for after in [planted(&base, 8, 9), planted(&base, 7, 10)] {
        assert_eq!(
            support::classify(&before, &after),
            Some(MoveClass::G),
            "a moved trace or outcome is a gameplay move"
        );
    }
    let mut both = base.clone();
    both["campaign"]["new_counter"] = 0.into();
    assert_eq!(
        support::classify(&before, &planted(&both, 7, 10)),
        Some(MoveClass::G),
        "gameplay outranks every other class"
    );

    // Array indices are stripped: item count does not move the key paths.
    let mut more = base.clone();
    more["campaign"]["items"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id": 3, "definition": "axe", "quality": null, "level": 1, "locked": false}));
    assert_eq!(
        support::json_key_paths_fnv(&base),
        support::json_key_paths_fnv(&more)
    );
    assert_ne!(support::items_fnv(&base), support::items_fnv(&more));

    // Data-keyed map entries are data, not shape: a newly claimed level ID
    // does not move the key paths.
    let mut claimed = base.clone();
    claimed["campaign"]["first_clear_claimed"]["level-2"] = 2.into();
    assert_eq!(
        support::json_key_paths_fnv(&base),
        support::json_key_paths_fnv(&claimed)
    );
    assert!(support::json_key_paths(&base).contains("/campaign/first_clear_claimed/*"));
}

#[test]
fn anatomy_reads_the_real_payload_shape() {
    let g = support::game(Mode::Personal);
    let payload = serde_json::to_value(&g).unwrap();
    let paths = support::json_key_paths(&payload);
    for path in [
        "/campaign/items",
        "/campaign/items/definition",
        "/campaign/items/quality",
        "/campaign/items/level",
        "/campaign/run_history",
        "/attempt/telemetry",
        "/attempt/dps",
        "/attempt/loot_events",
    ] {
        assert!(paths.contains(path), "the save payload has no {path}");
    }
    // Every data-keyed map the key paths collapse is a real payload field, so
    // a rename cannot silently turn its entries back into shape.
    for name in support::DATA_KEYED_MAPS {
        let suffix = format!("/{name}");
        assert!(
            paths.iter().any(|p| p.ends_with(&suffix)),
            "the save payload has no data-keyed map {name}"
        );
    }
    let core = support::json_key_paths(&support::without_telemetry(&payload));
    assert!(!core.iter().any(|p| p.contains("/telemetry")));
    let mut no_items = payload.clone();
    no_items["campaign"]["items"] = serde_json::json!([]);
    assert_ne!(support::items_fnv(&payload), support::items_fnv(&no_items));
}

#[test]
#[ignore = "prints the determinism anatomy of every pinned run"]
fn determinism_anatomy() {
    for (seed, mode, _) in GOLDEN {
        let (g, trace, summary) = observed_run(seed, mode);
        let payload = serde_json::to_value(&g).unwrap();
        let anatomy = Anatomy::of(&payload, g.state_digest(), trace, summary.digest());
        println!("({seed:#x}, Mode::{mode:?}) {anatomy}");
        println!("// {summary}");
    }
}

#[test]
fn golden_runs_exercise_a_live_battle_and_depend_on_the_seed() {
    let mut digests = std::collections::BTreeSet::new();
    for seed in SEEDS {
        for mode in [Mode::Personal, Mode::Npc] {
            let g = run(seed, mode);
            let a = g.attempt().expect("battle is still running");
            assert_eq!(a.tick, u64::from(TICKS));
            assert!(a.telemetry.spawned > 0);
            digests.insert(g.state_digest());
        }
    }
    assert_eq!(digests.len(), 6, "every seed/mode pair has its own state");
}

#[test]
fn outcome_summaries_sample_the_whole_run() {
    assert_eq!(
        support::OUTCOME_SAMPLE_TICKS.last().copied(),
        Some(u64::from(TICKS))
    );
    let summary = support::outcome_summary(SEEDS[0], Mode::Personal);
    let ticks: Vec<u64> = summary.samples.iter().map(|s| s.tick).collect();
    assert_eq!(ticks, support::OUTCOME_SAMPLE_TICKS);
    assert_eq!(summary, observed_run(SEEDS[0], Mode::Personal).2);
}

#[test]
fn the_digest_is_a_pure_function_of_state() {
    let g = run(SEEDS[0], Mode::Personal);
    assert_eq!(g.state_digest(), g.clone().state_digest());
    let mut moved = g.clone();
    advance(&mut moved, 1);
    assert_ne!(g.state_digest(), moved.state_digest());
}
