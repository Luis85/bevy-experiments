//! Instanced crowd rendering, separate from authoritative unit simulation.
//!
//! The rendering integration is adapted from Bevy's MIT/Apache-2.0 custom shader
//! instancing example (see THIRD_PARTY.md). Unlike the teaching example, GPU
//! buffers are retained and grow only when their capacity is insufficient.
//! Extraction shares an immutable Arc snapshot instead of cloning the unit array.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

use bevy::{
    core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d},
    ecs::{
        query::QueryItem,
        system::{SystemParamItem, lifetimeless::*},
    },
    mesh::{MeshVertexBufferLayoutRef, VertexBufferLayout},
    pbr::{
        MeshPipeline, MeshPipelineKey, MeshPipelineSystems, RenderMeshInstances, SetMeshBindGroup,
        SetMeshViewBindGroup, SetMeshViewBindingArrayBindGroup, ViewKeyCache,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::{RenderMesh, RenderMeshBufferInfo, allocator::MeshAllocator},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
            RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
        },
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        sync_component::SyncComponent,
        sync_world::MainEntity,
        view::ExtractedView,
    },
    shader::ShaderCacheError,
};
use bytemuck::{Pod, Zeroable};

/// Shader ABI: three vec4s, 48 bytes. This is render data, not the simulation state.
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct InstanceData {
    /// Current x/z followed by previous published x/z; the GPU interpolates.
    pub positions: [f32; 4],
    /// Linear RGB and model scale.
    pub appearance: [f32; 4],
    /// Heading, walking strength, attack flash and a stable animation phase.
    pub behavior: [f32; 4],
}

/// A tiny per-frame upload replaces rebuilding all instance transforms.
#[derive(Resource, Clone, Copy, Default, ShaderType, ExtractResource)]
#[extract_app(RenderApp)]
pub struct CrowdFrame {
    /// Interpolation alpha, map index, animation time in seconds, reserved.
    pub params: Vec4,
}

#[derive(Component, Clone)]
pub struct CrowdInstances {
    pub data: Arc<Vec<InstanceData>>,
    pub revision: u64,
    pub generation: u64,
    pub source_tick: u64,
    pub point_lod: bool,
}

/// Shared telemetry crosses back from the render world without copying arrays.
/// It reports queued uploads and compiled scene pipelines, not a GPU timing claim.
#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(RenderApp)]
pub struct CrowdRenderHealth(pub Arc<CrowdGpuStatus>);

#[derive(Default)]
pub struct CrowdGpuStatus {
    pub generation: AtomicU64,
    pub source_tick: AtomicU64,
    pub uploaded_points: AtomicUsize,
    pub pipelines_ready: AtomicBool,
    pub shader_error: Mutex<Option<String>>,
}

impl SyncComponent<RenderApp> for CrowdInstances {
    type Target = Self;
}

impl ExtractComponent<RenderApp> for CrowdInstances {
    type QueryData = &'static CrowdInstances;
    type QueryFilter = ();
    type Out = Self;

    fn extract_component(source: QueryItem<'_, '_, Self::QueryData>) -> Option<Self> {
        Some(source.clone())
    }
}

pub struct CrowdRenderPlugin;

impl Plugin for CrowdRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrowdFrame>()
            .init_resource::<CrowdRenderHealth>()
            .add_plugins((
                ExtractComponentPlugin::<CrowdInstances>::default(),
                ExtractResourcePlugin::<CrowdFrame>::default(),
                ExtractResourcePlugin::<CrowdRenderHealth>::default(),
            ));
        app.sub_app_mut(RenderApp)
            .add_render_command::<Transparent3d, DrawCrowd>()
            .init_resource::<SpecializedMeshPipelines<CrowdPipeline>>()
            .init_resource::<FrameBuffer>()
            .add_systems(RenderStartup, init_pipeline.after(MeshPipelineSystems))
            .add_systems(
                Render,
                (
                    queue_crowds.in_set(RenderSystems::QueueMeshes),
                    prepare_buffers.in_set(RenderSystems::PrepareResources),
                    prepare_frame.in_set(RenderSystems::PrepareBindGroups),
                ),
            );
    }
}

#[derive(Resource)]
struct CrowdPipeline {
    shader: Handle<Shader>,
    mesh_pipeline: MeshPipeline,
    frame_layout: BindGroupLayoutDescriptor,
}

fn init_pipeline(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mesh_pipeline: Res<MeshPipeline>,
) {
    commands.insert_resource(CrowdPipeline {
        shader: assets.load("shaders/crowd.wesl"),
        mesh_pipeline: mesh_pipeline.clone(),
        frame_layout: BindGroupLayoutDescriptor::new(
            "legion_frame_layout",
            &BindGroupLayoutEntries::single(
                ShaderStages::VERTEX,
                binding_types::uniform_buffer::<CrowdFrame>(false),
            ),
        ),
    });
}

