//! Original, bounded vehicle and character geometry for the reference game.
//! Models face -Z, use metres, and rest at y=0. No downloaded game assets.
use bevy::prelude::*;
use terra_bevy::{
    geometry::MeshBuilder,
    rules::{CharacterProfile, VehicleModel},
};

const RUBBER: [f32; 4] = [0.075, 0.085, 0.09, 1.];
const CHASSIS: [f32; 4] = [0.15, 0.18, 0.19, 1.];
const METAL: [f32; 4] = [0.62, 0.66, 0.66, 1.];
const GLASS: [f32; 4] = [0.13, 0.25, 0.29, 1.];
const CREAM: [f32; 4] = [0.85, 0.85, 0.77, 1.];
const WHITE: [f32; 4] = [0.92, 0.91, 0.84, 1.];

pub fn vehicle_dimensions(model: VehicleModel) -> Vec3 {
    Vec3::from_array(model.dimensions_m().map(|v| v as f32))
}

fn paint_color(paint: u8) -> [f32; 4] {
    [
        [0.21, 0.42, 0.36, 1.],
        [0.86, 0.82, 0.68, 1.],
        [0.63, 0.19, 0.16, 1.],
        [0.18, 0.37, 0.56, 1.],
        [0.18, 0.21, 0.24, 1.],
        [0.81, 0.43, 0.15, 1.],
        [0.83, 0.67, 0.20, 1.],
        [0.38, 0.25, 0.43, 1.],
    ][usize::from(paint.min(7))]
}

fn wheel(b: &mut MeshBuilder, x: f32, z: f32, radius: f32, width: f32) {
    let center = Vec3::new(x, radius, z);
    b.cylinder(
        center - Vec3::X * width * 0.5,
        center + Vec3::X * width * 0.5,
        radius,
        RUBBER,
        16,
    );
    let face = center + Vec3::X * (x.signum() * (width * 0.5 + 0.005));
    b.cylinder(
        face - Vec3::X * 0.007,
        face + Vec3::X * 0.007,
        radius * 0.57,
        METAL,
        12,
    );
    b.cylinder(
        face - Vec3::X * 0.009,
        face + Vec3::X * 0.009,
        radius * 0.22,
        CHASSIS,
        8,
    );
}

fn running_gear(
    b: &mut MeshBuilder,
    w: f32,
    length: f32,
    radius: f32,
    axles: &[f32],
    wheels: bool,
) {
    b.cuboid(
        Vec3::new(0., radius + 0.13, 0.),
        Vec3::new(w * 0.65, 0.24, length * 0.89),
        CHASSIS,
    );
    for &z in axles {
        b.cylinder(
            Vec3::new(-w * 0.42, radius, z),
            Vec3::new(w * 0.42, radius, z),
            0.07,
            CHASSIS,
            6,
        );
        if wheels {
            for x in [-w * 0.43, w * 0.43] {
                wheel(b, x, z, radius, w * 0.10);
            }
        }
    }
}

/// Sloped windscreen and rear window make the passenger cab readable even at
/// a distant camera. Opaque glazing avoids transparent sorting/overdraw.
fn cabin(b: &mut MeshBuilder, w: f32, base: f32, top: f32, front: f32, back: f32, paint: [f32; 4]) {
    let half = w * 0.41;
    let roof_front = front + (back - front) * 0.19;
    let roof_back = back - (back - front) * 0.12;
    b.quad(
        Vec3::new(-half, top, roof_front),
        Vec3::new(-half, top, roof_back),
        Vec3::new(half, top, roof_back),
        Vec3::new(half, top, roof_front),
        paint,
    );
    b.quad(
        Vec3::new(-half, base, front),
        Vec3::new(-half, top, roof_front),
        Vec3::new(half, top, roof_front),
        Vec3::new(half, base, front),
        GLASS,
    );
    b.quad(
        Vec3::new(half, base, back),
        Vec3::new(half, top, roof_back),
        Vec3::new(-half, top, roof_back),
        Vec3::new(-half, base, back),
        GLASS,
    );
    for sign in [-1., 1.] {
        let x = sign * half;
        let verts = [
            Vec3::new(x, base, front),
            Vec3::new(x, base, back),
            Vec3::new(x, top, roof_back),
            Vec3::new(x, top, roof_front),
        ];
        if sign < 0. {
            b.quad(verts[0], verts[1], verts[2], verts[3], GLASS);
        } else {
            b.quad(verts[3], verts[2], verts[1], verts[0], GLASS);
        }
        for z in [roof_front, (front + back) * 0.5, roof_back] {
            b.cuboid(
                Vec3::new(x, (base + top) * 0.5, z),
                Vec3::new(0.045, top - base, 0.055),
                paint,
            );
        }
        b.cuboid(
            Vec3::new(sign * w * 0.47, base + 0.13, front + 0.32),
            Vec3::new(w * 0.055, 0.13, 0.24),
            paint,
        );
        b.cuboid(
            Vec3::new(sign * w * 0.44, base - 0.13, (front + back) * 0.5),
            Vec3::new(0.025, 0.04, 0.20),
            METAL,
        );
    }
    b.cuboid(
        Vec3::new(0., top - 0.025, (roof_front + roof_back) * 0.5),
        Vec3::new(w * 0.84, 0.05, roof_back - roof_front + 0.05),
        paint,
    );
}

