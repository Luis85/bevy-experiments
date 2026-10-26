//! Progression × interaction (features `progression` + `interaction`):
//! learning from books and teachers (actor-capabilities spec §4.7).
//!
//! [`ProgressionInteractionPlugin`] registers two verbs (and their
//! components, hashed as `teaches` and `teacher`):
//!
//! - **`study`** (target: an entity with [`Teaches`], usually a book item,
//!   carried or within reach; claimed for the session): takes
//!   `Teaches::session_ticks`, then grants that many study ticks on
//!   `Teaches::skill` (`npc_progression::grant_study`, capped at the
//!   attribute + `study_cap_above_stat` and at `Teaches::max_level`).
//!   Refused (`why_not`) when the actor cannot learn (no `Experience` or no
//!   sheet), the skill is unknown, or the actor's level already reaches the
//!   book's maximum.
//! - **`teach`** (actor: a unit with [`Teacher`] and a sheet; target: the
//!   student, a unit with `Experience`; within reach, claimed): teaches the
//!   first of `Teacher::skills` (name order) the student can learn from this
//!   teacher, gated as `npc_progression::grant_teaching` is (the teacher
//!   needs the skill, at a level at least the student's +
//!   `teaching_min_lead`; the student never passes the teacher). Takes
//!   `Teacher::session_ticks`, then grants that many teaching ticks.
//!   Refused with the first skill's reason (`"teacher level 10 must be at
//!   least the student's 11 + 1"`).
//!
//! **Why the components live here.** `Teaches` / `Teacher` only make sense
//! with an interaction verb (and with progression's grants); defined in
//! `npc_progression` they would be data that crate cannot act on (ADR 0012
//! keeps it off `npc_interaction`) and would change its content hash for
//! everyone. Here they cost nothing unless both features are on and this
//! plugin is added.
//!
//! Skill checks of interactions (`InteractionDef::check`, `ToolTarget`
//! checks) already teach: with the interaction × rules glue they are
//! `roll_skill` rolls, which progression reads from the rules roll feed (a
//! use, roll practice, roll XP); nothing else is needed for them.

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_interaction::{Ctx, InteractionAppExt, InteractionDef};
use npc_kernel::SimId;
use npc_kernel::hash::{Fnv64, SimHash};
use npc_kernel::registry::KernelAppExt;
use npc_progression::{
    Experience, ProgressionConfig, ProgressionError, grant_study, grant_teaching,
};
use npc_rules::{CharacterSheet, RulesRegistry, effective_skill};
use serde::{Deserialize, Serialize};

/// The study verb.
pub const STUDY: &str = "study";
/// The teaching verb.
pub const TEACH: &str = "teach";

/// Something to study (a book, a manual, a training dummy): `skill` up to
/// `max_level`, `session_ticks` study ticks per session (the `study` verb's
/// duration). Hashed as `teaches`.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Teaches {
    pub skill: String,
    pub max_level: i32,
    pub session_ticks: u32,
}

impl Teaches {
    pub fn new(skill: impl Into<String>, max_level: i32, session_ticks: u32) -> Self {
        Self {
            skill: skill.into(),
            max_level,
            session_ticks,
        }
    }
}

impl SimHash for Teaches {
    const NAME: &'static str = "teaches";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.skill);
        h.i32(self.max_level);
        h.u32(self.session_ticks);
    }
}

/// A unit that teaches `skills` (sorted, unique), `session_ticks` teaching
/// ticks per session (the `teach` verb's duration). Hashed as `teacher`.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Teacher {
    pub skills: Vec<String>,
    pub session_ticks: u32,
}

impl Teacher {
    pub fn new<S: Into<String>>(skills: impl IntoIterator<Item = S>, session_ticks: u32) -> Self {
        let mut skills: Vec<String> = skills.into_iter().map(Into::into).collect();
        skills.sort_unstable();
        skills.dedup();
        Self {
            skills,
            session_ticks,
        }
    }
}

impl SimHash for Teacher {
    const NAME: &'static str = "teacher";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(u32::try_from(self.skills.len()).expect("too many skills"));
        for s in &self.skills {
            h.str(s);
        }
        h.u32(self.session_ticks);
    }
}

fn can_learn(world: &World, actor: Entity, id: SimId) -> Result<(), String> {
    if world.get::<Experience>(actor).is_none() || world.get::<CharacterSheet>(actor).is_none() {
        return Err(ProgressionError::NotProgressing { unit: id.0 }.to_string());
    }
    Ok(())
}

fn known_skill(world: &World, skill: &str) -> Result<(), String> {
    match world.get_resource::<RulesRegistry>() {
        Some(r) if r.skill(skill).is_some() => Ok(()),
        _ => Err(format!("unknown skill `{skill}`")),
    }
}

// ---------------------------------------------------------------- study

fn study_offer(world: &World, ctx: &Ctx) -> bool {
    world.get::<Teaches>(ctx.target).is_some()
}

