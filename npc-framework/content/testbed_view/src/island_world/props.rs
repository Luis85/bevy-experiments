//! Props placed with Shape (`frontier::props::props`): each drawn by the
//! model its catalog entry names when the asset manifest lists it, else by a
//! manifest model matched by words in the prop's key ([`prop_look`]), else a
//! small crate. A prop stands on its body's cells (`body_min..body_max`),
//! turned by its quarter turns, on the ground's height, and is taken away
//! when the simulation removes it. Small props (a flower bed) repeat over
//! their cells. The same table answers the placement ghost
//! ([`placement_look`]). View only: nothing here writes the simulation.
use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use npc_framework::prelude::SimId;
use testbed_content::village::frontier::props::PropView;
use testbed_content::village::frontier::terrain::{TerrainState, TerrainSurface};

/// A placed prop's drawing: the model, its scale, whether it is a Kenney
/// model (recoloured by its materials' names) and how it sits in its body.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PropLook {
    pub(crate) path: String,
    pub(crate) scale: f32,
    pub(crate) kenney: bool,
    /// Extra turn of the model in the prop's frame (a fence model runs along
    /// its Z; a fence piece runs across its front, X).
    pub(crate) turn: f32,
    /// How far the model's origin stands off its middle, model units along
    /// its own X (the KayKit fence is drawn a unit to its -X).
    pub(crate) shift_units: f32,
    /// One model on every body cell (flowers), or one in the middle.
    pub(crate) per_cell: bool,
}

#[derive(Clone, Copy)]
struct Row {
    path: &'static str,
    scale: f32,
    kenney: bool,
    turn: f32,
    shift_units: f32,
    per_cell: bool,
}

impl Row {
    fn look(self) -> PropLook {
        PropLook {
            path: self.path.to_string(),
            scale: self.scale,
            kenney: self.kenney,
            turn: self.turn,
            shift_units: self.shift_units,
            per_cell: self.per_cell,
        }
    }
}

const fn kaykit(path: &'static str, scale: f32) -> Row {
    Row {
        path,
        scale,
        kenney: false,
        turn: 0.0,
        shift_units: 0.0,
        per_cell: false,
    }
}

const fn kenney(path: &'static str, scale: f32) -> Row {
    Row {
        path,
        scale,
        kenney: true,
        turn: 0.0,
        shift_units: 0.0,
        per_cell: false,
    }
}

/// A KayKit fence piece: turned to run across the front, put back on its
/// middle.
const fn fence(path: &'static str) -> Row {
    Row {
        path,
        scale: 1.7,
        kenney: false,
        turn: std::f32::consts::FRAC_PI_2,
        shift_units: 1.0,
        per_cell: false,
    }
}

const fn bed(path: &'static str) -> Row {
    Row {
        path,
        scale: 3.0,
        kenney: true,
        turn: 0.0,
        shift_units: 0.0,
        per_cell: true,
    }
}

/// The generic prop: a small crate.
const UNKNOWN: Row = kaykit("living/props/crate_A_small.gltf", 2.6);

#[cfg(test)]
pub(crate) fn unknown_prop() -> PropLook {
    UNKNOWN.look()
}

