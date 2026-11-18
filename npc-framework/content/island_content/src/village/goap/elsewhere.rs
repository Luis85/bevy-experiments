//! Funded demand on the island that no maker is planned to meet and a
//! resident's trade can serve (economy review npc-ai#9): purchases the
//! residents of a settlement want (a working tool the ledger orders for each
//! woodcutter) beyond what its own makers are planned to make; in a
//! settlement with no maker of the good, those purchases or its stock
//! target, whichever is more; and what the merchant boat's announced order
//! still wants beyond the port's makers. Each such demand is shared out in
//! identity order among the makers of that settlement, or of the other
//! settlements when it has none, so two servers never both chase the whole.
//!
//! Pure queries: nothing here changes state. The planner turns a demand into
//! a goal whose last step carries the goods to `place` (a store or stall of
//! that settlement, or the trading post), where they are on sale as the
//! resident's own (consignment, `civic::depot_deposit`).
use crate::village::{
    Role, Villager, catalog, civic, frontier, living, market, merchant, production, routing, stock,
};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// How often a resident without such a plan looks for unplanned demand: in
/// one decision interval of every this many ticks, staggered by identity.
/// The look recounts settlements' purchases and stock; doing it on every
/// decision of every idle resident cost more than the work it found.
pub const LOOK_TICKS: u64 = 120;

/// Whether this decision is the resident's turn to look (see [`LOOK_TICKS`]).
pub fn looks_now(world: &World, actor: Entity) -> bool {
    let now = world.resource::<SimClock>().tick;
    let id = world.get::<SimId>(actor).map_or(0, |id| u64::from(id.0));
    let window = crate::village::policy::active(world).map_or(20, |policy| policy.decision_ticks);
    (now + id * 37) % LOOK_TICKS < window.min(LOOK_TICKS)
}

/// One demand this resident serves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub kind: &'static str,
    /// The store, stall or trading post the goods go to, and where it is;
    /// `None` when no outlet of that settlement takes them: they stay in the
    /// maker's pack for sale.
    pub place: Option<(SimId, SimPos)>,
    /// This resident's share of the open units.
    pub units: u32,
    /// What a unit fetches there now.
    pub unit_price: u32,
    /// Food, drink or clothing: ranked before other goods.
    pub essential: bool,
}

