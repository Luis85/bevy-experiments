//! The progression rules (XP, levels, spending, learning) and the world API.
//!
//! One code path: the pure step functions below work on a unit's
//! [`Experience`], [`Learning`], `CharacterSheet` and [`SpendPolicy`] and
//! return the journal entries they caused; the system and the world API
//! ([`award_xp`], [`grant_practice`], [`grant_study`], [`grant_teaching`])
//! both call them.

use std::collections::BTreeMap;
use std::fmt;

use bevy_ecs::prelude::*;
use npc_kernel::journal::{self, Journal, JournalKind};
use npc_kernel::{SimClock, SimId, SimIndex};
use npc_rules::{
    CharacterSheet, Purchase, RollCounters, RollRecord, RuleSet, Rules, RulesRegistry, with_rules,
};
use serde::{Deserialize, Serialize};

use crate::model::{Experience, Learning, ProgressionConfig, SkillProgress, SpendPolicy};
use crate::sources::XpSources;

/// How a skill point was learned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Via {
    Practice,
    Study,
    Teaching,
}

impl Via {
    pub fn name(self) -> &'static str {
        match self {
            Via::Practice => "practice",
            Via::Study => "study",
            Via::Teaching => "teaching",
        }
    }
}

/// Why a progression call did nothing. Display is a short human-readable
/// reason (usable as an affordance's `why_not`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProgressionError {
    /// No such unit.
    NoUnit {
        unit: u32,
    },
    /// The unit has no `Experience` or no `CharacterSheet`.
    NotProgressing {
        unit: u32,
    },
    UnknownSource {
        source: String,
        known: String,
    },
    UnknownSkill {
        skill: String,
        known: String,
    },
    /// The teacher has no level in the skill.
    TeacherLacksSkill {
        teacher: u32,
        skill: String,
    },
    /// The teacher is not far enough ahead of the student.
    TeacherTooWeak {
        teacher_level: i32,
        student_level: i32,
        min_lead: i32,
    },
    /// Teacher and student are the same unit.
    SelfTeaching,
}

impl fmt::Display for ProgressionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProgressionError::NoUnit { unit } => write!(f, "unit {unit} does not exist"),
            ProgressionError::NotProgressing { unit } => {
                write!(f, "unit {unit} has no experience or character sheet")
            }
            ProgressionError::UnknownSource { source, known } => {
                write!(f, "unknown xp source `{source}` (known: {known})")
            }
            ProgressionError::UnknownSkill { skill, known } => {
                write!(f, "unknown skill `{skill}` (known: {known})")
            }
            ProgressionError::TeacherLacksSkill { teacher, skill } => {
                write!(f, "teacher {teacher} does not know {skill}")
            }
            ProgressionError::TeacherTooWeak {
                teacher_level,
                student_level,
                min_lead,
            } => write!(
                f,
                "teacher level {teacher_level} must be at least the student's {student_level} + {min_lead}"
            ),
            ProgressionError::SelfTeaching => write!(f, "a unit cannot teach itself"),
        }
    }
}

impl std::error::Error for ProgressionError {}

/// What [`award_xp`] did.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct XpAward {
    /// XP actually added (after the source's scale).
    pub amount: u32,
    pub xp: u64,
    pub level: u32,
    pub levels_gained: u32,
    /// Character points granted by those levels.
    pub points_granted: i32,
}

/// What a learning grant did.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LearnOutcome {
    pub skill: String,
    pub via: Via,
    /// Skill points gained now.
    pub points: i32,
    /// The effective level afterwards (`None`: still no usable level).
    pub level: Option<i32>,
    /// The cap stopped (some of) it.
    pub capped: bool,
    /// The cap that applied.
    pub cap: i32,
    /// Ticks now accrued toward the next point.
    pub progress: u32,
}

/// One unit's mutable progression state.
pub struct UnitState<'a> {
    pub exp: &'a mut Experience,
    pub learning: &'a mut Learning,
    pub sheet: &'a mut CharacterSheet,
    pub policy: SpendPolicy,
}

fn unknown_skill(rules: &Rules<'_>, skill: &str) -> ProgressionError {
    ProgressionError::UnknownSkill {
        skill: skill.to_owned(),
        known: rules.registry.skill_names(),
    }
}

