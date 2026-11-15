//! Presentation-only environmental audio. Scene selection is a pure projection
//! of authored tags; the mixer never reads random rolls or advances a game tick.
use crate::animation::WeatherKind;
use bevy::{
    audio::{AudioSinkPlayback, Volume},
    prelude::*,
};
use horde_core::{content::Mode, world::StageTheme};

/// Shared with the offline renderer, which models this crossfade.
pub const CROSSFADE_SECONDS: f32 = horde_audio::render::CROSSFADE_SECONDS;
pub const CHANNEL_COUNT: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmbienceChannel {
    Base,
    Wind,
    Urban,
    Nature,
    Rain,
    Cold,
}
impl AmbienceChannel {
    pub const ALL: [Self; CHANNEL_COUNT] = [
        Self::Base,
        Self::Wind,
        Self::Urban,
        Self::Nature,
        Self::Rain,
        Self::Cold,
    ];
    pub fn index(self) -> usize {
        self as usize
    }
    /// The channel's loop, from the bank table the offline renderer reads.
    pub fn path(self) -> &'static str {
        horde_audio::bank::AMBIENCE_FILES[self.index()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AudioContext {
    #[default]
    Menu,
    Camp,
    Battle,
    Paused,
}
impl AudioContext {
    /// The context's share of the ambience bus, from the mix table.
    fn level(self) -> f32 {
        let table = horde_audio::mix::CONTEXT_GAIN;
        match self {
            Self::Menu => table.menu,
            Self::Camp => table.camp,
            Self::Battle => table.battle,
            Self::Paused => table.paused,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneAmbience {
    pub weights: [f32; CHANNEL_COUNT],
    /// Mode is useful to an inspector; the same physical scene has the same
    /// ambience whether the survivor is controlled manually or autonomously.
    pub mode: Option<Mode>,
}
impl Default for SceneAmbience {
    fn default() -> Self {
        Self::from_theme(None, None)
    }
}
impl SceneAmbience {
    pub fn from_theme(theme: Option<&StageTheme>, mode: Option<Mode>) -> Self {
        let Some(theme) = theme else {
            return Self {
                weights: [0.32, 0.24, 0.10, 0.0, 0.0, 0.0],
                mode,
            };
        };
        let weather = theme.weather.to_ascii_lowercase();
        let biome = theme.biome.to_ascii_lowercase();
        let location = format!("{} {}", theme.id, theme.setting).to_ascii_lowercase();
        let night = theme.lighting.to_ascii_lowercase().contains("night");
        let weather_kind = WeatherKind::from_scene(&theme.weather, &theme.biome);
        let rain = weather_kind == WeatherKind::Rain;
        let cold = weather_kind == WeatherKind::Snow
            || ["alpine", "boreal", "tundra", "polar", "snow"]
                .iter()
                .any(|tag| biome.contains(tag));
        let arid = ["arid", "desert", "dry"]
            .iter()
            .any(|tag| biome.contains(tag));
        let urban = [
            "depot",
            "industrial",
            "checkpoint",
            "railway",
            "market",
            "suburb",
            "highway",
            "waterfront",
            "warehouse",
            "urban",
            "city",
            "street",
        ]
        .iter()
        .any(|tag| location.contains(tag));
        let windy = weather_kind == WeatherKind::Dust || weather.contains("wind");
        let mut weights = [
            0.28,
            if windy { 0.40 } else { 0.22 },
            if urban { 0.18 } else { 0.0 },
            if !night && !cold && !arid {
                if rain { 0.035 } else { 0.12 }
            } else {
                0.0
            },
            if rain { 0.10 } else { 0.0 },
            if cold { 0.26 } else { 0.0 },
        ];
        normalize(&mut weights);
        Self { weights, mode }
    }
}

/// The frontend publishes this resource from its current or observed scene.
/// Effects volume deliberately is not an input: the SFX bus stays independent.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SceneAudioTarget {
    pub scene: SceneAmbience,
    pub master: f32,
    pub ambience: f32,
    pub context: AudioContext,
}
impl Default for SceneAudioTarget {
    fn default() -> Self {
        Self {
            scene: SceneAmbience::default(),
            master: 0.6,
            ambience: 0.15,
            context: AudioContext::Menu,
        }
    }
}
impl SceneAudioTarget {
    pub fn new(
        theme: Option<&StageTheme>,
        mode: Option<Mode>,
        master: f32,
        ambience: f32,
        context: AudioContext,
    ) -> Self {
        Self {
            scene: SceneAmbience::from_theme(theme, mode),
            master,
            ambience,
            context,
        }
    }
    pub fn gains(self) -> [f32; CHANNEL_COUNT] {
        let mut weights = self.scene.weights;
        normalize(&mut weights);
        let gain = safe_gain(self.master) * safe_gain(self.ambience) * self.context.level();
        weights.map(|weight| weight * gain)
    }
    fn muted(self) -> bool {
        safe_gain(self.master) == 0.0 || safe_gain(self.ambience) == 0.0
    }
}

fn safe_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn normalize(weights: &mut [f32; CHANNEL_COUNT]) {
    for weight in weights.iter_mut() {
        *weight = safe_gain(*weight);
    }
    let total = weights.iter().sum::<f32>();
    if total > 1.0 {
        for weight in weights {
            *weight /= total;
        }
    }
}

/// A convex smoothstep crossfade keeps the sum bounded throughout a transition.
/// Explicit muting is immediate, including sounds still waiting for decoding.
#[derive(Default)]
struct MixEnvelope {
    current: [f32; CHANNEL_COUNT],
    start: [f32; CHANNEL_COUNT],
    target: [f32; CHANNEL_COUNT],
    elapsed: f32,
}
impl MixEnvelope {
    fn advance(&mut self, target: SceneAudioTarget, delta: f32) -> [f32; CHANNEL_COUNT] {
        if target.muted() {
            *self = Self::default();
            return self.current;
        }
        let wanted = target.gains();
        if wanted != self.target {
            self.start = self.current;
            self.target = wanted;
            self.elapsed = 0.0;
        }
        let delta = if delta.is_finite() {
            delta.clamp(0.0, 0.25)
        } else {
            0.0
        };
        self.elapsed = (self.elapsed + delta).min(CROSSFADE_SECONDS);
        let progress = (self.elapsed / CROSSFADE_SECONDS).clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        self.current = std::array::from_fn(|index| {
            self.start[index] + (self.target[index] - self.start[index]) * eased
        });
        self.current
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct AtmosphereLayer {
    pub channel: AmbienceChannel,
}
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AmbienceUpdate;

pub struct AtmosphereAudioPlugin;
impl Plugin for AtmosphereAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneAudioTarget>()
            .add_systems(Update, update_ambience.in_set(AmbienceUpdate));
    }
}

#[allow(clippy::too_many_arguments)] // a Bevy system: each parameter is a world access
fn update_ambience(
    mut commands: Commands,
    assets: Res<AssetServer>,
    target: Res<SceneAudioTarget>,
    ducking: Option<Res<crate::sfx::Ducking>>,
    recorder: Option<ResMut<crate::sfx::CueLogRecorder>>,
    time: Res<Time>,
    mut layers: Query<(
        &AtmosphereLayer,
        &mut PlaybackSettings,
        Option<&mut AudioSink>,
    )>,
    mut started: Local<bool>,
    mut envelope: Local<MixEnvelope>,
) {
    if !*started {
        for channel in AmbienceChannel::ALL {
            commands.spawn((
                AtmosphereLayer { channel },
                AudioPlayer::new(assets.load(channel.path())),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
            ));
        }
        *started = true;
    }
    let gains = envelope.advance(*target, time.delta_secs());
    // The duck multiplies the crossfaded gains, so it never restarts the
    // crossfade. A stinger, a boss arrival, a wave warning, an explosion and a
    // crowd of effect voices pull the loops down (`horde_audio::duck`).
    let duck = ducking.map_or(1.0, |ducking| {
        ducking.gains.duck[horde_audio::mix::Bus::Atmosphere as usize]
    });
    // The mix's ambience calibration (`horde_audio::mix`, set by the loudness
    // gate): the loops play at the level the offline render meters.
    let calibration = horde_audio::mix::ambience_calibration_gain();
    // The review cue log takes the gains the loops are set to.
    if let Some(mut recorder) = recorder {
        recorder.ambience = gains.map(|gain| gain * calibration * duck);
    }
    for (layer, mut playback, sink) in &mut layers {
        let volume = Volume::Linear(gains[layer.channel.index()] * calibration * duck);
        // The component covers asynchronous asset/sink creation; the sink covers
        // already playing loops. No entity or decoder is recreated on a change.
        playback.volume = volume;
        if let Some(mut sink) = sink {
            sink.set_volume(volume);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn theme(biome: &str, weather: &str, lighting: &str) -> StageTheme {
        StageTheme {
            id: "depot".into(),
            biome: biome.into(),
            weather: weather.into(),
            lighting: lighting.into(),
            ..StageTheme::default()
        }
    }
    /// The offline loudness gate renders its reference scene on these beds:
    /// the first stage of the country a new journey starts in, and the same
    /// stage in rain. They must be what the game plays there.
    #[test]
    fn reference_weights_match_scene_ambience() {
        let stage1 = horde_core::world::world()
            .country("DEU")
            .expect("the start country exists")
            .stage_theme(0);
        assert_eq!(
            SceneAmbience::from_theme(Some(&stage1), Some(Mode::Personal)).weights,
            horde_audio::render::REFERENCE_WEIGHTS,
            "{stage1:?}"
        );
        let rain = StageTheme {
            weather: "rain".into(),
            ..stage1
        };
        assert_eq!(
            SceneAmbience::from_theme(Some(&rain), Some(Mode::Personal)).weights,
            horde_audio::render::RAIN_WEIGHTS
        );
        // The renderer reads the loops in channel order.
        assert_eq!(
            AmbienceChannel::ALL.map(AmbienceChannel::index),
            [0, 1, 2, 3, 4, 5]
        );
        assert_eq!(AmbienceChannel::Rain.path(), "audio/ambient_rain.wav");
        assert_eq!(horde_audio::bank::AMBIENCE_FILES.len(), CHANNEL_COUNT);
    }
    #[test]
    fn authored_scene_selects_layers_and_respects_clear_night() {
        let wet = SceneAmbience::from_theme(
            Some(&theme("Temperate", "light_rain", "dawn")),
            Some(Mode::Personal),
        );
        assert!(wet.weights[AmbienceChannel::Rain.index()] > 0.0);
        assert!(wet.weights[AmbienceChannel::Urban.index()] > 0.0);
        let clear = SceneAmbience::from_theme(Some(&theme("Tropical", "clear", "night")), None);
        assert_eq!(clear.weights[AmbienceChannel::Rain.index()], 0.0);
        assert_eq!(clear.weights[AmbienceChannel::Nature.index()], 0.0);
        let cold = SceneAmbience::from_theme(Some(&theme("Boreal", "snow", "dawn")), None);
        assert!(cold.weights[AmbienceChannel::Cold.index()] > 0.0);
    }
    #[test]
    fn manual_and_npc_share_the_physical_environment() {
        let t = theme("Arid", "dust", "dusk");
        assert_eq!(
            SceneAmbience::from_theme(Some(&t), Some(Mode::Personal)).weights,
            SceneAmbience::from_theme(Some(&t), Some(Mode::Npc)).weights
        );
    }
    #[test]
    fn mix_and_crossfades_are_bounded_at_all_slider_levels() {
        let mut envelope = MixEnvelope::default();
        for t in [
            theme("Tropical", "storm", "dawn"),
            theme("Boreal", "snow", "night"),
            theme("Arid", "wind", "dusk"),
        ] {
            let target = SceneAudioTarget::new(Some(&t), None, 1.0, 1.0, AudioContext::Battle);
            for _ in 0..100 {
                let gains = envelope.advance(target, 1.0 / 60.0);
                assert!(
                    gains
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                );
                assert!(gains.iter().sum::<f32>() <= 1.00001);
            }
        }
    }
    #[test]
    fn mute_is_immediate_even_during_crossfade() {
        let target = SceneAudioTarget {
            master: 1.0,
            ambience: 1.0,
            context: AudioContext::Battle,
            ..Default::default()
        };
        let mut envelope = MixEnvelope::default();
        envelope.advance(target, 0.2);
        let muted = SceneAudioTarget {
            ambience: 0.0,
            ..target
        };
        assert_eq!(envelope.advance(muted, 0.0), [0.0; CHANNEL_COUNT]);
        assert_eq!(
            SceneAudioTarget {
                master: f32::NAN,
                ..target
            }
            .gains(),
            [0.0; CHANNEL_COUNT]
        );
    }
    #[test]
    fn scene_transition_duration_is_display_rate_independent() {
        let target = SceneAudioTarget {
            master: 1.0,
            ambience: 1.0,
            context: AudioContext::Battle,
            ..Default::default()
        };
        let mut slow = MixEnvelope::default();
        let mut fast = MixEnvelope::default();
        for _ in 0..36 {
            slow.advance(target, 1.0 / 30.0);
        }
        for _ in 0..144 {
            fast.advance(target, 1.0 / 120.0);
        }
        for (a, b) in slow.current.into_iter().zip(fast.current) {
            assert!((a - b).abs() < 0.00001);
        }
    }
    /// The Atmosphere duck multiplies the loops' crossfaded gains; without
    /// the effects plugin the loops play unducked.
    #[test]
    fn loops_follow_the_atmosphere_duck() {
        use horde_audio::{duck::DuckGains, mix::Bus};
        let target = SceneAudioTarget {
            master: 1.0,
            ambience: 1.0,
            context: AudioContext::Battle,
            ..Default::default()
        };
        let volumes = |duck: Option<f32>| {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
                .init_asset::<AudioSource>()
                .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                    std::time::Duration::from_millis(100),
                ))
                .insert_resource(target)
                .add_systems(Update, update_ambience);
            if let Some(duck) = duck {
                let mut gains = DuckGains::NONE;
                gains.duck[Bus::Atmosphere as usize] = duck;
                let mut ducking = crate::sfx::Ducking::default();
                ducking.gains = gains;
                app.insert_resource(ducking);
            }
            // Past the 1.2 s crossfade.
            for _ in 0..20 {
                app.update();
            }
            let mut layers = app
                .world_mut()
                .query::<(&AtmosphereLayer, &PlaybackSettings)>();
            let mut volumes = [0.0; CHANNEL_COUNT];
            for (layer, playback) in layers.iter(app.world()) {
                volumes[layer.channel.index()] = match playback.volume {
                    Volume::Linear(gain) => gain,
                    other => panic!("{other:?}"),
                };
            }
            volumes
        };
        // The loops play at the mix's ambience calibration, as the offline
        // loudness gate renders them (`Sliders::bus_gain(Bus::Atmosphere)`).
        let calibration = horde_audio::mix::ambience_calibration_gain();
        assert_ne!(calibration, 1.0);
        let played = |duck: f32| target.gains().map(|gain| gain * calibration * duck);
        assert_eq!(volumes(None), played(1.0));
        assert_eq!(volumes(Some(1.0)), played(1.0));
        assert_eq!(volumes(Some(0.5)), played(0.5));
    }
    #[test]
    fn paused_and_menu_profiles_duck_without_changing_settings() {
        let live = SceneAudioTarget {
            master: 0.6,
            ambience: 0.4,
            context: AudioContext::Battle,
            ..Default::default()
        };
        assert!(
            SceneAudioTarget {
                context: AudioContext::Paused,
                ..live
            }
            .gains()
            .iter()
            .sum::<f32>()
                < live.gains().iter().sum::<f32>()
        );
        assert!(
            SceneAudioTarget {
                context: AudioContext::Menu,
                ..live
            }
            .gains()
            .iter()
            .sum::<f32>()
                < live.gains().iter().sum::<f32>()
        );
    }
}
