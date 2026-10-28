//! Shared economic material and recipe catalog; NPCs and visitors use these same recipes.
use super::model::Role;
use bevy_app::App;
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
            .input("log", 1)
            .station(SAW)
            .duration(160)
            .output("plank", 4),
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