/// Adds `amount` XP (journal `XpGained`), then every level it reaches
/// (`LevelUp`, `points_per_level` banked points each), then, for
/// [`SpendPolicy::Ai`] units that levelled, spends the bank
/// ([`ai_spend`]).
pub fn add_xp(
    rules: &Rules<'_>,
    config: &ProgressionConfig,
    u: &mut UnitState<'_>,
    amount: u32,
    source: &str,
    out: &mut Vec<JournalKind>,
) -> XpAward {
    u.exp.xp = u.exp.xp.saturating_add(u64::from(amount));
    out.push(JournalKind::XpGained {
        source: source.to_owned(),
        amount,
        xp: u.exp.xp,
    });
    let mut levels_gained = 0;
    let mut points_granted = 0i32;
    while u.exp.level < config.max_level {
        match config.curve.total_for(u.exp.level + 1) {
            Some(need) if u.exp.xp >= need => {}
            _ => break,
        }
        u.exp.level += 1;
        levels_gained += 1;
        let points = config.points_per_level;
        // Same arithmetic as `npc_rules::grant_points` (never below 0).
        u.sheet.unspent = u.sheet.unspent.saturating_add(points).max(0);
        points_granted = points_granted.saturating_add(points);
        out.push(JournalKind::LevelUp {
            level: u.exp.level,
            points,
        });
    }
    if levels_gained > 0 && u.policy == SpendPolicy::Ai {
        ai_spend(rules, u, out);
    }
    XpAward {
        amount,
        xp: u.exp.xp,
        level: u.exp.level,
        levels_gained,
        points_granted,
    }
}

/// The AI spend policy: one skill point at a time, each on the used skill
/// with the highest `uses / (points + 1)` (cross-multiplied, ties by name),
/// until the bank is empty or nothing more can be bought. Journals one
/// `SkillImproved { via: "spend" }` per skill bought, in name order.
/// Returns the points bought per skill.
pub fn ai_spend(
    rules: &Rules<'_>,
    u: &mut UnitState<'_>,
    out: &mut Vec<JournalKind>,
) -> BTreeMap<String, i32> {
    let mut candidates: Vec<(String, u32)> = u
        .learning
        .skills
        .iter()
        .filter(|(name, p)| p.uses > 0 && rules.registry.skill(name).is_some())
        .map(|(name, p)| (name.clone(), p.uses))
        .collect();
    let mut bought: BTreeMap<String, i32> = BTreeMap::new();
    // Each round buys a point or drops a candidate: bounded.
    while u.sheet.unspent > 0 && !candidates.is_empty() {
        let mut best = 0;
        for i in 1..candidates.len() {
            let (ref a, ua) = candidates[i];
            let (ref b, ub) = candidates[best];
            let pa = u64::try_from(u.sheet.skill_points(a).max(0)).unwrap_or(0) + 1;
            let pb = u64::try_from(u.sheet.skill_points(b).max(0)).unwrap_or(0) + 1;
            // ua / pa > ub / pb, names already ascending (ties keep `best`).
            if u64::from(ua) * pb > u64::from(ub) * pa {
                best = i;
            }
        }
        let skill = candidates[best].0.clone();
        let purchase = Purchase::Skill {
            skill: skill.clone(),
            points: 1,
        };
        match rules.apply_purchase(u.sheet, &purchase) {
            Ok(cost) if cost > 0 => *bought.entry(skill).or_insert(0) += 1,
            _ => {
                candidates.remove(best);
            }
        }
    }
    for (skill, points) in &bought {
        out.push(JournalKind::SkillImproved {
            skill: skill.clone(),
            points: *points,
            level: rules.effective_skill(u.sheet, skill).unwrap_or(0),
            via: "spend".to_owned(),
        });
    }
    bought
}

fn progress_field(p: &mut SkillProgress, via: Via) -> &mut u32 {
    match via {
        Via::Practice => &mut p.practice,
        Via::Study => &mut p.study,
        Via::Teaching => &mut p.teaching,
    }
}

