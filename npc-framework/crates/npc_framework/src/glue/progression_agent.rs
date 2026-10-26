//! Progression × agent-io (features `progression` + `agent-io`): experience
//! in observations.
//!
//! [`ProgressionAgentPlugin`] registers observation section
//! [`PROGRESSION_SECTION`] (`"progression"`) for units with `Experience`:
//! `npc_progression::experience_json` (`{"xp","level","next_level_xp",
//! "policy","skills":{name:{uses,practice,study,teaching}}}`) plus the
//! points per learning source (`"ticks_per_point"`), `"points_per_level"`
//! and `"spending"` (`"auto"` for `SpendPolicy::Ai`: at each level-up the
//! sheet's whole unspent bank is spent at once on the skills the unit used
//! most, or stays banked when it used none; `"bank"` for
//! `SpendPolicy::Player`: level-up points add to the sheet's unspent
//! points), with text lines that explain both. Banked points are spent with
//! the `spend_points` action of the rules × agent-io glue (under `Ai`, until
//! the next level-up spends them).

#![deny(
    clippy::float_arithmetic,
    clippy::float_cmp,
    clippy::iter_over_hash_type,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use npc_agent_io::{AgentCtx, AgentIoAppExt, Section};
use npc_progression::{Experience, Learning, ProgressionConfig, SpendPolicy, experience_json};
use serde_json::json;

/// Name of the section.
pub const PROGRESSION_SECTION: &str = "progression";

/// Section `progression` (see the module docs).
pub fn progression_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let exp = *world.get::<Experience>(ctx.entity)?;
    let mut value = experience_json(world, ctx.entity)?;
    let config = world
        .get_resource::<ProgressionConfig>()
        .cloned()
        .unwrap_or_default();
    if let Some(map) = value.as_object_mut() {
        map.insert(
            "ticks_per_point".into(),
            json!({
                "practice": config.practice_ticks_per_point,
                "study": config.study_ticks_per_point,
                "teaching": config.teaching_ticks_per_point,
            }),
        );
    }
    let next = value
        .get("next_level_xp")
        .and_then(serde_json::Value::as_u64);
    let policy_kind = world
        .get::<SpendPolicy>(ctx.entity)
        .copied()
        .unwrap_or_default();
    let per_level = config.points_per_level;
    let policy = match policy_kind {
        SpendPolicy::Ai => format!(
            "spent automatically ({per_level} per level, with the rest of the bank, on your most-used skills; policy Ai)"
        ),
        SpendPolicy::Player => format!(
            "banked for you ({per_level} per level; policy Player): spend them with spend_points"
        ),
    };
    if let Some(map) = value.as_object_mut() {
        map.insert("points_per_level".into(), json!(per_level));
        map.insert(
            "spending".into(),
            json!(match policy_kind {
                SpendPolicy::Ai => "auto",
                SpendPolicy::Player => "bank",
            }),
        );
    }
    let mut section = Section::new(value).line(match next {
        Some(n) => format!(
            "Level {} ({} XP; next level at {n} XP). Level-up points are {policy}.",
            exp.level, exp.xp
        ),
        None => format!(
            "Level {} ({} XP; highest level). Level-up points are {policy}.",
            exp.level, exp.xp
        ),
    });
    section = section.line(match policy_kind {
        SpendPolicy::Ai => "Unspent character points on your sheet are a bank: at each level-up \
             the whole bank is spent on the skills you used most (none used: it stays banked); \
             until then you may spend it yourself with spend_points."
            .to_owned(),
        SpendPolicy::Player => {
            "Level-up points add to the unspent character points on your sheet.".to_owned()
        }
    });
    if let Some(l) = world.get::<Learning>(ctx.entity) {
        let lines: Vec<String> = l
            .skills
            .iter()
            .map(|(name, p)| {
                format!(
                    "{name}: used {}x, practice {}/{}, study {}/{}, teaching {}/{}",
                    p.uses,
                    p.practice,
                    config.practice_ticks_per_point,
                    p.study,
                    config.study_ticks_per_point,
                    p.teaching,
                    config.teaching_ticks_per_point
                )
            })
            .collect();
        if !lines.is_empty() {
            section = section.line(format!("Learning: {}.", lines.join("; ")));
        }
    }
    Some(section)
}

/// Progression × agent-io glue (see the module docs). Add it with
/// `ProgressionPlugin` and `AgentIoPlugin`.
pub struct ProgressionAgentPlugin;

impl Plugin for ProgressionAgentPlugin {
    fn build(&self, app: &mut App) {
        app.register_observation(PROGRESSION_SECTION, progression_section);
    }
}
