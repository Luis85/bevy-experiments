//! Crop fields, plot by plot, from `view::fields`.
//!
//! The simulation gives a field its body and its plots' state, not where
//! each plot lies: the view lays the plots out as a grid on the body, in
//! plot order from the entrance side. A plot is a bed of soil with crop rows
//! on it whose colour and height say what stage it is at.
use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use island_content::village::frontier::life_spec::Shock;
use island_content::village::view::{self, FieldView, PlotView};
use npc_framework::prelude::{SimId, SimIndex};
use npc_framework::view_helpers::VisualIndex;

use crate::island_homes::model::BodyFrame;

/// Seconds between two reads of the fields: growth is slow.
const READ_SECS: f32 = 0.5;
/// Soil left between two plots, and around them.
const PATH_M: f32 = 0.28;
/// Height of the bed of soil, and of the tallest crop on it.
const BED_M: f32 = 0.07;
const TALLEST_M: f32 = 0.95;

/// What a plot's soil and crop look like.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tone {
    /// Bare, pale soil: nothing sown.
    Fallow,
    /// Stubble under frost: fields rest in winter.
    Winter,
    /// Dark, turned soil.
    Tilled,
    /// Seedlings: thin pale-green lines.
    Sprout,
    /// A standing green crop.
    Green,
    /// Ripe grain.
    Gold,
    /// A ripe crop that is not grain: fruit over leaves.
    Fruit,
}

/// What a weather shock does to a plot's colour, sRGB: a drought parches
/// crops straw-brown and cracks the soil pale; a blight withers crops to a
/// mottled grey-brown. A harsh winter shows as snow on the ground instead.
pub(crate) fn shocked_colour(tone: Tone, shock: Option<Shock>) -> [u8; 3] {
    let own = tone.colour();
    let towards = |target: [u8; 3], share: f32| -> [u8; 3] {
        std::array::from_fn(|i| {
            (own[i] as f32 + (target[i] as f32 - own[i] as f32) * share).round() as u8
        })
    };
    match (shock, tone) {
        (Some(Shock::Drought), Tone::Sprout | Tone::Green) => towards([176, 150, 82], 0.7),
        (Some(Shock::Drought), Tone::Fruit) => towards([150, 92, 60], 0.5),
        (Some(Shock::Drought), Tone::Tilled | Tone::Fallow) => towards([196, 170, 124], 0.6),
        (Some(Shock::Blight), Tone::Sprout | Tone::Green | Tone::Gold | Tone::Fruit) => {
            towards([102, 92, 64], 0.72)
        }
        _ => own,
    }
}

impl Tone {
    pub(crate) const ALL: [Tone; 7] = [
        Tone::Fallow,
        Tone::Winter,
        Tone::Tilled,
        Tone::Sprout,
        Tone::Green,
        Tone::Gold,
        Tone::Fruit,
    ];
    /// sRGB.
    pub(crate) fn colour(self) -> [u8; 3] {
        match self {
            Tone::Fallow => [166, 138, 96],
            Tone::Winter => [196, 196, 186],
            Tone::Tilled => [88, 60, 40],
            Tone::Sprout => [156, 196, 96],
            Tone::Green => [72, 142, 58],
            Tone::Gold => [226, 178, 58],
            Tone::Fruit => [176, 48, 62],
        }
    }
}

/// How one plot is drawn: its soil, and its crop rows if it has any, with
/// their height in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlotLook {
    pub(crate) soil: Tone,
    pub(crate) crop: Option<(Tone, f32)>,
}

/// What a plot's state looks like. Growth is shown in tenths, so a plot is
/// redrawn ten times between sowing and harvest.
pub(crate) fn plot_look(
    stage: &str,
    growth_permille: u32,
    crop: &str,
    fallow_season: bool,
) -> PlotLook {
    let grown = (growth_permille.min(1000) / 100) as f32 / 10.0;
    match stage {
        "sown" => PlotLook {
            soil: Tone::Tilled,
            crop: Some((Tone::Sprout, 0.06)),
        },
        "growing" => PlotLook {
            soil: Tone::Tilled,
            crop: Some((Tone::Green, 0.16 + 0.6 * grown)),
        },
        "ripe" => PlotLook {
            soil: Tone::Tilled,
            crop: Some((
                if crop.contains("grain") {
                    Tone::Gold
                } else {
                    Tone::Fruit
                },
                TALLEST_M,
            )),
        },
        // Fallow, harvested or lost: the bare bed.
        _ => PlotLook {
            soil: if fallow_season {
                Tone::Winter
            } else {
                Tone::Fallow
            },
            crop: None,
        },
    }
}

