//! Who the regional island's people look like: one family of bodies for the
//! residents, their children and the played character, with variety from
//! the body, a hat, a cape and the cape's colour, each picked from the
//! person's `SimId` (stable, no randomness at draw time). Adults stand
//! [`ADULT_HEIGHT_M`], so the door and bed proportions tuned for that height
//! hold; children are drawn [`CHILD_STATURE`] of it. View only: nothing here
//! writes the simulation, and legacy scenarios keep their own bodies.
//!
//! Capture overrides (read once at start, view only):
//! - `ISLAND_VIEW_CAST=adventurer|civilian|mixed`: the family. `adventurer`
//!   (the default) is the KayKit kit the buildings belong to; `civilian` the
//!   Quaternius villagers; `mixed` the roster before this pass, both kits by
//!   the resident's appearance, at their old sizes.
//! - `ISLAND_VIEW_PLAYER_BODY=<model name>`: the played character's body
//!   (`knight`, `rogue`, `unhooded_rogue`, `barbarian`, `mage`, or a
//!   villager: `village_woman`, `traveller_woman`, `farmer_man`,
//!   `traveller_man`).
use std::collections::BTreeMap;
use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use island_content::village::view;
use npc_framework::prelude::{SimId, SimIndex};
use npc_framework::view_helpers::VisualOf;

use crate::assets::ModelKind;
use crate::fog::FogTinted;
use crate::visuals::{ActorModel, ActorVisual};

/// How tall an adult stands on the island, metres: the height the homes'
/// doors (2.2 m) and beds (1.98 m) were tuned for.
pub(crate) const ADULT_HEIGHT_M: f32 = 1.8;
/// A KayKit adventurer's crown over its feet, in model units (Knight 2.31,
/// the others 2.19 to 2.25).
const KAYKIT_CROWN_UNITS: f32 = 2.2;
/// A child's height against an adult's.
pub(crate) const CHILD_STATURE: f32 = 0.62;
/// Seconds between two looks at who is a child.
const STATURE_SECS: f32 = 1.0;

/// The family of bodies the island's people wear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cast {
    Adventurers,
    Civilians,
    Mixed,
}

impl Cast {
    pub(crate) fn parse(value: Option<&str>) -> Cast {
        match value.map(str::trim) {
            Some("civilian" | "civilians") => Cast::Civilians,
            Some("mixed") => Cast::Mixed,
            _ => Cast::Adventurers,
        }
    }

    /// The family this run uses (`ISLAND_VIEW_CAST`).
    pub(crate) fn current() -> Cast {
        static CAST: OnceLock<Cast> = OnceLock::new();
        *CAST.get_or_init(|| Cast::parse(std::env::var("ISLAND_VIEW_CAST").ok().as_deref()))
    }

    /// The bodies residents are drawn with. The Knight is kept for the
    /// played character; the hooded rogue reads as a cloak from above.
    pub(crate) fn residents(self) -> &'static [ModelKind] {
        match self {
            Cast::Adventurers => &[
                ModelKind::Barbarian,
                ModelKind::Mage,
                ModelKind::UnhoodedRogue,
            ],
            Cast::Civilians => &[
                ModelKind::VillageWoman,
                ModelKind::FarmerMan,
                ModelKind::TravellerWoman,
                ModelKind::TravellerMan,
            ],
            Cast::Mixed => &[],
        }
    }
}

/// The played character's body forced for a capture
/// (`ISLAND_VIEW_PLAYER_BODY`); robots are refused.
pub(crate) fn player_body_override() -> Option<ModelKind> {
    static BODY: OnceLock<Option<ModelKind>> = OnceLock::new();
    *BODY.get_or_init(|| {
        std::env::var("ISLAND_VIEW_PLAYER_BODY")
            .ok()
            .and_then(|name| ModelKind::from_name(name.trim()))
            .filter(|kind| !kind.is_robot())
    })
}

/// What a resident wears over the body: hat (or helmet), cape, and the
/// cape's and hat's colour (an index into [`TINTS`], 0 their own).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Outfit {
    pub(crate) hat: bool,
    pub(crate) cape: bool,
    pub(crate) tint: u8,
}

/// Cloth colours, multiplied into the kit's own (linear).
pub(crate) const TINTS: [[f32; 3]; 6] = [
    [1.0, 1.0, 1.0],
    [1.2, 0.82, 0.62],
    [0.72, 1.05, 0.7],
    [0.66, 0.84, 1.2],
    [1.05, 0.72, 0.95],
    [1.12, 1.02, 0.6],
];

fn pick(id: u32, salt: u32, n: u32) -> u32 {
    let mut h = id.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h % n.max(1)
}

