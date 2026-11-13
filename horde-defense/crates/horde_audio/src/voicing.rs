//! Voicing: which bank a request plays and how it is tuned, beyond its cue.
//!
//! A cue names what happened; its [`Voicing`] names who it happened to. An
//! enemy's windup, contact and death play the bank of its archetype's voice
//! ([`ENEMY_VOICES`]), a boss moment the bank of the boss's kind, and a
//! defender's shot carries the tone of the gun's quality ([`quality_tone`]).
//!
//! All of it is presentation: the tables read catalogue ids and saved enemy
//! snapshots, never the simulation's randomness, and nothing here is saved.

use crate::{
    bank::{BankId, BossStage, EnemyStage, EnemyVoice},
    cue::Cue,
    mix::db_to_linear,
};
use horde_core::{
    content::{BossKind, EnemyKind},
    progression::BossRank,
    quality::Quality,
    sim::{Attempt, BattleId, CorpseRecord, Enemy},
};
use std::collections::BTreeMap;

/// How a clip is tuned: playback speed, an optional low-pass and a gain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    pub speed: f32,
    pub lowpass_hz: Option<f32>,
    pub gain_db: f32,
}
impl Tone {
    /// The clip as mastered.
    pub const NEUTRAL: Self = Self::new(1.0, None, 0.0);
    /// A shot heard from camp, on an observed expedition.
    pub const DISTANT: Self = Self::new(0.97, Some(3500.0), -4.0);

    pub const fn new(speed: f32, lowpass_hz: Option<f32>, gain_db: f32) -> Self {
        Self {
            speed,
            lowpass_hz,
            gain_db,
        }
    }
}

/// The small timbre offset of a gun's quality tier (user decision 17): a
/// poor gun is a little duller and lower, a legendary one a little brighter
/// and higher. An unequipped ranged slot counts as Common.
pub const fn quality_tone(quality: Quality) -> Tone {
    match quality {
        Quality::Poor => Tone::new(0.985, Some(6000.0), -0.5),
        Quality::Common => Tone::new(1.0, Some(9000.0), 0.0),
        Quality::Uncommon => Tone::new(1.0, None, 0.0),
        Quality::Rare => Tone::new(1.005, None, 0.3),
        Quality::Epic => Tone::new(1.010, None, 0.5),
        Quality::Legendary => Tone::new(1.015, None, 0.7),
    }
}

/// What one request plays: its bank and the tone on top of the start's
/// jitter and its cue's mix row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voicing {
    pub bank: BankId,
    pub speed: f32,
    pub gain_db: f32,
    pub lowpass_hz: Option<f32>,
}
impl Voicing {
    /// The cue's own bank, as mastered.
    pub const fn of(cue: Cue) -> Self {
        Self::bank(cue.bank())
    }
    /// `bank`, as mastered.
    pub const fn bank(bank: BankId) -> Self {
        Self {
            bank,
            speed: 1.0,
            gain_db: 0.0,
            lowpass_hz: None,
        }
    }
    /// `bank` at `speed` and `gain_db`.
    pub const fn tuned(bank: BankId, speed: f32, gain_db: f32) -> Self {
        Self {
            bank,
            speed,
            gain_db,
            lowpass_hz: None,
        }
    }
    /// The cue's own bank through `tone`.
    pub const fn toned(cue: Cue, tone: Tone) -> Self {
        Self {
            bank: cue.bank(),
            speed: tone.speed,
            gain_db: tone.gain_db,
            lowpass_hz: tone.lowpass_hz,
        }
    }
    /// The linear factor of [`Self::gain_db`].
    pub fn gain(&self) -> f32 {
        db_to_linear(self.gain_db)
    }
}

/// One archetype's voice row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyVoiceRow {
    /// The catalogue id (`enemy_catalog.json`).
    pub archetype_id: &'static str,
    pub voice: EnemyVoice,
    pub speed: f32,
    pub gain_db: f32,
}
const fn voice(
    archetype_id: &'static str,
    voice: EnemyVoice,
    speed: f32,
    gain_db: f32,
) -> EnemyVoiceRow {
    EnemyVoiceRow {
        archetype_id,
        voice,
        speed,
        gain_db,
    }
}

