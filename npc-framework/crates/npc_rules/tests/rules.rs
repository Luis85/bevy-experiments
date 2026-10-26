//! Wave D2: `npc_rules` (character sheet, skills, traits, 3d6 success rolls).

mod common;

use bevy_app::App;
use npc_kernel::hash::state_hash;
use npc_kernel::journal::JournalKind;
use npc_kernel::registry::ContentHash;
use npc_kernel::{Journal, NpcKernelPlugin, SimClock, SimId, WorldSeed, content_hash, spawn_sim};
use npc_rules::{
    Attributes, CharacterSheet, ContestWinner, Contestant, Critical, DiceSpec, Difficulty,
    LevelSource, Purchase, RollCounters, RollOutcome, RuleSet, Rules, RulesAppExt, RulesPlugin,
    RulesRegistry, SecondaryBuys, SheetTemplate, SheetView, SkillDef, SkillProgression, SpendError,
    Stat, apply_template, attribute, dice_total, effective_skill, grant_points, purchase_cost,
    quick_contest, resisted_roll, roll_against, roll_skill, roll_stat, roll_with, sheet_view,
    skill_level, spend_points, template_sheet,
};

fn spawn(app: &mut App, sheet: CharacterSheet) -> SimId {
    spawn_sim(app.world_mut(), sheet).0
}

fn set_tick(app: &mut App, tick: u64) {
    app.world_mut().resource_mut::<SimClock>().tick = tick;
}

fn sheet_with(attrs: Attributes, skills: &[(&str, i32)], traits: &[(&str, u8)]) -> CharacterSheet {
    CharacterSheet {
        attributes: attrs,
        skills: skills.iter().map(|&(n, p)| (n.to_owned(), p)).collect(),
        traits: traits.iter().map(|&(n, l)| (n.to_owned(), l)).collect(),
        ..Default::default()
    }
}

