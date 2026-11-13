//! The offline renderer: a scripted scene to the stereo signal the game would
//! play, for the loudness gate (`tests/offline_mix.rs`).
//!
//! A scene is a list of [`Frame`]s, one per 60 Hz tick: the cues requested in
//! that frame and the screen context the ambience follows. The render makes
//! the runtime's decisions with the runtime's code ([`crate::events`],
//! [`crate::select`], the mix table) and plays the shipped files.
//!
//! What it models of the Bevy adapter:
//!
//! - **Effects.** [`LegacyPolicy`] is the v1 voice policy of `horde_game`'s
//!   `SfxPlugin`: at most two starts per frame, a rotation that picks the
//!   variant, [`MAX_EFFECT_VOICES`] voices, and a more important cue cutting
//!   the least important voice at once. A voice plays its mono clip at
//!   `bus gain x row gain`, where the bus gain is
//!   `master x the bus's slider x calibration` of the cue's own bus
//!   ([`Sliders::bus_gain`]), into the two channels at the gains of its lane
//!   pan ([`pan::voice_gains`]; centred with [`Scene::mono`]).
//! - **Ambience.** The six loops play dual mono from the scene's first sample.
//!   Each follows `weight x master x ambience x context share x calibration`
//!   through the adapter's 1.2 s smoothstep crossfade, stepped per frame and
//!   ramped linearly inside a frame.
//!
//! What it leaves out: device resampling, and the frame of latency between a
//! request and its first sample. Clips at another rate than [`RATE`] (the
//! 22.05 kHz loops) are resampled linearly on load.

use crate::{
    bank::{AMBIENCE_FILES, variant_path},
    cue::Cue,
    events::{event_sound_at, push_cue},
    meter::{self, Loudness},
    mix::{Bus, CONTEXT_GAIN, MAX_EFFECT_VOICES, mix},
    pan::{self, Places},
    select::{CueGate, select_sounds},
    wav,
};
use horde_core::sim::{BattleId, Game, TICKS_PER_SECOND, smoke_input};
use std::{collections::BTreeMap, path::Path};

/// The render's sample rate, the rate of the shipped effect clips.
pub const RATE: u32 = 44_100;
/// Samples per frame: one 60 Hz tick.
pub const FRAME_SAMPLES: usize = (RATE / TICKS_PER_SECOND) as usize;
/// The adapter's ambience crossfade.
pub const CROSSFADE_SECONDS: f32 = 1.2;

/// Ambience weights of the stage-1 theme: the first stage of the country a
/// new journey starts in (DEU, stage 0), as `SceneAmbience::from_theme` gives
/// them. Pinned against the adapter by
/// `reference_weights_match_scene_ambience` in `horde_game`.
pub const REFERENCE_WEIGHTS: [f32; 6] = [0.28, 0.22, 0.0, 0.12, 0.0, 0.0];
/// The same theme in rain.
pub const RAIN_WEIGHTS: [f32; 6] = [0.28, 0.22, 0.0, 0.035, 0.38, 0.0];

/// The screen context the ambience follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Menu,
    Camp,
    Battle,
    Paused,
}
impl Context {
    fn level(self) -> f32 {
        match self {
            Self::Menu => CONTEXT_GAIN.menu,
            Self::Camp => CONTEXT_GAIN.camp,
            Self::Battle => CONTEXT_GAIN.battle,
            Self::Paused => CONTEXT_GAIN.paused,
        }
    }
}

/// One 60 Hz frame of a scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// The cues requested in this frame, before selection.
    pub cues: Vec<Cue>,
    /// Where the positional cues of this frame sound from; a cue without a
    /// place is centred.
    pub places: Places,
    pub context: Context,
}
impl Frame {
    /// A frame that requests `cues`, all centred.
    pub fn centred(cues: Vec<Cue>, context: Context) -> Self {
        Self {
            cues,
            places: Places::default(),
            context,
        }
    }
}

pub use crate::mix::Sliders;

/// What a render needs besides the frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scene {
    /// Ambience channel weights, in [`AMBIENCE_FILES`] order.
    pub weights: [f32; 6],
    pub sliders: Sliders,
    /// Reduced effects: the reduced gain and cooldown columns.
    pub reduced: bool,
    /// The "Mono audio" setting: every voice plays centred.
    pub mono: bool,
}

