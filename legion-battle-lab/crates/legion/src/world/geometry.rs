//! Small procedural mesh builder. Geometry is baked into a few shared assets.

use battle_core::{UnitCategory, UnitKind};
use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};

#[derive(Default)]
pub(super) struct Geometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
}

impl Geometry {
    pub fn triangle(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4], uv: [f32; 2]) {
        let normal = (b - a).cross(c - a).normalize_or_zero().to_array();
        for point in [a, b, c] {
            self.positions.push(point.to_array());
            self.normals.push(normal);
            self.uvs.push(uv);
            self.colors.push(color);
        }
    }

    pub fn quad(&mut self, corners: [Vec3; 4], color: [f32; 4], uv: [f32; 2]) {
        self.triangle(corners[0], corners[1], corners[2], color, uv);
        self.triangle(corners[0], corners[2], corners[3], color, uv);
    }

    pub fn cuboid(
        &mut self,
        center: Vec3,
        size: Vec3,
        rotation: Quat,
        color: [f32; 4],
        uv: [f32; 2],
    ) {
        let half = size * 0.5;
        let point = |x: f32, y: f32, z: f32| center + rotation * (Vec3::new(x, y, z) * half);
        let faces = [
            [
                point(1., -1., -1.),
                point(1., 1., -1.),
                point(1., 1., 1.),
                point(1., -1., 1.),
            ],
            [
                point(-1., -1., 1.),
                point(-1., 1., 1.),
                point(-1., 1., -1.),
                point(-1., -1., -1.),
            ],
            [
                point(-1., 1., -1.),
                point(-1., 1., 1.),
                point(1., 1., 1.),
                point(1., 1., -1.),
            ],
            [
                point(-1., -1., 1.),
                point(-1., -1., -1.),
                point(1., -1., -1.),
                point(1., -1., 1.),
            ],
            [
                point(-1., -1., 1.),
                point(1., -1., 1.),
                point(1., 1., 1.),
                point(-1., 1., 1.),
            ],
            [
                point(1., -1., -1.),
                point(-1., -1., -1.),
                point(-1., 1., -1.),
                point(1., 1., -1.),
            ],
        ];
        for face in faces {
            self.quad(face, color, uv);
        }
    }

    pub fn cone(&mut self, base: Vec3, radius: f32, height: f32, color: [f32; 4], rotation: f32) {
        let tip = base + Vec3::Y * height;
        for side in 0..7 {
            let a = rotation + side as f32 * std::f32::consts::TAU / 7.0;
            let b = rotation + (side + 1) as f32 * std::f32::consts::TAU / 7.0;
            let first = base + Vec3::new(a.cos() * radius, 0.0, a.sin() * radius);
            let second = base + Vec3::new(b.cos() * radius, 0.0, b.sin() * radius);
            self.triangle(second, first, tip, color, [0.0; 2]);
        }
    }

    pub fn rock(&mut self, center: Vec3, size: Vec3, angle: f32, color: [f32; 4]) {
        let rotate = Quat::from_rotation_y(angle);
        let points =
            [Vec3::X, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Z].map(|p| center + rotate * (p * size));
        let top = center + Vec3::Y * size.y;
        let bottom = center - Vec3::Y * size.y * 0.45;
        for index in 0..4 {
            let next = (index + 1) % 4;
            self.triangle(points[next], points[index], top, color, [0.0; 2]);
            self.triangle(points[index], points[next], bottom, color, [0.0; 2]);
        }
    }

    pub fn mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
    }
}

pub(super) fn linear(r: f32, g: f32, b: f32) -> [f32; 4] {
    Color::srgb(r, g, b).to_linear().to_f32_array()
}