/// The table, first match wins: words a key may hold and the look. Only
/// manifest-listed models. No bench or lantern post is listed: a bench is a
/// long crate; a lantern post falls to the crate.
const PROPS: [(&[&str], Row); 18] = [
    (&["barrel"], kaykit("living/props/barrel.gltf", 3.0)),
    (
        &["bench", "seat"],
        kaykit("living/props/crate_long_A.gltf", 2.6),
    ),
    (
        &["crate", "box"],
        kaykit("living/props/crate_A_big.gltf", 2.6),
    ),
    (
        &["stone_wall", "stone_fence", "wall"],
        fence("living/props/fence_stone_straight.gltf"),
    ),
    (
        &["gate"],
        fence("living/props/fence_wood_straight_gate.gltf"),
    ),
    (&["fence"], fence("living/props/fence_wood_straight.gltf")),
    (&["sign"], kenney("living/props/sign.glb", 4.0)),
    (
        &["flower_bed", "flowers"],
        bed("living/nature/flower_redA.glb"),
    ),
    (&["flower"], bed("living/nature/flower_yellowA.glb")),
    (
        &["log_pile", "woodpile", "log_stack", "logs"],
        kenney("living/nature/log_stack.glb", 3.0),
    ),
    (
        &["bucket", "trough"],
        kaykit("living/props/bucket_water.gltf", 3.0),
    ),
    (&["tent"], kaykit("living/props/tent.gltf", 1.6)),
    (
        &["wheelbarrow", "barrow", "cart"],
        kaykit("living/props/wheelbarrow.gltf", 2.8),
    ),
    (&["ladder"], kaykit("living/props/ladder.gltf", 2.4)),
    (&["pallet"], kaykit("living/props/pallet.gltf", 2.6)),
    (&["sack"], kaykit("living/props/sack.gltf", 5.0)),
    (
        &["flag", "banner"],
        kaykit("living/props/flag_yellow.gltf", 8.0),
    ),
    (
        &["rock", "boulder", "stone"],
        kenney("living/nature/rock_largeA.glb", 1.6),
    ),
];

/// The look of a prop with data key `key`, by words in it; the small crate
/// when no word of the table is in it.
pub(crate) fn prop_look(key: &str) -> PropLook {
    let key = key.to_ascii_lowercase();
    PROPS
        .iter()
        .find(|(words, _)| words.iter().any(|word| key.contains(word)))
        .map_or(UNKNOWN, |(_, row)| *row)
        .look()
}

/// The look of a placed prop: the catalog's `model` when `listed` (in the
/// asset manifest), drawn as the table draws that model (or as a KayKit or
/// Kenney model by its file type); else by its key ([`prop_look`]).
pub(crate) fn look_for(key: &str, model: &str, listed: impl Fn(&str) -> bool) -> PropLook {
    if model.is_empty() || !listed(model) {
        return prop_look(key);
    }
    let by_key = prop_look(key);
    if by_key.path == model {
        return by_key;
    }
    if let Some((_, row)) = PROPS.iter().find(|(_, row)| row.path == model) {
        return row.look();
    }
    PropLook {
        path: model.to_string(),
        // A Kenney nature model (`.glb`) is drawn as the scenery's; a
        // KayKit one (`.gltf`) as the dressing's props.
        scale: if model.ends_with(".glb") { 3.0 } else { 2.6 },
        kenney: model.ends_with(".glb"),
        ..by_key
    }
}

/// The asset manifest's paths, read once.
fn manifest_lists(path: &str) -> bool {
    static MANIFEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let text = MANIFEST.get_or_init(|| {
        std::fs::read_to_string(
            crate::assets::asset_dir()
                .path
                .join("living-world-manifest.json"),
        )
        .unwrap_or_default()
    });
    text.contains(&format!("\"{path}\""))
}