/// Accrues `ticks` of `via` on `skill` and converts every
/// `ticks_per_point` into a free skill point while the resulting effective
/// level stays `<= cap` (and within the rule set's skill point limit).
/// Journals `SkillImproved` when points were gained.
#[allow(clippy::too_many_arguments)]
pub fn learn(
    rules: &Rules<'_>,
    u: &mut UnitState<'_>,
    skill: &str,
    ticks: u32,
    via: Via,
    ticks_per_point: u32,
    cap: i32,
    out: &mut Vec<JournalKind>,
) -> Result<LearnOutcome, ProgressionError> {
    if rules.registry.skill(skill).is_none() {
        return Err(unknown_skill(rules, skill));
    }
    let tpp = ticks_per_point.max(1);
    let mut progress = *u.learning.skills.entry(skill.to_owned()).or_default();
    let field = progress_field(&mut progress, via);
    let mut acc = field.saturating_add(ticks);
    let mut points = 0;
    let mut capped = false;
    while acc >= tpp {
        let mut next = u.sheet.clone();
        let p = next.skill_points(skill).saturating_add(1);
        next.skills.insert(skill.to_owned(), p);
        let level = rules.effective_skill(&next, skill);
        let within = level.is_some_and(|l| l <= cap) && p <= rules.set.limits.skill_points_max;
        if !within {
            capped = true;
            acc = tpp;
            break;
        }
        *u.sheet = next;
        acc -= tpp;
        points += 1;
    }
    *field = acc;
    u.learning.skills.insert(skill.to_owned(), progress);
    let level = rules.effective_skill(u.sheet, skill);
    if points > 0 {
        out.push(JournalKind::SkillImproved {
            skill: skill.to_owned(),
            points,
            level: level.unwrap_or(0),
            via: via.name().to_owned(),
        });
    }
    Ok(LearnOutcome {
        skill: skill.to_owned(),
        via,
        points,
        level,
        capped,
        cap,
        progress: acc,
    })
}

/// The cap of practice (`via == Practice`) or study on `skill`: the
/// controlling attribute plus the configured margin.
pub fn stat_cap(
    rules: &Rules<'_>,
    config: &ProgressionConfig,
    sheet: &CharacterSheet,
    skill: &str,
    via: Via,
) -> Option<i32> {
    let def = rules.registry.skill(skill)?;
    let above = match via {
        Via::Practice => config.practice_cap_above_stat,
        Via::Study => config.study_cap_above_stat,
        Via::Teaching => return None,
    };
    Some(rules.stat(sheet, def.stat).saturating_add(above))
}

/// Processes one unit's rolls of this tick (in their order): every roll of
/// a registered skill counts a use and accrues `practice_per_roll`
/// practice; then the XP of every roll-driven source, summed per source,
/// is added (one `XpGained` per source).
pub fn apply_rolls(
    rules: &Rules<'_>,
    config: &ProgressionConfig,
    sources: &XpSources,
    u: &mut UnitState<'_>,
    rolls: &[&RollRecord],
    out: &mut Vec<JournalKind>,
) {
    let mut xp: BTreeMap<&'static str, u32> = BTreeMap::new();
    for r in rolls {
        let Some(def) = rules.registry.skill(&r.skill) else {
            continue;
        };
        let difficulty = Some(def.difficulty);
        let entry = u.learning.skills.entry(r.skill.clone()).or_default();
        entry.uses = entry.uses.saturating_add(1);
        if let Some(cap) = stat_cap(rules, config, u.sheet, &r.skill, Via::Practice) {
            let _ = learn(
                rules,
                u,
                &r.skill,
                config.practice_per_roll,
                Via::Practice,
                config.practice_ticks_per_point,
                cap,
                out,
            );
        }
        for src in sources.roll_sources() {
            let f = src.on_roll.expect("roll source");
            let amount = src.scale(f(config, r, difficulty));
            let e = xp.entry(src.name).or_insert(0);
            *e = e.saturating_add(amount);
        }
    }
    for (source, amount) in xp {
        if amount > 0 {
            add_xp(rules, config, u, amount, source, out);
        }
    }
}

// ------------------------------------------------------------ the system

