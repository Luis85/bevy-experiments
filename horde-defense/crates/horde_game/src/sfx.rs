//! `SfxPlugin`: plays the `CueRequest` messages the runner (and UI) write on a
//! small, prioritised voice budget. Each voice follows the bus its cue's mix
//! row names: Combat effects, Interface sounds or Music & stingers.
//!
//! `horde_audio::voices` caps the voices per bus and per cue family and
//! names the voice a start steals. A stolen voice is not cut: its decoder
//! fades it out over 40 ms on the audio thread and ends ([`Fading`]).
//!
//! Each start plays its bank's round-robin variant at a jittered speed and
//! gain (`horde_audio::select::Variety`): presentation counters only, so no
//! bank repeats a variant back to back and nothing reads the simulation.
//!
//! A started cue with a duck row pulls the ambience down ([`Ducking`],
//! `horde_audio::duck`), and a crowd of effect voices is levelled by the
//! density gain. Effects and UI voices never duck.
//!
//! A voice plays as a [`PannedClip`]: the cue's WAV into two channels at the
//! gains of its lane pan (`horde_audio::pan`; centred with "Mono audio"). A
//! variant whose WAV has not loaded yet plays as a plain centred
//! `AudioSource`, so no cue is lost.
//!
//! A request's voicing (`horde_audio::voicing`) names the bank the voice
//! plays and its tone: an enemy's voice, a boss's kind, a gun's quality. The
//! tone's speed and gain join the jitter's; its low-pass runs in the
//! `PannedClip` decoder (the plain fallback player plays unfiltered).
use crate::{
    audio::{Cue as SoundCue, CueRequest},
    runner::RuntimeSet,
    settings::Settings,
};
use bevy::{
    audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source},
    prelude::*,
};
use horde_audio::{
    cue_log::{CueLogFrame, LoggedStart},
    duck::{DuckGains, Ducker},
    mix::{Sliders, mix},
    pan::{self, Places},
    render::audible,
    select::{Variety, select_starts},
    voices::{Admission, LiveVoice, Ramp, VoiceControl, admit},
    voicing::OnePole,
};
use std::sync::Arc;

pub struct SfxPlugin;
impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CueRequest>()
            .add_audio_source::<PannedClip>()
            .init_resource::<SfxState>()
            .init_resource::<Ducking>()
            .add_systems(
                Update,
                (
                    // Cues written by this frame's runner passes play this frame.
                    play_sounds.after(RuntimeSet::Persist),
                    // This frame's density gain reaches the voices this frame.
                    update_effects_gain.after(play_sounds),
                ),
            );
    }
}

/// Fire rotation, per-cue rate limiting and per-bank variety across frames.
#[derive(Resource, Default)]
struct SfxState {
    /// Counts the frames with requests: fire sources take turns by it.
    rotation: u64,
    gate: crate::audio::CueGate,
    variety: Variety,
}

/// Ducking and density compensation (`horde_audio::duck`): the state, and
/// the gains of this frame. The ambience loops multiply in the Atmosphere
/// duck, the compensated effect voices the density gain. Effects and UI never
/// duck.
#[derive(Resource, Default)]
pub struct Ducking {
    ducker: Ducker,
    pub gains: DuckGains,
}

/// The review cue log's recorder (`horde_audio::cue_log`). Present only in a
/// review session with `LASTLIGHT_REVIEW_CUE_LOG`: [`play_sounds`] then adds
/// one frame per update with the voices it started and faded, and the review
/// service writes the frames out. Ordinary play has no recorder and logs
/// nothing.
#[derive(Resource, Default)]
pub struct CueLogRecorder {
    /// The frames not yet written.
    pub frames: Vec<CueLogFrame>,
    /// The gains the ambience loops were last set to (`update_ambience`).
    pub ambience: [f32; 6],
}

#[derive(Component)]
pub struct EffectVoice {
    /// When the voice started, in ms of real time: the oldest is stolen first.
    started_ms: u64,
    /// The fade control its decoder reads. `None` for a voice that fell back
    /// to the plain player: stolen, it is cut at once.
    control: Option<Arc<VoiceControl>>,
    last_gain: f32,
    /// The start's gain jitter times its voicing's gain: the voice keeps it
    /// while its bus gain and the effects mode change.
    jitter_gain: f32,
    cue: SoundCue,
    /// The `[left, right]` pan gains the voice started with; a voice that
    /// fell back to the plain player sounds centred whatever this says.
    #[cfg_attr(not(test), allow(dead_code))]
    pan: [f32; 2],
    /// The voice takes the density gain: not a warning, not the root
    /// defender's own shot.
    compensated: bool,
}

/// A stolen voice on its 40 ms fade. It holds no voice slot any more; its
/// decoder ends it and the finished player despawns it.
#[derive(Component)]
pub struct Fading;

