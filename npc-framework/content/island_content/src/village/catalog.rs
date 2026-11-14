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
/// Coins a night in an inn bed costs (`tuning.inn_bed_price`).
pub fn bed_price(world: &World) -> u32 {
    super::frontier::economy_spec::tuning(world).map_or(3, |tuning| tuning.inn_bed_price)
}
/// Ticks one rented night lasts (`tuning.inn_bed_ticks`).
pub fn bed_ticks(world: &World) -> u64 {
    super::frontier::economy_spec::tuning(world).map_or(300, |tuning| tuning.inn_bed_ticks)
}
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
/// Seed price of a good in this world's data.
pub fn price_in(world: &World, kind: &str) -> Option<u32> {
    super::frontier::economy_spec::registry(world)?
        .good(kind)
        .map(|good| good.base_price)
}
/// Hunger restored by a finished edible good in this world's data.
pub fn food_restore_in(world: &World, kind: &str) -> Option<i32> {
    super::frontier::economy_spec::registry(world)?
        .good(kind)
        .and_then(|good| good.food_value)
}
/// The work tool that speeds gathering `kind`, from this world's data.
pub fn gather_tool_in(world: &World, kind: &str) -> Option<&'static str> {
    super::frontier::economy_spec::registry(world)?
        .good(kind)
        .and_then(|good| good.gather_tool)
}
/// The tool without which `kind` cannot be gathered at all (a rod for fish).
pub fn gather_tool_required_in(world: &World, kind: &str) -> Option<&'static str> {
    super::frontier::economy_spec::registry(world)?
        .good(kind)
        .filter(|good| good.gather_tool_required)
        .and_then(|good| good.gather_tool)
}
/// The work tool a profession keeps one of, from this world's data.
pub fn role_tool_in(world: &World, role: Role) -> Option<&'static str> {
    super::frontier::economy_spec::registry(world)?
        .profession(role)
        .and_then(|entry| entry.tool)
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
    // The data-driven economy registers its goods and recipes
    // (`register_frontier`); the diary and the coin purse are the island's own.
    for (kind, grams, stack) in [(DIARY, 300, 1), (COINS, 0, 1)] {
        app.register_item_kind(ItemKindDef::new(kind).weight_g(grams).stackable(stack));
    }
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
    } else {
        LIVING_TRADE_KINDS
    }
}

pub fn outputs_for(world: &World, role: Role) -> &'static [&'static str] {
    if let Some(registry) = super::frontier::economy_spec::registry(world) {
        return registry.profession(role).map_or(&[], |entry| entry.outputs);
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
    match role {
        Role::Baker => return &[BAKE, COOK],
        Role::Smith => return &[SMELT, TOOL, FORGE_SPEAR],
        Role::Tanner => return &[TAN],
        Role::Tailor => return &[SPIN, TAILOR],
        _ => {}
    }
    match role {
        Role::Miller => &[MILL],
        Role::Baker => &[BAKE],
        Role::Carpenter => &[SAW, CARVE],
        Role::Smith => &[SMELT, TOOL],
        _ => &[],
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