/// +X faces forward, +Y points up. UV.x encodes shared material regions:
/// 0 team cloth, 1 skin, 2 steel, 3 leather. UV.y is a limb swing sign.
pub(super) fn soldier_mesh(kind: UnitKind) -> Mesh {
    let mut mesh = Geometry::default();
    let white = [1.0; 4];
    let heavy = kind == UnitKind::Heavy;
    let width = if heavy { 0.78 } else { 0.58 };
    let mut part = |center: [f32; 3], size: [f32; 3], material: f32, swing: f32, rotation: Quat| {
        mesh.cuboid(
            Vec3::from_array(center),
            Vec3::from_array(size),
            rotation,
            white,
            [material, swing],
        );
    };
    part(
        [0.0, 1.12, 0.0],
        [if heavy { 0.48 } else { 0.36 }, 0.66, width],
        0.0,
        0.0,
        Quat::IDENTITY,
    );
    part(
        [0.02, 1.67, 0.0],
        [0.38, 0.39, 0.38],
        1.0,
        0.0,
        Quat::IDENTITY,
    );
    part(
        [0.0, 1.85, 0.0],
        [0.43, 0.16, 0.44],
        if kind == UnitKind::Ranger { 0.0 } else { 2.0 },
        0.0,
        Quat::IDENTITY,
    );
    let leg_gap = width * 0.29;
    part(
        [0.0, 0.43, -leg_gap],
        [0.24, 0.80, 0.23],
        3.0,
        -1.0,
        Quat::IDENTITY,
    );
    part(
        [0.0, 0.43, leg_gap],
        [0.24, 0.80, 0.23],
        3.0,
        1.0,
        Quat::IDENTITY,
    );
    part(
        [0.02, 1.12, -width * 0.7],
        [0.20, 0.58, 0.20],
        0.0,
        0.5,
        Quat::from_rotation_z(-0.12),
    );
    part(
        [0.13, 1.11, width * 0.7],
        [0.25, 0.55, 0.20],
        0.0,
        -0.5,
        Quat::from_rotation_z(-0.32),
    );
    match kind {
        UnitKind::Infantry => {
            part(
                [0.21, 1.10, -0.53],
                [0.15, 0.71, 0.44],
                2.0,
                0.0,
                Quat::IDENTITY,
            );
            part(
                [0.63, 1.16, 0.48],
                [1.04, 0.07, 0.09],
                2.0,
                0.0,
                Quat::IDENTITY,
            );
        }
        UnitKind::Heavy => {
            part(
                [0.23, 1.06, -0.68],
                [0.20, 0.94, 0.65],
                2.0,
                0.0,
                Quat::IDENTITY,
            );
            part(
                [0.49, 1.22, 0.63],
                [0.55, 0.31, 0.23],
                2.0,
                0.0,
                Quat::from_rotation_z(0.3),
            );
        }
        UnitKind::Ranger => {
            part(
                [0.49, 1.32, 0.51],
                [0.09, 0.73, 0.10],
                3.0,
                0.0,
                Quat::from_rotation_z(-0.27),
            );
            part(
                [-0.27, 1.18, -0.1],
                [0.22, 0.72, 0.29],
                3.0,
                0.0,
                Quat::from_rotation_z(-0.24),
            );
        }
    }
    let mut mesh = mesh.mesh();
    // CrowdPipeline reserves shader locations 3..5 for per-instance data.
    // Unlike terrain, soldiers derive color from their instance and UV mask.
    mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    mesh
}

