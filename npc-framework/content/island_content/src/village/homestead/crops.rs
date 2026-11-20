//! One crop definition source for validation, physical effects and presentation.
//! Existing saves already retain crop kind and actual growth deadlines. No new
//! item registration or authoritative component is required for berry beds.
use bevy_ecs::prelude::*;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CropDef {
    pub kind: &'static str,
    pub label: &'static str,
    pub seed_kind: &'static str,
    pub seed_count: u32,
    pub water_kind: &'static str,
    pub water_count: u32,
    pub growth_ticks: u32,
    pub yield_kind: &'static str,
    pub yield_count: u32,
}

/// The garden crops of this world: its sown grain and what it forages, both
/// watered with its drink (the roles of `village::roles`, from the catalog
/// data in the living world).
fn crops(world: &World) -> [CropDef; 2] {
    let roles = super::super::roles::roles(world);
    [
        CropDef {
            kind: roles.grain,
            label: "Grain",
            seed_kind: roles.grain,
            seed_count: super::SEED_COUNT,
            water_kind: roles.drink,
            water_count: super::WATER_COUNT,
            growth_ticks: super::GROW_TICKS,
            yield_kind: roles.grain,
            yield_count: super::HARVEST_GRAIN,
        },
        CropDef {
            kind: roles.foraged,
            label: "Hardy berries",
            seed_kind: roles.foraged,
            seed_count: 1,
            water_kind: roles.drink,
            water_count: 1,
            growth_ticks: 9000,
            yield_kind: roles.foraged,
            yield_count: 4,
        },
    ]
}

pub fn crop_catalog(world: &World) -> Vec<CropDef> {
    let all = crops(world);
    all.to_vec()
}

pub fn crop_definition(world: &World, kind: &str) -> Option<CropDef> {
    crop_catalog(world)
        .into_iter()
        .find(|definition| definition.kind == kind)
}
