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
//! - **Effects.** [`VoicePolicy`] is the voice policy of `horde_game`'s
//!   `SfxPlugin`: the per-bus start budgets and the player-weapon lane, a
//!   per-bank round-robin that picks the variant and the jitter of the
//!   start's speed and gain ([`Variety`]), and the bus and family caps of
//!   [`crate::voices`], where a start at a cap fades its victim out over
//!   [`crate::voices::FADE_MS`]. A voice plays its mono clip at
//!   `bus gain x row gain`, where the bus gain is
//!   `master x the bus's slider x calibration` of the cue's own bus
//!   ([`Sliders::bus_gain`]), into the two channels at the gains of its lane
//!   pan ([`pan::voice_gains`]; centred with [`Scene::mono`]).
//! - **Ambience.** The six loops play dual mono from the scene's first sample.
//!   Each follows `weight x master x ambience x context share x calibration`
//!   through the adapter's 1.2 s smoothstep crossfade, stepped per frame and
//!   ramped linearly inside a frame.
//!
//! - **Ducking and density.** [Ducker] runs once a frame, as in the adapter:
//!   the ambience gains are multiplied by the Atmosphere duck after the
//!   crossfade, and a compensated voice ramps to the density gain through its
//!   [VoiceControl]. There are no music tracks to duck yet.
//!
//! What it leaves out: device resampling, and the frame of latency between a
//! request and its first sample. Clips at another rate than [`RATE`] (the
//! 22.05 kHz loops) are resampled linearly on load.

use crate::{
    bank::{AMBIENCE_FILES, BankId},
    cue::Cue,
    cue_log::CueLogFrame,
    duck::Ducker,
    events::{EventContext, event_requests, push_cue},
    gun::GunTracker,
    meter::{self, Loudness},
    mix::{Bus, CONTEXT_GAIN, mix},
    pan::{self, Places},
    select::{CueGate, Jitter, Variety, select_starts},
    voices::{Admission, LiveVoice, Ramp, VoiceControl, admit},
    voicing::{EnemyVoiceIndex, Voicing, lowpassed},
    wav,
};
use horde_core::sim::{BattleId, Game, TICKS_PER_SECOND, smoke_input};
use std::{borrow::Cow, collections::BTreeMap, path::Path};

/// The render's sample rate, the rate of the shipped effect clips.
pub const RATE: u32 = 44_100;
/// Samples per frame: one 60 Hz tick.
pub const FRAME_SAMPLES: usize = (RATE / TICKS_PER_SECOND) as usize;
/// One frame in the cue log's microseconds, rounded up: the last frame of a
/// render ends on the render's last sample.
const FRAME_MICROS: u64 = 1_000_000_u64.div_ceil(TICKS_PER_SECOND as u64);
/// The adapter's ambience crossfade.
pub const CROSSFADE_SECONDS: f32 = 1.2;

