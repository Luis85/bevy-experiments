//! Filesystem storage adapter, generation recovery, and save-state validation.
use super::migrations::{SAVE_VERSION, migrate};
use super::*;
use std::fmt;
use std::path::Path;

const MAX_GENERATION_BYTES: u64 = 64 * 1024 * 1024;

/// Revision of the gameplay rules. Bump when the same saved state and inputs
/// would produce different outcomes; a mismatch on load is logged, not fatal.
///
/// - 2 (W1-20): `SellItem` is strict. An equipped, preset, recruit-equipped
///   or NPC-issue item is refused with `SaleBlocked` instead of being sold
///   with its loadout slots replaced by starter equipment, so ruleset 1
///   recordings that sold such an item diverge. No seeded run sells, so
///   `GAMEPLAY_TRACE` and `GOLDEN` are unchanged.
pub const RULESET_VERSION: u32 = 2;

/// A save or load failure. `Display` keeps the player-facing wording.
#[derive(Debug)]
pub enum SaveError {
    /// The generation could not be inspected, read or staged.
    Io(std::io::Error),
    /// The staged generation could not replace the committed one.
    Commit(std::io::Error),
    /// The encoded generation exceeds the inclusive storage limit.
    TooLarge {
        length: u64,
        limit: u64,
    },
    /// The envelope or payload is not valid JSON for its schema.
    Encoding(serde_json::Error),
    UnsupportedVersion(u32),
    ChecksumMismatch,
    /// A legacy payload could not be upgraded to the current format.
    Migration(String),
    /// The decoded state violates a simulation invariant.
    Invalid(String),
    /// Neither the primary nor the backup generation could be read.
    Recovery {
        primary: Box<SaveError>,
        backup: Box<SaveError>,
    },
}
impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Commit(error) => write!(f, "Could not commit save: {error}"),
            Self::TooLarge { length, limit } => write!(
                f,
                "Save generation exceeds the supported storage limit ({length} bytes; maximum {limit} bytes)."
            ),
            Self::Encoding(error) => write!(f, "{error}"),
            Self::UnsupportedVersion(version) => write!(f, "Unsupported save version {version}"),
            Self::ChecksumMismatch => f.write_str("Save checksum mismatch"),
            Self::Migration(reason) => write!(f, "Save migration failed: {reason}"),
            Self::Invalid(reason) => f.write_str(reason),
            Self::Recovery { primary, backup } => write!(
                f,
                "Save recovery failed. Primary: {primary}. Backup: {backup}"
            ),
        }
    }
}
impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::Commit(error) => Some(error),
            Self::Encoding(error) => Some(error),
            _ => None,
        }
    }
}
impl From<std::io::Error> for SaveError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for SaveError {
    fn from(error: serde_json::Error) -> Self {
        Self::Encoding(error)
    }
}
/// Frontends that report failures as text can keep using `?`.
impl From<SaveError> for String {
    fn from(error: SaveError) -> Self {
        error.to_string()
    }
}

impl Game {
    /// Decode exactly one generation without filesystem access, fallback or writes.
    pub fn decode_save_generation(bytes: &[u8]) -> Result<Self, SaveError> {
        decode_generation(bytes)
    }
    /// Decode a save payload written at envelope `version`, exactly as a save
    /// generation would load: an older payload is migrated on raw JSON, then
    /// every game is reconciled and validated. Debug replay uses this for
    /// recordings made by an older build. Versions 0 and above
    /// [`SAVE_VERSION`] are [`SaveError::UnsupportedVersion`].
    pub fn from_payload(value: serde_json::Value, version: u32) -> Result<Self, SaveError> {
        let value = if version == SAVE_VERSION {
            value
        } else {
            // Legacy formats are upgraded on raw JSON before any typed decode;
            // `migrate` rejects version 0 and anything newer than this build.
            migrate(value, version)?
        };
        settle_decoded(serde_json::from_value(value)?)
    }
    /// Determinism/replay digest: the stable FNV-1a hash (the save checksum
    /// function) of the canonical serialized save payload. Two games with equal
    /// digests have identical authoritative state, including RNG streams, so
    /// golden tests and replay tooling compare one `u64` per tick or run.
    pub fn state_digest(&self) -> u64 {
        let payload = serde_json::to_string(self).expect("game state serializes");
        checksum(payload.as_bytes())
    }
    pub fn save(&self, path: &Path) -> Result<(), SaveError> {
        tracing::info!(path = %path.display(), "save started");
        let result = self.save_inner(path);
        match &result {
            Ok(()) => tracing::info!(path = %path.display(), "save succeeded"),
            Err(reason) => tracing::warn!(path = %path.display(), %reason, "save failed"),
        }
        result
    }
    fn save_inner(&self, path: &Path) -> Result<(), SaveError> {
        validate_game(self, usize::from(self.worker)).map_err(SaveError::Invalid)?;
        let payload = serde_json::to_string(self)?;
        let envelope = SaveEnvelope {
            version: SAVE_VERSION,
            checksum: checksum(payload.as_bytes()),
            ruleset_version: Some(RULESET_VERSION),
            content_fingerprint: Some(crate::catalog::content_fingerprint()),
            payload,
        };
        let bytes = serde_json::to_vec(&envelope)?;
        commit_generation(path, &bytes, MAX_GENERATION_BYTES)
    }
    pub fn load(path: &Path) -> Result<Self, SaveError> {
        tracing::info!(path = %path.display(), "load started");
        let result = match read_save(path) {
            Ok(game) => Ok(game),
            Err(primary) => {
                tracing::warn!(path = %path.display(), reason = %primary, "primary save unreadable, trying backup");
                read_save(&path.with_extension("bak")).map_err(|backup| SaveError::Recovery {
                    primary: Box::new(primary),
                    backup: Box::new(backup),
                })
            }
        };
        match &result {
            Ok(_) => tracing::info!(path = %path.display(), "load succeeded"),
            Err(reason) => tracing::warn!(path = %path.display(), %reason, "load failed"),
        }
        result
    }
}