fn study_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    let t = world
        .get::<Teaches>(ctx.target)
        .ok_or_else(|| format!("#{} teaches nothing", ctx.target_id.0))?;
    can_learn(world, ctx.actor, ctx.actor_id)?;
    known_skill(world, &t.skill)?;
    if let Some(level) = effective_skill(world, ctx.actor_id, &t.skill)
        && level >= t.max_level
    {
        return Err(format!(
            "already {} {level} (it teaches up to {})",
            t.skill, t.max_level
        ));
    }
    Ok(())
}

fn study_ticks(world: &World, ctx: &Ctx) -> u32 {
    world
        .get::<Teaches>(ctx.target)
        .map_or(0, |t| t.session_ticks)
}

fn study_effect(world: &mut World, ctx: &Ctx) {
    let Some(t) = world.get::<Teaches>(ctx.target).cloned() else {
        return;
    };
    // Journals `SkillImproved { via: "study" }` when a point is gained.
    let _ = grant_study(
        world,
        ctx.actor_id,
        &t.skill,
        t.session_ticks,
        Some(t.max_level),
    );
}

fn study_requires(world: &World, ctx: &Ctx) -> Vec<String> {
    world
        .get::<Teaches>(ctx.target)
        .map(|t| vec![format!("learns:{}", t.skill)])
        .unwrap_or_default()
}

/// The `study` verb.
pub fn study_verb() -> InteractionDef {
    InteractionDef::new(STUDY, study_pre, study_effect)
        .offer(study_offer)
        .duration_fn(study_ticks)
        .requires(study_requires)
}

// ---------------------------------------------------------------- teach

/// Whether `teacher` can teach `skill` to `student` now (`Err`: why not,
/// as `grant_teaching` would refuse).
pub fn teach_check(
    world: &World,
    teacher: SimId,
    student: SimId,
    skill: &str,
) -> Result<(), String> {
    if teacher == student {
        return Err(ProgressionError::SelfTeaching.to_string());
    }
    known_skill(world, skill)?;
    let t = effective_skill(world, teacher, skill).ok_or_else(|| {
        ProgressionError::TeacherLacksSkill {
            teacher: teacher.0,
            skill: skill.to_owned(),
        }
        .to_string()
    })?;
    let min_lead = world
        .get_resource::<ProgressionConfig>()
        .map_or(1, |c| c.teaching_min_lead);
    if let Some(s) = effective_skill(world, student, skill)
        && t < s.saturating_add(min_lead)
    {
        return Err(ProgressionError::TeacherTooWeak {
            teacher_level: t,
            student_level: s,
            min_lead,
        }
        .to_string());
    }
    Ok(())
}

/// The skill `ctx.actor` would teach `ctx.target`: the first of its
/// `Teacher::skills` that passes [`teach_check`]; else the first reason.
pub fn teach_skill(world: &World, ctx: &Ctx) -> Result<String, String> {
    let teacher = world
        .get::<Teacher>(ctx.actor)
        .ok_or_else(|| format!("#{} is not a teacher", ctx.actor_id.0))?;
    if world.get::<CharacterSheet>(ctx.actor).is_none() {
        return Err(format!("#{} has no character sheet", ctx.actor_id.0));
    }
    can_learn(world, ctx.target, ctx.target_id)?;
    let mut first = None;
    for skill in &teacher.skills {
        match teach_check(world, ctx.actor_id, ctx.target_id, skill) {
            Ok(()) => return Ok(skill.clone()),
            Err(why) => {
                first.get_or_insert(why);
            }
        }
    }
    Err(first.unwrap_or_else(|| "teaches no skills".into()))
}

fn teach_offer(world: &World, ctx: &Ctx) -> bool {
    ctx.actor != ctx.target
        && world.get::<Teacher>(ctx.actor).is_some()
        && world.get::<Experience>(ctx.target).is_some()
}

fn teach_pre(world: &World, ctx: &Ctx) -> Result<(), String> {
    teach_skill(world, ctx).map(|_| ())
}

fn teach_ticks(world: &World, ctx: &Ctx) -> u32 {
    world
        .get::<Teacher>(ctx.actor)
        .map_or(0, |t| t.session_ticks)
}

fn teach_effect(world: &mut World, ctx: &Ctx) {
    let Ok(skill) = teach_skill(world, ctx) else {
        return;
    };
    let ticks = teach_ticks(world, ctx);
    let _ = grant_teaching(world, ctx.actor_id, ctx.target_id, &skill, ticks);
}

fn teach_requires(world: &World, ctx: &Ctx) -> Vec<String> {
    teach_skill(world, ctx)
        .map(|s| vec![format!("teaches:{s}")])
        .unwrap_or_default()
}

/// The `teach` verb.
pub fn teach_verb() -> InteractionDef {
    InteractionDef::new(TEACH, teach_pre, teach_effect)
        .offer(teach_offer)
        .duration_fn(teach_ticks)
        .requires(teach_requires)
}

/// Progression × interaction glue (see the module docs). Add it with
/// `ProgressionPlugin` and `InteractionPlugin`.
pub struct ProgressionInteractionPlugin;

impl Plugin for ProgressionInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.register_sim_component::<Teaches>()
            .register_sim_component::<Teacher>()
            .register_interaction(study_verb())
            .register_interaction(teach_verb());
    }
}
