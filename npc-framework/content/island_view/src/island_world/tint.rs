//! Recolouring kit models through their textures, so one part of a model
//! changes and the rest keeps its colours. Kenney's ship kit paints whole
//! models from one palette texture, its wood a light toy peach next to
//! white sails and grey iron. A copy of the texture with only the warm wood
//! colours remapped gives brown hulls with pale sails. The copy is made
//! once per texture and shared by every model that uses it. View only.
use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

/// What a copy is recoloured for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Recolour {
    /// The ship kit's peach wood weathered to brown; sails, rope and iron
    /// keep theirs.
    Wood,
    /// Farm timber (fences, gates, the barrow): a mid brown, lighter than
    /// the ship's oak, whatever the kit's own shade of it.
    Field,
    /// A field gate: farm timber, its pale grey crossbars included.
    Gate,
}

/// A colour of the field gate's palette: the farm timber's mid brown, and
/// its pale neutral greys (the crossbars) a little darker brown; dark
/// greys (iron) keep theirs.
pub(crate) fn gate_wood(rgb: [u8; 3]) -> [u8; 3] {
    let timber = field_wood(rgb);
    if timber != rgb {
        return timber;
    }
    let [r, g, b] = rgb.map(f32::from);
    let light = 0.3 * r + 0.59 * g + 0.11 * b;
    let spread = r.max(g).max(b) - r.min(g).min(b);
    if spread < 24.0 && (110.0..=240.0).contains(&light) {
        field_wood([120, 80, 56]).map(|channel| (f32::from(channel) * 0.9) as u8)
    } else {
        rgb
    }
}

/// A colour of a kit's palette as farm timber: its warm wood tones become
/// one mid brown, kept a little lighter or darker with the kit's own shade,
/// everything else stays. Lit at noon it samples about R 160, R - B 65.
pub(crate) fn field_wood([r, g, b]: [u8; 3]) -> [u8; 3] {
    let (rf, gf, bf) = (r as f32, g as f32, b as f32);
    if !(rf > bf + 25.0 && rf >= gf) {
        return [r, g, b];
    }
    let light = 0.3 * rf + 0.59 * gf + 0.11 * bf;
    let shade = (light / 140.0).clamp(0.8, 1.2);
    let brown = [95.0, 69.0, 52.0];
    let tone = |channel: usize| (brown[channel] * shade).clamp(0.0, 255.0) as u8;
    [tone(0), tone(1), tone(2)]
}

/// The snow on a plant's upward faces: a copy of `mesh` whose vertex
/// colours turn faces whose normal points up (from about 35 degrees off
/// level, most of the way) to `snow`, under a material of colour `base` (both linear). The
/// sides keep the material's own colour (a vertex colour of one). `None`
/// when the mesh has no normals.
pub(crate) fn snow_dusted(mesh: &Mesh, base: LinearRgba, snow: LinearRgba) -> Option<Mesh> {
    use bevy::mesh::VertexAttributeValues;
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        return None;
    };
    let gain = |s: f32, b: f32| s / b.max(1e-4);
    let top = [
        gain(snow.red, base.red),
        gain(snow.green, base.green),
        gain(snow.blue, base.blue),
    ];
    let own: Option<Vec<[f32; 4]>> = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(colours)) => Some(colours.clone()),
        _ => None,
    };
    let colours: Vec<[f32; 4]> = normals
        .iter()
        .enumerate()
        .map(|(i, normal)| {
            let up = Vec3::from_array(*normal).normalize_or_zero().y;
            // Only the flattest tops, and not wholly white: a dusting.
            let t = ((up - 0.78) / 0.17).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t) * SNOW_COVER;
            let was = own.as_ref().map_or([1.0; 4], |own| own[i]);
            [
                was[0] * (1.0 + (top[0] - 1.0) * t),
                was[1] * (1.0 + (top[1] - 1.0) * t),
                was[2] * (1.0 + (top[2] - 1.0) * t),
                was[3],
            ]
        })
        .collect();
    let mut copy = mesh.clone();
    copy.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    Some(copy)
}

/// How far the flattest tops go to snow: a dusting, the plant showing through.
const SNOW_COVER: f32 = 0.8;

/// The snow's colour on plants (sRGB), a little blue in the shade of the sky.
pub(crate) const SNOW: [f32; 3] = [0.88, 0.9, 0.93];