impl SpecializedMeshPipeline for CrowdPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayoutRef,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut pipeline = self.mesh_pipeline.specialize(key, layout)?;
        pipeline.label = Some("legion_crowd_pipeline".into());
        pipeline.vertex.shader = self.shader.clone();
        pipeline.layout.push(self.frame_layout.clone());
        pipeline.vertex.buffers.push(VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceData>() as u64,
            step_mode: VertexStepMode::Instance,
            attributes: vec![
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 0,
                    shader_location: 3,
                },
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 4,
                },
                VertexAttribute {
                    format: VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 5,
                },
            ],
        });
        if let Some(fragment) = &mut pipeline.fragment {
            fragment.shader = self.shader.clone();
        }
        if let Some(depth) = &mut pipeline.depth_stencil {
            depth.depth_write_enabled = Some(true);
        }
        Ok(pipeline)
    }
}

#[derive(Resource, Default)]
struct FrameBuffer {
    uniform: UniformBuffer<CrowdFrame>,
    bind_group: Option<BindGroup>,
}

fn prepare_frame(
    frame: Res<CrowdFrame>,
    pipeline: Res<CrowdPipeline>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut buffer: ResMut<FrameBuffer>,
) {
    buffer.uniform.set(*frame);
    buffer.uniform.write_buffer(&device, &queue);
    if buffer.bind_group.is_none() {
        buffer.bind_group = Some(device.create_bind_group(
            "legion_frame_bind_group",
            &cache.get_bind_group_layout(&pipeline.frame_layout),
            &BindGroupEntries::single(&buffer.uniform),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn queue_crowds(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    pipeline: Res<CrowdPipeline>,
    mut pipelines: ResMut<SpecializedMeshPipelines<CrowdPipeline>>,
    cache: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    mesh_instances: Res<RenderMeshInstances>,
    crowds: Query<(Entity, &MainEntity, &CrowdInstances)>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<&ExtractedView>,
    view_keys: Res<ViewKeyCache>,
    health: Res<CrowdRenderHealth>,
) {
    let draw_function = draw_functions.read().id::<DrawCrowd>();
    let mut saw_draw = false;
    let mut all_ready = true;
    let mut shader_error = None;
    for view in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let Some(&view_key) = view_keys.get(&view.retained_view_entity) else {
            continue;
        };
        for (entity, main_entity, instances) in &crowds {
            if instances.data.is_empty() {
                continue;
            }
            saw_draw = true;
            let Some(mesh_instance) = mesh_instances.render_mesh_queue_data(*main_entity) else {
                all_ready = false;
                continue;
            };
            let Some(mesh) = meshes.get(mesh_instance.mesh_asset_id()) else {
                all_ready = false;
                continue;
            };
            let key = view_key
                | MeshPipelineKey::from_primitive_topology_and_strip_index(
                    mesh.primitive_topology(),
                    mesh.index_format(),
                );
            let pipeline_id = match pipelines.specialize(&cache, &pipeline, key, &mesh.layout) {
                Ok(id) => id,
                Err(error) => {
                    all_ready = false;
                    shader_error = Some(format!("{error:?}"));
                    continue;
                }
            };
            match cache.get_render_pipeline_state(pipeline_id) {
                CachedPipelineState::Ok(_) => {}
                CachedPipelineState::Err(error) => {
                    all_ready = false;
                    if !matches!(
                        error,
                        ShaderCacheError::ShaderNotLoaded(_)
                            | ShaderCacheError::ShaderImportNotYetAvailable
                    ) {
                        shader_error = Some(error.to_string());
                    }
                }
                _ => all_ready = false,
            }
            phase.add_retained(Transparent3d {
                // Opaque instanced bodies write depth; all poses are world-space.
                sorting_info: TransparentSortingInfo3d::Sorted {
                    mesh_center: Vec3::ZERO,
                    depth_bias: 0.0,
                },
                entity: (entity, *main_entity),
                pipeline: pipeline_id,
                draw_function,
                distance: 0.0,
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: mesh.index_format().is_some(),
            });
        }
    }
    // Crowd shaders can finish before Bevy's terrain/material pipelines. Treat
    // the whole scene as pending until those pipelines have settled as well;
    // otherwise early screenshots and loading UI can show armies over clear sky.
    let scene_ready = cache.waiting_pipelines().next().is_none()
        && cache
            .pipelines()
            .all(|pipeline| matches!(pipeline.state, CachedPipelineState::Ok(_)));
    for pipeline in cache.pipelines() {
        if let CachedPipelineState::Err(error) = &pipeline.state
            && !matches!(
                error,
                ShaderCacheError::ShaderNotLoaded(_)
                    | ShaderCacheError::ShaderImportNotYetAvailable
            )
        {
            shader_error.get_or_insert_with(|| error.to_string());
        }
    }
    health
        .0
        .pipelines_ready
        .store(saw_draw && all_ready && scene_ready, Ordering::Release);
    if let Ok(mut error) = health.0.shader_error.lock() {
        *error = shader_error;
    }
}

#[derive(Component)]
struct InstanceBuffer {
    buffer: Buffer,
    capacity: u64,
    length: u32,
    revision: u64,
}

fn prepare_buffers(
    mut commands: Commands,
    mut crowds: Query<(Entity, &CrowdInstances, Option<&mut InstanceBuffer>)>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    health: Res<CrowdRenderHealth>,
) {
    let mut points = 0;
    let mut source = None;
    let mut coherent = true;
    for (entity, instances, old) in &mut crowds {
        if instances.point_lod {
            points += instances.data.len();
            let version = (instances.generation, instances.source_tick);
            if source.is_some_and(|source| source != version) {
                coherent = false;
            }
            source = Some(version);
        }
        let bytes = bytemuck::cast_slice(instances.data.as_slice());
        let needed = bytes.len() as u64;
        if let Some(mut old) = old
            && old.capacity >= needed
        {
            if old.revision != instances.revision {
                if !bytes.is_empty() {
                    queue.write_buffer(&old.buffer, 0, bytes);
                }
                old.length = instances.data.len() as u32;
                old.revision = instances.revision;
            }
            continue;
        }
        let capacity = needed.max(64).next_power_of_two();
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("legion_crowd_instances"),
            size: capacity,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !bytes.is_empty() {
            queue.write_buffer(&buffer, 0, bytes);
        }
        commands.entity(entity).insert(InstanceBuffer {
            buffer,
            capacity,
            length: instances.data.len() as u32,
            revision: instances.revision,
        });
    }
    if let Some((generation, tick)) = source.filter(|_| coherent) {
        health.0.uploaded_points.store(points, Ordering::Release);
        health.0.source_tick.store(tick, Ordering::Release);
        health.0.generation.store(generation, Ordering::Release);
    }
}

type DrawCrowd = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetMeshBindGroup<2>,
    SetCrowdFrame<3>,
    DrawInstances,
);

struct SetCrowdFrame<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetCrowdFrame<I> {
    type Param = SRes<FrameBuffer>;
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        _view: (),
        _entity: Option<()>,
        buffer: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(group) = &buffer.into_inner().bind_group else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(I, group, &[]);
        RenderCommandResult::Success
    }
}

struct DrawInstances;

impl<P: PhaseItem> RenderCommand<P> for DrawInstances {
    type Param = (
        SRes<RenderAssets<RenderMesh>>,
        SRes<RenderMeshInstances>,
        SRes<MeshAllocator>,
    );
    type ViewQuery = ();
    type ItemQuery = Read<InstanceBuffer>;

    fn render<'w>(
        item: &P,
        _view: (),
        buffer: Option<&'w InstanceBuffer>,
        (meshes, mesh_instances, allocator): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let allocator = allocator.into_inner();
        let Some(instance) = mesh_instances.render_mesh_queue_data(item.main_entity()) else {
            return RenderCommandResult::Skip;
        };
        let mesh_id = instance.mesh_asset_id();
        let Some(mesh) = meshes.into_inner().get(mesh_id) else {
            return RenderCommandResult::Skip;
        };
        let Some(buffer) = buffer else {
            return RenderCommandResult::Skip;
        };
        if buffer.length == 0 {
            return RenderCommandResult::Skip;
        }
        let Some(vertex_slice) = allocator.mesh_vertex_slice(&mesh_id) else {
            return RenderCommandResult::Skip;
        };
        pass.set_vertex_buffer(0, vertex_slice.buffer.slice(..));
        pass.set_vertex_buffer(1, buffer.buffer.slice(..));
        match &mesh.buffer_info {
            RenderMeshBufferInfo::Indexed {
                index_format,
                count,
            } => {
                let Some(index_slice) = allocator.mesh_index_slice(&mesh_id) else {
                    return RenderCommandResult::Skip;
                };
                pass.set_index_buffer(index_slice.buffer.slice(..), *index_format);
                pass.draw_indexed(
                    index_slice.range.start..index_slice.range.start + count,
                    vertex_slice.range.start as i32,
                    0..buffer.length,
                );
            }
            RenderMeshBufferInfo::NonIndexed => pass.draw(vertex_slice.range, 0..buffer.length),
        }
        RenderCommandResult::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_layout_matches_shader_locations() {
        assert_eq!(std::mem::size_of::<InstanceData>(), 48);
        assert_eq!(std::mem::offset_of!(InstanceData, appearance), 16);
        assert_eq!(std::mem::offset_of!(InstanceData, behavior), 32);
    }
}