/// A rendered stereo signal at [`RATE`].
#[derive(Clone, Debug, PartialEq)]
pub struct Stereo {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}
impl Stereo {
    /// The signal's BS.1770-4 loudness and peaks.
    pub fn loudness(&self) -> Loudness {
        meter::measure(RATE, &[&self.left, &self.right])
    }
}

/// The shipped clips, decoded to mono at [`RATE`].
pub struct Library {
    clips: BTreeMap<String, Vec<f32>>,
    loops: [Vec<f32>; 6],
}
impl Library {
    /// Loads every runtime variant of every live cue and the ambience loops
    /// from `assets`.
    pub fn load(assets: &Path) -> Result<Self, String> {
        let read = |relative: &str, wrap: bool| -> Result<Vec<f32>, String> {
            let path = assets.join(relative);
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let clip = wav::read_pcm16(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(resample(&clip.mono(), clip.rate, RATE, wrap))
        };
        let mut clips = BTreeMap::new();
        for cue in Cue::ALL.into_iter().filter(|cue| !cue.retired()) {
            for variant in 0..cue.bank().variants() {
                let path = variant_path(cue, variant);
                if !clips.contains_key(&path) {
                    let samples = read(&path, false)?;
                    clips.insert(path, samples);
                }
            }
        }
        let mut loops: [Vec<f32>; 6] = Default::default();
        for (slot, path) in loops.iter_mut().zip(AMBIENCE_FILES) {
            *slot = read(path, true)?;
            if slot.is_empty() {
                return Err(format!("{path} is empty"));
            }
        }
        Ok(Self { clips, loops })
    }

    /// The samples `cue` plays for `variant` (variants wrap).
    pub fn clip(&self, cue: Cue, variant: u64) -> &[f32] {
        &self.clips[&variant_path(cue, variant)]
    }
}

/// Linear resampling. A looping clip interpolates its last sample towards its
/// first.
fn resample(samples: &[f32], from: u32, to: u32, wrap: bool) -> Vec<f32> {
    if from == to || samples.is_empty() {
        return samples.to_vec();
    }
    let count = (samples.len() as u64 * u64::from(to) / u64::from(from)) as usize;
    let step = f64::from(from) / f64::from(to);
    (0..count)
        .map(|index| {
            let position = index as f64 * step;
            let at = position as usize;
            let next = if at + 1 < samples.len() {
                samples[at + 1]
            } else if wrap {
                samples[0]
            } else {
                0.0
            };
            let fraction = (position - at as f64) as f32;
            samples[at] + (next - samples[at]) * fraction
        })
        .collect()
}

/// One voice to start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceStart {
    /// Unique per policy; a later [`FrameActions::stops`] names it.
    pub voice: u64,
    pub cue: Cue,
    /// The bank variant to play (variants wrap).
    pub variant: u64,
    pub priority: u8,
    /// The cue's row gain in the current effects mode. The executor multiplies
    /// in the gain of the cue's bus.
    pub gain: f32,
}

/// What the executor does this frame: stop first, then start.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameActions {
    /// Voices cut at once (v1 has no fade).
    pub stops: Vec<u64>,
    pub starts: Vec<VoiceStart>,
}

/// The v1 voice policy as a frame-actions state machine: the decisions of
/// `horde_game`'s `play_sounds`, on [`select_sounds`] and [`CueGate`].
#[derive(Debug, Default)]
pub struct LegacyPolicy {
    rotation: u64,
    gate: CueGate,
    next_voice: u64,
}
impl LegacyPolicy {
    /// One frame. `cues` are the frame's requests, `bus_gains` is
    /// [`Sliders::bus_gains`], and `live` lists the sounding voices as
    /// `(voice, priority)`. A cue whose bus is muted is dropped before
    /// selection: it takes no start slot and leaves its cooldown untouched.
    pub fn frame(
        &mut self,
        mut cues: Vec<Cue>,
        now_ms: u64,
        reduced: bool,
        bus_gains: &[f32; 5],
        live: &[(u64, u8)],
    ) -> FrameActions {
        let rotation = self.rotation;
        self.rotation = self.rotation.wrapping_add(u64::from(!cues.is_empty()));
        let mut actions = FrameActions::default();
        cues.retain(|cue| audible(*cue, bus_gains));
        let mut live = live.to_vec();
        for cue in select_sounds(cues, rotation) {
            let priority = mix(cue).priority;
            let replace = if live.len() >= MAX_EFFECT_VOICES {
                let Some((index, (_, least))) = live
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, (_, priority))| *priority)
                else {
                    continue;
                };
                if priority >= *least {
                    continue;
                }
                Some(index)
            } else {
                None
            };
            if !self.gate.accept(cue, now_ms, reduced) {
                continue;
            }
            if let Some(index) = replace {
                actions.stops.push(live.remove(index).0);
            }
            let voice = self.next_voice;
            self.next_voice += 1;
            actions.starts.push(VoiceStart {
                voice,
                cue,
                variant: rotation,
                priority,
                gain: mix(cue).level(reduced),
            });
            live.push((voice, priority));
        }
        actions
    }
}