/// `NpcSet::Cleanup`, before the journal flush (perf mechanic
/// `progression`): drains the rules' roll feed and applies every roll to
/// its unit (units without [`Experience`] are skipped), units in `SimId`
/// order, each unit's rolls in roll order.
#[allow(clippy::too_many_arguments)]
pub fn progression_system(
    clock: Res<SimClock>,
    config: Res<ProgressionConfig>,
    sources: Res<XpSources>,
    set: Res<RuleSet>,
    registry: Res<RulesRegistry>,
    index: Res<SimIndex>,
    mut counters: ResMut<RollCounters>,
    mut journal: Option<ResMut<Journal>>,
    mut units: Query<(
        &mut Experience,
        &mut Learning,
        &mut CharacterSheet,
        &SpendPolicy,
    )>,
) {
    if counters.feed.records.is_empty() {
        return;
    }
    let _span = tracing::info_span!("mechanic", name = "progression").entered();
    let mut records = counters.drain_feed();
    records.sort_by_key(|r| r.unit);
    let rules = Rules::new(&set, &registry);
    let mut i = 0;
    while i < records.len() {
        let unit = records[i].unit;
        let mut j = i;
        while j < records.len() && records[j].unit == unit {
            j += 1;
        }
        let rolls: Vec<&RollRecord> = records[i..j].iter().collect();
        i = j;
        let Some(entity) = index.get(unit) else {
            continue;
        };
        let Ok((mut exp, mut learning, mut sheet, &policy)) = units.get_mut(entity) else {
            continue;
        };
        let mut out = Vec::new();
        let mut state = UnitState {
            exp: &mut exp,
            learning: &mut learning,
            sheet: &mut sheet,
            policy,
        };
        apply_rolls(&rules, &config, &sources, &mut state, &rolls, &mut out);
        if let Some(j) = journal.as_deref_mut() {
            for kind in out {
                j.push(clock.tick, Some(unit), kind);
            }
        }
    }
}

// ------------------------------------------------------------- world API

fn entity_of(world: &World, unit: SimId) -> Result<Entity, ProgressionError> {
    world
        .get_resource::<SimIndex>()
        .and_then(|i| i.get(unit))
        .ok_or(ProgressionError::NoUnit { unit: unit.0 })
}

type Snapshot = (Experience, Learning, CharacterSheet, SpendPolicy);

fn snapshot(world: &World, unit: SimId) -> Result<(Entity, Snapshot), ProgressionError> {
    let e = entity_of(world, unit)?;
    let missing = ProgressionError::NotProgressing { unit: unit.0 };
    let exp = *world.get::<Experience>(e).ok_or(missing.clone())?;
    let learning = world.get::<Learning>(e).cloned().unwrap_or_default();
    let sheet = world.get::<CharacterSheet>(e).cloned().ok_or(missing)?;
    let policy = world.get::<SpendPolicy>(e).copied().unwrap_or_default();
    Ok((e, (exp, learning, sheet, policy)))
}

fn write_back(world: &mut World, unit: SimId, e: Entity, s: Snapshot, out: Vec<JournalKind>) {
    let (exp, learning, sheet, _) = s;
    let mut ent = world.entity_mut(e);
    if ent.get::<Experience>() != Some(&exp) {
        ent.insert(exp);
    }
    if ent.get::<Learning>() != Some(&learning) {
        ent.insert(learning);
    }
    if ent.get::<CharacterSheet>() != Some(&sheet) {
        ent.insert(sheet);
    }
    for kind in out {
        journal::record(world, Some(unit), || kind);
    }
}

fn config_of(world: &World) -> ProgressionConfig {
    world
        .get_resource::<ProgressionConfig>()
        .cloned()
        .unwrap_or_default()
}

/// Runs `f` on `unit`'s state with the world's rules and config, then
/// writes the state back and journals what `f` produced.
fn with_unit<R>(
    world: &mut World,
    unit: SimId,
    f: impl FnOnce(&Rules<'_>, &ProgressionConfig, &mut UnitState<'_>, &mut Vec<JournalKind>) -> R,
) -> Result<R, ProgressionError> {
    let (e, mut s) = snapshot(world, unit)?;
    let config = config_of(world);
    let mut out = Vec::new();
    let r = with_rules(world, |rules| {
        let mut state = UnitState {
            exp: &mut s.0,
            learning: &mut s.1,
            sheet: &mut s.2,
            policy: s.3,
        };
        f(&rules, &config, &mut state, &mut out)
    });
    write_back(world, unit, e, s, out);
    Ok(r)
}