fn with<R>(app: &App, f: impl FnOnce(Rules<'_>) -> R) -> R {
    npc_rules::with_rules(app.world(), f)
}

// ---------------------------------------------------------------------------
// Cost tables and point totals
// ---------------------------------------------------------------------------

#[test]
fn cost_tables_and_point_totals() {
    let app = common::app(1);
    let base = CharacterSheet::default();
    let p = with(&app, |r| r.points(&base));
    assert_eq!((p.spent, p.total), (0, 0), "all-10 sheet costs nothing");

    let mut sheet = sheet_with(
        Attributes::new(12, 11, 9, 10),
        &[("Climbing", 4)],
        &[("Keen Senses", 2), ("Butterfingers", 1)],
    );
    sheet.secondary = SecondaryBuys {
        hp: 2,
        speed_q: 1,
        ..Default::default()
    };
    sheet.unspent = 5;
    let p = with(&app, |r| r.points(&sheet));
    // ST +2 × 10, DX +1 × 20, IQ −1 × 20.
    assert_eq!(p.attributes, 20);
    // HP +2 × 2, Speed +1 quarter × 5.
    assert_eq!(p.secondary, 9);
    // Keen Senses 2 × 2, Butterfingers −10.
    assert_eq!(p.traits, -6);
    assert_eq!(p.skills, 4);
    assert_eq!(p.spent, 27);
    assert_eq!((p.unspent, p.total), (5, 32));

    let rules = RuleSet::default();
    assert_eq!(rules.stat_cost(Stat::Dx, 8), -40, "below base refunds");
    assert_eq!(rules.stat_cost(Stat::Move, -1), -5);
}

#[test]
fn skill_progression_and_difficulty() {
    let prog = SkillProgression::default();
    let bonus: Vec<Option<i32>> = [0, 1, 2, 3, 4, 7, 8, 11, 12, 20]
        .iter()
        .map(|&p| prog.bonus(p))
        .collect();
    assert_eq!(
        bonus,
        [
            None,
            Some(0),
            Some(1),
            Some(1),
            Some(2),
            Some(2),
            Some(3),
            Some(3),
            Some(4),
            Some(6)
        ]
    );
    for b in 0..10 {
        let p = prog.points_for(b).unwrap();
        assert_eq!(prog.bonus(p), Some(b), "points_for inverts bonus at {b}");
        if p > 1 {
            assert!(prog.bonus(p - 1) < Some(b), "minimal points for +{b}");
        }
    }
    assert_eq!(prog.points_for(-1), None);

    let r = RuleSet::default();
    assert_eq!(r.relative_level(Difficulty::Easy, 1), Some(0));
    assert_eq!(r.relative_level(Difficulty::Average, 1), Some(-1));
    assert_eq!(r.relative_level(Difficulty::Average, 2), Some(0));
    assert_eq!(r.relative_level(Difficulty::Average, 4), Some(1));
    assert_eq!(r.relative_level(Difficulty::Hard, 1), Some(-2));
    assert_eq!(r.relative_level(Difficulty::VeryHard, 8), Some(0));
    assert_eq!(r.relative_level(Difficulty::Easy, 0), None);
    assert_eq!(r.points_for_relative(Difficulty::Hard, 1), Some(8));
    assert_eq!(r.points_for_relative(Difficulty::Hard, -3), None);
}

// ---------------------------------------------------------------------------
// Relative / effective levels, defaults
// ---------------------------------------------------------------------------

#[test]
fn bought_skill_levels() {
    let mut app = common::app(1);
    let unit = spawn(
        &mut app,
        sheet_with(
            Attributes::new(10, 12, 10, 10),
            &[("Climbing", 4), ("Alchemy", 1)],
            &[],
        ),
    );
    let lvl = skill_level(app.world(), unit, "Climbing").unwrap();
    assert_eq!((lvl.level, lvl.relative, lvl.stat_value), (13, 1, 12));
    assert_eq!(lvl.relative_label(), "DX+1");
    assert_eq!(lvl.source, LevelSource::Points);
    let alch = skill_level(app.world(), unit, "Alchemy").unwrap();
    assert_eq!((alch.level, alch.relative_label()), (7, "IQ-3".to_owned()));
    assert_eq!(
        effective_skill(app.world(), unit, "Nope"),
        None,
        "unknown skill"
    );
    assert_eq!(
        effective_skill(app.world(), SimId(999), "Climbing"),
        None,
        "unknown unit"
    );
}

#[test]
fn defaults_and_best_default_selection() {
    let mut app = common::app(1);
    let plain = spawn(&mut app, CharacterSheet::default());
    let w = app.world();
    // Stat default: DX − 5.
    let climb = skill_level(w, plain, "Climbing").unwrap();
    assert_eq!(climb.level, 5);
    assert_eq!(
        climb.source,
        LevelSource::Default {
            from: "DX".to_owned(),
            modifier: -5
        }
    );
    // No points, no defaults: cannot be used.
    assert_eq!(effective_skill(w, plain, "Alchemy"), None);
    // Only a skill default, from a skill without points (chain depth 0).
    assert_eq!(effective_skill(w, plain, "Shield Bash"), None);
    // Mutual defaults (Axe <-> Mace) terminate and fall back to DX − 5.
    assert_eq!(effective_skill(w, plain, "Axe"), Some(5));

    // Best default: Herbalism is IQ − 6 or Alchemy − 3, whichever is better.
    let alchemist = spawn(
        &mut app,
        sheet_with(Attributes::default(), &[("Alchemy", 8)], &[]),
    );
    let herb = skill_level(app.world(), alchemist, "Herbalism").unwrap();
    // Alchemy: IQ 10 + VeryHard(−3) + 3 (8 points) = 10 → 10 − 3 = 7 > IQ − 6 = 4.
    assert_eq!(effective_skill(app.world(), alchemist, "Alchemy"), Some(10));
    assert_eq!(herb.level, 7);
    assert_eq!(
        herb.source,
        LevelSource::Default {
            from: "Alchemy".to_owned(),
            modifier: -3
        }
    );

    // Skill default through a bought skill.
    let mace = spawn(
        &mut app,
        sheet_with(Attributes::default(), &[("Mace", 4)], &[]),
    );
    assert_eq!(effective_skill(app.world(), mace, "Mace"), Some(11));
    assert_eq!(
        effective_skill(app.world(), mace, "Axe"),
        Some(8),
        "Mace 11 − 3"
    );
    assert_eq!(
        effective_skill(app.world(), mace, "Shield Bash"),
        Some(9),
        "Mace 11 − 2"
    );

    // With points, a better default wins (best_of_bought_and_default).
    let both = spawn(
        &mut app,
        sheet_with(Attributes::default(), &[("Axe", 1), ("Mace", 12)], &[]),
    );
    // Axe bought: 10 − 1 = 9; from Mace 13 − 3 = 10.
    let axe = skill_level(app.world(), both, "Axe").unwrap();
    assert_eq!(axe.level, 10);
    assert!(matches!(axe.source, LevelSource::Default { ref from, .. } if from == "Mace"));
}

#[test]
fn default_chain_is_configurable() {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), RulesPlugin));
    let mut rules = RuleSet::default();
    rules.defaults.max_depth = 1;
    rules.defaults.best_of_bought_and_default = false;
    app.set_rule_set(rules);
    common::register(&mut app);
    app.finish();
    let plain = spawn(&mut app, CharacterSheet::default());
    // Depth 1: Shield Bash may use Mace's own default (DX − 5 = 5) − 2.
    assert_eq!(effective_skill(app.world(), plain, "Shield Bash"), Some(3));
    let both = spawn(
        &mut app,
        sheet_with(Attributes::default(), &[("Axe", 1), ("Mace", 12)], &[]),
    );
    // Without best-of, bought points always decide.
    assert_eq!(effective_skill(app.world(), both, "Axe"), Some(9));
}

