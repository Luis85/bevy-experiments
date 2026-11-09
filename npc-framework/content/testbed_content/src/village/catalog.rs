//! Shared economic material and recipe catalog; NPCs and visitors use these same recipes.
use super::model::Role;
use bevy_app::App;
use bevy_ecs::prelude::*;
use npc_framework::crafting::Durability;
use npc_framework::prelude::*;

pub const GRAIN: &str = "village_grain";
pub const FLOUR: &str = "village_flour";
pub const WATER: &str = "village_water";
pub const BOWL: &str = "village_bowl";
pub const DIARY: &str = "village_diary";
pub const COINS: &str = "village_coins";
pub const MILL: &str = "village_mill";
pub const BAKE: &str = "village_bake";
pub const SAW: &str = "village_saw";
pub const CARVE: &str = "village_carve";
pub const SMELT: &str = "village_smelt";
pub const TOOL: &str = "village_tool";
pub const INN: &str = "village_inn";
pub const RAW_MEAT: &str = "village_raw_meat";
pub const HIDE: &str = "village_hide";
pub const LEATHER: &str = "village_leather";
pub const FIBER: &str = "village_fiber";
pub const THREAD: &str = "village_thread";
pub const COOKED_MEAL: &str = "village_cooked_meal";
pub const WARM_CLOTHES: &str = "village_warm_clothes";
pub const SPEAR: &str = "village_spear";
pub const BERRIES: &str = "village_berries";
pub const COOK: &str = "village_cook";
pub const TAN: &str = "village_tan";
pub const SPIN: &str = "village_spin";
pub const TAILOR: &str = "village_tailor";
pub const FORGE_SPEAR: &str = "village_forge_spear";
pub const SAW_LOG_COST: u32 = 1;
pub const SAW_PLANK_YIELD: u32 = 4;
// Four full exposed cold days at one wear/tick: thirteen actors need 3.25
// replacements/day, below eight daily hides' four-coat material capacity.
// Shelter lowers actual wear; this does not create extra prey or materials.
pub const CLOTHING_DURABILITY: u32 = 96_000;
/// Explicit aliases also available in hosts that freeze content without running
/// Bevy's plugin-finish phase. Each delegates to the framework's shared handler.
pub const RECIPE_VERBS: &[&str] = &[
    "craft:village_mill",
    "craft:village_bake",
    "craft:village_saw",
    "craft:village_carve",
    "craft:village_smelt",
    "craft:village_tool",
];
pub const LIVING_RECIPE_VERBS: &[&str] = &[
    "craft:village_cook",
    "craft:village_tan",
    "craft:village_spin",
    "craft:village_tailor",
    "craft:village_forge_spear",
];
pub const BED_PRICE: u32 = 3;
pub const BED_TICKS: u64 = 300;
/// Public goods, sorted by stable key. Merchant demand is derived with [`wants`].
pub const TRADE_KINDS: &[&str] = &[
    "axe", "bread", "hammer", "ingot", "log", "ore", "pickaxe", "plank", BOWL, FLOUR, GRAIN, WATER,
];
/// The opt-in economy's public product directory, sorted by stable kind.
pub const LIVING_TRADE_KINDS: &[&str] = &[
    "axe",
    "bread",
    "hammer",
    "ingot",
    "log",
    "ore",
    "pickaxe",
    "plank",
    BERRIES,
    BOWL,
    COOKED_MEAL,
    FIBER,
    FLOUR,
    GRAIN,
    HIDE,
    LEATHER,
    RAW_MEAT,
    SPEAR,
    THREAD,
    WARM_CLOTHES,
    WATER,
];
pub fn register(app: &mut App) {
    for (kind, grams, stack) in [
        (GRAIN, 250, 50),
        (FLOUR, 250, 50),
        (WATER, 1000, 20),
        (BOWL, 200, 20),
        (DIARY, 300, 1),
        (COINS, 0, 1),
    ] {
        app.register_item_kind(ItemKindDef::new(kind).weight_g(grams).stackable(stack));
    }
    for r in [
        RecipeDef::new(MILL)
            .input(GRAIN, 1)
            .station(MILL)
            .duration(140)
            .output(FLOUR, 1),
        RecipeDef::new(BAKE)
            .input(FLOUR, 2)
            .input(WATER, 1)
            .input("log", 1)
            .station(BAKE)
            .duration(240)
            .output("bread", 4),
        RecipeDef::new(SAW)
            .input("log", SAW_LOG_COST)
            .station(SAW)
            .duration(160)
            .output("plank", SAW_PLANK_YIELD),
        RecipeDef::new(CARVE)
            .input("plank", 2)
            .station(SAW)
            .duration(200)
            .output(BOWL, 1),
        RecipeDef::new(SMELT)
            .input("ore", 2)
            .input("log", 1)
            .station(SMELT)
            .duration(240)
            .output("ingot", 2),
        RecipeDef::new(TOOL)
            .input("ingot", 2)
            .input("plank", 1)
            .station(SMELT)
            .duration(240)
            .output("hammer", 1),
    ] {
        app.register_recipe(r);
    }
}

