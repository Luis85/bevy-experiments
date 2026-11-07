# Save format

The current save envelope is **v7** (`SAVE_VERSION` in
`crates/horde_core/src/sim/migrations.rs`). Storage, recovery and the 64 MiB
generation limit are described in [SIMULATION.md](SIMULATION.md).

## Envelope

A save generation is one JSON object:

| Key | Meaning |
|---|---|
| `version` | Envelope version of the writing build. 0 and anything above `SAVE_VERSION` are rejected as unsupported (a v7 build rejects version 8). |
| `checksum` | FNV-1a of `payload`. |
| `ruleset_version` | Optional. A mismatch is logged, never fatal. |
| `content_fingerprint` | Optional. Catalogue drift is logged once per distinct pair. |
| `payload` | The serialized `Game`, as a string. |

## Loading

Every loaded game passes through one shared step, `settle_decoded`
(`sim/persistence.rs`): it reconciles the decoded game (catalogue-aware fills,
`reconcile_saved_game`) and then validates it. Two paths reach it. A save
generation at the current version (v7) is decoded straight from the envelope
payload, without an intermediate JSON tree, and handed to `settle_decoded`; it
never reaches `Game::from_payload`. `Game::from_payload(value, version)`
migrates an older payload on raw JSON first, then decodes it and calls
`settle_decoded`. A save generation at an older version loads through it. So
a fill that every loaded game needs belongs in `settle_decoded` (in
`reconcile_saved_game`), not in `from_payload`, which v7 saves skip.

The debug replay tool also uses `from_payload` for a recording whose header
version is older than this build's, so the recorded initial state loads the
way that build's save would. A current-version recording is decoded as
recorded.

Migrations are ordered, one step per version. Each step freezes the meaning of
its source version, never reads the loaded catalogues and only fills in what is
missing, so running it twice gives the same value. Saves from v1 to v6 keep
loading. A migrated save is written back at the current version, and a second
resave is byte-identical to the first.

| Step | Change |
|---|---|
| v1 to v2 | Positional item definitions become item IDs; enum-indexed arrays become kind-keyed maps. |
| v2 to v3 | Status counters become statuses; skill unlock flags become an ID set; enemy snapshots gain `target_urgency`. |
| v3 to v4 | Quality-indexed loot counters become quality-keyed maps. |
| v4 to v5 | Ordnance counters become kind-keyed maps. |
| v5 to v6 | Positional level progress becomes level IDs. |
| v6 to v7 | Defender seats on expedition games, the Npc appearance back-fill, report drop tracking and the progression ledgers (below). |

## v7

### Defender seat

`Attempt` and `Report` carry an optional `"defender"` key: the seat of the
defender who fights the run. When the key is absent, the run belongs to the
mode owner: the player for `Personal`, Mara for `Npc`. A recruit seat is
written as `{"Recruit": id}`.

Every expedition deploy, and every later stage of a country operation, seats
its recruit on the worker attempt and gives the attempt profile that recruit's
identity appearance. Loading checks that each expedition game's `attempt` and
`last_result`, when present, have the seat `{"Recruit": recruit_id}` of their
`battles[*]` record; any other seat, including a missing key, is invalid.

### Recruit takeover and next defender

A recruit seat on the root `attempt` is a main-run takeover. Loading rejects
any of these as invalid:

- a `Personal` attempt or report with a recruit seat;
- a root takeover unless exactly one roster recruit is `Defending`, it is the
  seated one, `campaign.reserved` is `"Npc"`, `campaign.reserved_recruit`
  names it, its loadout is in `reserved_items`, and it has no guard and no
  running expedition;
- a `Defending` recruit or a `reserved_recruit` without a root takeover;
- a root report whose recruit seat is not on the roster (any status);
- an expedition game with `reserved_recruit`, `next_defender` or a
  `Defending` recruit;
- a `campaign.next_defender` of `{"Recruit": id}` that is not on the roster;
- a run record with no `worker_id` and a `recruit_id` (a main-run takeover)
  whose mode is not `Npc`.

While a recruit holds the seat, `reserved: "Npc"` no longer covers Mara's
loadout; only the recruit's items in `reserved_items` are reserved.

An expedition `summary` may carry an `outcome` or `acknowledged: true` only
once its battle has settled, and lists at most 32 `fronts`.

### Loot tracking and auto-sell

Every key below is optional and absent at its default, so a save that never
uses them is unchanged. Loading rejects any of these as invalid:

- `campaign.auto_sell` (the rule; off by default) with `sell_below: "Poor"`,
  or any non-default rule in an expedition game;