/// Whether `cue`'s bus is audible at `bus_gains` ([`Sliders::bus_gains`]).
pub fn audible(cue: Cue, bus_gains: &[f32; 5]) -> bool {
    bus_gains[mix(cue).bus as usize] > 0.0
}

/// The adapter's ambience envelope: a smoothstep crossfade to the target
/// gains, restarted from the current gains when the target changes.
#[derive(Default)]
struct Envelope {
    current: [f32; 6],
    start: [f32; 6],
    target: [f32; 6],
    elapsed: f32,
}
impl Envelope {
    fn advance(&mut self, wanted: [f32; 6], muted: bool, delta: f32) -> [f32; 6] {
        if muted {
            *self = Self::default();
            return self.current;
        }
        if wanted != self.target {
            self.start = self.current;
            self.target = wanted;
            self.elapsed = 0.0;
        }
        self.elapsed = (self.elapsed + delta).min(CROSSFADE_SECONDS);
        let progress = (self.elapsed / CROSSFADE_SECONDS).clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        self.current = std::array::from_fn(|index| {
            self.start[index] + (self.target[index] - self.start[index]) * eased
        });
        self.current
    }
}

/// The weights as the adapter normalises them: clamped to 0..1 and scaled
/// down when they sum to more than 1.
fn normalized(mut weights: [f32; 6]) -> [f32; 6] {
    for weight in &mut weights {
        *weight = if weight.is_finite() {
            weight.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    let total: f32 = weights.iter().sum();
    if total > 1.0 {
        for weight in &mut weights {
            *weight /= total;
        }
    }
    weights
}

struct Voice<'a> {
    id: u64,
    priority: u8,
    clip: &'a [f32],
    position: usize,
    /// Left and right.
    gains: [f32; 2],
}

/// Renders `frames` at the scene's sliders and theme.
pub fn render(library: &Library, frames: &[Frame], scene: &Scene) -> Stereo {
    let bus_gains = scene.sliders.bus_gains();
    let ambience = bus_gains[Bus::Atmosphere as usize];
    let weights = normalized(scene.weights);
    // The dual-mono ambience; it joins both sides at the end.
    let mut out = vec![0.0_f32; frames.len() * FRAME_SAMPLES];
    // The voices' left and right.
    let mut sides = [out.clone(), out.clone()];
    let mut policy = LegacyPolicy::default();
    let mut voices: Vec<Voice> = Vec::new();
    let mut envelope = Envelope::default();
    let mut loop_gains = [0.0_f32; 6];
    for (index, frame) in frames.iter().enumerate() {
        let block = &mut out[index * FRAME_SAMPLES..(index + 1) * FRAME_SAMPLES];
        // Effects.
        voices.retain(|voice| voice.position < voice.clip.len());
        let live: Vec<(u64, u8)> = voices.iter().map(|v| (v.id, v.priority)).collect();
        let now_ms = index as u64 * 1000 / u64::from(TICKS_PER_SECOND);
        let actions = policy.frame(frame.cues.clone(), now_ms, scene.reduced, &bus_gains, &live);
        voices.retain(|voice| !actions.stops.contains(&voice.id));
        for start in actions.starts {
            voices.push(Voice {
                id: start.voice,
                priority: start.priority,
                clip: library.clip(start.cue, start.variant),
                position: 0,
                gains: pan::voice_gains(start.cue, &frame.places, scene.reduced, scene.mono)
                    .map(|side| side * bus_gains[mix(start.cue).bus as usize] * start.gain),
            });
        }
        for voice in &mut voices {
            let rest = &voice.clip[voice.position..];
            for (side, gain) in sides.iter_mut().zip(voice.gains) {
                let block = &mut side[index * FRAME_SAMPLES..(index + 1) * FRAME_SAMPLES];
                for (sample, source) in block.iter_mut().zip(rest) {
                    *sample += source * gain;
                }
            }
            voice.position += FRAME_SAMPLES.min(rest.len());
        }
        // Ambience.
        let wanted = weights.map(|weight| weight * ambience * frame.context.level());
        let next = envelope.advance(wanted, ambience == 0.0, 1.0 / TICKS_PER_SECOND as f32);
        for (channel, samples) in library.loops.iter().enumerate() {
            let (from, to) = (loop_gains[channel], next[channel]);
            if from == 0.0 && to == 0.0 {
                continue;
            }
            let offset = index * FRAME_SAMPLES;
            for (n, sample) in block.iter_mut().enumerate() {
                let gain = from + (to - from) * (n as f32 / FRAME_SAMPLES as f32);
                *sample += samples[(offset + n) % samples.len()] * gain;
            }
        }
        loop_gains = next;
    }
    let [mut left, mut right] = sides;
    for ((left, right), ambience) in left.iter_mut().zip(&mut right).zip(&out) {
        *left += ambience;
        *right += ambience;
    }
    Stereo { left, right }
}

/// The x in yards the fire bed sounds from: the first burning zone. `None`
/// when nothing burns.
pub fn fire_bed_x(zones: &[horde_core::ordnance::FireZone]) -> Option<f32> {
    zones
        .first()
        .map(|zone| horde_core::ordnance::cell_center(zone.cell)[0])
}

/// Ticks `game`'s battle `ticks` times with [`smoke_input`], one frame per
/// tick, and returns the cues each tick requests: the root battle's events
/// through [`event_sound`], plus the fire bed while a fire zone burns, as the
/// runner's `tick_sounds` does. A tick without an attempt is a paused frame.
pub fn battle_frames(game: &mut Game, ticks: u32) -> Vec<Frame> {
    (0..ticks)
        .map(|_| {
            let input = smoke_input(game);
            let in_battle = game.attempt().is_some();
            let width = game.attempt().map(|attempt| attempt.width_yards);
            game.tick(input);
            let mut cues = Vec::new();
            let mut places = Places::default();
            for record in game.drain_events() {
                if record.battle == BattleId::Root {
                    event_sound_at(&mut cues, &mut places, &record.event, width);
                }
            }
            if let Some(attempt) = game.attempt()
                && let Some(x) = fire_bed_x(&attempt.ordnance.fire_zones)
            {
                push_cue(&mut cues, Cue::Fire);
                places.place(Cue::Fire, pan::lane_position(x, attempt.width_yards));
            }
            Frame {
                cues,
                places,
                context: if in_battle {
                    Context::Battle
                } else {
                    Context::Paused
                },
            }
        })
        .collect()
}

/// Seconds of each part of [`reference_mixed_60s`].
pub const REFERENCE_CAMP_SECONDS: u32 = 8;
pub const REFERENCE_BATTLE_SECONDS: u32 = 44;
pub const REFERENCE_RESULTS_SECONDS: u32 = 8;

/// The reference scene of the loudness gate: 8 s of camp with two confirmed
/// purchases, 44 s of `battle` under [`smoke_input`], and 8 s of results on
/// the paused ambience. `battle` is a deployed, unpaused game (the gate uses
/// seed 1, Personal, level 0).
///
/// The results open with the battle's stinger: the one its last tick played,
/// or the victory stinger when the battle is still running after 44 s. The
/// loot reveal and the coin batch join the results when those cues exist.
pub fn reference_mixed_60s(battle: &mut Game) -> Vec<Frame> {
    let idle = |context| Frame::centred(Vec::new(), context);
    let mut frames =
        vec![idle(Context::Camp); (REFERENCE_CAMP_SECONDS * TICKS_PER_SECOND) as usize];
    for second in [2, 5] {
        frames[(second * TICKS_PER_SECOND) as usize]
            .cues
            .push(Cue::Confirm);
    }
    frames.extend(battle_frames(
        battle,
        REFERENCE_BATTLE_SECONDS * TICKS_PER_SECOND,
    ));
    let finished = frames
        .iter()
        .flat_map(|frame| &frame.cues)
        .any(|cue| matches!(cue, Cue::Victory | Cue::Defeat | Cue::Retreat));
    let results = frames.len();
    frames.extend(vec![
        idle(Context::Paused);
        (REFERENCE_RESULTS_SECONDS * TICKS_PER_SECOND) as usize
    ]);
    if !finished {
        frames[results].cues.push(Cue::Victory);
    }
    frames
}