/// A cue's clip panned into two channels: the decoded source, each frame
/// written as `[left, right]` at `gains`, times the gain ramp of `control`.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct PannedClip {
    /// The WAV, shared with the loaded asset (the bytes are an `Arc`).
    pub source: AudioSource,
    /// `[left, right]`, from `horde_audio::pan`.
    pub gains: [f32; 2],
    /// Fades the voice out and ends it (`horde_audio::voices`).
    pub control: Arc<VoiceControl>,
    /// The tone's low-pass cutoff (`horde_audio::voicing`), if it has one.
    pub lowpass_hz: Option<f32>,
}
impl Decodable for PannedClip {
    type Decoder = PannedDecoder;
    fn decoder(&self) -> PannedDecoder {
        let inner = self.source.decoder();
        PannedDecoder {
            channels: inner.channels().get(),
            ramp: Ramp::new(inner.sample_rate().get()),
            control: self.control.clone(),
            lowpass: self.lowpass_hz.map(|cutoff| {
                let filter = OnePole::new(cutoff, inner.sample_rate().get());
                [filter, filter]
            }),
            inner,
            gains: self.gains,
            right: None,
        }
    }
}
/// Interleaved stereo at the source's rate. A mono source feeds both
/// channels; of a wider source the first two channels play.
pub struct PannedDecoder {
    inner: <AudioSource as Decodable>::Decoder,
    channels: u16,
    gains: [f32; 2],
    /// The right sample of the frame whose left sample went out last.
    right: Option<Sample>,
    control: Arc<VoiceControl>,
    /// One gain per frame; it ends the voice when its fade is over.
    ramp: Ramp,
    /// The tone's low-pass, one state per channel.
    lowpass: Option<[OnePole; 2]>,
}
impl Iterator for PannedDecoder {
    type Item = Sample;
    fn next(&mut self) -> Option<Sample> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        let ramp = self.ramp.next(&self.control)?;
        let mut left = self.inner.next()?;
        let mut right = left;
        for channel in 1..self.channels {
            let sample = self.inner.next().unwrap_or_default();
            if channel == 1 {
                right = sample;
            }
        }
        if let Some([low_left, low_right]) = &mut self.lowpass {
            left = low_left.next(left);
            right = low_right.next(right);
        }
        self.right = Some(right * self.gains[1] * ramp);
        Some(left * self.gains[0] * ramp)
    }
}
impl Source for PannedDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).expect("two channels")
    }
    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

