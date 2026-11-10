//! Settlement identities and explicit homes; employment belongs to labor.
use super::world_spec::{HomeDef, SettlementDef};
use crate::village::{Role, VillagePlace, catalog, colony::Household};
use bevy_app::App;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::crafting::Workstation;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

pub const WORKPLACE_OFFSET_MM: i32 = 3500;
pub const HOME_OFFSET_MM: i32 = -3500;

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Settlement {
    pub key: String,
    pub label: String,
    pub center: SimPos,
}
impl SimHash for Settlement {
    const NAME: &'static str = "living_world_settlement_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.key);
        h.str(&self.label);
        self.center.sim_hash(h);
    }
}
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct SettlementMember {
    pub settlement: SimId,
}
impl SimHash for SettlementMember {
    const NAME: &'static str = "living_world_settlement_member_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.settlement.0);
    }
}
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct SettlementSite {
    pub key: String,
}
impl SimHash for SettlementSite {
    const NAME: &'static str = "living_world_settlement_site_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.key);
    }
}
pub fn register(app: &mut App) {
    app.register_sim_component::<Settlement>()
        .register_sim_component::<SettlementMember>()
        .register_sim_component::<SettlementSite>();
}
pub fn all(world: &World) -> Vec<(SimId, Entity)> {
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, e)| world.get::<Settlement>(**e).is_some())
        .map(|(id, e)| (*id, *e))
        .collect()
}
pub fn member(world: &World, entity: Entity) -> Option<SimId> {
    world.get::<SettlementMember>(entity).map(|m| m.settlement)
}
pub fn definition(world: &World, settlement: SimId) -> Option<&SettlementDef> {
    let e = world.resource::<SimIndex>().get(settlement)?;
    let key = &world.get::<Settlement>(e)?.key;
    super::spec(world)?
        .settlements
        .iter()
        .find(|s| &s.key == key)
}
pub fn workplace(world: &World, settlement: SimId, role: Role) -> Option<SimId> {
    world.resource::<SimIndex>().map.iter().find_map(|(id, e)| {
        (member(world, *e) == Some(settlement)
            && world
                .get::<VillagePlace>(*e)
                .is_some_and(|s| s.role == role))
        .then_some(*id)
    })
}
pub fn site_entity(world: &World, settlement: SimId, key: &str) -> Option<Entity> {
    world
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .find(|e| {
            member(world, *e) == Some(settlement)
                && world
                    .get::<SettlementSite>(*e)
                    .is_some_and(|s| s.key == key)
        })
}
pub fn home_entity(world: &World, settlement: SimId, key: &str) -> Option<Entity> {
    site_entity(world, settlement, key).filter(|e| world.get::<Household>(*e).is_some())
}
pub fn home_def(world: &World, settlement: SimId, index: usize) -> Option<&HomeDef> {
    definition(world, settlement)?.homes.get(index)
}
pub fn nearest(world: &World, at: SimPos) -> Option<SimId> {
    all(world)
        .into_iter()
        .min_by_key(|(id, e)| {
            (
                world
                    .get::<Settlement>(*e)
                    .expect("settlement")
                    .center
                    .dist2(at),
                *id,
            )
        })
        .map(|(id, _)| id)
}
pub fn spawn(world: &mut World, ids: &mut crate::TestbedIds) {
    let definitions = super::spec(world)
        .expect("living world")
        .settlements
        .clone();
    for definition in definitions {
        let (settlement, _) = spawn_sim(
            world,
            (
                definition.center,
                Settlement {
                    key: definition.key.clone(),
                    label: definition.label.clone(),
                    center: definition.center,
                },
                Perceivable::new(crate::world::kinds::STATION),
            ),
        );
        ids.name(format!("settlement_{}", definition.key), settlement);
        for site in definition.sites {
            let station = match site.role {
                Role::Miller => catalog::MILL,
                Role::Baker => catalog::BAKE,
                Role::Carpenter => catalog::SAW,
                Role::Smith => catalog::SMELT,
                Role::Tanner => catalog::TAN,
                Role::Tailor => catalog::TAILOR,
                _ => catalog::INN,
            };
            let (id, _) = spawn_sim(
                world,
                (
                    site.position,
                    SettlementMember { settlement },
                    SettlementSite {
                        key: site.key.clone(),
                    },
                    VillagePlace {
                        role: site.role,
                        label: site.label,
                    },
                    Workstation::new([station]),
                    Approach(site.position),
                    Perceivable::new(crate::world::kinds::STATION),
                ),
            );
            ids.name(format!("{}_{}", definition.key, site.key), id);
            if !matches!(site.role, Role::WaterKeeper | Role::Fisher) {
                spawn_sim(
                    world,
                    (
                        SimPos::new(site.position.x, site.position.z + WORKPLACE_OFFSET_MM),
                        GridBlocker(Footprint {
                            half_x_mm: 2500,
                            half_z_mm: 2500,
                        }),
                        GridOccluder(Footprint {
                            half_x_mm: 2500,
                            half_z_mm: 2500,
                        }),
                    ),
                );
            }
        }
        for home in definition.homes {
            let (id, _) = spawn_sim(
                world,
                (
                    home.door,
                    SettlementMember { settlement },
                    SettlementSite {
                        key: home.key.clone(),
                    },
                    Household {
                        label: home.label,
                        capacity: home.capacity,
                        residents: Vec::new(),
                        door: home.door,
                        heated_until: 0,
                        logs_burned: 0,
                    },
                    Perceivable::new(crate::world::kinds::STATION),
                ),
            );
            ids.name(format!("{}_{}", definition.key, home.key), id);
            spawn_sim(
                world,
                (
                    SimPos::new(home.door.x, home.door.z + HOME_OFFSET_MM),
                    GridBlocker(Footprint {
                        half_x_mm: 1900,
                        half_z_mm: 2300,
                    }),
                    GridOccluder(Footprint {
                        half_x_mm: 1900,
                        half_z_mm: 2300,
                    }),
                ),
            );
        }
    }
}