// ---------------------------------------------------------------------------
// Traits and secondary characteristics
// ---------------------------------------------------------------------------

#[test]
fn secondary_characteristics() {
    let mut app = common::app(1);
    let plain = spawn(&mut app, CharacterSheet::default());
    let w = app.world();
    let stats: Vec<i32> = Stat::ALL
        .iter()
        .map(|&s| attribute(w, plain, s).unwrap())
        .collect();
    // ST DX IQ HT HP Will Per FP Speed(q) Move
    assert_eq!(stats, [10, 10, 10, 10, 10, 10, 10, 10, 20, 5]);

    let mut sheet = sheet_with(Attributes::new(10, 12, 10, 11), &[], &[]);
    let quick = spawn(&mut app, sheet.clone());
    assert_eq!(attribute(app.world(), quick, Stat::Speed), Some(23), "5.75");
    assert_eq!(attribute(app.world(), quick, Stat::Move), Some(5));
    assert_eq!(sheet_view(app.world(), quick).unwrap().basic_speed, "5.75");
    sheet.secondary.speed_q = 1;
    sheet.secondary.basic_move = 1;
    sheet.secondary.will = -2;
    let faster = spawn(&mut app, sheet);
    assert_eq!(attribute(app.world(), faster, Stat::Speed), Some(24));
    assert_eq!(
        attribute(app.world(), faster, Stat::Move),
        Some(7),
        "6 from speed + 1"
    );
    assert_eq!(attribute(app.world(), faster, Stat::Will), Some(8));
}

#[test]
fn trait_modifiers() {
    let mut app = common::app(1);
    let unit = spawn(
        &mut app,
        sheet_with(
            Attributes::default(),
            &[("Climbing", 2), ("Lockpicking", 2)],
            &[
                ("Strong Back", 2),
                ("Keen Senses", 3),
                ("Graceful", 1),
                ("Nimble Climber", 2),
                ("Butterfingers", 1),
            ],
        ),
    );
    let w = app.world();
    assert_eq!(attribute(w, unit, Stat::St), Some(12));
    assert_eq!(
        attribute(w, unit, Stat::Hp),
        Some(12),
        "HP follows effective ST"
    );
    assert_eq!(attribute(w, unit, Stat::Per), Some(13));
    assert_eq!(attribute(w, unit, Stat::Dx), Some(11));
    assert_eq!(
        attribute(w, unit, Stat::Speed),
        Some(21),
        "DX bonus flows into speed"
    );
    // Climbing: DX 11 + Average 2 points (+0) + Nimble Climber 2.
    let climb = skill_level(w, unit, "Climbing").unwrap();
    assert_eq!((climb.level, climb.trait_bonus, climb.relative), (13, 2, 2));
    // Lockpicking: DX 11 + 0 − 3.
    assert_eq!(effective_skill(w, unit, "Lockpicking"), Some(8));
    // Trait points: 2 × 10 + 3 × 2 + 20 + 2 × 5 − 10.
    assert_eq!(sheet_view(w, unit).unwrap().points.traits, 46);
}

#[test]
fn templates_apply_at_spawn() {
    let mut app = common::app(1);
    let sheet = template_sheet(app.world(), "scout").unwrap();
    let scout = spawn(&mut app, sheet);
    assert_eq!(
        attribute(app.world(), scout, Stat::Per),
        Some(13),
        "IQ 11 + Keen Senses 2"
    );
    assert_eq!(effective_skill(app.world(), scout, "Climbing"), Some(13));
    let (smith, e) = spawn_sim(app.world_mut(), ());
    apply_template(app.world_mut(), e, "smith").unwrap();
    assert_eq!(attribute(app.world(), smith, Stat::St), Some(14));
    assert_eq!(
        effective_skill(app.world(), smith, "Smithing"),
        Some(13),
        "IQ 11 + 2"
    );
    let err = apply_template(app.world_mut(), e, "pirate").unwrap_err();
    assert!(
        matches!(err, SpendError::UnknownTemplate { ref known, .. } if known == "scout, smith")
    );
}

