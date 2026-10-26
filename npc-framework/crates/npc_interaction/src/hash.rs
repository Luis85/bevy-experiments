//! State-hash encodings and their registration.

use bevy_app::App;
use npc_kernel::SimId;
use npc_kernel::hash::{Fnv64, SimHash, SimHashRegistry, SimHashResource};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::spatial::register_spatial_hash;

use crate::InteractionConfig;
use crate::defs::SkillCheck;
use crate::door::Door;
use crate::items::{CarriedBy, Equipment, Interactor, Inventory, Item};
use crate::leaf::InteractLeaf;
use crate::run::{Interacting, Source};
use crate::targets::{Interactable, ToolTarget};

fn len(n: usize) -> u32 {
    u32::try_from(n).expect("too many items to hash")
}

fn opt_id(h: &mut Fnv64, id: Option<SimId>) {
    h.opt(id.as_ref(), |h, id| h.u32(id.0));
}

fn opt_str(h: &mut Fnv64, s: Option<&String>) {
    h.opt(s, |h, s| h.str(s));
}

fn check(h: &mut Fnv64, c: Option<&SkillCheck>) {
    h.opt(c, |h, c| {
        h.str(&c.skill);
        h.i32(c.modifier);
    });
}

impl SimHash for Item {
    const NAME: &'static str = "item";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.kind);
        h.u32(self.stack);
        h.i32(self.quality);
    }
}

impl SimHash for Inventory {
    const NAME: &'static str = "inventory";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.slots);
        h.u32(self.max_weight_g);
        h.u32(len(self.items.len()));
        for id in &self.items {
            h.u32(id.0);
        }
    }
}

impl SimHash for Equipment {
    const NAME: &'static str = "equipment";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len(self.slots.len()));
        for (slot, id) in &self.slots {
            h.str(slot);
            opt_id(h, *id);
        }
    }
}

impl SimHash for CarriedBy {
    const NAME: &'static str = "carried_by";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.holder.0);
        opt_str(h, self.slot.as_ref());
    }
}

impl SimHash for Interactor {
    const NAME: &'static str = "interactor";
    fn sim_hash(&self, _h: &mut Fnv64) {}
}

impl SimHash for Door {
    const NAME: &'static str = "door";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u8(u8::from(self.open));
        opt_str(h, self.locked_by.as_ref());
        opt_str(h, self.key.as_ref());
    }
}

impl SimHash for Interactable {
    const NAME: &'static str = "interactable";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len(self.verbs.len()));
        for v in &self.verbs {
            h.str(v);
        }
    }
}

impl SimHash for ToolTarget {
    const NAME: &'static str = "tool_target";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.required_tag);
        h.u32(len(self.yields.len()));
        for y in &self.yields {
            h.str(&y.kind);
            h.u32(y.stack);
            h.i32(y.quality);
        }
        h.u32(self.duration_ticks);
        check(h, self.check.as_ref());
        h.opt(self.remaining.as_ref(), |h, n| h.u32(*n));
    }
}

impl SimHash for Interacting {
    const NAME: &'static str = "interacting";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.verb);
        h.u32(self.target.0);
        opt_id(h, self.item);
        h.u64(self.started);
        h.u64(self.ends);
        h.u8(match self.source {
            Source::Tree => 0,
            Source::Command => 1,
            Source::Auto => 2,
        });
    }
}

impl SimHash for InteractLeaf {
    const NAME: &'static str = "interact_leaf";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.verb);
        h.u32(self.target.0);
        opt_id(h, self.item);
    }
}

impl SimHashResource for InteractionConfig {
    const NAME: &'static str = "interaction_config";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.reach_mm);
        h.u8(self.target_slot);
        h.u8(self.item_slot);
        h.u8(self.door_tag_bit);
        h.u16(self.door_pass_cost);
        h.u8(u8::from(self.auto_open_doors));
        h.i32(self.affordance_radius_mm);
    }
}

/// Adds the interaction state to the state hash (idempotent): the
/// components above, `interaction_config`, and the spatial grid's static
/// layer (doors live in it). Changes the content hash; `InteractionPlugin`
/// calls it, so an app without the plugin hashes exactly as before.
pub fn register_interaction_hash(app: &mut App) {
    register_spatial_hash(app);
    let registry = app.world_mut().get_resource_or_init::<SimHashRegistry>();
    if registry.components.iter().any(|c| c.name == Item::NAME) {
        return;
    }
    app.register_sim_component::<Item>()
        .register_sim_component::<Inventory>()
        .register_sim_component::<Equipment>()
        .register_sim_component::<CarriedBy>()
        .register_sim_component::<Interactor>()
        .register_sim_component::<Door>()
        .register_sim_component::<Interactable>()
        .register_sim_component::<ToolTarget>()
        .register_sim_component::<Interacting>()
        .register_sim_component::<InteractLeaf>()
        .register_sim_resource::<InteractionConfig>();
}
