//! The music director: which music state the screen and the battle ask for,
//! the track slot of each state and the crossfade a track plays on.
//!
//! No music files ship (decision 9): every slot of [`TRACKS`] is
//! [`Track::Deferred`], so the director plays no track. What the Music bus
//! plays today is the attempt [`STINGERS`], which are existing survival
//! banks. `Track::File` playback is decided here ([`MusicDirector::voices`])
//! so a later track only fills a slot.
//!
//! The director is presentation: it reads a few numbers of the shown battle
//! and real time, never the core RNG, and changes no game state.

use crate::cue::Cue;

/// The screen, as far as music cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicScreen {
    Menu,
    Camp,
    /// The deployment briefing: scored as the camp.
    Briefing,
    Battle,
    Results,
    /// The upgrades home after an attempt: scored as the results.
    Upgrades,
    /// A screen laid over the others (Settings, History): what was playing
    /// goes on.
    Overlay,
}

/// What the director reads of the shown battle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BattleInput {
    pub enemies_alive: u32,
    /// The lane position of the nearest living enemy: 0 at the defender, 1
    /// at the far end (and 1 with no enemy on the field).
    pub nearest_x_frac: f32,
    /// The defender's health, 0..=1.
    pub hp_frac: f32,
    pub boss_alive: bool,
    pub paused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MusicInput {
    pub screen: MusicScreen,
    /// The battle on the battle screen, if one runs.
    pub battle: Option<BattleInput>,
}
impl Default for MusicInput {
    fn default() -> Self {
        Self {
            screen: MusicScreen::Menu,
            battle: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MusicState {
    #[default]
    Silent,
    Menu,
    Camp,
    BattleLow,
    BattleHigh,
    Boss,
    Results,
}
impl MusicState {
    pub const ALL: [Self; 7] = [
        Self::Silent,
        Self::Menu,
        Self::Camp,
        Self::BattleLow,
        Self::BattleHigh,
        Self::Boss,
        Self::Results,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Silent => "Silent",
            Self::Menu => "Menu",
            Self::Camp => "Camp",
            Self::BattleLow => "BattleLow",
            Self::BattleHigh => "BattleHigh",
            Self::Boss => "Boss",
            Self::Results => "Results",
        }
    }
}

/// What a music state plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track {
    /// No file: the slot is silent. The text says why.
    Deferred(&'static str),
    /// A looping file, relative to `assets/`.
    File(&'static str),
}

/// Why every track slot is empty.
pub const DEFERRED_REASON: &str =
    "music tracks deferred: no new music files (decision 9, user 2026-11-04)";

/// The track slot of every music state. Every slot is deferred
/// (FU-AUD-MUSIC: 7 track slots deferred, no files, no AI music).
pub const TRACKS: [(MusicState, Track); 7] = [
    (MusicState::Silent, Track::Deferred(DEFERRED_REASON)),
    (MusicState::Menu, Track::Deferred(DEFERRED_REASON)),
    (MusicState::Camp, Track::Deferred(DEFERRED_REASON)),
    (MusicState::BattleLow, Track::Deferred(DEFERRED_REASON)),
    (MusicState::BattleHigh, Track::Deferred(DEFERRED_REASON)),
    (MusicState::Boss, Track::Deferred(DEFERRED_REASON)),
    (MusicState::Results, Track::Deferred(DEFERRED_REASON)),
];

/// The slot of `state` in [`TRACKS`].
pub fn track(state: MusicState) -> Track {
    track_in(&TRACKS, state)
}
fn track_in(tracks: &[(MusicState, Track)], state: MusicState) -> Track {
    tracks
        .iter()
        .find(|(slot, _)| *slot == state)
        .map_or(Track::Deferred(DEFERRED_REASON), |(_, track)| *track)
}

/// The attempt stingers: the cues of the Music bus. Each plays an existing
/// survival bank (Victory the wave bank, Defeat the structure bank, Retreat
/// its own); `events::cues_of` requests them when an attempt finishes.
pub const STINGERS: [Cue; 3] = [Cue::Victory, Cue::Defeat, Cue::Retreat];

/// The low battle state goes high once the intensity held at this or more
/// for [`HIGH_HOLD_MS`].
pub const HIGH_AT: f32 = 0.60;
pub const HIGH_HOLD_MS: u32 = 2000;
/// The high battle state returns once the intensity held at this or less
/// for [`LOW_HOLD_MS`].
pub const LOW_AT: f32 = 0.35;
pub const LOW_HOLD_MS: u32 = 4000;
/// The gain of the tracks while the battle is paused.
pub const PAUSED_GAIN: f32 = 0.35;
/// A state change crossfades the two tracks over this long, on a smoothstep.
pub const CROSSFADE_MS: u32 = 2000;
/// The enemy count at which the crowd's share of the intensity is full.
pub const FULL_CROWD: u32 = 40;

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn smoothstep(progress: f32) -> f32 {
    let p = unit(progress);
    p * p * (3.0 - 2.0 * p)
}

/// How hard the battle presses, 0..=1: the crowd, how near it is and how
/// hurt the defender is.
pub fn intensity(battle: &BattleInput) -> f32 {
    let crowd = (battle.enemies_alive.min(FULL_CROWD) as f32) / FULL_CROWD as f32;
    unit(
        0.45 * crowd
            + 0.35 * (1.0 - unit(battle.nearest_x_frac))
            + 0.2 * (1.0 - unit(battle.hp_frac)),
    )
}

/// One track the director wants playing, at `gain` (before the Music bus
/// gain and the Music duck).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackVoice {
    pub path: &'static str,
    pub gain: f32,
}

/// The director's state machine. Feed it the input and the real time of
/// every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MusicDirector {
    state: MusicState,
    /// How long the intensity has held on the far side of the threshold
    /// that changes the battle state.
    hold_ms: u32,
    /// The state the crossfade leaves, and its track's level when it began.
    from: MusicState,
    from_level: f32,
    /// Time since the last state change, up to [`CROSSFADE_MS`].
    fade_ms: u32,
    paused: bool,
    intensity: f32,
}
impl Default for MusicDirector {
    fn default() -> Self {
        Self {
            state: MusicState::Silent,
            hold_ms: 0,
            from: MusicState::Silent,
            from_level: 0.0,
            fade_ms: CROSSFADE_MS,
            paused: false,
            intensity: 0.0,
        }
    }
}
impl MusicDirector {
    pub fn state(&self) -> MusicState {
        self.state
    }
    /// The battle intensity of the last update; 0 outside a battle.
    pub fn intensity(&self) -> f32 {
        self.intensity
    }
    /// The gain of the tracks in the current context: lower while the
    /// battle is paused.
    pub fn context_gain(&self) -> f32 {
        if self.paused { PAUSED_GAIN } else { 1.0 }
    }

