//! The lighting grade of a stage (design-vfx-lighting §3, W5-01): one static
//! over-rect across the whole canvas, painted after the world pass and under
//! the vignette and every cue, so a night or storm stage reads darker while
//! its warnings keep their full contrast.
//!
//! Presentation only. Storm stages come from a presentation table
//! ([`STORM_THEME_IDS`], ruling W-8): no content field, no theme reshuffle.
//! Nothing here reads the core RNG.
use bevy_egui::egui::{Color32 as C, Painter, Rect, Shape};
use horde_core::{sim::Attempt, world::StageTheme, world::world};

/// The grade classes, lightest to darkest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lighting {
    Neutral,
    Dawn,
    Sunset,
    Overcast,
    Dusk,
    Storm,
    Night,
}

#[cfg(test)]
impl Lighting {
    pub const ALL: [Self; 7] = [
        Self::Neutral,
        Self::Dawn,
        Self::Sunset,
        Self::Overcast,
        Self::Dusk,
        Self::Storm,
        Self::Night,
    ];
}

/// One grade: the over-rect's tint and alpha, and how strongly additive
/// light pools show under it (wave 5d reads `light_gain`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    pub tint: [u8; 3],
    pub alpha: f32,
    pub light_gain: f32,
}

/// The grade table (design-vfx-lighting §3).
pub fn grade(l: Lighting) -> Grade {
    let (tint, alpha, light_gain) = match l {
        Lighting::Neutral => ([0, 0, 0], 0.00, 0.55),
        Lighting::Dawn => ([255, 196, 150], 0.05, 0.55),
        Lighting::Sunset => ([236, 142, 84], 0.10, 0.65),
        Lighting::Overcast => ([96, 106, 116], 0.12, 0.70),
        Lighting::Dusk => ([54, 48, 92], 0.20, 0.85),
        Lighting::Storm => ([22, 30, 44], 0.26, 0.90),
        Lighting::Night => ([8, 14, 34], 0.34, 1.00),
    };
    Grade {
        tint,
        alpha,
        light_gain,
    }
}

/// Stage themes that are storm stages whatever weather they author (ruling
/// W-8): depot only, 131 of the 1026 stages, so storms stay rare.
pub const STORM_THEME_IDS: &[&str] = &["depot"];

/// A storm stage: a theme whose weather is "storm" (any case; no shipped
/// theme authors it yet) or whose id is in [`STORM_THEME_IDS`].
pub fn is_storm(theme: &StageTheme) -> bool {
    theme.weather.eq_ignore_ascii_case("storm") || STORM_THEME_IDS.contains(&theme.id.as_str())
}

/// The grade class of a stage theme: storm first, then its authored
/// lighting, else neutral.
pub fn lighting_for(theme: &StageTheme) -> Lighting {
    if is_storm(theme) {
        return Lighting::Storm;
    }
    let is = |name: &str| theme.lighting.eq_ignore_ascii_case(name);
    if is("dawn") {
        Lighting::Dawn
    } else if is("sunset") {
        Lighting::Sunset
    } else if is("overcast") {
        Lighting::Overcast
    } else if is("dusk") {
        Lighting::Dusk
    } else if is("storm") {
        Lighting::Storm
    } else if is("night") {
        Lighting::Night
    } else {
        Lighting::Neutral
    }
}

/// How one battlefield is dressed and lit: the backdrop and terrain theme of
/// its biome, its grade class, whether it is a storm stage, and the stage's
/// presentation seed (`StageTheme::variation`, never the core RNG).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneLook {
    pub theme_index: usize,
    pub lighting: Lighting,
    pub storm: bool,
    pub seed: u64,
}

impl SceneLook {
    /// A neutral, storm-free look on backdrop theme `theme_index`: legacy
    /// level battles, which have no country and so no stage theme.
    pub const fn neutral(theme_index: usize) -> Self {
        Self {
            theme_index,
            lighting: Lighting::Neutral,
            storm: false,
            seed: 0,
        }
    }

    /// The look of a stage on backdrop theme `theme_index` with stage theme
    /// `setting` (`None` for a legacy level battle).
    pub fn of(theme_index: usize, setting: Option<&StageTheme>) -> Self {
        setting.map_or(Self::neutral(theme_index), |theme| Self {
            theme_index,
            lighting: lighting_for(theme),
            storm: is_storm(theme),
            seed: theme.variation,
        })
    }
}

/// The look of attempt `a`, for any view of it (the main battle and the
/// operations observer): a country stage takes its biome's backdrop theme
/// and its stage theme's grade; a legacy level battle keeps its level's
/// backdrop under a neutral grade.
pub fn scene_look(a: &Attempt) -> SceneLook {
    match world().country(a.country_id.as_deref().unwrap_or("")) {
        Some(country) => SceneLook::of(
            crate::battle_view::biome_theme(&country.biome),
            Some(&country.stage_theme(a.stage)),
        ),
        None => SceneLook::neutral(a.level),
    }
}