#[test]
#[should_panic(expected = "unknown skill `Juggling`")]
fn template_with_unknown_skill_panics_at_freeze() {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), RulesPlugin));
    common::register(&mut app);
    app.register_sheet_template(SheetTemplate::new("bad").skill("Juggling", 1));
    app.finish();
}

#[test]
#[should_panic(expected = "defaults from unknown skill")]
fn skill_default_with_unknown_skill_panics_at_freeze() {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), RulesPlugin));
    app.register_skill(
        SkillDef::new("Orphan", Stat::Dx, Difficulty::Easy).default_skill("Ghost", -1),
    );
    app.finish();
}

// ---------------------------------------------------------------------------
// Rolls: distribution, crits, contests
// ---------------------------------------------------------------------------

/// `(successes, critical successes, critical failures)` over all 216 3d6 outcomes.
fn exhaustive(rules: &RuleSet, target: i32) -> (u32, u32, u32) {
    let mut out = (0, 0, 0);
    for a in 1..=6 {
        for b in 1..=6 {
            for c in 1..=6 {
                let o = rules.resolve_roll(a + b + c, target);
                assert_eq!(o.margin, target - o.roll);
                out.0 += u32::from(o.success);
                match o.critical {
                    Critical::Success => {
                        assert!(o.success);
                        out.1 += 1;
                    }
                    Critical::Failure => {
                        assert!(!o.success);
                        out.2 += 1;
                    }
                    Critical::None => {}
                }
            }
        }
    }
    out
}

#[test]
fn roll_distribution_is_exact() {
    let r = RuleSet::default();
    // Exact counts out of 216 (probability = count / 216).
    assert_eq!(
        exhaustive(&r, 3),
        (4, 4, 56),
        "3–4 always succeed; 13+ is 10 over"
    );
    assert_eq!(exhaustive(&r, 6), (20, 4, 10));
    assert_eq!(exhaustive(&r, 10), (108, 4, 1), "exactly one half");
    assert_eq!(exhaustive(&r, 12), (160, 4, 1));
    assert_eq!(exhaustive(&r, 15), (206, 10, 1), "5 is critical at 15");
    assert_eq!(exhaustive(&r, 16), (212, 20, 1), "6 is critical at 16");
    assert_eq!(exhaustive(&r, 25), (212, 20, 1), "17–18 always fail; cap 6");
    assert_eq!(
        exhaustive(&r, -20),
        (4, 4, 212),
        "only the automatic successes"
    );
}

#[test]
fn crit_thresholds() {
    let r = RuleSet::default();
    let o = |roll, target| r.resolve_roll(roll, target);
    assert_eq!(o(4, 3).critical, Critical::Success);
    assert_eq!(o(5, 14).critical, Critical::None);
    assert_eq!(o(5, 15).critical, Critical::Success);
    assert_eq!(o(6, 15).critical, Critical::None);
    assert_eq!(o(6, 16).critical, Critical::Success);
    assert_eq!(o(7, 30).critical, Critical::None, "capped at 6");
    assert_eq!(o(18, 30).critical, Critical::Failure);
    let seventeen = o(17, 30);
    assert!(!seventeen.success && seventeen.critical == Critical::None);
    assert_eq!(seventeen.margin, 13);
    assert_eq!(o(16, 6).critical, Critical::Failure, "10 over the target");
    assert_eq!(o(15, 6).critical, Critical::None);
    let auto = o(4, 2);
    assert!(auto.success && auto.margin == -2);
}

fn isqrt(n: u32) -> u32 {
    (0..=n).take_while(|r| r * r <= n).last().unwrap_or(0)
}

#[test]
fn dice_are_uniform_enough() {
    // Totals over 21 600 keys against the exact distribution × 100.
    const WAYS: [u32; 16] = [1, 3, 6, 10, 15, 21, 25, 27, 27, 25, 21, 15, 10, 6, 3, 1];
    let mut hist = [0u32; 16];
    for tick in 0..21_600u64 {
        let t = dice_total(7, SimId(3), tick, "dice.test", 0, DiceSpec::D6X3);
        assert!((3..=18).contains(&t));
        hist[usize::try_from(t - 3).unwrap()] += 1;
    }
    for (i, (&got, &ways)) in hist.iter().zip(WAYS.iter()).enumerate() {
        let want = ways * 100;
        // ~5 sigma.
        let tol = 5 * isqrt(want) + 20;
        assert!(
            got.abs_diff(want) <= tol,
            "total {}: {got} vs {want}",
            i + 3
        );
    }
}

