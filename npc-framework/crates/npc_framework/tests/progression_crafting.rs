//! Wave F3 through the facade: `with_progression()` / `with_crafting()` add
//! the capabilities (and what they build on), and the crafting completion
//! hook is enough for a host to award XP and practice (a host hook runs
//! next to the F4 glue's `npc_progression` hook; see `glue_learning.rs`).
//! Test-only content.

use npc_framework::crafting::{CraftCompleted, CraftSource, begin_craft};
use npc_framework::interaction::items::put_in_inventory;
use npc_framework::prelude::*;
use npc_framework::progression::{Learning, XP_CRAFT};

fn award(world: &mut bevy_ecs::world::World, done: &CraftCompleted) {
    if done.result.succeeded() {
        award_xp(world, done.unit, done.xp, XP_CRAFT).unwrap();
    }
    if let Some(skill) = &done.skill {
        grant_practice(world, done.unit, skill, done.duration_ticks).unwrap();
    }
}

#[test]
fn crafting_and_progression_through_the_facade() {
    let mut app = bevy_app::App::new();
    app.add_plugins(
        NpcPlugins::new()
            .seed(4)
            .without_driver()
            .with_progression()
            .with_crafting(),
    )
    .add_plugins(JournalPlugin::default())
    .register_item_kind(ItemKindDef::new("reed").stackable(20))
    .register_item_kind(ItemKindDef::new("basket"))
    .register_skill(SkillDef::new("Weaving", Stat::Dx, Difficulty::Easy).default_stat(Stat::Dx, -4))
    .register_recipe(
        RecipeDef::new("basket")
            .input("reed", 2)
            .skill("Weaving", 10)
            .duration(30)
            .output("basket", 1)
            .xp(40),
    )
    .add_craft_hook("test.progression", award);
    app.finish();
    app.cleanup();
    let world = app.world_mut();
    let mut sheet = CharacterSheet::default();
    sheet.skills.insert("Weaving".into(), 4);
    let (u, ue) = spawn_sim(
        world,
        (
            SimPos::new(0, 0),
            Interactor,
            Inventory::new(4, 100_000),
            sheet,
            Experience::default(),
        ),
    );
    let (_, reed) = spawn_sim(world, (Item::new("reed").stack(2), SimPos::new(0, 0)));
    put_in_inventory(world, ue, reed);
    begin_craft(world, ue, "basket", None, CraftSource::Command, None).unwrap();
    run_ticks(world, 31);
    let e = world.resource::<SimIndex>().get(u).unwrap();
    let exp = *world.get::<Experience>(e).unwrap();
    let learning = world.get::<Learning>(e).unwrap();
    assert_eq!(
        learning.uses("Weaving"),
        1,
        "the craft roll came through the feed"
    );
    assert!(
        learning.get("Weaving").practice >= 30,
        "roll practice + the hook's"
    );
    assert!(exp.xp >= 40, "craft XP (+ roll XP): {exp:?}");
}
