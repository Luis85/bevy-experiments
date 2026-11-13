//! `SfxPlugin`: plays the `CueRequest` messages the runner (and UI) write on a
//! small, prioritised voice budget. Each voice follows the bus its cue's mix
//! row names: Combat effects, Interface sounds or Music & stingers.
use crate::{
    audio::{Cue as SoundCue, CueRequest},
    runner::RuntimeSet,
    settings::Settings,
};
use bevy::prelude::*;
use horde_audio::{
    mix::{Sliders, mix},
    render::audible,
    select::select_sounds,
};

pub struct SfxPlugin;
impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CueRequest>()
            .init_resource::<SfxState>()
            .add_systems(
                Update,
                (
                    // Cues written by this frame's runner passes play this frame.
                    play_sounds.after(RuntimeSet::Persist),
                    update_effects_gain,
                ),
            );
    }
}

/// Variant rotation and per-cue rate limiting across frames.
#[derive(Resource, Default)]
struct SfxState {
    rotation: u64,
    gate: crate::audio::CueGate,
}

/// The priority of a cue, read from the mix table.
fn rank(cue: SoundCue) -> u8 {
    mix(cue).priority
}

pub const MAX_EFFECT_VOICES: usize = crate::audio::MAX_EFFECT_VOICES;
#[derive(Component)]
pub struct EffectVoice {
    priority: u8,
    last_gain: f32,
    cue_gain: f32,
    cue: SoundCue,
}
/// The gain of `cue`'s bus at the current sliders.
fn bus_gain(sliders: Sliders, cue: SoundCue) -> f32 {
    sliders.bus_gain(mix(cue).bus)
}
fn play_sounds(
    mut commands: Commands,
    assets: Res<AssetServer>,
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    mut cues: MessageReader<CueRequest>,
    mut state: ResMut<SfxState>,
    voices: Query<(Entity, &EffectVoice)>,
) {
    let sliders = settings.sliders();
    let bus_gains = sliders.bus_gains();
    let reduced = settings.reduced_effects;
    let now_ms = (time.elapsed_secs_f64() * 1000.0) as u64;
    let mut sounds: Vec<SoundCue> = cues.read().map(|request| request.0).collect();
    let rotation = state.rotation;
    state.rotation = state.rotation.wrapping_add(u64::from(!sounds.is_empty()));
    // A cue on a muted bus takes no start slot and keeps its cooldown.
    sounds.retain(|cue| audible(*cue, &bus_gains));
    let mut live: Vec<_> = voices
        .iter()
        .map(|(entity, voice)| (entity, voice.priority))
        .collect();
    for cue in select_sounds(sounds, rotation) {
        let replace = if live.len() >= MAX_EFFECT_VOICES {
            let Some((index, (_, priority))) = live
                .iter()
                .enumerate()
                .max_by_key(|(_, (_, priority))| *priority)
            else {
                continue;
            };
            if rank(cue) >= *priority {
                continue;
            }
            Some(index)
        } else {
            None
        };
        if !state.gate.accept(cue, now_ms, reduced) {
            continue;
        }
        if let Some(index) = replace {
            let (entity, _) = live.remove(index);
            commands.entity(entity).despawn();
        }
        let cue_gain = mix(cue).level(reduced);
        let voice_gain = bus_gain(sliders, cue) * cue_gain;
        let entity = commands
            .spawn((
                EffectVoice {
                    priority: rank(cue),
                    last_gain: voice_gain,
                    cue_gain,
                    cue,
                },
                AudioPlayer::new(assets.load(crate::audio::variant_path(cue, rotation))),
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(voice_gain)),
            ))
            .id();
        live.push((entity, rank(cue)));
    }
}
/// Both pending PlaybackSettings and live sinks follow their cue's bus.
fn update_effects_gain(
    settings: Res<Settings>,
    mut voices: Query<(
        &mut EffectVoice,
        &mut PlaybackSettings,
        Option<&mut AudioSink>,
    )>,
) {
    let sliders = settings.sliders();
    let reduced = settings.reduced_effects;
    for (mut voice, mut playback, sink) in &mut voices {
        voice.cue_gain = mix(voice.cue).level(reduced);
        let voice_gain = bus_gain(sliders, voice.cue) * voice.cue_gain;
        if (voice.last_gain - voice_gain).abs() > 0.00001 {
            playback.volume = bevy::audio::Volume::Linear(voice_gain);
            if let Some(mut sink) = sink {
                sink.set_volume(playback.volume);
            }
            voice.last_gain = voice_gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_audio::mix::calibration_gain;

    #[test]
    fn effect_bus_updates_pending_voices_and_mutes_without_recreating_them() {
        let mut app = App::new();
        app.insert_resource(Settings {
            master: 0.0,
            ..Settings::default()
        })
        .add_systems(Update, update_effects_gain);
        let voice = app
            .world_mut()
            .spawn((
                EffectVoice {
                    priority: 9,
                    last_gain: 0.7,
                    cue_gain: 1.0,
                    cue: SoundCue::Explosion,
                },
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(0.7)),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .entity(voice)
                .get::<PlaybackSettings>()
                .unwrap()
                .volume,
            bevy::audio::Volume::Linear(0.0)
        );
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.master = 1.0;
            settings.effects = 0.5;
        }
        app.update();
        assert_eq!(
            app.world()
                .entity(voice)
                .get::<PlaybackSettings>()
                .unwrap()
                .volume,
            bevy::audio::Volume::Linear(0.5 * calibration_gain())
        );
        app.world_mut().resource_mut::<Settings>().reduced_effects = true;
        app.update();
        let reduced = app
            .world()
            .entity(voice)
            .get::<PlaybackSettings>()
            .unwrap()
            .volume;
        assert_eq!(
            reduced,
            bevy::audio::Volume::Linear(
                0.5 * calibration_gain() * mix(SoundCue::Explosion).level(true)
            )
        );
    }

    fn volume(app: &App, voice: Entity) -> f32 {
        match app
            .world()
            .entity(voice)
            .get::<PlaybackSettings>()
            .unwrap()
            .volume
        {
            bevy::audio::Volume::Linear(gain) => gain,
            other => panic!("{other:?}"),
        }
    }

    /// One live voice per voice bus; each slider moves its own bus's voice
    /// and no other, and Master moves all three.
    #[test]
    fn each_bus_slider_changes_only_its_voices() {
        use horde_audio::mix::Bus;
        let mut app = App::new();
        app.insert_resource(Settings {
            master: 1.0,
            effects: 1.0,
            ui: 1.0,
            music: 1.0,
            ambience: 1.0,
            ..Settings::default()
        })
        .add_systems(Update, update_effects_gain);
        // A combat warning, an interface sound and a stinger.
        let cues = [SoundCue::Boss, SoundCue::Confirm, SoundCue::Victory];
        assert_eq!(
            cues.map(|cue| mix(cue).bus),
            [Bus::Effects, Bus::Ui, Bus::Music]
        );
        let voices = cues.map(|cue| {
            app.world_mut()
                .spawn((
                    EffectVoice {
                        priority: mix(cue).priority,
                        last_gain: -1.0,
                        cue_gain: mix(cue).level(false),
                        cue,
                    },
                    PlaybackSettings::DESPAWN,
                ))
                .id()
        });
        app.update();
        let full = voices.map(|voice| volume(&app, voice));
        for (cue, gain) in cues.into_iter().zip(full) {
            assert_eq!(gain, calibration_gain() * mix(cue).level(false), "{cue:?}");
        }
        type Set = fn(&mut Settings, f32);
        let sliders: [(Set, [bool; 3]); 5] = [
            (|s, v| s.effects = v, [true, false, false]),
            (|s, v| s.ui = v, [false, true, false]),
            (|s, v| s.music = v, [false, false, true]),
            (|s, v| s.ambience = v, [false, false, false]),
            (|s, v| s.master = v, [true, true, true]),
        ];
        for (index, (set, moves)) in sliders.into_iter().enumerate() {
            for level in [0.5, 0.0] {
                set(&mut app.world_mut().resource_mut::<Settings>(), level);
                app.update();
                for n in 0..3 {
                    let expected = if moves[n] { level * full[n] } else { full[n] };
                    assert_eq!(
                        volume(&app, voices[n]),
                        expected,
                        "slider {index} at {level}, {:?}",
                        cues[n]
                    );
                }
            }
            set(&mut app.world_mut().resource_mut::<Settings>(), 1.0);
            app.update();
        }
    }

    /// A request on a muted bus starts no voice; the other buses still play.
    #[test]
    fn muted_bus_starts_no_voice_and_other_buses_still_play() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<AudioSource>()
            .add_message::<CueRequest>()
            .init_resource::<SfxState>()
            .insert_resource(Settings {
                effects: 0.0,
                ..Settings::default()
            })
            .add_systems(Update, play_sounds);
        let started = |app: &mut App, cue: SoundCue| {
            app.world_mut().write_message(CueRequest(cue));
            app.update();
            let mut voices = app.world_mut().query::<&EffectVoice>();
            voices.iter(app.world()).filter(|v| v.cue == cue).count()
        };
        // Combat effects is muted: no combat voice, but the interface sound
        // and the stinger play at their own buses' gains.
        assert_eq!(started(&mut app, SoundCue::Explosion), 0);
        assert_eq!(started(&mut app, SoundCue::Confirm), 1);
        assert_eq!(started(&mut app, SoundCue::Victory), 1);
        let gains: Vec<(SoundCue, f32)> = {
            let mut voices = app.world_mut().query::<&EffectVoice>();
            voices
                .iter(app.world())
                .map(|voice| (voice.cue, voice.last_gain))
                .collect()
        };
        let settings = app.world().resource::<Settings>().sliders();
        for (cue, gain) in gains {
            assert_eq!(
                gain,
                settings.bus_gain(mix(cue).bus) * mix(cue).level(false),
                "{cue:?}"
            );
            assert!(gain > 0.0, "{cue:?}");
        }
        // Interface sounds muted: the confirm no longer starts.
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.effects = 1.0;
            settings.ui = 0.0;
        }
        let mut voices = app
            .world_mut()
            .query_filtered::<Entity, With<EffectVoice>>();
        let live: Vec<Entity> = voices.iter(app.world()).collect();
        for entity in live {
            app.world_mut().despawn(entity);
        }
        // Past every cooldown.
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_secs(10));
        assert_eq!(started(&mut app, SoundCue::Confirm), 0);
        assert_eq!(started(&mut app, SoundCue::Explosion), 1);
    }
}
