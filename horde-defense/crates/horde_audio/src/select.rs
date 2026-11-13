//! Cue selection: the repeat gate and the per-frame choice of which requested
//! cues start. The runtime and the offline renderer decide with the same
//! code.
//!
//! A frame may start [`START_BUDGET`] cues per bus (Effects 3, UI 2, Music
//! 1), plus the root defender's own shot on the reserved player-weapon lane
//! ([`Lane::PlayerWeapon`]): that shot takes no slot of any budget and starts
//! whenever its own gate passes.
//!
//! Variety: each bank keeps a [`RoundRobin`] and a trigger counter
//! ([`Variety`]). A start's variant and its [`Jitter`] come from a hash of
//! the bank's ordinal and that counter, so they depend on how often the bank
//! has played and on nothing of the game: no game state or simulation
//! randomness is read.

use crate::{
    bank::BankId,
    cue::{CUE_COUNT, Cue},
    mix::{Bus, GateGroup, JitterBounds, db_to_linear, mix},
};

/// Cues that may start in one frame on each bus, indexed by [`Bus`]. Master
/// and Atmosphere carry no cue voices.
pub const START_BUDGET: [usize; 5] = [0, 3, 2, 1, 0];

/// The start budget of `bus`.
pub const fn start_budget(bus: Bus) -> usize {
    START_BUDGET[bus as usize]
}

/// `(by, cue)`: when a frame requests `by`, its `cue` is dropped. A warning
/// already communicates arrival; a kill subsumes the hit that caused it; a
/// defeat stinger the collapse that ended the defense, and a victory stinger
/// the wave warning (each pair shares a bank). Applied in order.
pub const SUBSUMES: &[(Cue, Cue)] = &[
    (Cue::Wave, Cue::Incoming),
    (Cue::Kill, Cue::Impact),
    (Cue::EnemyDeath, Cue::Impact),
    (Cue::Defeat, Cue::Structure),
    (Cue::Victory, Cue::Wave),
];

/// How a selected cue got its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Inside the start budget of the cue's bus.
    Bus,
    /// The reserved lane of the root defender's own shot.
    PlayerWeapon,
}

/// One cue selected to start this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start {
    pub cue: Cue,
    pub lane: Lane,
}
impl Start {
    pub const fn bus(cue: Cue) -> Self {
        Self {
            cue,
            lane: Lane::Bus,
        }
    }
    pub const fn player(cue: Cue) -> Self {
        Self {
            cue,
            lane: Lane::PlayerWeapon,
        }
    }
}

/// Coalesce repeated ticks and accelerated observation into a restrained real-
/// time soundscape. A cue dropped by the gate never delays a simulation action.
#[derive(Debug)]
pub struct CueGate {
    /// By [`Cue::gate_key`]: cues that share a gate share a slot.
    last_started: [Option<u64>; CUE_COUNT],
    /// The last start on the player-weapon lane.
    last_player: Option<(Cue, u64)>,
}
impl Default for CueGate {
    fn default() -> Self {
        Self {
            last_started: [None; CUE_COUNT],
            last_player: None,
        }
    }
}
impl CueGate {
    pub fn accept(&mut self, cue: Cue, now_ms: u64, reduced: bool) -> bool {
        let last = &mut self.last_started[cue.gate_key() as usize];
        if last.is_some_and(|previous| held(cue, previous, now_ms, reduced)) {
            return false;
        }
        *last = Some(now_ms);
        true
    }
    /// The gate of the player-weapon lane: the cue's cooldown against the
    /// player's own last shot with it, whoever else played the cue since. An
    /// accepted shot also starts the cue's shared cooldown.
    pub fn accept_player(&mut self, cue: Cue, now_ms: u64, reduced: bool) -> bool {
        if self
            .last_player
            .is_some_and(|(last, previous)| last == cue && held(cue, previous, now_ms, reduced))
        {
            return false;
        }
        self.last_player = Some((cue, now_ms));
        self.last_started[cue.gate_key() as usize] = Some(now_ms);
        true
    }
    /// The gate of `start`'s lane.
    pub fn accept_start(&mut self, start: Start, now_ms: u64, reduced: bool) -> bool {
        match start.lane {
            Lane::Bus => self.accept(start.cue, now_ms, reduced),
            Lane::PlayerWeapon => self.accept_player(start.cue, now_ms, reduced),
        }
    }
}

/// Whether `cue`, last started at `previous`, is still inside its cooldown.
fn held(cue: Cue, previous: u64, now_ms: u64, reduced: bool) -> bool {
    now_ms >= previous && now_ms - previous < mix(cue.gate_key()).cooldown(reduced)
}

fn rank(cue: Cue) -> u8 {
    mix(cue).priority
}
fn fires(cue: Cue) -> bool {
    mix(cue).gate == GateGroup::Fire
}

