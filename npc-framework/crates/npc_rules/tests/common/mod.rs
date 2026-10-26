//! Test-only rules content (wave D2): a few skills, traits and templates
//! with neutral names. Not game content; the crate ships none.

#![allow(dead_code)]

use bevy_app::App;
use npc_kernel::{JournalPlugin, NpcKernelPlugin};
use npc_rules::{Difficulty, RulesAppExt, RulesPlugin, SheetTemplate, SkillDef, Stat, TraitDef};

pub fn register(app: &mut App) {
    app.register_skill(
        SkillDef::new("Climbing", Stat::Dx, Difficulty::Average).default_stat(Stat::Dx, -5),
    )
    .register_skill(
        SkillDef::new("Lockpicking", Stat::Dx, Difficulty::Average).default_stat(Stat::Iq, -5),
    )
    .register_skill(
        SkillDef::new("Axe", Stat::Dx, Difficulty::Average)
            .default_stat(Stat::Dx, -5)
            .default_skill("Mace", -3),
    )
    .register_skill(
        SkillDef::new("Mace", Stat::Dx, Difficulty::Average)
            .default_stat(Stat::Dx, -5)
            .default_skill("Axe", -3),
    )
    .register_skill(
        SkillDef::new("Shield Bash", Stat::Dx, Difficulty::Easy).default_skill("Mace", -2),
    )
    .register_skill(SkillDef::new("Alchemy", Stat::Iq, Difficulty::VeryHard))
    .register_skill(
        SkillDef::new("Herbalism", Stat::Iq, Difficulty::Hard)
            .default_stat(Stat::Iq, -6)
            .default_skill("Alchemy", -3),
    )
    .register_skill(
        SkillDef::new("Smithing", Stat::Iq, Difficulty::Average).default_stat(Stat::Iq, -5),
    )
    .register_skill(SkillDef::new("Running", Stat::Ht, Difficulty::Easy).default_stat(Stat::Ht, -4))
    .register_trait(
        TraitDef::new("Keen Senses", 2)
            .max_level(4)
            .stat(Stat::Per, 1),
    )
    .register_trait(
        TraitDef::new("Strong Back", 10)
            .max_level(3)
            .stat(Stat::St, 1),
    )
    .register_trait(TraitDef::new("Graceful", 20).stat(Stat::Dx, 1))
    .register_trait(
        TraitDef::new("Nimble Climber", 5)
            .max_level(2)
            .skill("Climbing", 1),
    )
    .register_trait(TraitDef::new("Butterfingers", -10).skill("Lockpicking", -3))
    .register_sheet_template(
        SheetTemplate::new("scout")
            .attributes(10, 12, 11, 11)
            .with_trait("Keen Senses", 2)
            .skill("Climbing", 4)
            .unspent(10),
    )
    .register_sheet_template(
        SheetTemplate::new("smith")
            .attributes(13, 10, 11, 12)
            .with_trait("Strong Back", 1)
            .skill("Smithing", 8)
            .skill("Axe", 2),
    );
}

/// Kernel + rules + journal + the test content, frozen.
pub fn app(seed: u64) -> App {
    let mut app = App::new();
    app.add_plugins((
        NpcKernelPlugin {
            seed,
            ..Default::default()
        },
        RulesPlugin,
        JournalPlugin::default(),
    ));
    register(&mut app);
    app.finish();
    app.cleanup();
    app
}
