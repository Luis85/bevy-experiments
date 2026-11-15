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

/// The lightning schedule (design §4 as amended by rulings RL and W-8), in
/// wall-clock milliseconds: one slot per 8 s, a pulse in 60% of the slots,
/// starting in the slot's first 4 s, so two pulse starts are always more
/// than 4 s of wall time apart, whatever the observation speed.
pub const SKY_SLOT_MS: u64 = 8000;
pub const SKY_FIRE_PERCENT: u64 = 60;
pub const SKY_OFFSET_RANGE_MS: u64 = 4000;
/// A pulse rises linearly over `SKY_RISE_MS` and decays over `SKY_DECAY_MS`.
pub const SKY_RISE_MS: u64 = 33;
pub const SKY_DECAY_MS: u64 = 233;
/// No flash within this much wall time after a BossWarning or Boss event.
pub const SKY_QUIET_BOSS_MS: u64 = 1000;
/// No flash within this much wall time after a Windup, Contact or Damage
/// event.
pub const SKY_QUIET_CUE_MS: u64 = 750;
/// Domain tag of the schedule hash ("SKYFLASH").
const SKY_TAG: u64 = 0x534B_5946_4C41_5348;

/// The lightning clock of one frame: unscaled real time accumulated over
/// unpaused frames, the observation speed (it turns event ages in ticks
/// into wall time for the quiet windows), and whether this frame's clock is
/// frozen. Presentation state only.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkyClock {
    pub wall_ms: f64,
    pub speed: u32,
    /// The world clock is paused (or no battle runs), so the clock did not
    /// advance this frame: no flash in any view, even of an expedition whose
    /// own attempt is not paused (`Game::set_paused` pauses only the root
    /// attempt; ruling W-8).
    pub frozen: bool,
}

impl SkyClock {
    /// Adds one frame of real time; a frozen (paused) frame adds nothing.
    pub fn advance(&mut self, dt_ms: f64, frozen: bool) {
        if !frozen && dt_ms.is_finite() && dt_ms > 0.0 {
            self.wall_ms += dt_ms;
        }
    }

    /// The clock a fixed-step export or automated screenshot draws: the
    /// attempt's own tick at 60 Hz, so its frames do not depend on the wall
    /// clock.
    pub fn of_tick(tick: u64, speed: u32) -> Self {
        Self {
            wall_ms: tick as f64 * 1000.0 / 60.0,
            speed,
            frozen: false,
        }
    }
}

/// Wall time since the latest boss cue and the latest attack cue, each
/// `None` when it is outside its quiet window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkyQuiet {
    pub boss_ms: Option<u64>,
    pub cue_ms: Option<u64>,
}

/// The quiet windows of `a` at observation speed `speed`: an event `n`
/// ticks old is `n * 1000 / (60 * speed)` ms of wall time old.
pub fn sky_quiet(a: &Attempt, speed: u32) -> SkyQuiet {
    use horde_core::sim::EventKind as K;
    let per_second = 60 * u64::from(speed.max(1));
    let mut quiet = SkyQuiet::default();
    // Events are in ascending tick order: stop at the first one older than
    // the longer window.
    for event in a.events.iter().rev() {
        let age = a.tick.saturating_sub(event.tick) * 1000 / per_second;
        if age >= SKY_QUIET_BOSS_MS.max(SKY_QUIET_CUE_MS) {
            break;
        }
        match event.kind {
            K::BossWarning | K::Boss if quiet.boss_ms.is_none() => quiet.boss_ms = Some(age),
            K::Windup | K::Contact | K::Damage
                if quiet.cue_ms.is_none() && age < SKY_QUIET_CUE_MS =>
            {
                quiet.cue_ms = Some(age);
            }
            _ => {}
        }
    }
    quiet
}

/// When the pulse of the slot holding `wall_ms` starts, if that slot fires.
pub fn sky_pulse_start(seed: u64, wall_ms: u64) -> Option<u64> {
    let slot = wall_ms / SKY_SLOT_MS;
    let h = splitmix(seed ^ slot ^ SKY_TAG);
    (h % 100 < SKY_FIRE_PERCENT).then(|| slot * SKY_SLOT_MS + (h >> 8) % SKY_OFFSET_RANGE_MS)
}

