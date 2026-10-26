//! State-hash encodings and their (opt-in) registration.

use bevy_app::App;
use npc_kernel::SimPos;
use npc_kernel::hash::{Fnv64, SimHash, SimHashRegistry, SimHashResource};
use npc_kernel::registry::KernelAppExt;
use npc_kernel::spatial::register_spatial_hash;
use npc_locomotion::register_locomotion_hash;

use crate::CommandConfig;
use crate::formation::{Formation, FormationKind};
use crate::types::{
    ActiveOrder, Commandable, ExecutingOrder, GroupLink, MemberOf, OrderProgress, OrderQueue,
    Owner, QueuedOrder, Squad, UnitOrder,
};

fn len(n: usize) -> u32 {
    u32::try_from(n).expect("too many entries to hash")
}

fn pair(h: &mut Fnv64, (x, z): (i32, i32)) {
    h.i32(x);
    h.i32(z);
}

fn formation(h: &mut Fnv64, f: &Formation) {
    h.u8(match f.kind {
        FormationKind::Line => 0,
        FormationKind::Box => 1,
        FormationKind::Wedge => 2,
    });
    h.i32(f.spacing_mm);
}

fn order(h: &mut Fnv64, o: &UnitOrder) {
    h.u8(o.discriminant());
    match o {
        UnitOrder::Move { pos } | UnitOrder::AttackMove { pos } => pair(h, *pos),
        UnitOrder::Stop | UnitOrder::Hold | UnitOrder::Unload => {}
        UnitOrder::Follow { target } => h.u32(target.0),
        UnitOrder::Patrol { points } => {
            h.u32(len(points.len()));
            for &p in points {
                pair(h, p);
            }
        }
        UnitOrder::Interact { verb, target } => {
            h.str(verb);
            h.u32(target.0);
        }
        UnitOrder::Mount { vehicle } => h.u32(vehicle.0),
        UnitOrder::Custom { name, args } => {
            h.str(name);
            // The JSON text (field order kept) is deterministic.
            h.str(&serde_json::to_string(args).unwrap_or_default());
        }
    }
}

fn queued(h: &mut Fnv64, q: &QueuedOrder) {
    order(h, &q.order);
    h.opt(q.group.as_ref(), |h, g: &GroupLink| {
        h.u32(g.leader.0);
        pair(h, g.offset);
    });
    h.u32(q.player.0);
    h.u64(q.issued);
}

fn progress(h: &mut Fnv64, p: &OrderProgress) {
    match p {
        OrderProgress::NotStarted => h.u8(0),
        OrderProgress::Moving => h.u8(1),
        OrderProgress::WaitingForLeader { since } => {
            h.u8(2);
            h.u64(*since);
        }
        OrderProgress::Holding { at } => {
            h.u8(3);
            at.sim_hash(h);
        }
        OrderProgress::Patrolling { route, next } => {
            h.u8(4);
            h.u32(len(route.len()));
            for p in route {
                SimPos::sim_hash(p, h);
            }
            h.u32(*next);
        }
        OrderProgress::Hooked => h.u8(5),
    }
}

impl SimHash for Owner {
    const NAME: &'static str = "command_owner";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.0.0);
    }
}

impl SimHash for Commandable {
    const NAME: &'static str = "commandable";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.speed_mm_per_tick);
        h.i32(self.arrive_mm);
    }
}

impl SimHash for OrderQueue {
    const NAME: &'static str = "order_queue";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.opt(self.current.as_ref(), |h, a: &ActiveOrder| {
            queued(h, &a.order);
            h.opt(a.started.as_ref(), |h, t| h.u64(*t));
            progress(h, &a.progress);
        });
        h.u32(len(self.queued.len()));
        for q in &self.queued {
            queued(h, q);
        }
        h.u8(u8::from(self.last_ok));
    }
}

impl SimHash for ExecutingOrder {
    const NAME: &'static str = "executing_order";
    fn sim_hash(&self, _h: &mut Fnv64) {}
}

impl SimHash for Squad {
    const NAME: &'static str = "squad";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(len(self.members.len()));
        for m in &self.members {
            h.u32(m.0);
        }
        formation(h, &self.formation);
    }
}

impl SimHash for MemberOf {
    const NAME: &'static str = "squad_member";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.0.0);
    }
}

impl SimHashResource for CommandConfig {
    const NAME: &'static str = "command_config";
    fn sim_hash(&self, h: &mut Fnv64) {
        formation(h, &self.default_formation);
        h.i32(self.follow_distance_mm);
        h.i32(self.follow_retarget_mm);
        h.u32(self.block_fail_ticks);
        h.u32(self.max_queue);
        h.u32(self.max_group);
        h.u32(self.max_patrol_points);
        h.u32(self.relocate_radius_cells);
        h.u32(self.leader_wait_ticks);
    }
}

/// Adds the command state to the state hash (idempotent): the components
/// above, `command_config`, plus the locomotion (`move_intent`,
/// `move_status`: order completion depends on them) and spatial (formation
/// slots are validated against the grid) state. Changes the content hash;
/// `CommandPlugin` calls it, so an app without the plugin hashes exactly as
/// before.
pub fn register_command_hash(app: &mut App) {
    register_spatial_hash(app);
    register_locomotion_hash(app);
    let registry = app.world_mut().get_resource_or_init::<SimHashRegistry>();
    if registry
        .components
        .iter()
        .any(|c| c.name == OrderQueue::NAME)
    {
        return;
    }
    app.register_sim_component::<Owner>()
        .register_sim_component::<Commandable>()
        .register_sim_component::<OrderQueue>()
        .register_sim_component::<ExecutingOrder>()
        .register_sim_component::<Squad>()
        .register_sim_component::<MemberOf>()
        .register_sim_resource::<CommandConfig>();
}