/// Compare immutable site identity with frozen construction; mutable household
/// membership and employment have their own validators.
pub fn validate(world: &World) -> Result<(), String> {
    let spec = super::spec(world).ok_or("Regional construction is missing")?;
    let actual = all(world);
    if actual.len() != spec.settlements.len() {
        return Err("Regional settlement identities are incomplete".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for (id, entity) in actual {
        let saved = world.get::<Settlement>(entity).expect("settlement");
        let def = spec
            .settlements
            .iter()
            .find(|d| d.key == saved.key)
            .ok_or("Unknown settlement identity")?;
        if !seen.insert(&saved.key)
            || saved.label != def.label
            || saved.center != def.center
            || world.get::<SimPos>(entity) != Some(&def.center)
        {
            return Err("Settlement identity differs from frozen construction".into());
        }
        for site in &def.sites {
            let e = site_entity(world, id, &site.key).ok_or("Authored workplace is missing")?;
            if world.get::<SimPos>(e) != Some(&site.position)
                || world
                    .get::<VillagePlace>(e)
                    .is_none_or(|p| p.role != site.role || p.label != site.label)
                || world.get::<Workstation>(e).is_none()
                || world.get::<Household>(e).is_some()
            {
                return Err("Workplace identity differs from frozen construction".into());
            }
        }
        for home in &def.homes {
            let e = home_entity(world, id, &home.key).ok_or("Authored household is missing")?;
            if world.get::<SimPos>(e) != Some(&home.door)
                || world.get::<Household>(e).is_none_or(|h| {
                    h.door != home.door || h.label != home.label || h.capacity != home.capacity
                })
            {
                return Err("Household identity differs from frozen construction".into());
            }
        }
        let sites = world
            .resource::<SimIndex>()
            .map
            .values()
            .filter(|e| {
                member(world, **e) == Some(id) && world.get::<SettlementSite>(**e).is_some()
            })
            .count();
        if sites != def.sites.len() + def.homes.len() {
            return Err("Duplicate or unexpected regional site identity".into());
        }
    }
    let plots = crate::village::homestead::plots(world);
    if plots.len() != spec.plots.len() {
        return Err("Regional homestead identities are incomplete".into());
    }
    let mut seen_plots = std::collections::BTreeSet::new();
    for (_, e) in plots {
        let plot = world
            .get::<crate::village::homestead::HomesteadPlot>(e)
            .expect("plot");
        let def = spec
            .plots
            .iter()
            .find(|p| p.id == plot.plot_id)
            .ok_or("Unknown regional homestead")?;
        if !seen_plots.insert(&plot.plot_id)
            || world.get::<SimPos>(e) != Some(&SimPos::new(def.x, def.z))
        {
            return Err("Homestead identity differs from frozen construction".into());
        }
    }
    Ok(())
}