/// Where a prop's models stand, in the world: its root (body middle, turned
/// by its quarter turns, on `ground`) and each model in the root's frame.
pub(crate) fn placement(
    view: &PropView,
    look: &PropLook,
    ground: f32,
) -> (Transform, Vec<Transform>) {
    let (lo, hi) = (view.body_min, view.body_max);
    let centre = Vec3::new(
        (lo.0 + hi.0 + 1) as f32 * 0.5,
        ground,
        (lo.1 + hi.1 + 1) as f32 * 0.5,
    );
    let yaw = -f32::from(view.rotation % 4) * std::f32::consts::FRAC_PI_2;
    let root = Transform::from_translation(centre).with_rotation(Quat::from_rotation_y(yaw));
    let turn = Quat::from_rotation_y(look.turn);
    let model = |at: Vec3| {
        Transform::from_translation(at + turn * Vec3::X * (look.shift_units * look.scale))
            .with_rotation(turn)
            .with_scale(Vec3::splat(look.scale))
    };
    if !look.per_cell {
        return (root, vec![model(Vec3::ZERO)]);
    }
    // Every cell of the body, in the root's frame (a quarter turn swaps
    // the body's sides).
    let (width, depth) = ((hi.0 - lo.0 + 1) as f32, (hi.1 - lo.1 + 1) as f32);
    let (across, deep) = if view.rotation % 2 == 1 {
        (depth, width)
    } else {
        (width, depth)
    };
    let mut models = Vec::new();
    for i in 0..across as usize {
        for j in 0..deep as usize {
            models.push(model(Vec3::new(
                -across * 0.5 + 0.5 + i as f32,
                0.0,
                -deep * 0.5 + 0.5 + j as f32,
            )));
        }
    }
    (root, models)
}

/// What a prop was drawn from: redrawn when any of it changes.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    root: Entity,
    view: PropView,
}

/// The props drawn now, by identity.
#[derive(Resource, Default)]
pub(crate) struct PropScene {
    drawn: BTreeMap<SimId, Drawn>,
    since: f32,
}

/// The ground under a prop's middle (water: the sea's level).
fn ground_at(terrain: Option<&TerrainState>, view: &PropView) -> f32 {
    let (x, z) = (
        (view.body_min.0 + view.body_max.0).div_euclid(2),
        (view.body_min.1 + view.body_max.1).div_euclid(2),
    );
    terrain
        .and_then(|terrain| terrain.cell(x, z))
        .map_or(0.0, |cell| {
            if cell.surface == TerrainSurface::Water {
                -0.11
            } else {
                cell.height_mm as f32 / 1000.0
            }
        })
}

/// Which props to draw and which to take away: those placed or changed,
/// and those removed.
pub(crate) fn changes(
    drawn: &BTreeMap<SimId, PropView>,
    now: &[PropView],
) -> (Vec<PropView>, Vec<SimId>) {
    let draw = now
        .iter()
        .filter(|view| drawn.get(&view.id) != Some(*view))
        .cloned()
        .collect();
    let gone = drawn
        .keys()
        .filter(|id| !now.iter().any(|view| view.id == **id))
        .copied()
        .collect();
    (draw, gone)
}

/// Follow the placed props: twice a second, draw the new and changed, take
/// away the removed.
pub(crate) fn sync(world: &mut World) {
    let delta = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    {
        let Some(mut scene) = world.get_resource_mut::<PropScene>() else {
            return;
        };
        scene.since += delta;
        if scene.since < 0.5 {
            return;
        }
        scene.since = 0.0;
    }
    let views = testbed_content::village::frontier::props::props(world);
    let drawn: BTreeMap<SimId, PropView> = world
        .resource::<PropScene>()
        .drawn
        .iter()
        .map(|(id, drawn)| (*id, drawn.view.clone()))
        .collect();
    let (draw, gone) = changes(&drawn, &views);
    if draw.is_empty() && gone.is_empty() {
        return;
    }
    for id in gone.iter().chain(draw.iter().map(|view| &view.id)) {
        let old = world.resource_mut::<PropScene>().drawn.remove(id);
        if let Some(old) = old
            && let Ok(entity) = world.get_entity_mut(old.root)
        {
            entity.despawn();
        }
    }
    let Some(server) = world.get_resource::<AssetServer>().cloned() else {
        return;
    };
    for view in draw {
        let look = look_for(&view.prop, &view.model, manifest_lists);
        let ground = ground_at(world.get_resource::<TerrainState>(), &view);
        let (root_at, models) = placement(&view, &look, ground);
        let scene: Handle<WorldAsset> =
            server.load(GltfAssetLabel::Scene(0).from_asset(look.path.clone()));
        let at = root_at.translation;
        let root = world
            .spawn((
                Name::new(format!("placed {}", view.display)),
                root_at,
                Visibility::default(),
                crate::fog::FogArea::around(at, Vec3::splat(1.0)),
                crate::fog::FogDim::default(),
            ))
            .id();
        for placed in models {
            let mut model = world.spawn((ChildOf(root), WorldAssetRoot(scene.clone()), placed));
            if look.kenney {
                model.observe(crate::colony_scene::living_scene::recolour_kenney);
            } else if look.path.contains("fence") || look.path.contains("wheelbarrow") {
                model.observe(super::tint::on_field_wood_ready);
            }
        }
        world
            .resource_mut::<PropScene>()
            .drawn
            .insert(view.id, Drawn { root, view });
    }
}

