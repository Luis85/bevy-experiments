//! Ordered save-format migrations on raw JSON, applied before typed
//! deserialization. Each step freezes the meaning of its source version, so a
//! migration never consults the currently loaded catalogues.
use super::persistence::SaveError;
use serde_json::{Map, Value};

/// Envelope version written by this build.
pub const SAVE_VERSION: u32 = 7;

/// Upgrade a payload saved at envelope `from` to [`SAVE_VERSION`].
pub(super) fn migrate(mut value: Value, from: u32) -> Result<Value, SaveError> {
    if from == 0 || from > SAVE_VERSION {
        return Err(SaveError::UnsupportedVersion(from));
    }
    for version in from..SAVE_VERSION {
        value = match version {
            1 => v1_to_v2(value)?,
            2 => v2_to_v3(value),
            3 => v3_to_v4(value)?,
            4 => v4_to_v5(value)?,
            5 => v5_to_v6(value)?,
            6 => v6_to_v7(value)?,
            _ => unreachable!("every version below SAVE_VERSION has a step"),
        };
    }
    Ok(value)
}

/// v1 item definition order (`content.json` items at save format v1). A v1
/// `definition: n` means `V1_ITEM_IDS[n]`.
const V1_ITEM_IDS: [&str; 29] = [
    "service_carbine",
    "crowbar",
    "work_jacket",
    "field_webbing",
    "marksman_rifle",
    "fire_axe",
    "padded_vest",
    "first_aid_rig",
    "patrol_rifle",
    "forged_machete",
    "scavenged_ballistic_vest",
    "rangefinder",
    "longwatch_rifle",
    "breach_hammer",
    "riot_armor",
    "survival_harness",
    "checkpoint_rifle",
    "tempered_saber",
    "fortress_armor",
    "veteran_kit",
    "worn_service_pistol",
    "compact_service_pistol",
    "compact_smg",
    "pump_shotgun",
    "patrol_rifle_tier1",
    "scout_marksman_rifle",
    "belt_fed_machine_gun",
    "heavy_revolver",
    "double_barrel_shotgun",
];
const V1_ENEMY_KINDS: &[&str] = &["Shambler", "Runner", "Brute", "Armored", "Demolisher"];
const V1_STRUCTURE_KINDS: &[&str] = &["Trap", "Wall", "Turret"];
const V1_BODY_ATTRIBUTES: &[&str] = &[
    "Vitality",
    "Strength",
    "Coordination",
    "Endurance",
    "Perception",
];
const V1_NPC_TRAINING: &[&str] = &[
    "Strength",
    "Dexterity",
    "Intelligence",
    "Health",
    "Guns",
    "Melee",
    "HitPoints",
    "Fatigue",
];
const V1_NPC_MANEUVERS: &[&str] = &[
    "DoNothing",
    "Aim",
    "Attack",
    "Ready",
    "Reload",
    "Recover",
    "Posture",
    "AllOutDefense",
];

/// v1 positional arrays, by field name, and the kind order their slots meant.
/// These names are unique to these fields across the whole save tree.
const V1_KIND_ARRAYS: &[(&str, &[&str])] = &[
    ("kills_by_kind", V1_ENEMY_KINDS),
    ("npc_decisions", V1_NPC_MANEUVERS),
    ("blueprints", V1_STRUCTURE_KINDS),
    ("structure_upgrades", V1_STRUCTURE_KINDS),
    ("structure_ranks", V1_STRUCTURE_KINDS),
    ("body", V1_BODY_ATTRIBUTES),
    ("body_development", V1_BODY_ATTRIBUTES),
    ("investments", V1_NPC_TRAINING),
];

/// v1 -> v2: positional item `definition` indices become stable item IDs and
/// enum-indexed arrays become kind-keyed maps, wherever they occur (campaign,
/// attempts, reports, run history, recruits and nested expedition games).
/// Values already in v2 shape are left untouched, so the step is idempotent.
fn v1_to_v2(mut value: Value) -> Result<Value, SaveError> {
    rewrite_v1(&mut value)?;
    Ok(value)
}

