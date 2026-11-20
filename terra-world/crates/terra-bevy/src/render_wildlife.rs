//! Small, shared species meshes with a bounded baked pose palette.
//! The authoritative animal body and behavior choose a pose; rendering never
//! spawns wildlife or advances its ecology. Assets are reusable by other games.
use crate::geometry::MeshBuilder;
use bevy::prelude::*;
use terra_sandbox::AnimalSpecies;

/// Eight gait samples plus standing, grazing, resting and drinking poses.
pub const WILDLIFE_POSE_COUNT: u8 = 12;
pub const WILDLIFE_STANDING: u8 = 8;
pub const WILDLIFE_GRAZING: u8 = 9;
pub const WILDLIFE_RESTING: u8 = 10;
pub const WILDLIFE_DRINKING: u8 = 11;

/// Meshes face local -Z and put the lowest foot at y=0. Pose palettes avoid
/// rebuilding geometry every frame or attaching a skeleton to each distant body.
pub fn wildlife_mesh(species: AnimalSpecies, pose: u8) -> Mesh {
    let pose = pose.min(WILDLIFE_POSE_COUNT - 1);
    let (body, radius, leg, fur, muzzle) = match species {
        AnimalSpecies::Deer => (
            0.96,
            Vec3::new(0.27, 0.32, 0.62),
            0.70,
            [0.46, 0.29, 0.15, 1.],
            [0.28, 0.21, 0.14, 1.],
        ),
        AnimalSpecies::Boar => (
            0.66,
            Vec3::new(0.36, 0.35, 0.63),
            0.36,
            [0.23, 0.20, 0.17, 1.],
            [0.37, 0.28, 0.23, 1.],
        ),
        AnimalSpecies::Hare => (
            0.27,
            Vec3::new(0.15, 0.19, 0.28),
            0.12,
            [0.46, 0.40, 0.30, 1.],
            [0.72, 0.66, 0.53, 1.],
        ),
        AnimalSpecies::Fox => (
            0.49,
            Vec3::new(0.18, 0.22, 0.44),
            0.31,
            [0.66, 0.29, 0.10, 1.],
            [0.86, 0.81, 0.65, 1.],
        ),
        AnimalSpecies::Ibex => (
            0.88,
            Vec3::new(0.27, 0.33, 0.56),
            0.61,
            [0.39, 0.35, 0.27, 1.],
            [0.27, 0.25, 0.21, 1.],
        ),
    };
    let resting = pose == WILDLIFE_RESTING;
    let feeding = matches!(pose, WILDLIFE_GRAZING | WILDLIFE_DRINKING);
    let body_y = if resting { body - leg * 0.62 } else { body };
    let phase = pose as f32 * std::f32::consts::TAU / 8.;
    let moving = pose < 8;
    let mut mesh = MeshBuilder::default();
    mesh.ellipsoid(Vec3::new(0., body_y, 0.), radius, fur, 12);
    // Lighter throat and belly give a visible silhouette under the directional sun.
    let belly = if species == AnimalSpecies::Fox {
        muzzle
    } else {
        tint(fur, 1.20)
    };
    mesh.ellipsoid(
        Vec3::new(0., body_y - radius.y * 0.46, -radius.z * 0.16),
        radius * Vec3::new(0.83, 0.47, 0.80),
        belly,
        8,
    );
    let head_y = if feeding {
        if species == AnimalSpecies::Hare {
            0.18
        } else if species == AnimalSpecies::Ibex {
            0.40
        } else {
            0.30
        }
    } else {
        body_y
            + match species {
                AnimalSpecies::Deer => 0.55,
                AnimalSpecies::Ibex => 0.32,
                AnimalSpecies::Hare => 0.14,
                AnimalSpecies::Fox => 0.16,
                AnimalSpecies::Boar => 0.03,
            }
    };
    let head_z = -radius.z
        - if feeding {
            radius.z * 0.30
        } else {
            radius.z * 0.15
        };
    let head = Vec3::new(0., head_y, head_z);
    let head_size = match species {
        AnimalSpecies::Deer => Vec3::new(0.16, 0.20, 0.25),
        AnimalSpecies::Boar => Vec3::new(0.27, 0.25, 0.34),
        AnimalSpecies::Hare => Vec3::new(0.115, 0.13, 0.15),
        AnimalSpecies::Fox => Vec3::new(0.15, 0.16, 0.20),
        AnimalSpecies::Ibex => Vec3::new(0.18, 0.21, 0.24),
    };
    mesh.cylinder(
        Vec3::new(0., body_y + radius.y * 0.20, -radius.z * 0.60),
        head,
        head_size.x * 0.80,
        fur,
        8,
    );
    mesh.ellipsoid(head, head_size, fur, 10);
    let nose = head + Vec3::new(0., -head_size.y * 0.23, -head_size.z * 0.86);
    mesh.ellipsoid(nose, head_size * Vec3::new(0.64, 0.51, 0.78), muzzle, 8);
    mesh.ellipsoid(
        nose - Vec3::Z * head_size.z * 0.64,
        head_size * Vec3::new(0.40, 0.25, 0.16),
        [0.10, 0.095, 0.085, 1.],
        6,
    );
    for sign in [-1., 1.] {
        mesh.ellipsoid(
            head + Vec3::new(
                sign * head_size.x * 0.84,
                head_size.y * 0.24,
                -head_size.z * 0.38,
            ),
            Vec3::splat(if species == AnimalSpecies::Hare {
                0.019
            } else {
                0.025
            }),
            [0.065, 0.07, 0.065, 1.],
            6,
        );
        let ear_base = head
            + Vec3::new(
                sign * head_size.x * 0.76,
                head_size.y * 0.7,
                head_size.z * 0.2,
            );
        if species == AnimalSpecies::Fox {
            mesh.triangle(
                ear_base + Vec3::new(-0.07, 0., 0.),
                ear_base + Vec3::new(sign * 0.025, 0.23, 0.02),
                ear_base + Vec3::new(0.07, 0., 0.),
                fur,
            );
            mesh.triangle(
                ear_base + Vec3::new(-0.04, 0.02, -0.006),
                ear_base + Vec3::new(sign * 0.02, 0.17, 0.013),
                ear_base + Vec3::new(0.04, 0.02, -0.006),
                [0.17, 0.13, 0.10, 1.],
            );
        } else {
            let ear = if species == AnimalSpecies::Hare {
                Vec3::new(0.045, 0.23, 0.034)
            } else if species == AnimalSpecies::Boar {
                Vec3::new(0.10, 0.14, 0.045)
            } else {
                Vec3::new(0.075, 0.15, 0.045)
            };
            mesh.ellipsoid(ear_base + Vec3::Y * ear.y * 0.65, ear, fur, 6);
            mesh.ellipsoid(
                ear_base + Vec3::new(0., ear.y * 0.7, -ear.z * 0.65),
                ear * Vec3::new(0.55, 0.7, 0.3),
                tint(muzzle, 1.20),
                6,
            );
        }
    }
    // Opposed diagonal legs provide an actual walking/running silhouette.
    for (index, (x, z)) in [
        (-radius.x * 0.72, -radius.z * 0.65),
        (radius.x * 0.72, -radius.z * 0.65),
        (-radius.x * 0.72, radius.z * 0.58),
        (radius.x * 0.72, radius.z * 0.58),
    ]
    .into_iter()
    .enumerate()
    {
        let swing = if moving {
            (phase
                + if matches!(index, 1 | 2) {
                    std::f32::consts::PI
                } else {
                    0.
                })
            .sin()
        } else {
            0.
        };
        let foot_y = if resting {
            0.035
        } else {
            0.04 + swing.max(0.) * leg * 0.18
        };
        let foot_z = z + if resting {
            radius.z * 0.18
        } else {
            swing * leg * 0.35
        };
        let upper = Vec3::new(x, body_y - radius.y * 0.30, z);
        let foot = Vec3::new(x, foot_y + 0.04, foot_z);
        let knee = upper.lerp(foot, 0.54) + Vec3::Z * (if index < 2 { 0.04 } else { -0.08 });
        let thickness = if species == AnimalSpecies::Boar {
            0.07
        } else {
            radius.x * 0.18
        };
        mesh.cylinder(upper, knee, thickness, fur, 7);
        mesh.cylinder(knee, foot, thickness * 0.68, tint(fur, 0.68), 6);
        mesh.ellipsoid(
            foot - Vec3::Y * 0.035,
            Vec3::new(thickness * 1.1, 0.045, thickness * 1.65),
            [0.15, 0.13, 0.11, 1.],
            6,
        );
    }
    match species {
        AnimalSpecies::Deer => {
            for sign in [-1., 1.] {
                let base = head + Vec3::new(sign * 0.10, 0.15, 0.04);
                let tip = base + Vec3::new(sign * 0.24, 0.48, 0.13);
                mesh.cylinder(base, tip, 0.025, [0.46, 0.40, 0.29, 1.], 5);
                for i in 1..4 {
                    let at = base.lerp(tip, i as f32 * 0.23);
                    mesh.cylinder(
                        at,
                        at + Vec3::new(sign * 0.11, 0.13, -0.11),
                        0.018,
                        [0.51, 0.45, 0.33, 1.],
                        5,
                    );
                }
            }
            mesh.ellipsoid(
                Vec3::new(0., body_y + 0.03, radius.z * 0.94),
                Vec3::new(0.11, 0.12, 0.16),
                [0.78, 0.72, 0.56, 1.],
                8,
            );
        }
        AnimalSpecies::Ibex => {
            for sign in [-1., 1.] {
                let base = head + Vec3::new(sign * 0.12, 0.16, 0.04);
                let a = base + Vec3::new(sign * 0.06, 0.30, 0.02);
                let b = base + Vec3::new(sign * 0.10, 0.51, 0.20);
                let c = base + Vec3::new(sign * 0.10, 0.39, 0.43);
                for (from, to, r) in [(base, a, 0.055), (a, b, 0.045), (b, c, 0.027)] {
                    mesh.cylinder(from, to, r, [0.24, 0.22, 0.17, 1.], 7);
                }
            }
            mesh.ellipsoid(
                head + Vec3::new(0., -0.24, -0.10),
                Vec3::new(0.06, 0.15, 0.07),
                [0.27, 0.24, 0.19, 1.],
                7,
            );
        }
        AnimalSpecies::Fox => {
            let tail = Vec3::new(0.06, body_y - 0.10, radius.z + 0.34);
            mesh.ellipsoid(tail, Vec3::new(0.15, 0.16, 0.42), fur, 10);
            mesh.ellipsoid(
                tail + Vec3::Z * 0.28,
                Vec3::new(0.12, 0.12, 0.18),
                muzzle,
                8,
            );
        }
        AnimalSpecies::Hare => {
            mesh.ellipsoid(
                Vec3::new(0., body_y + 0.03, radius.z),
                Vec3::splat(0.09),
                [0.83, 0.79, 0.67, 1.],
                8,
            );
        }
        AnimalSpecies::Boar => {
            for sign in [-1., 1.] {
                mesh.cylinder(
                    nose + Vec3::new(sign * 0.19, -0.05, -0.12),
                    nose + Vec3::new(sign * 0.22, 0.12, -0.19),
                    0.027,
                    [0.81, 0.77, 0.62, 1.],
                    6,
                );
            }
            for i in 0..6 {
                let z = radius.z * (i as f32 / 6. * 1.4 - 0.7);
                mesh.triangle(
                    Vec3::new(-0.035, body_y + radius.y * 0.86, z),
                    Vec3::new(0., body_y + radius.y + 0.12, z + 0.07),
                    Vec3::new(0.035, body_y + radius.y * 0.86, z + 0.1),
                    tint(fur, 0.65),
                );
            }
            mesh.cylinder(
                Vec3::new(0., body_y, radius.z * 0.94),
                Vec3::new(0.1, body_y + 0.05, radius.z + 0.22),
                0.025,
                fur,
                6,
            );
        }
    }
    mesh.finish().expect("wildlife model has a body")
}