/// The plots of a field on its body: centre and size of each in the body's
/// frame (x across, z from the entrance side to the back), in plot order.
/// A grid whose cells are as square as the body allows.
pub(crate) fn plot_rects(width: f32, depth: f32, plots: usize) -> Vec<(Vec2, Vec2)> {
    if plots == 0 {
        return Vec::new();
    }
    let columns = ((plots as f32 * width / depth).sqrt().ceil() as usize).clamp(1, plots);
    let rows = plots.div_ceil(columns);
    let cell = Vec2::new(
        (width - PATH_M) / columns as f32,
        (depth - PATH_M) / rows as f32,
    );
    (0..plots)
        .map(|index| {
            let (column, row) = (index % columns, index / columns);
            (
                Vec2::new(
                    -width * 0.5 + PATH_M * 0.5 + cell.x * (column as f32 + 0.5),
                    -depth * 0.5 + PATH_M * 0.5 + cell.y * (row as f32 + 0.5),
                ),
                cell - Vec2::splat(PATH_M),
            )
        })
        .collect()
}

/// The kit fence's length along its local Z, model units, its scale on
/// the island (a waist-high fence) and the length of one run of it.
const FENCE_MODEL_LENGTH: f32 = 1.155;
const FENCE_SCALE: f32 = 2.0;
const FENCE_SEGMENT_M: f32 = 2.4;
/// How far the kit's fence stands off its model's origin, along -X, units.
const FENCE_EDGE_UNITS: f32 = 1.0;

/// The fence's four runs round a field `width` by `depth`, a little inside
/// its edge: (from, to, whether it is the entrance side).
pub(crate) fn fence_runs(width: f32, depth: f32) -> [(Vec2, Vec2, bool); 4] {
    let (x, z) = (width * 0.5 - 0.1, depth * 0.5 - 0.1);
    [
        (Vec2::new(-x, -z), Vec2::new(x, -z), true),
        (Vec2::new(x, -z), Vec2::new(x, z), false),
        (Vec2::new(x, z), Vec2::new(-x, z), false),
        (Vec2::new(-x, z), Vec2::new(-x, -z), false),
    ]
}

struct PlotVisual {
    bed: Entity,
    rows: Entity,
    look: Option<PlotLook>,
}

struct FieldVisual {
    root: Entity,
    plots: Vec<PlotVisual>,
}

#[derive(Resource, Default)]
pub(crate) struct FieldScene {
    since: f32,
    /// The open construction sites when last read: a field site that
    /// finishes is redrawn as its field in the same frame.
    sites: Vec<SimId>,
    fields: BTreeMap<SimId, FieldVisual>,
    assets: Option<FieldAssets>,
}

struct FieldAssets {
    cube: Handle<Mesh>,
    /// Four ridges along a unit square, one high.
    rows: Handle<Mesh>,
    tones: BTreeMap<Tone, Handle<StandardMaterial>>,
    post: Handle<StandardMaterial>,
    /// The kit's wooden fence and its gate, when there is an asset server.
    fence: Option<(Handle<WorldAsset>, Handle<WorldAsset>)>,
    /// A wheelbarrow left by the gate.
    barrow: Option<Handle<WorldAsset>>,
    /// The shock the tones are coloured for.
    shock: Option<Shock>,
}

fn assets(world: &mut World) -> Option<FieldAssets> {
    if !world.contains_resource::<Assets<Mesh>>()
        || !world.contains_resource::<Assets<StandardMaterial>>()
    {
        return None;
    }
    let ridges: Vec<(Vec3, Vec3)> = (0..4)
        .map(|ridge| {
            (
                Vec3::new(-0.375 + 0.25 * ridge as f32, 0.5, 0.0),
                Vec3::new(0.15, 1.0, 0.96),
            )
        })
        .collect();
    let (cube, rows) = {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        (
            meshes.add(Cuboid::default()),
            meshes.add(super::boxes_mesh(&ridges)),
        )
    };
    let barrow = world.get_resource::<AssetServer>().map(|server| {
        server.load(GltfAssetLabel::Scene(0).from_asset("living/props/wheelbarrow.gltf"))
    });
    let fence = world.get_resource::<AssetServer>().map(|server| {
        let scene = |path: &'static str| server.load(GltfAssetLabel::Scene(0).from_asset(path));
        (
            scene("living/props/fence_wood_straight.gltf"),
            scene("living/props/fence_wood_straight_gate.gltf"),
        )
    });
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let mut matte = |[r, g, b]: [u8; 3]| {
        materials.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            perceptual_roughness: 0.95,
            ..default()
        })
    };
    Some(FieldAssets {
        cube,
        rows,
        tones: Tone::ALL
            .into_iter()
            .map(|tone| (tone, matte(tone.colour())))
            .collect(),
        post: matte([110, 76, 48]),
        fence,
        barrow,
        shock: None,
    })
}

