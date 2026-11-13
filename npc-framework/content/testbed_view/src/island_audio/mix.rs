//! The decisions of the island's sound, as pure functions: which ambience
//! layer plays at what level, which one-shot a pose or machine makes, how
//! many voices may sound, how a fast clock thins them, their pitch, and when
//! music plays. Tested without an audio device.
use crate::anim::Pose;
use testbed_content::village::Role;

/// The ambience layers, cross-faded from what the view shows. A storm is
/// the rain layer, harder, with thunder fired over it at gaps
/// ([`thunder_gap`]): the storm recording is not a clean loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Layer {
    Waves,
    Wind,
    Rustle,
    Rain,
    Birds,
    Crickets,
    Murmur,
    Fire,
}

impl Layer {
    pub(crate) const ALL: [Layer; 8] = [
        Layer::Waves,
        Layer::Wind,
        Layer::Rustle,
        Layer::Rain,
        Layer::Birds,
        Layer::Crickets,
        Layer::Murmur,
        Layer::Fire,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Layer::Waves => "waves",
            Layer::Wind => "wind",
            Layer::Rustle => "rustle",
            Layer::Rain => "rain",
            Layer::Birds => "birds",
            Layer::Crickets => "crickets",
            Layer::Murmur => "murmur",
            Layer::Fire => "fire",
        }
    }

    /// The manifest file's stem (`living/audio/ambience/<stem>.ogg`) a
    /// layer plays; `None` for the layers made in code.
    pub(crate) fn file(self) -> Option<&'static str> {
        match self {
            Layer::Wind => Some("wind"),
            Layer::Rain => Some("rain"),
            Layer::Birds => Some("birds_morning"),
            Layer::Fire => Some("fire"),
            Layer::Waves | Layer::Rustle | Layer::Crickets | Layer::Murmur => None,
        }
    }

    /// Whether a short or unclean loop plays as two copies half a loop
    /// apart, so its loop point is not heard.
    pub(crate) fn twinned(self) -> bool {
        matches!(self, Layer::Wind | Layer::Fire)
    }

    /// The loop made in code when the layer has no file (or its file is
    /// missing); `None` stays silent.
    pub(crate) fn made(self) -> Option<super::synth::Made> {
        use super::synth::Made;
        Some(match self {
            Layer::Waves => Made::Waves,
            Layer::Wind => Made::Wind,
            Layer::Rustle => Made::Rustle,
            Layer::Rain => Made::Rain,
            Layer::Fire => Made::Fire,
            Layer::Crickets => Made::Crickets,
            Layer::Murmur => Made::Murmur,
            Layer::Birds => return None,
        })
    }
}

/// Every sound's own level, by the manifest file's stem or a made loop's
/// name, in one table so the closing pass can tune them by ear. Started
/// from the measured loudness where the assets notes give it (wind about
/// -37 dBFS RMS, rain about -22): quiet files high, loud ones low.
pub(crate) fn sound_gain(stem: &str) -> f32 {
    let stem = stem.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_');
    match stem {
        // Ambience files.
        "wind" => 1.0,
        "rain" => 0.22,
        "birds_morning" | "birds" => 0.45,
        "fire" => 0.55,
        "thunder" => 0.6,
        // Made in code.
        "made:waves" => 0.8,
        "made:rustle" => 0.35,
        "made:crickets" => 0.45,
        "made:murmur" => 0.35,
        "made:wind" | "made:rain" | "made:fire" => 0.5,
        // Work.
        "chop" => 0.8,
        "hammer" => 0.6,
        "metal" => 0.4,
        "saw" | "saw_loop" => 0.45,
        "stone" => 0.55,
        "dig" => 0.7,
        "splash" => 0.45,
        "door_open" | "door_close" => 0.45,
        "step_dirt" => 0.5,
        // Music.
        "day" | "night" => 0.45,
        "jingle_morning" | "jingle_built" => 0.55,
        // Interface.
        "click" | "hover" | "open" | "close" | "confirm" | "error" | "place" | "alert" | "coin"
        | "page" => 0.7,
        _ => 0.6,
    }
}

/// Seconds to the next thunder in a storm, from how many there have been:
/// 12 to 40 s, never the same gap twice running.
pub(crate) fn thunder_gap(count: u32) -> f32 {
    12.0 + (count.wrapping_mul(2_246_822_519).rotate_left(7) >> 22) as f32 % 29.0
}