- `attempt.drop_ids` that are not strictly ascending, hold more ids than the
  attempt's `loot` or than 4096, or name id 0 or an id at or above
  `next_item_id`, or that are present on an attempt without `drop_tier` or
  on an expedition's attempt (only a root attempt with a drop tier records
  drops); an `attempt.drop_tier` outside 1 to the highest droppable item tier
  (absent means an attempt deployed before v7);
- `last_result.drop_ids` with a repeated id or the bounds above (any order);
  `last_result.loot_by_quality` summing above `loot`;
- a `last_result.auto_sold` receipt whose `count` plus `kept_on_overflow`
  exceeds `loot`, with more than `min(count, 12)` lines, a `by_quality` sum
  other than `count`, line coins above `coins`, coins without a sale, or a
  line definition that does not resolve;
- a run record whose `auto_sold_count` exceeds its `loot`, or that has
  `auto_sold_coins` without a count;
- a `battles[*].pending_auto_sell` on a settled battle, or one naming id 0,
  an id at or above the parent's `next_item_id`, an id pending for another
  expedition, a `reserved_items` id or a root `attempt.drop_ids` id.

### Progression ledgers

`campaign.player.level_grant_paid` is the highest level whose level-up coin
grant has been paid (absent means 0; 0 and 1 both mean none).
`campaign.milestones_claimed` is the set of ranks whose milestone reward has
been paid (absent means empty). Both are copied into expedition games and on
the operation-chain sync. Loading rejects, in any game, a `level_grant_paid`
above the player's `level`, more than 64 claims, or a claim above the
campaign's progression rank.

The receipts of what settlement paid are optional and absent at their
default. `last_result.progression_rewards` (root or expedition report) holds
`level_from`, `level_to`, `level_coins` and up to four `milestones`, each
`{rank, coins, item_id?, item_definition?}` (the item keys are absent when no
item was stored). A run record may carry `level_grant_coins` and
`milestone_coins`, with no bound of their own. Loading rejects a receipt with
more than four milestones, milestone ranks that are not strictly increasing,
a milestone with only one of `item_id` and `item_definition`, an
`item_definition` that does not resolve, or `level_from` above `level_to`
or `level_to` above the player level cap. A milestone item that could not be
stored is reported to the frontend once (`MilestoneItemLost`) and is never
saved.

### Skill modifier perks

The frozen `attempt.profile.modifiers` (root and expedition attempts) may
carry three optional keys, each absent at its neutral value:
`class_modifiers` (a map from weapon class to `{damage_multiplier,
reload_multiplier, cadence_multiplier, weak_bonus_add}`, every inner key
written), `armor_penetration` (absent at 0) and `hazard_damage_multiplier`
(absent at exactly 1.0). Loading rejects a class multiplier outside 0.01–100,
a `weak_bonus_add` outside 0–1, non-finite values, an `armor_penetration`
outside 0–1000 or a `hazard_damage_multiplier` outside 0.01–1. v6 attempts
load with neutral values and need no migration step.

### Combat telemetry

`telemetry` (on the attempt and the report, root or expedition) may carry
seven optional keys, each absent when empty or zero: `spawned_by_archetype`
and `kills_by_archetype` (maps from archetype id to a u32 count),
`damage_taken_by_archetype` (archetype id to u64 damage), `elites_spawned`,
`elites_killed`, `hazard_damage_taken` and `boss_phase_ticks` (a list of
ticks). Bosses and unresolved legacy enemies are never counted in the maps.
Loading rejects more than 256 keys in a map, a key that is not a catalogue
id (1-64 bytes of `[a-z0-9_.-]`), archetype kills summing above the owner's
`kills`, an archetype with more kills than spawns, or more than two boss
phase ticks. Nothing writes these keys yet; v6 telemetry loads with them
empty. Because an attempt resumed from an older save has living enemies whose
spawns were never counted, the writer counts a kill in `kills_by_archetype`
only while that archetype's kills are below its `spawned_by_archetype` count.
Otherwise the save could not load again.

### Enemy schema

Every key below is optional and absent at its neutral value; v6 saves load
with the neutral values and need no migration step. Nothing writes a
non-neutral value yet (enemy-balance, wave 2).