/// The starts of one frame's requests, most important first.
///
/// `sounds` are the requested cues and `player_shot` the cue of the root
/// defender's own shot this frame, if he fired (it need not be in `sounds`).
/// `rotation` counts the frames that had requests, so fire sources take
/// turns.
///
/// The policy: drop what [`SUBSUMES`] names, keep each cue once, order by
/// `(priority, request order)`. Critical cues (priority <= 3) go first; when
/// there is none, one non-fire cue of priority <= 8; then the fire cues,
/// rotated; then the rest. Each cue starts while its bus has budget left.
/// The player's shot rides its own lane after the critical cues.
pub fn select_starts(mut sounds: Vec<Cue>, player_shot: Option<Cue>, rotation: u64) -> Vec<Start> {
    for (by, cue) in SUBSUMES {
        if sounds.contains(by) {
            sounds.retain(|sound| sound != cue);
        }
    }
    // The player's shot rides its own lane, once.
    if let Some(shot) = player_shot {
        sounds.retain(|sound| *sound != shot);
    }
    let mut seen = [false; CUE_COUNT];
    sounds.retain(|cue| !std::mem::replace(&mut seen[*cue as usize], true));
    sounds.sort_by_key(|cue| rank(*cue));
    let mut left = START_BUDGET;
    let mut selected: Vec<Cue> = Vec::with_capacity(START_BUDGET.iter().sum());
    let mut admit = |selected: &mut Vec<Cue>, cue: Cue| {
        let slots = &mut left[mix(cue).bus as usize];
        if *slots > 0 && !selected.contains(&cue) {
            *slots -= 1;
            selected.push(cue);
        }
    };
    for cue in sounds.iter().copied().filter(|cue| rank(*cue) <= 3) {
        admit(&mut selected, cue);
    }
    let critical = selected.len();
    if selected.is_empty()
        && let Some(cue) = sounds
            .iter()
            .copied()
            .find(|cue| !fires(*cue) && rank(*cue) <= 8)
    {
        admit(&mut selected, cue);
    }
    let fire: Vec<_> = sounds.iter().copied().filter(|cue| fires(*cue)).collect();
    if !fire.is_empty() {
        let first = (rotation % fire.len() as u64) as usize;
        for n in 0..fire.len() {
            admit(&mut selected, fire[(first + n) % fire.len()]);
        }
    }
    for cue in sounds {
        admit(&mut selected, cue);
    }
    let mut starts: Vec<Start> = selected.into_iter().map(Start::bus).collect();
    if let Some(shot) = player_shot {
        starts.insert(critical, Start::player(shot));
    }
    starts
}

/// The cues of [`select_starts`] for a frame without a player shot.
pub fn select_sounds(sounds: Vec<Cue>, rotation: u64) -> Vec<Cue> {
    select_starts(sounds, None, rotation)
        .into_iter()
        .map(|start| start.cue)
        .collect()
}

/// The SplitMix64 step: a counter in, 64 well-mixed bits out.
pub const fn splitmix64(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// One bank's variant rotation: the next variant is never the last one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundRobin {
    /// The variant played last; `u8::MAX` before the first (and after load).
    last: u8,
}
impl Default for RoundRobin {
    fn default() -> Self {
        Self { last: u8::MAX }
    }
}
impl RoundRobin {
    /// The next of `variants` variants for the hash `h`:
    /// `(last + 1 + h % (n - 1)) % n`, which skips `last` for `n >= 2`. The
    /// first pick is `h % n`.
    pub const fn next(&mut self, variants: u64, h: u64) -> u64 {
        let last = self.last as u64;
        let next = if variants <= 1 {
            0
        } else if last >= variants {
            h % variants
        } else {
            (last + 1 + h % (variants - 1)) % variants
        };
        self.last = next as u8;
        next
    }
}

/// How one start strays from its clip: both are linear factors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jitter {
    /// The playback speed.
    pub speed: f32,
    /// Multiplies the voice's gain.
    pub gain: f32,
}
impl Jitter {
    pub const NONE: Self = Self {
        speed: 1.0,
        gain: 1.0,
    };
}

/// 24 bits of a hash as a point in -1..=1.
fn unit(bits: u64) -> f32 {
    const MAX: u64 = (1 << 24) - 1;
    (bits & MAX) as f32 / MAX as f32 * 2.0 - 1.0
}

/// The jitter inside `bounds` for the hash `h`: the speed from bits 16-39,
/// the gain from bits 40-63 (the variant reads `h` whole, modulo a small
/// count).
pub fn jitter(bounds: JitterBounds, h: u64) -> Jitter {
    Jitter {
        speed: 1.0 + bounds.speed * unit(h >> 16),
        gain: db_to_linear(bounds.gain_db * unit(h >> 40)),
    }
}

/// Round-robin slots: one per bank ordinal.
pub const BANK_SLOTS: usize = 64;

/// What one start of a bank plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    pub variant: u64,
    pub jitter: Jitter,
}

/// The variety state: per bank, the rotation and how often it has played.
/// Presentation only; it is not saved and starts over on load.
#[derive(Clone, Debug)]
pub struct Variety {
    banks: [RoundRobin; BANK_SLOTS],
    counters: [u64; BANK_SLOTS],
}
impl Default for Variety {
    fn default() -> Self {
        Self {
            banks: [RoundRobin::default(); BANK_SLOTS],
            counters: [0; BANK_SLOTS],
        }
    }
}
impl Variety {
    /// The next start of `bank`: its variant and its jitter inside `bounds`,
    /// from `splitmix64((ordinal << 32) ^ the bank's trigger count)`.
    pub fn pick(&mut self, bank: BankId, bounds: JitterBounds) -> Pick {
        let ordinal = bank.ordinal();
        let slot = ordinal as usize;
        let counter = self.counters[slot];
        self.counters[slot] = counter.wrapping_add(1);
        let h = splitmix64((ordinal << 32) ^ counter);
        Pick {
            variant: self.banks[slot].next(bank.variants(), h),
            jitter: jitter(bounds, h),
        }
    }
    /// The next start of `cue`: its bank, at its mix row's jitter group.
    pub fn pick_cue(&mut self, cue: Cue) -> Pick {
        self.pick(cue.bank(), mix(cue).jitter)
    }
}