#[test]
fn quick_contest_outcomes() {
    let r = RuleSet::default();
    let roll = |roll, target| r.resolve_roll(roll, target);
    let w = |a: RollOutcome, b: RollOutcome| r.contest_winner(&a, &b);
    assert_eq!(
        w(roll(10, 12), roll(14, 12)),
        ContestWinner::A,
        "only A succeeds"
    );
    assert_eq!(
        w(roll(14, 12), roll(10, 12)),
        ContestWinner::B,
        "only B succeeds"
    );
    assert_eq!(
        w(roll(9, 12), roll(7, 12)),
        ContestWinner::B,
        "both succeed: 3 vs 5"
    );
    assert_eq!(
        w(roll(13, 12), roll(16, 12)),
        ContestWinner::A,
        "both fail: −1 vs −4"
    );
    assert_eq!(w(roll(9, 12), roll(10, 13)), ContestWinner::Tie);
    // An automatic success beats a failure with a better margin.
    assert_eq!(w(roll(4, 2), roll(17, 20)), ContestWinner::A);
}

#[test]
fn quick_contests_and_resisted_rolls_in_the_world() {
    let mut app = common::app(11);
    let expert = spawn(
        &mut app,
        sheet_with(Attributes::splat(14), &[("Climbing", 16)], &[]),
    );
    let novice = spawn(&mut app, CharacterSheet::default());
    let (mut a_wins, mut b_wins) = (0, 0);
    for tick in 0..300 {
        set_tick(&mut app, tick);
        let c = quick_contest(
            app.world_mut(),
            Contestant::skill(expert, "Climbing", 0),
            Contestant::skill(novice, "Climbing", 0),
            "race",
        )
        .unwrap();
        assert_eq!(c.by, (c.a.margin - c.b.margin).abs());
        // DX 14 + Average(−1) + 5 (16 points) vs DX 10 − 5.
        assert_eq!((c.a.target, c.b.target), (18, 5));
        match c.winner {
            ContestWinner::A => a_wins += 1,
            ContestWinner::B => b_wins += 1,
            ContestWinner::Tie => {}
        }
        let r = resisted_roll(
            app.world_mut(),
            Contestant::stat(novice, Stat::Iq, 0),
            Contestant::stat(expert, Stat::Will, 0),
            "persuade",
        )
        .unwrap();
        assert_eq!(
            r.success,
            r.contest.a.success && r.contest.winner == ContestWinner::A
        );
    }
    assert!(a_wins > 10 * b_wins.max(1), "{a_wins} vs {b_wins}");
    // A side that cannot roll: nothing happens.
    let journal_len = app.world().resource::<Journal>().entries.len();
    assert!(
        quick_contest(
            app.world_mut(),
            Contestant::skill(expert, "Alchemy", 0),
            Contestant::skill(novice, "Climbing", 0),
            "race",
        )
        .is_none()
    );
    assert_eq!(app.world().resource::<Journal>().entries.len(), journal_len);
}

// ---------------------------------------------------------------------------
// Determinism and journal
// ---------------------------------------------------------------------------

#[test]
fn same_key_same_roll_independent_of_other_rolls() {
    let roll_once = |extra_first: bool| {
        let mut app = common::app(42);
        let a = spawn(
            &mut app,
            sheet_with(Attributes::default(), &[("Climbing", 2)], &[]),
        );
        let b = spawn(&mut app, CharacterSheet::default());
        set_tick(&mut app, 77);
        if extra_first {
            // Other units and other purposes roll first, in some order.
            roll_stat(app.world_mut(), b, Stat::Per, 0, "spot").unwrap();
            roll_skill(app.world_mut(), a, "Climbing", 0, "other").unwrap();
            roll_skill(app.world_mut(), b, "Climbing", 0, "climb").unwrap();
        }
        let first = roll_skill(app.world_mut(), a, "Climbing", 0, "climb").unwrap();
        let second = roll_skill(app.world_mut(), a, "Climbing", 0, "climb").unwrap();
        (a, first, second)
    };
    let (a, first, second) = roll_once(false);
    let (_, first2, second2) = roll_once(true);
    assert_eq!((first, second), (first2, second2));
    // It is the pure dice key: (seed, unit, tick, purpose, roll index).
    assert_eq!(
        first.roll,
        dice_total(42, a, 77, "climb", 0, DiceSpec::D6X3)
    );
    assert_eq!(
        second.roll,
        dice_total(42, a, 77, "climb", 1, DiceSpec::D6X3)
    );

    // A new tick restarts the roll index.
    let mut app = common::app(42);
    let a = spawn(
        &mut app,
        sheet_with(Attributes::default(), &[("Climbing", 2)], &[]),
    );
    set_tick(&mut app, 78);
    let r = roll_skill(app.world_mut(), a, "Climbing", 0, "climb").unwrap();
    assert_eq!(r.roll, dice_total(42, a, 78, "climb", 0, DiceSpec::D6X3));
    // Another seed is another stream.
    let differs = (0..50u64).any(|t| {
        dice_total(42, a, t, "climb", 0, DiceSpec::D6X3)
            != dice_total(43, a, t, "climb", 0, DiceSpec::D6X3)
    });
    assert!(differs);
}

