//! Content definitions registered by stable name: skills, traits and sheet
//! templates ([`RulesRegistry`]).

use std::collections::BTreeMap;

use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::stats::{Attributes, Difficulty, SecondaryBuys, Stat};

/// One way to use a skill without points in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillDefault {
    /// A stat plus `modifier` (e.g. DX −5).
    Stat { stat: Stat, modifier: i32 },
    /// Another skill's level plus `modifier` (e.g. a related skill −3).
    Skill { skill: &'static str, modifier: i32 },
}

/// A skill, registered by name (`register_skill`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SkillDef {
    /// Stable name: sheets, templates, journal and `roll_skill` use it.
    pub name: &'static str,
    /// The stat the level is relative to.
    pub stat: Stat,
    pub difficulty: Difficulty,
    /// Defaults; the best one applies.
    pub defaults: Vec<SkillDefault>,
}

impl SkillDef {
    pub fn new(name: &'static str, stat: Stat, difficulty: Difficulty) -> Self {
        Self {
            name,
            stat,
            difficulty,
            defaults: Vec::new(),
        }
    }

    /// Adds a stat default (`stat + modifier`).
    pub fn default_stat(mut self, stat: Stat, modifier: i32) -> Self {
        self.defaults.push(SkillDefault::Stat { stat, modifier });
        self
    }

    /// Adds a skill default (`skill level + modifier`).
    pub fn default_skill(mut self, skill: &'static str, modifier: i32) -> Self {
        self.defaults.push(SkillDefault::Skill { skill, modifier });
        self
    }
}

/// What one level of a trait changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TraitModifier {
    /// `per_level` to a stat (attribute changes flow into derived values).
    Stat { stat: Stat, per_level: i32 },
    /// `per_level` to one skill's effective level.
    Skill { skill: &'static str, per_level: i32 },
}

/// An advantage (`points_per_level > 0`) or disadvantage (`< 0`),
/// registered by name (`register_trait`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraitDef {
    pub name: &'static str,
    /// Point value of each level (negative for a disadvantage).
    pub points_per_level: i32,
    /// Highest level (1 for a trait without levels).
    pub max_level: u8,
    pub modifiers: Vec<TraitModifier>,
}

impl TraitDef {
    pub fn new(name: &'static str, points_per_level: i32) -> Self {
        Self {
            name,
            points_per_level,
            max_level: 1,
            modifiers: Vec::new(),
        }
    }

    pub fn max_level(mut self, max: u8) -> Self {
        self.max_level = max;
        self
    }

    pub fn stat(mut self, stat: Stat, per_level: i32) -> Self {
        self.modifiers.push(TraitModifier::Stat { stat, per_level });
        self
    }

    pub fn skill(mut self, skill: &'static str, per_level: i32) -> Self {
        self.modifiers
            .push(TraitModifier::Skill { skill, per_level });
        self
    }
}

/// A starting sheet registered by name (`register_sheet_template`), applied
/// at spawn ([`apply_template`](crate::apply_template) /
/// [`template_sheet`](crate::template_sheet)).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SheetTemplate {
    pub name: &'static str,
    pub attributes: Attributes,
    pub secondary: SecondaryBuys,
    /// `(trait, level)`.
    pub traits: Vec<(&'static str, u8)>,
    /// `(skill, points)`.
    pub skills: Vec<(&'static str, i32)>,
    /// Banked character points.
    pub unspent: i32,
}

impl SheetTemplate {
    /// All attributes at 10, nothing else.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            attributes: Attributes::default(),
            secondary: SecondaryBuys::default(),
            traits: Vec::new(),
            skills: Vec::new(),
            unspent: 0,
        }
    }

    pub fn attributes(mut self, st: i32, dx: i32, iq: i32, ht: i32) -> Self {
        self.attributes = Attributes::new(st, dx, iq, ht);
        self
    }

    pub fn secondary(mut self, buys: SecondaryBuys) -> Self {
        self.secondary = buys;
        self
    }

    pub fn with_trait(mut self, name: &'static str, level: u8) -> Self {
        self.traits.push((name, level));
        self
    }

    pub fn skill(mut self, name: &'static str, points: i32) -> Self {
        self.skills.push((name, points));
        self
    }

    pub fn unspent(mut self, points: i32) -> Self {
        self.unspent = points;
        self
    }
}

/// Every registered skill, trait and template, keyed (so sorted) by name.
/// Frozen with the rest of the content (`ensure_frozen`): references are
/// checked then, and registering afterwards panics.
#[derive(Resource, Clone, Debug, Default)]
pub struct RulesRegistry {
    pub skills: BTreeMap<&'static str, SkillDef>,
    pub traits: BTreeMap<&'static str, TraitDef>,
    pub templates: BTreeMap<&'static str, SheetTemplate>,
    frozen: bool,
}

