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

`Game::from_payload(value, version)` is the one decode seam. An older payload
is migrated on raw JSON first. Then every game is decoded, reconciled
(catalogue-aware fills) and validated. A save generation at an older version
loads through it. The debug replay tool also uses it for a recording whose
header version is older than this build's, so the recorded initial state
loads the way that build's save would. A current-version recording is decoded
as recorded.

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
| v6 to v7 | Defender seats on expedition games, the Npc appearance back-fill and report drop tracking (below). |

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
4. (c) The level-grant ledger and (d) rank milestones are added by their
   owners' commits.

The step adds no `acknowledged` flag, so v6 expeditions that had already
settled never raise a notice.

**v6 expeditions keep Female until their next re-deploy.** A v6 expedition
was deployed with Mara's Female appearance, not its recruit's identity. The
migration does not re-stamp it. The expedition keeps that appearance until the
next stage of its operation chain deploys it again.