/// A resident's body and outfit: from the `SimId` in the island's family,
/// or (mixed) from the profile's appearance as before this pass.
pub(crate) fn resident(cast: Cast, id: u32, appearance: u8) -> (ModelKind, Option<Outfit>) {
    let roster = cast.residents();
    if roster.is_empty() {
        return (ModelKind::for_resident(appearance), None);
    }
    let kind = roster[pick(id, 1, roster.len() as u32) as usize];
    if cast != Cast::Adventurers {
        return (kind, None);
    }
    (
        kind,
        Some(Outfit {
            // Most wear their hat: it is what tells the bodies apart from
            // above.
            hat: pick(id, 2, 4) != 0,
            cape: pick(id, 3, 3) != 0,
            tint: pick(id, 4, TINTS.len() as u32) as u8,
        }),
    )
}

/// The played character's body on the regional island: the Knight, which
/// no resident wears, whatever the preset's `model` names (the preset is not
/// changed); the capture override (`ISLAND_VIEW_PLAYER_BODY`) wins. In the
/// villager cast it is the traveller; the mixed cast keeps the old look.
pub(crate) fn player(cast: Cast, kind: ModelKind) -> ModelKind {
    player_body(cast, kind, player_body_override())
}

/// [`player`] with the override given.
pub(crate) fn player_body(cast: Cast, kind: ModelKind, forced: Option<ModelKind>) -> ModelKind {
    if let Some(forced) = forced {
        return forced;
    }
    match cast {
        Cast::Adventurers => ModelKind::Knight,
        Cast::Civilians if !kind.civilian_rig() => ModelKind::TravellerMan,
        _ => kind,
    }
}

/// The model's scale for an adult of `kind`: [`ADULT_HEIGHT_M`] tall, or
/// the legacy size in the mixed cast.
pub(crate) fn adult_scale(cast: Cast, kind: ModelKind) -> f32 {
    if cast == Cast::Mixed || kind.civilian_rig() {
        kind.scale()
    } else {
        ADULT_HEIGHT_M / KAYKIT_CROWN_UNITS
    }
}

/// On an actor's model on the island: its adult scale, for [`stature`].
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct IslandBody {
    pub(crate) adult: f32,
}

/// Mesh nodes that make a hat, a helmet or a cape on the KayKit bodies.
fn part(name: &str) -> Option<bool> {
    if name.ends_with("_Hat") || name == "Knight_Helmet" {
        Some(true)
    } else if name.ends_with("_Cape") {
        Some(false)
    } else {
        None
    }
}

/// Tinted copies of a kit's material, by (model, tint).
#[derive(Resource, Default)]
pub(crate) struct OutfitMaterials(BTreeMap<(usize, u8), Handle<StandardMaterial>>);

/// Observer on an island model: take off the hat or cape the outfit does
/// not wear, and colour the rest of it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn on_outfit_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    outfits: Query<(&Outfit, &ActorModel)>,
    actors: Query<&ActorVisual>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut meshes: Query<(&mut MeshMaterial3d<StandardMaterial>, Option<&FogTinted>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cache: Option<ResMut<OutfitMaterials>>,
) {
    let Ok((outfit, model)) = outfits.get(ready.entity) else {
        return;
    };
    let Some(mut cache) = cache else { return };
    let Ok(actor) = actors.get(model.root) else {
        return;
    };
    for node in children.iter_descendants(ready.entity) {
        let Some(hat) = names.get(node).ok().and_then(|name| part(name.as_str())) else {
            continue;
        };
        let worn = if hat { outfit.hat } else { outfit.cape };
        if !worn {
            commands.entity(node).insert(Visibility::Hidden);
            continue;
        }
        if outfit.tint == 0 {
            continue;
        }
        for mesh in std::iter::once(node).chain(children.iter_descendants(node)) {
            let Ok((mut material, dimmed)) = meshes.get_mut(mesh) else {
                continue;
            };
            let original = dimmed.map_or_else(|| material.0.clone(), |d| d.0.clone());
            let key = (actor.kind.index(), outfit.tint);
            let tinted = match cache.0.get(&key) {
                Some(handle) => handle.clone(),
                None => {
                    let Some(mut copy) = materials.get(&original).cloned() else {
                        continue;
                    };
                    let c = copy.base_color.to_linear();
                    let [r, g, b] = TINTS[outfit.tint as usize % TINTS.len()];
                    copy.base_color = Color::LinearRgba(LinearRgba::new(
                        c.red * r,
                        c.green * g,
                        c.blue * b,
                        c.alpha,
                    ));
                    let handle = materials.add(copy);
                    cache.0.insert(key, handle.clone());
                    handle
                }
            };
            if dimmed.is_some() {
                // The fog puts this back when it lifts.
                commands.entity(mesh).insert(FogTinted(tinted));
            } else {
                material.0 = tinted;
            }
        }
    }
}