/// What the ambience is mixed from, all read by the view already.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Scene {
    /// Metres from the camera's focus to the nearest open water.
    pub(crate) coast_m: f32,
    /// 0 open land .. 1 deep woods, round the focus.
    pub(crate) forest: f32,
    /// The island's wind: 1 a fresh breeze, about 3 a storm.
    pub(crate) wind: f32,
    /// 0 .. 1 rain falling; storm 0 .. 1 on top of it.
    pub(crate) rain: f32,
    pub(crate) storm: f32,
    /// 0 day .. 1 night (the lamp curve).
    pub(crate) night: f32,
    /// Residents within earshot of the focus.
    pub(crate) residents: usize,
    /// 0 .. 1: a lit forge or hearth near the focus, by nearness.
    pub(crate) fire: f32,
    /// The camera's distance from its focus, metres.
    pub(crate) camera_m: f32,
    /// Paused or in a menu: everything a little quieter.
    pub(crate) held: bool,
}

/// How near sounds fall off with the camera's height: full up to 40 m,
/// a quarter from the highest Oversee camera.
pub(crate) fn nearness(camera_m: f32) -> f32 {
    (40.0 / camera_m.max(1.0)).clamp(0.25, 1.0)
}

/// Each layer's level (in [`Layer::ALL`] order), 0 .. 1, before the
/// sound's own gain and the player's volume.
pub(crate) fn layer_gains(scene: Scene) -> [f32; 8] {
    let near = nearness(scene.camera_m);
    let night = scene.night.clamp(0.0, 1.0);
    let day = 1.0 - night;
    let storm = scene.storm.clamp(0.0, 1.0);
    let rain = scene.rain.clamp(0.0, 1.0).max(storm);
    let wind = scene.wind.clamp(0.0, 3.0);
    let forest = scene.forest.clamp(0.0, 1.0);
    // The sea is heard from 60 m inland, louder in wind; from high up the
    // whole coast is in view, so it stays.
    let shore = (1.0 - scene.coast_m / 60.0).clamp(0.0, 1.0);
    let waves = shore * (0.55 + 0.15 * wind) * (0.5 + 0.5 * near);
    // Low gusts everywhere with the wind's strength; the leaves' rustle
    // in the woods.
    let gusts = (0.15 + 0.3 * wind).clamp(0.0, 1.0) * (0.6 + 0.4 * forest);
    let rustle = (forest * (0.25 + 0.3 * wind)).clamp(0.0, 1.0);
    // Birds by day (fewer in rain), crickets by night (not in rain).
    let birds = day * (1.0 - 0.85 * rain) * (0.45 + 0.55 * forest) * near;
    let crickets = night * (1.0 - rain) * 0.8 * near;
    // A village murmur where residents are about, quieter at night.
    let murmur = (scene.residents as f32 / 10.0).min(1.0) * (1.0 - 0.6 * night) * near * near;
    let fire = scene.fire.clamp(0.0, 1.0) * near * near;
    let held = if scene.held { 0.55 } else { 1.0 };
    [waves, gusts, rustle, rain, birds, crickets, murmur, fire]
        .map(|level: f32| (level * held).clamp(0.0, 1.0))
}

/// One step of a cross-fade: an exponential approach over about `seconds`.
pub(crate) fn fade_towards(current: f32, target: f32, delta_secs: f32, seconds: f32) -> f32 {
    let next = current + (target - current) * (1.0 - (-delta_secs / seconds.max(0.01)).exp());
    if (next - target).abs() < 0.002 {
        target
    } else {
        next
    }
}

/// The one-shot sounds of work and movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Shot {
    Chop,
    Hammer,
    Anvil,
    Saw,
    Pick,
    Hoe,
    Splash,
    Door,
    Mill,
    Gallop,
    Thunder,
}

