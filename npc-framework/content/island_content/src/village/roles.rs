//! The roles goods and trades play, resolved once per world (ADR 0042).
//!
//! Logic shared by every profile asks for a role (the fuel, the drink, the
//! garment, the hunting tool, the trade that builds, ...) instead of naming a
//! good or a trade. In the living world the roles come from the catalog data:
//! good tags, a profession's `hunt`, `builds` and `sows` fields, and the
//! profession whose outputs include the garment. A world without catalog
//! data (the Actor Testbed's scenarios, which share this code) uses
//! [`TESTBED`], a fixed table holding the names those scenarios were built
//! with, so their replays cannot move. A living world whose data lacks a
//! role the logic needs is refused at load with a sentence naming the role.
use super::Role;
use super::catalog;
use super::frontier::economy_spec::{EconomySpec, GoodTag, intern};
use bevy_ecs::prelude::*;

/// What each role is in one world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roles {
    /// What residents drink.
    pub drink: &'static str,
    /// What hearths burn.
    pub fuel: &'static str,
    /// The warm garment residents wear.
    pub garment: &'static str,
    /// Raw building material (felled), and sawn building material.
    pub timber: &'static str,
    pub lumber: &'static str,
    /// Smelted metal, and the ore it is made from.
    pub metal: &'static str,
    pub ore: &'static str,
    /// The tool a hunt takes, and what a hunt yields: meat and a hide, so
    /// many of each a hunt.
    pub hunt_tool: &'static str,
    pub meat: &'static str,
    pub meat_per_hunt: u32,
    pub hide: &'static str,
    pub hide_per_hunt: u32,
    /// The garment chain: tanned hide, spun fibre, the fibre itself.
    pub leather: &'static str,
    pub yarn: &'static str,
    pub fiber: &'static str,
    /// The baked staple, the cooked meal and what is foraged.
    pub baked: &'static str,
    pub cooked: &'static str,
    pub foraged: &'static str,
    /// The crop sown in the fields and milled for bread.
    pub grain: &'static str,
    /// The trades that hunt, build homes, sow fields and make the garment.
    pub hunter: Role,
    pub builder: Role,
    pub sower: Role,
    pub garment_maker: Role,
}

/// The roles of a world without catalog data (the Actor Testbed's
/// scenarios): the names those scenarios were built with.
pub const TESTBED: Roles = Roles {
    drink: catalog::WATER,
    fuel: "log",
    garment: catalog::WARM_CLOTHES,
    timber: "log",
    lumber: "plank",
    metal: "ingot",
    ore: "ore",
    hunt_tool: catalog::SPEAR,
    meat: catalog::RAW_MEAT,
    meat_per_hunt: 2,
    hide: catalog::HIDE,
    hide_per_hunt: 1,
    leather: catalog::LEATHER,
    yarn: catalog::THREAD,
    fiber: catalog::FIBER,
    baked: "bread",
    cooked: catalog::COOKED_MEAL,
    foraged: catalog::BERRIES,
    grain: catalog::GRAIN,
    hunter: Role::Hunter,
    builder: Role::Carpenter,
    sower: Role::Farmer,
    garment_maker: Role::Tailor,
};

/// The roles of a living world, resolved at load.
#[derive(Resource, Clone, Debug)]
pub struct WorldRoles(pub Roles);

/// The roles of this world: the living world's from its catalog data, the
/// Actor Testbed's from [`TESTBED`].
pub fn roles(world: &World) -> &Roles {
    world
        .get_resource::<WorldRoles>()
        .map_or(&TESTBED, |roles| &roles.0)
}

/// Resolves every role from a living world's economy data.
///
/// # Errors
/// A sentence naming the first role the data does not fill, for example
/// "economy: no good is tagged `fuel`, so hearths have nothing to burn".
pub fn from_spec(spec: &EconomySpec) -> Result<Roles, String> {
    let tagged = |tag: GoodTag, what: &str| -> Result<&'static str, String> {
        spec.goods
            .iter()
            .find(|good| good.tags.contains(&tag))
            .map(|good| intern(&good.key))
            .ok_or_else(|| {
                let name = serde_json::to_value(tag)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default();
                format!("economy: no good is tagged `{name}`, so {what}")
            })
    };
    let drink = tagged(GoodTag::Drink, "residents have nothing to drink")?;
    let fuel = tagged(GoodTag::Fuel, "hearths have nothing to burn")?;
    let garment = tagged(GoodTag::Clothing, "nobody has a garment against the cold")?;
    let timber = tagged(GoodTag::Timber, "homes have no raw building material")?;
    let lumber = tagged(GoodTag::Lumber, "homes have no sawn building material")?;
    let metal = tagged(GoodTag::Metal, "tools have no metal")?;
    let ore = tagged(GoodTag::Ore, "metal has no ore")?;
    let leather = tagged(GoodTag::Leather, "garments have no leather")?;
    let yarn = tagged(GoodTag::Yarn, "garments have no yarn")?;
    let fiber = tagged(GoodTag::Fiber, "yarn has no fibre")?;
    let baked = tagged(GoodTag::Baked, "there is no baked staple")?;
    let cooked = tagged(GoodTag::Cooked, "there is no cooked meal")?;
    let foraged = tagged(GoodTag::Foraged, "there is nothing to forage")?;
    let grain = tagged(GoodTag::Grain, "the fields have no grain to sow")?;
    let hunter = spec
        .professions
        .iter()
        .find(|profession| profession.hunt.is_some())
        .ok_or("economy: no profession has a `hunt`, so nobody hunts")?;
    let hunt = hunter.hunt.as_ref().expect("found by its hunt");
    let yield_of = |tag: GoodTag, what: &str| -> Result<(&'static str, u32), String> {
        hunt.yields
            .iter()
            .find(|stack| {
                spec.goods
                    .iter()
                    .any(|good| good.key == stack.good && good.tags.contains(&tag))
            })
            .map(|stack| (intern(&stack.good), stack.count))
            .ok_or_else(|| format!("economy: the hunt yields no {what}"))
    };
    let (meat, meat_per_hunt) = yield_of(GoodTag::Meat, "meat")?;
    let (hide, hide_per_hunt) = yield_of(GoodTag::Hide, "hide")?;
    let builder = spec
        .professions
        .iter()
        .find(|profession| profession.builds)
        .ok_or("economy: no profession `builds`, so nobody builds a home")?
        .role;
    let sower = spec
        .professions
        .iter()
        .find(|profession| profession.sows)
        .ok_or("economy: no profession `sows`, so nobody works the fields")?
        .role;
    let garment_maker = spec
        .professions
        .iter()
        .find(|profession| profession.outputs.iter().any(|good| good == garment))
        .ok_or("economy: no profession makes the garment")?
        .role;
    Ok(Roles {
        drink,
        fuel,
        garment,
        timber,
        lumber,
        metal,
        ore,
        hunt_tool: intern(&hunt.tool),
        meat,
        meat_per_hunt,
        hide,
        hide_per_hunt,
        leather,
        yarn,
        fiber,
        baked,
        cooked,
        foraged,
        grain,
        hunter: hunter.role,
        builder,
        sower,
        garment_maker,
    })
}
