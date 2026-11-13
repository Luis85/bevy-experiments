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
pub const FISH: &str = "village_fish";
pub const FISHING_ROD: &str = "village_fishing_rod";
pub const MAKE_ROD: &str = "village_make_rod";
pub const COOK_FISH: &str = "village_cook_fish";
pub const WEAVE_COAT: &str = "village_weave_coat";
pub const HOE: &str = "village_hoe";
pub const FORGE_AXE: &str = "village_forge_axe";
pub const FORGE_PICKAXE: &str = "village_forge_pickaxe";
pub const FORGE_HOE: &str = "village_forge_hoe";
/// Completed gathers one regional work tool lasts before it breaks.
pub const TOOL_DURABILITY: u32 = 120;
/// Gathering duration with / without the matching tool, as a percentage of the
/// base ticks of the patch. Bare hands stay possible so a new world cannot deadlock.
pub const TOOL_GATHER_PERCENT: u32 = 75;
pub const BARE_GATHER_PERCENT: u32 = 150;

/// The work tool that speeds gathering this raw good in the regional profile.
pub fn gather_tool(kind: &str) -> Option<&'static str> {
    match kind {
        "log" => Some("axe"),
        "ore" => Some("pickaxe"),
        GRAIN => Some(HOE),
        _ => None,
    }
}

/// The work tool a regional gathering profession keeps one of.
pub fn role_tool(role: Role) -> Option<&'static str> {
    match role {
        Role::Woodcutter => Some("axe"),
        Role::Miner => Some("pickaxe"),
        Role::Farmer => Some(HOE),
        _ => None,
    }
}
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
pub const FRONTIER_TRADE_KINDS: &[&str] = &[
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
    FISH,
    FISHING_ROD,
    FLOUR,
    GRAIN,
    HIDE,
    HOE,
    LEATHER,
    RAW_MEAT,
    SPEAR,
    THREAD,
    WARM_CLOTHES,
    WATER,
];

fn frontier_profile(app: &App) -> bool {
    app.world()
        .contains_resource::<super::frontier::economy_spec::EconomyRegistry>()
}

/// The regional profile registers every good and recipe of its `economy` data
/// block; the Rust tables below serve the older profiles only.
pub fn register_frontier(app: &mut App) {
    use npc_framework::agent_io::AgentIoAppExt;
    let spec = app
        .world()
        .resource::<super::frontier::economy_spec::EconomyRegistry>()
        .spec
        .clone();
    for good in &spec.goods {
        let Some(item) = &good.item else {
            continue;
        };
        let mut kind = ItemKindDef::new(&good.key).weight_g(item.weight_g);
        if item.max_stack > 1 {
            kind = kind.stackable(item.max_stack);
        }
        for tag in &item.tags {
            kind = kind.tag(tag);
        }
        if let Some(slot) = &item.equip_slot {
            kind = kind.equip_slot(slot);
        }
        app.register_item_kind(kind);
    }
    for recipe in &spec.recipes {
        let mut def = RecipeDef::new(&recipe.key)
            .station(&recipe.station)
            .duration(recipe.duration);
        for input in &recipe.inputs {
            def = def.input(&input.good, input.count);
        }
        for output in &recipe.outputs {
            def = def.output(&output.good, output.count);
        }
        if let Some(tool) = &recipe.tool {
            def = def.tool(tool);
        }
        if let Some(skill) = &recipe.skill {
            def = def.skill(&skill.name, skill.level);
        }
        app.register_recipe(def);
        app.register_action_handler(
            super::frontier::economy_spec::intern(&format!("craft:{}", recipe.key)),
            npc_framework::glue::crafting_agent::craft_handler,
        );
    }
    app.add_craft_hook("island.frontier_product_wear", |world, completed| {
        for id in &completed.outputs {
            let Some(entity) = world.resource::<SimIndex>().get(*id) else {
                continue;
            };
            let durability = world
                .get::<Item>(entity)
                .and_then(|item| durability_in(world, &item.kind));
            if let Some(durability) = durability {
                world.entity_mut(entity).insert(Durability::new(durability));
            }
        }
    });
}

