//! One-time acknowledgment that regional terrain has reached a rendered frame.
//!
//! CPU asset loading and an empty pipeline queue cannot establish this: shaders
//! may not even have been specialized yet. Keep the actual terrain mesh IDs and
//! inspect their GPU buffers and each visible chunk's specialized pipeline after
//! the render graph has run. The next extraction returns that acknowledgment to
//! the host. This is presentation state only; it never advances the simulation.

use std::collections::BTreeMap;

use bevy::{
    pbr::{PreprocessBindGroups, PreprocessPipelines, SpecializedMaterialPipelineCache},
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems,
        mesh::{RenderMesh, allocator::MeshAllocator},
        render_asset::RenderAssets,
        render_resource::{CachedPipelineState, PipelineCache},
        sync_world::MainEntity,
        view::{ExtractedView, RenderVisibleEntities, ViewTarget},
    },
    shader::ShaderCacheError,
};

use crate::colony_scene::living_scene::TerrainChunk;

/// Initial regional-scene presentation progress, available only in a rendered
/// regional host. Readiness remains latched across ordinary New/Continue flows.
#[derive(Resource, Reflect, Clone, Debug, Default, PartialEq, Eq)]
#[reflect(Resource, Default, Clone)]
pub struct SceneReadiness {
    pub ready: bool,
    pub error: Option<String>,
    pub expected_meshes: u32,
    pub prepared_meshes: u32,
    pub pending_pipelines: u32,
    /// Render-graph completions with an actual 3D view target, not update frames.
    pub rendered_frames: u64,
}

#[derive(Resource, Default)]
struct ExpectedTerrain(BTreeMap<MainEntity, AssetId<Mesh>>);