fn spawn_field(
    world: &mut World,
    assets: &FieldAssets,
    root: Entity,
    field: &FieldView,
) -> Vec<PlotVisual> {
    let plot = &field.plot;
    let frame = BodyFrame::of_cells(
        (plot.lo_x, plot.lo_z),
        (plot.hi_x, plot.hi_z),
        plot.rotation,
        plot.door(),
    );
    // The field's root stands on the body's middle with +Z from the
    // entrance into the body (`island_build::scene`): the body's own frame.
    for x in [-1.0, 1.0] {
        for z in [-1.0, 1.0] {
            world.spawn((
                ChildOf(root),
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.post.clone()),
                Transform::from_xyz(
                    x * (frame.width * 0.5 - 0.06),
                    0.3,
                    z * (frame.depth * 0.5 - 0.06),
                )
                .with_scale(Vec3::new(0.12, 0.6, 0.12)),
            ));
        }
    }
    // A waist-high wooden fence round the field, a gate in the middle of
    // the entrance side (-Z).
    if let Some((fence, gate)) = &assets.fence {
        for (from, to, entrance) in fence_runs(frame.width, frame.depth) {
            let length = from.distance(to);
            let count = (length / FENCE_SEGMENT_M).ceil().max(1.0) as usize;
            let step = (to - from) / count as f32;
            let turn = Quat::from_rotation_y(step.x.atan2(step.y));
            for i in 0..count {
                let middle = from + step * (i as f32 + 0.5);
                let gated = entrance && i == count / 2;
                // The kit draws its fence along a hex tile's edge, a unit
                // to the model's -X of its origin: shifted back onto the run.
                let at = Vec3::new(middle.x, 0.0, middle.y)
                    + turn * Vec3::X * (FENCE_EDGE_UNITS * FENCE_SCALE);
                let mut piece = world.spawn((
                    ChildOf(root),
                    WorldAssetRoot(if gated { gate.clone() } else { fence.clone() }),
                    Transform::from_translation(at)
                        .with_rotation(turn)
                        .with_scale(Vec3::new(
                            FENCE_SCALE,
                            FENCE_SCALE,
                            step.length() / FENCE_MODEL_LENGTH,
                        )),
                ));
                // The gate's pale grey crossbars take the timber too.
                if gated {
                    piece.observe(super::tint::on_gate_wood_ready);
                } else {
                    piece.observe(super::tint::on_field_wood_ready);
                }
            }
        }
    }
    // A wheelbarrow by the gate, outside the fence.
    if let Some(barrow) = &assets.barrow {
        world
            .spawn((
                ChildOf(root),
                WorldAssetRoot(barrow.clone()),
                Transform::from_xyz(frame.width * 0.25, 0.0, -frame.depth * 0.5 - 0.9)
                    .with_rotation(Quat::from_rotation_y(1.2))
                    .with_scale(Vec3::splat(2.8)),
            ))
            .observe(super::tint::on_field_wood_ready);
    }
    plot_rects(frame.width, frame.depth, field.plots.len())
        .into_iter()
        .map(|(centre, size)| {
            let bed = world
                .spawn((
                    ChildOf(root),
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.tones[&Tone::Fallow].clone()),
                    Transform::from_xyz(centre.x, BED_M * 0.5, centre.y)
                        .with_scale(Vec3::new(size.x, BED_M, size.y)),
                ))
                .id();
            let rows = world
                .spawn((
                    ChildOf(root),
                    Mesh3d(assets.rows.clone()),
                    MeshMaterial3d(assets.tones[&Tone::Green].clone()),
                    Transform::from_xyz(centre.x, BED_M, centre.y)
                        .with_scale(Vec3::new(size.x, 0.1, size.y)),
                    Visibility::Hidden,
                    // The crop ripples in the wind (`ambient::ripple`).
                    super::ambient::Ripple,
                ))
                .id();
            PlotVisual {
                bed,
                rows,
                look: None,
            }
        })
        .collect()
}

