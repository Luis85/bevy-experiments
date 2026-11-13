//! Settlement identity: each settlement's buildings wear their own roof
//! colour. Only the blue KayKit variants ship, so the shared atlas is
//! recoloured once per palette (team-blue texels only; stone, timber and
//! plaster keep their colour) and building meshes are pointed at a copy of
//! their material. View entities and assets only.
use std::collections::{HashMap, HashSet};

use bevy::{prelude::*, render::render_resource::TextureFormat};
use npc_framework::prelude::SimPos;
use npc_framework::view_helpers::VisualOf;
use testbed_content::village::{
    VillagePlace,
    colony::Household,
    frontier::world_spec::{Biome, LivingWorldSpec},
    homestead::Cottage,
};

use crate::fog::FogTinted;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum RoofPalette {
    /// The shipped harbour blue: nothing is replaced.
    Blue,
    Moss,
    Terracotta,
    Ochre,
    Plum,
}

impl RoofPalette {
    /// Target hue in degrees, saturation scale and value scale.
    fn shift(self) -> Option<(f32, f32, f32)> {
        match self {
            RoofPalette::Blue => None,
            RoofPalette::Moss => Some((112.0, 0.62, 0.86)),
            RoofPalette::Terracotta => Some((9.0, 0.88, 1.0)),
            RoofPalette::Ochre => Some((41.0, 0.86, 1.08)),
            RoofPalette::Plum => Some((292.0, 0.55, 0.88)),
        }
    }
}

/// A settlement's roof colour follows the land it stands on.
pub(crate) fn palette_for(biome: Biome) -> RoofPalette {
    match biome {
        Biome::Coast => RoofPalette::Blue,
        Biome::Woodland => RoofPalette::Moss,
        Biome::Ridge => RoofPalette::Terracotta,
        Biome::Meadow => RoofPalette::Ochre,
        Biome::Wetland => RoofPalette::Plum,
    }
}

/// The settlement a building belongs to visually: the nearest centre.
pub(crate) fn settlement_palette(spec: &LivingWorldSpec, at: SimPos) -> RoofPalette {
    spec.settlements
        .iter()
        .min_by_key(|settlement| (settlement.center.dist2(at), settlement.key.as_str()))
        .map_or(RoofPalette::Blue, |settlement| {
            palette_for(settlement.biome)
        })
}