fn rewrite_v1(value: &mut Value) -> Result<(), SaveError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if key == "definition"
                    && let Value::Number(index) = child
                {
                    let id = index
                        .as_u64()
                        .and_then(|n| V1_ITEM_IDS.get(usize::try_from(n).ok()?))
                        .ok_or_else(|| {
                            SaveError::Migration(format!("Unknown v1 item definition {index}"))
                        })?;
                    *child = Value::String((*id).to_owned());
                    continue;
                }
                if let Some((_, kinds)) = V1_KIND_ARRAYS.iter().find(|(name, _)| name == key)
                    && let Value::Array(slots) = child
                {
                    if slots.len() != kinds.len() {
                        return Err(SaveError::Migration(format!(
                            "v1 field {key} has {} entries; expected {}",
                            slots.len(),
                            kinds.len()
                        )));
                    }
                    let map: Map<String, Value> = kinds
                        .iter()
                        .map(|kind| (*kind).to_owned())
                        .zip(std::mem::take(slots))
                        .collect();
                    *child = Value::Object(map);
                    continue;
                }
                rewrite_v1(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_v1(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// v2 boolean skill unlock flags and the unlock IDs that replaced them.
const V2_UNLOCK_FLAGS: &[(&str, &str)] = &[
    ("piercing", "piercing"),
    ("sweep", "sweep"),
    ("heal", "heal"),
    ("brace", "brace"),
    ("grenade_unlocked", "grenade"),
    ("molotov_unlocked", "molotov"),
];
/// v2 per-actor tick counters and the status IDs that replaced them.
const V2_STATUS_COUNTERS: &[(&str, &str)] =
    &[("hit_stop_left", "HitStop"), ("brace_left", "Brace")];
/// NPC target urgency every v2 hostile category implied (only Runners were
/// prioritised, by 3 yards).
const V2_RUNNER_TARGET_URGENCY: f64 = -3.0;

/// v2 -> v3, wherever the shapes occur (attempts, nested expedition games):
/// - enemy `hit_stop_left` / attempt `brace_left` counters become a sorted
///   `statuses` list (a zero counter is an absent status);
/// - skill-modifier unlock booleans become the sorted `unlocks` ID set;
/// - resolved enemy snapshots gain the `target_urgency` their category implied.
///
/// Values already in v3 shape are left untouched, so the step is idempotent.
fn v2_to_v3(mut value: Value) -> Value {
    rewrite_v2(&mut value);
    value
}

fn rewrite_v2(value: &mut Value) {
    match value {
        Value::Object(object) => {
            migrate_status_counters(object);
            if object.contains_key("cadence_multiplier") {
                migrate_unlock_flags(object);
            }
            if object.contains_key("archetype_id")
                && object.contains_key("npc_damage_dice")
                && !object.contains_key("target_urgency")
            {
                let runner = object.get("kind").and_then(Value::as_str) == Some("Runner");
                let urgency = if runner {
                    V2_RUNNER_TARGET_URGENCY
                } else {
                    0.0
                };
                object.insert("target_urgency".into(), urgency.into());
            }
            for child in object.values_mut() {
                rewrite_v2(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(rewrite_v2),
        _ => {}
    }
}

fn migrate_status_counters(object: &mut Map<String, Value>) {
    let mut statuses = Vec::new();
    // V2_STATUS_COUNTERS is in StatusId order, so the list stays sorted.
    for (field, id) in V2_STATUS_COUNTERS {
        if let Some(left) = object.remove(*field)
            && let Some(left) = left.as_u64().filter(|left| *left > 0)
        {
            statuses.push(serde_json::json!({
                "id": id, "remaining_ticks": left, "stacks": 1, "magnitude": 0
            }));
        }
    }
    if !statuses.is_empty() {
        object.insert("statuses".into(), Value::Array(statuses));
    }
}

fn migrate_unlock_flags(object: &mut Map<String, Value>) {
    let mut unlocks: Vec<&str> = Vec::new();
    for (flag, id) in V2_UNLOCK_FLAGS {
        if object.remove(*flag).and_then(|v| v.as_bool()) == Some(true) {
            unlocks.push(id);
        }
    }
    unlocks.sort_unstable();
    if !unlocks.is_empty() || !object.contains_key("unlocks") {
        object.insert("unlocks".into(), unlocks.into());
    }
}

/// v3 quality order (`Quality` declaration order at save format v3). A v3
/// `loot_by_quality` / `loot_quality_counts` slot `n` meant `V3_QUALITIES[n]`.
const V3_QUALITIES: &[&str] = &["Poor", "Common", "Uncommon", "Rare", "Epic", "Legendary"];
/// v3 positional quality arrays, by field name (unique across the save tree).
const V3_QUALITY_ARRAYS: &[&str] = &["loot_by_quality", "loot_quality_counts"];

/// v3 -> v4: the quality-indexed loot counters (attempts, reports, run
/// history, nested expedition games) become quality-keyed maps. Values already
/// in v4 shape are left untouched, so the step is idempotent.
fn v3_to_v4(mut value: Value) -> Result<Value, SaveError> {
    rewrite_v3(&mut value)?;
    Ok(value)
}

fn rewrite_v3(value: &mut Value) -> Result<(), SaveError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if V3_QUALITY_ARRAYS.contains(&key.as_str())
                    && let Value::Array(slots) = child
                {
                    if slots.len() != V3_QUALITIES.len() {
                        return Err(SaveError::Migration(format!(
                            "v3 field {key} has {} entries; expected {}",
                            slots.len(),
                            V3_QUALITIES.len()
                        )));
                    }
                    let map: Map<String, Value> = V3_QUALITIES
                        .iter()
                        .map(|quality| (*quality).to_owned())
                        .zip(std::mem::take(slots))
                        .collect();
                    *child = Value::Object(map);
                    continue;
                }
                rewrite_v3(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_v3(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// v4 `OrdnanceKind` order (declaration order at save format v4). A v4
/// ordnance counter slot `n` meant `V4_ORDNANCE_KINDS[n]`.
const V4_ORDNANCE_KINDS: &[&str] = &["Grenade", "Molotov"];
/// v4 positional per-kind counters of an attempt's `ordnance` state.
const V4_ORDNANCE_ARRAYS: &[&str] = &["cooldowns", "charges", "issued", "spent"];

/// v4 -> v5: the per-kind ordnance counters of every attempt's `ordnance`
/// state (including nested expedition games) become kind-keyed maps. Values
/// already in v5 shape are left untouched, so the step is idempotent.
fn v4_to_v5(mut value: Value) -> Result<Value, SaveError> {
    rewrite_v4(&mut value)?;
    Ok(value)
}

fn rewrite_v4(value: &mut Value) -> Result<(), SaveError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if key == "ordnance"
                    && let Value::Object(state) = child
                {
                    for field in V4_ORDNANCE_ARRAYS {
                        if let Some(slots) = state.get_mut(*field)
                            && let Value::Array(entries) = slots
                        {
                            if entries.len() != V4_ORDNANCE_KINDS.len() {
                                return Err(SaveError::Migration(format!(
                                    "v4 ordnance {field} has {} entries; expected {}",
                                    entries.len(),
                                    V4_ORDNANCE_KINDS.len()
                                )));
                            }
                            let map: Map<String, Value> = V4_ORDNANCE_KINDS
                                .iter()
                                .map(|kind| (*kind).to_owned())
                                .zip(std::mem::take(entries))
                                .collect();
                            *slots = Value::Object(map);
                        }
                    }
                    continue;
                }
                rewrite_v4(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_v4(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// v5 level order (`content.json` levels at save format v5). A v5 level
/// position `n` (clear flag slot, claim slot, attempt/report `level`, run
/// `template`) meant `V5_LEVEL_IDS[n]`.
const V5_LEVEL_IDS: [&str; 5] = [
    "the_old_road",
    "ashwood_crossing",
    "depot_nine",
    "the_iron_yard",
    "lastlight_checkpoint",
];

/// v5 -> v6: level-positional progress becomes level-ID keyed, wherever it
/// occurs (campaign, its NPC, recruits, attempts, reports, run history and
/// nested expedition games):
/// - `first_clears: [bool; 5]` becomes the sorted set of cleared level IDs;
/// - `first_clear_claimed: [u32; 5]` becomes a level-ID map of non-zero claims;
/// - the campaign's numeric `unlocked` frontier is dropped (v6 derives it from
///   the clears; v5 validation already required exactly that value);
/// - an attempt's or report's `level` and a run's `template` become level IDs.
///
/// Values already in v6 shape are left untouched, so the step is idempotent.
fn v5_to_v6(mut value: Value) -> Result<Value, SaveError> {
    rewrite_v5(&mut value)?;
    Ok(value)
}

fn v5_level(key: &str, slot: &Value) -> Result<Value, SaveError> {
    slot.as_u64()
        .and_then(|n| V5_LEVEL_IDS.get(usize::try_from(n).ok()?))
        .map(|id| Value::String((*id).to_owned()))
        .ok_or_else(|| SaveError::Migration(format!("Unknown v5 level {key} {slot}")))
}

fn v5_slots<'a>(key: &str, slots: &'a [Value]) -> Result<&'a [Value], SaveError> {
    if slots.len() == V5_LEVEL_IDS.len() {
        Ok(slots)
    } else {
        Err(SaveError::Migration(format!(
            "v5 field {key} has {} entries; expected {}",
            slots.len(),
            V5_LEVEL_IDS.len()
        )))
    }
}

fn rewrite_v5(value: &mut Value) -> Result<(), SaveError> {
    match value {
        Value::Object(object) => {
            if let Some(Value::Array(slots)) = object.get("first_clears")
                && slots.iter().any(Value::is_boolean)
            {
                let mut cleared: Vec<&str> = V5_LEVEL_IDS
                    .iter()
                    .zip(v5_slots("first_clears", slots)?)
                    .filter(|(_, clear)| clear.as_bool() == Some(true))
                    .map(|(id, _)| *id)
                    .collect();
                // V5_LEVEL_IDS is not alphabetical; ID sets serialize sorted.
                cleared.sort_unstable();
                object.insert("first_clears".into(), cleared.into());
                if object.get("unlocked").is_some_and(Value::is_number) {
                    object.remove("unlocked");
                }
            }
            if let Some(Value::Array(slots)) = object.get("first_clear_claimed") {
                let mut claims = Map::new();
                for (id, claimed) in V5_LEVEL_IDS
                    .iter()
                    .zip(v5_slots("first_clear_claimed", slots)?)
                {
                    if claimed.as_u64().is_none_or(|n| n > 0) {
                        claims.insert((*id).to_owned(), claimed.clone());
                    }
                }
                object.insert("first_clear_claimed".into(), Value::Object(claims));
            }
            if object.contains_key("country_name") {
                if object.contains_key("stage")
                    && let Some(level) = object.get_mut("level")
                    && level.is_number()
                {
                    *level = v5_level("level", level)?;
                }
                if let Some(template) = object.get_mut("template")
                    && template.is_number()
                {
                    *template = v5_level("template", template)?;
                }
            }
            for child in object.values_mut() {
                rewrite_v5(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_v5(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// v6 -> v7: path-based, idempotent (`or_insert` only) and catalogue-free.
/// The sub-steps run in this fixed order; later steps land in their owners'
/// commits at the marked slots and never reorder the earlier ones.
fn v6_to_v7(mut value: Value) -> Result<Value, SaveError> {
    v7_worker_seats(&mut value)?; // (a) defender seats
    v7_npc_appearance(&mut value); // (e) defensive appearance back-fill
    v7_report_drops(&mut value)?; // (b) report drop tracking
    v7_level_grant_paid(&mut value)?; // (c) level-grant ledger
    v7_milestones_claimed(&mut value)?; // (d) milestones
    Ok(value)
}

/// The root campaign and every expedition game's campaign.
type V6Campaigns<'a> = (Option<&'a mut Value>, Vec<&'a mut Value>);

fn v6_campaigns(value: &mut Value) -> Result<V6Campaigns<'_>, SaveError> {
    let Some(root) = value.as_object_mut() else {
        return Ok((None, Vec::new()));
    };
    let (mut campaign, mut battles) = (None, None);
    for (key, child) in root.iter_mut() {
        match key.as_str() {
            "campaign" => campaign = Some(child),
            "battles" => battles = Some(child),
            _ => {}
        }
    }
    let mut workers = Vec::new();
    for battle in v6_battles(battles)? {
        if let Some(worker) = battle
            .get_mut("game")
            .and_then(|game| game.get_mut("campaign"))
        {
            workers.push(worker);
        }
    }
    Ok((campaign, workers))
}

/// (c) Every player at level 2 or above, the root's and each expedition's,
/// is marked as already paid the level-up grant for its level: a v6 save is
/// owed no retroactive grant. Level 1 writes nothing (0 and 1 both mean
/// none paid).
fn v7_level_grant_paid(value: &mut Value) -> Result<(), SaveError> {
    let (root, workers) = v6_campaigns(value)?;
    for campaign in root.into_iter().chain(workers) {
        if let Some(Value::Object(player)) = campaign.get_mut("player")
            && let Some(level) = player.get("level").and_then(Value::as_u64)
            && level >= 2
        {
            player
                .entry("level_grant_paid")
                .or_insert_with(|| Value::from(level));
        }
    }
    Ok(())
}

/// The milestone ranks at save format v7, frozen for sub-step (d): the
/// milestones a v6 campaign had reached are claimed, whatever the current
/// catalogue authors. Progression settlement
/// (`Game::settle_progression_rewards`) claims the same ranks, unpaid, when
/// the catalogue authors no reward for them.
pub(crate) const V7_MILESTONE_RANKS: [u32; 4] = [5, 10, 15, 20];

/// A frozen copy of `Campaign::progression_rank` on a v6 campaign as it will
/// load: a pre-threat campaign (revision 0) counts the larger of its
/// victories and its legacy clear rank, which `migrate_threat_history` folds
/// into victories at load; otherwise its victories.
fn v6_rank(campaign: &Value) -> u32 {
    let victories = campaign
        .get("victories")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let threat_revision = campaign
        .get("threat_revision")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let rank = if threat_revision == 0 {
        victories.max(u64::from(v6_legacy_clear_rank(campaign)))
    } else {
        victories
    };
    u32::try_from(rank).unwrap_or(u32::MAX)
}

/// The frozen `Campaign::legacy_clear_rank`: per country the larger of its
/// clear high-water and completed levels, summed, and at least the number of
/// first clears.
fn v6_legacy_clear_rank(campaign: &Value) -> u32 {
    let mut cleared: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    let world = campaign.get("world");
    for key in ["clear_high_water", "country_levels"] {
        if let Some(Value::Object(counts)) = world.and_then(|world| world.get(key)) {
            for (id, count) in counts {
                let count = count.as_u64().unwrap_or(0);
                let slot = cleared.entry(id.as_str()).or_insert(count);
                *slot = (*slot).max(count);
            }
        }
    }
    let first_clears = campaign
        .get("first_clears")
        .and_then(Value::as_array)
        .map_or(0, Vec::len) as u64;
    cleared
        .values()
        .fold(0u64, |sum, count| sum.saturating_add(*count))
        .max(first_clears) as u32
}

/// (d) The root campaign claims every frozen milestone its v6 rank reached,
/// so none is paid retroactively, and every expedition campaign gets the
/// parent's claims. An expedition cloned at a lower rank keeps only the
/// claims its own rank reached, so the claims never exceed the rank they are
/// validated against. Nothing is written below rank 5.
fn v7_milestones_claimed(value: &mut Value) -> Result<(), SaveError> {
    let (Some(root), workers) = v6_campaigns(value)? else {
        return Ok(());
    };
    let parent = v6_rank(root);
    for campaign in std::iter::once(root).chain(workers) {
        let rank = parent.min(v6_rank(campaign));
        let claimed: Vec<u32> = V7_MILESTONE_RANKS
            .into_iter()
            .filter(|milestone| *milestone <= rank)
            .collect();
        if let Value::Object(campaign) = campaign
            && !claimed.is_empty()
        {
            campaign
                .entry("milestones_claimed")
                .or_insert_with(|| Value::from(claimed));
        }
    }
    Ok(())
}

/// The expedition records; a missing array means there are none.
fn v6_battles(battles: Option<&mut Value>) -> Result<&mut [Value], SaveError> {
    match battles {
        None | Some(Value::Null) => Ok(&mut []),
        Some(Value::Array(battles)) => Ok(battles),
        Some(_) => Err(SaveError::Migration("v6 battles is not an array".into())),
    }
}

/// (a) Every worker game's attempt and report gain `"defender": {"Recruit":
/// recruit_id}`. The root attempt and report stay keyless: absent means the
/// mode owner (the player for Personal, Mara for Npc), which is what a v6 root
/// meant.
fn v7_worker_seats(value: &mut Value) -> Result<(), SaveError> {
    for battle in v6_battles(value.get_mut("battles"))? {
        let recruit = battle
            .get("recruit_id")
            .and_then(Value::as_u64)
            .ok_or_else(|| SaveError::Migration("v6 battle without a u64 recruit_id".into()))?;
        let Some(game) = battle.get_mut("game").and_then(Value::as_object_mut) else {
            continue;
        };
        for key in ["attempt", "last_result"] {
            if let Some(Value::Object(record)) = game.get_mut(key) {
                record
                    .entry("defender")
                    .or_insert_with(|| serde_json::json!({ "Recruit": recruit }));
            }
        }
    }
    Ok(())
}

/// (e) An Npc attempt profile without an appearance reads as Female, the
/// appearance every v6 Npc deploy stamped (the serde default is Male). Saved
/// appearances are kept, and v6 expeditions are not re-stamped with their
/// recruit's identity. Runs after (a), which rejects malformed battles.
fn v7_npc_appearance(value: &mut Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };
    let mut attempts = Vec::new();
    for (key, child) in root.iter_mut() {
        match key.as_str() {
            "attempt" => attempts.push(child),
            "battles" => {
                if let Ok(battles) = v6_battles(Some(child)) {
                    attempts.extend(
                        battles
                            .iter_mut()
                            .filter_map(|battle| battle.get_mut("game")?.get_mut("attempt")),
                    );
                }
            }
            _ => {}
        }
    }
    for attempt in attempts {
        if attempt.get("mode").and_then(Value::as_str) == Some("Npc")
            && let Some(Value::Object(profile)) = attempt.get_mut("profile")
        {
            profile
                .entry("appearance")
                .or_insert_with(|| Value::from("Female"));
        }
    }
}

/// (b) The root report and every worker report gain `drop_ids`, the sorted
/// item ids of their loot receipts, and `loot_by_quality`. The root report
/// takes the counts of its own run record (`attempt_id` equal, `worker_id`
/// null), which saw every drop; otherwise the receipts are counted. Keys
/// are inserted only when absent and only with a drop, so a report without
/// receipts stays as it was. Every other v7 loot field (the auto-sell rule,
/// attempt drop tracking, receipts, pending sales) relies on its serde
/// default: off and empty. Runs after (a), which rejects malformed battles.
fn v7_report_drops(value: &mut Value) -> Result<(), SaveError> {
    let Some(root) = value.as_object_mut() else {
        return Ok(());
    };
    let history = root
        .get("campaign")
        .and_then(|campaign| campaign.get("run_history"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(report) = root.get_mut("last_result") {
        let attempt = report.get("attempt_id").and_then(Value::as_u64);
        let recorded = history
            .iter()
            .find(|record| {
                attempt.is_some()
                    && record.get("attempt_id").and_then(Value::as_u64) == attempt
                    && record.get("worker_id").is_none_or(Value::is_null)
            })
            .and_then(|record| record.get("loot_by_quality"));
        v7_report_drop_keys(report, recorded)?;
    }
    if let Some(battles) = root.get_mut("battles") {
        for battle in v6_battles(Some(battles))? {
            if let Some(report) = battle
                .get_mut("game")
                .and_then(|game| game.get_mut("last_result"))
            {
                v7_report_drop_keys(report, None)?;
            }
        }
    }
    Ok(())
}

fn v7_report_drop_keys(report: &mut Value, recorded: Option<&Value>) -> Result<(), SaveError> {
    let Some(report) = report.as_object_mut() else {
        return Ok(());
    };
    let malformed = || SaveError::Migration("v6 report has a malformed loot receipt".into());
    let events = match report.get("loot_events") {
        None | Some(Value::Null) => &Vec::new(),
        Some(Value::Array(events)) => events,
        Some(_) => return Err(malformed()),
    };
    let mut ids = Vec::with_capacity(events.len());
    let mut counted = [0u64; V3_QUALITIES.len()];
    for event in events {
        ids.push(
            event
                .get("item_id")
                .and_then(Value::as_u64)
                .ok_or_else(malformed)?,
        );
        let quality = event.get("quality").and_then(Value::as_str);
        let slot = V3_QUALITIES
            .iter()
            .position(|name| Some(*name) == quality)
            .ok_or_else(malformed)?;
        counted[slot] += 1;
    }
    ids.sort_unstable();
    ids.dedup();
    if !ids.is_empty() && !report.contains_key("drop_ids") {
        report.insert("drop_ids".into(), Value::from(ids));
    }
    if !report.contains_key("loot_by_quality") {
        let counts = match recorded {
            Some(recorded) => recorded.clone(),
            // v6 quality names are the v3 ones (Quality was never reordered).
            None => Value::Object(
                V3_QUALITIES
                    .iter()
                    .zip(counted)
                    .map(|(name, n)| ((*name).to_owned(), Value::from(n)))
                    .collect(),
            ),
        };
        let any = counts
            .as_object()
            .is_some_and(|map| map.values().any(|n| n.as_u64() != Some(0)));
        if any {
            report.insert("loot_by_quality".into(), counts);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::{BodyAttribute, NpcTraining};
    use crate::content::{EnemyKind, StructureKind, content};
    use crate::sim::NpcManeuver;
    use serde_json::json;

    fn serde_names<T: serde::Serialize>(kinds: &[T]) -> Vec<String> {
        kinds
            .iter()
            .map(|kind| json!(kind).as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn frozen_v1_orders_match_the_kinds_they_replaced() {
        // The constants are frozen on purpose; this documents that the v1 slot
        // order equals the declaration order the v1 arrays were indexed by.
        assert_eq!(serde_names(&EnemyKind::ALL), V1_ENEMY_KINDS);
        assert_eq!(serde_names(&StructureKind::ALL), V1_STRUCTURE_KINDS);
        assert_eq!(serde_names(&BodyAttribute::ALL), V1_BODY_ATTRIBUTES);
        assert_eq!(serde_names(&NpcTraining::ALL), V1_NPC_TRAINING);
        assert_eq!(
            serde_names(<NpcManeuver as crate::kind_map::StableKind>::ALL),
            V1_NPC_MANEUVERS
        );
        // Every frozen v1 item still resolves in the bundled catalogue.
        for id in V1_ITEM_IDS {
            assert!(content().item_index(&id.into()).is_some(), "{id}");
        }
    }

    #[test]
    fn v1_positions_become_ids_and_maps_everywhere_and_the_step_is_idempotent() {
        let v1 = json!({
            "campaign": {
                "items": [{"id": 9, "definition": 24}],
                "blueprints": [true, false, true],
                "structure_upgrades": [0, 2, 1],
                "player": {"body": [1, 0, 0, 0, 2]},
                "npc": {"investments": [1, 0, 0, 0, 3, 0, 0, 0]},
                "run_history": [{"gear": [{"definition": 0}]}]
            },
            "attempt": {
                "telemetry": {"kills_by_kind": [4, 0, 1, 0, 0], "npc_decisions": [0, 5, 0, 0, 0, 0, 0, 1]},
                "loot_events": [{"definition": 12}],
                "profile": {
                    "modifiers": {"body_development": [0, 0, 1, 0, 0], "blueprints": [false, false, true], "structure_ranks": [0, 0, 3]},
                    "equipment_visual": {"outfit": {"definition": 2}}
                }
            },
            "battles": [{"recruit_id": 1, "game": {"campaign": {"items": [{"definition": 3}]}}}]
        });
        let v2 = migrate(v1, 1).unwrap();
        assert_eq!(
            v2["campaign"]["items"][0]["definition"],
            "patrol_rifle_tier1"
        );
        assert_eq!(v2["campaign"]["items"][0]["id"], 9);
        assert_eq!(
            v2["campaign"]["blueprints"],
            json!({"Trap": true, "Wall": false, "Turret": true})
        );
        assert_eq!(v2["campaign"]["structure_upgrades"]["Wall"], 2);
        assert_eq!(v2["campaign"]["player"]["body"]["Perception"], 2);
        assert_eq!(v2["campaign"]["npc"]["investments"]["Guns"], 3);
        assert_eq!(
            v2["campaign"]["run_history"][0]["gear"][0]["definition"],
            "service_carbine"
        );
        assert_eq!(v2["attempt"]["telemetry"]["kills_by_kind"]["Brute"], 1);
        assert_eq!(
            v2["attempt"]["telemetry"]["npc_decisions"]["AllOutDefense"],
            1
        );
        assert_eq!(
            v2["attempt"]["loot_events"][0]["definition"],
            "longwatch_rifle"
        );
        assert_eq!(
            v2["attempt"]["profile"]["modifiers"]["structure_ranks"]["Turret"],
            3
        );
        assert_eq!(
            v2["attempt"]["profile"]["equipment_visual"]["outfit"]["definition"],
            "work_jacket"
        );
        assert_eq!(
            v2["battles"][0]["game"]["campaign"]["items"][0]["definition"],
            "field_webbing"
        );
        assert_eq!(v1_to_v2(v2.clone()).unwrap(), v2);
    }

    #[test]
    fn malformed_v1_shapes_and_versions_are_typed_errors() {
        assert!(matches!(
            migrate(json!({"definition": 29}), 1),
            Err(SaveError::Migration(_))
        ));
        assert!(matches!(
            migrate(json!({"blueprints": [true, true]}), 1),
            Err(SaveError::Migration(_))
        ));
        assert!(matches!(
            migrate(json!({}), 0),
            Err(SaveError::UnsupportedVersion(0))
        ));
        assert!(matches!(
            migrate(json!({}), SAVE_VERSION + 1),
            Err(SaveError::UnsupportedVersion(_))
        ));
        assert_eq!(
            migrate(json!({"x": 1}), SAVE_VERSION).unwrap(),
            json!({"x": 1})
        );
    }

    #[test]
    fn frozen_v2_names_match_the_ids_that_replaced_them() {
        use crate::skill_catalog::unlocks;
        use crate::status::StatusId;
        let ids: Vec<_> = V2_UNLOCK_FLAGS.iter().map(|(_, id)| *id).collect();
        assert_eq!(ids, unlocks::ALL);
        assert_eq!(
            V2_STATUS_COUNTERS
                .iter()
                .map(|(_, id)| *id)
                .collect::<Vec<_>>(),
            serde_names(&[StatusId::HitStop, StatusId::Brace])
        );
    }

    #[test]
    fn v2_counters_flags_and_snapshots_become_v3_and_the_step_is_idempotent() {
        let snapshot =
            |kind: &str| json!({"archetype_id": "x", "kind": kind, "npc_damage_dice": 1});
        let v2 = json!({
            "attempt": {
                "brace_left": 12,
                "profile": {
                    "brace": true,
                    "modifiers": {
                        "cadence_multiplier": 1.0, "piercing": true, "sweep": false,
                        "heal": false, "brace": true, "grenade_unlocked": true,
                        "molotov_unlocked": false
                    }
                },
                "enemies": [
                    {"hit_stop_left": 4, "resolved": snapshot("Runner")},
                    {"hit_stop_left": 0, "resolved": snapshot("Brute")},
                    {"hit_stop_left": 0, "resolved": null}
                ]
            },
            "battles": [{"recruit_id": 1, "game": {"attempt": {"brace_left": 0, "enemies": []}}}]
        });
        let v3 = migrate(v2, 2).unwrap();
        let a = &v3["attempt"];
        assert_eq!(
            a["statuses"],
            json!([{"id": "Brace", "remaining_ticks": 12, "stacks": 1, "magnitude": 0}])
        );
        assert!(a.get("brace_left").is_none());
        // Only skill modifiers lose their flags; the profile's own `brace` stays.
        assert_eq!(a["profile"]["brace"], true);
        assert_eq!(
            a["profile"]["modifiers"],
            json!({"cadence_multiplier": 1.0, "unlocks": ["brace", "grenade", "piercing"]})
        );
        assert_eq!(
            a["enemies"][0]["statuses"][0],
            json!({"id": "HitStop", "remaining_ticks": 4, "stacks": 1, "magnitude": 0})
        );
        assert_eq!(a["enemies"][0]["resolved"]["target_urgency"], -3.0);
        assert_eq!(a["enemies"][1]["resolved"]["target_urgency"], 0.0);
        assert_eq!(
            a["enemies"][1],
            json!({"resolved": {"archetype_id": "x", "kind": "Brute", "npc_damage_dice": 1, "target_urgency": 0.0}})
        );
        assert_eq!(
            v3["battles"][0]["game"]["attempt"],
            json!({"enemies": [], "defender": {"Recruit": 1}})
        );
        assert_eq!(v2_to_v3(v3.clone()), v3);
    }

    #[test]
    fn frozen_v3_quality_order_matches_the_kind_that_replaced_it() {
        use crate::quality::Quality;
        assert_eq!(serde_names(&Quality::ALL), V3_QUALITIES);
    }

    #[test]
    fn v3_quality_arrays_become_maps_everywhere_and_the_step_is_idempotent() {
        let v3 = json!({
            "attempt": {"loot_quality_counts": [0, 2, 1, 0, 1, 3]},
            "campaign": {"run_history": [{"loot_by_quality": [1, 0, 0, 0, 0, 5]}]},
            "battles": [{"recruit_id": 1, "game": {"attempt": {"loot_quality_counts": [0, 0, 0, 9, 0, 0]}}}]
        });
        let v4 = migrate(v3, 3).unwrap();
        assert_eq!(
            v4["attempt"]["loot_quality_counts"],
            json!({"Poor": 0, "Common": 2, "Uncommon": 1, "Rare": 0, "Epic": 1, "Legendary": 3})
        );
        assert_eq!(
            v4["campaign"]["run_history"][0]["loot_by_quality"]["Legendary"],
            5
        );
        assert_eq!(
            v4["battles"][0]["game"]["attempt"]["loot_quality_counts"]["Rare"],
            9
        );
        assert_eq!(v3_to_v4(v4.clone()).unwrap(), v4);
        assert!(matches!(
            migrate(json!({"loot_by_quality": [1, 2]}), 3),
            Err(SaveError::Migration(_))
        ));
    }

    #[test]
    fn frozen_v4_ordnance_order_matches_the_kind_that_replaced_it() {
        use crate::kind_map::StableKind;
        use crate::ordnance::OrdnanceKind;
        assert_eq!(serde_names(OrdnanceKind::ALL), V4_ORDNANCE_KINDS);
    }

    #[test]
    fn v4_ordnance_arrays_become_maps_everywhere_and_the_step_is_idempotent() {
        let v4 = json!({
            "attempt": {"ordnance": {
                "projectiles": [], "next_id": 3,
                "cooldowns": [10, 61], "charges": [1, 4], "issued": [2, 5], "spent": [1, 1]
            }},
            "battles": [{"recruit_id": 1, "game": {"attempt": {"ordnance": {"charges": [0, 7]}}}}]
        });
        let v5 = migrate(v4, 4).unwrap();
        let o = &v5["attempt"]["ordnance"];
        assert_eq!(o["cooldowns"], json!({"Grenade": 10, "Molotov": 61}));
        assert_eq!(o["charges"], json!({"Grenade": 1, "Molotov": 4}));
        assert_eq!(o["issued"], json!({"Grenade": 2, "Molotov": 5}));
        assert_eq!(o["spent"], json!({"Grenade": 1, "Molotov": 1}));
        assert_eq!(o["next_id"], 3);
        assert_eq!(
            v5["battles"][0]["game"]["attempt"]["ordnance"]["charges"]["Molotov"],
            7
        );
        assert_eq!(v4_to_v5(v5.clone()).unwrap(), v5);
        assert!(matches!(
            migrate(json!({"ordnance": {"charges": [1]}}), 4),
            Err(SaveError::Migration(_))
        ));
        // Arrays with these names outside an ordnance state are not ordnance.
        let other = json!({"npc": {"spent": [1, 2, 3]}});
        assert_eq!(v4_to_v5(other.clone()).unwrap(), other);
    }

    #[test]
    fn frozen_v5_level_order_matches_the_bundled_catalogue_it_replaced() {
        let ids: Vec<_> = content().levels.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, V5_LEVEL_IDS);
    }

    #[test]
    fn v5_level_positions_become_ids_everywhere_and_the_step_is_idempotent() {
        let npc = json!({
            "first_clears": [true, false, false, false, false],
            "first_clear_claimed": [2, 0, 0, 0, 0]
        });
        let v5 = json!({
            "campaign": {
                "unlocked": 4,
                "first_clears": [true, true, true, false, false],
                "npc": npc,
                "run_history": [{"country_name": "", "stage": 0, "template": 2}],
                "world": {"unlocked": ["FRA"]},
                "items": [{"id": 1, "level": 3, "definition": "crowbar"}]
            },
            "attempt": {"country_name": "", "stage": 3, "level": 4},
            "last_result": {"country_name": "", "stage": 0, "level": 1},
            "strategy": {"roster": [{"npc": {
                "first_clears": [false, true, false, false, false],
                "first_clear_claimed": [0, 1, 0, 0, 0]
            }}]},
            "battles": [{"recruit_id": 1, "stage": 2, "game": {"campaign": {
                "unlocked": 1,
                "first_clears": [false, false, false, false, false]
            }}}]
        });
        let v6 = migrate(v5, 5).unwrap();
        let c = &v6["campaign"];
        assert!(c.get("unlocked").is_none());
        assert_eq!(
            c["first_clears"],
            json!(["ashwood_crossing", "depot_nine", "the_old_road"])
        );
        assert_eq!(c["npc"]["first_clears"], json!(["the_old_road"]));
        assert_eq!(c["npc"]["first_clear_claimed"], json!({"the_old_road": 2}));
        assert_eq!(c["run_history"][0]["template"], "depot_nine");
        // Country unlocks, item levels and expedition stages are not levels.
        assert_eq!(c["world"]["unlocked"], json!(["FRA"]));
        assert_eq!(c["items"][0]["level"], 3);
        assert_eq!(v6["battles"][0]["stage"], 2);
        assert_eq!(v6["attempt"]["level"], "lastlight_checkpoint");
        assert_eq!(v6["attempt"]["stage"], 3);
        assert_eq!(v6["last_result"]["level"], "ashwood_crossing");
        let recruit = &v6["strategy"]["roster"][0]["npc"];
        assert_eq!(recruit["first_clears"], json!(["ashwood_crossing"]));
        assert_eq!(
            recruit["first_clear_claimed"],
            json!({"ashwood_crossing": 1})
        );
        let nested = &v6["battles"][0]["game"]["campaign"];
        assert_eq!(nested["first_clears"], json!([]));
        assert!(nested.get("unlocked").is_none());
        assert_eq!(v5_to_v6(v6.clone()).unwrap(), v6);
    }

    #[test]
    fn malformed_v5_level_shapes_are_typed_errors() {
        for bad in [
            json!({"first_clears": [true, false]}),
            json!({"first_clear_claimed": [0, 0, 0]}),
            json!({"country_name": "", "stage": 0, "level": 5}),
            json!({"country_name": "", "template": 9}),
        ] {
            assert!(
                matches!(migrate(bad.clone(), 5), Err(SaveError::Migration(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn v6_worker_games_gain_their_recruit_seat_root_records_stay_keyless() {
        let v6 = json!({
            "attempt": {"mode": "Npc", "profile": {"appearance": "Female"}},
            "last_result": {"mode": "Npc", "outcome": "Victory"},
            "battles": [
                {"recruit_id": 3, "settled": false, "game": {
                    "attempt": {"mode": "Npc", "profile": {"appearance": "Female"}},
                    "last_result": {"mode": "Npc", "outcome": "Victory"}
                }},
                {"recruit_id": 2, "settled": true, "game": {
                    "attempt": null,
                    "last_result": {"mode": "Npc", "outcome": "Defeat"}
                }},
                {"recruit_id": 5, "settled": false, "game": {
                    "attempt": {"mode": "Npc", "profile": {"appearance": "Male"}, "defender": {"Recruit": 9}}
                }}
            ]
        });
        let v7 = migrate(v6.clone(), 6).unwrap();
        // The root attempt and report are never touched: absent = mode owner.
        assert_eq!(v7["attempt"], v6["attempt"]);
        assert_eq!(v7["last_result"], v6["last_result"]);
        let seat =
            |battle: usize, key: &str| v7["battles"][battle]["game"][key]["defender"].clone();
        assert_eq!(seat(0, "attempt"), json!({"Recruit": 3}));
        assert_eq!(seat(0, "last_result"), json!({"Recruit": 3}));
        assert!(v7["battles"][1]["game"]["attempt"].is_null());
        assert_eq!(seat(1, "last_result"), json!({"Recruit": 2}));
        // An existing key is kept (or_insert), and a missing report stays missing.
        assert_eq!(seat(2, "attempt"), json!({"Recruit": 9}));
        assert!(v7["battles"][2]["game"].get("last_result").is_none());
        // No acknowledged flag and nothing else on the battle records.
        for (old, new) in v6["battles"]
            .as_array()
            .unwrap()
            .iter()
            .zip(v7["battles"].as_array().unwrap())
        {
            let keys = |b: &Value| b.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
            assert_eq!(keys(old), keys(new));
        }
        assert_eq!(v6_to_v7(v7.clone()).unwrap(), v7, "idempotent");
        // A save without expeditions has nothing to migrate.
        let idle = json!({"attempt": null, "last_result": null});
        assert_eq!(migrate(idle.clone(), 6).unwrap(), idle);
    }

    #[test]
    fn malformed_v6_battle_without_recruit_id_is_a_typed_migration_error() {
        for bad in [
            json!({"battles": [{"game": {"attempt": null}}]}),
            json!({"battles": [{"recruit_id": "3", "game": {}}]}),
            json!({"battles": [{"recruit_id": -1, "game": {}}]}),
            json!({"battles": [{"recruit_id": 1.5, "game": {}}]}),
            json!({"battles": [7]}),
            json!({"battles": {"recruit_id": 3}}),
        ] {
            assert!(
                matches!(migrate(bad.clone(), 6), Err(SaveError::Migration(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn v6_npc_attempt_without_appearance_loads_female() {
        let v6 = json!({
            "attempt": {"mode": "Npc", "profile": {"weapon": {}}},
            "last_result": {"mode": "Npc"},
            "battles": [
                {"recruit_id": 1, "game": {"attempt": {"mode": "Npc", "profile": {}}}},
                {"recruit_id": 2, "game": {"attempt": {"mode": "Npc", "profile": {"appearance": "Male"}}}},
                {"recruit_id": 3, "game": {"attempt": {"mode": "Personal", "profile": {}}}}
            ]
        });
        let v7 = migrate(v6, 6).unwrap();
        assert_eq!(
            v7["attempt"]["profile"],
            json!({"weapon": {}, "appearance": "Female"})
        );
        assert!(v7["last_result"].get("profile").is_none());
        let appearance = |battle: usize| {
            v7["battles"][battle]["game"]["attempt"]["profile"]
                .get("appearance")
                .cloned()
        };
        assert_eq!(appearance(0), Some(json!("Female")));
        // or_insert: a saved appearance is kept, and Personal profiles are untouched.
        assert_eq!(appearance(1), Some(json!("Male")));
        assert_eq!(appearance(2), None);
        assert_eq!(v6_to_v7(v7.clone()).unwrap(), v7, "idempotent");
    }

    fn loot_event(item_id: u64, quality: &str) -> Value {
        json!({"item_id": item_id, "quality": quality, "definition": "fire_axe"})
    }

    fn counts(
        poor: u32,
        common: u32,
        uncommon: u32,
        rare: u32,
        epic: u32,
        legendary: u32,
    ) -> Value {
        json!({"Poor": poor, "Common": common, "Uncommon": uncommon,
               "Rare": rare, "Epic": epic, "Legendary": legendary})
    }

    #[test]
    fn v6_report_drop_ids_and_quality_counts_come_from_loot_events() {
        let v6 = json!({
            "campaign": {"run_history": [
                // Same attempt id, but an expedition's record: never matched.
                {"attempt_id": 7, "worker_id": 2, "loot_by_quality": counts(0, 0, 0, 0, 0, 9)},
                // The root report's own record: its counts are complete.
                {"attempt_id": 7, "worker_id": null, "loot_by_quality": counts(1, 4, 2, 0, 1, 0)},
                {"attempt_id": 8, "worker_id": null, "loot_by_quality": counts(0, 0, 0, 0, 0, 0)}
            ]},
            "last_result": {"attempt_id": 7, "loot_events": [
                loot_event(41, "Rare"), loot_event(12, "Common"), loot_event(30, "Common")
            ]},
            "battles": [
                // No root record applies inside a worker: count its receipts.
                {"recruit_id": 1, "game": {"last_result": {"attempt_id": 7, "loot_events": [
                    loot_event(9, "Epic"), loot_event(3, "Poor"), loot_event(6, "Epic")
                ]}}},
                // A report without drops gains no key.
                {"recruit_id": 2, "game": {"last_result": {"attempt_id": 1, "loot_events": []}}},
                {"recruit_id": 3, "game": {"attempt": null, "last_result": null}}
            ]
        });
        let v7 = migrate(v6.clone(), 6).unwrap();
        let root = &v7["last_result"];
        assert_eq!(root["drop_ids"], json!([12, 30, 41]), "sorted item ids");
        assert_eq!(root["loot_by_quality"], counts(1, 4, 2, 0, 1, 0));
        let worker = &v7["battles"][0]["game"]["last_result"];
        assert_eq!(worker["drop_ids"], json!([3, 6, 9]));
        assert_eq!(worker["loot_by_quality"], counts(1, 0, 0, 0, 2, 0));
        let empty = v7["battles"][1]["game"]["last_result"].as_object().unwrap();
        assert!(!empty.contains_key("drop_ids") && !empty.contains_key("loot_by_quality"));
        assert!(v7["battles"][2]["game"]["last_result"].is_null());
        // A root report without a matching record counts its receipts, and
        // an all-zero record adds no key; saved values are kept (or_insert).
        let mut unmatched = v6.clone();
        unmatched["last_result"]["attempt_id"] = json!(5);
        let v7 = migrate(unmatched, 6).unwrap();
        assert_eq!(
            v7["last_result"]["loot_by_quality"],
            counts(0, 2, 0, 1, 0, 0)
        );
        let mut zero = v6.clone();
        zero["last_result"] = json!({"attempt_id": 8, "loot_events": []});
        let v7 = migrate(zero, 6).unwrap();
        assert!(v7["last_result"].get("loot_by_quality").is_none());
        let mut kept = v6.clone();
        kept["last_result"]["drop_ids"] = json!([41]);
        kept["last_result"]["loot_by_quality"] = counts(0, 0, 0, 1, 0, 0);
        let v7 = migrate(kept.clone(), 6).unwrap();
        assert_eq!(
            v7["last_result"]["drop_ids"],
            kept["last_result"]["drop_ids"]
        );
        assert_eq!(
            v7["last_result"]["loot_by_quality"],
            kept["last_result"]["loot_by_quality"]
        );
        let v7 = migrate(v6, 6).unwrap();
        assert_eq!(v6_to_v7(v7.clone()).unwrap(), v7, "idempotent");
        // Malformed receipts are typed errors, not panics.
        for bad in [
            json!({"last_result": {"loot_events": [{"item_id": "3", "quality": "Rare"}]}}),
            json!({"last_result": {"loot_events": [{"item_id": 3, "quality": "Shiny"}]}}),
            json!({"last_result": {"loot_events": {"item_id": 3}}}),
        ] {
            assert!(
                matches!(migrate(bad.clone(), 6), Err(SaveError::Migration(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn v7_ledgers_mark_levels_paid_and_claim_reached_milestones_idempotently() {
        let campaign = |victories: u64, level: u32| json!({"victories": victories, "threat_revision": 1, "first_clears": [], "player": {"level": level}});
        let v6 = json!({
            "campaign": campaign(15, 7),
            "battles": [
                {"recruit_id": 1, "game": {"campaign": campaign(15, 1)}},
                {"recruit_id": 2, "game": {"campaign": campaign(9, 4)}},
                {"recruit_id": 3}
            ]
        });
        let v7 = migrate(v6, 6).unwrap();
        assert_eq!(v7["campaign"]["player"]["level_grant_paid"], 7);
        assert_eq!(v7["campaign"]["milestones_claimed"], json!([5, 10, 15]));
        let worker = |n: usize| &v7["battles"][n]["game"]["campaign"];
        // Level 1 writes nothing; the parent's claims are copied.
        assert!(worker(0)["player"].get("level_grant_paid").is_none());
        assert_eq!(worker(0)["milestones_claimed"], json!([5, 10, 15]));
        // An expedition cloned below the parent's rank keeps only the claims
        // its own rank reached, so it still validates.
        assert_eq!(worker(1)["player"]["level_grant_paid"], 4);
        assert_eq!(worker(1)["milestones_claimed"], json!([5]));
        assert!(v7["battles"][2].get("game").is_none());
        assert_eq!(v6_to_v7(v7.clone()).unwrap(), v7, "idempotent");
        // Saved values are kept (or_insert).
        let mut kept = v7.clone();
        kept["campaign"]["player"]["level_grant_paid"] = json!(3);
        kept["campaign"]["milestones_claimed"] = json!([5]);
        assert_eq!(v6_to_v7(kept.clone()).unwrap(), kept);
        // Below rank 5 at level 1, and with no campaign, nothing is written;
        // rank 20 and above claims all four.
        for unchanged in [json!({"campaign": campaign(4, 1)}), json!({})] {
            assert_eq!(migrate(unchanged.clone(), 6).unwrap(), unchanged);
        }
        let v7 = migrate(json!({"campaign": campaign(40, 50)}), 6).unwrap();
        assert_eq!(
            v7["campaign"]["milestones_claimed"],
            json!(V7_MILESTONE_RANKS)
        );
    }

    /// Each checked-in fixture as saved, and as a pre-threat (revision 0)
    /// save with no victories, so the legacy clear rank decides. The v1 root
    /// keeps its 4 victories: its live attempt is at rank 4, above its three
    /// clears, and would not validate.
    fn rank_fixtures() -> Vec<(String, u32, Value)> {
        let fixtures = [
            include_str!("../../tests/fixtures/save_v1.json"),
            include_str!("../../tests/fixtures/save_v2.json"),
            include_str!("../../tests/fixtures/save_v3.json"),
            include_str!("../../tests/fixtures/save_v4.json"),
            include_str!("../../tests/fixtures/save_v5.json"),
            include_str!("../../tests/fixtures/save_v6.json"),
        ];
        let mut out = Vec::new();
        for (n, text) in fixtures.into_iter().enumerate() {
            let version = n as u32 + 1;
            let envelope: Value = serde_json::from_str(text).unwrap();
            assert_eq!(envelope["version"], version);
            let payload: Value =
                serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
            let pre_threat = |campaign: &mut Value, victories: u64| {
                let campaign = campaign.as_object_mut().unwrap();
                campaign.remove("threat_revision");
                campaign.insert("victories".into(), json!(victories));
            };
            let mut legacy = payload.clone();
            pre_threat(&mut legacy["campaign"], if version == 1 { 4 } else { 0 });
            if let Some(Value::Array(battles)) = legacy.get_mut("battles") {
                for battle in battles {
                    pre_threat(&mut battle["game"]["campaign"], 0);
                }
            }
            out.push((format!("v{version}"), version, payload));
            out.push((format!("v{version} pre-threat"), version, legacy));
        }
        out
    }

    /// Parity pin: the frozen `v6_rank` equals the rank the loaded game
    /// reports (after `migrate_threat_history`), for every fixture's root and
    /// expedition campaigns.
    #[test]
    fn v6_rank_migration_matches_runtime_progression_rank_on_every_fixture() {
        let mut legacy_ranks = Vec::new();
        for (label, version, payload) in rank_fixtures() {
            let migrated = migrate(payload.clone(), version).unwrap();
            let game = crate::sim::Game::from_payload(payload, version)
                .unwrap_or_else(|e| panic!("{label}: {e}"));
            let rank = v6_rank(&migrated["campaign"]);
            assert_eq!(rank, game.campaign().progression_rank(), "{label}");
            if label.ends_with("pre-threat") {
                legacy_ranks.push(rank);
            }
            for (n, battle) in game.battles().iter().enumerate() {
                assert_eq!(
                    v6_rank(&migrated["battles"][n]["game"]["campaign"]),
                    battle.game().campaign().progression_rank(),
                    "{label} expedition {n}"
                );
            }
        }
        // The pre-threat v5 and v6 ranks come from their clears alone.
        assert_eq!(legacy_ranks, [4, 0, 0, 0, 2, 14]);
    }
}