/// Draw children smaller than adults, and an adult again once grown: looks
/// at each person's life stage once a second. Exclusive, as the life stage
/// is read through `village::view::age`.
pub(crate) fn stature(world: &mut World, mut since: Local<f32>) {
    let delta = world.resource::<Time>().delta_secs();
    *since += delta;
    if *since < STATURE_SECS {
        return;
    }
    *since = 0.0;
    let mut models = world.query::<(Entity, &ActorModel, &IslandBody, &Transform)>();
    let rows: Vec<(Entity, Entity, f32, f32)> = models
        .iter(world)
        .map(|(entity, model, body, transform)| (entity, model.root, body.adult, transform.scale.x))
        .collect();
    if rows.is_empty() {
        return;
    }
    let mut changes = Vec::new();
    for (entity, root, adult, now) in rows {
        let Some(sim) = world.get::<VisualOf>(root).map(|of| of.0) else {
            continue;
        };
        let child = world
            .get::<SimId>(sim)
            .copied()
            .filter(|_| world.contains_resource::<SimIndex>())
            .and_then(|id| view::age(world, id))
            .is_some_and(|age| age.stage == "child");
        let want = adult * if child { CHILD_STATURE } else { 1.0 };
        if (want - now).abs() > 1e-4 {
            changes.push((entity, want));
        }
    }
    for (entity, want) in changes {
        if let Some(mut transform) = world.get_mut::<Transform>(entity) {
            transform.scale = Vec3::splat(want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_family_with_stable_variety() {
        assert_eq!(Cast::parse(None), Cast::Adventurers);
        assert_eq!(Cast::parse(Some("civilian")), Cast::Civilians);
        assert_eq!(Cast::parse(Some("mixed")), Cast::Mixed);
        // Every resident of the adventurer cast wears a KayKit body, never
        // the played character's Knight, and the same id gives the same look.
        let mut bodies = std::collections::BTreeSet::new();
        let mut looks = std::collections::BTreeSet::new();
        for id in 1..400 {
            let (kind, outfit) = resident(Cast::Adventurers, id, (id % 8) as u8);
            assert!(!kind.civilian_rig() && kind != ModelKind::Knight && !kind.is_robot());
            assert_eq!(resident(Cast::Adventurers, id, 0), (kind, outfit));
            let outfit = outfit.expect("an adventurer has an outfit");
            bodies.insert(kind.index());
            looks.insert((kind.index(), outfit.hat, outfit.cape, outfit.tint));
        }
        assert_eq!(bodies.len(), 3);
        assert!(looks.len() > 40, "{}", looks.len());
        // Villagers wear only villager bodies; mixed keeps the old roster.
        for id in 1..50 {
            assert!(resident(Cast::Civilians, id, 5).0.civilian_rig());
            assert_eq!(
                resident(Cast::Mixed, id, (id % 8) as u8).0,
                ModelKind::for_resident((id % 8) as u8)
            );
        }
    }

    #[test]
    fn adults_stand_one_metre_eighty_and_children_smaller() {
        for kind in [
            ModelKind::Barbarian,
            ModelKind::Mage,
            ModelKind::UnhoodedRogue,
            ModelKind::Knight,
        ] {
            let tall = adult_scale(Cast::Adventurers, kind) * KAYKIT_CROWN_UNITS;
            assert!((tall - ADULT_HEIGHT_M).abs() < 0.01, "{tall}");
            // The mixed cast keeps the legacy size.
            assert_eq!(adult_scale(Cast::Mixed, kind), kind.scale());
        }
        assert_eq!(adult_scale(Cast::Adventurers, ModelKind::FarmerMan), 1.0);
        let child = ADULT_HEIGHT_M * CHILD_STATURE;
        assert!(child > 1.0 && child < 1.25, "{child}");
        // The played character is the Knight on the island whatever its
        // preset names; the capture override wins; residents never get it.
        for named in [ModelKind::Rogue, ModelKind::Mage, ModelKind::FarmerMan] {
            assert_eq!(
                player_body(Cast::Adventurers, named, None),
                ModelKind::Knight
            );
            assert_eq!(
                player_body(Cast::Adventurers, named, Some(ModelKind::Barbarian)),
                ModelKind::Barbarian
            );
        }
        assert!(
            Cast::Adventurers
                .residents()
                .iter()
                .all(|kind| *kind != ModelKind::Knight)
        );
        // A villager played character becomes the traveller in that cast;
        // the mixed cast keeps the body as before.
        assert_eq!(
            player_body(Cast::Civilians, ModelKind::Rogue, None),
            ModelKind::TravellerMan
        );
        assert_eq!(
            player_body(Cast::Mixed, ModelKind::Rogue, None),
            ModelKind::Rogue
        );
    }

    #[test]
    fn hats_and_capes_are_found_by_name() {
        assert_eq!(part("Mage_Hat"), Some(true));
        assert_eq!(part("Barbarian_Hat"), Some(true));
        assert_eq!(part("Knight_Helmet"), Some(true));
        assert_eq!(part("Rogue_Cape"), Some(false));
        assert_eq!(part("Mage_Head"), None);
        assert_eq!(part("Knight_Head"), None);
    }
}
