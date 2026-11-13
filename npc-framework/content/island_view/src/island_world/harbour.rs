//! The harbour: a jetty of planks on posts from the nearest shore out to the
//! merchant boat's berth, a small quay at its foot with crates, barrels and
//! a lantern, and goods stacked on the quay while the boat is moored.
//!
//! The simulation gives the berth (`view::boat().berth`, beside the trading
//! post at the waterfront) and no jetty: the jetty is laid out from that
//! berth to the nearest dry land, once per berth, as view entities fogged
//! like other dressing. Nothing here has a coordinate of its own. The unloaded goods
//! appear while `view::boat` says the boat is in and the boat is in sight,
//! and go when it sails.
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use island_content::village::frontier::terrain::{TerrainState, TerrainSurface};

use crate::fog::{FogArea, FogDim};

/// Half the moored hull's beam and a hand's breadth of fender: where the
/// jetty's head stands off the berth.
pub(crate) const HULL_OFFSET_M: f32 = super::boat::HULL.beam * 0.5 + 0.3;
/// How far a shore is looked for from the jetty's head, cells.
const SHORE_REACH: i32 = 24;
/// The jetty's width and the pitch of its planks.
pub(crate) const JETTY_WIDTH_M: f32 = 1.8;
const PLANK_PITCH_M: f32 = 0.34;
const PLANK_M: f32 = 0.3;
/// Posts stand at most this far apart along the jetty.
const POST_PITCH_M: f32 = 2.2;
/// The quay at the jetty's foot: across and back from the shore.
pub(crate) const QUAY_SIZE_M: Vec2 = Vec2::new(4.6, 3.4);
/// With the trading post's entrance this near the jetty's foot, the post is
/// the quay: no quay of planks, no stores or unloaded goods of its own.
pub(crate) const POST_IS_QUAY_M: f32 = 6.0;
/// The deck stands at least this high, clear of the swell.
const DECK_LEAST_Y: f32 = 0.32;
/// How far the planked walkway stands above the kit frame's rails.
const DECK_ON_FRAME_M: f32 = 0.04;

/// Where the jetty runs: from `foot` on the shore to `head` beside the hull.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Landing {
    pub(crate) foot: Vec2,
    pub(crate) head: Vec2,
    /// The shore cell under the foot.
    pub(crate) shore: (i32, i32),
}

impl Landing {
    /// Unit vector from the foot out to the head.
    pub(crate) fn along(&self) -> Vec2 {
        (self.head - self.foot).normalize_or(Vec2::Y)
    }

    pub(crate) fn length(&self) -> f32 {
        self.foot.distance(self.head)
    }
}

/// The jetty for a berth: its head stands off the hull's landward side, its
/// foot on the edge of the land cell nearest to that head. `land` says
/// whether a cell is dry ground. `None` when no shore is near.
pub(crate) fn landing(berth: Vec2, out: Vec2, land: impl Fn(i32, i32) -> bool) -> Option<Landing> {
    let head = berth - out * HULL_OFFSET_M;
    let cell = (head.x.floor() as i32, head.y.floor() as i32);
    let mut best: Option<(f32, (i32, i32))> = None;
    for dz in -SHORE_REACH..=SHORE_REACH {
        for dx in -SHORE_REACH..=SHORE_REACH {
            let (x, z) = (cell.0 + dx, cell.1 + dz);
            if !land(x, z) {
                continue;
            }
            let centre = Vec2::new(x as f32 + 0.5, z as f32 + 0.5);
            let reach = centre.distance_squared(head);
            if best.is_none_or(|(least, at)| reach < least || (reach == least && (x, z) < at)) {
                best = Some((reach, (x, z)));
            }
        }
    }
    best.map(|(_, shore)| {
        let centre = Vec2::new(shore.0 as f32 + 0.5, shore.1 as f32 + 0.5);
        // The foot at the cell's edge towards the water.
        let toward = (head - centre).normalize_or(-out);
        let foot = centre + toward * 0.5;
        Landing { foot, head, shore }
    })
}

