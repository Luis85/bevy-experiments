//! The testbed's registered content, by stable name: item kinds, skills,
//! traits, sheet templates and recipes. Nothing here lives in a core crate.
//!
//! | Item kind | Weight | Stack | Tags | Slot |
//! |---|---|---|---|---|
//! | `axe` | 1500 g | 1 | `tool:axe` | main hand |
//! | `pickaxe` | 2000 g | 1 | `tool:pickaxe` | main hand |
//! | `hammer` | 800 g | 1 | `tool:hammer` | main hand |
//! | `log` | 3000 g | 10 | `material:wood` | |
//! | `plank` | 800 g | 20 | `material:wood`, `material:plank` | |
//! | `ore` | 2500 g | 10 | `material:ore` | |
//! | `ingot` | 1500 g | 10 | `material:iron` | |
//! | `bread` | 300 g | 5 | `food` | |
//! | `book` | 800 g | 1 | `book` | |
//! | `scroll` | 100 g | 1 | `book`, `scroll` | |
//! | `store_key` | 50 g | 1 | `key:store` | |
//!
//! Skills: `Carpentry` (DX, Average), `Smithing` (IQ, Average), `Woodcutting`
//! (ST, Easy), `Mining` (ST, Average), `Stealth` (DX, Average); each defaults
//! to its attribute −5 (Easy −4). Traits: `Keen Eyes` (+1 Per per level, 5
//! points, max 3), `Clumsy` (−1 DX, −5 points). Templates: see
//! [`TEMPLATES`]. Recipes: `planks`, `ingot`, `forge_axe`, `forge_pickaxe`.
//!
//! Learning (the progression × interaction glue): every `book` item
//! [`Teaches`] Carpentry and every `scroll` Smithing ([`TEACHES`], the
//! `study` verb), and units made from the `smith` template are a
//! [`Teacher`] of Smithing ([`TEACHERS`], the `teach` verb).

use bevy_app::App;
use npc_framework::glue::{Teacher, Teaches};
use npc_framework::interaction::MAIN_HAND;
use npc_framework::prelude::*;

pub const AXE: &str = "axe";
pub const PICKAXE: &str = "pickaxe";
pub const HAMMER: &str = "hammer";
pub const LOG: &str = "log";
pub const PLANK: &str = "plank";
pub const ORE: &str = "ore";
pub const INGOT: &str = "ingot";
pub const BREAD: &str = "bread";
pub const BOOK: &str = "book";
pub const SCROLL: &str = "scroll";
pub const STORE_KEY: &str = "store_key";
/// The key tag of the storeroom lock.
pub const KEY_STORE: &str = "key:store";

pub const TOOL_AXE: &str = "tool:axe";
pub const TOOL_PICKAXE: &str = "tool:pickaxe";
pub const TOOL_HAMMER: &str = "tool:hammer";

pub const CARPENTRY: &str = "Carpentry";
pub const SMITHING: &str = "Smithing";
pub const WOODCUTTING: &str = "Woodcutting";
pub const MINING: &str = "Mining";
pub const STEALTH: &str = "Stealth";

pub const KEEN_EYES: &str = "Keen Eyes";
pub const CLUMSY: &str = "Clumsy";

/// Workstation kinds.
pub const WORKBENCH: &str = "workbench";
pub const ANVIL: &str = "anvil";
pub const FORGE: &str = "forge";

/// Recipes.
pub const RECIPE_PLANKS: &str = "planks";
pub const RECIPE_INGOT: &str = "ingot";
pub const RECIPE_FORGE_AXE: &str = "forge_axe";
pub const RECIPE_FORGE_PICKAXE: &str = "forge_pickaxe";

/// Sheet templates, faction A (adventurers) then faction B (skeletons) and
/// the plain `novice`.
pub const TEMPLATES: [&str; 7] = [
    "carpenter",
    "smith",
    "woodcutter",
    "scout",
    "skeleton_warrior",
    "skeleton_minion",
    "novice",
];

/// What an item kind teaches (`Teaches` on every item of that kind, the
/// `study` verb): `(kind, skill, max level, study ticks per session)`.
/// Study grants 200 ticks per point, so one session is one point (capped at
/// the attribute + 4 and at the max level).
pub const TEACHES: [(&str, &str, i32, u32); 2] =
    [(BOOK, CARPENTRY, 12, 200), (SCROLL, SMITHING, 12, 200)];

/// Templates whose units teach (`Teacher`, the `teach` verb): `(template,
/// skills, teaching ticks per session)`. Teaching grants 100 ticks per
/// point; the teacher must be at least the student's level + 1.
pub const TEACHERS: [(&str, &[&str], u32); 1] = [("smith", &[SMITHING], 100)];

/// The `Teaches` of an item of `kind` ([`TEACHES`]).
pub fn teaches(kind: &str) -> Option<Teaches> {
    TEACHES
        .iter()
        .find(|t| t.0 == kind)
        .map(|&(_, skill, max, ticks)| Teaches::new(skill, max, ticks))
}

/// The `Teacher` of a unit made from `template` ([`TEACHERS`]).
pub fn teacher(template: &str) -> Option<Teacher> {
    TEACHERS
        .iter()
        .find(|t| t.0 == template)
        .map(|&(_, skills, ticks)| Teacher::new(skills.iter().copied(), ticks))
}

