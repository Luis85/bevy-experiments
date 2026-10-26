//! State-hash encodings and their registration.

use bevy_app::App;
use npc_kernel::SimId;
use npc_kernel::hash::{Fnv64, SimHash, SimHashResource};
use npc_kernel::registry::KernelAppExt;

use crate::defs::CraftingConfig;
use crate::leaf::CraftLeaf;
use crate::run::{CraftSource, Crafting};
use crate::station::{CraftingOrders, Durability, Workstation};

fn len(n: usize) -> u32 {
    u32::try_from(n).expect("too many entries to hash")
}

fn opt_id(h: &mut Fnv64, id: Option<SimId>) {
    h.opt(id.as_ref(), |h, id| h.u32(id.0));
}

impl SimHash for Workstation {
    const NAME: &'static str = "workstation";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len(self.kinds.len()));
        for k in &self.kinds {
            h.str(k);
        }
    }
}

impl SimHash for CraftingOrders {
    const NAME: &'static str = "crafting_orders";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len(self.orders.len()));
        for o in &self.orders {
            h.u32(o.id);
            h.str(&o.recipe);
            h.u32(o.remaining);
        }
        h.u32(self.next_id);
    }
}

impl SimHash for Durability {
    const NAME: &'static str = "durability";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.current);
        h.u32(self.max);
    }
}

impl SimHash for Crafting {
    const NAME: &'static str = "crafting";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.recipe);
        opt_id(h, self.workstation);
        h.u32(len(self.claimed.len()));
        for id in &self.claimed {
            h.u32(id.0);
        }
        h.u64(self.started);
        h.u64(self.ends);
        h.u8(match self.source {
            CraftSource::Tree => 0,
            CraftSource::Command => 1,
        });
        h.opt(self.order.as_ref(), |h, o| h.u32(*o));
        h.u32(len(self.tools.len()));
        for id in &self.tools {
            h.u32(id.0);
        }
        h.u32(len(self.inputs.len()));
        for (id, units) in &self.inputs {
            h.u32(id.0);
            h.u32(*units);
        }
    }
}

impl SimHash for CraftLeaf {
    const NAME: &'static str = "craft_leaf";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.recipe);
        opt_id(h, self.workstation);
    }
}

impl SimHashResource for CraftingConfig {
    const NAME: &'static str = "crafting_config";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.reach_mm);
        h.i32(self.quality.margin_per_quality);
        h.i32(self.quality.max);
        h.i32(self.quality.crit_success_bonus);
        h.u32(self.wear_per_craft);
        h.u32(self.crit_fail_wear);
    }
}

/// Adds the crafting state to the state hash (`CraftingPlugin` calls it,
/// so an app without the plugin hashes exactly as before).
pub fn register_crafting_hash(app: &mut App) {
    app.register_sim_component::<Workstation>()
        .register_sim_component::<CraftingOrders>()
        .register_sim_component::<Durability>()
        .register_sim_component::<Crafting>()
        .register_sim_component::<CraftLeaf>()
        .register_sim_resource::<CraftingConfig>();
}
