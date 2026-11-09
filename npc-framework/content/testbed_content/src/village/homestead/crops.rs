//! One crop definition source for validation, physical effects and presentation.
//! Existing saves already retain crop kind and actual growth deadlines. No new
//! item registration or authoritative component is required for berry beds.
use super::super::catalog;
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

const CROPS: [CropDef; 2] = [
    CropDef {
        kind: catalog::GRAIN,
        label: "Grain",
        seed_kind: catalog::GRAIN,
        seed_count: super::SEED_COUNT,
        water_kind: catalog::WATER,
        water_count: super::WATER_COUNT,
        growth_ticks: super::GROW_TICKS,
        yield_kind: catalog::GRAIN,
        yield_count: super::HARVEST_GRAIN,
    },
    CropDef {
        kind: catalog::BERRIES,
        label: "Hardy berries",
        seed_kind: catalog::BERRIES,
        seed_count: 1,
        water_kind: catalog::WATER,
        water_count: 1,
        growth_ticks: 9000,
        yield_kind: catalog::BERRIES,
        yield_count: 4,
    },
];

pub fn crop_catalog(world: &World) -> &'static [CropDef] {
    if super::super::colony::enabled(world) {
        &CROPS
    } else {
        &CROPS[..1]
    }
}

pub fn crop_definition(world: &World, kind: &str) -> Option<&'static CropDef> {
    crop_catalog(world)
        .iter()
        .find(|definition| definition.kind == kind)
}