pub fn register_items(app: &mut App) {
    let tool = |name: &str, grams, tag: &str| {
        ItemKindDef::new(name)
            .weight_g(grams)
            .tag(tag)
            .equip_slot(MAIN_HAND)
    };
    app.register_item_kind(tool(AXE, 1500, TOOL_AXE))
        .register_item_kind(tool(PICKAXE, 2000, TOOL_PICKAXE))
        .register_item_kind(tool(HAMMER, 800, TOOL_HAMMER))
        .register_item_kind(
            ItemKindDef::new(LOG)
                .weight_g(3000)
                .stackable(10)
                .tag("material:wood"),
        )
        .register_item_kind(
            ItemKindDef::new(PLANK)
                .weight_g(800)
                .stackable(20)
                .tag("material:wood")
                .tag("material:plank"),
        )
        .register_item_kind(
            ItemKindDef::new(ORE)
                .weight_g(2500)
                .stackable(10)
                .tag("material:ore"),
        )
        .register_item_kind(
            ItemKindDef::new(INGOT)
                .weight_g(1500)
                .stackable(10)
                .tag("material:iron"),
        )
        .register_item_kind(
            ItemKindDef::new(BREAD)
                .weight_g(300)
                .stackable(5)
                .tag("food"),
        )
        .register_item_kind(ItemKindDef::new(BOOK).weight_g(800).tag("book"))
        .register_item_kind(
            ItemKindDef::new(SCROLL)
                .weight_g(100)
                .tag("book")
                .tag("scroll"),
        )
        .register_item_kind(ItemKindDef::new(STORE_KEY).weight_g(50).tag(KEY_STORE));
}

pub fn register_rules(app: &mut App) {
    app.register_skill(
        SkillDef::new(CARPENTRY, Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
    )
    .register_skill(
        SkillDef::new(SMITHING, Stat::Iq, Difficulty::Average).default_stat(Stat::Iq, -5),
    )
    .register_skill(
        SkillDef::new(WOODCUTTING, Stat::St, Difficulty::Easy).default_stat(Stat::St, -4),
    )
    .register_skill(SkillDef::new(MINING, Stat::St, Difficulty::Average).default_stat(Stat::St, -5))
    .register_skill(
        SkillDef::new(STEALTH, Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
    )
    .register_trait(TraitDef::new(KEEN_EYES, 5).max_level(3).stat(Stat::Per, 1))
    .register_trait(TraitDef::new(CLUMSY, -5).max_level(1).stat(Stat::Dx, -1));

    // Faction A: specialists.
    app.register_sheet_template(
        SheetTemplate::new("carpenter")
            .attributes(10, 12, 11, 10)
            .skill(CARPENTRY, 8)
            .skill(WOODCUTTING, 1)
            .unspent(5),
    )
    .register_sheet_template(
        SheetTemplate::new("smith")
            .attributes(13, 10, 11, 12)
            .skill(SMITHING, 8)
            .skill(MINING, 2),
    )
    .register_sheet_template(
        SheetTemplate::new("woodcutter")
            .attributes(12, 11, 10, 11)
            .skill(WOODCUTTING, 4)
            .skill(CARPENTRY, 1),
    )
    .register_sheet_template(
        SheetTemplate::new("scout")
            .attributes(10, 12, 10, 10)
            .with_trait(KEEN_EYES, 2)
            .skill(STEALTH, 4)
            .unspent(10),
    )
    // Faction B: skeletons.
    .register_sheet_template(
        SheetTemplate::new("skeleton_warrior")
            .attributes(12, 11, 9, 11)
            .skill(MINING, 1)
            .skill(STEALTH, 1),
    )
    .register_sheet_template(
        SheetTemplate::new("skeleton_minion")
            .attributes(9, 10, 8, 10)
            .with_trait(CLUMSY, 1)
            .skill(STEALTH, 2),
    )
    // Everything at 10, no skills (defaults only).
    .register_sheet_template(SheetTemplate::new("novice"));
}

pub fn register_recipes(app: &mut App) {
    app.register_recipe(
        RecipeDef::new(RECIPE_PLANKS)
            .input(LOG, 1)
            .station(WORKBENCH)
            .skill(CARPENTRY, 0)
            .duration(40)
            .output(PLANK, 2)
            .xp(10),
    )
    .register_recipe(
        RecipeDef::new(RECIPE_INGOT)
            .input(ORE, 2)
            .station(FORGE)
            .skill(SMITHING, 0)
            .duration(80)
            .output(INGOT, 1)
            .xp(15),
    )
    .register_recipe(
        RecipeDef::new(RECIPE_FORGE_AXE)
            .input(INGOT, 1)
            .input(PLANK, 1)
            .tool(TOOL_HAMMER)
            .station(ANVIL)
            .skill(SMITHING, -2)
            .duration(120)
            .output(AXE, 1)
            .xp(30),
    )
    .register_recipe(
        RecipeDef::new(RECIPE_FORGE_PICKAXE)
            .input(INGOT, 2)
            .input(PLANK, 1)
            .tool(TOOL_HAMMER)
            .station(ANVIL)
            .skill(SMITHING, -3)
            .duration(150)
            .output(PICKAXE, 1)
            .xp(40),
    );
}