/// The ghost of something about to be placed with Shape, by its catalog
/// key: a prop (its look, footprint and whether it blocks) or a farm or
/// garden plot (its footprint; drawn as the field's soil). `None` for a key
/// that is neither (a building's ghost is the build tool's box).
// For the Shape ghost (the UI agent's), not yet called.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlacementLook {
    Prop {
        look: PropLook,
        footprint: (u32, u32),
        solid: bool,
    },
    Plot {
        footprint: (u32, u32),
        plots: usize,
    },
}

/// What the Shape ghost for catalog key `key` should show (the UI asks this
/// and hands the footprint to the build tool's ghost box).
#[allow(dead_code)]
pub(crate) fn placement_look(world: &World, key: &str) -> Option<PlacementLook> {
    if let Some(def) = testbed_content::village::frontier::props::catalog(world)
        .into_iter()
        .find(|def| def.id == key)
    {
        return Some(PlacementLook::Prop {
            look: look_for(&def.id, &def.model, manifest_lists),
            footprint: (def.footprint.width, def.footprint.depth),
            solid: def.solid,
        });
    }
    let def = testbed_content::village::civic::definition(world, key)?;
    let field = def.field.as_ref()?;
    Some(PlacementLook::Plot {
        footprint: (def.footprint.width, def.footprint.depth),
        plots: field.plots as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: u32, prop: &str, model: &str, rotation: u8) -> PropView {
        PropView {
            id: SimId(id),
            prop: prop.into(),
            display: prop.into(),
            model: model.into(),
            x: 10,
            z: -4,
            rotation,
            body_min: (10, -4),
            body_max: (11, -4),
            solid: true,
            settlement: SimId(1),
        }
    }

    #[test]
    fn known_props_find_their_model_and_an_unknown_one_is_a_crate() {
        assert_eq!(prop_look("barrel_stack").path, "living/props/barrel.gltf");
        assert_eq!(prop_look("signpost").path, "living/props/sign.glb");
        assert!(prop_look("signpost").kenney);
        assert_eq!(
            prop_look("fence_piece").path,
            "living/props/fence_wood_straight.gltf"
        );
        assert_eq!(
            prop_look("garden_gate").path,
            "living/props/fence_wood_straight_gate.gltf"
        );
        assert_eq!(
            prop_look("Flower_Bed").path,
            "living/nature/flower_redA.glb"
        );
        assert_eq!(
            prop_look("market_bench").path,
            "living/props/crate_long_A.gltf"
        );
        // Unknown: the small crate, never a panic.
        assert_eq!(prop_look("lantern_post"), unknown_prop());
        assert_eq!(prop_look(""), unknown_prop());
        assert_eq!(prop_look("statue_of_a_founder"), unknown_prop());
    }

    #[test]
    fn the_catalog_model_wins_when_listed_and_falls_back_when_not() {
        let listed = |path: &str| path.starts_with("living/");
        // The shipped barrel: its catalog model, drawn as the table draws it.
        let barrel = look_for("barrel", "living/props/barrel.gltf", listed);
        assert_eq!(
            (barrel.path.as_str(), barrel.scale),
            ("living/props/barrel.gltf", 3.0)
        );
        // A catalog model the table does not know, but listed: drawn by its
        // file type.
        let tent = look_for("market_tent", "living/props/tent.gltf", listed);
        assert_eq!(tent.path, "living/props/tent.gltf");
        let lily = look_for("pond_lily", "living/nature/lily_large.glb", listed);
        assert!(lily.kenney && lily.path.ends_with("lily_large.glb"));
        // Not listed, or empty: by the key, then the crate.
        let unlisted = look_for("barrel", "elsewhere/barrel.gltf", listed);
        assert_eq!(unlisted.path, "living/props/barrel.gltf");
        assert_eq!(look_for("odd_thing", "", listed), unknown_prop());
        assert_eq!(
            look_for("odd_thing", "nowhere/odd.gltf", listed),
            unknown_prop()
        );
    }

    #[test]
    fn a_placed_prop_stands_on_its_body_turned_and_a_bed_fills_its_cells() {
        // A fence piece over cells (10,-4)..(11,-4), rotation 0 and 1.
        let fence = prop_look("fence");
        let (root, models) = placement(&view(5, "fence", "", 0), &fence, 0.4);
        assert_eq!(root.translation, Vec3::new(11.0, 0.4, -3.5));
        assert_eq!(models.len(), 1);
        let (turned, _) = placement(&view(5, "fence", "", 1), &fence, 0.4);
        let ahead = turned.rotation * Vec3::X;
        assert!(
            (ahead - Vec3::Z).length() < 1e-4 || (ahead + Vec3::Z).length() < 1e-4,
            "{ahead}"
        );
        // The fence model is put back on its middle: its origin a unit off.
        let shift = models[0].translation.length();
        assert!((shift - fence.scale).abs() < 1e-4, "{shift}");
        // A flower bed: one flower per cell, inside the body.
        let beds = prop_look("flower_bed");
        let (_, flowers) = placement(&view(6, "flower_bed", "", 0), &beds, 0.0);
        assert_eq!(flowers.len(), 2);
        for flower in &flowers {
            assert!(flower.translation.x.abs() <= 1.0 && flower.translation.z.abs() <= 0.5);
        }
    }

    #[test]
    fn a_placed_prop_is_drawn_once_redrawn_when_changed_and_taken_away_when_removed() {
        let a = view(5, "barrel", "living/props/barrel.gltf", 0);
        let b = view(6, "flag", "living/props/flag_blue.gltf", 2);
        let (draw, gone) = changes(&BTreeMap::new(), &[a.clone(), b.clone()]);
        assert_eq!((draw.len(), gone.len()), (2, 0));
        let drawn: BTreeMap<SimId, PropView> =
            [(a.id, a.clone()), (b.id, b.clone())].into_iter().collect();
        assert_eq!(changes(&drawn, &[a.clone(), b.clone()]), (vec![], vec![]));
        // Turned: redrawn. Removed: taken away.
        let turned = PropView {
            rotation: 3,
            ..a.clone()
        };
        let (draw, gone) = changes(&drawn, &[turned.clone()]);
        assert_eq!(draw, vec![turned]);
        assert_eq!(gone, vec![b.id]);
    }

    #[test]
    fn a_ghost_without_a_catalog_is_nothing_to_draw() {
        let world = World::new();
        assert_eq!(placement_look(&world, "barrel"), None);
        assert_eq!(placement_look(&world, "garden"), None);
    }

    #[test]
    fn every_prop_model_is_listed_in_the_manifest() {
        let manifest = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/living-world-manifest.json"
        ))
        .expect("the manifest");
        for row in PROPS.iter().map(|(_, row)| row).chain([&UNKNOWN]) {
            assert!(
                manifest.contains(&format!("\"{}\"", row.path)),
                "{} is not listed",
                row.path
            );
            assert!(row.scale > 0.5 && row.scale < 10.0);
        }
    }
}