/// Awards `amount` XP from the registered source `source` (scaled by its
/// `scale_pct`) to `unit`: levels, granted points and the AI spend follow
/// (see [`add_xp`]). For completed crafts, jobs, discoveries and any
/// content source. Not recorded as a command: call it from sim code
/// (systems, effects, hooks), not from tooling.
pub fn award_xp(
    world: &mut World,
    unit: SimId,
    amount: u32,
    source: &str,
) -> Result<XpAward, ProgressionError> {
    let def = world
        .get_resource::<XpSources>()
        .and_then(|s| s.get(source).copied())
        .ok_or_else(|| ProgressionError::UnknownSource {
            source: source.to_owned(),
            known: world
                .get_resource::<XpSources>()
                .map(XpSources::names)
                .unwrap_or_default(),
        })?;
    let amount = def.scale(amount);
    with_unit(world, unit, |rules, config, u, out| {
        add_xp(rules, config, u, amount, def.name, out)
    })
}

/// Practice ticks on `skill` (e.g. a completed craft's duration), capped at
/// the attribute + `practice_cap_above_stat`.
pub fn grant_practice(
    world: &mut World,
    unit: SimId,
    skill: &str,
    ticks: u32,
) -> Result<LearnOutcome, ProgressionError> {
    with_unit(world, unit, |rules, config, u, out| {
        let cap = stat_cap(rules, config, u.sheet, skill, Via::Practice)
            .ok_or_else(|| unknown_skill(rules, skill))?;
        learn(
            rules,
            u,
            skill,
            ticks,
            Via::Practice,
            config.practice_ticks_per_point,
            cap,
            out,
        )
    })?
}

/// Study ticks on `skill` (a `study` verb with a book, wired by facade
/// glue), capped at the attribute + `study_cap_above_stat` and at
/// `max_level` (the book's own limit) when given.
pub fn grant_study(
    world: &mut World,
    unit: SimId,
    skill: &str,
    ticks: u32,
    max_level: Option<i32>,
) -> Result<LearnOutcome, ProgressionError> {
    with_unit(world, unit, |rules, config, u, out| {
        let cap = stat_cap(rules, config, u.sheet, skill, Via::Study)
            .ok_or_else(|| unknown_skill(rules, skill))?;
        let cap = max_level.map_or(cap, |m| cap.min(m));
        learn(
            rules,
            u,
            skill,
            ticks,
            Via::Study,
            config.study_ticks_per_point,
            cap,
            out,
        )
    })?
}

/// Teaching ticks from `teacher` to `student` on `skill` (a `teach` verb,
/// wired by facade glue): the teacher needs a level of at least the
/// student's + `teaching_min_lead`, and the student cannot pass the
/// teacher's level. The teacher needs a `CharacterSheet`, not `Experience`.
pub fn grant_teaching(
    world: &mut World,
    teacher: SimId,
    student: SimId,
    skill: &str,
    ticks: u32,
) -> Result<LearnOutcome, ProgressionError> {
    if teacher == student {
        return Err(ProgressionError::SelfTeaching);
    }
    let te = entity_of(world, teacher)?;
    let teacher_sheet = world
        .get::<CharacterSheet>(te)
        .cloned()
        .ok_or(ProgressionError::NotProgressing { unit: teacher.0 })?;
    with_unit(world, student, |rules, config, u, out| {
        if rules.registry.skill(skill).is_none() {
            return Err(unknown_skill(rules, skill));
        }
        let teacher_level = rules.effective_skill(&teacher_sheet, skill).ok_or(
            ProgressionError::TeacherLacksSkill {
                teacher: teacher.0,
                skill: skill.to_owned(),
            },
        )?;
        if let Some(student_level) = rules.effective_skill(u.sheet, skill)
            && teacher_level < student_level.saturating_add(config.teaching_min_lead)
        {
            return Err(ProgressionError::TeacherTooWeak {
                teacher_level,
                student_level,
                min_lead: config.teaching_min_lead,
            });
        }
        learn(
            rules,
            u,
            skill,
            ticks,
            Via::Teaching,
            config.teaching_ticks_per_point,
            teacher_level,
            out,
        )
    })?
}

/// Total XP to reach `level` under the world's curve.
pub fn xp_for_level(world: &World, level: u32) -> Option<u64> {
    config_of(world).curve.total_for(level)
}
