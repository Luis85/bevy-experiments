//! Weapon effect profiles (W5-03, design-vfx-lighting section 5): one
//! [`WeaponFx`] per weapon class plus the turret, and [`paint_shot`], the one
//! painter of a Shot event's muzzle flash and tracer. The class is the one the
//! attempt deployed with ([`shooter_class`], the gear snapshot). Presentation
//! only: nothing here reads the core RNG or changes the attempt.
use crate::battle_view::Volley;
use bevy_egui::egui::{self, Color32 as C, Painter, Pos2, Stroke, vec2};
use horde_core::{content::Slot, sim::Attempt, weapons::WeaponClass};

/// What fired a Shot event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxSource {
    Weapon(WeaponClass),
    Turret,
}

impl FxSource {
    /// The six weapon classes, then the turret (the W5-09 gallery order).
    #[cfg(test)]
    pub const ALL: [Self; 7] = [
        Self::Weapon(WeaponClass::Pistol),
        Self::Weapon(WeaponClass::Smg),
        Self::Weapon(WeaponClass::Shotgun),
        Self::Weapon(WeaponClass::Rifle),
        Self::Weapon(WeaponClass::Marksman),
        Self::Weapon(WeaponClass::MachineGun),
        Self::Turret,
    ];
}

/// One source's shot look. Flash length and width are in lane heights, lives
/// in 60 Hz ticks, widths in points at the reference zoom ([`width_scale`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponFx {
    pub flash_len: f32,
    pub flash_width: f32,
    /// Corners of the flash polygon (4, 5 or 6).
    pub flash_points: u8,
    pub flash_life_ticks: u8,
    pub flash_rgb: [u8; 3],
    pub tracer_width: f32,
    pub tracer_life_ticks: u8,
    pub tracer_rgb: [u8; 3],
    /// A bright core line down the tracer.
    pub tracer_core_rgb: Option<[u8; 3]>,
    pub tracer_core_width: f32,
    /// The full tracer is drawn on volleys whose ordinal is a multiple of
    /// this; the others draw the dim tracer at half alpha.
    pub tracer_every: u8,
    pub dim_tracer_width: f32,
    pub dim_tracer_rgb: [u8; 3],
    /// A pellet weapon: every pellet draws a streak this fraction of its
    /// path, and the volley shares one flash.
    pub pellet_streak: Option<f32>,
    /// Flash size on odd volley ordinals.
    pub odd_volley_scale: f32,
    /// The muzzle light pool, in lanes (W5-04 reads it).
    #[allow(dead_code)]
    pub light_radius: f32,
    #[allow(dead_code)]
    pub light_intensity: f32,
    /// Suffix of the `rook.` / `june.` attachment skin that gives the class
    /// its silhouette.
    #[allow(dead_code)]
    pub weapon_skin: &'static str,
}

const AMBER_RGB: [u8; 3] = [239, 181, 82];

const BASE: WeaponFx = WeaponFx {
    flash_len: 0.26,
    flash_width: 0.30,
    flash_points: 4,
    flash_life_ticks: 4,
    flash_rgb: [248, 207, 117],
    tracer_width: 1.4,
    tracer_life_ticks: 7,
    tracer_rgb: AMBER_RGB,
    tracer_core_rgb: None,
    tracer_core_width: 0.0,
    tracer_every: 1,
    dim_tracer_width: 0.0,
    dim_tracer_rgb: AMBER_RGB,
    pellet_streak: None,
    odd_volley_scale: 1.0,
    light_radius: 1.0,
    light_intensity: 0.4,
    weapon_skin: "weapon.rifle",
};

