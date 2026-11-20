//! Island Village atmosphere: post-processing, night lights, smoke and embers.
//! Installed only for the regional living-world scene; legacy scenarios keep
//! their previous look. Every entity here is a view entity and simulation
//! state is read-only (see `docs/island-atmosphere.md`).
pub(crate) mod daylight;
pub(crate) mod dressing;
pub(crate) mod goods;
pub(crate) mod ground;
pub(crate) mod landmark;
mod legend;
mod markers;
pub(crate) mod night;
pub(crate) mod palette;
mod post;
pub(crate) mod sky;
pub(crate) mod smoke;

use bevy::prelude::*;
pub(crate) use markers::role_color;
use npc_framework::view_helpers::ViewSystems;

/// Whether the Trades overlay shows its key (the legend of marker colours).
pub(crate) fn trades_key(world: &World) -> bool {
    world
        .get_resource::<legend::Legend>()
        .is_some_and(|legend| !legend.collapsed)
}

/// Show or fold the Trades key. View state only.
pub(crate) fn set_trades_key(world: &mut World, shown: bool) {
    if let Some(mut legend) = world.get_resource_mut::<legend::Legend>() {
        legend.collapsed = !shown;
    }
}

pub(crate) fn plugin(app: &mut App) {
    // Capture aids, read once. Neither reaches the simulation.
    app.insert_resource(daylight::ViewDayOverride::parse(
        std::env::var("ISLAND_VIEW_DAY_PHASE").ok().as_deref(),
    ))
    .insert_resource(post::PostEnabled(
        std::env::var("ISLAND_VIEW_POST").as_deref() != Ok("off"),
    ));
    crate::game_ui::install(app);
    app.init_resource::<markers::Markers>()
        .init_resource::<legend::Legend>()
        .add_observer(legend::activate)
        .add_systems(Startup, (markers::setup, legend::spawn))
        .add_systems(Update, markers::sync.in_set(ViewSystems::Sync))
        // The sun follows the season the land is drawn in.
        .add_systems(Update, daylight::follow_season.in_set(ViewSystems::Sync))
        .add_systems(
            Update,
            (markers::present, legend::refresh)
                .in_set(ViewSystems::Present)
                .after(crate::colony_scene::living_scene::elevation),
        );
    app.init_resource::<night::GlowHomes>()
        .init_resource::<smoke::Dressed>()
        .init_resource::<goods::GoodsLinks>()
        .init_resource::<palette::RoofAssets>()
        .init_resource::<crate::island_poses::IslandPoses>()
        .init_resource::<crate::island_poses::ReposeCache>()
        .init_resource::<crate::island_poses::WorkProps>()
        .init_resource::<crate::island_poses::FishingLines>()
        .add_systems(
            Update,
            crate::island_poses::cache_repose.in_set(ViewSystems::Sync),
        )
        .add_systems(
            Update,
            (
                crate::island_poses::work_props,
                crate::island_poses::face_water,
                crate::island_poses::face_site,
                crate::island_poses::fishing_lines,
            )
                .in_set(ViewSystems::Present),
        )
        .add_systems(
            Startup,
            (
                night::spawn_lanterns,
                smoke::setup,
                crate::island_poses::setup_tools,
                goods::setup,
                landmark::spawn,
            ),
        )
        .add_systems(
            Update,
            (
                post::install,
                post::fit_cascades,
                dressing::sync,
                dressing::bush_season,
                night::dress_homes,
                smoke::dress,
                goods::sync,
                (palette::assign, palette::apply).chain(),
            )
                .in_set(ViewSystems::Sync),
        )
        .add_systems(
            Update,
            (
                night::night_lights,
                smoke::animate,
                smoke::blaze.after(smoke::animate),
            )
                .in_set(ViewSystems::Present),
        );
    // Review aid: `ISLAND_VIEW_DRESSING=off` leaves out the outcrop rocks and
    // edge bushes, to measure what they cost.
    if std::env::var("ISLAND_VIEW_DRESSING").as_deref() != Ok("off") {
        app.add_systems(Startup, dressing::spawn);
    }
}