/// The boxes of the jetty and the quay, in the landing's own frame: +Z from
/// the foot out to the head, origin at the foot on the deck's level
/// (`deck`), `water` the sea's level below. Planks, two stringers and posts
/// for the jetty; planks on bearers for the quay behind the foot.
pub(crate) fn timbers(
    length: f32,
    deck: f32,
    water: f32,
    quay: bool,
) -> (Vec<(Vec3, Vec3)>, Vec<(Vec3, Vec3)>) {
    let mut planks = Vec::new();
    let mut frame = Vec::new();
    let plank_t = 0.07;
    let count = (length / PLANK_PITCH_M).ceil().max(1.0) as i32;
    for i in 0..count {
        let z = (i as f32 + 0.5) * length / count as f32;
        // A slight stagger, so the deck does not read as a grid.
        let shift = if i % 2 == 0 { 0.03 } else { -0.03 };
        planks.push((
            Vec3::new(shift, deck - plank_t * 0.5, z),
            Vec3::new(JETTY_WIDTH_M, plank_t, PLANK_M),
        ));
    }
    let under = deck - plank_t;
    for side in [-1.0, 1.0] {
        frame.push((
            Vec3::new(
                side * (JETTY_WIDTH_M * 0.5 - 0.15),
                under - 0.07,
                length * 0.5,
            ),
            Vec3::new(0.12, 0.14, length),
        ));
    }
    let bays = (length / POST_PITCH_M).ceil().max(1.0) as i32;
    let bottom = water - 1.0;
    for i in 0..=bays {
        let z = i as f32 * length / bays as f32;
        for side in [-1.0, 1.0] {
            let top = deck + if i == bays { 0.55 } else { 0.0 };
            frame.push((
                Vec3::new(side * (JETTY_WIDTH_M * 0.5 - 0.08), (top + bottom) * 0.5, z),
                Vec3::new(0.18, top - bottom, 0.18),
            ));
        }
    }
    if !quay {
        return (planks, frame);
    }
    // The quay: planks running across, behind the foot.
    let rows = (QUAY_SIZE_M.y / PLANK_PITCH_M).ceil() as i32;
    for i in 0..rows {
        let z = -(i as f32 + 0.5) * QUAY_SIZE_M.y / rows as f32;
        planks.push((
            Vec3::new(0.0, deck - plank_t * 0.5, z),
            Vec3::new(QUAY_SIZE_M.x, plank_t, PLANK_M),
        ));
    }
    for x in [-1.0, 0.0, 1.0] {
        frame.push((
            Vec3::new(
                x * (QUAY_SIZE_M.x * 0.5 - 0.2),
                under - 0.25,
                -QUAY_SIZE_M.y * 0.5,
            ),
            Vec3::new(0.16, 0.5, QUAY_SIZE_M.y),
        ));
    }
    (planks, frame)
}

/// On a small boat afloat in the harbour: where it lies at rest and its own
/// phase on the swell.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Afloat {
    pub(crate) base: Transform,
    pub(crate) seed: f32,
}

/// How a small boat rides the swell at view time `seconds` in a wind of
/// `strength`: livelier than the ship (it is light), never under water.
pub(crate) fn small_ride(seconds: f32, strength: f32, seed: f32) -> (f32, Quat) {
    let ride = super::boat::ride(seconds * 1.3 + seed, strength, false);
    (
        ride.heave * 1.4,
        Quat::from_rotation_z(ride.roll * 2.2) * Quat::from_rotation_x(ride.pitch * 1.6),
    )
}

/// Rock the harbour's boats afloat on the view clock (it runs while the
/// game is paused; Reduce motion stops it with the wind's clock).
pub(crate) fn bob(
    wind: Option<Res<super::ambient::Wind>>,
    mut boats: Query<(&Afloat, &mut Transform)>,
) {
    let Some(wind) = wind else { return };
    for (boat, mut transform) in &mut boats {
        let (heave, rock) = small_ride(wind.time, wind.strength, boat.seed);
        let next = Transform {
            translation: boat.base.translation + Vec3::Y * heave,
            rotation: boat.base.rotation * rock,
            scale: boat.base.scale,
        };
        if *transform != next {
            *transform = next;
        }
    }
}