impl Shot {
    pub(crate) const ALL: [Shot; 11] = [
        Shot::Chop,
        Shot::Hammer,
        Shot::Anvil,
        Shot::Saw,
        Shot::Pick,
        Shot::Hoe,
        Shot::Splash,
        Shot::Door,
        Shot::Mill,
        Shot::Gallop,
        Shot::Thunder,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Shot::Chop => "chop",
            Shot::Hammer => "hammer",
            Shot::Anvil => "anvil",
            Shot::Saw => "saw",
            Shot::Pick => "pick",
            Shot::Hoe => "hoe",
            Shot::Splash => "splash",
            Shot::Door => "door",
            Shot::Mill => "mill",
            Shot::Gallop => "gallop",
            Shot::Thunder => "thunder",
        }
    }

    /// The manifest files' stems it plays, rotating through them (stand-ins
    /// where nothing better was found: stones for the pick, mud and gravel
    /// for the hoe, metal hits for the anvil, soft steps for hooves). A
    /// sound with none (the mill's creak) is silent.
    pub(crate) fn stems(self) -> &'static [&'static str] {
        match self {
            Shot::Chop => &["chop_1", "chop_2", "chop_3"],
            Shot::Hammer => &["hammer_1", "hammer_2"],
            Shot::Anvil => &["metal_1", "metal_2"],
            Shot::Saw => &["saw_loop"],
            Shot::Pick => &["stone_1", "stone_2", "stone_3"],
            Shot::Hoe => &["dig_1", "dig_2"],
            Shot::Splash => &["splash_1", "splash_2", "splash_3"],
            Shot::Door => &["door_open"],
            Shot::Mill => &[],
            Shot::Gallop => &["step_dirt_1", "step_dirt_2"],
            Shot::Thunder => &["thunder"],
        }
    }

    /// The manifest folder its files are in.
    pub(crate) fn folder(self) -> &'static str {
        if self == Shot::Thunder {
            "ambience"
        } else {
            "work"
        }
    }

    /// Real seconds between two sounds from one emitter.
    pub(crate) fn interval(self) -> f32 {
        match self {
            Shot::Chop => 1.15,
            Shot::Hammer => 0.95,
            Shot::Anvil => 1.1,
            Shot::Saw => 2.4,
            Shot::Pick => 1.2,
            Shot::Hoe => 1.4,
            Shot::Splash => 6.0,
            Shot::Door => 0.0,
            Shot::Mill => 2.6,
            Shot::Gallop => 0.3,
            Shot::Thunder => 0.0,
        }
    }
}

/// The one-shot a playing work pose makes, if any.
pub(crate) fn pose_shot(pose: Pose) -> Option<Shot> {
    match pose {
        Pose::Chop => Some(Shot::Chop),
        Pose::Hammer => Some(Shot::Hammer),
        Pose::Work => Some(Shot::Pick),
        Pose::Farm => Some(Shot::Hoe),
        Pose::Fish => Some(Shot::Splash),
        _ => None,
    }
}

/// The machinery heard at a workplace while a craft runs there: the mill,
/// the saw, the smith's anvil.
pub(crate) fn machine_shot(role: Role) -> Option<Shot> {
    match role {
        Role::Miller | Role::Fisher => Some(Shot::Mill),
        Role::Woodcutter | Role::Carpenter => Some(Shot::Saw),
        Role::Smith => Some(Shot::Anvil),
        // A data-only trade has no machine of its own: its worker's hammer
        // pose makes the work sound.
        Role::Trade(_) => None,
        _ => None,
    }
}

/// Most one-shots sounding at once.
pub(crate) const MAX_VOICES: usize = 8;
/// Emitters farther than this from the camera's focus are not heard.
pub(crate) const AUDIBLE_M: f32 = 45.0;

/// How a sped-up clock thins the one-shots, rather than pitching them up:
/// the most started in one second and how much longer each emitter waits.
pub(crate) fn thinning(multiplier: u32) -> (usize, f32) {
    match multiplier {
        0..=1 => (5, 1.0),
        2..=3 => (3, 1.6),
        _ => (2, 2.5),
    }
}

/// A one-shot's playback speed: a small view-side variation, 0.92 .. 1.08,
/// from the emitter and how many sounds it has made.
pub(crate) fn pitch(emitter: u32, count: u32) -> f32 {
    let h = emitter
        .wrapping_mul(2_654_435_761)
        .wrapping_add(count.wrapping_mul(40_503));
    0.92 + ((h >> 16) % 1601) as f32 / 10_000.0
}