- A spawn snapshot `attempt.enemies[*].resolved` may carry `reach` (yards,
  0–12, absent at 0), `ranged_armor` (0–1000), `npc_ranged_armor` (0–100),
  `stagger_immune` (absent when false), `group_size` (1–6, absent at 1),
  `death_effect` (`{"Burst": {radius, damage, structure_damage, npc_dice,
  npc_add, fuse}}` with radius in (0, 3], damage 1–10000, structure damage
  0–10000, dice 1–20 and add −30–100, or `{"Split": {archetype_id, count}}`
  with count 1–4), `tags` (at most 8 unique catalogue ids) and `elite`
  (`{affix_id, name}`, a non-blank name of at most 60 bytes). An unknown key
  is still refused. A split target or affix id need not exist in the current
  catalogue, but must be a catalogue id.
- An enemy may carry `split_child` (absent when false) and `boss_state`
  (`{profile: {name, speed_mult, armor_mult, call_interval_mult,
  siege_recovery_mult, hold_ticks}, transitions, entered, hold_until}`).
  Loading rejects a split child that is a boss, an elite or a splitter, a
  `boss_state` on a regular enemy, more than two transitions, `entered`
  above `transitions`, a profile outside its bounds (name 1–40 bytes,
  speed 1–2, armor 0.25–1, call interval and siege recovery 0.3–1, hold at
  most 180 ticks), a non-zero `hold_until` before any transition was
  entered, or a `hold_until` more than `hold_ticks` past the attempt tick.
- `attempt.progression` may carry `elite_rate_bp` (0–10000, and 0 below
  rank 3), `front_ramp` (1–2, absent at exactly 1.0), `boss_rank` (`Sector`, `Regional` or
  `Capstone`; absent at `Sector`), `boss_phases` (0–2),
  `boss_armor_multiplier` (1–4), `npc_boss_armor_add` (0–20),
  `boss_health_multiplier` and `boss_reward_multiplier` (1–2) and
  `boss_damage_multiplier` (1–4); the multipliers are absent at exactly 1.0.
  `wave_tier` may be at most 15 (`MAX_WAVE_TIER`) and `enemy_cap` at most
  224 (`GAMEPLAY_ENEMY_CAP`, one boss may come on top).
- A run record may carry `elite_kills` (at most `kills`), `boss_ttk_ticks`
  (at most `duration_ticks`) and `boss_rank`; a report may carry
  `boss_rank`.
- The saved presentation events (`attempt.events[*].kind`) gain `Spit`,
  `Detonation`, `Split` and `BossPhase`.

### v6 to v7

The step runs its sub-steps in a fixed order. Later production-pass commits add
theirs at the marked slots.

1. **(a) Defender seats.** Every `battles[*]` record must have a u64
   `recruit_id`, or the load fails with a migration error. The expedition
   game's `attempt` and `last_result`, when present, gain
   `"defender": {"Recruit": recruit_id}` unless they already have one. The
   root attempt and root report are never touched. A v6 root therefore reads
   as the mode owner, which is what it meant in v6. A missing `battles` array
   means there is nothing to do.
2. **(e) Npc appearance.** The root attempt and every expedition attempt in
   `Npc` mode whose profile has no `appearance` gain `"Female"`. This is what
   every v6 Npc deploy stamped. The serde default is `Male`. A saved
   appearance is kept.
3. **(b) Report drop tracking.** The root report and every expedition report
   with loot receipts gain `drop_ids`, the sorted, de-duplicated receipt item
   ids, and `loot_by_quality`. The root report takes the counts of its own
   run record (same `attempt_id`, `worker_id` null), which saw every drop;
   otherwise the receipts are counted. A count of all zeros adds no key, and
   saved keys are kept. A receipt without a u64 `item_id` or a known quality
   is a migration error. The auto-sell rule, attempt drop tracking, receipts
   and pending sales keep their defaults: off and empty.
4. **(c) Level-grant ledger.** The root player and every expedition game's
   player at level 2 or above gain `level_grant_paid` equal to their
   `level`, unless they already have one, so a v6 save is owed no
   retroactive level-up grant. Level 1 writes nothing.
5. **(d) Rank milestones.** The root campaign gains `milestones_claimed`,
   every rank of the frozen list 5, 10, 15, 20 at or below its v6 rank,
   unless it already has one. The v6 rank is a frozen copy of
   `progression_rank`: the victories, or for a pre-threat campaign
   (`threat_revision` 0 or absent) the larger of the victories and the legacy
   clear count. Every expedition campaign gains the parent's claims, limited
   to those its own rank reached. Below rank 5 nothing is written.

The step adds no `acknowledged` flag, so v6 expeditions that had already
settled never raise a notice.

**v6 expeditions keep Female until their next re-deploy.** A v6 expedition
was deployed with Mara's Female appearance, not its recruit's identity. The
migration does not re-stamp it. The expedition keeps that appearance until the
next stage of its operation chain deploys it again.