/// Additional materials and joining production chains for the living economy.
/// The original island's registrations, seed stock and recipe names stay separate.
pub fn register_living(app: &mut App) {
    for (kind, grams, stack) in [
        (RAW_MEAT, 500, 20),
        (HIDE, 500, 20),
        (LEATHER, 400, 20),
        (FIBER, 100, 40),
        (THREAD, 100, 40),
        (COOKED_MEAL, 500, 10),
        (BERRIES, 100, 40),
    ] {
        app.register_item_kind(ItemKindDef::new(kind).weight_g(grams).stackable(stack));
    }
    app.register_item_kind(ItemKindDef::new(SPEAR).weight_g(1500).tag("tool:hunting"))
        .register_item_kind(
            ItemKindDef::new(WARM_CLOTHES)
                .weight_g(1500)
                .equip_slot("body"),
        );
    for recipe in [
        RecipeDef::new(FORGE_SPEAR)
            .input("ingot", 1)
            .input("plank", 1)
            .station(SMELT)
            .duration(200)
            .output(SPEAR, 1),
        RecipeDef::new(COOK)
            .input(RAW_MEAT, 2)
            .input(WATER, 1)
            .input("log", 1)
            .station(BAKE)
            .duration(200)
            .output(COOKED_MEAL, 2),
        RecipeDef::new(TAN)
            .input(HIDE, 2)
            .input(WATER, 1)
            .input(BOWL, 1)
            .station(TAN)
            .duration(240)
            .output(LEATHER, 2),
        RecipeDef::new(SPIN)
            .input(FIBER, 2)
            .station(TAILOR)
            .duration(140)
            .output(THREAD, 1),
        RecipeDef::new(TAILOR)
            .input(LEATHER, 2)
            .input(THREAD, 2)
            .station(TAILOR)
            .duration(320)
            .output(WARM_CLOTHES, 1),
    ] {
        app.register_recipe(recipe);
    }
    app.add_craft_hook("island.living_product_wear", |world, completed| {
        for id in &completed.outputs {
            let Some(entity) = world.resource::<SimIndex>().get(*id) else {
                continue;
            };
            if world
                .get::<Item>(entity)
                .is_some_and(|item| item.kind == WARM_CLOTHES)
            {
                world
                    .entity_mut(entity)
                    .insert(Durability::new(CLOTHING_DURABILITY));
            }
        }
    });
}

/// Hunger restored by a finished edible good. Raw meat is deliberately not food.
pub fn food_restore(kind: &str) -> Option<i32> {
    match kind {
        "bread" => Some(80_000),
        COOKED_MEAL => Some(90_000),
        BERRIES => Some(20_000),
        _ => None,
    }
}

pub fn trade_kinds(world: &World) -> &'static [&'static str] {
    if super::needs_driven(world) {
        LIVING_TRADE_KINDS
    } else {
        TRADE_KINDS
    }
}

pub fn outputs_for(world: &World, role: Role) -> &'static [&'static str] {
    if !super::needs_driven(world) {
        return outputs(role);
    }
    match role {
        Role::Farmer => &[GRAIN, FIBER, BERRIES],
        Role::Baker => &["bread", COOKED_MEAL],
        Role::Smith => &["ingot", "hammer", SPEAR],
        Role::Hunter => &[RAW_MEAT, HIDE],
        Role::Tanner => &[LEATHER],
        Role::Tailor => &[THREAD, WARM_CLOTHES],
        _ => outputs(role),
    }
}