/// Where the mooring posts stand along a pier `length` metres long, in
/// its own frame (x across, z out from the foot): one every three metres,
/// alternating sides, and a pair at the head.
pub(crate) fn pier_posts(length: f32) -> Vec<(f32, f32)> {
    let side = JETTY_WIDTH_M * 0.5 + 0.15;
    let mut posts: Vec<(f32, f32)> = (1..)
        .map(|i| i as f32 * 3.0)
        .take_while(|z| *z < length - 1.5)
        .enumerate()
        .map(|(i, z)| (if i % 2 == 0 { side } else { -side }, z))
        .collect();
    posts.push((side, length - 0.2));
    posts.push((-side, length - 0.2));
    posts
}

/// The rowing boats' scale: 3.6 m long.
const ROWING_BOAT_SCALE: f32 = 1.3;

/// The ship kit's narrow pier section: its deck's height over its base, its
/// width across and its length along, model units.
const DOCK_HEIGHT_UNITS: f32 = 1.312;
const DOCK_WIDTH_UNITS: f32 = 1.9;
const DOCK_LENGTH_UNITS: f32 = 2.512;

/// The pier sections along a jetty `length` metres long: each one's middle
/// (metres from the foot) and its stretch along, so they meet end to end
/// from the foot to the head.
pub(crate) fn dock_sections(length: f32) -> Vec<(f32, f32)> {
    let count = (length / DOCK_LENGTH_UNITS).round().max(1.0) as usize;
    let each = length / count as f32;
    (0..count)
        .map(|i| ((i as f32 + 0.5) * each, each / DOCK_LENGTH_UNITS))
        .collect()
}

/// The harbour's entities and the berth they were laid out for.
#[derive(Resource, Default)]
pub(crate) struct Harbour {
    pub(crate) root: Option<Entity>,
    /// The goods stacked while the boat is in; shown and hidden by
    /// [`super::boat::sail`].
    pub(crate) cargo: Option<Entity>,
    built_for: Option<(Vec2, Vec2)>,
    pub(crate) landing: Option<Landing>,
}

/// The ground's height under a world point: 0 on the water.
fn land_height(world: &World, at: Vec3) -> f32 {
    world
        .get_resource::<TerrainState>()
        .and_then(|terrain| terrain.cell(at.x.floor() as i32, at.z.floor() as i32))
        .filter(|cell| cell.surface != TerrainSurface::Water)
        .map_or(0.0, |cell| cell.height_mm.max(0) as f32 / 1000.0)
}

