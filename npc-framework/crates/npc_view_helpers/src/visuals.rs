//! Visual entities for sim entities: each sim entity a game draws gets its
//! own visual root entity carrying [`VisualOf(sim)`](VisualOf). Nothing is
//! ever inserted on the sim entity itself.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;

/// Links a visual entity to the sim entity it presents.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualOf(pub Entity);

/// View-side map sim entity → visual root entity (maintained by the
/// content's visual spawner).
#[derive(Resource, Debug, Default)]
pub struct VisualIndex {
    pub map: BTreeMap<Entity, Entity>,
}

/// Sim entities the view hides right now (for example a fog of war's units
/// that the watched actor does not sense). Content fills it; the generic
/// helpers honour it: no crew or path gizmos for them, they cannot be
/// clicked or box-selected, and a selected one leaves the selection (so no
/// selection ring or order gizmos follow it). Empty by default: nothing is
/// hidden.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewHidden(pub BTreeSet<Entity>);

impl ViewHidden {
    /// Whether the view hides `sim`.
    pub fn hides(&self, sim: Entity) -> bool {
        self.0.contains(&sim)
    }
}

/// Whether `sim` is hidden by an optional [`ViewHidden`] (none: shown).
pub fn hidden(view: Option<&ViewHidden>, sim: Entity) -> bool {
    view.is_some_and(|v| v.hides(sim))
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<VisualIndex>()
        .init_resource::<ViewHidden>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_hidden_hides_only_its_entities() {
        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        let view = ViewHidden([a].into_iter().collect());
        assert!(view.hides(a) && !view.hides(b));
        assert!(hidden(Some(&view), a));
        assert!(!hidden(Some(&view), b));
        assert!(!hidden(None, a), "no resource: nothing hidden");
        assert!(!ViewHidden::default().hides(a));
    }
}