/// Uses a freshly made or granted unit of this good lasts; regional data only.
pub fn durability_in(world: &World, kind: &str) -> Option<u32> {
    super::frontier::economy_spec::registry(world)?
        .good(kind)?
        .durability
}
/// Seed price of a good in this world.
pub fn price_in(world: &World, kind: &str) -> Option<u32> {
    match super::frontier::economy_spec::registry(world) {
        Some(registry) => registry.good(kind).map(|good| good.base_price),
        None => price(kind),
    }
}
pub fn stock_limit_in(world: &World, kind: &str) -> u32 {
    match super::frontier::economy_spec::registry(world) {
        Some(registry) => registry.good(kind).map_or(0, |good| good.stock_limit),
        None => stock_limit(kind),
    }
}
/// Hunger restored by a finished edible good in this world.
pub fn food_restore_in(world: &World, kind: &str) -> Option<i32> {
    match super::frontier::economy_spec::registry(world) {
        Some(registry) => registry.good(kind).and_then(|good| good.food_value),
        None => food_restore(kind),
    }
}
pub fn gather_tool_in(world: &World, kind: &str) -> Option<&'static str> {
    match super::frontier::economy_spec::registry(world) {
        Some(registry) => registry.good(kind).and_then(|good| good.gather_tool),
        None => gather_tool(kind),
    }
}
pub fn role_tool_in(world: &World, role: Role) -> Option<&'static str> {
    match super::frontier::economy_spec::registry(world) {
        Some(registry) => registry.profession(role).and_then(|entry| entry.tool),
        None => role_tool(role),
    }
}
/// Living world: the first good the world's data tags with `tag` (its
/// coats are the `Clothing` good, its fuel the `Fuel` good, ...), so logic
/// reads a good's role from data instead of naming the good.
pub fn good_with_tag(world: &World, tag: super::frontier::economy_spec::GoodTag) -> Option<&str> {
    super::frontier::economy_spec::registry(world)?
        .spec
        .goods
        .iter()
        .find(|good| good.tags.contains(&tag))
        .map(|good| good.key.as_str())
}

/// Living world: whether the world's data tags `kind` with `tag`.
pub fn has_tag(world: &World, kind: &str, tag: super::frontier::economy_spec::GoodTag) -> bool {
    super::frontier::economy_spec::registry(world).is_some_and(|registry| {
        registry
            .spec
            .goods
            .iter()
            .any(|good| good.key == kind && good.tags.contains(&tag))
    })
}

/// Lower-case display noun of a good, when the world's data names it.
pub fn display_in(world: &World, kind: &str) -> Option<&'static str> {
    super::frontier::economy_spec::registry(world)?
        .good(kind)
        .map(|good| good.display)
}
pub fn register(app: &mut App) {
    let data_driven = frontier_profile(app);
    for (kind, grams, stack) in [
        (GRAIN, 250, 50),
        (FLOUR, 250, 50),
        (WATER, 1000, 20),
        (BOWL, 200, 20),
        (DIARY, 300, 1),
        (COINS, 0, 1),
    ] {
        if data_driven && !matches!(kind, DIARY | COINS) {
            continue;
        }
        app.register_item_kind(ItemKindDef::new(kind).weight_g(grams).stackable(stack));
    }
    if data_driven {
        return;
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
    if frontier_profile(app) {
        return;
    }
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
    if let Some(registry) = super::frontier::economy_spec::registry(world) {
        registry.trade_kinds()
    } else if super::needs_driven(world) {
        LIVING_TRADE_KINDS
    } else {
        TRADE_KINDS
    }
}

pub fn outputs_for(world: &World, role: Role) -> &'static [&'static str] {
    if let Some(registry) = super::frontier::economy_spec::registry(world) {
        return registry.profession(role).map_or(&[], |entry| entry.outputs);
    }
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
    if let Some(registry) = super::frontier::economy_spec::registry(world) {
        // Someone without a profession (a visitor) still accepts the staples.
        return match registry.profession(role) {
            Some(entry) => entry.wants.contains(&kind),
            None => registry.good(kind).is_some_and(|good| good.staple),
        };
    }
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
    if let Some(registry) = super::frontier::economy_spec::registry(world) {
        return registry.profession(role).map_or(&[], |entry| entry.recipes);
    }
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
        FISH => 4,
        FISHING_ROD => 12,
        HOE => 14,
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
        FISH => 20,
        FISHING_ROD => 4,
        HOE => 3,
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
