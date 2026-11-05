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
| v6 to v7 | Defender seats on expedition games and the Npc appearance back-fill (below). |

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
3. (b) Report drop tracking, then (c) the level-grant ledger and (d) rank
   milestones, are added by their owners' commits.

The step adds no `acknowledged` flag, so v6 expeditions that had already
settled never raise a notice.

**v6 expeditions keep Female until their next re-deploy.** A v6 expedition
was deployed with Mara's Female appearance, not its recruit's identity. The
migration does not re-stamp it. The expedition keeps that appearance until the
next stage of its operation chain deploys it again.