fn tint(color: [f32; 4], factor: f32) -> [f32; 4] {
    [
        color[0] * factor,
        color[1] * factor,
        color[2] * factor,
        color[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    #[test]
    fn species_pose_palette_has_distinct_finite_bounded_grounded_geometry() {
        let mut silhouettes = Vec::new();
        for species in [
            AnimalSpecies::Deer,
            AnimalSpecies::Boar,
            AnimalSpecies::Hare,
            AnimalSpecies::Fox,
            AnimalSpecies::Ibex,
        ] {
            let mut standing = None;
            for pose in 0..WILDLIFE_POSE_COUNT {
                let mesh = wildlife_mesh(species, pose);
                let VertexAttributeValues::Float32x3(points) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!()
                };
                assert!(points.len() < 9_000, "{species:?}: {}", points.len());
                assert!(points.iter().flatten().all(|p| p.is_finite()));
                assert!(
                    points.iter().all(|p| p[1] >= -0.08),
                    "{species:?} pose {pose}"
                );
                if pose == WILDLIFE_STANDING {
                    standing = Some(points.clone());
                }
                if pose == WILDLIFE_GRAZING {
                    assert_ne!(points, standing.as_ref().unwrap());
                }
            }
            let points = standing.unwrap();
            silhouettes.push((
                points.len(),
                points.iter().map(|p| p[1]).fold(0f32, f32::max).to_bits(),
            ));
        }
        silhouettes.sort();
        silhouettes.dedup();
        assert_eq!(silhouettes.len(), 5);
    }
}
