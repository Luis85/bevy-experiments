//! Per-broadcast indexes over immutable authoritative state.
use crate::{GeoSpatialIndex, LivingInterestIndex, SandboxError, SandboxState, SpatialQuery};
use terra_core::GeoPosition;

/// Build once after world advancement and reuse for all peers in that broadcast.
/// The borrow deliberately prevents mutation until the batch is complete.
/// Nothing here is persisted or accepted from a client.
#[derive(Debug)]
pub struct SandboxInterestIndex<'a> {
    pub(crate) state: &'a SandboxState,
    pub(crate) living: LivingInterestIndex<'a>,
    pub(crate) plots: GeoSpatialIndex,
    pub(crate) structures: GeoSpatialIndex,
    pub(crate) farms: GeoSpatialIndex,
    pub(crate) resources: GeoSpatialIndex<&'a str>,
    pub(crate) edits: GeoSpatialIndex<&'a str>,
}

impl<'a> SandboxInterestIndex<'a> {
    pub fn new(state: &'a SandboxState) -> Result<Self, SandboxError> {
        Ok(Self {
            state,
            living: LivingInterestIndex::new(&state.living)?,
            plots: GeoSpatialIndex::from_positions(
                256.,
                state.plots.values().map(|v| (v.id, v.center)),
            )?,
            structures: GeoSpatialIndex::from_positions(
                256.,
                state.structures.values().map(|v| (v.id, v.position)),
            )?,
            farms: GeoSpatialIndex::from_positions(
                256.,
                state.farms.values().map(|v| (v.id, v.position)),
            )?,
            resources: GeoSpatialIndex::from_positions(
                256.,
                state
                    .resource_nodes
                    .values()
                    .map(|v| (v.id.as_str(), v.position)),
            )?,
            edits: GeoSpatialIndex::from_positions(
                256.,
                state.feature_edits.iter().filter_map(|id| {
                    state
                        .world_features
                        .get(id)
                        .map(|v| (id.as_str(), v.position))
                }),
            )?,
        })
    }

    pub fn living(&self) -> &LivingInterestIndex<'a> {
        &self.living
    }

    pub fn query_resources(
        &self,
        center: GeoPosition,
        radius_m: f64,
    ) -> Result<SpatialQuery<&'a str>, SandboxError> {
        self.resources.query(center, radius_m)
    }
}
