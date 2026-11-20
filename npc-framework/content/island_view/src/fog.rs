//! What is left of the fog of war on the island: nothing is ever fogged.
//! The island has no fog (no host flag, no key, ADR 0044), so there is no
//! mask plane, no dimmed material and no per-tick pass over the visuals. The
//! types stay so the spawners and readers keep one meaning: every visual's
//! look is [`Look::Live`] (its `Fogged` default) and nothing reads `FogDim`
//! or `FogArea` to change it. [`FogSet`] stays an ordering point that runs
//! nothing. The farm view keeps its own perception-scoped crop facts behind
//! [`FogOfWar`], which the island never turns on.
use bevy::prelude::*;
use npc_framework::perception::SeenState;
use npc_framework::prelude::SimPos;

/// Whether the window shows fog of war: never on the island. A resource so
/// the views that keep a perception-scoped reading (`village_homestead`)
/// can still be tested with it on.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FogOfWar {
    pub enabled: bool,
}

/// How a visual is drawn. On the island always [`Look::Live`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    /// As it is now.
    #[default]
    Live,
    /// As it is now, dimmed.
    Dim,
    /// Dimmed, in its last-seen state.
    LastSeen(SeenState),
    /// Not drawn.
    Hidden,
}

impl Look {
    /// Drawn dimmed.
    pub const fn dimmed(self) -> bool {
        matches!(self, Look::Dim | Look::LastSeen(_))
    }

    pub const fn shown(self) -> bool {
        !matches!(self, Look::Hidden)
    }
}

/// Where a marker for a visual with this look stands: its own position
/// (live), its last-seen position, or nowhere (hidden).
pub fn marker_pos(look: Look, pos: SimPos) -> Option<SimPos> {
    match look {
        Look::Hidden => None,
        Look::LastSeen(s) => Some(s.pos),
        Look::Live | Look::Dim => Some(pos),
    }
}

/// A visual's look; on the island always the default, [`Look::Live`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Fogged(pub Look);

/// A view entity that never changes (dressing). A marker only: nothing
/// fogs it, so the cells under it are not kept.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct FogArea;

impl FogArea {
    /// The marker for a dressing piece at `centre ± half`; the place is
    /// not needed while the island has no fog.
    pub fn around(_centre: Vec3, _half: Vec3) -> Self {
        Self
    }
}

/// Whether an entity's meshes are drawn dimmed: never on the island.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FogDim(pub bool);

/// On a mesh swapped to a dimmed material: the original. Nothing swaps
/// one on the island; the palette and the cast still look through it.
#[derive(Component, Clone, Debug)]
pub(crate) struct FogTinted(pub(crate) Handle<StandardMaterial>);

/// An ordering point the visuals and dressing still run after; it holds no
/// system.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FogSet;

/// The fog's resource, always off. No systems.
pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<FogOfWar>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_island_draws_everything_live_and_nothing_fogs_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        plugin(&mut app);
        let root = app
            .world_mut()
            .spawn((Fogged::default(), FogDim::default(), Visibility::Inherited))
            .id();
        for _ in 0..3 {
            app.update();
        }
        let world = app.world();
        assert!(!world.resource::<FogOfWar>().enabled);
        assert_eq!(world.get::<Fogged>(root), Some(&Fogged(Look::Live)));
        assert_eq!(world.get::<FogDim>(root), Some(&FogDim(false)));
        assert_eq!(world.get::<Visibility>(root), Some(&Visibility::Inherited));
        assert_eq!(
            marker_pos(Look::Live, SimPos::new(3, 4)),
            Some(SimPos::new(3, 4))
        );
        assert_eq!(marker_pos(Look::Hidden, SimPos::new(3, 4)), None);
    }
}