#[test]
fn system_side_roll_matches_world_roll() {
    let mut app = common::app(5);
    let a = spawn(&mut app, CharacterSheet::default());
    set_tick(&mut app, 9);
    let world_roll = roll_against(app.world_mut(), a, "test", 11, "p");
    let rules = app.world().resource::<RuleSet>().clone();
    let seed = app.world().resource::<WorldSeed>().0;
    let mut counters = RollCounters::default();
    let sys_roll = roll_with(&rules, &mut counters, None, seed, 9, a, "test", 11, "p");
    assert_eq!(world_roll, sys_roll);
}

#[test]
fn every_roll_is_journaled() {
    let mut app = common::app(3);
    let a = spawn(
        &mut app,
        sheet_with(Attributes::default(), &[("Climbing", 4)], &[]),
    );
    set_tick(&mut app, 12);
    let out = roll_skill(app.world_mut(), a, "Climbing", -3, "climb.wall").unwrap();
    let per = roll_stat(app.world_mut(), a, Stat::Per, 2, "spot").unwrap();
    assert_eq!(roll_skill(app.world_mut(), a, "Alchemy", 0, "brew"), None);
    let rolls: Vec<_> = app
        .world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| e.kind.name() == "Roll")
        .cloned()
        .collect();
    assert_eq!(rolls.len(), 2, "no entry for a roll that could not happen");
    assert_eq!((rolls[0].tick, rolls[0].unit), (12, Some(a)));
    assert_eq!(
        rolls[0].kind,
        JournalKind::Roll {
            skill: "Climbing".to_owned(),
            target: 8,
            roll: out.roll,
            margin: 8 - out.roll,
            success: out.success,
            crit: out.critical,
            purpose: "climb.wall".to_owned(),
        }
    );
    assert!(matches!(
        &rolls[1].kind,
        JournalKind::Roll { skill, target: 12, purpose, .. } if skill == "Per" && purpose == "spot" && per.target == 12
    ));
    let json = serde_json::to_value(&rolls[0]).unwrap();
    assert_eq!(json["kind"]["type"], "Roll");
    assert_eq!(JournalKind::parse_name("roll"), Some("Roll"));
}

// ---------------------------------------------------------------------------
// Spending points
// ---------------------------------------------------------------------------