#[derive(Resource, Default)]
struct RenderProgress {
    status: SceneReadiness,
    last_evidence: Option<FrameEvidence>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FrameEvidence {
    expected_meshes: u32,
    prepared_meshes: u32,
    pending_pipelines: u32,
    missing_preprocess_views: u32,
    visible_chunks: u32,
    has_view_target: bool,
    error: Option<String>,
}

impl SceneReadiness {
    fn observe_rendered_frame(&mut self, evidence: &FrameEvidence) {
        if self.ready || self.error.is_some() {
            return;
        }
        self.expected_meshes = evidence.expected_meshes;
        self.prepared_meshes = evidence.prepared_meshes;
        self.pending_pipelines = evidence.pending_pipelines;
        self.error.clone_from(&evidence.error);
        if evidence.has_view_target {
            self.rendered_frames = self.rendered_frames.saturating_add(1);
        }
        self.ready = self.error.is_none()
            && evidence.expected_meshes > 0
            && evidence.prepared_meshes == evidence.expected_meshes
            && evidence.pending_pipelines == 0
            && evidence.missing_preprocess_views == 0
            && evidence.has_view_target;
    }
}

pub(crate) fn plugin(app: &mut App) {
    if !testbed_content::village::frontier::enabled(app.world())
        || app.get_sub_app(RenderApp).is_none()
    {
        return;
    }
    app.init_resource::<SceneReadiness>()
        .register_type::<SceneReadiness>();
    app.sub_app_mut(RenderApp)
        .init_resource::<ExpectedTerrain>()
        .init_resource::<RenderProgress>()
        .add_systems(ExtractSchedule, exchange_progress)
        .add_systems(
            Render,
            sample_rendered_terrain
                .after(RenderSystems::Render)
                .before(RenderSystems::Cleanup),
        );
}

/// Extraction is the renderer's exclusive access to MainWorld. Return the
/// previous rendered frame before collecting the next frame's terrain IDs.
fn exchange_progress(
    mut main: ResMut<MainWorld>,
    mut expected: ResMut<ExpectedTerrain>,
    progress: Res<RenderProgress>,
) {
    if let Some(mut readiness) = main.get_resource_mut::<SceneReadiness>() {
        readiness.set_if_neq(progress.status.clone());
    }
    if progress.status.ready || progress.status.error.is_some() {
        return;
    }
    let mut chunks = main.query_filtered::<(Entity, &Mesh3d), With<TerrainChunk>>();
    // All-water chunk identities deliberately have no Mesh3d. Keep any mesh ID
    // that exists, even if its GPU asset is still absent, so it cannot be skipped.
    expected.0 = chunks
        .iter(&main)
        .map(|(entity, mesh)| (MainEntity::from(entity), mesh.id()))
        .collect();
}

#[derive(Debug, PartialEq, Eq)]
enum PipelineReadiness {
    Ready,
    Pending,
    Failed(String),
}

fn pipeline_readiness(state: &CachedPipelineState) -> PipelineReadiness {
    match state {
        CachedPipelineState::Ok(_) => PipelineReadiness::Ready,
        CachedPipelineState::Queued | CachedPipelineState::Creating(_) => {
            PipelineReadiness::Pending
        }
        // Bevy retries these as imports arrive; they are not compiler failures.
        CachedPipelineState::Err(
            ShaderCacheError::ShaderNotLoaded(_) | ShaderCacheError::ShaderImportNotYetAvailable,
        ) => PipelineReadiness::Pending,
        CachedPipelineState::Err(error) => {
            PipelineReadiness::Failed(format!("Scene shader compilation failed: {error}"))
        }
    }
}

fn include_pipeline(state: &CachedPipelineState, evidence: &mut FrameEvidence) {
    match pipeline_readiness(state) {
        PipelineReadiness::Ready => {}
        PipelineReadiness::Pending => evidence.pending_pipelines += 1,
        PipelineReadiness::Failed(error) => {
            evidence.error.get_or_insert(error);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sample_rendered_terrain(
    expected: Res<ExpectedTerrain>,
    meshes: Option<Res<RenderAssets<RenderMesh>>>,
    allocator: Option<Res<MeshAllocator>>,
    pipelines: Option<Res<PipelineCache>>,
    specialized: Option<Res<SpecializedMaterialPipelineCache>>,
    preprocessing: Option<Res<PreprocessPipelines>>,
    views: Query<
        (
            &ExtractedView,
            &RenderVisibleEntities,
            Has<PreprocessBindGroups>,
        ),
        (With<Camera3d>, With<ViewTarget>),
    >,
    mut progress: ResMut<RenderProgress>,
) {
    if progress.status.ready || progress.status.error.is_some() {
        return;
    }
    let mut evidence = FrameEvidence {
        expected_meshes: expected.0.len() as u32,
        ..default()
    };
    if let (Some(meshes), Some(allocator)) = (meshes, allocator) {
        evidence.prepared_meshes = expected
            .0
            .values()
            .filter(|id| {
                meshes.get(**id).is_some_and(|mesh| {
                    mesh.vertex_count > 0
                        && allocator.mesh_vertex_slice(id).is_some()
                        && (!mesh.indexed() || allocator.mesh_index_slice(id).is_some())
                })
            })
            .count() as u32;
    }
    let mut checked_pipelines = Vec::new();
    for (view, visible, has_preprocess_bind_groups) in &views {
        evidence.has_view_target = true;
        let Some(visible_meshes) = visible.get::<Mesh3d>() else {
            // VisibleEntities starts with an empty Mesh3d class even for an
            // off-island camera. No render-world class means extraction has
            // not established that visibility result yet.
            evidence.pending_pipelines += 1;
            continue;
        };
        let view_pipelines = specialized
            .as_ref()
            .and_then(|cache| cache.get(&view.retained_view_entity));
        let previous_visible = evidence.visible_chunks;
        for (_, entity) in visible_meshes.iter_visible() {
            if !expected.0.contains_key(entity) {
                continue;
            }
            evidence.visible_chunks += 1;
            let Some((pipeline, cache)) = view_pipelines
                .and_then(|cache| cache.get(entity))
                .zip(pipelines.as_ref())
            else {
                // This chunk has not been specialized yet. An empty global
                // pipeline queue is not evidence that its shader is ready.
                evidence.pending_pipelines += 1;
                continue;
            };
            if checked_pipelines.contains(pipeline) {
                continue;
            }
            checked_pipelines.push(*pipeline);
            include_pipeline(cache.get_render_pipeline_state(*pipeline), &mut evidence);
        }
        // DrawMesh skips draws without these groups when GPU preprocessing is
        // enabled, even if the mesh's material pipeline has already compiled.
        if evidence.visible_chunks > previous_visible
            && preprocessing.is_some()
            && !has_preprocess_bind_groups
        {
            evidence.missing_preprocess_views += 1;
        }
    }
    if let Some(cache) = pipelines {
        // Prepare has queued the supported GPU preprocessing variants by this
        // point. Those compute pipelines, postprocessing and UI shaders must
        // also finish: a material pipeline alone does not produce a frame.
        // Keep the explicit per-terrain specialization check above so a global
        // empty queue cannot bypass missing terrain specialization.
        for (id, pipeline) in cache.pipelines().enumerate() {
            if !checked_pipelines.iter().any(|checked| checked.id() == id) {
                include_pipeline(&pipeline.state, &mut evidence);
            }
        }
    } else {
        evidence.pending_pipelines += 1;
    }
    // An intentional off-island/custom camera has no visible terrain pipeline
    // to compile. Its completed target plus every expected uploaded mesh is
    // sufficient. A missing view or an unpopulated terrain list never is.
    progress.status.observe_rendered_frame(&evidence);
    if progress.last_evidence.as_ref() != Some(&evidence) {
        if let Some(error) = &evidence.error {
            error!("{error}");
        } else {
            info!(
                ready = progress.status.ready,
                expected_meshes = evidence.expected_meshes,
                prepared_meshes = evidence.prepared_meshes,
                visible_chunks = evidence.visible_chunks,
                pending_pipelines = evidence.pending_pipelines,
                missing_preprocess_views = evidence.missing_preprocess_views,
                has_view_target = evidence.has_view_target,
                rendered_frames = progress.status.rendered_frames,
                "regional scene presentation progress"
            );
        }
        progress.last_evidence = Some(evidence);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_frame() -> FrameEvidence {
        FrameEvidence {
            expected_meshes: 12,
            prepared_meshes: 12,
            visible_chunks: 8,
            has_view_target: true,
            ..default()
        }
    }

    #[test]
    fn empty_queue_without_expected_geometry_or_a_rendered_view_is_not_ready() {
        let mut state = SceneReadiness::default();
        state.observe_rendered_frame(&FrameEvidence {
            has_view_target: true,
            ..default()
        });
        assert!(!state.ready);
        state.observe_rendered_frame(&FrameEvidence {
            has_view_target: false,
            ..complete_frame()
        });
        assert!(!state.ready);
        assert_eq!(state.rendered_frames, 1);
    }

    #[test]
    fn all_mesh_buffers_and_visible_shaders_must_be_ready() {
        let mut state = SceneReadiness::default();
        state.observe_rendered_frame(&FrameEvidence {
            prepared_meshes: 11,
            ..complete_frame()
        });
        assert!(!state.ready);
        state.observe_rendered_frame(&FrameEvidence {
            pending_pipelines: 1,
            ..complete_frame()
        });
        assert!(!state.ready);
        state.observe_rendered_frame(&complete_frame());
        assert!(state.ready);
        assert_eq!(state.rendered_frames, 3);
        let ready = state.clone();
        state.observe_rendered_frame(&FrameEvidence::default());
        assert_eq!(state, ready, "New/Continue must not reopen initial loading");
    }

    #[test]
    fn compiler_failure_is_retained_and_never_becomes_ready() {
        let PipelineReadiness::Failed(error) = pipeline_readiness(&CachedPipelineState::Err(
            ShaderCacheError::ProcessShaderError("invalid shader fixture".into()),
        )) else {
            panic!("a compiler error must not be treated as pending");
        };
        let mut state = SceneReadiness::default();
        state.observe_rendered_frame(&FrameEvidence {
            error: Some(error.clone()),
            ..complete_frame()
        });
        assert_eq!(state.error.as_deref(), Some(error.as_str()));
        assert!(!state.ready);
        state.observe_rendered_frame(&complete_frame());
        assert_eq!(state.error.as_deref(), Some(error.as_str()));
        assert!(!state.ready);
    }

    #[test]
    fn queued_shaders_and_not_yet_loaded_imports_are_pending() {
        for state in [
            CachedPipelineState::Queued,
            CachedPipelineState::Err(ShaderCacheError::ShaderImportNotYetAvailable),
            CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(
                Handle::<Shader>::default().id(),
            )),
        ] {
            assert_eq!(pipeline_readiness(&state), PipelineReadiness::Pending);
        }
    }

    #[test]
    fn shared_compute_and_postprocessing_must_finish_before_reveal() {
        let mut pending = complete_frame();
        include_pipeline(&CachedPipelineState::Queued, &mut pending);
        let mut state = SceneReadiness::default();
        state.observe_rendered_frame(&pending);
        assert!(!state.ready);
        assert_eq!(state.pending_pipelines, 1);

        let mut missing_bind_groups = complete_frame();
        missing_bind_groups.missing_preprocess_views = 1;
        state.observe_rendered_frame(&missing_bind_groups);
        assert!(!state.ready);

        let mut failed = complete_frame();
        include_pipeline(
            &CachedPipelineState::Err(ShaderCacheError::CreateShaderModule(
                "shared compute fixture".into(),
            )),
            &mut failed,
        );
        state.observe_rendered_frame(&failed);
        assert!(!state.ready);
        assert!(state.error.unwrap().contains("shared compute fixture"));
    }

    #[test]
    fn an_intentional_camera_away_from_the_island_does_not_deadlock() {
        let mut state = SceneReadiness::default();
        state.observe_rendered_frame(&FrameEvidence {
            visible_chunks: 0,
            ..complete_frame()
        });
        assert!(state.ready);
        assert_eq!(state.rendered_frames, 1);
    }

    #[test]
    fn headless_regional_and_legacy_hosts_have_no_loading_resource() {
        let mut regional = App::new();
        regional.insert_resource(testbed_content::world_runtime::ActiveWorld(
            testbed_content::worlds::builtin_preset("living_world").unwrap(),
        ));
        assert!(testbed_content::village::frontier::enabled(
            regional.world()
        ));
        plugin(&mut regional);
        assert!(!regional.world().contains_resource::<SceneReadiness>());

        let mut legacy = App::new();
        legacy.insert_sub_app(RenderApp, SubApp::new());
        plugin(&mut legacy);
        assert!(!legacy.world().contains_resource::<SceneReadiness>());
    }
}