    /// Advances by `dt_ms` of real time and returns the state.
    pub fn update(&mut self, input: &MusicInput, dt_ms: u32) -> MusicState {
        let next = match input.screen {
            MusicScreen::Overlay => self.state,
            MusicScreen::Menu => self.leave_battle(MusicState::Menu),
            MusicScreen::Camp | MusicScreen::Briefing => self.leave_battle(MusicState::Camp),
            MusicScreen::Results | MusicScreen::Upgrades => self.leave_battle(MusicState::Results),
            MusicScreen::Battle => match input.battle {
                Some(battle) => self.battle_state(&battle, dt_ms),
                None => self.leave_battle(MusicState::Silent),
            },
        };
        if next != self.state {
            self.from_level = self.level();
            self.from = self.state;
            self.state = next;
            self.fade_ms = 0;
        }
        self.fade_ms = self.fade_ms.saturating_add(dt_ms).min(CROSSFADE_MS);
        self.state
    }
    fn leave_battle(&mut self, state: MusicState) -> MusicState {
        self.hold_ms = 0;
        self.paused = false;
        self.intensity = 0.0;
        state
    }
    fn battle_state(&mut self, battle: &BattleInput, dt_ms: u32) -> MusicState {
        self.paused = battle.paused;
        self.intensity = intensity(battle);
        if battle.boss_alive {
            self.hold_ms = 0;
            return MusicState::Boss;
        }
        let (threshold_met, hold, other) = match self.state {
            MusicState::BattleLow => (
                self.intensity >= HIGH_AT,
                HIGH_HOLD_MS,
                MusicState::BattleHigh,
            ),
            MusicState::BattleHigh => {
                (self.intensity <= LOW_AT, LOW_HOLD_MS, MusicState::BattleLow)
            }
            // A battle begins low, and so does the stretch after a boss.
            _ => {
                self.hold_ms = 0;
                return MusicState::BattleLow;
            }
        };
        if battle.paused {
            // The hold clocks stand still with the battle.
            return self.state;
        }
        if !threshold_met {
            self.hold_ms = 0;
            return self.state;
        }
        self.hold_ms = self.hold_ms.saturating_add(dt_ms);
        if self.hold_ms >= hold {
            self.hold_ms = 0;
            other
        } else {
            self.state
        }
    }
    /// The level of the current state's track on its fade in.
    fn level(&self) -> f32 {
        smoothstep(self.fade_ms as f32 / CROSSFADE_MS as f32)
    }

    /// The tracks to play now from the slots of `tracks`: the current
    /// state's file fading in and, during a crossfade, the last state's
    /// fading out. The two gains of a full crossfade sum to the context
    /// gain. A deferred slot plays nothing.
    pub fn voices(&self, tracks: &[(MusicState, Track)]) -> Vec<TrackVoice> {
        let level = self.level();
        let context = self.context_gain();
        let mut voices: Vec<TrackVoice> = Vec::with_capacity(2);
        let mut add = |state: MusicState, gain: f32| {
            let Track::File(path) = track_in(tracks, state) else {
                return;
            };
            match voices.iter_mut().find(|voice| voice.path == path) {
                Some(voice) => voice.gain += gain,
                None => voices.push(TrackVoice { path, gain }),
            }
        };
        add(self.state, level * context);
        if level < 1.0 {
            add(self.from, self.from_level * (1.0 - level) * context);
        }
        voices
    }
}
