use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct Crop {
    pub kind: String,
    pub planted_at: u64,
    pub watered_at: Option<u64>,
    pub ready_at: Option<u64>,
    pub ready_announced: bool,
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct HomesteadPlot {
    pub plot_id: String,
    pub owner: Option<SimId>,
    pub cottage_built: bool,
    pub cottage_entity: Option<SimId>,
    pub beds: Vec<Option<Crop>>,
}
impl SimHash for HomesteadPlot {
    const NAME: &'static str = "island_homestead_plot_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.plot_id);
        h.opt(self.owner.as_ref(), |h, id| h.u32(id.0));
        h.u8(u8::from(self.cottage_built));
        h.opt(self.cottage_entity.as_ref(), |h, id| h.u32(id.0));
        h.u32(self.beds.len() as u32);
        for bed in &self.beds {
            h.opt(bed.as_ref(), |h, crop| {
                h.str(&crop.kind);
                h.u64(crop.planted_at);
                h.opt(crop.watered_at.as_ref(), |h, tick| h.u64(*tick));
                h.opt(crop.ready_at.as_ref(), |h, tick| h.u64(*tick));
                h.u8(u8::from(crop.ready_announced));
            });
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Cottage {
    pub plot: SimId,
    pub plot_id: String,
}
impl SimHash for Cottage {
    const NAME: &'static str = "island_homestead_cottage_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.plot.0);
        h.str(&self.plot_id);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct BuildMaterial {
    pub item: SimId,
    pub kind: String,
    pub count: u32,
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct CottageBuild {
    pub plot: SimId,
    pub ends: u64,
    pub materials: Vec<BuildMaterial>,
}
impl SimHash for CottageBuild {
    const NAME: &'static str = "island_cottage_build_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.plot.0);
        h.u64(self.ends);
        h.u32(self.materials.len() as u32);
        for material in &self.materials {
            h.u32(material.item.0);
            h.str(&material.kind);
            h.u32(material.count);
        }
    }
}
