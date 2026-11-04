//! `SfxPlugin`: plays the `SoundCue` messages the runner (and UI) write on a
//! small, prioritised voice budget that follows the effects bus.
use crate::{audio::Cue as SoundCue, runner::RuntimeSet, settings::Settings};
use bevy::prelude::*;

pub struct SfxPlugin;
impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SoundCue>()
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

impl SoundCue {
    fn priority(self) -> u8 {
        match self {
            Self::Boss | Self::Retreat | Self::Victory | Self::Defeat => 0,
            Self::Wave => 1,
            Self::Structure | Self::Explosion | Self::Molotov => 2,
            Self::Damage | Self::Confirm => 3,
            Self::Melee | Self::Throw => 4,
            Self::Reload => 5,
            Self::Ready | Self::Kill => 6,
            Self::Armor | Self::Impact => 7,
            Self::Trap => 8,
            Self::Pistol
            | Self::Smg
            | Self::Shotgun
            | Self::Rifle
            | Self::Marksman
            | Self::MachineGun => 9,
            Self::NpcShot => 10,
            Self::TurretShot => 11,
            Self::Incoming => 12,
            Self::Fire => 13,
        }
    }
    fn fire(self) -> bool {
        matches!(
            self,
            Self::Pistol
                | Self::Smg
                | Self::Shotgun
                | Self::Rifle
                | Self::Marksman
                | Self::MachineGun
                | Self::NpcShot
                | Self::TurretShot
        )
    }
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
    level(master) * level(effects) * 0.7
}
fn select_sounds(mut sounds: Vec<SoundCue>, rotation: u64) -> Vec<SoundCue> {
    // A warning already communicates arrival; avoid stacking a second spawn cue.
    if sounds.contains(&SoundCue::Wave) {
        sounds.retain(|cue| *cue != SoundCue::Incoming);
    }
    // A kill subsumes the hit that caused it, and a defeat stinger the
    // collapse that ended the defense (they share the same bank).
    if sounds.contains(&SoundCue::Kill) {
        sounds.retain(|cue| *cue != SoundCue::Impact);
    }
    if sounds.contains(&SoundCue::Defeat) {
        sounds.retain(|cue| *cue != SoundCue::Structure);
    }
    if sounds.contains(&SoundCue::Victory) {
        sounds.retain(|cue| *cue != SoundCue::Wave);
    }
    sounds.sort_by_key(|cue| cue.priority());
    sounds.dedup();
    let mut selected = Vec::with_capacity(2);
    selected.extend(
        sounds
            .iter()
            .copied()
            .filter(|cue| cue.priority() <= 3)
            .take(2),
    );
    if selected.is_empty()
        && let Some(cue) = sounds
            .iter()
            .copied()
            .find(|cue| !cue.fire() && cue.priority() <= 8)
    {
        selected.push(cue);
    }
    let fire: Vec<_> = sounds.iter().copied().filter(|cue| cue.fire()).collect();
    for n in 0..fire.len() {
        if selected.len() == 2 {
            break;
        }
        selected.push(fire[(rotation as usize + n) % fire.len()]);
    }
    for cue in sounds {
        if selected.len() == 2 {
            break;
        }
        if !selected.contains(&cue) {
            selected.push(cue);
        }
    }
    selected
}
fn play_sounds(
    mut commands: Commands,
    assets: Res<AssetServer>,
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    mut cues: MessageReader<SoundCue>,
    mut state: ResMut<SfxState>,
    voices: Query<(Entity, &EffectVoice)>,
) {
    let gain = safe_bus_gain(settings.master, settings.effects);
    let reduced = settings.reduced_effects;
    let now_ms = (time.elapsed_secs_f64() * 1000.0) as u64;
    let sounds: Vec<SoundCue> = cues.read().copied().collect();
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
            if cue.priority() >= *priority {
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
        let cue_gain = cue.gain(reduced);
        let voice_gain = gain * cue_gain;
        let entity = commands
            .spawn((
                EffectVoice {
                    priority: cue.priority(),
                    last_gain: voice_gain,
                    cue_gain,
                    cue,
                },
                AudioPlayer::new(assets.load(crate::audio::variant_path(cue, rotation))),
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(voice_gain)),
            ))
            .id();
        live.push((entity, cue.priority()));
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
        voice.cue_gain = voice.cue.gain(reduced);
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
    fn important_audio_cues_are_not_displaced_by_fire_and_sources_take_turns() {
        let selected = select_sounds(
            vec![
                SoundCue::Rifle,
                SoundCue::TurretShot,
                SoundCue::Wave,
                SoundCue::Incoming,
            ],
            0,
        );
        assert!(selected.contains(&SoundCue::Wave));
        assert_eq!(selected.len(), 2);
        assert_eq!(
            select_sounds(vec![SoundCue::Boss, SoundCue::Damage, SoundCue::Rifle], 0),
            vec![SoundCue::Boss, SoundCue::Damage]
        );
        assert_eq!(
            select_sounds(vec![SoundCue::Wave, SoundCue::Incoming], 0),
            vec![SoundCue::Wave]
        );
        let a = select_sounds(
            vec![SoundCue::Armor, SoundCue::NpcShot, SoundCue::TurretShot],
            0,
        );
        let b = select_sounds(
            vec![SoundCue::Armor, SoundCue::NpcShot, SoundCue::TurretShot],
            1,
        );
        assert!(a.contains(&SoundCue::Armor) && a.contains(&SoundCue::NpcShot));
        assert!(b.contains(&SoundCue::Armor) && b.contains(&SoundCue::TurretShot));
    }
    #[test]
    fn stingers_and_kills_replace_the_cues_they_subsume() {
        assert_eq!(
            select_sounds(vec![SoundCue::Rifle, SoundCue::Impact, SoundCue::Kill], 0),
            vec![SoundCue::Kill, SoundCue::Rifle]
        );
        assert_eq!(
            select_sounds(
                vec![
                    SoundCue::Kill,
                    SoundCue::Victory,
                    SoundCue::Boss,
                    SoundCue::Rifle
                ],
                0
            ),
            vec![SoundCue::Victory, SoundCue::Boss]
        );
        assert_eq!(
            select_sounds(
                vec![SoundCue::Structure, SoundCue::Damage, SoundCue::Defeat],
                0
            ),
            vec![SoundCue::Defeat, SoundCue::Damage]
        );
        assert_eq!(
            select_sounds(vec![SoundCue::Fire, SoundCue::Confirm], 0),
            vec![SoundCue::Confirm, SoundCue::Fire]
        );
    }
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
            bevy::audio::Volume::Linear(0.35)
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
            bevy::audio::Volume::Linear(0.35 * SoundCue::Explosion.gain(true))
        );
        assert_eq!(safe_bus_gain(f32::NAN, 1.0), 0.0);
        assert_eq!(safe_bus_gain(9.0, 9.0), 0.7);
    }
}