/// Lay the harbour out for the boat's berth, once per berth.
pub(crate) fn build(world: &mut World) {
    let Some(berth) = world.resource::<super::boat::BoatScene>().berth() else {
        return;
    };
    if world.resource::<Harbour>().built_for == Some(berth) {
        return;
    }
    if !world.contains_resource::<Assets<Mesh>>()
        || !world.contains_resource::<Assets<StandardMaterial>>()
        || !world.contains_resource::<AssetServer>()
    {
        return;
    }
    let Some(terrain) = world.get_resource::<TerrainState>() else {
        return;
    };
    let ground = |x: i32, z: i32| {
        terrain
            .cell(x, z)
            .map(|cell| (cell.surface, cell.height_mm))
    };
    let Some(landing) = landing(berth.0, berth.1, |x, z| {
        ground(x, z).is_some_and(|(surface, _)| surface != TerrainSurface::Water)
    }) else {
        world.resource_mut::<Harbour>().built_for = Some(berth);
        return;
    };
    let shore_y = ground(landing.shore.0, landing.shore.1)
        .map_or(0.0, |(_, height)| height.max(0) as f32 / 1000.0);
    let deck = (shore_y + 0.12).max(DECK_LEAST_Y);
    let water = -0.11;
    let (old_root, old_cargo) = {
        let harbour = world.resource::<Harbour>();
        (harbour.root, harbour.cargo)
    };
    for entity in [old_root, old_cargo].into_iter().flatten() {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
    let along = landing.along();
    let turn = Quat::from_rotation_y(along.x.atan2(along.y));
    let at = Vec3::new(landing.foot.x, 0.0, landing.foot.y);
    // A trading post at the waterfront is the quay itself.
    let quay = island_content::village::view::boat(world)
        .dock
        .is_none_or(|dock| {
            landing
                .foot
                .distance(Vec2::new(dock.x as f32 / 1000.0, dock.z as f32 / 1000.0))
                > POST_IS_QUAY_M
        });
    let (planks, frame) = timbers(landing.length(), deck, water, quay);
    let (plank_mesh, frame_mesh) = {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        (
            meshes.add(super::boxes_mesh(&planks)),
            meshes.add(super::boxes_mesh(&frame)),
        )
    };
    let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::default());
    let (deck_wood, dark_wood, post_wood) = {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut matte = |r: u8, g: u8, b: u8| {
            materials.add(StandardMaterial {
                base_color: Color::srgb_u8(r, g, b),
                perceptual_roughness: 0.95,
                ..default()
            })
        };
        // The deck's planks in the ship's brown family, a little lighter
        // than its oak hull; darker timber below.
        (matte(104, 76, 56), matte(96, 62, 44), matte(72, 50, 38))
    };
    let (crate_model, barrel, sack) = {
        let server = world.resource::<AssetServer>();
        let scene = |path: &'static str| -> Handle<WorldAsset> {
            server.load(GltfAssetLabel::Scene(0).from_asset(path))
        };
        (
            scene("hex/crate_A_big.gltf"),
            scene("hex/barrel.gltf"),
            scene("hex/sack.gltf"),
        )
    };
    let glass = world
        .get_resource::<crate::island_atmosphere::night::LanternGlass>()
        .map(|glass| glass.0.clone());
    // Fogged as one piece of dressing: the jetty and the quay together.
    let reach = landing.length().max(QUAY_SIZE_M.y) + QUAY_SIZE_M.x;
    let middle = at + (turn * Vec3::Z) * (landing.length() - QUAY_SIZE_M.y) * 0.5;
    let root = world
        .spawn((
            Name::new("harbour jetty"),
            Transform::from_translation(at).with_rotation(turn),
            Visibility::default(),
            FogArea::around(middle, Vec3::new(reach * 0.5, 1.0, reach * 0.5)),
            FogDim::default(),
        ))
        .id();
    // The jetty: the ship kit's pier sections on piles, end to end from
    // the foot to the head (stretched along to fit exactly), their wood
    // weathered like the ship's. The code-built planks stand in without
    // the kit (headless).
    let dock = world.get_resource::<AssetServer>().map(|server| {
        server.load(
            GltfAssetLabel::Scene(0).from_asset("living/ships/structure-platform-dock-small.glb"),
        )
    });
    match dock {
        Some(dock) => {
            for (centre, stretch) in dock_sections(landing.length()) {
                world
                    .spawn((
                        Name::new("jetty section"),
                        ChildOf(root),
                        WorldAssetRoot(dock.clone()),
                        Transform::from_xyz(0.0, deck - DOCK_HEIGHT_UNITS, centre)
                            .with_scale(Vec3::new(JETTY_WIDTH_M / DOCK_WIDTH_UNITS, 1.0, stretch)),
                    ))
                    .observe(super::tint::on_wood_ready);
            }
            // The kit's section is a frame whose own boards lie low, under
            // the sea (seen only through the shallows at the foot, as a
            // blurred streak): a walkway of planks on top of it, its width
            // the jetty's, just above the frame's rails.
            world.spawn((
                Name::new("jetty deck"),
                ChildOf(root),
                Mesh3d(plank_mesh),
                MeshMaterial3d(deck_wood),
                Transform::from_xyz(0.0, DECK_ON_FRAME_M, 0.0),
            ));
        }
        None => {
            world.spawn((
                ChildOf(root),
                Mesh3d(plank_mesh),
                MeshMaterial3d(deck_wood),
                Transform::default(),
            ));
            world.spawn((
                ChildOf(root),
                Mesh3d(frame_mesh),
                MeshMaterial3d(post_wood.clone()),
                Transform::default(),
            ));
        }
    }
    // The quay's own stores: crates, barrels and a sack against its back.
    let back = -QUAY_SIZE_M.y + 0.55;
    let stores = if quay {
        vec![
            (&crate_model, -1.5, back, 3.2, 0.1),
            (&crate_model, -1.45, back + 1.25, 3.2, -0.25),
            (&barrel, 1.6, back, 3.2, 0.0),
            (&barrel, 0.85, back - 0.05, 3.2, 0.7),
            (&sack, 1.7, back + 0.95, 7.0, 0.4),
        ]
    } else {
        Vec::new()
    };
    for (model, x, z, scale, yaw) in stores {
        world.spawn((
            ChildOf(root),
            WorldAssetRoot(model.clone()),
            Transform::from_xyz(x, deck, z)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::splat(scale)),
        ));
    }
    // A lantern on a post at the jetty's foot.
    let post = Vec3::new(JETTY_WIDTH_M * 0.5 + 0.35, deck, -0.3);
    world.spawn((
        ChildOf(root),
        Mesh3d(cube.clone()),
        MeshMaterial3d(dark_wood.clone()),
        Transform::from_translation(post + Vec3::Y * 1.0).with_scale(Vec3::new(0.1, 2.0, 0.1)),
    ));
    world.spawn((
        ChildOf(root),
        Mesh3d(cube.clone()),
        MeshMaterial3d(dark_wood.clone()),
        Transform::from_translation(post + Vec3::new(-0.2, 1.95, 0.0))
            .with_scale(Vec3::new(0.45, 0.06, 0.06)),
    ));
    if let Some(glass) = glass {
        world.spawn((
            ChildOf(root),
            Mesh3d(cube.clone()),
            MeshMaterial3d(glass),
            Transform::from_translation(post + Vec3::new(-0.38, 1.72, 0.0))
                .with_scale(Vec3::new(0.2, 0.28, 0.2)),
        ));
        world.spawn((
            ChildOf(root),
            crate::island_atmosphere::night::Lantern,
            PointLight {
                color: Color::srgb(1.0, 0.69, 0.31),
                intensity: 0.0,
                range: 10.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(post + Vec3::new(-0.38, 1.6, 0.0)),
            Visibility::Hidden,
        ));
    }
    // Harbour dressing from the ship kits: mooring posts at the head, two
    // rowing boats pulled up on the strand either side of the foot, a
    // bucket of fish and an anchor by the foot, and a flag on a pole.
    let ground_y = |world: &World, local: Vec3| land_height(world, at + turn * local);
    let dressing = {
        let server = world.resource::<AssetServer>();
        let scene = |path: &'static str| -> Handle<WorldAsset> {
            server.load(GltfAssetLabel::Scene(0).from_asset(path))
        };
        let length = landing.length();
        let pole = scene("living/ships/Environment_Dock_Pole.gltf");
        // Mooring posts along the pier, every three metres either side,
        // and a pair at its head where the ship ties up.
        let mut pieces: Vec<_> = pier_posts(length)
            .into_iter()
            .map(|(x, z)| (pole.clone(), Vec3::new(x, deck, z), 1.0, 0.0))
            .collect();
        pieces.extend([(
            scene("living/ships/Prop_Bucket_Fishes.gltf"),
            Vec3::new(-0.55, deck, 0.6),
            0.6,
            0.4,
        )]);
        // One rowing boat pulled right up on the strand, on dry land; one
        // afloat, tied beside the jetty. The kit's boats are about 2.8
        // units long along their Z: at this scale 3.6 m.
        let up = (1..12)
            .map(|step| Vec3::new(-6.0, 0.0, -0.5 * step as f32))
            .find(|local| land_height(world, at + turn * *local) > 0.0)
            .unwrap_or(Vec3::new(4.6, 0.0, -3.0));
        pieces.push((
            scene("living/ships/boat-row-small.glb"),
            Vec3::new(up.x, ground_y(world, up) + 0.02, up.z),
            ROWING_BOAT_SCALE,
            0.35,
        ));
        // The kit's boat is long along its Z, its oars out along X.
        pieces.push((
            scene("living/ships/boat-row-large.glb"),
            Vec3::new(
                -(JETTY_WIDTH_M * 0.5 + 2.7),
                -0.32,
                (length * 0.45).max(3.0),
            ),
            ROWING_BOAT_SCALE,
            0.0,
        ));
        let anchor = Vec3::new(2.2, 0.0, -1.6);
        pieces.push((
            scene("living/ships/Prop_Anchor.gltf"),
            Vec3::new(anchor.x, ground_y(world, anchor), anchor.z),
            1.5,
            0.9,
        ));
        pieces
    };
    for (model, local, scale, yaw) in dressing {
        let placed = Transform::from_translation(local)
            .with_rotation(Quat::from_rotation_y(yaw))
            .with_scale(Vec3::splat(scale));
        let mut piece = world.spawn((ChildOf(root), WorldAssetRoot(model), placed));
        piece.observe(super::tint::on_wood_ready);
        // The rowing boat afloat (the only piece below the water's level)
        // rides the swell on its painter.
        if local.y < water {
            piece.insert(Afloat {
                base: placed,
                seed: 1.7,
            });
        }
    }
    // On land at the pier's foot, on the side away from the beached boat.
    let flag_at = Vec3::new(JETTY_WIDTH_M * 0.5 + 0.9, 0.0, -1.2);
    let flag = world
        .resource::<AssetServer>()
        .load(GltfAssetLabel::Scene(0).from_asset("living/props/flag_blue.gltf"));
    world.spawn((
        Name::new("harbour flag"),
        ChildOf(root),
        WorldAssetRoot(flag.clone()),
        super::ambient::WindFlag { seed: 0.3 },
        Transform::from_translation(Vec3::new(flag_at.x, ground_y(world, flag_at), flag_at.z))
            .with_scale(Vec3::splat(8.0)),
    ));
    // The trading post flies one too, beside its entrance along the shore.
    if let Some(dock) = island_content::village::view::boat(world).dock {
        let entrance = Vec3::new(dock.x as f32 / 1000.0, 0.0, dock.z as f32 / 1000.0);
        let beside = entrance + turn * Vec3::new(2.4, 0.0, 0.0);
        let mut local = turn.inverse() * (beside - at);
        // Never on the harbour's own flag: a pole's width apart at least.
        if Vec2::new(local.x - flag_at.x, local.z - flag_at.z).length() < 2.5 {
            local.x = flag_at.x + 2.5;
            local.z = flag_at.z;
        }
        let beside = at + turn * local;
        world.spawn((
            Name::new("trading post flag"),
            ChildOf(root),
            WorldAssetRoot(flag),
            super::ambient::WindFlag { seed: 1.1 },
            Transform::from_translation(Vec3::new(local.x, land_height(world, beside), local.z))
                .with_scale(Vec3::splat(8.0)),
        ));
    }
    // Unloaded goods: on the quay by the jetty's foot, hidden until the
    // boat is in. Not fogged as dressing: they come and go with the boat.
    let cargo = world
        .spawn((
            Name::new("harbour cargo"),
            Transform::from_translation(at).with_rotation(turn),
            Visibility::Hidden,
        ))
        .id();
    let unloaded: &[(&Handle<WorldAsset>, f32, f32, f32, f32)] = if quay {
        &[
            (&crate_model, -0.65, -0.8, 3.0, 0.3),
            (&crate_model, 0.55, -0.95, 3.0, -0.2),
            (&crate_model, -0.1, -1.9, 3.0, 0.05),
            (&sack, 1.1, -1.8, 6.5, 1.1),
            (&barrel, -1.2, -1.9, 3.0, 0.0),
            (&crate_model, -0.05, -0.85, 2.6, 0.6),
        ]
    } else {
        &[]
    };
    for &(model, x, z, scale, yaw) in unloaded {
        // The last crate stands on top, so the stack reads from above.
        let lift = if z == -0.85 && scale < 2.9 { 0.62 } else { 0.0 };
        world.spawn((
            ChildOf(cargo),
            WorldAssetRoot(model.clone()),
            Transform::from_xyz(x, deck + lift, z)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::splat(scale)),
        ));
    }
    info!(
        "harbour jetty: foot {:.1}, {:.1}; head {:.1}, {:.1}; {:.1} m long; deck at {:.2} m",
        landing.foot.x,
        landing.foot.y,
        landing.head.x,
        landing.head.y,
        landing.length(),
        deck
    );
    let mut harbour = world.resource_mut::<Harbour>();
    harbour.root = Some(root);
    harbour.cargo = Some(cargo);
    harbour.built_for = Some(berth);
    harbour.landing = Some(landing);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boat_floats_at_the_simulations_berth_and_the_jetty_reaches_it_from_dry_land() {
        let mut app = island_content::new_sim_app(7, "living_world");
        npc_framework::prelude::run_ticks(app.world_mut(), 3);
        let world = app.world();
        let boat = island_content::village::view::boat(world);
        let berth = boat.berth.expect("the living world has a berth");
        let terrain = world.resource::<TerrainState>();
        let water = |x: i32, z: i32| {
            terrain
                .cell(x, z)
                .is_none_or(|cell| cell.surface == TerrainSurface::Water)
        };
        use super::super::boat::{HULL, hull_floats, mooring, seaward};
        let (at, out) = mooring(berth, boat.dock, water);
        // On the simulation's berth, or straight out from it the least way
        // that floats the whole hull clear of the beach and the land.
        let (sim_at, sim_out) = seaward(berth, boat.dock, &water);
        assert_eq!(
            (sim_at.x, sim_at.y),
            (berth.x as f32 / 1000.0, berth.z as f32 / 1000.0),
        );
        assert_eq!(out, sim_out);
        let moved = at - sim_at;
        assert!(
            moved.length() < 34.0 && moved.perp_dot(out).abs() < 1e-3,
            "{moved}"
        );
        assert!(
            hull_floats(at, out, HULL, &water),
            "the hull floats clear at {at}"
        );
        // Never on the trading post's ground.
        let dock = boat.dock.expect("a post");
        let post = Vec2::new(dock.x as f32 / 1000.0, dock.z as f32 / 1000.0);
        assert!(at.distance(post) > HULL.length * 0.5, "{at} {post}");
        // Moored on water, the sea beyond it.
        assert!(water(berth.x.div_euclid(1000), berth.z.div_euclid(1000)));
        let ahead = at + out * 3.0;
        assert!(
            water(ahead.x.floor() as i32, ahead.y.floor() as i32),
            "{out}"
        );
        // The jetty: its foot on a dry shore cell, its head beside the hull.
        let landing = landing(at, out, |x, z| !water(x, z)).expect("a shore");
        assert!(!water(landing.shore.0, landing.shore.1));
        assert!((landing.head.distance(at) - HULL_OFFSET_M).abs() < 1e-4);
        // A real pier: at least ten metres from the strand to its head.
        assert!(
            (9.5..30.0).contains(&landing.length()),
            "{}",
            landing.length()
        );
        // Near the trading post, which stands at the waterfront.
        assert!(
            landing.foot.distance(post) < 8.0,
            "{:?} {post}",
            landing.foot
        );
    }

    #[test]
    fn a_small_boat_rides_the_swell_and_stays_afloat() {
        let rides: Vec<_> = (0..120)
            .map(|i| small_ride(i as f32 * 0.1, 1.0, 1.7))
            .collect();
        assert!(rides.windows(2).any(|w| w[0].0 != w[1].0), "it heaves");
        assert!(
            rides.iter().all(|(heave, _)| heave.abs() < 0.12),
            "a gentle swell"
        );
        assert!(
            rides
                .iter()
                .any(|(_, rock)| rock.angle_between(Quat::IDENTITY) > 0.01)
        );
        assert!(
            rides
                .iter()
                .all(|(_, rock)| rock.angle_between(Quat::IDENTITY) < 0.2)
        );
        let calm = small_ride(3.0, 0.0, 1.7).0.abs();
        let storm = (0..120)
            .map(|i| small_ride(i as f32 * 0.1, 3.0, 1.7).0.abs())
            .fold(0.0, f32::max);
        assert!(storm > calm);
    }

    #[test]
    fn mooring_posts_line_the_pier_and_pair_at_its_head() {
        let posts = pier_posts(12.0);
        assert!(posts.len() >= 5, "{posts:?}");
        let head: Vec<_> = posts.iter().filter(|(_, z)| *z > 11.0).collect();
        assert_eq!(head.len(), 2);
        assert!(
            posts
                .iter()
                .all(|(x, z)| x.abs() > JETTY_WIDTH_M * 0.5 && *z <= 12.0)
        );
    }

    #[test]
    fn the_pier_sections_meet_end_to_end_from_the_foot_to_the_head() {
        for length in [2.0_f32, 6.3, 11.0] {
            let sections = dock_sections(length);
            let mut reach = 0.0;
            for (centre, stretch) in &sections {
                let half = stretch * DOCK_LENGTH_UNITS * 0.5;
                assert!((centre - half - reach).abs() < 1e-4, "{length}");
                reach = centre + half;
                assert!((0.6..1.5).contains(stretch), "{stretch}");
            }
            assert!((reach - length).abs() < 1e-4);
        }
    }

    #[test]
    fn the_jetty_runs_from_the_nearest_shore_to_beside_the_hull() {
        // Land for z >= 0, the sea beyond; the boat lies off the shore.
        let land = |_x: i32, z: i32| z >= 0;
        let (berth, out) = (Vec2::new(5.5, -2.9 - HULL_OFFSET_M), Vec2::NEG_Y);
        let landing = landing(berth, out, land).expect("a shore");
        // Its head stands off the hull's landward side.
        assert!((landing.head - Vec2::new(5.5, -2.9)).length() < 1e-4);
        // Its foot is on the shore's edge, straight in from the head.
        assert_eq!(landing.shore, (5, 0));
        assert!((landing.foot - Vec2::new(5.5, 0.0)).length() < 1e-4);
        assert!((landing.along() - Vec2::NEG_Y).length() < 1e-4);
        assert!((landing.length() - 2.9).abs() < 1e-4);
        // No land anywhere near: no jetty.
        assert_eq!(super::landing(berth, out, |_, _| false), None);
    }

    #[test]
    fn the_deck_covers_the_jetty_and_the_posts_reach_into_the_water() {
        let (length, deck, water) = (6.0, 0.4, -0.11);
        let (planks, frame) = timbers(length, deck, water, true);
        // Planks from the foot to the head, nothing past it.
        let jetty: Vec<_> = planks.iter().filter(|(at, _)| at.z > 0.0).collect();
        assert!(jetty.len() >= (length / PLANK_PITCH_M) as usize);
        let far = jetty
            .iter()
            .map(|(at, size)| at.z + size.z * 0.5)
            .fold(0.0, f32::max);
        assert!((length - 0.2..=length + 0.2).contains(&far), "{far}");
        for (at, size) in &planks {
            assert!((at.y + size.y * 0.5 - deck).abs() < 1e-4);
        }
        // The quay lies behind the foot.
        assert!(planks.iter().any(|(at, size)| at.z < -2.0 && size.x > 4.0));
        // Posts stand from below the water to the deck, at both ends.
        let posts: Vec<_> = frame.iter().filter(|(_, size)| size.y > 1.0).collect();
        assert!(posts.len() >= 4);
        for (at, size) in &posts {
            assert!(at.y - size.y * 0.5 < water - 0.5);
            assert!(at.y + size.y * 0.5 >= deck - 1e-4);
        }
        assert!(posts.iter().any(|(at, _)| at.z.abs() < 1e-4));
        assert!(posts.iter().any(|(at, _)| (at.z - length).abs() < 1e-4));
        // A trading post at the waterfront is the quay: nothing behind the
        // foot.
        let (planks, frame) = timbers(length, deck, water, false);
        assert!(planks.iter().chain(&frame).all(|(at, _)| at.z >= -1e-4));
    }
}