#[test]
fn spend_points_validation() {
    let mut app = common::app(1);
    let sheet = CharacterSheet {
        unspent: 3,
        ..Default::default()
    };
    let u = spawn(&mut app, sheet);
    let skill = |s: &str, points| Purchase::Skill {
        skill: s.to_owned(),
        points,
    };
    let w = app.world_mut();
    assert_eq!(
        spend_points(w, SimId(999), &skill("Climbing", 1)),
        Err(SpendError::NoSheet { unit: 999 })
    );
    assert!(matches!(
        spend_points(w, u, &skill("Juggling", 1)),
        Err(SpendError::UnknownSkill { ref known, .. }) if known.contains("Climbing")
    ));
    assert_eq!(
        spend_points(w, u, &skill("Climbing", 0)),
        Err(SpendError::InvalidAmount { amount: 0 })
    );
    let before = w
        .get::<CharacterSheet>(w.resource::<npc_kernel::SimIndex>().get(u).unwrap())
        .cloned();
    assert_eq!(
        spend_points(w, u, &skill("Climbing", 4)),
        Err(SpendError::NotEnoughPoints {
            cost: 4,
            unspent: 3
        })
    );
    assert_eq!(
        purchase_cost(w, u, &skill("Climbing", 4)),
        Ok(4),
        "preview ignores the bank"
    );
    assert_eq!(
        npc_rules::roll::sheet(w, u).cloned(),
        before,
        "unchanged on error"
    );
    assert_eq!(spend_points(w, u, &skill("Climbing", 2)), Ok(2));
    assert_eq!(effective_skill(w, u, "Climbing"), Some(10));
    assert_eq!(npc_rules::roll::sheet(w, u).unwrap().unspent, 1);

    assert_eq!(grant_points(w, u, 60), Ok(61));
    let stat = |stat, levels| Purchase::Stat { stat, levels };
    assert_eq!(spend_points(w, u, &stat(Stat::Dx, 1)), Ok(20));
    assert_eq!(spend_points(w, u, &stat(Stat::Hp, 2)), Ok(4));
    assert_eq!(attribute(w, u, Stat::Dx), Some(11));
    assert_eq!(attribute(w, u, Stat::Hp), Some(12));
    assert!(matches!(
        spend_points(w, u, &stat(Stat::Iq, 21)),
        Err(SpendError::AboveMax {
            max: 30,
            value: 31,
            ..
        })
    ));
    let tr = |name: &str, levels| Purchase::Trait {
        name: name.to_owned(),
        levels,
    };
    assert_eq!(
        spend_points(w, u, &tr("Butterfingers", 1)),
        Err(SpendError::NotPurchasable {
            name: "Butterfingers".to_owned()
        })
    );
    assert!(matches!(
        spend_points(w, u, &tr("Keen Senses", 5)),
        Err(SpendError::AboveMax { max: 4, .. })
    ));
    assert_eq!(spend_points(w, u, &tr("Keen Senses", 2)), Ok(4));
    assert_eq!(attribute(w, u, Stat::Per), Some(12));
    assert_eq!(npc_rules::roll::sheet(w, u).unwrap().unspent, 33);
    assert_eq!(
        grant_points(w, u, -40),
        Err(SpendError::NotEnoughPoints {
            cost: 40,
            unspent: 33
        })
    );
    // Total value is conserved: the 3 banked at spawn plus the 60 granted.
    assert_eq!(sheet_view(w, u).unwrap().points.total, 63);
}

// ---------------------------------------------------------------------------
// Serialization, inspect, hashing, opt-in
// ---------------------------------------------------------------------------

#[test]
fn serialization_roundtrip() {
    let mut app = common::app(1);
    let sheet = template_sheet(app.world(), "scout").unwrap();
    let json = serde_json::to_string(&sheet).unwrap();
    assert_eq!(
        serde_json::from_str::<CharacterSheet>(&json).unwrap(),
        sheet
    );
    assert!(json.contains(r#""skills":{"Climbing":4}"#), "{json}");

    let u = spawn(&mut app, sheet);
    let view = sheet_view(app.world(), u).unwrap();
    let vjson = serde_json::to_string(&view).unwrap();
    assert_eq!(serde_json::from_str::<SheetView>(&vjson).unwrap(), view);
    assert_eq!(view.skills[0].relative_label.as_deref(), Some("DX+1"));
    assert_eq!(view.rule_set, "npc_rules.default");

    let p = Purchase::Skill {
        skill: "Climbing".to_owned(),
        points: 2,
    };
    let pj = serde_json::to_string(&p).unwrap();
    assert_eq!(pj, r#"{"type":"Skill","skill":"Climbing","points":2}"#);
    assert_eq!(serde_json::from_str::<Purchase>(&pj).unwrap(), p);
}

#[test]
fn inspect_shows_the_sheet_and_hash_covers_it() {
    let mut app = common::app(1);
    let smith = template_sheet(app.world(), "smith").unwrap();
    let u = spawn(&mut app, smith);
    let unit = npc_kernel::inspect::unit_json(app.world(), u).unwrap();
    let cs = &unit["components"]["character_sheet"];
    assert_eq!(cs["rule_set"], "npc_rules.default");
    assert_eq!(cs["stats"][0]["name"], "ST");
    assert_eq!(cs["stats"][0]["value"], 14);
    assert_eq!(cs["skills"][0]["name"], "Axe");
    let h1 = state_hash(app.world_mut());
    grant_points(app.world_mut(), u, 1).unwrap();
    assert_ne!(h1, state_hash(app.world_mut()), "the sheet is sim state");
}

#[test]
fn opt_in_without_the_plugin_nothing_is_registered() {
    let mut plain = App::new();
    plain.add_plugins(NpcKernelPlugin::default());
    plain.finish();
    let manifest = &plain.world().resource::<ContentHash>().manifest;
    assert!(
        !manifest
            .iter()
            .any(|(c, n)| c.starts_with("rules") || c == "rule_set" || n == "character_sheet"),
        "{manifest:?}"
    );
    assert!(!plain.world().contains_resource::<RulesRegistry>());

    let mut with = App::new();
    with.add_plugins((NpcKernelPlugin::default(), RulesPlugin));
    with.finish();
    let manifest = &with.world().resource::<ContentHash>().manifest;
    assert!(manifest.contains(&("sim_component".to_owned(), "character_sheet".to_owned())));
    assert!(manifest.contains(&("rule_set".to_owned(), "npc_rules.default".to_owned())));
}

#[test]
fn registration_order_does_not_matter() {
    let mut a = App::new();
    a.add_plugins((NpcKernelPlugin::default(), RulesPlugin));
    common::register(&mut a);
    let mut b = App::new();
    common::register(&mut b);
    b.add_plugins((RulesPlugin, NpcKernelPlugin::default()));
    assert_eq!(content_hash(a.world_mut()), content_hash(b.world_mut()));
}

fn d20_under(_: &RuleSet, roll: i32, target: i32) -> RollOutcome {
    RollOutcome {
        roll,
        target,
        margin: target - roll,
        success: roll <= target,
        critical: match roll {
            1 => Critical::Success,
            20 => Critical::Failure,
            _ => Critical::None,
        },
    }
}

#[test]
fn rule_set_is_pluggable() {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), RulesPlugin));
    app.set_rule_set(RuleSet {
        name: "test.d20",
        dice: DiceSpec {
            count: 1,
            sides: 20,
        },
        resolve: d20_under,
        ..Default::default()
    });
    common::register(&mut app);
    app.finish();
    let u = spawn(
        &mut app,
        sheet_with(Attributes::default(), &[("Climbing", 2)], &[]),
    );
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for tick in 0..400 {
        set_tick(&mut app, tick);
        let r = roll_skill(app.world_mut(), u, "Climbing", 0, "climb").unwrap();
        assert_eq!(r.success, r.roll <= 10);
        lo = lo.min(r.roll);
        hi = hi.max(r.roll);
    }
    assert_eq!((lo, hi), (1, 20));
    let manifest = &app.world().resource::<ContentHash>().manifest;
    assert!(manifest.contains(&("rule_set".to_owned(), "test.d20".to_owned())));
}

