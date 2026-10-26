//! Frontline client: rendering, camera, input and debug tooling.
use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub mod ai;
pub mod camera;
pub mod debug;
pub mod driver;
pub mod feature;
pub mod link;
pub mod map_view;
pub mod orders;
pub mod overlays;
pub mod placeholder;
pub mod selection;
pub mod terrain;
pub mod view;

pub use ai::AiPlugin;
pub use camera::CameraPlugin;
pub use debug::DebugToolsPlugin;
pub use driver::LocalDriver;
pub use feature::ClientFeature;
pub use link::RunnerLink;
pub use map_view::MapViewPlugin;
pub use orders::OrdersPlugin;
pub use overlays::OverlaysPlugin;
pub use placeholder::PlaceholderPlugin;
pub use selection::SelectionPlugin;
pub use view::SimViewPlugin;

/// All client plugins.
pub struct ClientPlugins;

impl PluginGroup for ClientPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(CameraPlugin)
            .add(MapViewPlugin)
            .add(SimViewPlugin)
            .add(PlaceholderPlugin)
            .add(SelectionPlugin)
            .add(OrdersPlugin)
            .add(OverlaysPlugin)
            .add(AiPlugin)
            .add(DebugToolsPlugin)
    }
}
