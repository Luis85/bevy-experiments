//! Wave F3: `npc_progression` (XP, levels, spend policies, practice, study,
//! teaching) on test-only rules content (neutral skill names, no game
//! content).

use bevy_app::App;
use npc_kernel::hash::state_hash;
use npc_kernel::journal::JournalKind;
use npc_kernel::registry::KernelAppExt;
use npc_kernel::{
    Journal, JournalPlugin, NpcKernelPlugin, SimClock, SimCommand, SimId, run_ticks, spawn_sim,
    submit,
};
use npc_progression::{
    Experience, Learning, ProgressionAppExt, ProgressionConfig, ProgressionError,
    ProgressionPlugin, SkillProgress, SpendPolicy, Via, XP_JOB, XpSourceDef, award_xp,
    default_roll_xp, experience_json_of, grant_practice, grant_study, grant_teaching,
};
use npc_rules::{
    CharacterSheet, Critical, Difficulty, Purchase, RollCounters, RollOutcome, RollRecord,
    RulesAppExt, RulesPlugin, SkillDef, SpendPoints, Stat, effective_skill, roll_skill,
};

fn base_app(seed: u64, journal: bool, configure: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((
        NpcKernelPlugin {
            seed,
            ..Default::default()
        },
        RulesPlugin,
        ProgressionPlugin::default(),
    ));
    if journal {
        app.add_plugins(JournalPlugin::default());
    }
    app.register_skill(
        SkillDef::new("Climbing", Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
    )
    .register_skill(
        SkillDef::new("Smithing", Stat::Iq, Difficulty::Average).default_stat(Stat::Iq, -5),
    )
    .register_skill(SkillDef::new("Herbalism", Stat::Iq, Difficulty::Hard))
    .register_command::<SpendPoints>();
    configure(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn app() -> App {
    base_app(7, true, |_| {})
}

fn sheet(skills: &[(&str, i32)]) -> CharacterSheet {
    CharacterSheet {
        skills: skills.iter().map(|&(n, p)| (n.to_owned(), p)).collect(),
        ..Default::default()
    }
}

fn unit(app: &mut App, skills: &[(&str, i32)], policy: SpendPolicy) -> SimId {
    spawn_sim(
        app.world_mut(),
        (sheet(skills), Experience::default(), policy),
    )
    .0
}

fn comp<T: bevy_ecs::component::Component + Clone>(app: &App, id: SimId) -> T {
    let e = app
        .world()
        .resource::<npc_kernel::SimIndex>()
        .get(id)
        .unwrap();
    app.world().get::<T>(e).cloned().unwrap()
}

fn kinds(app: &App, unit: SimId) -> Vec<JournalKind> {
    app.world()
        .resource::<Journal>()
        .entries
        .iter()
        .filter(|e| e.unit == Some(unit))
        .map(|e| e.kind.clone())
        .collect()
}

fn record(target: i32, success: bool, critical: Critical) -> RollRecord {
    RollRecord {
        tick: 0,
        unit: SimId(1),
        skill: "Climbing".into(),
        outcome: RollOutcome {
            roll: 9,
            target,
            margin: target - 9,
            success,
            critical,
        },
        purpose: "t".into(),
    }
}

#[test]
fn default_roll_xp_scales_with_target_and_difficulty() {
    let c = ProgressionConfig::default();
    let avg = Some(Difficulty::Average);
    assert_eq!(
        default_roll_xp(&c, &record(10, true, Critical::None), avg),
        13
    );
    assert_eq!(
        default_roll_xp(&c, &record(14, true, Critical::None), avg),
        9
    );
    assert_eq!(
        default_roll_xp(
            &c,
            &record(10, true, Critical::None),
            Some(Difficulty::VeryHard)
        ),
        17
    );
    assert_eq!(
        default_roll_xp(&c, &record(10, true, Critical::Success), avg),
        18
    );
    assert_eq!(
        default_roll_xp(&c, &record(10, false, Critical::None), avg),
        0
    );
    // Hardness is capped: a target of -20 is not worth more than max_hardness.
    assert_eq!(
        default_roll_xp(&c, &record(-20, true, Critical::None), avg),
        17
    );
}

#[test]
fn xp_comes_from_successful_rolls_through_the_feed() {
    let mut app = app();
    let u = unit(&mut app, &[("Climbing", 4)], SpendPolicy::Player);
    let mut expected = 0u64;
    let mut successes = 0;
    for _ in 0..12 {
        let out = roll_skill(app.world_mut(), u, "Climbing", 0, "climb").unwrap();
        if out.success {
            successes += 1;
            let r = RollRecord {
                tick: 0,
                unit: u,
                skill: "Climbing".into(),
                outcome: out,
                purpose: "climb".into(),
            };
            expected += u64::from(default_roll_xp(
                &ProgressionConfig::default(),
                &r,
                Some(Difficulty::Average),
            ));
        }
        run_ticks(app.world_mut(), 1);
    }
    assert!(successes > 0, "some rolls succeed at level 11");
    let exp: Experience = comp(&app, u);
    assert_eq!(exp.xp, expected);
    let gained: u64 = kinds(&app, u)
        .iter()
        .filter_map(|k| match k {
            JournalKind::XpGained { source, amount, .. } if source == "roll" => {
                Some(u64::from(*amount))
            }
            _ => None,
        })
        .sum();
    assert_eq!(gained, expected);
    let learning: Learning = comp(&app, u);
    assert_eq!(learning.uses("Climbing"), 12);
    assert!(
        app.world()
            .resource::<RollCounters>()
            .feed
            .records
            .is_empty(),
        "drained every tick"
    );
}

#[test]
fn stat_rolls_and_units_without_experience_earn_nothing() {
    let mut app = app();
    let plain = spawn_sim(app.world_mut(), sheet(&[("Climbing", 4)])).0;
    let u = unit(&mut app, &[("Climbing", 4)], SpendPolicy::Player);
    for _ in 0..5 {
        roll_skill(app.world_mut(), plain, "Climbing", 10, "climb").unwrap();
        npc_rules::roll_stat(app.world_mut(), u, Stat::Per, 10, "spot").unwrap();
        run_ticks(app.world_mut(), 1);
    }
    let exp: Experience = comp(&app, u);
    assert_eq!(exp.xp, 0);
    assert!(comp::<Learning>(&app, u).skills.is_empty());
}

#[test]
fn level_up_grants_points_and_player_policy_banks_them() {
    let mut app = app();
    let u = unit(&mut app, &[("Climbing", 1)], SpendPolicy::Player);
    let a = award_xp(app.world_mut(), u, 99, XP_JOB).unwrap();
    assert_eq!((a.level, a.levels_gained), (0, 0));
    let a = award_xp(app.world_mut(), u, 1, XP_JOB).unwrap();
    assert_eq!(
        (a.xp, a.level, a.levels_gained, a.points_granted),
        (100, 1, 1, 5)
    );
    // 100 -> 700 crosses level 2 (300) and 3 (600) at once.
    let a = award_xp(app.world_mut(), u, 600, XP_JOB).unwrap();
    assert_eq!((a.level, a.levels_gained, a.points_granted), (3, 2, 10));
    let s: CharacterSheet = comp(&app, u);
    assert_eq!(s.unspent, 15, "Player banks every point");
    assert_eq!(s.skill_points("Climbing"), 1);
    let levels: Vec<(u32, i32)> = kinds(&app, u)
        .iter()
        .filter_map(|k| match k {
            JournalKind::LevelUp { level, points } => Some((*level, *points)),
            _ => None,
        })
        .collect();
    assert_eq!(levels, vec![(1, 5), (2, 5), (3, 5)]);
    // The player spends through the existing rules command.
    submit(
        app.world_mut(),
        None,
        SimCommand::content(&SpendPoints {
            unit: u.0,
            purchase: Purchase::Skill {
                skill: "Smithing".into(),
                points: 4,
            },
        }),
    )
    .unwrap();
    run_ticks(app.world_mut(), 1);
    let s: CharacterSheet = comp(&app, u);
    assert_eq!((s.unspent, s.skill_points("Smithing")), (11, 4));
    assert_eq!(
        experience_json_of(app.world(), u).unwrap()["next_level_xp"],
        1000
    );
}

#[test]
fn ai_policy_spends_on_the_most_used_skill_deterministically() {
    let run = || {
        let mut app = app();
        let u = unit(
            &mut app,
            &[("Climbing", 1), ("Smithing", 1)],
            SpendPolicy::Ai,
        );
        {
            let e = app
                .world()
                .resource::<npc_kernel::SimIndex>()
                .get(u)
                .unwrap();
            let mut l = app.world_mut().get_mut::<Learning>(e).unwrap();
            l.skills.insert(
                "Climbing".into(),
                SkillProgress {
                    uses: 10,
                    ..Default::default()
                },
            );
            l.skills.insert(
                "Smithing".into(),
                SkillProgress {
                    uses: 2,
                    ..Default::default()
                },
            );
        }
        award_xp(app.world_mut(), u, 100, XP_JOB).unwrap();
        let s: CharacterSheet = comp(&app, u);
        (s, state_hash(app.world_mut()), kinds(&app, u))
    };
    let (s, h1, journal) = run();
    assert_eq!(s.unspent, 0);
    assert_eq!(
        s.skill_points("Climbing"),
        6,
        "all 5 points to the most-used skill"
    );
    assert_eq!(s.skill_points("Smithing"), 1);
    assert!(journal.iter().any(|k| matches!(
        k,
        JournalKind::SkillImproved { skill, points: 5, via, .. } if skill == "Climbing" && via == "spend"
    )));
    assert_eq!(run().1, h1, "same inputs, same state");
}

#[test]
fn ai_policy_weights_spending_by_uses_per_point() {
    let mut app = app();
    let u = unit(
        &mut app,
        &[("Climbing", 1), ("Smithing", 1)],
        SpendPolicy::Ai,
    );
    let e = app
        .world()
        .resource::<npc_kernel::SimIndex>()
        .get(u)
        .unwrap();
    {
        let mut l = app.world_mut().get_mut::<Learning>(e).unwrap();
        for (s, n) in [("Climbing", 6), ("Smithing", 5)] {
            l.skills.insert(
                s.into(),
                SkillProgress {
                    uses: n,
                    ..Default::default()
                },
            );
        }
    }
    award_xp(app.world_mut(), u, 100, XP_JOB).unwrap();
    let s: CharacterSheet = comp(&app, u);
    // uses / (points + 1): C 6/2 > S 5/2 -> C; S 5/2 > C 6/3 -> S;
    // C 6/3 > S 5/3 -> C; S 5/3 > C 6/4 -> S; C 6/4 > S 5/4 -> C.
    assert_eq!(s.skill_points("Climbing") + s.skill_points("Smithing"), 7);
    assert_eq!(
        (s.skill_points("Climbing"), s.skill_points("Smithing")),
        (4, 3)
    );
}

#[test]
fn ai_policy_without_used_skills_banks() {
    let mut app = app();
    let u = unit(&mut app, &[], SpendPolicy::Ai);
    award_xp(app.world_mut(), u, 100, XP_JOB).unwrap();
    assert_eq!(comp::<CharacterSheet>(&app, u).unspent, 5);
}

#[test]
fn practice_accrues_from_rolls_and_converts_to_points() {
    let mut app = app();
    let u = unit(&mut app, &[("Climbing", 1)], SpendPolicy::Player);
    // 30 practice ticks per roll, 300 per point: 10 rolls = 1 point.
    for i in 0..10 {
        roll_skill(app.world_mut(), u, "Climbing", 0, "climb").unwrap();
        run_ticks(app.world_mut(), 1);
        let pts = comp::<CharacterSheet>(&app, u).skill_points("Climbing");
        assert_eq!(pts, if i < 9 { 1 } else { 2 }, "after roll {}", i + 1);
    }
    let l: Learning = comp(&app, u);
    assert_eq!(l.get("Climbing").practice, 0);
    assert!(kinds(&app, u).iter().any(|k| matches!(
        k,
        JournalKind::SkillImproved { via, points: 1, .. } if via == "practice"
    )));
}

#[test]
fn practice_alone_is_capped_at_attribute_plus_two() {
    let mut app = app();
    let u = unit(&mut app, &[("Climbing", 4)], SpendPolicy::Player);
    assert_eq!(effective_skill(app.world(), u, "Climbing"), Some(11));
    let out = grant_practice(app.world_mut(), u, "Climbing", 300 * 50).unwrap();
    assert!(out.capped);
    assert_eq!(out.cap, 12, "DX 10 + 2");
    assert_eq!(out.level, Some(12));
    assert_eq!(out.progress, 300, "one point's worth waits at the cap");
    let pts = comp::<CharacterSheet>(&app, u).skill_points("Climbing");
    // One more point would pass the cap.
    let mut next = comp::<CharacterSheet>(&app, u);
    next.skills.insert("Climbing".into(), pts + 1);
    let lvl = npc_rules::with_rules(app.world(), |r| r.effective_skill(&next, "Climbing"));
    assert_eq!(lvl, Some(13));
    // Capped practice gains nothing more.
    let again = grant_practice(app.world_mut(), u, "Climbing", 300).unwrap();
    assert_eq!(again.points, 0);
    assert!(again.capped);
}

#[test]
fn study_has_its_own_rate_and_caps() {
    let mut app = app();
    let u = unit(&mut app, &[("Smithing", 1)], SpendPolicy::Player);
    let out = grant_study(app.world_mut(), u, "Smithing", 199, None).unwrap();
    assert_eq!((out.points, out.progress, out.via), (0, 199, Via::Study));
    let out = grant_study(app.world_mut(), u, "Smithing", 1, None).unwrap();
    assert_eq!(out.points, 1);
    // The book only teaches up to level 10.
    let out = grant_study(app.world_mut(), u, "Smithing", 200 * 40, Some(10)).unwrap();
    assert!(out.capped);
    assert_eq!(out.level, Some(10));
    // Without a book limit: IQ 10 + 4.
    let out = grant_study(app.world_mut(), u, "Smithing", 200 * 80, None).unwrap();
    assert_eq!((out.cap, out.level), (14, Some(14)));
    assert_eq!(
        grant_study(app.world_mut(), u, "Juggling", 1, None)
            .unwrap_err()
            .to_string(),
        "unknown skill `Juggling` (known: Climbing, Herbalism, Smithing)"
    );
}

#[test]
fn teaching_is_gated_and_capped_by_the_teacher() {
    let mut app = app();
    let teacher = spawn_sim(app.world_mut(), sheet(&[("Herbalism", 8)])).0;
    let novice = unit(&mut app, &[], SpendPolicy::Player);
    let peer = unit(&mut app, &[("Herbalism", 8)], SpendPolicy::Player);
    let t_level = effective_skill(app.world(), teacher, "Herbalism").unwrap();
    let out = grant_teaching(app.world_mut(), teacher, novice, "Herbalism", 100 * 60).unwrap();
    assert_eq!(out.via, Via::Teaching);
    assert!(out.capped);
    assert_eq!(out.level, Some(t_level), "never past the teacher");
    let err = grant_teaching(app.world_mut(), teacher, peer, "Herbalism", 100).unwrap_err();
    assert!(matches!(
        err,
        ProgressionError::TeacherTooWeak { min_lead: 1, .. }
    ));
    assert_eq!(
        grant_teaching(app.world_mut(), peer, peer, "Herbalism", 1).unwrap_err(),
        ProgressionError::SelfTeaching
    );
    let err = grant_teaching(app.world_mut(), novice, teacher, "Smithing", 1).unwrap_err();
    assert!(
        matches!(err, ProgressionError::NotProgressing { .. }),
        "the student needs Experience: {err}"
    );
}

#[test]
fn errors_name_the_problem() {
    let mut app = app();
    let u = unit(&mut app, &[], SpendPolicy::Ai);
    let plain = spawn_sim(app.world_mut(), sheet(&[])).0;
    assert_eq!(
        award_xp(app.world_mut(), u, 1, "luck")
            .unwrap_err()
            .to_string(),
        "unknown xp source `luck` (known: craft, discovery, job, roll)"
    );
    assert_eq!(
        award_xp(app.world_mut(), plain, 1, XP_JOB).unwrap_err(),
        ProgressionError::NotProgressing { unit: plain.0 }
    );
    assert_eq!(
        award_xp(app.world_mut(), SimId(999), 1, XP_JOB).unwrap_err(),
        ProgressionError::NoUnit { unit: 999 }
    );
}

#[test]
fn custom_sources_and_config() {
    let mut app = base_app(1, true, |app| {
        app.register_xp_source(XpSourceDef::new("quest").scale_pct(200))
            .set_progression_config(ProgressionConfig {
                points_per_level: 2,
                curve: npc_progression::LevelCurve::Table {
                    totals: vec![10, 30],
                    then_every: 0,
                },
                ..Default::default()
            });
    });
    let u = unit(&mut app, &[], SpendPolicy::Player);
    let a = award_xp(app.world_mut(), u, 10, "quest").unwrap();
    assert_eq!((a.amount, a.level, a.points_granted), (20, 1, 2));
    let a = award_xp(app.world_mut(), u, 1000, "quest").unwrap();
    assert_eq!(a.level, 2, "the table is the last level");
}

fn scripted_run(seed: u64, journal: bool) -> u64 {
    let mut app = base_app(seed, journal, |_| {});
    let a = unit(&mut app, &[("Climbing", 4)], SpendPolicy::Ai);
    let b = unit(&mut app, &[("Smithing", 2)], SpendPolicy::Player);
    for t in 0..120u64 {
        roll_skill(app.world_mut(), a, "Climbing", 0, "climb").unwrap();
        if t % 3 == 0 {
            roll_skill(app.world_mut(), b, "Smithing", 1, "smith").unwrap();
        }
        run_ticks(app.world_mut(), 1);
    }
    assert_eq!(app.world().resource::<SimClock>().tick, 120);
    assert!(
        comp::<Experience>(&app, a).level >= 1,
        "climbing every tick levels"
    );
    state_hash(app.world_mut())
}

#[test]
fn progression_is_deterministic_and_independent_of_the_journal() {
    let h = scripted_run(3, true);
    assert_eq!(scripted_run(3, true), h);
    assert_eq!(scripted_run(3, false), h, "tooling never changes results");
    assert_ne!(scripted_run(4, true), h, "the seed matters");
}

#[test]
fn without_the_plugin_nothing_is_registered() {
    let mut app = App::new();
    app.add_plugins((NpcKernelPlugin::default(), RulesPlugin));
    app.finish();
    assert!(!app.world().resource::<RollCounters>().feed.enabled);
    let names: Vec<_> = app
        .world()
        .resource::<npc_kernel::hash::SimHashRegistry>()
        .components
        .iter()
        .map(|c| c.name)
        .collect();
    assert!(!names.contains(&"experience"));
}