fn lamps(b: &mut MeshBuilder, w: f32, length: f32, y: f32) {
    for x in [-w * 0.32, w * 0.32] {
        b.cuboid(
            Vec3::new(x, y, -length * 0.495),
            Vec3::new(w * 0.18, 0.15, 0.025),
            [0.98, 0.91, 0.65, 1.],
        );
        b.cuboid(
            Vec3::new(x, y, length * 0.495),
            Vec3::new(w * 0.14, 0.14, 0.025),
            [0.68, 0.10, 0.08, 1.],
        );
    }
    b.cuboid(
        Vec3::new(0., y - 0.21, -length * 0.48),
        Vec3::new(w * 0.82, 0.13, 0.13),
        CHASSIS,
    );
    b.cuboid(
        Vec3::new(0., y - 0.21, length * 0.48),
        Vec3::new(w * 0.82, 0.13, 0.13),
        CHASSIS,
    );
    b.cuboid(
        Vec3::new(0., y - 0.11, length * 0.497),
        Vec3::new(0.37, 0.10, 0.012),
        WHITE,
    );
    for n in 0..4 {
        b.cuboid(
            Vec3::new(-0.28 + n as f32 * 0.185, y + 0.01, -length * 0.499),
            Vec3::new(0.06, 0.14, 0.008),
            CHASSIS,
        );
    }
}

fn service_lights(b: &mut MeshBuilder, w: f32, y: f32, z: f32, amber: bool) {
    b.cuboid(
        Vec3::new(0., y - 0.055, z),
        Vec3::new(w * 0.68, 0.10, 0.24),
        CHASSIS,
    );
    for sign in [-1., 1.] {
        let color = if amber {
            [0.98, 0.58, 0.10, 1.]
        } else if sign < 0. {
            [0.12, 0.39, 0.92, 1.]
        } else {
            [0.88, 0.17, 0.13, 1.]
        };
        b.cuboid(
            Vec3::new(sign * w * 0.23, y + 0.02, z),
            Vec3::new(w * 0.20, 0.12, 0.22),
            color,
        );
    }
}

fn utility_cab(b: &mut MeshBuilder, w: f32, h: f32, length: f32, paint: [f32; 4]) {
    let front = -length * 0.49;
    let back = -length * 0.10;
    b.cuboid(
        Vec3::new(0., h * 0.34, (front + back) * 0.5),
        Vec3::new(w * 0.9, h * 0.32, back - front),
        paint,
    );
    cabin(b, w, h * 0.50, h * 0.80, front + 0.12, back, paint);
    for sign in [-1., 1.] {
        b.cuboid(
            Vec3::new(sign * w * 0.43, h * 0.22, -length * 0.2),
            Vec3::new(w * 0.12, 0.12, 0.75),
            METAL,
        );
        b.cuboid(
            Vec3::new(sign * w * 0.45, h * 0.42, -length * 0.27),
            Vec3::new(0.025, 0.08, 0.28),
            METAL,
        );
    }
}