/// A colour of a kit's palette as weathered wood: its warm peach and salmon
/// tones turn to oak, everything else stays. Drawn under a bright sun and
/// a filmic tonemapper the texel comes out some 1.4 times lighter, so the
/// browns are dark here: the saturated salmon of hulls and planks becomes
/// a dark oak, the pale peach of decks and trim a mid brown, keeping the
/// contrast the kit has within a model.
pub(crate) fn weathered([r, g, b]: [u8; 3]) -> [u8; 3] {
    let (rf, gf, bf) = (r as f32, g as f32, b as f32);
    if !(rf > bf + 40.0 && rf >= gf) {
        return [r, g, b];
    }
    let light = 0.3 * rf + 0.59 * gf + 0.11 * bf;
    // Pale peach (green near red) is the deck's; deep salmon the hull's.
    let pale = ((gf / rf.max(1.0) - 0.58) / 0.2).clamp(0.0, 1.0);
    let oak = [0.33, 0.23, 0.17];
    let deck = [0.44, 0.33, 0.24];
    let tone = |channel: usize| {
        (light * (oak[channel] + (deck[channel] - oak[channel]) * pale)).clamp(0.0, 255.0) as u8
    };
    [tone(0), tone(1), tone(2)]
}

impl Recolour {
    /// A key for the caches.
    fn key(self) -> u8 {
        match self {
            Recolour::Wood => 0,
            Recolour::Field => 1,
            Recolour::Gate => 2,
        }
    }
}

fn remap(recolour: Recolour, rgb: [u8; 3]) -> [u8; 3] {
    match recolour {
        Recolour::Wood => weathered(rgb),
        Recolour::Field => field_wood(rgb),
        Recolour::Gate => gate_wood(rgb),
    }
}

/// A copy of `image` with every texel remapped; `None` when its texels are
/// not plain 8-bit RGBA in main-world memory.
pub(crate) fn remapped(image: &Image, recolour: Recolour) -> Option<Image> {
    use bevy::render::render_resource::TextureFormat;
    if !matches!(
        image.texture_descriptor.format,
        TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm
    ) {
        return None;
    }
    let mut copy = image.clone();
    let data = copy.data.as_mut()?;
    let (texels, _) = data.as_chunks_mut::<4>();
    for texel in texels {
        let [r, g, b] = remap(recolour, [texel[0], texel[1], texel[2]]);
        texel[0] = r;
        texel[1] = g;
        texel[2] = b;
    }
    Some(copy)
}

/// The recoloured materials, by original material and recolouring.
#[derive(Resource, Default)]
pub(crate) struct Recoloured {
    materials: BTreeMap<(AssetId<StandardMaterial>, u8), Handle<StandardMaterial>>,
    images: BTreeMap<(AssetId<Image>, u8), Handle<Image>>,
}

/// The recoloured copy of `original`: its texture remapped (or, without a
/// texture it can read, its own colour remapped).
pub(crate) fn recoloured(
    original: &Handle<StandardMaterial>,
    recolour: Recolour,
    cache: &mut Recoloured,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Option<Handle<StandardMaterial>> {
    if let Some(handle) = cache.materials.get(&(original.id(), recolour.key())) {
        return Some(handle.clone());
    }
    let mut material = materials.get(original)?.clone();
    match material.base_color_texture.clone() {
        Some(texture) => {
            let image = match cache.images.get(&(texture.id(), recolour.key())) {
                Some(handle) => handle.clone(),
                None => {
                    let copy = remapped(images.get(&texture)?, recolour)?;
                    let handle = images.add(copy);
                    cache
                        .images
                        .insert((texture.id(), recolour.key()), handle.clone());
                    handle
                }
            };
            material.base_color_texture = Some(image);
        }
        None => {
            let srgb = material.base_color.to_srgba();
            let to = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
            let [r, g, b] = remap(recolour, [to(srgb.red), to(srgb.green), to(srgb.blue)]);
            material.base_color = Color::srgba_u8(r, g, b, to(srgb.alpha));
        }
    }
    let handle = materials.add(material);
    cache
        .materials
        .insert((original.id(), recolour.key()), handle.clone());
    Some(handle)
}

/// Observer on a ship-kit model: its meshes take the weathered copy.
pub(crate) fn on_wood_ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    meshes: Query<&mut MeshMaterial3d<StandardMaterial>>,
    cache: ResMut<Recoloured>,
    materials: ResMut<Assets<StandardMaterial>>,
    images: ResMut<Assets<Image>>,
) {
    recolour_model(
        ready.entity,
        Recolour::Wood,
        children,
        meshes,
        cache,
        materials,
        images,
    );
}

/// Observer on a farm model (fence, gate, barrow): its wood a mid brown.
pub(crate) fn on_field_wood_ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    meshes: Query<&mut MeshMaterial3d<StandardMaterial>>,
    cache: ResMut<Recoloured>,
    materials: ResMut<Assets<StandardMaterial>>,
    images: ResMut<Assets<Image>>,
) {
    recolour_model(
        ready.entity,
        Recolour::Field,
        children,
        meshes,
        cache,
        materials,
        images,
    );
}

/// Observer on a field gate: its timber and crossbars a mid brown.
pub(crate) fn on_gate_wood_ready(
    ready: On<WorldInstanceReady>,
    children: Query<&Children>,
    meshes: Query<&mut MeshMaterial3d<StandardMaterial>>,
    cache: ResMut<Recoloured>,
    materials: ResMut<Assets<StandardMaterial>>,
    images: ResMut<Assets<Image>>,
) {
    recolour_model(
        ready.entity,
        Recolour::Gate,
        children,
        meshes,
        cache,
        materials,
        images,
    );
}