/// The start of the first pulse that starts at or after `wall_ms` (the storm
/// review scene stages its motion window around it). The schedule fires 60%
/// of its slots, so the search ends within a few of them.
pub fn first_pulse_after(seed: u64, wall_ms: u64) -> u64 {
    (wall_ms / SKY_SLOT_MS..)
        .filter_map(|slot| sky_pulse_start(seed, slot * SKY_SLOT_MS))
        .find(|&start| start >= wall_ms)
        .expect("the schedule fires 60% of its slots")
}

/// SplitMix64's finaliser: the schedule's own hash of the presentation seed
/// and the slot.
fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The lightning intensity (0..=1) at `wall_ms` of the storm clock for a
/// stage with presentation seed `seed`. Zero on a storm-free stage, while
/// paused, in reduced effects (which shows the static Storm grade only) and
/// inside a quiet window, so a flash never competes with a warning.
pub fn sky_flash(
    seed: u64,
    wall_ms: u64,
    quiet: SkyQuiet,
    paused: bool,
    reduced: bool,
    storm: bool,
) -> f32 {
    let quiet = quiet.boss_ms.is_some_and(|ms| ms < SKY_QUIET_BOSS_MS)
        || quiet.cue_ms.is_some_and(|ms| ms < SKY_QUIET_CUE_MS);
    if !storm || paused || reduced || quiet {
        return 0.0;
    }
    let Some(start) = sky_pulse_start(seed, wall_ms) else {
        return 0.0;
    };
    // A pulse (266 ms) ends inside its own slot: it starts in the first 4 s.
    let Some(t) = wall_ms.checked_sub(start) else {
        return 0.0;
    };
    if t <= SKY_RISE_MS {
        t as f32 / SKY_RISE_MS as f32
    } else if t < SKY_RISE_MS + SKY_DECAY_MS {
        1.0 - (t - SKY_RISE_MS) as f32 / SKY_DECAY_MS as f32
    } else {
        0.0
    }
}

/// The pulse one frame of `a` asks [`scene_grade`] for: dark while `a` is
/// paused or the frame's clock is frozen (the world clock's pause leaves an
/// expedition's attempt unpaused).
pub fn sky_pulse(look: SceneLook, clock: SkyClock, a: &Attempt, reduced: bool) -> f32 {
    let paused = a.paused || clock.frozen;
    if !look.storm || reduced || paused {
        return 0.0;
    }
    sky_flash(
        look.seed,
        clock.wall_ms.max(0.0) as u64,
        sky_quiet(a, clock.speed),
        paused,
        reduced,
        look.storm,
    )
}

/// The fill of an over-rect of `rgb` at `alpha` that composites as the
/// straight linear-space blend `alpha * lin(rgb) + (1 - alpha) * ground`
/// (design §3, §12; CF-3-21). bevy_egui decodes a vertex colour from sRGB to
/// linear in its fragment shader and blends it premultiplied on the sRGB
/// target, so the fill is the sRGB encoding of the linear premultiplied tint.
/// An unmultiplied sRGB fill was premultiplied in gamma space instead and so
/// decoded to nearly black: each rect only darkened by its alpha, whatever
/// its tint.
pub(crate) fn over(rgb: [u8; 3], alpha: f32) -> C {
    use bevy_egui::egui::ecolor::{gamma_u8_from_linear_f32, linear_f32_from_gamma_u8};
    let alpha = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    let share = f32::from(alpha) / 255.0;
    let [r, g, b] = rgb.map(|v| gamma_u8_from_linear_f32(share * linear_f32_from_gamma_u8(v)));
    C::from_rgba_premultiplied(r, g, b, alpha)
}