#[cfg(test)]
fn vehicle_mesh(model: VehicleModel, paint: u8) -> Mesh {
    vehicle_geometry(model, paint, true)
}
/// Animated native vehicles attach shared rolling wheels to this body mesh.
pub fn vehicle_body_mesh(model: VehicleModel, paint: u8) -> Mesh {
    vehicle_geometry(model, paint, false)
}
fn vehicle_geometry(model: VehicleModel, paint: u8, wheels: bool) -> Mesh {
    let d = vehicle_dimensions(model);
    let (w, length) = (d.x, d.z);
    let color = paint_color(paint);
    let mut b = MeshBuilder::default();
    match model {
        VehicleModel::Compact | VehicleModel::Taxi => {
            let taxi = model == VehicleModel::Taxi;
            let paint = if taxi && paint == 0 {
                [0.86, 0.68, 0.19, 1.]
            } else {
                color
            };
            running_gear(
                &mut b,
                w,
                length,
                0.31,
                &[-length * 0.31, length * 0.30],
                wheels,
            );
            b.cuboid(
                Vec3::new(0., 0.67, 0.),
                Vec3::new(w * 0.9, 0.43, length * 0.96),
                paint,
            );
            cabin(
                &mut b,
                w,
                0.89,
                if taxi { 1.48 } else { 1.51 },
                -length * 0.20,
                length * 0.30,
                paint,
            );
            b.cuboid(
                Vec3::new(0., 0.91, -length * 0.34),
                Vec3::new(w * 0.85, 0.08, length * 0.25),
                paint,
            );
            for sign in [-1., 1.] {
                b.cuboid(
                    Vec3::new(sign * w * 0.454, 0.67, 0.),
                    Vec3::new(0.02, 0.06, length * 0.70),
                    CHASSIS,
                );
                if taxi {
                    for n in 0..10 {
                        b.cuboid(
                            Vec3::new(sign * w * 0.46, 0.87, -0.8 + n as f32 * 0.18),
                            Vec3::new(0.02, 0.09, 0.09),
                            if n % 2 == 0 { CHASSIS } else { WHITE },
                        );
                    }
                }
            }
            if taxi {
                b.cuboid(
                    Vec3::new(0., 1.63, 0.06),
                    Vec3::new(0.50, 0.20, 0.30),
                    CREAM,
                );
                b.cuboid(
                    Vec3::new(0., 1.63, -0.096),
                    Vec3::new(0.28, 0.07, 0.015),
                    CHASSIS,
                );
            }
            lamps(&mut b, w, length, 0.72);
        }
        VehicleModel::Pickup => {
            running_gear(&mut b, w, length, 0.40, &[-1.55, 1.55], wheels);
            b.cuboid(
                Vec3::new(0., 0.84, -0.55),
                Vec3::new(w * 0.9, 0.51, length * 0.72),
                color,
            );
            cabin(&mut b, w, 1.12, 1.88, -1.66, 0.14, color);
            b.cuboid(
                Vec3::new(0., 1.05, 1.30),
                Vec3::new(w * 0.86, 0.15, 2.30),
                [0.30, 0.28, 0.24, 1.],
            );
            for sign in [-1., 1.] {
                b.cuboid(
                    Vec3::new(sign * w * 0.425, 1.28, 1.30),
                    Vec3::new(0.12, 0.43, 2.30),
                    color,
                );
            }
            b.cuboid(
                Vec3::new(0., 1.28, 2.41),
                Vec3::new(w * 0.9, 0.43, 0.12),
                color,
            );
            lamps(&mut b, w, length, 0.91);
        }
        VehicleModel::Bus => {
            running_gear(&mut b, w, length, 0.51, &[-3.25, 2.55], wheels);
            b.cuboid(
                Vec3::new(0., 1.77, 0.),
                Vec3::new(w * 0.92, 2.50, length * 0.97),
                CREAM,
            );
            b.cuboid(
                Vec3::new(0., 1.12, 0.),
                Vec3::new(w * 0.928, 0.53, length * 0.974),
                color,
            );
            b.cuboid(
                Vec3::new(0., 2.29, -length * 0.489),
                Vec3::new(w * 0.81, 1.02, 0.025),
                GLASS,
            );
            b.cuboid(
                Vec3::new(0., 2.91, -length * 0.490),
                Vec3::new(w * 0.65, 0.20, 0.03),
                CHASSIS,
            );
            for n in 0..6 {
                b.cuboid(
                    Vec3::new(-0.46 + n as f32 * 0.18, 2.91, -length * 0.493),
                    Vec3::new(0.08, 0.05, 0.015),
                    [0.89, 0.68, 0.25, 1.],
                );
            }
            for sign in [-1., 1.] {
                for n in 0..8 {
                    b.cuboid(
                        Vec3::new(sign * w * 0.466, 2.30, -3.85 + n as f32 * 1.08),
                        Vec3::new(0.025, 0.90, 0.93),
                        GLASS,
                    );
                }
                b.cuboid(
                    Vec3::new(sign * w * 0.466, 0.64, 0.),
                    Vec3::new(0.03, 0.10, length * 0.92),
                    CHASSIS,
                );
            }
            for z in [-3.95, 0.20] {
                b.cuboid(
                    Vec3::new(w * 0.477, 1.67, z),
                    Vec3::new(0.025, 2.10, 0.95),
                    CHASSIS,
                );
                for offset in [-0.24, 0.24] {
                    b.cuboid(
                        Vec3::new(w * 0.483, 1.91, z + offset),
                        Vec3::new(0.02, 1.34, 0.38),
                        GLASS,
                    );
                }
                b.cuboid(
                    Vec3::new(w * 0.483, 0.77, z),
                    Vec3::new(0.02, 0.10, 0.86),
                    [0.86, 0.67, 0.24, 1.],
                );
            }
            b.cuboid(
                Vec3::new(0., 3.12, 0.75),
                Vec3::new(w * 0.65, 0.20, 2.25),
                [0.64, 0.67, 0.65, 1.],
            );
            lamps(&mut b, w, length, 1.13);
        }
        VehicleModel::BoxTruck => {
            running_gear(&mut b, w, length, 0.49, &[-2.35, 2.15], wheels);
            utility_cab(&mut b, w, 3.1, length, color);
            b.cuboid(
                Vec3::new(0., 2.20, 1.0),
                Vec3::new(w * 0.94, 2.62, 4.95),
                CREAM,
            );
            for sign in [-1., 1.] {
                b.cuboid(
                    Vec3::new(sign * w * 0.475, 2.22, 1.0),
                    Vec3::new(0.02, 0.50, 4.55),
                    color,
                );
                for n in 0..8 {
                    b.cuboid(
                        Vec3::new(sign * w * 0.477, 2.20, -1.27 + n as f32 * 0.64),
                        Vec3::new(0.015, 2.35, 0.025),
                        METAL,
                    );
                }
            }
            for x in [-w * 0.23, w * 0.23] {
                b.cuboid(
                    Vec3::new(x, 2.20, length * 0.486),
                    Vec3::new(w * 0.445, 2.43, 0.04),
                    [0.73, 0.75, 0.71, 1.],
                );
                b.cylinder(
                    Vec3::new(x + 0.15, 1.13, length * 0.493),
                    Vec3::new(x + 0.15, 3.26, length * 0.493),
                    0.025,
                    CHASSIS,
                    6,
                );
            }
            lamps(&mut b, w, length, 0.95);
        }
        VehicleModel::Semi => {
            running_gear(&mut b, w, length, 0.52, &[-2.20, 1.45, 2.45], wheels);
            utility_cab(&mut b, w, 3.5, length, color);
            b.cuboid(
                Vec3::new(0., 2.49, -0.80),
                Vec3::new(w * 0.88, 2.05, 1.32),
                color,
            );
            b.cuboid(
                Vec3::new(0., 3.50, -1.15),
                Vec3::new(w * 0.83, 0.12, 1.87),
                color,
            );
            b.cylinder(
                Vec3::new(0., 1.12, 1.48),
                Vec3::new(0., 1.26, 1.48),
                0.48,
                CHASSIS,
                12,
            );
            for sign in [-1., 1.] {
                b.cylinder(
                    Vec3::new(sign * w * 0.36, 0.83, -0.12),
                    Vec3::new(sign * w * 0.36, 0.83, 0.92),
                    0.27,
                    METAL,
                    12,
                );
                b.cylinder(
                    Vec3::new(sign * w * 0.40, 1.3, -0.35),
                    Vec3::new(sign * w * 0.40, 3.45, -0.35),
                    0.065,
                    METAL,
                    8,
                );
                b.cuboid(
                    Vec3::new(sign * w * 0.39, 1.17, 1.96),
                    Vec3::new(w * 0.21, 0.12, 2.0),
                    color,
                );
            }
            lamps(&mut b, w, length, 1.02);
        }
        VehicleModel::TowTruck => {
            running_gear(&mut b, w, length, 0.46, &[-2.05, 2.03], wheels);
            utility_cab(&mut b, w, 2.95, length, color);
            b.cuboid(
                Vec3::new(0., 1.03, 1.02),
                Vec3::new(w * 0.90, 0.20, 4.25),
                [0.30, 0.32, 0.32, 1.],
            );
            for sign in [-1., 1.] {
                b.cuboid(
                    Vec3::new(sign * w * 0.42, 1.25, 1.02),
                    Vec3::new(0.12, 0.35, 4.20),
                    color,
                );
                for n in 0..7 {
                    b.cuboid(
                        Vec3::new(sign * w * 0.45, 1.26, -0.62 + n as f32 * 0.55),
                        Vec3::new(0.02, 0.22, 0.23),
                        [0.93, 0.67, 0.14, 1.],
                    );
                }
            }
            b.cylinder(
                Vec3::new(0., 1.13, 0.18),
                Vec3::new(0., 2.35, 1.67),
                0.13,
                color,
                8,
            );
            b.cylinder(
                Vec3::new(0., 2.35, 1.67),
                Vec3::new(0., 2.12, 2.60),
                0.09,
                METAL,
                8,
            );
            b.cylinder(
                Vec3::new(0., 2.12, 2.60),
                Vec3::new(0., 1.43, 2.60),
                0.022,
                RUBBER,
                6,
            );
            b.cylinder(
                Vec3::new(-0.16, 1.43, 2.60),
                Vec3::new(0.16, 1.43, 2.60),
                0.06,
                METAL,
                8,
            );
            service_lights(&mut b, w, 2.52, -1.25, true);
            lamps(&mut b, w, length, 0.99);
        }
        VehicleModel::Ambulance => {
            running_gear(&mut b, w, length, 0.41, &[-1.90, 1.80], wheels);
            utility_cab(&mut b, w, 2.8, length, WHITE);
            b.cuboid(
                Vec3::new(0., 1.83, 0.76),
                Vec3::new(w * 0.90, 1.86, 4.15),
                WHITE,
            );
            for sign in [-1., 1.] {
                b.cuboid(
                    Vec3::new(sign * w * 0.455, 1.42, 0.72),
                    Vec3::new(0.02, 0.28, 3.96),
                    [0.10, 0.43, 0.49, 1.],
                );
                b.cuboid(
                    Vec3::new(sign * w * 0.46, 2.17, 0.85),
                    Vec3::new(0.025, 0.63, 0.19),
                    [0.10, 0.43, 0.49, 1.],
                );
                b.cuboid(
                    Vec3::new(sign * w * 0.46, 2.17, 0.85),
                    Vec3::new(0.025, 0.19, 0.63),
                    [0.10, 0.43, 0.49, 1.],
                );
            }
            for x in [-w * 0.22, w * 0.22] {
                b.cuboid(
                    Vec3::new(x, 1.90, 2.85),
                    Vec3::new(w * 0.42, 1.63, 0.04),
                    CREAM,
                );
                b.cuboid(
                    Vec3::new(x, 2.28, 2.88),
                    Vec3::new(w * 0.32, 0.59, 0.025),
                    GLASS,
                );
            }
            service_lights(&mut b, w, 2.80, -0.73, false);
            lamps(&mut b, w, length, 0.95);
        }
        VehicleModel::FireEngine => {
            let paint = if paint == 0 {
                [0.64, 0.16, 0.12, 1.]
            } else {
                color
            };
            running_gear(&mut b, w, length, 0.53, &[-2.55, 2.50], wheels);
            utility_cab(&mut b, w, 3.35, length, paint);
            b.cuboid(
                Vec3::new(0., 1.94, 1.10),
                Vec3::new(w * 0.91, 2.12, 5.58),
                paint,
            );
            for sign in [-1., 1.] {
                for n in 0..4 {
                    let z = -0.94 + n as f32 * 1.33;
                    b.cuboid(
                        Vec3::new(sign * w * 0.46, 1.94, z),
                        Vec3::new(0.035, 1.54, 1.16),
                        [0.61, 0.64, 0.63, 1.],
                    );
                    for row in 0..6 {
                        b.cuboid(
                            Vec3::new(sign * w * 0.468, 1.29 + row as f32 * 0.23, z),
                            Vec3::new(0.015, 0.025, 1.10),
                            CHASSIS,
                        );
                    }
                }
                b.cuboid(
                    Vec3::new(sign * w * 0.462, 1.04, 1.10),
                    Vec3::new(0.025, 0.14, 5.35),
                    [0.90, 0.75, 0.23, 1.],
                );
            }
            for x in [-0.43, 0.43] {
                b.cylinder(
                    Vec3::new(x, 3.20, -1.65),
                    Vec3::new(x, 3.20, 3.62),
                    0.045,
                    METAL,
                    8,
                );
            }
            for n in 0..13 {
                let z = -1.53 + n as f32 * 0.42;
                b.cylinder(
                    Vec3::new(-0.43, 3.20, z),
                    Vec3::new(0.43, 3.20, z),
                    0.025,
                    METAL,
                    6,
                );
            }
            b.cylinder(
                Vec3::new(0., 2.99, 2.52),
                Vec3::new(0., 3.20, 2.52),
                0.38,
                [0.25, 0.27, 0.25, 1.],
                12,
            );
            service_lights(&mut b, w, 2.89, -2.55, false);
            lamps(&mut b, w, length, 1.05);
        }
    }
    b.finish()
        .expect("reference vehicle contains solid geometry")
}