fn recolour_model(
    root: Entity,
    recolour: Recolour,
    children: Query<&Children>,
    mut meshes: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut cache: ResMut<Recoloured>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for e in children.iter_descendants(root) {
        let Ok(mut material) = meshes.get_mut(e) else {
            continue;
        };
        if let Some(handle) = recoloured(
            &material.0,
            recolour,
            &mut cache,
            &mut materials,
            &mut images,
        ) {
            material.0 = handle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_wood_weathers() {
        // Kenney's peach wood turns brown; sails, iron and rope keep theirs.
        // The hull's salmon becomes a dark oak, dark enough to read brown
        // once lit (about 1.4 times lighter on screen); the deck's peach a
        // lighter brown, so the ship keeps its contrast.
        let hull = weathered([234, 145, 104]);
        assert!(
            hull[0] <= 85 && hull[0] > hull[1] && hull[1] > hull[2],
            "{hull:?}"
        );
        assert!((hull[0] - hull[2]) as f32 * 1.4 <= 75.0, "{hull:?}");
        let deck = weathered([238, 185, 147]);
        assert!(deck[0] > hull[0] + 12, "{deck:?} {hull:?}");
        assert_eq!(weathered([248, 248, 251]), [248, 248, 251]);
        assert_eq!(weathered([69, 69, 79]), [69, 69, 79]);
    }

    #[test]
    fn farm_timber_is_a_mid_brown_lighter_than_the_ships_oak() {
        // The KayKit fence's browns and the ship kit's salmon alike.
        for wood in [
            [150, 100, 70],
            [120, 80, 56],
            [234, 145, 104],
            [190, 130, 90],
        ] {
            let field = field_wood(wood);
            assert!(field[0] > field[1] && field[1] > field[2], "{field:?}");
            assert!((75..=125).contains(&field[0]), "{field:?}");
            let lean = field[0] as i32 - field[2] as i32;
            assert!((34..=55).contains(&lean), "{field:?}");
            assert!(field[0] > weathered([234, 145, 104])[0] + 15);
        }
        assert_eq!(field_wood([248, 248, 251]), [248, 248, 251]);
        assert_eq!(field_wood([90, 140, 60]), [90, 140, 60]);
    }

    #[test]
    fn a_gates_pale_crossbars_turn_to_timber_and_its_iron_stays() {
        let pales = field_wood([150, 100, 70]);
        for grey in [[200, 200, 205], [160, 162, 158], [230, 228, 226]] {
            let bar = gate_wood(grey);
            assert!(bar[0] > bar[1] && bar[1] > bar[2], "{bar:?}");
            assert!(bar[0] <= pales[0] + 4, "{bar:?} {pales:?}");
            assert!(bar[0] >= 60, "{bar:?}");
        }
        // Iron and white paint keep theirs; the timber is the fence's.
        assert_eq!(gate_wood([60, 62, 66]), [60, 62, 66]);
        assert_eq!(gate_wood([250, 250, 252]), [250, 250, 252]);
        assert_eq!(gate_wood([150, 100, 70]), field_wood([150, 100, 70]));
    }

    #[test]
    fn snow_lies_on_the_upward_faces_and_the_sides_keep_their_colour() {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::PrimitiveTopology;
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0, 0.0, 0.0]; 3]);
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            vec![[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]],
        );
        let base = LinearRgba::rgb(0.02, 0.05, 0.03);
        let snow = Color::srgb(SNOW[0], SNOW[1], SNOW[2]).to_linear();
        let dusted = snow_dusted(&mesh, base, snow).expect("normals");
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colours)) =
            dusted.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours");
        };
        // The top: mostly snow, once the material's colour is applied.
        let top = colours[0];
        let expect = |s: f32, b: f32| b + (s - b) * SNOW_COVER;
        assert!((top[0] * base.red - expect(snow.red, base.red)).abs() < 1e-4);
        assert!((top[1] * base.green - expect(snow.green, base.green)).abs() < 1e-4);
        // The side and the underside: the plant's own colour.
        assert_eq!(colours[1], [1.0; 4]);
        assert_eq!(colours[2], [1.0; 4]);
    }

    #[test]
    fn a_texture_copy_remaps_every_texel_and_keeps_its_size() {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let image = Image::new(
            Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![234, 145, 104, 255, 248, 248, 251, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::all(),
        );
        let copy = remapped(&image, Recolour::Wood).expect("rgba8");
        let data = copy.data.expect("texels");
        assert_eq!(&data[4..], &[248, 248, 251, 255]);
        assert_eq!(&data[..3], &weathered([234, 145, 104]));
        assert_eq!(copy.texture_descriptor.size, image.texture_descriptor.size);
    }
}
