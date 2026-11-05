//! Helpers shared by the integration test crates. Each test binary uses a
//! different subset, so unused items are expected.
#![allow(dead_code)]
use horde_core::balance::BalanceSettings;
use horde_core::campaign::Campaign;
use horde_core::content::{EnemyKind, Mode, Policy};
use horde_core::sim::{Game, Input, smoke_input};
use horde_core::skill_catalog::skills;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;

/// FNV-1a (64-bit) of `bytes`, as used by save checksums and pinned digests.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    fnv1a_extend(0xcbf2_9ce4_8422_2325, bytes)
}

/// Continue an FNV-1a hash over more `bytes`.
pub fn fnv1a_extend(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A default game with the legacy (fixed, unscaled) balance and features.
pub fn legacy_game() -> Game {
    let mut game = Game::default();
    game.campaign_mut().legacy_features = true;
    game.campaign_mut().balance_settings = BalanceSettings::legacy();
    game
}

/// A legacy game with an unpaused attempt deployed in `mode`.
pub fn game(mode: Mode) -> Game {
    let mut g = legacy_game();
    g.campaign_mut().attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    g
}

/// Spawn a Shambler (or boss) in `lane` and move it to `x`.
pub fn enemy(g: &mut Game, lane: u8, x: f32, boss: bool) {
    let mut a = g.attempt_slot_mut().take().unwrap();
    g.spawn_enemy(&mut a, EnemyKind::Shambler, lane, boss);
    a.enemies.last_mut().unwrap().x = x;
    *g.attempt_slot_mut() = Some(a);
}

/// Tick with default (idle) input `count` times.
pub fn advance(game: &mut Game, count: u32) {
    for _ in 0..count {
        game.tick(Input::default());
    }
}

/// The catalogue price of skill node `id`.
pub fn node_price(id: &str) -> u64 {
    skills().node(id).unwrap().price
}

/// The summed catalogue price of the named skill nodes.
pub fn nodes_price(nodes: &[&str]) -> u64 {
    nodes.iter().map(|id| node_price(id)).sum()
}

/// A fresh campaign that bought the named skill nodes and still holds
/// `spare` coins.
pub fn trained_campaign(spare: u64, nodes: &[&str]) -> Campaign {
    let mut campaign = Campaign::default();
    campaign.reward(spare + nodes_price(nodes), 0, Mode::Personal);
    for node in nodes {
        campaign.purchase_node(node).unwrap();
    }
    campaign
}

/// A default game that started its campaign in `country`.
pub fn country_game(country: &str) -> Game {
    let mut game = Game::default();
    game.campaign_mut().choose_start_country(country).unwrap();
    game
}

/// Decode a JSON game value as a v1 save envelope with a valid checksum, the
/// way an old build's save would reach migration.
pub fn decode_value(value: serde_json::Value) -> Result<Game, String> {
    let payload = serde_json::to_string(&value).unwrap();
    let checksum = fnv1a(payload.as_bytes());
    Ok(Game::decode_save_generation(
        &serde_json::to_vec(&serde_json::json!({
            "version": 1, "checksum": checksum, "payload": payload,
        }))
        .unwrap(),
    )?)
}

/// A legacy game with four cleared countries, 10k coins and a started campaign,
/// ready to recruit survivors and deploy parallel expeditions.
pub fn strategic_game() -> Game {
    let mut g = legacy_game();
    let countries: Vec<_> = horde_core::world::world()
        .countries
        .iter()
        .take(4)
        .map(|c| (c.id.clone(), c.level_count()))
        .collect();
    g.campaign_mut()
        .choose_start_country(&countries[0].0)
        .unwrap();
    for (id, count) in countries {
        g.campaign_mut()
            .world
            .country_levels
            .insert(id.clone(), count);
        g.campaign_mut().world.cleared.push(id);
    }
    g.campaign_mut().coins = 10000;
    g.campaign_mut().attempts = 1;
    g
}

// ---------------------------------------------------------------------------
// Determinism tooling (tests/determinism.rs). These helpers explain a digest
// move; they never decide whether one is allowed.
// ---------------------------------------------------------------------------

/// The first-stage encounter the `smoke` command plays: a default game with
/// `seed`, deployed in `mode` (Npc with the legacy features and the BossFocus
/// policy) and driven by `smoke_input` for `ticks` ticks. `observe` sees the
/// game after every tick.
pub fn smoke_run(seed: u64, mode: Mode, ticks: u32, mut observe: impl FnMut(&Game)) -> Game {
    let mut g = Game::default();
    g.set_seed(seed);
    g.campaign_mut().attempts = 1;
    if mode == Mode::Npc {
        g.campaign_mut().legacy_features = true;
        g.campaign_mut().npc.policy = Policy::BossFocus;
    }
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    for _ in 0..ticks {
        let input = smoke_input(&g);
        g.tick(input);
        observe(&g);
    }
    g
}

/// The attempt ticks at which [`OutcomeSummary`] samples a run.
pub const OUTCOME_SAMPLE_TICKS: [u64; 3] = [600, 1200, 1800];

/// The gameplay outcome of a live attempt at one tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeSample {
    pub tick: u64,
    pub kills: u32,
    pub coins: u64,
    pub hp: i32,
    pub loot: u32,
    pub spawned: u32,
    pub rng: u64,
}