/// Radius and axle positions shared by native wheel animation and body geometry.
pub fn wheel_layout(model: VehicleModel) -> (f32, Vec<f32>) {
    use VehicleModel::*;
    let length = vehicle_dimensions(model).z;
    match model {
        Compact | Taxi => (0.31, vec![-length * 0.31, length * 0.30]),
        Pickup => (0.40, vec![-1.55, 1.55]),
        Bus => (0.51, vec![-3.25, 2.55]),
        BoxTruck => (0.49, vec![-2.35, 2.15]),
        Semi => (0.52, vec![-2.20, 1.45, 2.45]),
        TowTruck => (0.46, vec![-2.05, 2.03]),
        Ambulance => (0.41, vec![-1.90, 1.80]),
        FireEngine => (0.53, vec![-2.55, 2.50]),
    }
}
pub fn rolling_wheel_mesh(radius: f32, width: f32) -> Mesh {
    let mut b = MeshBuilder::default();
    b.cylinder(
        Vec3::new(-width * 0.5, 0., 0.),
        Vec3::new(width * 0.5, 0., 0.),
        radius,
        RUBBER,
        20,
    );
    for side in [-1., 1.] {
        let x = side * (width * 0.5 + 0.008);
        b.cylinder(
            Vec3::new(x - 0.008, 0., 0.),
            Vec3::new(x + 0.008, 0., 0.),
            radius * 0.57,
            CHASSIS,
            16,
        );
        for spoke in 0..5 {
            let angle = spoke as f32 * std::f32::consts::TAU / 5.;
            let direction = Vec3::new(0., angle.cos(), angle.sin());
            b.cylinder(
                Vec3::new(x, 0., 0.) + direction * radius * 0.16,
                Vec3::new(x, 0., 0.) + direction * radius * 0.52,
                radius * 0.065,
                METAL,
                5,
            );
        }
        b.cylinder(
            Vec3::new(x - 0.012, 0., 0.),
            Vec3::new(x + 0.012, 0., 0.),
            radius * 0.18,
            METAL,
            8,
        );
    }
    b.finish().expect("a wheel has solid geometry")
}