pub fn wants_for(world: &World, role: Role, kind: &str) -> bool {
    if !super::needs_driven(world) {
        return wants(role, kind);
    }
    if matches!(kind, COOKED_MEAL | BERRIES | WARM_CLOTHES) {
        return true;
    }
    wants(role, kind)
        || match role {
            Role::Baker => kind == RAW_MEAT,
            Role::Hunter => kind == SPEAR,
            Role::Tanner => matches!(kind, HIDE | BOWL),
            Role::Tailor => matches!(kind, LEATHER | FIBER | THREAD),
            _ => false,
        }
}

/// Candidate jobs; the NPC planner chooses from current backed demand, not this order.
pub fn recipes_for(world: &World, role: Role) -> &'static [&'static str] {
    if super::needs_driven(world) {
        match role {
            Role::Baker => return &[BAKE, COOK],
            Role::Smith => return &[SMELT, TOOL, FORGE_SPEAR],
            Role::Tanner => return &[TAN],
            Role::Tailor => return &[SPIN, TAILOR],
            _ => {}
        }
    }
    match role {
        Role::Miller => &[MILL],
        Role::Baker => &[BAKE],
        Role::Carpenter => &[SAW, CARVE],
        Role::Smith => &[SMELT, TOOL],
        _ => &[],
    }
}
pub fn price(kind: &str) -> Option<u32> {
    Some(match kind {
        GRAIN => 3,
        FLOUR => 6,
        WATER => 1,
        "log" => 3,
        "plank" => 2,
        "ore" => 3,
        "ingot" => 6,
        "bread" => 6,
        BOWL => 8,
        "hammer" | "axe" | "pickaxe" => 18,
        RAW_MEAT => 5,
        HIDE => 4,
        LEATHER => 10,
        FIBER => 1,
        THREAD => 4,
        COOKED_MEAL => 9,
        WARM_CLOTHES => 34,
        SPEAR => 10,
        BERRIES => 2,
        _ => return None,
    })
}
pub fn stock_limit(kind: &str) -> u32 {
    match kind {
        WATER | GRAIN | FLOUR => 40,
        "bread" => 24,
        "log" | "ore" | "plank" => 20,
        "ingot" => 12,
        BOWL => 6,
        "hammer" | "axe" | "pickaxe" => 3,
        RAW_MEAT | HIDE | LEATHER => 16,
        FIBER | BERRIES => 40,
        THREAD | COOKED_MEAL => 24,
        WARM_CLOTHES => 4,
        SPEAR => 8,
        _ => 0,
    }
}
pub fn outputs(role: Role) -> &'static [&'static str] {
    match role {
        Role::Farmer => &[GRAIN],
        Role::Miller => &[FLOUR],
        Role::Baker => &["bread"],
        Role::WaterKeeper => &[WATER],
        Role::Woodcutter => &["log"],
        Role::Carpenter => &["plank", BOWL],
        Role::Miner => &["ore"],
        Role::Smith => &["ingot", "hammer"],
        _ => &[],
    }
}
pub fn wants(role: Role, kind: &str) -> bool {
    matches!(kind, "bread")
        || kind == WATER
        || match role {
            Role::Miller => kind == GRAIN,
            Role::Baker => matches!(kind, FLOUR | "log"),
            Role::Carpenter => matches!(kind, "log" | "plank"),
            Role::Smith => matches!(kind, "ore" | "log" | "ingot" | "plank"),
            Role::Innkeeper => matches!(kind, "log" | "hammer" | BOWL),
            _ => false,
        }
}
pub fn recipe(role: Role, secondary: bool) -> Option<&'static str> {
    match role {
        Role::Miller => Some(MILL),
        Role::Baker => Some(BAKE),
        Role::Carpenter => Some(if secondary { CARVE } else { SAW }),
        Role::Smith => Some(if secondary { TOOL } else { SMELT }),
        _ => None,
    }
}