/// Kills, coins, hp, loot, spawned and `rng.state` of a run at each of
/// [`OUTCOME_SAMPLE_TICKS`]. Feed it every tick with [`OutcomeSummary::observe`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutcomeSummary {
    pub samples: Vec<OutcomeSample>,
}

impl OutcomeSummary {
    /// Record the live attempt if it stands on a sample tick.
    pub fn observe(&mut self, g: &Game) {
        let Some(a) = g.attempt() else { return };
        if OUTCOME_SAMPLE_TICKS.contains(&a.tick) {
            self.samples.push(OutcomeSample {
                tick: a.tick,
                kills: a.kills,
                coins: a.coins,
                hp: a.hp,
                loot: a.loot,
                spawned: a.telemetry.spawned,
                rng: a.rng.state,
            });
        }
    }
    /// FNV-1a of the printed summary.
    pub fn digest(&self) -> u64 {
        fnv1a(self.to_string().as_bytes())
    }
}

impl fmt::Display for OutcomeSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, s) in self.samples.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(
                f,
                "t{} kills {} coins {} hp {} loot {} spawned {} rng {:#018x}",
                s.tick, s.kills, s.coins, s.hp, s.loot, s.spawned, s.rng
            )?;
        }
        Ok(())
    }
}

/// The outcome summary of the [`smoke_run`] of `seed` and `mode` to the last
/// sample tick.
pub fn outcome_summary(seed: u64, mode: Mode) -> OutcomeSummary {
    let mut summary = OutcomeSummary::default();
    let ticks = u32::try_from(OUTCOME_SAMPLE_TICKS[2]).unwrap();
    smoke_run(seed, mode, ticks, |g| summary.observe(g));
    summary
}

/// Every JSON-pointer path of an object key in `value` (no leaf values),
/// with array indices stripped: array elements add their keys under the
/// array's own path, so element count and order never change the set.
pub fn json_key_paths(value: &Value) -> BTreeSet<String> {
    fn walk(value: &Value, path: &str, paths: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let escaped = key.replace('~', "~0").replace('/', "~1");
                    let child_path = format!("{path}/{escaped}");
                    walk(child, &child_path, paths);
                    paths.insert(child_path);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, path, paths);
                }
            }
            _ => {}
        }
    }
    let mut paths = BTreeSet::new();
    walk(value, "", &mut paths);
    paths
}