fn hsv(rgb: [u8; 3]) -> (f32, f32, f32) {
    let [r, g, b] = rgb.map(|c| c as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (hue, if max <= 0.0 { 0.0 } else { delta / max }, max)
}

fn rgb(hue: f32, saturation: f32, value: f32) -> [u8; 3] {
    let c = value * saturation;
    let h = hue.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    [r, g, b].map(|v| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The atlas's team colour: saturated blue. Grey-blue stone (saturation
/// about 0.1) and every warm texel are left exactly as they are.
pub(crate) fn is_team_blue(texel: [u8; 3]) -> bool {
    let (hue, saturation, value) = hsv(texel);
    (188.0..=238.0).contains(&hue) && saturation >= 0.45 && value >= 0.12
}

pub(crate) fn recolour_texel(texel: [u8; 3], palette: RoofPalette) -> [u8; 3] {
    let Some((hue, saturation_scale, value_scale)) = palette.shift() else {
        return texel;
    };
    if !is_team_blue(texel) {
        return texel;
    }
    let (_, saturation, value) = hsv(texel);
    rgb(
        hue,
        (saturation * saturation_scale).clamp(0.0, 1.0),
        (value * value_scale).clamp(0.0, 1.0),
    )
}

/// The roof colour of a palette as one plain colour, for roofs the view
/// builds itself: the atlas's mid roof blue, recoloured like its texels.
pub(crate) fn roof_colour(palette: RoofPalette) -> Color {
    let [r, g, b] = recolour_texel([35, 104, 170], palette);
    Color::srgb_u8(r, g, b)
}

/// `None` when the image is not plain 8-bit RGBA (then the roof stays blue).
fn recolour_image(image: &Image, palette: RoofPalette) -> Option<Image> {
    if !matches!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm
    ) {
        return None;
    }
    let mut out = image.clone();
    for texel in out.data.as_mut()?.as_chunks_mut::<4>().0 {
        let [r, g, b] = recolour_texel([texel[0], texel[1], texel[2]], palette);
        (texel[0], texel[1], texel[2]) = (r, g, b);
    }
    Some(out)
}

/// On a building model whose meshes should wear this palette.
#[derive(Component, Clone, Copy)]
pub(crate) struct RoofTarget(pub RoofPalette);
#[derive(Component)]
pub(crate) struct RoofDone;
#[derive(Component)]
pub(crate) struct RoofChecked;

#[derive(Resource, Default)]
pub(crate) struct RoofAssets {
    images: HashMap<(AssetId<Image>, RoofPalette), Option<Handle<Image>>>,
    materials: HashMap<(AssetId<StandardMaterial>, RoofPalette), Handle<StandardMaterial>>,
    produced: HashSet<AssetId<StandardMaterial>>,
}

/// Tag the model children of settlement homes and workplaces once.
pub(crate) fn assign(
    mut commands: Commands,
    spec: Res<LivingWorldSpec>,
    roots: Query<(Entity, &VisualOf, Option<&Children>), Without<RoofChecked>>,
    buildings: Query<
        &SimPos,
        (
            Or<(
                With<Household>,
                With<VillagePlace>,
                With<testbed_content::village::civic::Building>,
            )>,
            Without<Cottage>,
        ),
    >,
    models: Query<(), With<WorldAssetRoot>>,
) {
    for (root, of, children) in &roots {
        commands.entity(root).insert(RoofChecked);
        let Ok(position) = buildings.get(of.0) else {
            continue;
        };
        let palette = settlement_palette(&spec, *position);
        if palette == RoofPalette::Blue {
            continue;
        }
        for child in children.into_iter().flatten() {
            if models.contains(*child) {
                commands.entity(*child).insert(RoofTarget(palette));
            }
        }
    }
}

/// Swap the materials under tagged models once their scene, material and
/// atlas are loaded. A fog-dimmed mesh is handed back to the fog pass so it
/// dims the recoloured material instead of restoring the blue one later.
pub(crate) fn apply(
    mut commands: Commands,
    targets: Query<(Entity, &RoofTarget), Without<RoofDone>>,
    children: Query<&Children>,
    // The roof's snow layer is not the kit's: it is left as it is.
    meshes: Query<
        (&MeshMaterial3d<StandardMaterial>, Option<&FogTinted>),
        Without<crate::island_homes::snow::SnowCap>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<RoofAssets>,
) {
    for (target, RoofTarget(palette)) in &targets {
        let mut found = false;
        let mut waiting = false;
        for mesh in children.iter_descendants(target) {
            let Ok((material, tinted)) = meshes.get(mesh) else {
                continue;
            };
            found = true;
            let original = tinted.map_or(&material.0, |tinted| &tinted.0).clone();
            if cache.produced.contains(&original.id()) {
                continue;
            }
            let recoloured = match cache.materials.get(&(original.id(), *palette)) {
                Some(handle) => Some(handle.clone()),
                None => {
                    let Some(source) = materials.get(&original).cloned() else {
                        waiting = true;
                        continue;
                    };
                    let Some(texture) = source.base_color_texture.clone() else {
                        continue;
                    };
                    let image = match cache.images.get(&(texture.id(), *palette)) {
                        Some(image) => image.clone(),
                        None => {
                            let Some(atlas) = images.get(&texture) else {
                                waiting = true;
                                continue;
                            };
                            let image = recolour_image(atlas, *palette);
                            let image = image.map(|image| images.add(image));
                            cache.images.insert((texture.id(), *palette), image.clone());
                            image
                        }
                    };
                    image.map(|image| {
                        let handle = materials.add(StandardMaterial {
                            base_color_texture: Some(image),
                            ..source
                        });
                        cache.produced.insert(handle.id());
                        cache
                            .materials
                            .insert((original.id(), *palette), handle.clone());
                        handle
                    })
                }
            };
            let Some(recoloured) = recoloured else {
                continue;
            };
            // Remove-then-insert makes the material `Added` again, which is
            // what the fog pass watches to (re)apply its dimming.
            commands
                .entity(mesh)
                .remove::<(FogTinted, MeshMaterial3d<StandardMaterial>)>()
                .insert(MeshMaterial3d(recoloured));
        }
        if found && !waiting {
            commands.entity(target).insert(RoofDone);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Texels sampled by the shipped building UVs (see the atmosphere notes).
    const ROOF_BLUES: [[u8; 3]; 4] = [
        [35, 104, 170],
        [33, 80, 150],
        [36, 115, 180],
        [39, 144, 204],
    ];
    const STONE: [u8; 3] = [109, 118, 122];
    const TIMBER: [u8; 3] = [149, 84, 64];

    #[test]
    fn only_roof_blue_is_recoloured_and_shading_steps_survive() {
        for palette in [
            RoofPalette::Moss,
            RoofPalette::Terracotta,
            RoofPalette::Ochre,
            RoofPalette::Plum,
        ] {
            assert_eq!(recolour_texel(STONE, palette), STONE);
            assert_eq!(recolour_texel(TIMBER, palette), TIMBER);
            assert_eq!(recolour_texel([255, 255, 255], palette), [255, 255, 255]);
            let shifted = ROOF_BLUES.map(|texel| recolour_texel(texel, palette));
            for (before, after) in ROOF_BLUES.iter().zip(shifted) {
                assert_ne!(*before, after);
                assert!(!is_team_blue(after), "{palette:?} left {after:?} blue");
            }
            // Darker roof shades stay darker than lighter ones.
            let value = |texel: [u8; 3]| hsv(texel).2;
            assert!(value(shifted[1]) < value(shifted[0]));
            assert!(value(shifted[0]) < value(shifted[3]));
        }
        for texel in ROOF_BLUES {
            assert_eq!(recolour_texel(texel, RoofPalette::Blue), texel);
        }
        let [r, g, b] = recolour_texel(ROOF_BLUES[0], RoofPalette::Terracotta);
        assert!(r > g && r > b, "terracotta is red-led: {r},{g},{b}");
        let [r, g, b] = recolour_texel(ROOF_BLUES[0], RoofPalette::Moss);
        assert!(g > r && g > b, "moss is green-led: {r},{g},{b}");
    }

    #[test]
    fn hsv_round_trips_within_rounding() {
        for texel in [
            [35, 104, 170],
            [200, 30, 90],
            [12, 240, 33],
            [128, 128, 128],
        ] {
            let (h, s, v) = hsv(texel);
            let back = rgb(h, s, v);
            for (a, b) in texel.iter().zip(back) {
                assert!((*a as i32 - b as i32).abs() <= 1, "{texel:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn the_three_authored_settlements_get_three_different_roofs() {
        let spec = LivingWorldSpec::default();
        let palettes: Vec<_> = spec
            .settlements
            .iter()
            .map(|settlement| settlement_palette(&spec, settlement.center))
            .collect();
        assert_eq!(
            palettes,
            vec![
                RoofPalette::Blue,
                RoofPalette::Moss,
                RoofPalette::Terracotta
            ]
        );
        // Every authored home and workplace resolves to its own settlement.
        for settlement in &spec.settlements {
            let own = palette_for(settlement.biome);
            for site in &settlement.sites {
                assert_eq!(settlement_palette(&spec, site.position), own);
            }
            for home in &settlement.homes {
                assert_eq!(settlement_palette(&spec, home.door), own);
            }
        }
        assert_eq!(
            settlement_palette(
                &LivingWorldSpec {
                    settlements: vec![],
                    ..spec
                },
                SimPos::new(0, 0)
            ),
            RoofPalette::Blue
        );
    }

    #[test]
    fn a_recoloured_atlas_changes_only_team_texels() {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension};
        let data = [ROOF_BLUES[0], STONE, TIMBER, ROOF_BLUES[3]]
            .iter()
            .flat_map(|[r, g, b]| [*r, *g, *b, 255])
            .collect::<Vec<u8>>();
        let image = Image::new(
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data.clone(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let out = recolour_image(&image, RoofPalette::Terracotta).unwrap();
        let out = out.data.unwrap();
        assert_ne!(out[0..3], data[0..3]);
        assert_eq!(out[4..12], data[4..12]);
        assert_ne!(out[12..15], data[12..15]);
        assert!(out.as_chunks::<4>().0.iter().all(|texel| texel[3] == 255));
    }
}