/// The alpha of the sky-flash over-rect at pulse intensity `sky` (0..=1).
pub fn sky_alpha(sky: f32) -> f32 {
    SKY_PEAK_ALPHA * sky.clamp(0.0, 1.0)
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
            over(SKY_RGB, sky_alpha(sky)),
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

    /// How an over-rect is composited: on the encoded sRGB values, on the
    /// decoded linear ones, or as bevy_egui 0.43 really does it (`Egui`).
    /// Every luminance gate holds under all three.
    #[derive(Clone, Copy, Debug)]
    enum Blend {
        Gamma,
        Linear,
        /// The real pipeline (CF-3-21): the painted shape's premultiplied
        /// sRGB vertex colour is decoded to linear in the fragment shader
        /// (`egui.wesl`, `linear_from_gamma_rgb`) and blended premultiplied
        /// (`PREMULTIPLIED_ALPHA_BLENDING`) in linear space on the sRGB
        /// target: `out = lin(colour) + lin(ground) * (1 - alpha)`.
        Egui,
    }

    /// The fill the grade pass paints for `g`: its over-rect's colour.
    fn painted_fill(g: Grade) -> C {
        let canvas = Rect::from_min_size(pos2(0.0, 0.0), vec2(16.0, 16.0));
        match grade_shapes(canvas, g, 0.0).first() {
            Some(Shape::Rect(rect)) => rect.fill,
            None => C::TRANSPARENT,
            Some(other) => panic!("the grade pass paints rects only: {other:?}"),
        }
    }

    /// The linear RGB of `ground` under the painted over-rect of `g`, through
    /// the real egui pipeline ([`Blend::Egui`]).
    fn egui_composite(ground: [u8; 3], g: Grade) -> [f32; 3] {
        egui_over(ground.map(srgb_decode), g)
    }

    /// [`egui_composite`] over a ground given in linear RGB.
    fn egui_over(ground: [f32; 3], g: Grade) -> [f32; 3] {
        let fill = painted_fill(g);
        let alpha = f32::from(fill.a()) / 255.0;
        let colour = [fill.r(), fill.g(), fill.b()].map(srgb_decode);
        [0, 1, 2].map(|n| colour[n] + ground[n] * (1.0 - alpha))
    }

    /// Relative luminance of a linear RGB colour.
    fn linear_luminance([r, g, b]: [f32; 3]) -> f32 {
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Relative luminance of `ground` under grade `g`.
    fn graded_luminance(ground: [u8; 3], g: Grade, blend: Blend) -> f32 {
        match blend {
            Blend::Egui => linear_luminance(egui_composite(ground, g)),
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

    /// CF-3-21: every over-rect the grade pass paints (each grade, and the
    /// sky flash at its peak) composites, through the real egui pipeline,
    /// as the straight linear-space blend of its tint at its alpha (design
    /// §3, §12: the analytic model the captures are checked against), within
    /// 0.002 per linear channel on every ground. Before, the unmultiplied
    /// sRGB fill was premultiplied in gamma space and decoded to linear, so
    /// each rect darkened by its alpha whatever its tint: the Dawn tint
    /// vanished and the night/dawn ratio measured 0.686 against 0.45.
    #[test]
    fn grade_over_rects_composite_as_their_tint_through_the_egui_pipeline() {
        let sky = Grade {
            tint: SKY_RGB,
            alpha: sky_alpha(1.0),
            light_gain: 0.0,
        };
        let rects = Lighting::ALL.map(grade).into_iter().chain([sky]);
        for g in rects.filter(|g| g.alpha > 0.0) {
            let tint = g.tint.map(srgb_decode);
            // The alpha a vertex colour carries: the table's, to 1/255.
            let alpha = (g.alpha * 255.0).round() / 255.0;
            for ground in grounds().into_iter().chain([[0; 3], [255; 3]]) {
                let painted = egui_composite(ground, g);
                let intended =
                    [0, 1, 2].map(|n| tint[n] * alpha + srgb_decode(ground[n]) * (1.0 - alpha));
                for n in 0..3 {
                    assert!(
                        (painted[n] - intended[n]).abs() <= 0.002,
                        "{g:?} over {ground:?}: channel {n} composites to {:.4}, \
                         the linear blend of the tint is {:.4}",
                        painted[n],
                        intended[n]
                    );
                }
            }
        }
    }

    /// G7.2: a night stage's ground is at most 0.70 of the same ground at
    /// dawn, under gamma-space and linear-space blending and through the
    /// real egui pipeline.
    #[test]
    fn night_grade_at_most_0_70_of_dawn_under_gamma_and_linear_blending() {
        for ground in grounds() {
            for blend in [Blend::Gamma, Blend::Linear, Blend::Egui] {
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
            for blend in [Blend::Gamma, Blend::Linear, Blend::Egui] {
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
            assert_eq!(fill(&shapes[0]), (canvas, over([22, 30, 44], 66.0 / 255.0)));
            assert_eq!(fill(&shapes[0]).1.a(), 66, "the 0.26 Storm alpha");
        }
        let (g, sky) = scene_grade(look, false, 1.0);
        let shapes = grade_shapes(canvas, g, sky);
        assert_eq!(shapes.len(), 2, "normal effects flash over the grade");
        assert_eq!(fill(&shapes[1]), (canvas, over(SKY_RGB, 36.0 / 255.0)));
        assert_eq!(fill(&shapes[1]).1.a(), 36, "the 0.14 peak alpha");
        let calm = SceneLook::of(1, Some(&theme("suburb", "mist", "dawn")));
        assert_eq!(scene_grade(calm, false, 1.0).1, 0.0);
        assert_eq!(scene_grade(SceneLook::neutral(0), false, 1.0).1, 0.0);
    }

    const SKY_SEEDS: [u64; 6] = [0, 1, 7, 0x00C0_FFEE, u64::MAX, 0x1234_5678_9ABC_DEF0];

    /// G7.3 (rulings RL, W-8): lightning is paced on the wall clock. At 1x,
    /// 2x and 4x observation speed, with even and uneven frame times, pulse
    /// starts are at least 4,000 ms of wall time apart, the schedule is the
    /// same at every speed, and paused frames do not move it.
    #[test]
    fn sky_flash_starts_at_least_4000_ms_apart_at_observation_speed_4x() {
        // Ten minutes of frames; `frame_ms(n)` is the n-th frame's length.
        let starts = |seed: u64, speed: u32, frame_ms: &dyn Fn(u32) -> f64| {
            let mut clock = SkyClock {
                wall_ms: 0.0,
                speed,
                frozen: false,
            };
            let (mut starts, mut lit, mut ticks, mut frame) = (Vec::new(), false, 0u64, 0u32);
            while clock.wall_ms < 600_000.0 {
                clock.advance(frame_ms(frame), false);
                // A paused frame every 50th: no wall time, no ticks.
                if frame % 50 == 0 {
                    let before = clock;
                    clock.advance(frame_ms(frame), true);
                    assert_eq!(clock, before, "a paused frame is excluded");
                }
                ticks += u64::from(speed);
                frame += 1;
                let now = sky_flash(
                    seed,
                    clock.wall_ms as u64,
                    SkyQuiet::default(),
                    false,
                    false,
                    true,
                );
                assert!((0.0..=1.0).contains(&now));
                if now > 0.0 && !lit {
                    starts.push(clock.wall_ms as u64);
                }
                lit = now > 0.0;
            }
            assert!(ticks >= 14_000 * u64::from(speed), "{ticks} ticks");
            starts
        };
        let even = |_: u32| 1000.0 / 60.0;
        let uneven = |n: u32| [7.0, 33.4, 16.7, 4.2, 41.0][n as usize % 5];
        let mut pulses = 0;
        for seed in SKY_SEEDS {
            let at_1x = starts(seed, 1, &even);
            for speed in [1, 2, 4] {
                for frames in [&even as &dyn Fn(u32) -> f64, &uneven] {
                    let s = starts(seed, speed, frames);
                    assert!(
                        s.len() >= 20,
                        "seed {seed} at {speed}x: {} pulses in 10 minutes",
                        s.len()
                    );
                    for pair in s.windows(2) {
                        assert!(
                            pair[1] - pair[0] >= 4000,
                            "seed {seed} at {speed}x: starts {} and {} ms",
                            pair[0],
                            pair[1]
                        );
                    }
                    pulses += s.len();
                }
                assert_eq!(starts(seed, speed, &even), at_1x, "seed {seed} at {speed}x");
            }
            // The scheduled starts themselves, slot by slot.
            let scheduled: Vec<u64> = (0..600_000 / SKY_SLOT_MS)
                .filter_map(|slot| sky_pulse_start(seed, slot * SKY_SLOT_MS))
                .collect();
            assert!(scheduled.windows(2).all(|p| p[1] - p[0] > 4000));
            for start in scheduled {
                assert!(start % SKY_SLOT_MS < SKY_OFFSET_RANGE_MS);
            }
        }
        // 60% of 75 slots, 6 seeds, 6 runs each: about 1 pulse per 13 s.
        assert!((1200..=2100).contains(&pulses), "{pulses} pulses");
    }

    /// The first wall time at which `seed`'s schedule is at full intensity.
    fn first_peak_ms(seed: u64) -> u64 {
        (0..200_000)
            .find(|&ms| sky_flash(seed, ms, SkyQuiet::default(), false, false, true) >= 1.0)
            .expect("a pulse reaches full intensity within 200 s")
    }

    /// G7.3: the flash at its peak raises the mean luminance of any graded
    /// ground by less than 0.2, under gamma-space and linear-space blending
    /// and through the real egui pipeline, and the pulse itself rises over
    /// 33 ms and decays over 233 ms.
    #[test]
    fn sky_flash_peak_delta_below_0_2_under_gamma_and_linear_blending() {
        let flash = |ms| sky_flash(7, ms, SkyQuiet::default(), false, false, true);
        let peak_ms = first_peak_ms(7);
        let start = sky_pulse_start(7, peak_ms).expect("the peak's slot fires");
        assert_eq!(peak_ms, start + SKY_RISE_MS);
        let mut peak = 0.0_f32;
        for ms in start.saturating_sub(50)..start + 400 {
            let now = flash(ms);
            assert!((0.0..=1.0).contains(&now), "{ms}: {now}");
            peak = peak.max(now);
            let lit = ms > start && ms < start + SKY_RISE_MS + SKY_DECAY_MS;
            assert_eq!(now > 0.0, lit, "{ms} ms, start {start}: {now}");
        }
        assert_eq!(peak, 1.0);
        assert!(flash(start + 10) < flash(start + 20), "rises");
        assert!(flash(start + 100) > flash(start + 200), "decays");
        let sky = Grade {
            tint: SKY_RGB,
            alpha: SKY_PEAK_ALPHA * peak,
            light_gain: 0.0,
        };
        let mut worst = 0.0_f32;
        for ground in grounds().into_iter().chain([[255; 3], [0; 3]]) {
            for l in Lighting::ALL {
                for blend in [Blend::Gamma, Blend::Linear, Blend::Egui] {
                    let g = grade(l);
                    let (dark, lit) = match blend {
                        Blend::Egui => {
                            let graded = egui_composite(ground, g);
                            (
                                linear_luminance(graded),
                                linear_luminance(egui_over(graded, sky)),
                            )
                        }
                        Blend::Gamma => {
                            let graded = [0, 1, 2].map(|n| {
                                (f32::from(g.tint[n]) * g.alpha
                                    + f32::from(ground[n]) * (1.0 - g.alpha))
                                    .round() as u8
                            });
                            (
                                relative_luminance(graded),
                                graded_luminance(graded, sky, blend),
                            )
                        }
                        Blend::Linear => {
                            let dark = graded_luminance(ground, g, blend);
                            (
                                dark,
                                relative_luminance(SKY_RGB) * sky.alpha + dark * (1.0 - sky.alpha),
                            )
                        }
                    };
                    let delta = (lit - dark).abs();
                    assert!(delta < 0.2, "{ground:?} {l:?} {blend:?}: dL {delta:.3}");
                    worst = worst.max(delta);
                }
            }
        }
        // The flash is not invisible either.
        assert!(worst > 0.01, "worst dL {worst:.3}");
    }

    /// A deployed attempt with an empty event window at tick 1000.
    fn calm_attempt() -> Attempt {
        use horde_core::sim::Game;
        let mut game = Game::default();
        game.campaign_mut().attempts = 1;
        game.deploy(0, horde_core::content::Mode::Personal).unwrap();
        let mut a = game.attempt().unwrap().clone();
        a.events.clear();
        a.tick = 1000;
        a
    }

    fn storm_look() -> SceneLook {
        SceneLook {
            storm: true,
            lighting: Lighting::Storm,
            seed: 7,
            ..SceneLook::neutral(0)
        }
    }

    /// Never masks a cue: no flash while paused, in reduced effects, on a
    /// storm-free stage, within 1,000 ms of a boss cue or within 750 ms of a
    /// windup, contact or damage cue, in wall time at the observation speed.
    #[test]
    fn sky_flash_is_quiet_near_boss_and_attack_cues_in_wall_time() {
        let peak = first_peak_ms(7);
        let at = |quiet, paused, reduced, storm| sky_flash(7, peak, quiet, paused, reduced, storm);
        let none = SkyQuiet::default();
        assert_eq!(at(none, false, false, true), 1.0);
        assert_eq!(at(none, true, false, true), 0.0, "paused");
        assert_eq!(at(none, false, true, true), 0.0, "reduced");
        assert_eq!(at(none, false, false, false), 0.0, "no storm");
        let boss = |ms| SkyQuiet {
            boss_ms: Some(ms),
            cue_ms: None,
        };
        let cue = |ms| SkyQuiet {
            boss_ms: None,
            cue_ms: Some(ms),
        };
        assert_eq!(at(boss(0), false, false, true), 0.0);
        assert_eq!(at(boss(SKY_QUIET_BOSS_MS - 1), false, false, true), 0.0);
        assert_eq!(at(boss(SKY_QUIET_BOSS_MS), false, false, true), 1.0);
        assert_eq!(at(cue(SKY_QUIET_CUE_MS - 1), false, false, true), 0.0);
        assert_eq!(at(cue(SKY_QUIET_CUE_MS), false, false, true), 1.0);

        // The windows are wall time: an event 100 ticks old is 1,666 ms old
        // at 1x and 416 ms old at 4x.
        use horde_core::sim::{Event, EventKind};
        let mut a = calm_attempt();
        let blank: Event =
            serde_json::from_str(r#"{"kind":"Shot","from":[0,0],"to":[0,0],"value":0,"tick":0}"#)
                .unwrap();
        for (kind, tick) in [
            (EventKind::BossWarning, 900),
            (EventKind::Windup, 960),
            (EventKind::Shot, 990),
        ] {
            a.events.push_back(Event {
                kind,
                tick,
                ..blank.clone()
            });
        }
        let quiet = |boss_ms, cue_ms| SkyQuiet { boss_ms, cue_ms };
        assert_eq!(sky_quiet(&a, 1), quiet(None, Some(666)));
        assert_eq!(sky_quiet(&a, 2), quiet(Some(833), Some(333)));
        assert_eq!(sky_quiet(&a, 4), quiet(Some(416), Some(166)));
        assert_eq!(sky_quiet(&a, 0), sky_quiet(&a, 1), "speed 0 reads as 1x");
        for kind in [EventKind::Contact, EventKind::Damage, EventKind::Boss] {
            let mut one = calm_attempt();
            one.events.push_back(Event {
                kind,
                tick: 999,
                ..blank.clone()
            });
            assert_ne!(sky_quiet(&one, 1), SkyQuiet::default(), "{kind:?}");
        }
        // The frame's pulse: quiet at 4x, lit once the cues have aged out.
        let look = storm_look();
        let clock = |speed| SkyClock {
            wall_ms: peak as f64,
            speed,
            frozen: false,
        };
        assert_eq!(sky_pulse(look, clock(4), &a, false), 0.0);
        a.tick = 2000;
        assert_eq!(sky_pulse(look, clock(4), &a, false), 1.0);
        assert_eq!(sky_pulse(look, clock(4), &a, true), 0.0, "reduced");
        a.paused = true;
        assert_eq!(sky_pulse(look, clock(4), &a, false), 0.0, "paused");
        a.paused = false;
        let calm = SceneLook {
            storm: false,
            ..look
        };
        assert_eq!(sky_pulse(calm, clock(4), &a, false), 0.0, "no storm");
        // A fixed-step export reads the attempt's tick at 60 Hz.
        assert_eq!(
            SkyClock::of_tick(600, 2),
            SkyClock {
                wall_ms: 10_000.0,
                speed: 2,
                frozen: false,
            }
        );
    }

    /// W-8: the world clock's pause darkens every view. `Game::set_paused`
    /// pauses only the root attempt, so an expedition the Operations observer
    /// draws stays unpaused; the frozen clock alone must keep its frame dark,
    /// or a pulse lit when the clock stopped would hold for the whole pause.
    #[test]
    fn sky_pulse_is_dark_on_a_frozen_clock_even_for_an_unpaused_attempt() {
        let peak = first_peak_ms(7);
        let mut a = calm_attempt();
        a.tick = 2000;
        assert!(
            !a.paused,
            "a worker attempt is not paused by the world clock"
        );
        let live = SkyClock {
            wall_ms: peak as f64,
            speed: 1,
            frozen: false,
        };
        assert_eq!(
            sky_pulse(storm_look(), live, &a, false),
            1.0,
            "lit at the peak"
        );
        let frozen = SkyClock {
            frozen: true,
            ..live
        };
        assert_eq!(
            sky_pulse(storm_look(), frozen, &a, false),
            0.0,
            "frozen clock"
        );
    }

    /// ISO-8: the lightning schedule is keyed by the stage's presentation
    /// seed (`StageTheme::variation`) and the wall clock only. It reads no
    /// core RNG and no keyed roll, and reading an attempt's quiet windows
    /// leaves the attempt unchanged.
    #[test]
    fn sky_flash_never_reads_core_rng() {
        let source = include_str!("lighting.rs").replace('\r', "");
        let production = source
            .split("\n#[cfg(test)]\nmod tests {")
            .next()
            .expect("the production part");
        assert!(production.contains("pub fn sky_flash("));
        assert!(!production.contains("fn sky_flash_never_reads_core_rng"));
        let code: String = production
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for needle in [".rng", "Rng", "hash::", "keyed", "rand::", "random"] {
            assert!(!code.contains(needle), "lighting.rs reads `{needle}`");
        }
        // The schedule is a function of (seed, wall_ms): seeds differ, and
        // the same seed repeats.
        let schedule = |seed: u64| -> Vec<Option<u64>> {
            (0..64)
                .map(|slot| sky_pulse_start(seed, slot * SKY_SLOT_MS))
                .collect()
        };
        for seed in SKY_SEEDS {
            assert_eq!(schedule(seed), schedule(seed));
            let fired = schedule(seed).iter().flatten().count();
            assert!((24..=52).contains(&fired), "seed {seed}: {fired} of 64");
        }
        assert_ne!(schedule(0), schedule(1));
        assert_ne!(schedule(7), schedule(8));
        // Two attempts with different RNG states on the same stage look
        // flash alike, and neither is changed by the read.
        use horde_core::sim::Game;
        let clock = SkyClock {
            wall_ms: first_peak_ms(7) as f64,
            speed: 1,
            frozen: false,
        };
        let mut pulses = Vec::new();
        for ticks in [0, 240] {
            let mut game = Game::default();
            game.campaign_mut().attempts = 1;
            game.deploy(0, horde_core::content::Mode::Personal).unwrap();
            for _ in 0..ticks {
                game.tick(horde_core::sim::Input::default());
            }
            let mut a = game.attempt().unwrap().clone();
            a.events.clear();
            let before = serde_json::to_string(&a).unwrap();
            pulses.push(sky_pulse(storm_look(), clock, &a, false));
            assert_eq!(serde_json::to_string(&a).unwrap(), before);
        }
        assert_eq!(pulses, [1.0, 1.0]);
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