#[test]
fn rules_view_matches_world_api() {
    let mut app = common::app(1);
    let sheet = template_sheet(app.world(), "scout").unwrap();
    let u = spawn(&mut app, sheet.clone());
    let set = app.world().resource::<RuleSet>();
    let reg = app.world().resource::<RulesRegistry>();
    let r = Rules::new(set, reg);
    assert_eq!(
        r.effective_skill(&sheet, "Climbing"),
        effective_skill(app.world(), u, "Climbing")
    );
    assert_eq!(
        Some(r.stat(&sheet, Stat::Per)),
        attribute(app.world(), u, Stat::Per)
    );
    assert_eq!(Some(r.view(&sheet)), sheet_view(app.world(), u));
}

#[test]
fn roll_feed_is_off_by_default_and_records_every_roll_when_on() {
    let mut app = common::app(3);
    let a = spawn(
        &mut app,
        sheet_with(Attributes::default(), &[("Climbing", 4)], &[]),
    );
    set_tick(&mut app, 4);
    roll_skill(app.world_mut(), a, "Climbing", 0, "climb").unwrap();
    assert!(
        app.world()
            .resource::<RollCounters>()
            .feed
            .records
            .is_empty(),
        "nothing is recorded without a consumer"
    );
    app.world_mut().resource_mut::<RollCounters>().feed.enabled = true;
    let world_roll = roll_skill(app.world_mut(), a, "Climbing", -1, "climb").unwrap();
    let rules = app.world().resource::<RuleSet>().clone();
    let seed = app.world().resource::<WorldSeed>().0;
    let sys_roll = {
        let mut counters = app.world_mut().resource_mut::<RollCounters>();
        roll_with(&rules, &mut counters, None, seed, 4, a, "Per", 12, "spot")
    };
    let records = app.world_mut().resource_mut::<RollCounters>().drain_feed();
    assert_eq!(records.len(), 2);
    assert_eq!(
        (
            records[0].unit,
            records[0].skill.as_str(),
            records[0].purpose.as_str()
        ),
        (a, "Climbing", "climb")
    );
    assert_eq!(records[0].outcome, world_roll);
    assert_eq!(records[0].tick, 4);
    assert_eq!(records[1].outcome, sys_roll);
    assert_eq!(records[1].skill, "Per");
    assert!(
        app.world()
            .resource::<RollCounters>()
            .feed
            .records
            .is_empty()
    );
    assert!(app.world().resource::<RollCounters>().feed.enabled);
}