pub fn trailer_mesh() -> Mesh {
    let mut b = MeshBuilder::default();
    let paint = [0.29, 0.36, 0.37, 1.];
    running_gear(&mut b, 2.10, 4.65, 0.40, &[0.10, 1.13], true);
    b.cuboid(
        Vec3::new(0., 0.88, 0.0),
        Vec3::new(2.0, 0.18, 4.1),
        [0.46, 0.35, 0.23, 1.],
    );
    for x in [-1.0, 1.0] {
        b.cuboid(Vec3::new(x, 1.20, 0.), Vec3::new(0.10, 0.54, 4.1), paint);
        for z in [-1.87, 0., 1.87] {
            b.cuboid(Vec3::new(x, 1.27, z), Vec3::new(0.11, 0.78, 0.08), METAL);
        }
    }
    for z in [-2.0, 2.0] {
        b.cuboid(Vec3::new(0., 1.20, z), Vec3::new(2.0, 0.54, 0.10), paint);
    }
    for x in [-0.78, 0.78] {
        b.cylinder(
            Vec3::new(x, 0.65, -2.0),
            Vec3::new(0., 0.65, -3.0),
            0.065,
            METAL,
            8,
        );
    }
    b.cuboid(
        Vec3::new(0., 0.65, -3.05),
        Vec3::new(0.20, 0.15, 0.27),
        CHASSIS,
    );
    for x in [-0.81, 0.81] {
        b.cuboid(
            Vec3::new(x, 0.88, 2.07),
            Vec3::new(0.20, 0.14, 0.03),
            [0.77, 0.12, 0.08, 1.],
        );
    }
    b.finish().expect("cargo trailer contains solid geometry")
}

