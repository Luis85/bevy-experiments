//! Admission for validated presentation packs. Counts are reservations, not a
//! GPU allocator or a measured hardware performance guarantee.
use super::{pack::ActorSceneModel, quality::RenderQuality, scenes::ActorSceneAppearance};
use bevy::{asset::LoadState, prelude::*};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// `None` follows the current render quality. Games may replace the complete
/// limits explicitly; zero is a supported way to disable imported scenes.
#[derive(Resource, Clone, Debug, Default)]
pub struct ActorSceneBudget {
    pub limits: Option<ActorSceneLimits>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ActorSceneLimits {
    pub instances: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub primitives: usize,
    pub animation_nodes: usize,
    pub animation_channels: usize,
    pub texture_pixels: usize,
    pub estimated_bytes: usize,
    pub concurrent_model_loads: usize,
    pub distance_m: f32,
    pub distance_hysteresis_m: f32,
}
impl ActorSceneLimits {
    pub fn for_quality(quality: RenderQuality) -> Self {
        let (instances, vertices, triangles, primitives, nodes, channels, pixels, mib, distance) =
            match quality {
                RenderQuality::Performance => {
                    (64, 400_000, 200_000, 2048, 4096, 1024, 8_388_608, 128, 650.)
                }
                RenderQuality::Balanced => (
                    128, 800_000, 400_000, 4096, 8192, 2048, 16_777_216, 256, 1200.,
                ),
                RenderQuality::High => (
                    256, 1_600_000, 800_000, 8192, 16_384, 4096, 33_554_432, 512, 2000.,
                ),
            };
        Self {
            instances,
            vertices,
            triangles,
            primitives,
            animation_nodes: nodes,
            animation_channels: channels,
            texture_pixels: pixels,
            estimated_bytes: mib * 1024 * 1024,
            concurrent_model_loads: if quality == RenderQuality::Performance {
                1
            } else {
                2
            },
            distance_m: distance,
            distance_hysteresis_m: 150.,
        }
    }
    fn valid(&self) -> bool {
        self.distance_m.is_finite()
            && self.distance_m >= 0.
            && self.distance_hysteresis_m.is_finite()
            && self.distance_hysteresis_m >= 0.
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ActorSceneUsage {
    pub instances: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub primitives: usize,
    pub animation_nodes: usize,
    pub animation_channels: usize,
    /// Texture slots, including aliases, counted once per retained model.
    pub texture_pixels: usize,
    pub resident_models: usize,
    /// Explicit payload estimate including loading/retiring reservations.
    pub estimated_bytes: usize,
}
impl ActorSceneUsage {
    fn plus(self, rhs: Self) -> Self {
        Self {
            instances: self.instances.saturating_add(rhs.instances),
            vertices: self.vertices.saturating_add(rhs.vertices),
            triangles: self.triangles.saturating_add(rhs.triangles),
            primitives: self.primitives.saturating_add(rhs.primitives),
            animation_nodes: self.animation_nodes.saturating_add(rhs.animation_nodes),
            animation_channels: self
                .animation_channels
                .saturating_add(rhs.animation_channels),
            texture_pixels: self.texture_pixels.saturating_add(rhs.texture_pixels),
            resident_models: self.resident_models.saturating_add(rhs.resident_models),
            estimated_bytes: self.estimated_bytes.saturating_add(rhs.estimated_bytes),
        }
    }
    fn fits(self, limit: &ActorSceneLimits) -> bool {
        self.instances <= limit.instances
            && self.vertices <= limit.vertices
            && self.triangles <= limit.triangles
            && self.primitives <= limit.primitives
            && self.animation_nodes <= limit.animation_nodes
            && self.animation_channels <= limit.animation_channels
            && self.texture_pixels <= limit.texture_pixels
            && self.estimated_bytes <= limit.estimated_bytes
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ActorSceneBudgetDiagnostics {
    pub limits: ActorSceneLimits,
    /// Requested stable selection; may still be waiting for its loader.
    pub selected: ActorSceneUsage,
    /// Includes old scene instances through teardown and all retained models.
    pub reserved: ActorSceneUsage,
    pub eligible_instances: usize,
    pub deferred_instances: usize,
    pub hidden_instances: usize,
    pub distant_instances: usize,
    pub unmetered_instances: usize,
    pub loading_models: usize,
    pub retiring_models: usize,
    pub immutable_pack_estimated_bytes: usize,
    pub peak_reserved_estimated_bytes: usize,
    /// Reservations exceed the current limit. Retirement can reduce this, but
    /// an immutable-pack floor above the byte limit needs a larger limit.
    pub draining: bool,
    pub invalid_limits: bool,
}
impl Default for ActorSceneBudgetDiagnostics {
    fn default() -> Self {
        Self {
            limits: ActorSceneLimits::for_quality(RenderQuality::Balanced),
            selected: ActorSceneUsage::default(),
            reserved: ActorSceneUsage::default(),
            eligible_instances: 0,
            deferred_instances: 0,
            hidden_instances: 0,
            distant_instances: 0,
            unmetered_instances: 0,
            loading_models: 0,
            retiring_models: 0,
            immutable_pack_estimated_bytes: 0,
            peak_reserved_estimated_bytes: 0,
            draining: false,
            invalid_limits: false,
        }
    }
}

pub(super) fn instance_cost(appearance: &ActorSceneAppearance) -> ActorSceneUsage {
    let Some(model) = &appearance.model else {
        return ActorSceneUsage::default();
    };
    let b = &model.model.budget;
    let (nodes, channels) = appearance.animation.as_ref().map_or((0, 0), |animation| {
        let p = &animation.policy;
        let channels = [&p.idle, &p.walk, &p.run, &p.feed, &p.drink]
            .into_iter()
            .chain(p.human.as_ref().map(|human| &human.seated))
            .filter_map(|name| animation.model.clips.get(name))
            .map(|clip| clip.channels.len())
            .max()
            .unwrap_or(0);
        let channels = channels.saturating_add(
            p.vehicle_motion
                .as_ref()
                .map_or(0, |rig| rig.wheels.len().saturating_add(rig.steering.len())),
        );
        (b.nodes, channels)
    });
    ActorSceneUsage {
        instances: 1,
        vertices: b.scene_renderer_vertices,
        triangles: b.scene_triangles,
        primitives: b.scene_primitives,
        animation_nodes: nodes,
        animation_channels: channels,
        // 1 KiB/node is an explicit ECS payload allowance, plus the sampler's
        // three transform arrays and entity list. It is not allocator telemetry.
        estimated_bytes: b
            .nodes
            .saturating_add(b.scene_primitives)
            .saturating_add(2)
            .saturating_mul(1024)
            .saturating_add(nodes.saturating_mul(
                3 * std::mem::size_of::<Transform>() + std::mem::size_of::<Entity>(),
            ))
            // Exactly three per-instance materials preserve independent skin,
            // hair and outfit choices. Their immutable source textures remain
            // charged once to the model; tinting cannot add textures.
            .saturating_add(
                appearance
                    .animation
                    .as_ref()
                    .filter(|a| a.policy.human.is_some())
                    .map_or(0, |_| 3 * 4096),
            ),
        ..Default::default()
    }
}
pub(super) fn model_cost(model: &ActorSceneModel) -> ActorSceneUsage {
    let b = &model.model.budget;
    ActorSceneUsage {
        resident_models: 1,
        texture_pixels: b.texture_slot_pixels,
        // Reserve loading and resident payload together for the entire model
        // lifetime. Texture allowance includes decoder/output, CPU and mip/GPU
        // payload; implementation/driver overhead remains outside the estimate.
        estimated_bytes: b
            .bytes
            .saturating_mul(2)
            .saturating_add(b.decoded_scalars.saturating_mul(4))
            .saturating_add(b.renderer_vertices.saturating_mul(96))
            .saturating_add(b.triangles.saturating_mul(12))
            .saturating_add(b.texture_slot_pixels.saturating_mul(24))
            .saturating_add(b.nodes.saturating_mul(1024))
            .saturating_add(b.materials.saturating_add(1).saturating_mul(4096)),
        ..Default::default()
    }
}

pub(super) struct Candidate {
    pub entity: Entity,
    pub body_id: u64,
    pub priority: u8,
    pub distance_m: f32,
    pub visible: bool,
    pub appearance: ActorSceneAppearance,
}

struct ResidentModel {
    model: ActorSceneModel,
    handle: Option<Handle<WorldAsset>>,
    retiring: bool,
    quiet_frames: u8,
}
impl ResidentModel {
    fn loading(&self, server: &AssetServer) -> bool {
        self.handle.as_ref().is_some_and(|handle| {
            matches!(
                server.load_state(handle),
                LoadState::Loading | LoadState::NotLoaded
            )
        })
    }
    fn any_assets_managed(&self, server: &AssetServer) -> bool {
        self.model.model.asset_labels.iter().any(|label| {
            let path = if label.is_empty() {
                self.model.path.clone()
            } else {
                format!("{}#{label}", self.model.path)
            };
            !server.get_path_ids(path).is_empty()
        })
    }
}

#[derive(Resource, Default)]
pub(super) struct ActorSceneAdmission {
    models: BTreeMap<String, ResidentModel>,
    incumbents: BTreeSet<Entity>,
    bands: BTreeMap<u64, u32>,
    pub failed_models: BTreeSet<String>,
    pub immutable_estimated_bytes: usize,
    pub diagnostics: ActorSceneBudgetDiagnostics,
}
impl ActorSceneAdmission {
    fn refresh(&mut self, server: &AssetServer) {
        self.models.retain(|key, resident| {
            if resident.handle.as_ref().is_some_and(|handle| {
                matches!(server.load_state(handle), LoadState::Failed(_))
                    || matches!(
                        server.get_recursive_dependency_load_state(handle),
                        Some(bevy::asset::RecursiveDependencyLoadState::Failed(_))
                    )
            }) {
                self.failed_models.insert(key.clone());
                resident.retiring = true;
            }
            // Dropping a still-loading scene would hide its unfinished loader
            // from the accounting ledger. Retain it until that load settles.
            if resident.retiring && !resident.loading(server) {
                resident.handle = None;
                if resident.any_assets_managed(server) {
                    resident.quiet_frames = 0;
                } else {
                    resident.quiet_frames = resident.quiet_frames.saturating_add(1);
                }
                // Covers deferred scene teardown and a main-world/render
                // extraction boundary. GPU-driver reclamation is not measured.
                return resident.quiet_frames < 2;
            }
            true
        });
    }

    pub fn select(
        &mut self,
        mut candidates: Vec<Candidate>,
        current: &BTreeMap<Entity, ActorSceneAppearance>,
        limits: ActorSceneLimits,
        server: &AssetServer,
    ) -> BTreeMap<Entity, ActorSceneAppearance> {
        self.refresh(server);
        let base = ActorSceneUsage {
            estimated_bytes: self.immutable_estimated_bytes,
            ..Default::default()
        };
        let mut report = ActorSceneBudgetDiagnostics {
            limits: limits.clone(),
            immutable_pack_estimated_bytes: self.immutable_estimated_bytes,
            peak_reserved_estimated_bytes: self.diagnostics.peak_reserved_estimated_bytes,
            invalid_limits: !limits.valid(),
            ..Default::default()
        };
        let alive: BTreeSet<_> = candidates.iter().map(|c| c.body_id).collect();
        self.bands.retain(|id, _| alive.contains(id));
        candidates.retain(|candidate| {
            if !candidate.visible {
                report.hidden_instances += 1;
                return false;
            }
            let distance_limit = limits.distance_m
                + if self.incumbents.contains(&candidate.entity) {
                    limits.distance_hysteresis_m
                } else {
                    0.
                };
            if !candidate.distance_m.is_finite() || candidate.distance_m > distance_limit {
                report.distant_instances += 1;
                return false;
            }
            true
        });
        for c in &candidates {
            let band = self
                .bands
                .entry(c.body_id)
                .or_insert((c.distance_m / 100.) as u32);
            if c.distance_m < *band as f32 * 100. - 25.
                || c.distance_m > (*band as f32 + 1.) * 100. + 25.
            {
                *band = (c.distance_m / 100.) as u32;
            }
        }
        candidates.sort_by_key(|c| {
            (
                c.priority,
                self.bands[&c.body_id],
                !self.incumbents.contains(&c.entity),
                c.body_id,
                c.entity,
            )
        });
        report.eligible_instances = candidates.len();
        let mut selected = BTreeMap::new();
        let mut wanted = BTreeMap::<String, ActorSceneModel>::new();
        let mut desired_cost = base;
        if !report.invalid_limits {
            for c in &candidates {
                let model = c.appearance.model.as_ref().expect("pack candidate");
                let key = model.key();
                if self.failed_models.contains(key) {
                    continue;
                }
                let mut next = desired_cost.plus(instance_cost(&c.appearance));
                if !wanted.contains_key(key) {
                    next = next.plus(model_cost(model));
                }
                if next.fits(&limits) {
                    desired_cost = next;
                    selected.insert(c.entity, c.appearance.clone());
                    wanted.insert(key.to_owned(), model.clone());
                }
            }
        }
        self.incumbents = selected.keys().copied().collect();
        for (key, resident) in &mut self.models {
            if !wanted.contains_key(key) {
                resident.retiring = true;
            }
        }
        let mut reserved = self
            .models
            .values()
            .fold(base, |usage, m| usage.plus(model_cost(&m.model)));
        for appearance in current.values() {
            reserved = reserved.plus(instance_cost(appearance));
        }
        let mut loading = self.models.values().filter(|m| m.loading(server)).count();
        // Candidate priority, not hash ordering, determines who receives newly
        // freed loading headroom. Shared models are charged only once.
        for c in &candidates {
            if !selected.contains_key(&c.entity) {
                continue;
            }
            let model = c.appearance.model.as_ref().unwrap();
            if !self.models.contains_key(model.key())
                && loading < limits.concurrent_model_loads
                && reserved.plus(model_cost(model)).fits(&limits)
            {
                reserved = reserved.plus(model_cost(model));
                self.models.insert(
                    model.key().to_owned(),
                    ResidentModel {
                        model: model.clone(),
                        handle: Some(model.load(server)),
                        retiring: false,
                        quiet_frames: 0,
                    },
                );
                loading += 1;
            }
        }
        let mut admitted = BTreeMap::new();
        for c in &candidates {
            let Some(mut appearance) = selected.remove(&c.entity) else {
                continue;
            };
            let model = appearance.model.as_ref().unwrap();
            let Some(resident) = self.models.get(model.key()).filter(|m| !m.retiring) else {
                continue;
            };
            let Some(handle) = &resident.handle else {
                continue;
            };
            appearance.scene = handle.clone();
            if current.get(&c.entity) != Some(&appearance) {
                let next = reserved.plus(instance_cost(&appearance));
                if !next.fits(&limits) {
                    continue;
                }
                reserved = next;
            }
            admitted.insert(c.entity, appearance);
        }
        report.selected = desired_cost;
        report.reserved = reserved;
        report.deferred_instances = report.eligible_instances.saturating_sub(admitted.len());
        report.loading_models = loading;
        report.retiring_models = self.models.values().filter(|m| m.retiring).count();
        report.draining = !reserved.fits(&limits);
        report.peak_reserved_estimated_bytes = report
            .peak_reserved_estimated_bytes
            .max(reserved.estimated_bytes);
        self.diagnostics = report;
        admitted
    }
}

#[cfg(test)]
#[path = "render_budget_tests.rs"]
mod tests;