/// The grade and sky-flash intensity one frame paints for `look`. `pulse` is
/// the lightning pulse the caller asks for (0..=1; W5-02 feeds it). Reduced
/// effects and storm-free stages never flash: a storm stage in reduced
/// effects shows its static Storm grade only (rulings RL, W-8).
pub fn scene_grade(look: SceneLook, reduced: bool, pulse: f32) -> (Grade, f32) {
    let sky = if look.storm && !reduced {
        pulse.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (grade(look.lighting), sky)
}

/// Peak alpha and colour of the sky-flash over-rect (design §4).
pub const SKY_PEAK_ALPHA: f32 = 0.14;
pub const SKY_RGB: [u8; 3] = [200, 214, 235];

fn over(rgb: [u8; 3], alpha: f32) -> C {
    C::from_rgba_unmultiplied(
        rgb[0],
        rgb[1],
        rgb[2],
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// The grade pass's shapes over `canvas`: the grade over-rect (none for a
/// zero-alpha grade), then the sky flash, only while `sky` > 0.
pub fn grade_shapes(canvas: Rect, g: Grade, sky: f32) -> Vec<Shape> {
    let mut shapes = Vec::with_capacity(2);
    if g.alpha > 0.0 {
        shapes.push(Shape::rect_filled(canvas, 0.0, over(g.tint, g.alpha)));
    }
    if sky > 0.0 {
        shapes.push(Shape::rect_filled(
            canvas,
            0.0,
            over(SKY_RGB, SKY_PEAK_ALPHA * sky),
        ));
    }
    shapes
}

/// Paints the grade pass over `canvas` with `p`, a painter clipped to the
/// canvas (the forest backdrop is graded with the field).
pub fn paint_grade(p: &Painter, canvas: Rect, g: Grade, sky: f32) {
    for shape in grade_shapes(canvas, g, sky) {
        p.add(shape);
    }
}

/// WCAG 2.x relative luminance of an sRGB colour (design §12): the model
/// the luminance and contrast gates are tested with.
#[cfg(test)]
pub fn relative_luminance(rgb: [u8; 3]) -> f32 {
    let [r, g, b] = rgb.map(srgb_decode);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG contrast ratio of two relative luminances.
#[cfg(test)]
pub fn contrast(a: f32, b: f32) -> f32 {
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
fn srgb_decode(v: u8) -> f32 {
    let v = f32::from(v) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::{pos2, vec2};

    /// How an over-rect is composited: on the encoded sRGB values or on the
    /// decoded linear ones. The blend space of bevy_egui is unverified
    /// (design §3), so every luminance gate holds under both.
    #[derive(Clone, Copy, Debug)]
    enum Blend {
        Gamma,
        Linear,
    }

    /// Relative luminance of `ground` under grade `g`.
    fn graded_luminance(ground: [u8; 3], g: Grade, blend: Blend) -> f32 {
        match blend {
            Blend::Gamma => {
                let mixed = [0, 1, 2].map(|n| {
                    (f32::from(g.tint[n]) * g.alpha + f32::from(ground[n]) * (1.0 - g.alpha))
                        .round() as u8
                });
                relative_luminance(mixed)
            }
            Blend::Linear => {
                relative_luminance(g.tint) * g.alpha + relative_luminance(ground) * (1.0 - g.alpha)
            }
        }
    }

    /// The ground colours the gate runs over: the soil of each terrain
    /// family (`ground_material::soil`) and a grey at each measured
    /// composite mean and p90 luminance (design §12: temperate 0.028/0.033,
    /// boreal 0.053/0.062, arid 0.077/0.097, tropical 0.037/0.049).
    fn grounds() -> Vec<[u8; 3]> {
        let mut grounds = vec![[22, 38, 27], [76, 53, 31], [36, 49, 55], [15, 41, 20]];
        for luminance in [0.028, 0.033, 0.053, 0.062, 0.077, 0.097, 0.037, 0.049_f32] {
            let grey = (0u8..=255)
                .min_by(|a, b| {
                    let d = |v: u8| (relative_luminance([v; 3]) - luminance).abs();
                    d(*a).total_cmp(&d(*b))
                })
                .unwrap();
            grounds.push([grey; 3]);
        }
        grounds
    }

    /// G7.2: a night stage's ground is at most 0.70 of the same ground at
    /// dawn, under gamma-space and linear-space blending alike.
    #[test]
    fn night_grade_at_most_0_70_of_dawn_under_gamma_and_linear_blending() {
        for ground in grounds() {
            for blend in [Blend::Gamma, Blend::Linear] {
                let night = graded_luminance(ground, grade(Lighting::Night), blend);
                let dawn = graded_luminance(ground, grade(Lighting::Dawn), blend);
                assert!(
                    night <= 0.70 * dawn,
                    "{ground:?} {blend:?}: night {night:.4} vs dawn {dawn:.4} (ratio {:.3})",
                    night / dawn
                );
            }
        }
    }

    /// The grade table darkens in its declared order on every ground, and
    /// the neutral grade paints nothing.
    #[test]
    fn grades_darken_from_dawn_to_night_and_neutral_paints_nothing() {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
        assert!(grade_shapes(canvas, grade(Lighting::Neutral), 0.0).is_empty());
        for ground in grounds() {
            for blend in [Blend::Gamma, Blend::Linear] {
                let order = [
                    Lighting::Dawn,
                    Lighting::Sunset,
                    Lighting::Overcast,
                    Lighting::Dusk,
                    Lighting::Storm,
                    Lighting::Night,
                ]
                .map(|l| graded_luminance(ground, grade(l), blend));
                // Dusk, storm and night each read darker than the ungraded
                // ground; the three light grades stay above night.
                let bare = relative_luminance(ground);
                assert!(order[3] < bare + 0.02 && order[5] < bare, "{ground:?}");
                assert!(
                    order[4] > order[5],
                    "{ground:?} {blend:?}: storm over night"
                );
                assert!(order[..3].iter().all(|l| *l > order[5]), "{ground:?}");
            }
        }
        let gains = Lighting::ALL.map(|l| grade(l).light_gain);
        assert!(gains.windows(2).all(|pair| pair[0] <= pair[1]), "{gains:?}");
    }

    /// The ten stage theme ids of the shipped world, in order.
    const STAGE_THEME_IDS: [&str; 10] = [
        "roadside",
        "suburb",
        "depot",
        "industrial",
        "checkpoint",
        "farmland",
        "railway",
        "waterfront",
        "market",
        "highway",
    ];

    /// `ids` are exactly the ten pinned theme ids in their order.
    fn theme_ids_are_pinned<'a>(ids: impl IntoIterator<Item = &'a str>) -> bool {
        ids.into_iter().eq(STAGE_THEME_IDS)
    }

    /// Pin (ISO-3): the stage theme table is the ten ids in order. The storm
    /// table and the theme rule `(stage + 5 * bit) % 10` depend on it; a
    /// regenerated world.json that drops or reorders themes fails here.
    #[test]
    fn stage_theme_ids_are_pinned_to_the_10_in_order() {
        let ids: Vec<&str> = world().stage_themes.iter().map(|t| t.id.as_str()).collect();
        assert!(theme_ids_are_pinned(ids.iter().copied()), "{ids:?}");
        for id in STORM_THEME_IDS {
            assert!(STAGE_THEME_IDS.contains(id), "storm id {id} is a theme");
        }
    }

    /// Self-test of the pin: a reordered, a shortened, a lengthened and a
    /// renamed table each fail it.
    #[test]
    fn stage_theme_pin_rejects_reordered_dropped_added_and_renamed_ids() {
        let mut swapped = STAGE_THEME_IDS;
        swapped.swap(2, 6);
        assert!(!theme_ids_are_pinned(swapped));
        assert!(!theme_ids_are_pinned(STAGE_THEME_IDS[..9].iter().copied()));
        assert!(!theme_ids_are_pinned(
            STAGE_THEME_IDS.into_iter().chain(["storm_front"])
        ));
        let mut renamed = STAGE_THEME_IDS;
        renamed[2] = "Depot";
        assert!(!theme_ids_are_pinned(renamed));
        assert!(theme_ids_are_pinned(STAGE_THEME_IDS));
    }

    fn theme(id: &str, weather: &str, lighting: &str) -> StageTheme {
        StageTheme {
            id: id.into(),
            weather: weather.into(),
            lighting: lighting.into(),
            variation: 7,
            ..StageTheme::default()
        }
    }

    /// Storm tagging is the presentation table plus an authored "storm"
    /// weather; railway (drizzle) keeps its Overcast grade.
    #[test]
    fn storm_is_the_depot_table_or_an_authored_storm_weather() {
        assert!(is_storm(&theme("depot", "light_rain", "overcast")));
        assert!(is_storm(&theme("market", "Storm", "sunset")));
        assert!(!is_storm(&theme("railway", "drizzle", "overcast")));
        assert_eq!(
            lighting_for(&theme("depot", "light_rain", "overcast")),
            Lighting::Storm
        );
        assert_eq!(
            lighting_for(&theme("railway", "drizzle", "overcast")),
            Lighting::Overcast
        );
        assert_eq!(
            lighting_for(&theme("checkpoint", "clear", "Night")),
            Lighting::Night
        );
        assert_eq!(
            lighting_for(&theme("x", "clear", "noon")),
            Lighting::Neutral
        );
        // Every shipped theme's lighting has a grade class of its own name.
        for t in &world().stage_themes {
            let look = lighting_for(&theme(&t.id, &t.weather, &t.lighting));
            assert_ne!(look, Lighting::Neutral, "{} / {}", t.id, t.lighting);
        }
    }

    /// Measured coverage (design §3 "Storm tagging"): depot is 131 of the
    /// 1026 stages, in 120 countries.
    #[test]
    fn storm_stage_coverage_is_131_of_1026() {
        let (mut stages, mut storms, mut countries) = (0, 0, 0);
        for country in &world().countries {
            let count = country.level_count();
            let here = (0..count)
                .filter(|&stage| is_storm(&country.stage_theme(stage)))
                .count();
            stages += count;
            storms += here;
            countries += usize::from(here > 0);
        }
        assert_eq!((storms, stages, countries), (131, 1026, 120));
    }

    /// Rulings RL and W-8: in reduced effects a storm stage paints its
    /// static Storm grade and no sky flash, whatever pulse is asked for; in
    /// normal effects the pulse adds the second rect; a storm-free stage
    /// never flashes.
    #[test]
    fn reduced_effects_storm_stage_draws_static_storm_grade_and_no_flash() {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
        let depot = theme("depot", "light_rain", "overcast");
        let look = SceneLook::of(2, Some(&depot));
        assert!(look.storm);
        assert_eq!((look.lighting, look.seed), (Lighting::Storm, 7));
        let storm = grade(Lighting::Storm);
        let fill = |shape: &Shape| match shape {
            Shape::Rect(rect) => (rect.rect, rect.fill),
            other => panic!("the grade pass paints rects only: {other:?}"),
        };
        for pulse in [0.0, 0.4, 1.0] {
            let (g, sky) = scene_grade(look, true, pulse);
            assert_eq!((g, sky), (storm, 0.0), "reduced, pulse {pulse}");
            let shapes = grade_shapes(canvas, g, sky);
            assert_eq!(shapes.len(), 1, "reduced, pulse {pulse}: one static rect");
            assert_eq!(
                fill(&shapes[0]),
                (canvas, C::from_rgba_unmultiplied(22, 30, 44, 66))
            );
        }
        let (g, sky) = scene_grade(look, false, 1.0);
        let shapes = grade_shapes(canvas, g, sky);
        assert_eq!(shapes.len(), 2, "normal effects flash over the grade");
        assert_eq!(
            fill(&shapes[1]),
            (canvas, C::from_rgba_unmultiplied(200, 214, 235, 36))
        );
        let calm = SceneLook::of(1, Some(&theme("suburb", "mist", "dawn")));
        assert_eq!(scene_grade(calm, false, 1.0).1, 0.0);
        assert_eq!(scene_grade(SceneLook::neutral(0), false, 1.0).1, 0.0);
    }

    /// The operations observer dressed a worker's battle by `a.level`; every
    /// view takes the country's biome theme and the stage's grade.
    #[test]
    fn worker_view_uses_biome_theme_not_level() {
        use horde_core::sim::Game;
        let countries = &world().countries;
        let country = countries
            .iter()
            .find(|c| crate::battle_view::biome_theme(&c.biome) == 2)
            .expect("an arid country");
        let mut game = Game::default();
        game.campaign_mut().attempts = 1;
        game.deploy(0, horde_core::content::Mode::Personal).unwrap();
        let mut a = game.attempt().unwrap().clone();
        a.country_id = Some(country.id.clone());
        a.level = 0;
        a.stage = (0..country.level_count())
            .find(|&stage| country.stage_theme(stage).id == "checkpoint")
            .unwrap_or(0);
        let look = scene_look(&a);
        let stage = country.stage_theme(a.stage);
        assert_eq!(look.theme_index, 2, "the biome theme, not level 0");
        assert_eq!(look, SceneLook::of(2, Some(&stage)));
        assert_eq!(look.seed, stage.variation);
        a.country_id = None;
        a.level = 3;
        assert_eq!(scene_look(&a), SceneLook::neutral(3));
        // The observer paints through `scene_look`, never `a.level`.
        let observer: String = include_str!("../ui/operations.rs")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(observer.contains("art::scene_look(a)"));
        assert!(!observer.contains("theme:a.level") && !observer.contains("projection,a.level"));
    }
}