/// The settlements with a living maker of each good, as `(settlement,
/// good)`, and every maker as `(id, entity, settlement, role)`. Whether a
/// maker can work now (`labor::work_feasible`: routes, patches) is asked
/// only of the resident planning, which keeps this a cheap scan.
struct Makers {
    covered: BTreeSet<(SimId, &'static str)>,
    people: Vec<(SimId, Entity, SimId, Role)>,
}

fn makers(world: &World) -> Makers {
    let mut people: Vec<(SimId, Entity, SimId, Role)> = world
        .try_query::<(Entity, &SimId, &Villager)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter(|(entity, _, person)| {
                    person.role != Role::Visitor
                        && living(world, *entity)
                        && !catalog::outputs_for(world, person.role).is_empty()
                })
                .filter_map(|(entity, id, person)| {
                    Some((
                        *id,
                        entity,
                        production::economic_settlement(world, entity)?,
                        person.role,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    people.sort_unstable_by_key(|(id, ..)| *id);
    let mut covered = BTreeSet::new();
    for (_, _, settlement, role) in &people {
        for kind in catalog::outputs_for(world, *role) {
            covered.insert((*settlement, *kind));
        }
    }
    Makers { covered, people }
}

/// `units` shared out among `servers` (ascending identities): the share of
/// `me`, the earlier servers taking the remainder first.
fn share(units: u32, servers: &[SimId], me: SimId) -> u32 {
    let Some(index) = servers.iter().position(|id| *id == me) else {
        return 0;
    };
    let n = servers.len() as u32;
    units / n + u32::from((index as u32) < units % n)
}

fn essential(world: &World, kind: &str) -> bool {
    let roles = crate::village::roles::roles(world);
    catalog::food_restore_in(world, kind).is_some() || kind == roles.drink || kind == roles.garment
}

/// The store or stall of `settlement` nearest `from` that takes `kind` and
/// has room, never the trading post (the boat's stall).
fn outlet(
    world: &World,
    actor: Entity,
    settlement: SimId,
    kind: &str,
    from: SimPos,
) -> Option<(SimId, SimPos)> {
    let me = world.get::<SimId>(actor).copied();
    let now = world.resource::<SimClock>().tick;
    let post = merchant::post(world).map(|(id, _)| id);
    let storable = civic::storable(world, kind);
    civic::depots(world, settlement)
        .into_iter()
        .filter(|(id, _)| Some(*id) != post)
        .filter_map(|(id, entity)| {
            let depot = world.get::<civic::Depot>(entity)?;
            let pos = *world.get::<SimPos>(entity)?;
            (depot.accepts(kind)
                && (depot.public || storable)
                && depot.room(me, now) > 0
                && !routing::avoiding(world, actor, pos, Some(id)))
            .then_some((from.dist2(pos), id, pos))
        })
        .min_by_key(|(distance, id, _)| (*distance, *id))
        .map(|(_, id, pos)| (id, pos))
}

/// Whether `settlement` itself wants more of `kind` than it has on offer:
/// its stock target, or what its residents' funded purchases want (a hen
/// keeper's grain). Such a good is not sent to another settlement or the
/// boat, nor bought there as an input for them (review: farmers carried
/// grain to the boat's order while their own hen keepers and millers went
/// short, and eggs fell on every seed).
pub fn short(world: &World, settlement: SimId, kind: &str) -> bool {
    let offer = stock::on_offer(world, settlement, kind);
    if stock::target(world, settlement, kind) > offer {
        return true;
    }
    let orders: u32 = crate::village::demography::members(world, settlement)
        .into_iter()
        .map(|(_, member)| {
            production::purchase_target(world, member, kind)
                .saturating_sub(crate::village::economy::count(world, member, kind))
        })
        .fold(0, u32::saturating_add);
    orders > offer
}

/// Every such demand this resident's trade can serve now, best first:
/// essential goods, then by value (units times price), then good and place.
pub fn goals(world: &World, actor: Entity) -> Vec<Remote> {
    let _probe = crate::village::probe::lookup(world, "sup.elsewhere.start", "sup.elsewhere.other");
    let (Some(person), Some(me), Some(here)) = (
        world.get::<Villager>(actor),
        world.get::<SimId>(actor).copied(),
        world.get::<SimPos>(actor).copied(),
    ) else {
        return Vec::new();
    };
    let outputs = catalog::outputs_for(world, person.role);
    if outputs.is_empty() {
        return Vec::new();
    }
    let Some(home) = production::economic_settlement(world, actor) else {
        return Vec::new();
    };
    let makers = makers(world);
    let servers = |kind: &str, keep: &dyn Fn(SimId) -> bool| -> Vec<SimId> {
        makers
            .people
            .iter()
            .filter(|(_, _, settlement, role)| {
                keep(*settlement) && catalog::outputs_for(world, *role).contains(&kind)
            })
            .map(|(id, ..)| *id)
            .collect()
    };
    let mut out = Vec::new();
    // Goods the resident's own settlement is short of serve it first.
    let mut home_short: BTreeMap<&str, bool> = BTreeMap::new();
    let mut spare_at_home = |kind: &'static str| {
        !*home_short
            .entry(kind)
            .or_insert_with(|| short(world, home, kind))
    };
    // Each settlement's funded purchases (a working tool the ledger orders
    // for each woodcutter, ...) its own makers are not planned to make: in
    // the resident's own settlement, its makers serve them; in one without
    // a maker of the good, the makers elsewhere serve them or its stock
    // target, whichever is more. Beyond what is on offer there.
    for (settlement, _) in frontier::settlements::all(world) {
        let mut members = None;
        for kind in outputs {
            let covered = makers.covered.contains(&(settlement, *kind));
            if settlement != home && (covered || !spare_at_home(kind)) {
                continue;
            }
            let members = members
                .get_or_insert_with(|| crate::village::demography::members(world, settlement));
            let orders: u32 = members
                .iter()
                .map(|(_, member)| {
                    production::purchase_target(world, *member, kind)
                        .saturating_sub(crate::village::economy::count(world, *member, kind))
                })
                .fold(0, u32::saturating_add);
            let wanted = if covered {
                let planned: u32 = makers
                    .people
                    .iter()
                    .filter(|(_, _, region, _)| *region == settlement)
                    .map(|(_, entity, ..)| production::production_demand(world, *entity, kind))
                    .fold(0, u32::saturating_add);
                orders.saturating_sub(planned)
            } else {
                stock::target(world, settlement, kind).max(orders)
            };
            if wanted == 0 {
                continue;
            }
            let open = wanted.saturating_sub(stock::on_offer(world, settlement, kind));
            let units = if covered {
                share(open, &servers(kind, &|region| region == settlement), me)
            } else {
                share(open, &servers(kind, &|region| region != settlement), me)
            };
            let Some(price) = market::unit_price(world, kind) else {
                continue;
            };
            if units == 0 {
                continue;
            }
            out.push(Remote {
                kind,
                place: outlet(world, actor, settlement, kind, here),
                units,
                unit_price: price,
                essential: essential(world, kind),
            });
        }
    }
    // The boat's announced order, beyond what the port's makers are to make
    // and what already lies at the post for it.
    if let (Some(boat), Some((post, post_entity))) = (
        world.get_resource::<merchant::Merchant>(),
        merchant::post(world),
    ) && let Some(port) = boat.settlement
        && port != home
        && let Some(pos) = world.get::<SimPos>(post_entity).copied()
        && !routing::avoiding(world, actor, pos, Some(post))
    {
        for order in &boat.order {
            let Some(kind) = outputs.iter().find(|kind| **kind == order.good) else {
                continue;
            };
            if !spare_at_home(kind) {
                continue;
            }
            let at_post = world
                .get::<civic::Depot>(post_entity)
                .map_or(0, |depot| depot.stock(kind));
            let planned: u32 = makers
                .people
                .iter()
                .filter(|(_, _, settlement, _)| *settlement == port)
                .map(|(_, entity, ..)| production::production_demand(world, *entity, kind))
                .fold(0, u32::saturating_add);
            let open = order
                .units
                .saturating_sub(order.delivered)
                .saturating_sub(at_post)
                .saturating_sub(planned);
            let units = share(open, &servers(kind, &|region| region != port), me);
            if units > 0 {
                out.push(Remote {
                    kind,
                    place: Some((post, pos)),
                    units,
                    unit_price: order.unit_price,
                    essential: essential(world, kind),
                });
            }
        }
    }
    // Whether this resident can work at all (routes, patches) is the dearer
    // question: asked last, and only with something to serve.
    if out.is_empty() || !frontier::labor::work_feasible(world, actor) {
        return Vec::new();
    }
    out.sort_by(|a, b| {
        b.essential
            .cmp(&a.essential)
            .then_with(|| {
                (u64::from(b.units) * u64::from(b.unit_price))
                    .cmp(&(u64::from(a.units) * u64::from(a.unit_price)))
            })
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| a.place.map(|(id, _)| id).cmp(&b.place.map(|(id, _)| id)))
    });
    out
}