/// Small camera-space interior: eye at origin, forward -Z. The caller parents
/// this to the first-person camera and removes it in exterior/RTS modes.
pub fn cockpit_mesh(model: VehicleModel) -> Mesh {
    let mut b = MeshBuilder::default();
    let commercial = !matches!(
        model,
        VehicleModel::Compact | VehicleModel::Pickup | VehicleModel::Taxi
    );
    let trim = if commercial {
        [0.24, 0.28, 0.28, 1.]
    } else {
        [0.19, 0.22, 0.22, 1.]
    };
    b.cuboid(
        Vec3::new(0.15, -0.48, -0.80),
        Vec3::new(2.16, 0.32, 0.57),
        trim,
    );
    b.cuboid(
        Vec3::new(-0.05, -0.33, -0.58),
        Vec3::new(0.66, 0.22, 0.10),
        CHASSIS,
    );
    for x in [-0.20, 0.10] {
        b.cylinder(
            Vec3::new(x, -0.32, -0.52),
            Vec3::new(x, -0.32, -0.50),
            0.095,
            [0.07, 0.12, 0.14, 1.],
            16,
        );
        for n in 0..7 {
            let angle = std::f32::consts::PI * (0.13 + n as f32 * 0.124);
            b.cuboid(
                Vec3::new(x + angle.cos() * 0.069, -0.32 + angle.sin() * 0.069, -0.485),
                Vec3::new(0.012, 0.016, 0.008),
                [0.77, 0.85, 0.75, 1.],
            );
        }
        b.cylinder(
            Vec3::new(x, -0.32, -0.48),
            Vec3::new(x - 0.044, -0.273, -0.48),
            0.004,
            [0.90, 0.35, 0.14, 1.],
            5,
        );
    }
    b.cuboid(
        Vec3::new(0.61, -0.31, -0.52),
        Vec3::new(0.36, 0.20, 0.035),
        [0.11, 0.23, 0.26, 1.],
    );
    for n in 0..4 {
        b.cuboid(
            Vec3::new(0.61, -0.26 - n as f32 * 0.033, -0.496),
            Vec3::new(0.23, 0.012, 0.008),
            [0.41, 0.66, 0.62, 1.],
        );
    }
    let center = Vec3::new(-0.05, -0.35, -0.37);
    let radius = if commercial { 0.22 } else { 0.19 };
    for n in 0..20 {
        let a = n as f32 * std::f32::consts::TAU / 20.;
        let c = (n + 1) as f32 * std::f32::consts::TAU / 20.;
        b.cylinder(
            center + Vec3::new(a.cos() * radius, a.sin() * radius, 0.),
            center + Vec3::new(c.cos() * radius, c.sin() * radius, 0.),
            0.019,
            RUBBER,
            6,
        );
    }
    for offset in [
        Vec3::new(-radius, 0., 0.),
        Vec3::new(radius, 0., 0.),
        Vec3::new(0., -radius, 0.),
    ] {
        b.cylinder(center, center + offset, 0.025, CHASSIS, 6);
    }
    b.cuboid(center, Vec3::new(0.13, 0.075, 0.055), trim);
    b.finish().expect("driver cockpit contains solid geometry")
}