/// The voice of every catalogue archetype, by id. Presentation only: no
/// content file names a voice. Whoever appends an archetype appends its row
/// (`every_catalog_archetype_has_explicit_voice_row_with_existing_files`).
pub const ENEMY_VOICES: &[EnemyVoiceRow] = &[
    voice("shambler", EnemyVoice::Shambler, 1.00, 0.0),
    voice("hungry-shambler", EnemyVoice::Shambler, 0.94, 0.0),
    voice("runner", EnemyVoice::Runner, 1.00, 0.0),
    voice("plated-runner", EnemyVoice::Runner, 0.95, 0.0),
    voice("caller-pack", EnemyVoice::Runner, 1.04, 0.0),
    voice("skitter", EnemyVoice::Swarm, 1.00, -2.0),
    voice("brute", EnemyVoice::Brute, 1.00, 0.0),
    voice("breacher-pack", EnemyVoice::Brute, 0.97, 0.0),
    voice("armored", EnemyVoice::Armored, 1.00, 0.0),
    voice("demolisher", EnemyVoice::Demolisher, 1.00, 0.0),
    voice("siege-pack", EnemyVoice::Demolisher, 0.96, 0.0),
    // The goal-6 archetypes.
    voice("exploder", EnemyVoice::Exploder, 1.00, 0.0),
    voice("splitter", EnemyVoice::Brute, 0.90, 0.0),
    voice("splitter-brood", EnemyVoice::Swarm, 0.96, -2.0),
    voice("spitter", EnemyVoice::Spitter, 1.00, 0.0),
    voice("shield-bearer", EnemyVoice::Armored, 0.92, 0.0),
    voice("sprinter", EnemyVoice::Swarm, 1.08, 0.0),
    voice("juggernaut", EnemyVoice::Demolisher, 0.85, 1.0),
];

/// The row of `archetype_id`, if it has one.
pub fn voice_row(archetype_id: &str) -> Option<&'static EnemyVoiceRow> {
    ENEMY_VOICES
        .iter()
        .find(|row| row.archetype_id == archetype_id)
}

/// The fallback voice of an enemy whose archetype has no row (an id from a
/// newer catalogue, or a save without a snapshot): its legacy kind's.
pub const fn kind_voice(kind: EnemyKind) -> EnemyVoice {
    match kind {
        EnemyKind::Shambler => EnemyVoice::Shambler,
        EnemyKind::Runner => EnemyVoice::Runner,
        EnemyKind::Brute => EnemyVoice::Brute,
        EnemyKind::Armored => EnemyVoice::Armored,
        EnemyKind::Demolisher => EnemyVoice::Demolisher,
    }
}

/// An elite plays its family's bank lower and louder; no files of its own.
pub const ELITE_SPEED: f32 = 0.94;
pub const ELITE_GAIN_DB: f32 = 1.0;
/// The capstone boss plays its kind's bank lower and louder.
pub const CAPSTONE_SPEED: f32 = 0.94;
pub const CAPSTONE_GAIN_DB: f32 = 1.0;

/// How one enemy sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyVoiceRef {
    pub voice: EnemyVoice,
    pub speed: f32,
    pub gain_db: f32,
    pub elite: bool,
    /// The kind of a boss that has one; its moments play the kind's banks.
    pub boss: Option<BossKind>,
}
impl EnemyVoiceRef {
    /// The voice of an archetype id, or of `kind` when the id has no row.
    pub fn resolve(archetype_id: Option<&str>, kind: EnemyKind, elite: bool) -> Self {
        let (voice, speed, gain_db) = match archetype_id.and_then(voice_row) {
            Some(row) => (row.voice, row.speed, row.gain_db),
            None => (kind_voice(kind), 1.0, 0.0),
        };
        Self {
            voice,
            speed: speed * if elite { ELITE_SPEED } else { 1.0 },
            gain_db: gain_db + if elite { ELITE_GAIN_DB } else { 0.0 },
            elite,
            boss: None,
        }
    }
    /// The voice of a live enemy: its spawn snapshot's archetype.
    pub fn of(enemy: &Enemy) -> Self {
        let resolved = enemy.resolved.as_ref();
        Self {
            boss: if enemy.boss { enemy.boss_kind } else { None },
            ..Self::resolve(
                resolved.map(|resolved| resolved.archetype_id.as_str()),
                enemy.kind,
                resolved.is_some_and(|resolved| resolved.elite.is_some()),
            )
        }
    }
    /// The voice of a corpse: only its kind survives the death.
    pub fn of_corpse(corpse: &CorpseRecord) -> Self {
        Self {
            boss: corpse.boss_kind,
            ..Self::resolve(None, corpse.kind, false)
        }
    }
    /// What the enemy's `stage` plays.
    pub fn voicing(&self, stage: EnemyStage) -> Voicing {
        Voicing::tuned(BankId::Enemy(self.voice, stage), self.speed, self.gain_db)
    }
}

/// What a boss of `kind` plays at `stage`; the capstone lower and louder.
pub const fn boss_voicing(kind: BossKind, stage: BossStage, capstone: bool) -> Voicing {
    if capstone {
        Voicing::tuned(BankId::Boss(kind, stage), CAPSTONE_SPEED, CAPSTONE_GAIN_DB)
    } else {
        Voicing::bank(BankId::Boss(kind, stage))
    }
}

