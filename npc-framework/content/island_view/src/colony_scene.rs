//! Live terrain and restrained village atmosphere. These are separate view entities.
#[path = "living_scene.rs"]
pub(crate) mod living_scene;
use bevy::prelude::*;
use npc_framework::view_helpers::ViewHidden;
use npc_framework::view_helpers::{ViewSystems, VisualOf};
use std::collections::BTreeSet;

#[derive(Resource, Default)]
struct HiddenArrivals(BTreeSet<Entity>);

pub(crate) fn plugin(app: &mut App) {
    living_scene::plugin(app);
    crate::island_atmosphere::plugin(app);
    crate::island_audio::plugin(app);
    app.init_resource::<HiddenArrivals>().add_systems(
        Update,
        hide_arrivals
            .in_set(ViewSystems::Present)
            .after(crate::fog::FogSet),
    );
}
fn hide_arrivals(
    awaiting: Query<(), With<island_content::village::colony::AwaitingArrival>>,
    mut visuals: Query<(&VisualOf, &mut Visibility), With<crate::visuals::ActorVisual>>,
    mut previous: ResMut<HiddenArrivals>,
    mut hidden: ResMut<ViewHidden>,
) {
    for (of, mut visibility) in &mut visuals {
        if awaiting.contains(of.0) {
            *visibility = Visibility::Hidden;
            previous.0.insert(of.0);
            hidden.0.insert(of.0);
        } else if previous.0.remove(&of.0) {
            *visibility = Visibility::Inherited;
            hidden.0.remove(&of.0);
        }
    }
}
