//! `SfxPlugin`: plays the `CueRequest` messages the runner (and UI) write on a
//! small, prioritised voice budget that follows the effects bus.
use crate::{
    audio::{Cue as SoundCue, CueRequest},
    runner::RuntimeSet,
    settings::Settings,
};
use bevy::prelude::*;
use horde_audio::{
    mix::{calibration_gain, mix},
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
fn safe_bus_gain(master: f32, effects: f32) -> f32 {
    fn level(x: f32) -> f32 {
        if x.is_finite() {
            x.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
    level(master) * level(effects) * calibration_gain()
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
    let gain = safe_bus_gain(settings.master, settings.effects);
    let reduced = settings.reduced_effects;
    let now_ms = (time.elapsed_secs_f64() * 1000.0) as u64;
    let sounds: Vec<SoundCue> = cues.read().map(|request| request.0).collect();
    let rotation = state.rotation;
    state.rotation = state.rotation.wrapping_add(u64::from(!sounds.is_empty()));
    if gain == 0.0 {
        return;
    }
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
        let voice_gain = gain * cue_gain;
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
/// Both pending PlaybackSettings and live sinks follow the current SFX bus.
fn update_effects_gain(
    settings: Res<Settings>,
    mut voices: Query<(
        &mut EffectVoice,
        &mut PlaybackSettings,
        Option<&mut AudioSink>,
    )>,
) {
    let gain = safe_bus_gain(settings.master, settings.effects);
    let reduced = settings.reduced_effects;
    for (mut voice, mut playback, sink) in &mut voices {
        voice.cue_gain = mix(voice.cue).level(reduced);
        let voice_gain = gain * voice.cue_gain;
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
        assert_eq!(safe_bus_gain(f32::NAN, 1.0), 0.0);
        assert_eq!(safe_bus_gain(9.0, 9.0), calibration_gain());
    }
}