fn known<V>(map: &BTreeMap<&'static str, V>) -> String {
    let names: Vec<&str> = map.keys().copied().collect();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

impl RulesRegistry {
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    fn open(&self, what: &str) {
        assert!(
            !self.frozen,
            "cannot register {what}: rules content is already frozen"
        );
    }

    /// Panics on a duplicate name or after freeze.
    pub fn add_skill(&mut self, def: SkillDef) {
        self.open(&format!("skill `{}`", def.name));
        let name = def.name;
        assert!(
            self.skills.insert(name, def).is_none(),
            "skill `{name}` is registered twice"
        );
    }

    /// Panics on a duplicate name or after freeze.
    pub fn add_trait(&mut self, def: TraitDef) {
        self.open(&format!("trait `{}`", def.name));
        let name = def.name;
        assert!(
            self.traits.insert(name, def).is_none(),
            "trait `{name}` is registered twice"
        );
    }

    /// Panics on a duplicate name or after freeze.
    pub fn add_template(&mut self, t: SheetTemplate) {
        self.open(&format!("sheet template `{}`", t.name));
        let name = t.name;
        assert!(
            self.templates.insert(name, t).is_none(),
            "sheet template `{name}` is registered twice"
        );
    }

    pub fn skill(&self, name: &str) -> Option<&SkillDef> {
        self.skills.get(name)
    }

    pub fn trait_def(&self, name: &str) -> Option<&TraitDef> {
        self.traits.get(name)
    }

    pub fn template(&self, name: &str) -> Option<&SheetTemplate> {
        self.templates.get(name)
    }

    /// Known skill names, sorted (error messages).
    pub fn skill_names(&self) -> String {
        known(&self.skills)
    }

    /// Known trait names, sorted (error messages).
    pub fn trait_names(&self) -> String {
        known(&self.traits)
    }

    /// Known template names, sorted (error messages).
    pub fn template_names(&self) -> String {
        known(&self.templates)
    }

    /// Checks every reference and freezes (idempotent).
    ///
    /// # Panics
    /// On a skill default naming an unknown skill (or itself), a trait
    /// modifier naming an unknown skill, a trait with `max_level == 0`, or a
    /// template with an unknown skill/trait, non-positive skill points or a
    /// trait level outside `1..=max_level`. Messages list the known names.
    pub fn freeze(&mut self) {
        if self.frozen {
            return;
        }
        for def in self.skills.values() {
            for d in &def.defaults {
                if let SkillDefault::Skill { skill, .. } = d {
                    assert!(
                        *skill != def.name && self.skills.contains_key(skill),
                        "skill `{}` defaults from unknown skill `{skill}` (known: {})",
                        def.name,
                        self.skill_names()
                    );
                }
            }
        }
        for def in self.traits.values() {
            assert!(def.max_level >= 1, "trait `{}` has max_level 0", def.name);
            for m in &def.modifiers {
                if let TraitModifier::Skill { skill, .. } = m {
                    assert!(
                        self.skills.contains_key(skill),
                        "trait `{}` modifies unknown skill `{skill}` (known: {})",
                        def.name,
                        self.skill_names()
                    );
                }
            }
        }
        for t in self.templates.values() {
            for &(skill, points) in &t.skills {
                assert!(
                    self.skills.contains_key(skill),
                    "sheet template `{}` names unknown skill `{skill}` (known: {})",
                    t.name,
                    self.skill_names()
                );
                assert!(
                    points > 0,
                    "sheet template `{}`: skill `{skill}` needs > 0 points",
                    t.name
                );
            }
            for &(name, level) in &t.traits {
                let def = self.traits.get(name).unwrap_or_else(|| {
                    panic!(
                        "sheet template `{}` names unknown trait `{name}` (known: {})",
                        t.name,
                        self.trait_names()
                    )
                });
                assert!(
                    (1..=def.max_level).contains(&level),
                    "sheet template `{}`: trait `{name}` level {level} is outside 1..={}",
                    t.name,
                    def.max_level
                );
            }
        }
        self.frozen = true;
    }

    /// `(category, name)` of everything registered, for the content hash.
    pub fn manifest(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        out.extend(
            self.skills
                .keys()
                .map(|n| ("rules_skill".to_owned(), (*n).to_owned())),
        );
        out.extend(
            self.traits
                .keys()
                .map(|n| ("rules_trait".to_owned(), (*n).to_owned())),
        );
        out.extend(
            self.templates
                .keys()
                .map(|n| ("rules_template".to_owned(), (*n).to_owned())),
        );
        out
    }
}