fn ensure_generation_size(length: u64, limit: u64) -> Result<(), SaveError> {
    if length > limit {
        return Err(SaveError::TooLarge { length, limit });
    }
    Ok(())
}

fn commit_generation(path: &Path, bytes: &[u8], limit: u64) -> Result<(), SaveError> {
    // Guard the complete encoded envelope before creating a directory or
    // touching either committed generation or an existing temporary file.
    ensure_generation_size(bytes.len() as u64, limit)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    use std::io::Write;
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes).and_then(|()| file.sync_all())?;
    if path.exists() && read_save(path).is_ok() {
        std::fs::copy(path, &backup)?;
    }
    std::fs::rename(&temporary, path).map_err(SaveError::Commit)?;
    Ok(())
}

/// v1 envelopes predate the ruleset/fingerprint fields; both are optional.
#[derive(Serialize, Deserialize)]
struct SaveEnvelope {
    version: u32,
    checksum: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ruleset_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_fingerprint: Option<u64>,
    payload: String,
}
fn checksum(bytes: &[u8]) -> u64 {
    crate::hash::fnv1a(bytes)
}
fn read_save(path: &Path) -> Result<Game, SaveError> {
    ensure_generation_size(std::fs::metadata(path)?.len(), MAX_GENERATION_BYTES)?;
    let bytes = std::fs::read(path)?;
    Game::decode_save_generation(&bytes)
}
fn decode_generation(bytes: &[u8]) -> Result<Game, SaveError> {
    ensure_generation_size(bytes.len() as u64, MAX_GENERATION_BYTES)?;
    let envelope: SaveEnvelope = serde_json::from_slice(bytes)?;
    if envelope.version == 0 || envelope.version > SAVE_VERSION {
        return Err(SaveError::UnsupportedVersion(envelope.version));
    }
    if checksum(envelope.payload.as_bytes()) != envelope.checksum {
        return Err(SaveError::ChecksumMismatch);
    }
    warn_on_catalog_drift(&envelope);
    if envelope.version == SAVE_VERSION {
        // The current format skips the intermediate JSON tree.
        settle_decoded(serde_json::from_str(&envelope.payload)?)
    } else {
        Game::from_payload(serde_json::from_str(&envelope.payload)?, envelope.version)
    }
}
/// Catalogue-aware fills, then the invariants every loaded game must hold.
fn settle_decoded(mut game: Game) -> Result<Game, SaveError> {
    reconcile_saved_game(&mut game);
    validate_game(&game, 0).map_err(SaveError::Invalid)?;
    Ok(game)
}
/// Which envelope metadata drifted from this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Drift {
    Ruleset,
    Content,
}
/// Record a drift and report whether it is new: true exactly once per distinct
/// `(kind, saved, current)` in `seen`.
fn first_sighting(
    seen: &mut BTreeSet<(Drift, u64, u64)>,
    kind: Drift,
    saved: u64,
    current: u64,
) -> bool {
    seen.insert((kind, saved, current))
}
/// Process-wide [`first_sighting`]: every decode (including the verify read
/// during each save) checks drift, but each distinct pair is logged once.
fn should_warn(kind: Drift, saved: u64, current: u64) -> bool {
    static SEEN: std::sync::Mutex<BTreeSet<(Drift, u64, u64)>> =
        std::sync::Mutex::new(BTreeSet::new());
    let mut seen = SEEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    first_sighting(&mut seen, kind, saved, current)
}
/// Saves stay loadable across rules or catalogue changes; drift is only logged.
fn warn_on_catalog_drift(envelope: &SaveEnvelope) {
    if let Some(saved) = envelope.ruleset_version
        && saved != RULESET_VERSION
        && should_warn(Drift::Ruleset, saved.into(), RULESET_VERSION.into())
    {
        tracing::warn!(
            saved,
            current = RULESET_VERSION,
            "save was written under a different ruleset version"
        );
    }
    let current = crate::catalog::content_fingerprint();
    if let Some(saved) = envelope.content_fingerprint
        && saved != current
        && should_warn(Drift::Content, saved, current)
    {
        tracing::warn!(
            saved = format_args!("{saved:016x}"),
            current = format_args!("{current:016x}"),
            "save was written against different content catalogues"
        );
    }
}
fn reconcile_saved_game(game: &mut Game) {
    if game.campaign.journey_revision == 0 {
        game.campaign.journey_active =
            !game.worker && (game.attempt.is_some() || game.last_result.is_some());
        game.campaign.journey_revision = 1;
    }
    game.campaign.migrate_threat_history();
    game.strategy.reconcile_loadouts(&game.campaign);
    if let Some(a) = &mut game.attempt {
        if a.dps.version == 0 {
            a.dps.version = 1;
            a.dps.totals.defender = a.active_damage;
            a.dps.totals.structures = a.structure_damage;
            a.dps.buckets.clear();
        }
        if a.event_seq == 0 && !a.events.is_empty() {
            // Legacy windows predate sequence numbers; presentation dedupes by
            // them, so give every retained event a distinct one.
            let mut seq = game.next_event_seq;
            for event in &mut a.events {
                event.seq = seq;
                seq += 1;
            }
            a.event_seq = seq;
        }
        for e in &mut a.enemies {
            if e.boss && e.boss_kind.is_none() {
                e.boss_kind = Some(
                    content()
                        .levels
                        .get(a.level)
                        .map_or(BossKind::Breacher, |l| l.boss),
                );
            }
            if e.contact_since.is_some() && e.attack_phase == EnemyPhase::Advance {
                e.attack_phase = EnemyPhase::Windup;
                e.windup_total = e.attack_left;
            }
        }
    }
    for battle in &mut game.battles {
        reconcile_saved_game(&mut battle.game);
    }
}
/// D7 seat and takeover rules (schema-v7 §5, rules 1-4, 6 and 7; rule 5 is
/// in the parent battle loop and rule 8 in `RunRecord::validate`). Returns
/// the recruit holding the live depth-0 seat, whose duties and loadout
/// `validate_game` checks with the other reservations.
fn validate_defender_seats(game: &Game, depth: usize) -> Result<Option<u64>, String> {
    use crate::strategy::RecruitStatus;
    let on_roster = |id: u64| game.strategy.roster.iter().any(|r| r.id == id);
    let defending: Vec<u64> = game
        .strategy
        .roster
        .iter()
        .filter(|r| r.status == RecruitStatus::Defending)
        .map(|r| r.id)
        .collect();
    let attempt_seat = game.attempt.as_ref().map(|a| (a.mode, a.seat));
    let report_seat = game.last_result.as_ref().map(|r| (r.mode, r.seat));
    if [attempt_seat, report_seat]
        .into_iter()
        .flatten()
        .any(|(mode, seat)| mode == Mode::Personal && !seat.is_mode_owner())
    {
        return Err("A personal defense names a recruit defender".into());
    }
    if let Some(DefenderId::Recruit(id)) = game.campaign.next_defender
        && !on_roster(id)
    {
        return Err("The next defender is not on the roster".into());
    }
    if depth > 0 {
        // A worker game is one expedition; main-run defender state stays
        // with the parent.
        if game.campaign.reserved_recruit.is_some()
            || game.campaign.next_defender.is_some()
            || !defending.is_empty()
        {
            return Err("An expedition holds main-run defender state".into());
        }
        return Ok(None);
    }
    if let Some((_, DefenderSeat::Recruit(id))) = report_seat
        && !on_roster(id)
    {
        return Err("Saved report names a survivor missing from the roster".into());
    }
    match attempt_seat {
        Some((_, DefenderSeat::Recruit(id))) => {
            if defending != [id]
                || game.campaign.reserved != Some(Mode::Npc)
                || game.campaign.reserved_recruit != Some(id)
            {
                return Err("Recruit takeover does not match its Defending survivor".into());
            }
            Ok(Some(id))
        }
        _ => {
            if !defending.is_empty() || game.campaign.reserved_recruit.is_some() {
                return Err("A survivor is Defending without a recruit takeover".into());
            }
            Ok(None)
        }
    }
}
/// A report's drop ids (unique, at most its loot and [`MAX_RUN_DROPS`], each
/// an id the owning game issued), quality counts and auto-sold receipt
/// (schema v7 §3.4).
fn validate_report_loot(report: &Report, next_item_id: u64) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    if report.drop_ids.len() > MAX_RUN_DROPS
        || report.drop_ids.len() > report.loot as usize
        || report
            .drop_ids
            .iter()
            .any(|id| *id == 0 || *id >= next_item_id || !ids.insert(*id))
        || report
            .loot_by_quality
            .values()
            .map(|n| u64::from(*n))
            .sum::<u64>()
            > u64::from(report.loot)
    {
        return Err("Invalid saved report drop tracking".into());
    }
    report.auto_sold.validate(report.loot)
}
/// An attempt's drop ids (strictly ascending, at most its loot and
/// [`MAX_RUN_DROPS`], each an issued id) and frozen drop tier (schema v7 §3.3).
fn validate_attempt_loot(a: &Attempt, next_item_id: u64) -> Result<(), String> {
    if a.drop_ids.len() > MAX_RUN_DROPS
        || a.drop_ids.len() > a.loot as usize
        || a.drop_ids.first().is_some_and(|id| *id == 0)
        || a.drop_ids.last().is_some_and(|id| *id >= next_item_id)
        || a.drop_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || a.drop_tier
            .is_some_and(|tier| !(1..=content().max_drop_tier()).contains(&tier))
    {
        return Err("Invalid saved attempt drop tracking".into());
    }
    Ok(())
}
pub(super) fn validate_game(game: &Game, depth: usize) -> Result<(), String> {
    game.campaign.validate()?;
    if depth > 0 && !game.campaign.auto_sell.is_default() {
        // The parent pays an expedition's sales; a worker rule would pay twice.
        return Err("An expedition carries an auto-sell rule".into());
    }
    if let Some(report) = &game.last_result {
        validate_report_loot(report, game.campaign.next_item_id)?;
        if !(MIN_LANES..=MAX_LANES).contains(&report.lane_count)
            || report.lane_count.is_multiple_of(2)
            || report.telemetry.damage_by_lane.len() != usize::from(report.lane_count)
            || (report.outcome != Outcome::Victory && report.completion_xp != 0)
            || report.completion_xp > 1_000_000
        {
            return Err("Invalid frozen report arena or completion XP".into());
        }
        crate::quality::validate_loot_events(
            &report.loot_events,
            game.campaign.next_item_id,
            None,
            report.lane_count,
        )?;
    }
    game.strategy.validate(&game.campaign)?;
    if (!game.worker && !game.pending_runs.is_empty())
        || (game.worker && !game.campaign.run_history.is_empty())
    {
        return Err("Run history must have one parent owner".into());
    }
    for record in &game.pending_runs {
        record.validate()?;
        if record.id != 0 {
            return Err("Pending run already has global identity".into());
        }
    }
    if depth > 1
        || game.worker != (depth == 1)
        || game.battles.len() > 32
        || (depth == 1 && !game.battles.is_empty())
    {
        return Err("Invalid expedition tree".into());
    }
    let takeover = validate_defender_seats(game, depth)?;
    if let Some(a) = &game.attempt {
        validate_attempt_loot(a, game.campaign.next_item_id)?;
        crate::quality::validate_loot_events(
            &a.loot_events,
            game.campaign.next_item_id,
            Some(a.tick),
            a.lane_count,
        )?;
        a.profile.equipment_visual.validate()?;
        if a.profile
            .equipment_visual
            .weapon_class
            .is_some_and(|class| class != a.profile.weapon.class)
        {
            return Err("Frozen equipment visual class disagrees with combat weapon".into());
        }
        a.balance.validate()?;
        a.dps.validate(a.tick)?;
        if !a.width_yards.is_finite() || !(8.0..=30.0).contains(&a.width_yards) {
            return Err("Invalid frozen clearing width".into());
        }
        a.progression.validate()?;
        let level = content().levels.get(a.level).ok_or("Invalid saved level")?;
        let boss_base = if a.mode == Mode::Npc {
            level.boss_npc_hp.max(a.progression.npc_boss_health_floor)
        } else {
            level.boss_health.max(a.progression.boss_health_floor)
        };
        if boss_base.max(crate::enemy_catalog::enemy_catalog().maximum_health(a.mode)) as f32
            * a.modifiers.enemy_health
            > 10_000_000.0
        {
            return Err("Saved encounter exceeds supported health numerical bounds".into());
        }
        if a.progression.rank > game.campaign.progression_rank() {
            return Err("Saved encounter rank precedes campaign clears".into());
        }
        a.profile.modifiers.validate()?;
        if !a.profile.weapon.validate() {
            return Err("Invalid saved firearm projectile profile".into());
        }
        a.ordnance.validate(
            a.mode,
            &a.profile.modifiers,
            a.next_id,
            a.lane_count,
            a.width_yards,
        )?;
        for projectile in &a.ordnance.projectiles {
            projectile.validate_release_cues(a.tick)?;
        }
        if !(MIN_LANES..=MAX_LANES).contains(&a.lane_count)
            || a.lane_count.is_multiple_of(2)
            || a.telemetry.damage_by_lane.len() != usize::from(a.lane_count)
            || a.level >= content().levels.len()
            || a.enemies.len() > a.enemy_cap() + 1
            || a.enemies.iter().filter(|enemy| !enemy.boss).count() > a.enemy_cap()
            || a.structures.len() > usize::from(a.lane_count) * 5
            || a.enemies
                .iter()
                .any(|e| e.lane >= a.lane_count || !e.x.is_finite() || e.x < DEFENSE_LINE)
            || a.structures
                .iter()
                .any(|s| s.lane >= a.lane_count || s.column >= 5)
        {
            return Err("Invalid saved battlefield".into());
        }
        if a.last_damage_lane.is_some_and(|lane| lane >= a.lane_count)
            || a.lane_history.iter().any(|lane| *lane >= a.lane_count)
            || a.caller_lanes
                .is_some_and(|lanes| lanes.into_iter().any(|lane| lane >= a.lane_count))
        {
            return Err("Invalid frozen lane history or warning".into());
        }
        if !a.stamina.is_finite()
            || !a.max_stamina.is_finite()
            || a.stamina > a.max_stamina
            || a.stamina < -a.max_stamina
            || a.tick > u64::MAX - 100_000
            || a.events.len() > MAX_EVENTS
            || a.log.len() > MAX_LOG
            || a.events.iter().any(|e| {
                e.tick > a.tick
                    || e.from
                        .iter()
                        .chain(e.to.iter())
                        .any(|value| !value.is_finite())
                    || e.lane.is_some_and(|lane| lane >= a.lane_count)
            })
            || !a.injury.valid()
            || !a.profile.stats.valid()
            || !(0..=2).contains(&a.defense_bonus)
            || [
                a.modifiers.enemy_health,
                a.modifiers.enemy_speed,
                a.modifiers.reward,
                a.modifiers.personal_damage,
            ]
            .into_iter()
            .any(|value| !value.is_finite() || value <= 0.0)
            || !a.spawn_credit.is_finite()
            || !(0.0..1.0).contains(&a.spawn_credit)
        {
            return Err("Invalid saved simulation clock, physiology, modifiers or feedback".into());
        }
        if game.campaign.reserved != Some(a.mode)
            || a.max_hp <= 0
            || a.max_stamina <= 0.0
            || a.ammo > a.magazine
        {
            return Err("Saved defender state is inconsistent".into());
        }
        if a.boss_spawned != a.enemies.iter().any(|e| e.boss)
            || a.enemies.iter().filter(|e| e.boss).count() > 1
        {
            return Err("Saved boss lifecycle is inconsistent".into());
        }
        let mut cells = BTreeSet::new();
        let mut deployed_supply = 0u64;
        for s in &a.structures {
            if !cells.insert((s.lane, s.column))
                || s.hp <= 0
                || s.max_hp <= 0
                || s.hp > s.max_hp
                || s.cost == 0
                || !s.x().is_finite()
                || !s.footprint_width().is_finite()
                || s.footprint_width() <= 0.0
                || s.x() - s.footprint_width() * 0.5 < 1.5
                || s.x() + s.footprint_width() * 0.5 > a.width_yards * 0.5 + 0.001
            {
                return Err("Saved structure footprints or durability are invalid".into());
            }
            deployed_supply += u64::from(s.cost);
        }
        let starting_supply = (i64::from(content().levels[a.level].supply)
            + i64::from(a.modifiers.supply)
            + i64::from(a.progression.supply_bonus)
            + i64::from(a.balance.supply))
        .max(0) as u64;
        if deployed_supply + u64::from(a.supply) > starting_supply {
            return Err("Saved construction budget exceeds the authored allowance".into());
        }
        a.statuses
            .validate(&[(StatusId::Brace, BRACE_DURATION_TICKS)])?;
        for e in &a.enemies {
            if let Some(snapshot) = &e.resolved {
                snapshot.validate()?;
                if snapshot.kind != e.kind {
                    return Err("Saved hostile profile category is inconsistent".into());
                }
            }
            if !e.arena_width.is_finite()
                || e.arena_width != a.width_yards
                || e.max_hp <= 0
                || !e.pass_offset.is_finite()
                || e.pass_offset.abs() > MAX_PASS_OFFSET
                || (e.lane == 0 && e.pass_offset < 0.0)
                || (e.lane + 1 == a.lane_count && e.pass_offset > 0.0)
                || e.hp > e.max_hp
                || e.injury.max_hp != e.max_hp
                || !e.injury.valid()
                || e.born_tick > a.tick
                || e.statuses
                    .validate(&[(StatusId::HitStop, ENEMY_HIT_STOP_TICKS)])
                    .is_err()
                || !e.boss_timing.validate()
                || enemy_defeated(a, e)
                || (a.mode == Mode::Npc && e.hp != e.injury.hp)
                || e.target_structure.is_some_and(|id| {
                    !a.structures
                        .iter()
                        .any(|s| s.id == id && s.lane == e.lane && s.kind != StructureKind::Trap)
                })
            {
                return Err("Saved hostile physiology or blocker relationship is invalid".into());
            }
        }
        let mut actors = BTreeSet::new();
        for id in a
            .enemies
            .iter()
            .map(|e| e.id)
            .chain(a.structures.iter().map(|s| s.id))
            .chain(a.ordnance.projectiles.iter().map(|p| p.id))
            .chain(a.ordnance.fire_zones.iter().map(|z| z.id))
        {
            if id == 0 || id >= a.next_id || !actors.insert(id) || a.reward_ledger.contains(&id) {
                return Err("Invalid live actor identity".into());
            }
        }
        if a.reward_ledger
            .iter()
            .any(|id| *id == 0 || *id >= a.next_id)
        {
            return Err("Saved reward identity is invalid".into());
        }
        let mut corpses = BTreeSet::new();
        let mut previous_tick = 0;
        for corpse in &a.corpses {
            if corpse.id == 0
                || corpse.id >= a.next_id
                || !corpses.insert(corpse.id)
                || actors.contains(&corpse.id)
                || !a.reward_ledger.contains(&corpse.id)
                || corpse.tick > a.tick
                || corpse.tick < previous_tick
                || corpse.at.iter().any(|value| !value.is_finite())
                || corpse.at[0] < DEFENSE_LINE
                || !(0.5..=f32::from(a.lane_count) - 0.5).contains(&corpse.at[1])
                || corpse.visual.as_ref().is_some_and(|visual| {
                    !visual.scale.is_finite()
                        || !(0.25..=3.0).contains(&visual.scale)
                        || visual
                            .tint
                            .iter()
                            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                })
            {
                return Err(
                    "Invalid saved corpse identity, position, appearance or ordering".into(),
                );
            }
            previous_tick = corpse.tick;
        }
    } else if game.campaign.reserved.is_some() {
        return Err("Deployment equipment is reserved without an active attempt".into());
    }
    let mut battle_ids = BTreeSet::new();
    let mut recruits = BTreeSet::new();
    let actual_reservations: BTreeSet<_> = game.campaign.reserved_items.iter().copied().collect();
    if actual_reservations.len() != game.campaign.reserved_items.len() {
        return Err("Duplicate global equipment reservation".into());
    }
    let mut expected_reservations = BTreeSet::new();
    let root_drops: &[u64] = game.attempt.as_ref().map_or(&[], |a| &a.drop_ids);
    let mut pending_sales = BTreeSet::new();
    for guard in &game.strategy.guards {
        let recruit = game
            .strategy
            .roster
            .iter()
            .find(|r| r.id == guard.recruit_id)
            .ok_or("Guard missing from roster")?;
        expected_reservations.extend(recruit.npc.loadout);
    }
    for b in &game.battles {
        if !battle_ids.insert(b.id)
            || b.synced_coins > b.game.campaign.coins
            || b.synced_item_id > b.game.campaign.next_item_id
            || b.synced_item_index > b.game.campaign.items.len()
        {
            return Err("Invalid expedition settlement cursor".into());
        }
        // Parent-side ids awaiting this expedition's auto-sale: issued, owned
        // by no other pending sale, reserved by no duty, not a main-run drop.
        if (b.settled && !b.pending_auto_sell.is_empty())
            || b.pending_auto_sell.iter().any(|id| {
                *id == 0
                    || *id >= game.campaign.next_item_id
                    || !pending_sales.insert(*id)
                    || actual_reservations.contains(id)
                    || root_drops.contains(id)
            })
        {
            return Err("Invalid expedition pending auto-sale".into());
        }
        if b.summary.as_ref().is_some_and(|s| {
            ((s.acknowledged || s.outcome.is_some()) && !b.settled)
                || s.fronts.len() > ExpeditionSummary::MAX_FRONTS
        }) {
            return Err("Invalid expedition outcome notice".into());
        }
        validate_game(&b.game, depth + 1)?;
        // A worker game carries exactly one recruit: its attempt and report
        // seats name that recruit (v7 stamping, v6_to_v7 (a) for old saves).
        let seat = DefenderSeat::Recruit(b.recruit_id);
        if b.game.attempt.as_ref().is_some_and(|a| a.seat != seat)
            || b.game.last_result.as_ref().is_some_and(|r| r.seat != seat)
        {
            return Err("Expedition defender seat does not name its survivor".into());
        }
        if !b.settled {
            expected_reservations.extend(b.loadout);
            let r = game
                .strategy
                .roster
                .iter()
                .find(|r| r.id == b.recruit_id)
                .ok_or("Expedition survivor missing from roster")?;
            if !recruits.insert(b.recruit_id)
                || r.status != crate::strategy::RecruitStatus::Deployed
                || r.npc.loadout != b.loadout
                || b.loadout
                    .iter()
                    .any(|id| !game.campaign.reserved_items.contains(id))
            {
                return Err("Expedition reservation is inconsistent".into());
            }
        }
    }
    if let Some(id) = takeover {
        // D7 rule 2: the takeover recruit holds no other duty and its
        // loadout is reserved for the main run alone.
        let r = game
            .strategy
            .roster
            .iter()
            .find(|r| r.id == id)
            .ok_or("Defending survivor missing from roster")?;
        if game.strategy.guards.iter().any(|g| g.recruit_id == id)
            || game
                .battles
                .iter()
                .any(|b| !b.settled && b.recruit_id == id)
            || r.npc
                .loadout
                .iter()
                .any(|item| !expected_reservations.insert(*item))
        {
            return Err("Defending survivor has another duty or shares equipment".into());
        }
    }
    if actual_reservations != expected_reservations {
        return Err("Global equipment reservations do not match active duties".into());
    }
    if depth == 0
        && game.strategy.roster.iter().any(|r| {
            r.status == crate::strategy::RecruitStatus::Deployed && !recruits.contains(&r.id)
        })
    {
        return Err("Deployed survivor has no active expedition".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn release_cue_clocks_reject_future_and_inconsistent_saved_facts() {
        fn decode(game: &Game) -> Result<Game, String> {
            let payload = serde_json::to_string(game).unwrap();
            let envelope = SaveEnvelope {
                version: SAVE_VERSION,
                checksum: checksum(payload.as_bytes()),
                ruleset_version: Some(RULESET_VERSION),
                content_fingerprint: None,
                payload,
            };
            Ok(Game::decode_save_generation(
                &serde_json::to_vec(&envelope).unwrap(),
            )?)
        }
        let mut game = Game::default();
        game.campaign.coins = 1000;
        game.campaign.purchase_node("grenade_training").unwrap();
        game.deploy(0, Mode::Personal).unwrap();
        game.set_paused(false);
        game.tick(Input {
            throw: Some(ordnance::ThrowRequest {
                kind: ordnance::OrdnanceKind::Grenade,
                cell: [3, 1],
            }),
            ..Default::default()
        });
        for _ in 0..10 {
            game.tick(Input::default());
        }
        assert!(decode(&game).is_ok());
        let cue = game.attempt.as_ref().unwrap().ordnance.projectiles[0]
            .release_cues
            .unwrap();
        let now = game.attempt.as_ref().unwrap().tick;
        for bad in [
            ordnance::ReleaseCues {
                tick: now + 1,
                ..cue
            },
            ordnance::ReleaseCues {
                tick: cue.tick + 1,
                ..cue
            },
            ordnance::ReleaseCues { tick: 0, ..cue },
            ordnance::ReleaseCues {
                last_shot_tick: Some(cue.tick + 1),
                ..cue
            },
            ordnance::ReleaseCues {
                last_hit_tick: Some(cue.tick + 1),
                ..cue
            },
        ] {
            let mut invalid = game.clone();
            invalid.attempt.as_mut().unwrap().ordnance.projectiles[0].release_cues = Some(bad);
            assert!(
                validate_game(&invalid, 0)
                    .unwrap_err()
                    .contains("release cue clock")
            );
            assert!(decode(&invalid).unwrap_err().contains("release cue clock"));
        }

        // Additive optional data does not rewrite legacy projectile JSON.
        game.attempt.as_mut().unwrap().ordnance.projectiles[0].release_cues = None;
        let legacy = serde_json::to_value(&game).unwrap();
        assert!(
            legacy["attempt"]["ordnance"]["projectiles"][0]
                .get("release_cues")
                .is_none()
        );
        let restored = decode(&game).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), legacy);

        let mut npc = Game::default();
        npc.campaign.legacy_features = true;
        npc.campaign.balance_settings = crate::balance::BalanceSettings::legacy();
        npc.campaign.attempts = 1;
        npc.deploy(0, Mode::Npc).unwrap();
        let before = serde_json::to_string(&npc).unwrap();
        assert!(!before.contains("release_cues"));
        assert_eq!(
            serde_json::to_string(&decode(&npc).unwrap()).unwrap(),
            before
        );
    }

    #[test]
    fn ordinary_population_cannot_borrow_the_reserved_boss_slot_on_restore() {
        fn restore(game: &Game) -> Result<Game, SaveError> {
            let payload = serde_json::to_string(game).unwrap();
            let envelope = SaveEnvelope {
                version: SAVE_VERSION,
                checksum: checksum(payload.as_bytes()),
                ruleset_version: None,
                content_fingerprint: None,
                payload,
            };
            Game::decode_save_generation(&serde_json::to_vec(&envelope).unwrap())
        }
        let mut game = Game::default();
        game.deploy(0, Mode::Personal).unwrap();
        let mut attempt = game.attempt.take().unwrap();
        let cap = attempt.enemy_cap();
        for lane in 0..cap {
            game.spawn_enemy(&mut attempt, EnemyKind::Shambler, (lane % 11) as u8, false);
        }
        game.attempt = Some(attempt);
        assert!(restore(&game).is_ok());

        let mut overfull = game.clone();
        let attempt = overfull.attempt.as_mut().unwrap();
        let mut extra = attempt.enemies[0].clone();
        extra.id = attempt.next_id;
        attempt.next_id += 1;
        attempt.enemies.push(extra);
        assert!(restore(&overfull).is_err());

        let mut attempt = game.attempt.take().unwrap();
        attempt.boss_spawned = true;
        attempt.telemetry.boss_arrival_tick = Some(attempt.tick);
        game.spawn_enemy(&mut attempt, EnemyKind::Brute, 5, true);
        assert_eq!(attempt.enemies.len(), cap + 1);
        game.attempt = Some(attempt);
        assert!(restore(&game).is_ok());
    }

    #[test]
    fn writer_and_reader_share_an_inclusive_generation_size_boundary() {
        assert_eq!(MAX_GENERATION_BYTES, 67_108_864);
        assert!(ensure_generation_size(MAX_GENERATION_BYTES, MAX_GENERATION_BYTES).is_ok());
        assert!(ensure_generation_size(MAX_GENERATION_BYTES + 1, MAX_GENERATION_BYTES).is_err());
    }

    #[test]
    fn oversized_commit_preserves_both_readable_generations_and_any_temporary_file() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lastlight-generation-limit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let primary = root.join("campaign.json");
        let backup = primary.with_extension("bak");
        let temporary = primary.with_extension("tmp");
        let mut game = Game::default();
        game.save(&primary).unwrap();
        game.campaign.reward(7, 0, Mode::Personal);
        game.save(&primary).unwrap();
        std::fs::write(&temporary, b"previous uncommitted generation").unwrap();
        let primary_before = std::fs::read(&primary).unwrap();
        let backup_before = std::fs::read(&backup).unwrap();
        let temporary_before = std::fs::read(&temporary).unwrap();

        // Exercise the production commit path at a small injected boundary,
        // avoiding a 64 MiB object/fixture allocation in the regression suite.
        let length = primary_before.len() as u64;
        let error = commit_generation(&primary, &primary_before, length - 1).unwrap_err();
        assert!(matches!(error, SaveError::TooLarge { .. }));
        assert!(error.to_string().contains("storage limit"));
        assert_eq!(std::fs::read(&primary).unwrap(), primary_before);
        assert_eq!(std::fs::read(&backup).unwrap(), backup_before);
        assert_eq!(std::fs::read(&temporary).unwrap(), temporary_before);
        assert_eq!(read_save(&primary).unwrap().campaign.coins, 7);
        assert_eq!(read_save(&backup).unwrap().campaign.coins, 0);

        let missing = root.join("never-created").join("campaign.json");
        assert!(commit_generation(&missing, &primary_before, length - 1).is_err());
        assert!(!missing.parent().unwrap().exists());

        let exact = root.join("boundary.json");
        commit_generation(&exact, &primary_before, length).unwrap();
        assert_eq!(read_save(&exact).unwrap().campaign.coins, 7);
        for path in [primary, backup, temporary, exact] {
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }

    /// Ruleset 2 (W1-20): `SellItem` is strict. The same saved state and
    /// commands that sold an equipped, preset, recruit-equipped or NPC-issue
    /// item under ruleset 1 now reject it with `SaleBlocked`.
    #[test]
    fn ruleset_version_is_2() {
        assert_eq!(RULESET_VERSION, 2);
    }

    #[test]
    fn envelopes_carry_version_ruleset_and_fingerprint_and_reject_unknown_versions() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lastlight-envelope-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let game = Game::default();
        game.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let envelope: SaveEnvelope = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(envelope.version, SAVE_VERSION);
        assert_eq!(envelope.ruleset_version, Some(RULESET_VERSION));
        assert_eq!(
            envelope.content_fingerprint,
            Some(crate::catalog::content_fingerprint())
        );
        // Drifted metadata is tolerated: the save still loads.
        let drifted = SaveEnvelope {
            ruleset_version: Some(RULESET_VERSION + 1),
            content_fingerprint: Some(!crate::catalog::content_fingerprint()),
            ..envelope
        };
        let restored =
            Game::decode_save_generation(&serde_json::to_vec(&drifted).unwrap()).unwrap();
        assert_eq!(restored.campaign, game.campaign);
        for version in [0, SAVE_VERSION + 1] {
            let future = SaveEnvelope {
                version,
                checksum: drifted.checksum,
                ruleset_version: None,
                content_fingerprint: None,
                payload: drifted.payload.clone(),
            };
            assert!(matches!(
                Game::decode_save_generation(&serde_json::to_vec(&future).unwrap()),
                Err(SaveError::UnsupportedVersion(v)) if v == version
            ));
        }
        let corrupt = SaveEnvelope {
            checksum: drifted.checksum ^ 1,
            ..drifted
        };
        assert!(matches!(
            Game::decode_save_generation(&serde_json::to_vec(&corrupt).unwrap()),
            Err(SaveError::ChecksumMismatch)
        ));
    }

    #[test]
    fn catalogue_drift_is_reported_once_per_distinct_saved_and_current_pair() {
        let mut seen = BTreeSet::new();
        assert!(first_sighting(&mut seen, Drift::Content, 1, 2));
        // Repeated decodes (including each save's verify read) stay quiet.
        assert!(!first_sighting(&mut seen, Drift::Content, 1, 2));
        assert!(!first_sighting(&mut seen, Drift::Content, 1, 2));
        // A different saved or current value, or the other metadata, is new.
        assert!(first_sighting(&mut seen, Drift::Content, 3, 2));
        assert!(first_sighting(&mut seen, Drift::Content, 1, 4));
        assert!(first_sighting(&mut seen, Drift::Ruleset, 1, 2));
        assert!(!first_sighting(&mut seen, Drift::Ruleset, 1, 2));
        // The process-wide gate behaves the same (unique values: tests share it).
        let saved = u64::from(std::process::id()) << 32 | 0xD81F;
        assert!(should_warn(Drift::Content, saved, !saved));
        assert!(!should_warn(Drift::Content, saved, !saved));
    }
}