/// FNV-1a of the sorted [`json_key_paths`], one per line: moves only when
/// the serialized shape gains, loses or renames a key.
pub fn json_key_paths_fnv(value: &Value) -> u64 {
    json_key_paths(value).iter().fold(fnv1a(b""), |hash, path| {
        fnv1a_extend(fnv1a_extend(hash, path.as_bytes()), b"\n")
    })
}

/// Object keys whose whole subtree is telemetry or a receipt: what a run
/// measured or handed out, never what the next tick reads. A move confined to
/// these subtrees is class T.
pub const TELEMETRY_SUBTREES: [&str; 8] = [
    "telemetry",
    "dps",
    "loot_events",
    "loot_quality_counts",
    "gear_snapshot",
    "run_history",
    "pending_runs",
    "progression_rewards",
];

/// `value` with every [`TELEMETRY_SUBTREES`] key removed at any depth.
pub fn without_telemetry(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !TELEMETRY_SUBTREES.contains(&key.as_str()))
                .map(|(key, child)| (key.clone(), without_telemetry(child)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_telemetry).collect()),
        other => other.clone(),
    }
}

/// FNV-1a of the definition, quality and level of every `campaign.items`
/// entry, in order. Panics when the payload has no `campaign.items` array, so
/// a renamed field cannot silently hide content moves.
pub fn items_fnv(payload: &Value) -> u64 {
    let items = payload
        .pointer("/campaign/items")
        .and_then(Value::as_array)
        .expect("the payload has a campaign.items array");
    items.iter().fold(fnv1a(b""), |hash, item| {
        let line = format!(
            "{} {} {}\n",
            item["definition"], item["quality"], item["level"]
        );
        fnv1a_extend(hash, line.as_bytes())
    })
}

/// What a run's digests are made of, for classifying a move between two
/// builds (`determinism_anatomy` prints one per run).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anatomy {
    /// `Game::state_digest`.
    pub golden: u64,
    /// The gameplay trace hash.
    pub trace: u64,
    /// [`json_key_paths_fnv`] of the payload.
    pub keys: u64,
    /// [`items_fnv`] of the payload.
    pub items: u64,
    /// FNV-1a of the payload [`without_telemetry`].
    pub core: u64,
    /// [`OutcomeSummary::digest`].
    pub outcome: u64,
}

impl Anatomy {
    pub fn of(payload: &Value, golden: u64, trace: u64, outcome: u64) -> Self {
        Self {
            golden,
            trace,
            keys: json_key_paths_fnv(payload),
            items: items_fnv(payload),
            core: fnv1a(without_telemetry(payload).to_string().as_bytes()),
            outcome,
        }
    }
}

impl fmt::Display for Anatomy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "golden {:#018x} trace {:#018x} keys {:#018x} items {:#018x} core {:#018x} outcome {:#018x}",
            self.golden, self.trace, self.keys, self.items, self.core, self.outcome
        )
    }
}

/// The class of a digest move (determinism protocol in
/// design-save-determinism section 9; ledger ruling P3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveClass {
    /// Format: the serialized shape or encoding moved (typically keys) with
    /// no gameplay, content or telemetry-only explanation.
    F,
    /// Content: campaign items moved, the outcome did not.
    C,
    /// Telemetry: only telemetry and receipt subtrees moved.
    T,
    /// Gameplay: the outcome summary or the gameplay trace moved.
    G,
}

/// Classify the move from `before` to `after`; None when nothing moved.
///
/// In precedence order: G when the outcome or the trace moved; T when the
/// payload without telemetry is unchanged; C when the items moved; F for any
/// other payload move (a key move, or a value re-encoding with no outcome,
/// trace or item move).
pub fn classify(before: &Anatomy, after: &Anatomy) -> Option<MoveClass> {
    if before == after {
        None
    } else if before.outcome != after.outcome || before.trace != after.trace {
        Some(MoveClass::G)
    } else if before.core == after.core {
        Some(MoveClass::T)
    } else if before.items != after.items {
        Some(MoveClass::C)
    } else {
        Some(MoveClass::F)
    }
}