fn dress(
    world: &mut World,
    assets: &FieldAssets,
    visual: &mut PlotVisual,
    plot: &PlotView,
    fallow: bool,
) {
    let look = plot_look(plot.stage, plot.growth_permille, &plot.crop, fallow);
    if visual.look == Some(look) {
        return;
    }
    visual.look = Some(look);
    if let Some(mut soil) = world.get_mut::<MeshMaterial3d<StandardMaterial>>(visual.bed) {
        soil.0 = assets.tones[&look.soil].clone();
    }
    let Ok(mut rows) = world.get_entity_mut(visual.rows) else {
        return;
    };
    match look.crop {
        Some((tone, height)) => {
            if let Some(mut crop) = rows.get_mut::<MeshMaterial3d<StandardMaterial>>() {
                crop.0 = assets.tones[&tone].clone();
            }
            if let Some(mut transform) = rows.get_mut::<Transform>() {
                transform.scale.y = height;
            }
            rows.insert(Visibility::Inherited);
        }
        None => {
            rows.insert(Visibility::Hidden);
        }
    }
}

/// Draw every field's plots and keep them at the simulation's stage. Reads
/// the fields twice a second.
pub(crate) fn sync(world: &mut World) {
    let delta = world
        .get_resource::<Time>()
        .map_or(0.0, |time| time.delta_secs());
    let sites = super::props::open_sites(world);
    {
        let mut scene = world.resource_mut::<FieldScene>();
        scene.since += delta;
        if scene.since < READ_SECS && scene.sites == sites {
            return;
        }
        scene.since = 0.0;
        scene.sites = sites;
    }
    let fields = view::fields(world);
    let mut scene = world.remove_resource::<FieldScene>().expect("field scene");
    if scene.assets.is_none() && !fields.is_empty() {
        scene.assets = assets(world);
    }
    // A shock recolours every plot at once, through the shared tones.
    let shock = world
        .get_resource::<super::weather::WeatherLook>()
        .and_then(|look| look.shock);
    if let Some(assets) = scene.assets.as_mut()
        && assets.shock != shock
    {
        assets.shock = shock;
        if let Some(mut materials) = world.get_resource_mut::<Assets<StandardMaterial>>() {
            for (tone, handle) in &assets.tones {
                if let Some(mut material) = materials.get_mut(handle) {
                    let [r, g, b] = shocked_colour(*tone, shock);
                    material.base_color = Color::srgb_u8(r, g, b);
                }
            }
        }
    }
    if let Some(assets) = &scene.assets {
        // A field whose visual is gone (rebuilt, or another world) is drawn anew.
        scene
            .fields
            .retain(|_, visual| world.get_entity(visual.root).is_ok());
        for field in &fields {
            let root = world
                .get_resource::<SimIndex>()
                .and_then(|index| index.get(field.id))
                .and_then(|entity| {
                    world
                        .get_resource::<VisualIndex>()?
                        .map
                        .get(&entity)
                        .copied()
                })
                .filter(|root| world.get_entity(*root).is_ok());
            let Some(root) = root else { continue };
            if scene
                .fields
                .get(&field.id)
                .is_none_or(|visual| visual.root != root || visual.plots.len() != field.plots.len())
            {
                if let Some(old) = scene.fields.remove(&field.id) {
                    for plot in old.plots {
                        for entity in [plot.bed, plot.rows] {
                            if world.get_entity(entity).is_ok() {
                                world.entity_mut(entity).despawn();
                            }
                        }
                    }
                }
                let plots = spawn_field(world, assets, root, field);
                scene.fields.insert(field.id, FieldVisual { root, plots });
            }
            let visual = scene.fields.get_mut(&field.id).expect("just drawn");
            for (plot, state) in visual.plots.iter_mut().zip(&field.plots) {
                dress(world, assets, plot, state, field.fallow_season);
            }
        }
    }
    world.insert_resource(scene);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drought_parches_and_a_blight_withers_the_plots() {
        for tone in Tone::ALL {
            assert_eq!(shocked_colour(tone, None), tone.colour());
            assert_eq!(
                shocked_colour(tone, Some(Shock::HarshWinter)),
                tone.colour()
            );
        }
        let luma = |[r, g, b]: [u8; 3]| 0.3 * r as f32 + 0.59 * g as f32 + 0.11 * b as f32;
        // A green crop is no longer green under either.
        for shock in [Shock::Drought, Shock::Blight] {
            let [r, g, _] = shocked_colour(Tone::Green, Some(shock));
            assert!(r as f32 > g as f32 * 0.8, "{shock:?}");
        }
        // Parched soil is paler, a blighted crop darker and duller.
        assert!(
            luma(shocked_colour(Tone::Tilled, Some(Shock::Drought))) > luma(Tone::Tilled.colour())
        );
        assert!(luma(shocked_colour(Tone::Gold, Some(Shock::Blight))) < luma(Tone::Gold.colour()));
    }

    #[test]
    fn every_stage_of_a_plot_looks_different_from_the_one_before() {
        let fallow = plot_look("fallow", 0, "", false);
        let sown = plot_look("sown", 0, "grain", false);
        let young = plot_look("growing", 150, "grain", false);
        let grown = plot_look("growing", 900, "grain", false);
        let ripe = plot_look("ripe", 1000, "grain", false);
        assert_eq!(fallow.crop, None);
        assert_ne!(fallow.soil, sown.soil, "turned soil is darker");
        // Sown, growing and ripe differ in colour; growing also in height.
        let (sprout, low) = sown.crop.expect("sprouts");
        let (green, short) = young.crop.expect("crop");
        let (_, tall) = grown.crop.expect("crop");
        let (gold, full) = ripe.crop.expect("crop");
        assert_eq!(
            [sprout, green, gold],
            [Tone::Sprout, Tone::Green, Tone::Gold]
        );
        assert!(
            low < short && short + 0.3 < tall && tall < full,
            "{low} {short} {tall} {full}"
        );
        // Ten steps of growth, each a different height.
        let heights: Vec<f32> = (0..10)
            .map(|tenth| {
                plot_look("growing", tenth * 100 + 50, "grain", false)
                    .crop
                    .expect("crop")
                    .1
            })
            .collect();
        assert!(heights.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            plot_look("growing", 420, "grain", false),
            plot_look("growing", 480, "grain", false),
            "within a tenth nothing is redrawn"
        );
        // Berries ripen red, not gold; winter rests under frost.
        assert_eq!(
            plot_look("ripe", 1000, "village_berries", false)
                .crop
                .expect("crop")
                .0,
            Tone::Fruit
        );
        assert_eq!(plot_look("fallow", 0, "", true).soil, Tone::Winter);
        // Colours far enough apart to tell at settlement zoom: every pair
        // of tones differs by at least 40 in some channel.
        for (index, a) in Tone::ALL.iter().enumerate() {
            for b in &Tone::ALL[index + 1..] {
                let apart = (0..3)
                    .map(|channel| (a.colour()[channel] as i32 - b.colour()[channel] as i32).abs())
                    .max()
                    .unwrap_or(0);
                assert!(apart >= 40, "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn plots_tile_the_body_without_touching() {
        for (width, depth, plots) in [
            (8.0, 6.0, 8),
            (8.0, 6.0, 5),
            (6.0, 8.0, 8),
            (4.0, 4.0, 1),
            (10.0, 3.0, 12),
            // A garden placed with Shape: four plots on five by four metres.
            (5.0, 4.0, 4),
        ] {
            let rects = plot_rects(width, depth, plots);
            assert_eq!(rects.len(), plots);
            for (index, (centre, size)) in rects.iter().enumerate() {
                assert!(
                    size.x > 0.4 && size.y > 0.4,
                    "{width}x{depth}/{plots}: {size}"
                );
                let (lo, hi) = (*centre - *size * 0.5, *centre + *size * 0.5);
                assert!(lo.x > -width * 0.5 && hi.x < width * 0.5);
                assert!(lo.y > -depth * 0.5 && hi.y < depth * 0.5);
                for (other, other_size) in &rects[index + 1..] {
                    let gap = (*centre - *other).abs() - (*size + *other_size) * 0.5;
                    assert!(gap.x > 0.2 || gap.y > 0.2, "plots {index} touch");
                }
            }
            // The first plot is on the entrance side.
            assert!(rects[0].0.y <= rects[plots - 1].0.y);
        }
        // The shipped field: eight plots on eight by six metres, four by two.
        let shipped = plot_rects(8.0, 6.0, 8);
        assert!(
            (shipped[0].1 - Vec2::new(1.65, 2.58)).length() < 0.01,
            "{}",
            shipped[0].1
        );
        assert!(plot_rects(8.0, 6.0, 0).is_empty());
    }
}