/// What the index knows of one battle.
#[derive(Clone, Debug, Default, PartialEq)]
struct BattleVoices {
    attempt_id: u64,
    /// The attempt's boss is the capstone.
    capstone: bool,
    /// The quality of the defender's gun; `None` is an empty ranged slot.
    quality: Option<Quality>,
    enemies: BTreeMap<u64, EnemyVoiceRef>,
}

/// The voices of the live enemies, by battle and enemy id. An event names an
/// enemy only by id, and a kill names one that is already gone, so the
/// runner syncs the index before each tick and again before it maps the
/// tick's events.
///
/// Presentation only: it is not saved. After a load the first sync rebuilds
/// it from the attempt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnemyVoiceIndex {
    battles: BTreeMap<BattleId, BattleVoices>,
}
impl EnemyVoiceIndex {
    /// Brings `battle` up to `attempt`: a new attempt id (a load, a deploy,
    /// another expedition under the same key) rebuilds the battle's voices;
    /// otherwise the enemies not yet indexed are added.
    pub fn sync(&mut self, battle: BattleId, attempt: &Attempt) {
        let voices = self.battles.entry(battle).or_default();
        if voices.attempt_id != attempt.id {
            voices.enemies.clear();
            voices.attempt_id = attempt.id;
        }
        voices.capstone = attempt.progression.boss_rank == BossRank::Capstone;
        voices.quality = attempt
            .profile
            .equipment_visual
            .ranged
            .as_ref()
            .map(|item| item.quality);
        for enemy in &attempt.enemies {
            voices
                .enemies
                .entry(enemy.id)
                .or_insert_with(|| EnemyVoiceRef::of(enemy));
        }
    }
    /// The voice of enemy `id` of `battle`, if it is indexed.
    pub fn get(&self, battle: BattleId, id: u64) -> Option<EnemyVoiceRef> {
        self.battles.get(&battle)?.enemies.get(&id).copied()
    }
    /// Takes enemy `id` of `battle` out: it died.
    pub fn remove(&mut self, battle: BattleId, id: u64) -> Option<EnemyVoiceRef> {
        self.battles.get_mut(&battle)?.enemies.remove(&id)
    }
    /// Whether `battle`'s boss is the capstone.
    pub fn capstone(&self, battle: BattleId) -> bool {
        self.battles
            .get(&battle)
            .is_some_and(|voices| voices.capstone)
    }
    /// The tone of the defender's gun in `battle`: its quality's, Common's
    /// for an empty slot. `None` before the battle's first sync.
    pub fn shot_tone(&self, battle: BattleId) -> Option<Tone> {
        self.battles
            .get(&battle)
            .map(|voices| quality_tone(voices.quality.unwrap_or(Quality::Common)))
    }
    /// Drops every battle but those in `keep`: a settled expedition's
    /// voices are never needed again.
    pub fn retain(&mut self, keep: impl Fn(BattleId) -> bool) {
        self.battles.retain(|battle, _| keep(*battle));
    }
    /// How many enemies of `battle` are indexed.
    pub fn len(&self, battle: BattleId) -> usize {
        self.battles
            .get(&battle)
            .map_or(0, |voices| voices.enemies.len())
    }
}

/// A one-pole low-pass, the tone filter of a voice. The runtime's decoder
/// and the offline renderer run the same one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnePole {
    /// The share of the new sample in the output.
    alpha: f32,
    state: f32,
}
impl OnePole {
    /// A low-pass at `cutoff_hz` for audio at `rate`. A cutoff that is not
    /// positive and finite, or at or above Nyquist, passes everything.
    pub fn new(cutoff_hz: f32, rate: u32) -> Self {
        let nyquist = rate as f32 * 0.5;
        let alpha = if cutoff_hz.is_finite() && cutoff_hz > 0.0 && cutoff_hz < nyquist {
            1.0 - (-std::f32::consts::TAU * cutoff_hz / rate as f32).exp()
        } else {
            1.0
        };
        Self { alpha, state: 0.0 }
    }
    /// The next output sample.
    pub fn next(&mut self, sample: f32) -> f32 {
        self.state += self.alpha * (sample - self.state);
        self.state
    }
}

/// `samples` through a [`OnePole`] at `cutoff_hz`.
pub fn lowpassed(samples: &[f32], cutoff_hz: f32, rate: u32) -> Vec<f32> {
    let mut filter = OnePole::new(cutoff_hz, rate);
    samples.iter().map(|sample| filter.next(*sample)).collect()
}