/// Seconds of silence after a music track, from its start count: long,
/// and not the same each time (90 .. 210 s).
pub(crate) fn music_gap(count: u32) -> f32 {
    90.0 + (count.wrapping_mul(2_654_435_761) >> 20) as f32 % 121.0
}

/// Which music may play: none in a pause menu, a day or a night track
/// otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tune {
    Day,
    Night,
    Morning,
    Built,
}

impl Tune {
    /// The manifest file's stem (`living/audio/music/<stem>.ogg`).
    pub(crate) fn stem(self) -> &'static str {
        match self {
            Tune::Day => "day",
            Tune::Night => "night",
            Tune::Morning => "jingle_morning",
            Tune::Built => "jingle_built",
        }
    }
}

pub(crate) fn tune_for(night: f32) -> Tune {
    if night > 0.5 { Tune::Night } else { Tune::Day }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> Scene {
        Scene {
            coast_m: 200.0,
            forest: 0.0,
            wind: 1.0,
            rain: 0.0,
            storm: 0.0,
            night: 0.0,
            residents: 0,
            fire: 0.0,
            camera_m: 30.0,
            held: false,
        }
    }

    fn level(gains: [f32; 8], layer: Layer) -> f32 {
        gains[Layer::ALL.iter().position(|l| *l == layer).unwrap()]
    }

    #[test]
    fn each_layer_follows_what_the_view_shows() {
        // Shore waves by nearness to the coast.
        let inland = layer_gains(scene());
        let beach = layer_gains(Scene {
            coast_m: 5.0,
            ..scene()
        });
        assert_eq!(level(inland, Layer::Waves), 0.0);
        assert!(level(beach, Layer::Waves) > 0.5);
        // Wind in the trees by forest and by wind.
        let woods = layer_gains(Scene {
            forest: 1.0,
            ..scene()
        });
        let gale = layer_gains(Scene {
            forest: 1.0,
            wind: 3.0,
            ..scene()
        });
        assert!(level(woods, Layer::Rustle) > level(inland, Layer::Rustle));
        assert!(level(gale, Layer::Rustle) > level(woods, Layer::Rustle));
        assert!(level(gale, Layer::Wind) > level(woods, Layer::Wind));
        assert_eq!(level(inland, Layer::Rustle), 0.0, "no leaves, no rustle");
        // Rain and storm from the weather.
        let rain = layer_gains(Scene {
            rain: 1.0,
            ..scene()
        });
        let storm = layer_gains(Scene {
            rain: 1.0,
            storm: 1.0,
            ..scene()
        });
        assert!(level(rain, Layer::Rain) > 0.9);
        assert!(level(storm, Layer::Rain) >= level(rain, Layer::Rain));
        assert!(level(storm, Layer::Wind) > 0.0);
        assert!(level(rain, Layer::Birds) < level(inland, Layer::Birds));
        // Birds by day, crickets by night.
        let night = layer_gains(Scene {
            night: 1.0,
            ..scene()
        });
        assert!(level(inland, Layer::Birds) > 0.3 && level(inland, Layer::Crickets) == 0.0);
        assert!(level(night, Layer::Crickets) > 0.5 && level(night, Layer::Birds) == 0.0);
        // A murmur where residents are about; fire near a lit hearth.
        let street = layer_gains(Scene {
            residents: 12,
            fire: 1.0,
            ..scene()
        });
        assert!(level(street, Layer::Murmur) > 0.9 && level(street, Layer::Fire) > 0.9);
        assert_eq!(level(inland, Layer::Murmur), 0.0);
    }

    #[test]
    fn close_up_is_loud_the_oversee_camera_quiet_and_a_pause_quieter() {
        let close = layer_gains(Scene {
            residents: 12,
            fire: 1.0,
            camera_m: 12.0,
            ..scene()
        });
        let high = layer_gains(Scene {
            residents: 12,
            fire: 1.0,
            camera_m: 300.0,
            ..scene()
        });
        for layer in [Layer::Murmur, Layer::Fire, Layer::Birds] {
            assert!(level(high, layer) < level(close, layer) * 0.5, "{layer:?}");
        }
        let held = layer_gains(Scene {
            residents: 12,
            held: true,
            camera_m: 12.0,
            ..scene()
        });
        assert!(level(held, Layer::Murmur) < level(close, Layer::Murmur));
        assert!(
            level(held, Layer::Murmur) > 0.0,
            "the ambience goes on, quieter"
        );
        for gains in [close, high, held] {
            assert!(gains.iter().all(|g| (0.0..=1.0).contains(g)));
        }
        let mut current = 0.0;
        for _ in 0..30 {
            let next = fade_towards(current, 1.0, 1.0 / 60.0, 2.5);
            assert!(next > current && next - current < 0.02);
            current = next;
        }
    }

    #[test]
    fn work_poses_and_machines_pick_their_sound_and_idle_is_quiet() {
        assert_eq!(pose_shot(Pose::Chop), Some(Shot::Chop));
        assert_eq!(pose_shot(Pose::Hammer), Some(Shot::Hammer));
        assert_eq!(pose_shot(Pose::Work), Some(Shot::Pick));
        assert_eq!(pose_shot(Pose::Farm), Some(Shot::Hoe));
        assert_eq!(pose_shot(Pose::Fish), Some(Shot::Splash));
        for quiet in [Pose::Idle, Pose::Walk, Pose::Sleep, Pose::Sit, Pose::Carry] {
            assert_eq!(pose_shot(quiet), None);
        }
        assert_eq!(machine_shot(Role::Miller), Some(Shot::Mill));
        assert_eq!(machine_shot(Role::Woodcutter), Some(Shot::Saw));
        assert_eq!(machine_shot(Role::Smith), Some(Shot::Anvil));
        // Every sound but the mill has a file; the stand-ins are named.
        for shot in Shot::ALL {
            assert_eq!(shot.stems().is_empty(), shot == Shot::Mill, "{shot:?}");
        }
        assert_eq!(machine_shot(Role::Baker), None);
        // A trade only the data defines: no machine, the generic hammer.
        assert_eq!(machine_shot(Role::from_key("weaver")), None);
        assert_eq!(pose_shot(Pose::Hammer), Some(Shot::Hammer));
    }

    #[test]
    fn a_fast_clock_thins_the_one_shots_and_pitch_varies_a_little() {
        let (normal, wait) = thinning(1);
        let (three, wait3) = thinning(3);
        let (ten, wait10) = thinning(10);
        assert!(normal > three && three > ten && ten >= 1);
        assert!(wait < wait3 && wait3 < wait10);
        let pitches: Vec<f32> = (0..50).map(|n| pitch(17, n)).collect();
        assert!(pitches.iter().all(|p| (0.92..=1.08).contains(p)));
        assert!(pitches.windows(2).any(|w| w[0] != w[1]));
        assert_eq!(pitch(17, 3), pitch(17, 3));
        assert!(MAX_VOICES >= 4 && MAX_VOICES <= 12);
    }

    #[test]
    fn every_sound_has_its_own_level_and_thunder_comes_at_uneven_gaps() {
        // Quiet recordings are raised, loud ones lowered.
        assert!(sound_gain("wind") > sound_gain("rain") * 3.0);
        assert_eq!(sound_gain("chop_2"), sound_gain("chop"));
        assert_eq!(sound_gain("stone_3"), sound_gain("stone"));
        for stem in ["wind", "rain", "chop_1", "made:waves", "click", "unknown"] {
            assert!((0.0..=1.0).contains(&sound_gain(stem)), "{stem}");
        }
        for count in 0..30 {
            assert!((12.0..=40.0).contains(&thunder_gap(count)));
        }
        assert!((0..10).any(|n| thunder_gap(n) != thunder_gap(n + 1)));
        assert_eq!(Tune::Morning.stem(), "jingle_morning");
        assert!(Layer::Wind.twinned() && Layer::Fire.twinned() && !Layer::Rain.twinned());
        for layer in Layer::ALL {
            assert!(
                layer.file().is_some() || layer.made().is_some(),
                "{layer:?}"
            );
        }
    }

    #[test]
    fn music_waits_long_between_tracks_and_follows_the_hour() {
        for count in 0..20 {
            assert!((90.0..=210.0).contains(&music_gap(count)));
        }
        assert_ne!(music_gap(1), music_gap(2));
        assert_eq!(tune_for(0.1), Tune::Day);
        assert_eq!(tune_for(0.9), Tune::Night);
    }
}