pub const PISTOL_FX: WeaponFx = WeaponFx {
    flash_len: 0.18,
    tracer_width: 1.0,
    tracer_life_ticks: 5,
    light_radius: 0.9,
    light_intensity: 0.35,
    weapon_skin: "weapon.pistol",
    ..BASE
};
pub const SMG_FX: WeaponFx = WeaponFx {
    flash_len: 0.15,
    flash_width: 0.24,
    flash_points: 5,
    flash_life_ticks: 3,
    flash_rgb: [250, 226, 150],
    tracer_width: 0.8,
    tracer_life_ticks: 5,
    tracer_rgb: [246, 222, 140],
    odd_volley_scale: 0.8,
    light_radius: 0.7,
    light_intensity: 0.25,
    weapon_skin: "weapon.smg",
    ..BASE
};
pub const SHOTGUN_FX: WeaponFx = WeaponFx {
    flash_len: 0.34,
    flash_width: 0.60,
    flash_points: 6,
    flash_life_ticks: 5,
    flash_rgb: [255, 196, 110],
    tracer_width: 0.9,
    tracer_life_ticks: 5,
    tracer_rgb: [236, 170, 96],
    pellet_streak: Some(0.35),
    light_radius: 1.2,
    light_intensity: 0.5,
    weapon_skin: "weapon.shotgun",
    ..BASE
};
pub const RIFLE_FX: WeaponFx = BASE;
pub const MARKSMAN_FX: WeaponFx = WeaponFx {
    flash_len: 0.34,
    flash_width: 0.18,
    flash_life_ticks: 6,
    flash_rgb: [255, 236, 200],
    tracer_width: 2.0,
    tracer_life_ticks: 12,
    tracer_rgb: [255, 214, 150],
    tracer_core_rgb: Some([255, 252, 240]),
    tracer_core_width: 0.8,
    light_radius: 1.3,
    light_intensity: 0.55,
    weapon_skin: "weapon.marksman",
    ..BASE
};
pub const MACHINE_GUN_FX: WeaponFx = WeaponFx {
    flash_len: 0.24,
    flash_width: 0.28,
    flash_points: 6,
    flash_life_ticks: 3,
    flash_rgb: [255, 186, 96],
    tracer_width: 1.6,
    tracer_life_ticks: 7,
    tracer_rgb: [255, 120, 60],
    tracer_every: 3,
    dim_tracer_width: 0.7,
    dim_tracer_rgb: [220, 170, 110],
    weapon_skin: "weapon.machinegun",
    ..BASE
};
pub const TURRET_FX: WeaponFx = WeaponFx {
    flash_len: 0.14,
    flash_width: 0.20,
    flash_life_ticks: 3,
    flash_rgb: [200, 230, 240],
    tracer_width: 1.1,
    tracer_life_ticks: 6,
    tracer_rgb: [180, 220, 235],
    light_radius: 0.6,
    light_intensity: 0.2,
    weapon_skin: "turret",
    ..BASE
};

/// The shot look of `source`.
pub fn weapon_fx(source: FxSource) -> &'static WeaponFx {
    match source {
        FxSource::Weapon(WeaponClass::Pistol) => &PISTOL_FX,
        FxSource::Weapon(WeaponClass::Smg) => &SMG_FX,
        FxSource::Weapon(WeaponClass::Shotgun) => &SHOTGUN_FX,
        FxSource::Weapon(WeaponClass::Rifle) => &RIFLE_FX,
        FxSource::Weapon(WeaponClass::Marksman) => &MARKSMAN_FX,
        FxSource::Weapon(WeaponClass::MachineGun) => &MACHINE_GUN_FX,
        FxSource::Turret => &TURRET_FX,
    }
}

/// The weapon class the attempt's shooter fires: the Ranged record of the
/// gear snapshot taken at deploy, else the profile's weapon (an attempt saved
/// before the snapshot carried a class).
pub fn shooter_class(a: &Attempt) -> WeaponClass {
    a.gear_snapshot
        .iter()
        .find(|record| record.slot == Slot::Ranged)
        .and_then(|record| record.weapon_class)
        .unwrap_or(a.profile.weapon.class)
}

/// The lane height the stroke widths are authored at, in points.
const REFERENCE_LANE: f32 = 62.9;

/// Stroke widths grow with the zoom: by `lane / 62.9`, within 1 to 1.6.
pub(crate) fn width_scale(lane: f32) -> f32 {
    (lane / REFERENCE_LANE).clamp(1.0, 1.6)
}

/// The longest any profile draws a shot, in seconds: the Shot row of
/// `fx_lifetime` must cover it.
#[cfg(test)]
pub(crate) fn longest_shot_seconds() -> f32 {
    FxSource::ALL
        .into_iter()
        .map(|source| {
            let fx = weapon_fx(source);
            fx.flash_life_ticks.max(fx.tracer_life_ticks)
        })
        .max()
        .map_or(0.0, |ticks| f32::from(ticks) / 60.0)
}