/// Shared, intentionally simple silhouettes for the seven sandbox categories.
/// Air and space elevation is baked into the mesh; underground units use a
/// surface marker rather than disappearing beneath opaque terrain.
pub(super) fn category_mesh(category: UnitCategory, kind: UnitKind) -> Mesh {
    if category == UnitCategory::Creature {
        return soldier_mesh(kind);
    }
    let mut geometry = Geometry::default();
    let white = [1.0; 4];
    let elevation = category.elevation().max(0.0);
    let heavy = kind == UnitKind::Heavy;
    let mut part = |center: [f32; 3], size: [f32; 3], material: f32, rotation: Quat| {
        geometry.cuboid(
            Vec3::from_array(center) + Vec3::Y * elevation,
            Vec3::from_array(size),
            rotation,
            white,
            [material, 2.0], // Rigid body: no humanoid gait.
        );
    };
    match category {
        UnitCategory::Creature => unreachable!(),
        UnitCategory::Vehicle => {
            part([0.0, 0.72, 0.0], [2.6, 0.65, 1.5], 0.0, Quat::IDENTITY);
            for z in [-0.88, 0.88] {
                part([0.0, 0.36, z], [2.85, 0.58, 0.38], 2.0, Quat::IDENTITY);
            }
            part([-0.1, 1.22, 0.0], [1.2, 0.55, 1.05], 0.0, Quat::IDENTITY);
            let length = if kind == UnitKind::Ranger { 1.85 } else { 1.2 };
            part([0.85, 1.30, 0.0], [length, 0.16, 0.18], 2.0, Quat::IDENTITY);
            if heavy {
                part([-0.3, 1.61, 0.0], [0.5, 0.24, 0.55], 2.0, Quat::IDENTITY);
            }
        }
        UnitCategory::Building => {
            part([0.0, 0.22, 0.0], [2.65, 0.44, 2.65], 2.0, Quat::IDENTITY);
            part([0.0, 1.55, 0.0], [2.15, 2.5, 2.15], 0.0, Quat::IDENTITY);
            part([0.0, 2.9, 0.0], [2.65, 0.35, 2.65], 2.0, Quat::IDENTITY);
            for x in [-0.95, 0.95] {
                for z in [-0.95, 0.95] {
                    part([x, 3.35, z], [0.65, 0.65, 0.65], 0.0, Quat::IDENTITY);
                }
            }
            part([1.09, 0.95, 0.0], [0.06, 1.5, 0.65], 3.0, Quat::IDENTITY);
            part([1.1, 2.12, 0.0], [0.07, 0.28, 0.3], 3.0, Quat::IDENTITY);
        }
        UnitCategory::AirUnit => {
            part([0.0, 0.4, 0.0], [2.6, 0.55, 0.55], 0.0, Quat::IDENTITY);
            part(
                [0.12, 0.4, 0.0],
                [0.95, 0.13, 3.6],
                0.0,
                Quat::from_rotation_y(0.1),
            );
            part([-1.02, 0.48, 0.0], [0.65, 0.12, 1.75], 2.0, Quat::IDENTITY);
            part([-1.0, 0.8, 0.0], [0.7, 0.7, 0.12], 0.0, Quat::IDENTITY);
            part([0.4, 0.73, 0.0], [0.7, 0.25, 0.45], 2.0, Quat::IDENTITY);
            for z in [-0.9, 0.9] {
                part([0.3, 0.27, z], [0.75, 0.25, 0.3], 2.0, Quat::IDENTITY);
            }
        }
        UnitCategory::SeaUnit => {
            part([0.0, 0.40, 0.0], [2.7, 0.6, 1.15], 0.0, Quat::IDENTITY);
            part([0.1, 0.75, 0.0], [2.5, 0.18, 1.04], 3.0, Quat::IDENTITY);
            part([-0.45, 1.09, 0.0], [0.9, 0.62, 0.8], 0.0, Quat::IDENTITY);
            part([0.1, 1.72, 0.0], [0.1, 2.05, 0.1], 3.0, Quat::IDENTITY);
            part(
                [0.1, 2.0, 0.0],
                [0.06, 1.15, 1.18],
                0.0,
                Quat::from_rotation_y(-0.3),
            );
            part(
                [1.30, 0.48, 0.0],
                [0.6, 0.48, 0.78],
                0.0,
                Quat::from_rotation_y(0.3),
            );
        }
        UnitCategory::UndergroundUnit => {
            part(
                [0.0, 0.12, 0.0],
                [1.65, 0.2, 1.65],
                2.0,
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            );
            part([0.0, 0.25, 0.0], [1.35, 0.12, 0.25], 0.0, Quat::IDENTITY);
            part([0.0, 0.25, 0.0], [0.25, 0.12, 1.35], 0.0, Quat::IDENTITY);
            part(
                [0.0, 0.58, 0.0],
                [0.5, 0.6, 0.5],
                0.0,
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            );
            part([0.4, 0.59, 0.0], [0.5, 0.18, 0.18], 2.0, Quat::IDENTITY);
        }
        UnitCategory::SpaceUnit => {
            part([0.0, 0.5, 0.0], [3.15, 0.52, 0.62], 0.0, Quat::IDENTITY);
            for z in [-0.95_f32, 0.95_f32] {
                part(
                    [-0.32, 0.42, z],
                    [1.85, 0.18, 1.30],
                    0.0,
                    Quat::from_rotation_y(z.signum() * 0.32),
                );
                part(
                    [-1.0, 0.48, z * 1.45],
                    [1.2, 0.42, 0.35],
                    2.0,
                    Quat::IDENTITY,
                );
                part(
                    [-1.64, 0.48, z * 1.45],
                    [0.12, 0.29, 0.25],
                    0.0,
                    Quat::IDENTITY,
                );
            }
            part([0.45, 0.91, 0.0], [0.84, 0.3, 0.42], 2.0, Quat::IDENTITY);
            part([-1.18, 0.86, 0.0], [0.5, 0.65, 0.12], 0.0, Quat::IDENTITY);
        }
    }
    // Authored silhouettes must fit the body's gameplay footprint in every
    // heading. Shrink X/Z only: category elevation and model height stay intact.
    let radius = category.definition(kind).radius;
    let authored_radius = geometry
        .positions
        .iter()
        .map(|position| position[0].hypot(position[2]))
        .fold(0.0_f32, f32::max);
    let scale = radius / authored_radius.max(radius);
    if scale < 1.0 {
        for position in &mut geometry.positions {
            position[0] *= scale;
            position[2] *= scale;
        }
        // Normals use the inverse transpose of the nonuniform X/Z transform.
        for normal in &mut geometry.normals {
            *normal = Vec3::new(normal[0] / scale, normal[1], normal[2] / scale)
                .normalize_or_zero()
                .to_array();
        }
    }
    let mut mesh = geometry.mesh();
    mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    mesh
}

/// Exactly one GPU point per instance. UV.y selects the cheap shader branch.
pub(super) fn point_mesh(category: UnitCategory) -> Mesh {
    let center = match category {
        UnitCategory::Building => 1.8,
        UnitCategory::UndergroundUnit => 0.6,
        _ => 1.0,
    };
    Mesh::new(PrimitiveTopology::PointList, RenderAssetUsages::default())
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[0.0, category.elevation().max(0.0) + center, 0.0]],
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 99.0]])
}
