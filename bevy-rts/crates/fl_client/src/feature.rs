//! [`ClientFeature`]: the client half of a feature (spec §5.2, ADR-0001).
use bevy::prelude::*;

/// The presentation of one sim feature: visuals, UI panels, sounds, overlays and the decoders
/// for its view channels. Implemented by `fl_feat_x_client` crates.
///
/// A client feature reads only `fl_view` snapshots and events, and changes the sim only through
/// commands issued with [`LocalDriver`](crate::driver::LocalDriver).
pub trait ClientFeature: Send + Sync + 'static {
    /// The id of the `SimFeature` this feature presents; the match must include it.
    fn sim_feature(&self) -> &'static str;
    /// Adds the feature's client plugins, systems and resources.
    fn build(&self, app: &mut App);
}