/// One Shot event to paint.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shot {
    /// The drawn muzzle.
    pub from: Pos2,
    pub to: Pos2,
    /// Lane height in points.
    pub lane: f32,
    /// Age in 60 Hz ticks.
    pub age_ticks: u32,
    pub volley: Volley,
    pub reduced: bool,
}

/// The muzzle flash outline: `points` corners (4 a kite, 5 a blunt wedge, 6
/// a long spike), `length` along `direction` and `width` across, convex.
fn flash_polygon(
    from: Pos2,
    direction: egui::Vec2,
    length: f32,
    width: f32,
    points: u8,
) -> Vec<Pos2> {
    let side = vec2(-direction.y, direction.x) * (width * 0.5);
    let at = |along: f32, across: f32| from + direction * (length * along) + side * across;
    let back = from - direction;
    match points {
        5 => vec![
            back,
            at(0.3, 1.0),
            at(1.0, 0.4),
            at(1.0, -0.4),
            at(0.3, -1.0),
        ],
        6 => vec![
            back,
            at(0.3, 1.0),
            at(0.65, 0.6),
            at(1.0, 0.0),
            at(0.65, -0.6),
            at(0.3, -1.0),
        ],
        _ => vec![back, at(0.3, 1.0), at(1.0, 0.0), at(0.3, -1.0)],
    }
}

fn rgba([r, g, b]: [u8; 3], alpha: f32) -> C {
    C::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Paints one Shot event with the profile `fx`.
///
/// Normal effects: the volley's first event draws the flash (so a nine-pellet
/// shell flashes once); every event draws its tracer, a pellet weapon a short
/// streak instead, and a `tracer_every` weapon its full tracer only on every
/// n-th volley. Reduced effects: no flash, and one static 1-pt line per
/// volley in the tracer colour for the tracer's life.
pub(crate) fn paint_shot(p: &Painter, fx: &WeaponFx, shot: Shot) {
    let Shot {
        from,
        to,
        lane,
        age_ticks,
        volley,
        reduced,
    } = shot;
    let tracer_life = u32::from(fx.tracer_life_ticks);
    if reduced {
        if volley.first && age_ticks < tracer_life {
            p.line_segment([from, to], Stroke::new(1.0, rgba(fx.tracer_rgb, 1.0)));
        }
        return;
    }
    let scale = width_scale(lane);
    let path = to - from;
    let flash_life = u32::from(fx.flash_life_ticks);
    if volley.first && age_ticks < flash_life && path.length_sq() > 0.0 {
        let left = 1.0 - age_ticks as f32 / flash_life as f32;
        let size = if volley.ordinal % 2 == 1 {
            fx.odd_volley_scale
        } else {
            1.0
        };
        p.add(egui::Shape::convex_polygon(
            flash_polygon(
                from,
                path.normalized(),
                lane * fx.flash_len * size * (0.5 + 0.5 * left),
                lane * fx.flash_width * size,
                fx.flash_points,
            ),
            rgba(fx.flash_rgb, left * 230.0 / 255.0),
            Stroke::NONE,
        ));
    }
    if age_ticks >= tracer_life {
        return;
    }
    let fade = 1.0 - age_ticks as f32 / tracer_life as f32;
    if let Some(fraction) = fx.pellet_streak {
        p.line_segment(
            [from, from + path * fraction],
            Stroke::new(fx.tracer_width * scale, rgba(fx.tracer_rgb, fade)),
        );
        return;
    }
    if volley.ordinal % u32::from(fx.tracer_every.max(1)) != 0 {
        p.line_segment(
            [from, to],
            Stroke::new(
                fx.dim_tracer_width * scale,
                rgba(fx.dim_tracer_rgb, fade * 0.5),
            ),
        );
        return;
    }
    p.line_segment(
        [from, to],
        Stroke::new(fx.tracer_width * scale, rgba(fx.tracer_rgb, fade)),
    );
    if let Some(core) = fx.tracer_core_rgb {
        p.line_segment(
            [from, to],
            Stroke::new(fx.tracer_core_width * scale, rgba(core, fade)),
        );
    }
}

#[cfg(test)]
mod tests;