/// Ambience weights of the stage-1 theme: the first stage of the country a
/// new journey starts in (DEU, stage 0), as `SceneAmbience::from_theme` gives
/// them. Pinned against the adapter by
/// `reference_weights_match_scene_ambience` in `horde_game`.
pub const REFERENCE_WEIGHTS: [f32; 6] = [0.28, 0.22, 0.0, 0.12, 0.0, 0.0];
/// The same theme in rain. The rain loop is broadband and reads loud for its
/// weight: at 0.10 the rain bed sits about 1.5 LU over the dry one (it was 9 LU
/// over at 0.38), so both themes fit the loudness gate.
pub const RAIN_WEIGHTS: [f32; 6] = [0.28, 0.22, 0.0, 0.035, 0.10, 0.0];

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
    /// Loads every variant of every shipped bank and the ambience loops
    /// from `assets`.
    pub fn load(assets: &Path) -> Result<Self, String> {
        let read = |relative: &str, wrap: bool| -> Result<Vec<f32>, String> {
            let path = assets.join(relative);
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let clip = wav::read_pcm16(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(resample(&clip.mono(), clip.rate, RATE, wrap))
        };
        let mut clips = BTreeMap::new();
        for bank in BankId::shipped() {
            for variant in 0..bank.variants() {
                let path = bank.variant_path(variant);
                let samples = read(&path, false)?;
                clips.insert(path, samples);
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

    /// The samples `cue`'s own bank plays for `variant` (variants wrap).
    pub fn clip(&self, cue: Cue, variant: u64) -> &[f32] {
        self.bank_clip(cue.bank(), variant)
    }
    /// The samples of the clip at `path`, relative to the asset folder, as a
    /// cue log names it.
    pub fn path_clip(&self, path: &str) -> Option<&[f32]> {
        self.clips.get(path).map(Vec::as_slice)
    }
    /// The samples of `bank`'s `variant` (variants wrap).
    pub fn bank_clip(&self, bank: BankId, variant: u64) -> &[f32] {
        &self.clips[&bank.variant_path(variant)]
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

/// `samples` played at `speed`, as the runtime's sink plays a jittered voice:
/// read by linear interpolation at `speed` source samples per output sample.
pub fn pitched(samples: &[f32], speed: f32) -> Vec<f32> {
    if speed == 1.0 || samples.is_empty() {
        return samples.to_vec();
    }
    let step = f64::from(speed);
    let count = (samples.len() as f64 / step) as usize;
    (0..count)
        .map(|index| {
            let position = index as f64 * step;
            let at = (position as usize).min(samples.len() - 1);
            let next = samples.get(at + 1).copied().unwrap_or(0.0);
            samples[at] + (next - samples[at]) * (position - at as f64) as f32
        })
        .collect()
}

/// One voice to start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceStart {
    /// Unique per policy; a later [`FrameActions::fades`] names it.
    pub voice: u64,
    pub cue: Cue,
    /// The bank and tone of the request: the executor plays `voicing.bank`
    /// at `voicing.speed` times the jitter's speed, through its low-pass,
    /// and multiplies in its gain.
    pub voicing: Voicing,
    /// The variant of `voicing.bank` to play: the bank's round-robin pick.
    pub variant: u64,
    pub priority: u8,
    /// The cue's row gain in the current effects mode. The executor multiplies
    /// in the gain of the cue's bus and the jitter's gain.
    pub gain: f32,
    /// The start's playback speed and gain factor.
    pub jitter: Jitter,
    /// The voice takes the density gain ([`crate::duck::compensated`]).
    pub compensated: bool,
}

/// What the executor does this frame: fade first, then start.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameActions {
    /// Stolen voices: each fades out over [`crate::voices::FADE_MS`] and
    /// ends. A fading voice is no longer live.
    pub fades: Vec<u64>,
    pub starts: Vec<VoiceStart>,
}

/// The voice policy as a frame-actions state machine: the decisions of
/// `horde_game`'s `play_sounds`, on [`select_starts`], [`admit`] and
/// [`CueGate`].
#[derive(Debug, Default)]
pub struct VoicePolicy {
    /// Counts the frames with requests: fire sources take turns by it.
    rotation: u64,
    gate: CueGate,
    variety: Variety,
    next_voice: u64,
}
impl VoicePolicy {
    /// One frame without a player shot: [`Self::frame_requests`].
    pub fn frame(
        &mut self,
        cues: Vec<Cue>,
        now_ms: u64,
        reduced: bool,
        bus_gains: &[f32; 5],
        live: &[LiveVoice],
    ) -> FrameActions {
        self.frame_requests(cues, None, now_ms, reduced, bus_gains, live)
    }

    /// [`Self::frame_places`] for requests that all play their own banks as
    /// mastered.
    pub fn frame_requests(
        &mut self,
        cues: Vec<Cue>,
        player_shot: Option<Cue>,
        now_ms: u64,
        reduced: bool,
        bus_gains: &[f32; 5],
        live: &[LiveVoice],
    ) -> FrameActions {
        let mut places = Places::default();
        if let Some(shot) = player_shot {
            places.mark_player_shot(shot);
        }
        self.frame_places(cues, &places, now_ms, reduced, bus_gains, live)
    }

    /// One frame. `cues` are the frame's requests and `places` holds their
    /// voicings and the cue of
    /// the root defender's own shot (the player-weapon lane), `bus_gains` is
    /// [`Sliders::bus_gains`], and `live` lists the sounding voices, without
    /// the fading ones. A cue whose bus is muted is dropped before
    /// selection: it takes no start slot and leaves its cooldown untouched.
    /// So does a start the voice caps refuse.
    pub fn frame_places(
        &mut self,
        mut cues: Vec<Cue>,
        places: &Places,
        now_ms: u64,
        reduced: bool,
        bus_gains: &[f32; 5],
        live: &[LiveVoice],
    ) -> FrameActions {
        let player_shot = places.player_shot();
        let rotation = self.rotation;
        self.rotation = self
            .rotation
            .wrapping_add(u64::from(!cues.is_empty() || player_shot.is_some()));
        let mut actions = FrameActions::default();
        cues.retain(|cue| audible(*cue, bus_gains));
        let player_shot = player_shot.filter(|cue| audible(*cue, bus_gains));
        let mut live = live.to_vec();
        for start in select_starts(cues, player_shot, rotation) {
            let cue = start.cue;
            let priority = mix(cue).priority;
            let admission = admit(start, &live);
            if admission == Admission::Refuse || !self.gate.accept_start(start, now_ms, reduced) {
                continue;
            }
            if let Admission::Steal(victim) = admission {
                live.retain(|voice| voice.id != victim);
                actions.fades.push(victim);
            }
            let voice = self.next_voice;
            self.next_voice += 1;
            // Only a start that plays advances its bank.
            let voicing = places.voicing(cue);
            let pick = self.variety.pick(voicing.bank, mix(cue).jitter);
            actions.starts.push(VoiceStart {
                voice,
                cue,
                voicing,
                variant: pick.variant,
                priority,
                gain: mix(cue).level(reduced),
                jitter: pick.jitter,
                compensated: crate::duck::compensated(start),
            });
            live.push(LiveVoice {
                id: voice,
                cue,
                started_ms: now_ms,
            });
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

/// A sounding clip, as the executor mixes it: shared by the render and the
/// cue-log replay ([`crate::cue_log::replay`]).
pub(crate) struct Playing<'a> {
    /// The fade of a stolen voice, as the runtime's decoder runs it.
    pub(crate) control: VoiceControl,
    ramp: Ramp,
    /// The clip at the start's speed, through its low-pass.
    clip: Cow<'a, [f32]>,
    position: usize,
    /// Left and right.
    gains: [f32; 2],
    /// The voice follows the density gain.
    compensated: bool,
}
impl<'a> Playing<'a> {
    /// `clip` from its first sample at `speed`, through the low-pass, at the
    /// `[left, right]` gains.
    pub(crate) fn start(
        clip: &'a [f32],
        speed: f32,
        lowpass_hz: Option<f32>,
        gains: [f32; 2],
        compensated: bool,
    ) -> Self {
        let clip = if speed == 1.0 {
            Cow::Borrowed(clip)
        } else {
            Cow::Owned(pitched(clip, speed))
        };
        Self {
            control: VoiceControl::default(),
            ramp: Ramp::new(RATE),
            clip: match lowpass_hz {
                Some(cutoff) => Cow::Owned(lowpassed(&clip, cutoff, RATE)),
                None => clip,
            },
            position: 0,
            gains,
            compensated,
        }
    }
    pub(crate) fn ended(&self) -> bool {
        self.position >= self.clip.len()
    }
    /// The frame's density gain: a compensated voice ramps to it. A fading
    /// voice keeps its way to silence.
    pub(crate) fn follow_density(&self, density: f32) {
        if self.compensated && !self.control.stopping() {
            self.control.set_target(density);
        }
    }
    /// Mixes the next `count` samples into `sides` from `offset`.
    pub(crate) fn mix_into(&mut self, sides: &mut [Vec<f32>; 2], offset: usize, count: usize) {
        let rest = &self.clip[self.position..];
        let mut played = 0;
        for (n, source) in rest.iter().take(count).enumerate() {
            let Some(ramp) = self.ramp.next(&self.control) else {
                // The fade is over: the voice ends here.
                played = rest.len();
                break;
            };
            for (side, gain) in sides.iter_mut().zip(self.gains) {
                side[offset + n] += source * gain * ramp;
            }
            played = n + 1;
        }
        self.position += played;
    }
}

/// Adds the six loops to `block`, the samples from `offset` on, each ramped
/// linearly from its gain in `from` to its gain in `to`.
pub(crate) fn mix_loops(
    library: &Library,
    block: &mut [f32],
    offset: usize,
    from: [f32; 6],
    to: [f32; 6],
) {
    let length = block.len() as f32;
    for (channel, samples) in library.loops.iter().enumerate() {
        let (from, to) = (from[channel], to[channel]);
        if from == 0.0 && to == 0.0 {
            continue;
        }
        for (n, sample) in block.iter_mut().enumerate() {
            let gain = from + (to - from) * (n as f32 / length);
            *sample += samples[(offset + n) % samples.len()] * gain;
        }
    }
}

/// The voices' two sides plus the dual-mono ambience.
pub(crate) fn joined(sides: [Vec<f32>; 2], ambience: &[f32]) -> Stereo {
    let [mut left, mut right] = sides;
    for ((left, right), ambience) in left.iter_mut().zip(&mut right).zip(ambience) {
        *left += ambience;
        *right += ambience;
    }
    Stereo { left, right }
}

struct Voice<'a> {
    id: u64,
    cue: Cue,
    started_ms: u64,
    playing: Playing<'a>,
}

/// Renders `frames` at the scene's sliders and theme.
pub fn render(library: &Library, frames: &[Frame], scene: &Scene) -> Stereo {
    render_logged(library, frames, scene).0
}

/// [`render`], and the cue log of what the executor did each frame: the log
/// the game's review service writes for a real session
/// ([`crate::cue_log`]). Replaying the log gives the same signal.
pub fn render_logged(
    library: &Library,
    frames: &[Frame],
    scene: &Scene,
) -> (Stereo, Vec<CueLogFrame>) {
    let bus_gains = scene.sliders.bus_gains();
    let ambience = bus_gains[Bus::Atmosphere as usize];
    let weights = normalized(scene.weights);
    // The dual-mono ambience; it joins both sides at the end.
    let mut out = vec![0.0_f32; frames.len() * FRAME_SAMPLES];
    // The voices' left and right.
    let mut sides = [out.clone(), out.clone()];
    let mut policy = VoicePolicy::default();
    let mut ducker = Ducker::default();
    let mut voices: Vec<Voice> = Vec::new();
    let mut envelope = Envelope::default();
    let mut loop_gains = [0.0_f32; 6];
    let mut log = Vec::with_capacity(frames.len());
    // The sounding voices; a fading one has given up its slot.
    let live_of = |voices: &[Voice]| -> Vec<LiveVoice> {
        voices
            .iter()
            .filter(|voice| !voice.playing.control.stopping())
            .map(|voice| LiveVoice {
                id: voice.id,
                cue: voice.cue,
                started_ms: voice.started_ms,
            })
            .collect()
    };
    for (index, frame) in frames.iter().enumerate() {
        let offset = index * FRAME_SAMPLES;
        // Effects.
        voices.retain(|voice| !voice.playing.ended());
        let live = live_of(&voices);
        let now_ms = index as u64 * 1000 / u64::from(TICKS_PER_SECOND);
        let actions = policy.frame_places(
            frame.cues.clone(),
            &frame.places,
            now_ms,
            scene.reduced,
            &bus_gains,
            &live,
        );
        for voice in &voices {
            if actions.fades.contains(&voice.id) {
                voice.playing.control.fade_out();
            }
        }
        // The `[left, right]` gains a start plays at.
        let gains_of = |start: &VoiceStart| {
            pan::voice_gains(start.cue, &frame.places, scene.reduced, scene.mono).map(|side| {
                side * bus_gains[mix(start.cue).bus as usize]
                    * start.gain
                    * start.jitter.gain
                    * start.voicing.gain()
            })
        };
        for start in &actions.starts {
            ducker.trigger(start.cue);
            voices.push(Voice {
                id: start.voice,
                cue: start.cue,
                started_ms: now_ms,
                playing: Playing::start(
                    library.bank_clip(start.voicing.bank, start.variant),
                    start.jitter.speed * start.voicing.speed,
                    start.voicing.lowpass_hz,
                    gains_of(start),
                    start.compensated,
                ),
            });
        }
        // Ducking and density, on the voices that sound from this frame on.
        let ducks = ducker.advance(1.0 / TICKS_PER_SECOND as f32, &live_of(&voices));
        for voice in &mut voices {
            voice.playing.follow_density(ducks.density);
            voice.playing.mix_into(&mut sides, offset, FRAME_SAMPLES);
        }
        // Ambience.
        let wanted = weights.map(|weight| weight * ambience * frame.context.level());
        // The duck multiplies the crossfaded gains: it never restarts the
        // crossfade.
        let next = envelope
            .advance(wanted, ambience == 0.0, 1.0 / TICKS_PER_SECOND as f32)
            .map(|gain| gain * ducks.duck[Bus::Atmosphere as usize]);
        mix_loops(
            library,
            &mut out[offset..offset + FRAME_SAMPLES],
            offset,
            loop_gains,
            next,
        );
        loop_gains = next;
        log.push(CueLogFrame::from_actions(
            index as u64 * 1_000_000 / u64::from(TICKS_PER_SECOND),
            FRAME_MICROS,
            &actions,
            gains_of,
            next,
            ducks.density,
        ));
    }
    (joined(sides, &out), log)
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
/// through [`event_requests`], plus the fire bed while a fire zone burns, as
/// the runner's `tick_sounds` does, then the gun's handling through the
/// [`GunTracker`], as the runner's `run_tick` does. A tick without an
/// attempt is a paused frame.
pub fn battle_frames(game: &mut Game, ticks: u32) -> Vec<Frame> {
    let mut voices = EnemyVoiceIndex::default();
    let mut guns = GunTracker::default();
    (0..ticks)
        .map(|_| {
            let input = smoke_input(game);
            let in_battle = game.attempt().is_some();
            let width = game.attempt().map(|attempt| attempt.width_yards);
            // Before the tick, so this tick's kills still have their voices.
            if let Some(attempt) = game.attempt() {
                voices.sync(BattleId::Root, attempt);
            }
            game.tick(input.clone());
            if let Some(attempt) = game.attempt() {
                voices.sync(BattleId::Root, attempt);
            }
            let mut cues = Vec::new();
            let mut places = Places::default();
            let records: Vec<_> = game.drain_events().collect();
            let ctx = EventContext {
                battle: BattleId::Root,
                width_yards: width,
                distant: false,
                corpses: game
                    .attempt()
                    .map_or(&[], |attempt| attempt.corpses.as_slice()),
            };
            for record in records {
                if record.battle == BattleId::Root {
                    event_requests(&mut cues, &mut places, &mut voices, &record.event, &ctx);
                }
            }
            if let Some(attempt) = game.attempt()
                && let Some(x) = fire_bed_x(&attempt.ordnance.fire_zones)
            {
                push_cue(&mut cues, Cue::Fire);
                places.place(Cue::Fire, pan::lane_position(x, attempt.width_yards));
            }
            if let Some(attempt) = game.attempt() {
                guns.observe(BattleId::Root, attempt, &input, &mut cues, &mut places);
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