/// The gain of `cue`'s bus at the current sliders.
fn bus_gain(sliders: Sliders, cue: SoundCue) -> f32 {
    sliders.bus_gain(mix(cue).bus)
}
#[allow(clippy::too_many_arguments)] // a Bevy system: each parameter is a world access
fn play_sounds(
    mut commands: Commands,
    assets: Res<AssetServer>,
    sources: Res<Assets<AudioSource>>,
    mut clips: ResMut<Assets<PannedClip>>,
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    mut cues: MessageReader<CueRequest>,
    mut state: ResMut<SfxState>,
    mut ducking: ResMut<Ducking>,
    mut recorder: Option<ResMut<CueLogRecorder>>,
    voices: Query<(Entity, &EffectVoice), Without<Fading>>,
) {
    // The review cue log's frame: filled only while a recorder exists.
    let mut logged_fades: Vec<u64> = Vec::new();
    let mut logged_starts: Vec<LoggedStart> = Vec::new();
    let sliders = settings.sliders();
    let bus_gains = sliders.bus_gains();
    let reduced = settings.reduced_effects;
    let now_ms = (time.elapsed_secs_f64() * 1000.0) as u64;
    let mut places = Places::default();
    // The root defender's own shot, for the player-weapon lane.
    let mut player_shot = None;
    let mut sounds: Vec<SoundCue> = cues
        .read()
        .map(|request| {
            if let Some(place) = request.place {
                places.place(request.cue, place);
            }
            if let Some(voicing) = request.voicing {
                places.voice(request.cue, voicing);
            }
            if request.player_weapon {
                player_shot.get_or_insert(request.cue);
            }
            request.cue
        })
        .collect();
    let rotation = state.rotation;
    state.rotation = state.rotation.wrapping_add(u64::from(!sounds.is_empty()));
    // A cue on a muted bus takes no start slot and keeps its cooldown.
    sounds.retain(|cue| audible(*cue, &bus_gains));
    let player_shot = player_shot.filter(|cue| audible(*cue, &bus_gains));
    // The sounding voices; a fading one has given up its slot.
    let mut controls: Vec<(Entity, Option<Arc<VoiceControl>>)> = Vec::new();
    let mut live: Vec<LiveVoice> = voices
        .iter()
        .map(|(entity, voice)| {
            controls.push((entity, voice.control.clone()));
            LiveVoice {
                id: entity.to_bits(),
                cue: voice.cue,
                started_ms: voice.started_ms,
            }
        })
        .collect();
    for start in select_starts(sounds, player_shot, rotation) {
        let cue = start.cue;
        let admission = admit(start, &live);
        if admission == Admission::Refuse || !state.gate.accept_start(start, now_ms, reduced) {
            continue;
        }
        if let Admission::Steal(victim) = admission {
            live.retain(|voice| voice.id != victim);
            if recorder.is_some() {
                logged_fades.push(victim);
            }
            if let Some((entity, control)) = controls
                .iter()
                .find(|(entity, _)| entity.to_bits() == victim)
            {
                match control {
                    Some(control) => {
                        control.fade_out();
                        commands.entity(*entity).insert(Fading);
                    }
                    // A plain player has no ramp to fade on.
                    None => commands.entity(*entity).despawn(),
                }
            }
        }
        // The request's bank and tone, the bank's next variant, and this
        // start's speed and gain jitter.
        let voicing = places.voicing(cue);
        let pick = state.variety.pick(voicing.bank, mix(cue).jitter);
        ducking.ducker.trigger(cue);
        let tuned_gain = pick.jitter.gain * voicing.gain();
        let voice_gain = bus_gain(sliders, cue) * mix(cue).level(reduced) * tuned_gain;
        let gains = pan::voice_gains(cue, &places, reduced, settings.mono_audio);
        let handle: Handle<AudioSource> = assets.load(voicing.bank.variant_path(pick.variant));
        let source = sources.get(&handle);
        let control = source.map(|_| Arc::new(VoiceControl::default()));
        let voice = (
            EffectVoice {
                started_ms: now_ms,
                control: control.clone(),
                last_gain: voice_gain,
                jitter_gain: tuned_gain,
                cue,
                pan: gains,
                compensated: horde_audio::duck::compensated(start),
            },
            PlaybackSettings::DESPAWN
                .with_volume(bevy::audio::Volume::Linear(voice_gain))
                .with_speed(pick.jitter.speed * voicing.speed),
        );
        let entity = match (source, control.clone()) {
            (Some(source), Some(control)) => commands.spawn((
                voice,
                AudioPlayer(clips.add(PannedClip {
                    source: source.clone(),
                    gains,
                    control,
                    lowpass_hz: voicing.lowpass_hz,
                })),
            )),
            // Not loaded yet: the plain player waits for the load and plays
            // the cue centred.
            _ => commands.spawn((voice, AudioPlayer::new(handle))),
        }
        .id();
        if recorder.is_some() {
            logged_starts.push(LoggedStart {
                voice: entity.to_bits(),
                cue: horde_audio::cue_log::cue_name(cue),
                clip: voicing.bank.variant_path(pick.variant),
                speed: pick.jitter.speed * voicing.speed,
                lowpass_hz: voicing.lowpass_hz,
                gains: gains.map(|side| side * voice_gain),
                compensated: horde_audio::duck::compensated(start),
            });
        }
        controls.push((entity, control));
        live.push(LiveVoice {
            id: entity.to_bits(),
            cue,
            started_ms: now_ms,
        });
    }
    // Real time, like the cue gates: the envelopes run on while the game is
    // paused.
    ducking.gains = ducking.ducker.advance(time.delta_secs(), &live);
    if let Some(recorder) = recorder.as_mut() {
        let ambience = recorder.ambience;
        recorder.frames.push(CueLogFrame {
            us: time.elapsed().as_micros() as u64,
            dt_us: time.delta().as_micros() as u64,
            fades: logged_fades,
            starts: logged_starts,
            ambience,
            density: ducking.gains.density,
        });
    }
}
/// Both pending PlaybackSettings and live sinks follow their cue's bus. A
/// compensated voice also follows the density gain: its decoder ramps to it
/// (no step), and a voice on the plain fallback player takes it in its
/// volume.
fn update_effects_gain(
    settings: Res<Settings>,
    ducking: Option<Res<Ducking>>,
    mut voices: Query<(
        &mut EffectVoice,
        &mut PlaybackSettings,
        Option<&mut AudioSink>,
    )>,
) {
    let sliders = settings.sliders();
    let reduced = settings.reduced_effects;
    let density = ducking.map_or(1.0, |ducking| ducking.gains.density);
    for (mut voice, mut playback, sink) in &mut voices {
        let mut voice_gain =
            bus_gain(sliders, voice.cue) * mix(voice.cue).level(reduced) * voice.jitter_gain;
        if voice.compensated {
            match &voice.control {
                // A fading voice keeps its way to silence.
                Some(control) if control.stopping() => {}
                Some(control) => control.set_target(density),
                None => voice_gain *= density,
            }
        }
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
                    started_ms: 0,
                    control: None,
                    last_gain: 0.7,
                    jitter_gain: 1.0,
                    cue: SoundCue::Explosion,
                    pan: pan::CENTRE,
                    compensated: false,
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
                        started_ms: 0,
                        control: None,
                        last_gain: -1.0,
                        jitter_gain: 1.0,
                        cue,
                        pan: pan::CENTRE,
                        compensated: false,
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
            .init_asset::<PannedClip>()
            .add_message::<CueRequest>()
            .init_resource::<SfxState>()
            .init_resource::<Ducking>()
            .insert_resource(Settings {
                effects: 0.0,
                ..Settings::default()
            })
            .add_systems(Update, play_sounds);
        let started = |app: &mut App, cue: SoundCue| {
            app.world_mut().write_message(CueRequest::centred(cue));
            app.update();
            let mut voices = app.world_mut().query::<&EffectVoice>();
            voices.iter(app.world()).filter(|v| v.cue == cue).count()
        };
        // Combat effects is muted: no combat voice, but the interface sound
        // and the stinger play at their own buses' gains.
        assert_eq!(started(&mut app, SoundCue::Explosion), 0);
        assert_eq!(started(&mut app, SoundCue::Confirm), 1);
        assert_eq!(started(&mut app, SoundCue::Victory), 1);
        let gains: Vec<(SoundCue, f32, f32)> = {
            let mut voices = app.world_mut().query::<&EffectVoice>();
            voices
                .iter(app.world())
                .map(|voice| (voice.cue, voice.last_gain, voice.jitter_gain))
                .collect()
        };
        let settings = app.world().resource::<Settings>().sliders();
        for (cue, gain, jitter) in gains {
            assert_eq!(
                gain,
                settings.bus_gain(mix(cue).bus) * mix(cue).level(false) * jitter,
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

    /// A play-systems app with `wav` already loaded as the asset every
    /// variant path of `cue` resolves to. Returns the app.
    fn pan_app(settings: Settings) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<AudioSource>()
            .init_asset::<PannedClip>()
            .add_message::<CueRequest>()
            .init_resource::<SfxState>()
            .init_resource::<Ducking>()
            .insert_resource(settings)
            .add_systems(Update, play_sounds);
        app
    }
    fn start(app: &mut App, request: CueRequest) -> Entity {
        app.world_mut().write_message(request);
        app.update();
        let mut voices = app.world_mut().query::<(Entity, &EffectVoice)>();
        let found: Vec<Entity> = voices
            .iter(app.world())
            .filter(|(_, voice)| voice.cue == request.cue)
            .map(|(entity, _)| entity)
            .collect();
        assert_eq!(found.len(), 1, "{request:?}");
        found[0]
    }
    fn clear(app: &mut App) {
        let mut voices = app
            .world_mut()
            .query_filtered::<Entity, With<EffectVoice>>();
        let live: Vec<Entity> = voices.iter(app.world()).collect();
        for entity in live {
            app.world_mut().despawn(entity);
        }
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_secs(10));
    }
    fn far(cue: SoundCue) -> CueRequest {
        CueRequest {
            cue,
            place: Some(1.0),
            ..CueRequest::centred(cue)
        }
    }

    /// A placed request starts its voice at the balance law's gains: the
    /// far end at `[1 - D, 1]`, half the depth in reduced effects, and a cue
    /// that never pans (the defender's gun, a stinger, an interface sound)
    /// centred even with a place.
    #[test]
    fn placed_requests_start_voices_at_the_lane_pan_gains() {
        let mut app = pan_app(Settings::default());
        let voice = start(&mut app, far(SoundCue::Explosion));
        let pan_of =
            |app: &App, voice: Entity| app.world().entity(voice).get::<EffectVoice>().unwrap().pan;
        assert_eq!(pan_of(&app, voice), [1.0 - pan::PAN_DEPTH, 1.0]);
        clear(&mut app);
        let centred = start(&mut app, CueRequest::centred(SoundCue::Explosion));
        assert_eq!(pan_of(&app, centred), pan::CENTRE);
        for cue in [SoundCue::Rifle, SoundCue::Victory, SoundCue::Confirm] {
            clear(&mut app);
            let voice = start(&mut app, far(cue));
            assert_eq!(pan_of(&app, voice), pan::CENTRE, "{cue:?}");
        }
        clear(&mut app);
        app.world_mut().resource_mut::<Settings>().reduced_effects = true;
        let reduced = start(&mut app, far(SoundCue::Explosion));
        assert_eq!(pan_of(&app, reduced), [1.0 - pan::PAN_DEPTH_REDUCED, 1.0]);
    }

    /// A6-18: with a recorder, every update logs one cue-log frame: the
    /// started voice by entity bits with its clip, speed and final
    /// `[left, right]` gains, a stolen voice as a fade, and the frame's
    /// ambience and density gains. Without a recorder nothing is logged.
    #[test]
    fn started_and_stolen_voices_reach_the_cue_log() {
        let mut app = pan_app(Settings::default());
        app.init_resource::<CueLogRecorder>();
        app.world_mut().resource_mut::<CueLogRecorder>().ambience = [0.25; 6];
        let voice = start(&mut app, far(SoundCue::Explosion));
        let settings = Settings::default();
        let frames = app.world().resource::<CueLogRecorder>().frames.clone();
        assert_eq!(frames.len(), 1, "one frame per update");
        let frame = &frames[0];
        assert_eq!(frame.ambience, [0.25; 6]);
        assert_eq!(
            frame.density,
            app.world().resource::<Ducking>().gains.density
        );
        assert!(frame.fades.is_empty());
        assert_eq!(frame.starts.len(), 1);
        let logged = &frame.starts[0];
        let played = app.world().entity(voice).get::<EffectVoice>().unwrap();
        assert_eq!(logged.voice, voice.to_bits());
        assert_eq!(logged.cue, "Explosion");
        assert!(
            logged.clip.starts_with("audio/") && logged.clip.ends_with(".wav"),
            "{}",
            logged.clip
        );
        let voice_gain = bus_gain(settings.sliders(), SoundCue::Explosion)
            * mix(SoundCue::Explosion).level(false)
            * played.jitter_gain;
        assert_eq!(
            logged.gains,
            [1.0 - pan::PAN_DEPTH, 1.0].map(|side| side * voice_gain)
        );
        assert_eq!(logged.compensated, played.compensated);
        // An update without requests still logs its frame, later in time.
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(std::time::Duration::from_millis(20));
        app.update();
        let frames = &app.world().resource::<CueLogRecorder>().frames;
        assert_eq!(frames.len(), 2);
        assert!(frames[1].starts.is_empty() && frames[1].us >= frames[0].us);
        // The log reads back as written.
        let text = horde_audio::cue_log::write(frames);
        assert_eq!(&horde_audio::cue_log::parse(&text).unwrap(), frames);
        // A third gun steals the oldest: the frame logs its fade by id.
        let later = |app: &mut App| {
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(std::time::Duration::from_millis(100));
        };
        let pistol = start(&mut app, CueRequest::centred(SoundCue::Pistol));
        later(&mut app);
        start(&mut app, CueRequest::centred(SoundCue::Rifle));
        later(&mut app);
        let smg = start(&mut app, CueRequest::centred(SoundCue::Smg));
        let frames = &app.world().resource::<CueLogRecorder>().frames;
        let last = frames.last().unwrap();
        assert_eq!(last.fades, [pistol.to_bits()]);
        assert_eq!(last.starts[0].voice, smg.to_bits());
        // No recorder: nothing is logged and the voice still starts.
        let mut plain = pan_app(Settings::default());
        start(&mut plain, far(SoundCue::Explosion));
        assert!(plain.world().get_resource::<CueLogRecorder>().is_none());
    }

    /// The "Mono audio" setting: every voice starts centred, wherever its
    /// event happened.
    #[test]
    fn mono_audio_forces_centre() {
        let mut app = pan_app(Settings {
            mono_audio: true,
            ..Settings::default()
        });
        for cue in [SoundCue::Explosion, SoundCue::Kill, SoundCue::Incoming] {
            clear(&mut app);
            let voice = start(&mut app, far(cue));
            let voice = app.world().entity(voice);
            assert_eq!(
                voice.get::<EffectVoice>().unwrap().pan,
                pan::CENTRE,
                "{cue:?}"
            );
        }
        // Off again, the same request pans.
        clear(&mut app);
        app.world_mut().resource_mut::<Settings>().mono_audio = false;
        let voice = start(&mut app, far(SoundCue::Explosion));
        let voice = app.world().entity(voice);
        assert_ne!(voice.get::<EffectVoice>().unwrap().pan, pan::CENTRE);
    }

    fn tone(frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|n| ((n % 64) as f32 / 64.0 - 0.5) * 0.8)
            .collect()
    }

    /// The decoder writes every source frame as `[left, right]` at the pan
    /// gains, as two channels at the source's own rate.
    #[test]
    fn panned_clip_outputs_two_channels_at_source_rate() {
        for rate in [22_050_u32, 44_100] {
            let samples = tone(500);
            let source = AudioSource {
                bytes: horde_audio::wav::pcm16_bytes(rate, &samples).into(),
            };
            let plain: Vec<Sample> = source.decoder().collect();
            assert_eq!(plain.len(), samples.len());
            let gains = [0.7, 1.0];
            let decoder = PannedClip {
                source,
                gains,
                control: Arc::default(),
                lowpass_hz: None,
            }
            .decoder();
            assert_eq!(decoder.channels().get(), 2);
            assert_eq!(decoder.sample_rate().get(), rate);
            let out: Vec<Sample> = decoder.collect();
            assert_eq!(out.len(), 2 * plain.len());
            for (frame, sample) in out.as_chunks::<2>().0.iter().zip(&plain) {
                assert_eq!(*frame, [sample * gains[0], sample * gains[1]]);
            }
            assert!(out.iter().any(|sample| sample.abs() > 0.2));
        }
    }

    /// A variant that has not loaded yet plays as a plain `AudioSource`
    /// player, so the cue is not lost; once its WAV is loaded the voice is a
    /// `PannedClip` with the request's gains.
    #[test]
    fn unloaded_clip_falls_back_to_audio_source_player() {
        let mut app = pan_app(Settings::default());
        let voice = start(&mut app, far(SoundCue::Explosion));
        let entity = app.world().entity(voice);
        assert!(entity.contains::<AudioPlayer<AudioSource>>());
        assert!(!entity.contains::<AudioPlayer<PannedClip>>());
        // Load every variant the rotation can pick.
        let wav = AudioSource {
            bytes: horde_audio::wav::pcm16_bytes(44_100, &tone(100)).into(),
        };
        let handles: Vec<Handle<AudioSource>> = (0..SoundCue::Explosion.bank().variants())
            .map(|variant| {
                let path = crate::audio::variant_path(SoundCue::Explosion, variant);
                let handle = app.world().resource::<AssetServer>().load(path);
                app.world_mut()
                    .resource_mut::<Assets<AudioSource>>()
                    .insert(handle.id(), wav.clone())
                    .unwrap();
                handle
            })
            .collect();
        clear(&mut app);
        let voice = start(&mut app, far(SoundCue::Explosion));
        let entity = app.world().entity(voice);
        assert!(!entity.contains::<AudioPlayer<AudioSource>>());
        let player = entity
            .get::<AudioPlayer<PannedClip>>()
            .expect("a panned voice");
        let clip = app
            .world()
            .resource::<Assets<PannedClip>>()
            .get(&player.0)
            .expect("the clip asset");
        assert_eq!(clip.gains, [1.0 - pan::PAN_DEPTH, 1.0]);
        assert_eq!(clip.gains, entity.get::<EffectVoice>().unwrap().pan);
        assert_eq!(clip.source.bytes, wav.bytes);
        drop(handles);
    }

    /// A voiced request (A6-12) plays the bank its voicing names, at the
    /// voicing's speed and gain on top of the jitter's, through the
    /// voicing's low-pass; the gain survives a slider move.
    #[test]
    fn a_voiced_request_plays_its_voicings_bank_speed_gain_and_lowpass() {
        use horde_audio::{
            bank::{BankId, EnemyStage, EnemyVoice},
            voicing::{Tone, Voicing},
        };
        let mut app = pan_app(Settings::default());
        app.add_systems(Update, update_effects_gain.after(play_sounds));
        // A brute's death, lower and louder.
        let bank = BankId::Enemy(EnemyVoice::Brute, EnemyStage::Death);
        let voicing = Voicing::tuned(bank, 0.9, 1.0);
        let cue = SoundCue::EnemyDeath;
        let entity = start(
            &mut app,
            CueRequest {
                voicing: Some(voicing),
                ..CueRequest::centred(cue)
            },
        );
        let pick = Variety::default().pick(bank, mix(cue).jitter);
        let sliders = app.world().resource::<Settings>().sliders();
        let voice = app.world().entity(entity);
        let playback = voice.get::<PlaybackSettings>().unwrap();
        assert_eq!(playback.speed, pick.jitter.speed * 0.9);
        let tuned = pick.jitter.gain * voicing.gain();
        assert!(voicing.gain() > 1.0);
        let expected = sliders.bus_gain(mix(cue).bus) * mix(cue).level(false) * tuned;
        assert_eq!(playback.volume, bevy::audio::Volume::Linear(expected));
        let handle = voice.get::<AudioPlayer<AudioSource>>().unwrap().0.clone();
        let path = app
            .world()
            .resource::<AssetServer>()
            .get_path(handle.id())
            .unwrap()
            .to_string();
        assert_eq!(path, bank.variant_path(pick.variant));
        assert!(
            path.starts_with("audio/survival_enemy_brute_death_"),
            "{path}"
        );
        // A slider move keeps the voicing's gain.
        app.world_mut().resource_mut::<Settings>().effects = 0.4;
        app.update();
        let sliders = app.world().resource::<Settings>().sliders();
        let voice = app.world().get::<EffectVoice>(entity).unwrap();
        assert_eq!(
            voice.last_gain,
            sliders.bus_gain(mix(cue).bus) * mix(cue).level(false) * tuned
        );
        // The decoder runs the tone's low-pass: a 10 kHz tone through the
        // distant tone's 3.5 kHz comes out well under the unfiltered clip.
        let rate = 44_100_u32;
        let samples: Vec<f32> = (0..rate / 10)
            .map(|n| 0.5 * (std::f32::consts::TAU * 10_000.0 * n as f32 / rate as f32).sin())
            .collect();
        let peak = |lowpass_hz: Option<f32>| {
            let source = AudioSource {
                bytes: horde_audio::wav::pcm16_bytes(rate, &samples).into(),
            };
            PannedClip {
                source,
                gains: pan::CENTRE,
                control: Arc::default(),
                lowpass_hz,
            }
            .decoder()
            .skip(200)
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
        };
        let (plain, distant) = (peak(None), peak(Tone::DISTANT.lowpass_hz));
        assert!(plain > 0.45, "{plain}");
        assert!(distant < 0.5 * plain, "{distant} of {plain}");
        // And passes what lies under the cutoff: the filter is no gain.
        assert!(distant > 0.1 * plain, "{distant} of {plain}");
    }

    /// The adapter plays what `Variety` picks: the bank's round-robin variant,
    /// at the pick's speed, with the pick's gain kept through a slider move.
    #[test]
    fn starts_play_the_banks_pick_at_its_jittered_speed_and_gain() {
        let mut app = pan_app(Settings::default());
        app.add_systems(Update, update_effects_gain.after(play_sounds));
        let mut variety = Variety::default();
        let mut paths = Vec::new();
        for _ in 0..6 {
            let entity = start(&mut app, CueRequest::centred(SoundCue::Explosion));
            let pick = variety.pick_cue(SoundCue::Explosion);
            let sliders = app.world().resource::<Settings>().sliders();
            let world = app.world();
            let voice = world.entity(entity);
            let playback = voice.get::<PlaybackSettings>().unwrap();
            assert_eq!(playback.speed, pick.jitter.speed);
            let expected = sliders.bus_gain(mix(SoundCue::Explosion).bus)
                * mix(SoundCue::Explosion).level(false)
                * pick.jitter.gain;
            assert_eq!(playback.volume, bevy::audio::Volume::Linear(expected));
            assert_eq!(
                voice.get::<EffectVoice>().unwrap().jitter_gain,
                pick.jitter.gain
            );
            let handle = voice.get::<AudioPlayer<AudioSource>>().unwrap().0.clone();
            let path = world
                .resource::<AssetServer>()
                .get_path(handle.id())
                .unwrap()
                .to_string();
            assert_eq!(
                path,
                crate::audio::variant_path(SoundCue::Explosion, pick.variant)
            );
            paths.push(path);
            clear(&mut app);
        }
        assert!(paths.windows(2).all(|pair| pair[0] != pair[1]), "{paths:?}");
        // A slider move keeps the start's gain jitter.
        let entity = start(&mut app, CueRequest::centred(SoundCue::Explosion));
        let jitter = app.world().get::<EffectVoice>(entity).unwrap().jitter_gain;
        assert_ne!(jitter, 1.0);
        app.world_mut().resource_mut::<Settings>().effects = 0.4;
        app.update();
        let sliders = app.world().resource::<Settings>().sliders();
        assert_eq!(
            app.world().get::<PlaybackSettings>(entity).unwrap().volume,
            bevy::audio::Volume::Linear(
                sliders.bus_gain(mix(SoundCue::Explosion).bus)
                    * mix(SoundCue::Explosion).level(false)
                    * jitter
            )
        );
    }

    /// The root defender's shot starts in a frame whose Effects budget is
    /// full of critical cues; the same cue without the lane does not.
    #[test]
    fn player_weapon_request_starts_beside_a_full_effects_budget() {
        let crowded = [SoundCue::Boss, SoundCue::Explosion, SoundCue::Structure];
        for (player_weapon, expected) in [(true, 1), (false, 0)] {
            let mut app = pan_app(Settings::default());
            for cue in crowded {
                app.world_mut().write_message(CueRequest::centred(cue));
            }
            app.world_mut().write_message(CueRequest {
                player_weapon,
                ..CueRequest::centred(SoundCue::Rifle)
            });
            app.update();
            let mut voices = app.world_mut().query::<&EffectVoice>();
            let started: Vec<SoundCue> = voices.iter(app.world()).map(|voice| voice.cue).collect();
            for cue in crowded {
                assert!(started.contains(&cue), "{cue:?} in {started:?}");
            }
            let rifles = started
                .iter()
                .filter(|cue| **cue == SoundCue::Rifle)
                .count();
            assert_eq!(
                rifles, expected,
                "player_weapon {player_weapon}: {started:?}"
            );
        }
    }

    /// Loads a short WAV as every variant of `cue`, so its voices play as
    /// `PannedClip`s. The handles keep the assets alive.
    fn load_variants(app: &mut App, cue: SoundCue) -> Vec<Handle<AudioSource>> {
        let wav = AudioSource {
            bytes: horde_audio::wav::pcm16_bytes(44_100, &tone(100)).into(),
        };
        (0..cue.bank().variants())
            .map(|variant| {
                let path = crate::audio::variant_path(cue, variant);
                let handle = app.world().resource::<AssetServer>().load(path);
                app.world_mut()
                    .resource_mut::<Assets<AudioSource>>()
                    .insert(handle.id(), wav.clone())
                    .unwrap();
                handle
            })
            .collect()
    }

    /// A third gun at the gun family's cap of two steals the oldest gun. A
    /// panned voice is told to fade and keeps playing its tail, without a
    /// voice slot; a voice on the plain fallback player is cut.
    #[test]
    fn stolen_voice_fades_through_its_control_and_gives_up_its_slot() {
        use horde_audio::voices::{Family, family};
        let guns = [
            SoundCue::Pistol,
            SoundCue::Rifle,
            SoundCue::Smg,
            SoundCue::Shotgun,
        ];
        assert!(guns.iter().all(|cue| family(*cue) == Some(Family::Guns)));
        assert_eq!(Family::Guns.cap(), 2);
        let later = |app: &mut App| {
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(std::time::Duration::from_millis(100));
        };
        let fading = |app: &App, voice: Entity| app.world().entity(voice).contains::<Fading>();
        let stopping = |app: &App, voice: Entity| {
            let control = &app
                .world()
                .entity(voice)
                .get::<EffectVoice>()
                .unwrap()
                .control;
            control.as_ref().expect("a panned voice").stopping()
        };

        // Panned voices.
        let mut app = pan_app(Settings::default());
        let handles = guns.map(|cue| load_variants(&mut app, cue));
        let pistol = start(&mut app, CueRequest::centred(guns[0]));
        later(&mut app);
        let rifle = start(&mut app, CueRequest::centred(guns[1]));
        assert!(!fading(&app, pistol) && !stopping(&app, pistol));
        later(&mut app);
        let smg = start(&mut app, CueRequest::centred(guns[2]));
        // The oldest gun fades; it is still there to play its tail.
        assert!(fading(&app, pistol) && stopping(&app, pistol));
        assert!(!fading(&app, rifle) && !stopping(&app, rifle));
        // Its clip shares the control the frame thread wrote.
        let player = app.world().entity(pistol).get::<AudioPlayer<PannedClip>>();
        let clip = app
            .world()
            .resource::<Assets<PannedClip>>()
            .get(&player.expect("a panned voice").0)
            .unwrap();
        assert!(clip.control.stopping());
        // The fading voice holds no slot: the next gun steals the rifle.
        later(&mut app);
        let shotgun = start(&mut app, CueRequest::centred(guns[3]));
        assert!(fading(&app, rifle) && stopping(&app, rifle));
        assert!(!fading(&app, smg) && !fading(&app, shotgun));
        drop(handles);

        // Fallback voices: nothing loaded, so the stolen voice is cut.
        let mut app = pan_app(Settings::default());
        let pistol = start(&mut app, CueRequest::centred(guns[0]));
        later(&mut app);
        let rifle = start(&mut app, CueRequest::centred(guns[1]));
        later(&mut app);
        start(&mut app, CueRequest::centred(guns[2]));
        assert!(app.world().get_entity(pistol).is_err());
        assert!(app.world().get_entity(rifle).is_ok());
    }

    /// The decoder runs the fade: full gain until the stop, then 40 ms of
    /// ramp without a jump, then the end of the voice.
    #[test]
    fn panned_decoder_fades_for_40ms_after_the_stop_and_ends() {
        use horde_audio::voices::{CONTROL_PERIOD, FADE_MS};
        let rate = 44_100_u32;
        let source = AudioSource {
            bytes: horde_audio::wav::pcm16_bytes(rate, &vec![0.5; rate as usize]).into(),
        };
        let level: Sample = source.decoder().next().unwrap();
        let control = Arc::new(VoiceControl::default());
        let mut decoder = PannedClip {
            source,
            gains: pan::CENTRE,
            control: control.clone(),
            lowpass_hz: None,
        }
        .decoder();
        // Ten control periods at full gain; the stop lands on a control read.
        let frames = 10 * CONTROL_PERIOD as usize;
        let head: Vec<Sample> = decoder.by_ref().take(2 * frames).collect();
        assert!(head.iter().all(|sample| *sample == level * pan::CENTRE[0]));
        control.fade_out();
        let tail: Vec<Sample> = decoder.collect();
        assert_eq!(tail.len(), 2 * (rate * FADE_MS / 1000) as usize);
        let mut last = head[0];
        for frame in tail.as_chunks::<2>().0 {
            assert_eq!(frame[0], frame[1]);
            assert!(frame[0] <= last && last - frame[0] <= 0.1 * level);
            last = frame[0];
        }
        assert!(last < 0.01 * level, "{last}");
    }

    /// A boss arrival that starts triggers its duck; a cue without a duck
    /// row, and a ducking cue its muted bus keeps from starting, do not.
    #[test]
    fn started_cues_trigger_their_duck() {
        use horde_audio::mix::{Bus, db_to_linear};
        let mut app = pan_app(Settings::default());
        start(&mut app, CueRequest::centred(SoundCue::Impact));
        assert!(!app.world().resource::<Ducking>().ducker.ducking());
        start(&mut app, CueRequest::centred(SoundCue::Boss));
        let mut ducker = app.world().resource::<Ducking>().ducker.clone();
        assert!(ducker.ducking());
        // The rest of the 30 ms attack: the ambience is 6 dB down.
        let gains = ducker.advance(0.03, &[]);
        let atmosphere = gains.duck[Bus::Atmosphere as usize];
        assert!(
            (atmosphere - db_to_linear(-6.0)).abs() < 1e-3,
            "{atmosphere}"
        );
        // The published gains never touch Master, Effects or UI.
        assert_eq!(app.world().resource::<Ducking>().gains.duck[..3], [1.0; 3]);

        let mut app = pan_app(Settings {
            effects: 0.0,
            ..Settings::default()
        });
        app.world_mut()
            .write_message(CueRequest::centred(SoundCue::Boss));
        app.update();
        assert!(!app.world().resource::<Ducking>().ducker.ducking());
    }

    /// A start is marked for the density gain unless it is a warning or the
    /// root defender's own shot.
    #[test]
    fn starts_are_marked_for_density_compensation() {
        let mut app = pan_app(Settings::default());
        let marked = |app: &mut App, request: CueRequest| {
            let voice = start(app, request);
            app.world().get::<EffectVoice>(voice).unwrap().compensated
        };
        assert!(marked(&mut app, CueRequest::centred(SoundCue::Impact)));
        assert!(marked(&mut app, CueRequest::centred(SoundCue::Rifle)));
        assert!(!marked(&mut app, CueRequest::centred(SoundCue::Boss)));
        assert!(!marked(&mut app, CueRequest::centred(SoundCue::Confirm)));
        clear(&mut app);
        assert!(!marked(
            &mut app,
            CueRequest {
                player_weapon: true,
                ..CueRequest::centred(SoundCue::Rifle)
            }
        ));
    }

    /// The density gain reaches a compensated voice through its control (the
    /// decoder ramps to it), or through its volume on the plain fallback
    /// player. A warning and a fading voice are left alone.
    #[test]
    fn density_gain_reaches_compensated_voices_only() {
        let mut app = App::new();
        app.insert_resource(Settings::default())
            .insert_resource(Ducking {
                gains: DuckGains {
                    density: 0.5,
                    ..DuckGains::NONE
                },
                ..Ducking::default()
            })
            .add_systems(Update, update_effects_gain);
        let mut spawn = |cue: SoundCue, compensated: bool, control: Option<Arc<VoiceControl>>| {
            app.world_mut()
                .spawn((
                    EffectVoice {
                        started_ms: 0,
                        control,
                        last_gain: -1.0,
                        jitter_gain: 1.0,
                        cue,
                        pan: pan::CENTRE,
                        compensated,
                    },
                    PlaybackSettings::DESPAWN,
                ))
                .id()
        };
        let controls: [Arc<VoiceControl>; 3] = Default::default();
        controls[2].fade_out();
        let panned = spawn(SoundCue::Impact, true, Some(controls[0].clone()));
        let warning = spawn(SoundCue::Boss, false, Some(controls[1].clone()));
        let fading = spawn(SoundCue::Impact, true, Some(controls[2].clone()));
        let plain = spawn(SoundCue::Impact, true, None);
        app.update();
        assert_eq!(controls[0].target(), 0.5);
        assert_eq!(controls[1].target(), 1.0);
        assert_eq!(controls[2].target(), 0.0);
        let sliders = Settings::default().sliders();
        let full = |cue: SoundCue| sliders.bus_gain(mix(cue).bus) * mix(cue).level(false);
        assert_eq!(volume(&app, panned), full(SoundCue::Impact));
        assert_eq!(volume(&app, warning), full(SoundCue::Boss));
        assert_eq!(volume(&app, fading), full(SoundCue::Impact));
        assert_eq!(volume(&app, plain), full(SoundCue::Impact) * 0.5);
        // The crowd thins out: the voices come back.
        app.world_mut().resource_mut::<Ducking>().gains.density = 1.0;
        app.update();
        assert_eq!(controls[0].target(), 1.0);
        assert_eq!(volume(&app, plain), full(SoundCue::Impact));
    }
}