pub fn profile_mesh(body: u8, skin: u8, hair: u8, outfit: u8) -> Mesh {
    profile_mesh_pose(body, skin, hair, outfit, false)
}

/// Same saved customization, with the hip at local origin for an explicit
/// vehicle-seat presentation. No gameplay position or collision body changes.
pub fn seated_profile_mesh(body: u8, skin: u8, hair: u8, outfit: u8) -> Mesh {
    profile_mesh_pose(body, skin, hair, outfit, true)
}

fn profile_mesh_pose(body: u8, skin: u8, hair: u8, outfit: u8, seated: bool) -> Mesh {
    let profile = CharacterProfile {
        name: "Avatar".into(),
        body_style: body.min(2),
        skin: skin.min(7),
        hair: hair.min(7),
        outfit: outfit.min(7),
    };
    let (skin, hair, outfit) = crate::character::appearance_colors(&profile);
    let rgba = |c: [f32; 3]| [c[0], c[1], c[2], 1.];
    let scale = Vec3::from_array(crate::character::body_scale(&profile));
    let p = |x: f32, y: f32, z: f32| Vec3::new(x, y - if seated { 0.85 } else { 0. }, z) * scale;
    let size = |x, y, z| Vec3::new(x, y, z) * scale;
    let mut b = MeshBuilder::default();
    let trousers = [0.20, 0.25, 0.27, 1.];
    for sign in [-1., 1.] {
        if seated {
            b.cylinder(
                p(sign * 0.115, 0.85, 0.),
                p(sign * 0.13, 0.814, -0.398),
                0.079 * scale.x,
                trousers,
                10,
            );
            b.cylinder(
                p(sign * 0.13, 0.814, -0.398),
                p(sign * 0.13, 0.671, -0.745),
                0.072 * scale.x,
                trousers,
                10,
            );
        } else {
            b.cylinder(
                p(sign * 0.115, 0.81, 0.),
                p(sign * 0.13, 0.17, 0.01),
                0.079 * scale.x,
                trousers,
                10,
            );
        }
        b.cuboid(
            p(
                sign * 0.13,
                if seated { 0.671 } else { 0.075 },
                if seated { -0.800 } else { -0.055 },
            ),
            size(0.17, 0.14, 0.30),
            [0.13, 0.13, 0.12, 1.],
        );
        b.cylinder(
            p(sign * 0.24, 1.39, 0.),
            p(
                sign * (if seated { 0.15 } else { 0.29 }),
                if seated { 1.19 } else { 1.09 },
                if seated { -0.19 } else { 0. },
            ),
            0.075 * scale.x,
            rgba(outfit),
            10,
        );
        b.cylinder(
            p(
                sign * (if seated { 0.15 } else { 0.29 }),
                if seated { 1.19 } else { 1.09 },
                if seated { -0.19 } else { 0. },
            ),
            p(
                sign * (if seated { 0.15 } else { 0.31 }),
                if seated { 1.40 } else { 0.87 },
                if seated { -0.23 } else { -0.015 },
            ),
            0.055 * scale.x,
            rgba(skin),
            10,
        );
        b.ellipsoid(
            p(
                sign * (if seated { 0.15 } else { 0.31 }),
                if seated { 1.44 } else { 0.83 },
                if seated { -0.23 } else { -0.015 },
            ),
            size(0.057, 0.078, 0.06),
            rgba(skin),
            8,
        );
    }
    b.ellipsoid(p(0., 0.86, 0.), size(0.22, 0.15, 0.13), trousers, 10);
    b.ellipsoid(p(0., 1.17, 0.), size(0.235, 0.315, 0.145), rgba(outfit), 12);
    b.cylinder(p(0., 1.43, 0.), p(0., 1.53, 0.), 0.067, rgba(skin), 10);
    b.ellipsoid(p(0., 1.64, 0.), size(0.13, 0.17, 0.13), rgba(skin), 12);
    b.ellipsoid(
        p(0., 1.755, 0.014),
        size(0.137, 0.078, 0.136),
        rgba(hair),
        12,
    );
    for x in [-0.047, 0.047] {
        b.ellipsoid(
            p(x, 1.665, -0.121),
            size(0.014, 0.011, 0.012),
            [0.12, 0.11, 0.10, 1.],
            6,
        );
    }
    b.ellipsoid(
        p(0., 1.62, -0.132),
        size(0.022, 0.033, 0.028),
        rgba(skin),
        6,
    );
    b.cuboid(
        p(0., 1.25, -0.144),
        size(0.025, 0.27, 0.018),
        [0.14, 0.18, 0.18, 1.],
    );
    b.finish()
        .expect("character profile contains solid geometry")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn all_reference_models_fit_shared_collision_dimensions_and_have_bounded_meshes() {
        for model in terra_bevy::rules::VEHICLE_MODELS {
            let mesh = vehicle_mesh(model, 0);
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions missing");
            };
            assert!(points.len() > 100 && points.len() < 24_000);
            let d = vehicle_dimensions(model);
            for &p in points {
                assert!(p.iter().all(|n| n.is_finite()));
                assert!(p[0].abs() <= d.x * 0.5 + 0.035, "{model:?} width {p:?}");
                assert!(
                    p[1] >= -0.001 && p[1] <= d.y + 0.035,
                    "{model:?} height {p:?}"
                );
                assert!(p[2].abs() <= d.z * 0.5 + 0.035, "{model:?} length {p:?}");
            }
        }
    }

    #[test]
    fn profile_palettes_match_character_creation_and_body_scales_are_distinct() {
        for body in 0..3 {
            let mesh = profile_mesh(body, 0, 0, 0);
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions missing");
            };
            let max = points.iter().map(|p| p[1]).fold(0_f32, f32::max);
            let expected = 1.833
                * crate::character::body_scale(&CharacterProfile {
                    body_style: body,
                    ..Default::default()
                })[1];
            assert!((max - expected).abs() < 0.01);
        }
        let a = profile_mesh(0, 0, 0, 0);
        let b = profile_mesh(0, 7, 7, 7);
        let Some(VertexAttributeValues::Float32x4(a)) = a.attribute(Mesh::ATTRIBUTE_COLOR) else {
            panic!("colors missing");
        };
        let Some(VertexAttributeValues::Float32x4(b)) = b.attribute(Mesh::ATTRIBUTE_COLOR) else {
            panic!("colors missing");
        };
        assert_ne!(a, b);
    }

    #[test]
    fn seated_profile_fallback_keeps_all_choices_and_a_bounded_hip_origin() {
        for body in 0..3 {
            for palette in 0..8 {
                let mesh = seated_profile_mesh(body, palette, palette, palette);
                let VertexAttributeValues::Float32x3(vertices) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                else {
                    panic!()
                };
                assert!(vertices.len() < 8000);
                assert!(vertices.iter().flatten().all(|v| v.is_finite()));
                let top = vertices
                    .iter()
                    .map(|p| p[1] + 0.46)
                    .fold(f32::NEG_INFINITY, f32::max);
                let bottom = vertices
                    .iter()
                    .map(|p| p[1] + 0.46)
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    top < 1.535,
                    "body {body} must fit below the ready imported cabin roof, top={top}"
                );
                assert!(
                    bottom > 0.18,
                    "body {body} must clear the declared cabin floor, bottom={bottom}"
                );
                assert!(
                    vertices.iter().all(|p| p[1] >= -0.70
                        && p[1] < 1.1
                        && p[0].abs() < 0.6
                        && p[2].abs() < 1.1)
                );
                let VertexAttributeValues::Float32x4(colors) =
                    mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
                else {
                    panic!()
                };
                let profile = CharacterProfile {
                    body_style: body,
                    skin: palette,
                    hair: palette,
                    outfit: palette,
                    ..Default::default()
                };
                let (skin, hair, outfit) = crate::character::appearance_colors(&profile);
                for c in [skin, hair, outfit] {
                    let linear = Color::srgb(c[0], c[1], c[2]).to_linear().to_f32_array();
                    assert!(
                        colors.contains(&linear),
                        "all palette roles must retain exact sRGB-to-linear world colors"
                    );
                }
            }
        }
    }
}
