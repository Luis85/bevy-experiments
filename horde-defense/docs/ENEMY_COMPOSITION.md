# Enemy composition

Ordinary enemies are authored in `crates/horde_core/enemy_catalog.json`. The
five `EnemyKind` values remain legacy categories and visual fallbacks; they do
not limit how many archetypes the director can select. The shipped catalogue
contains the five original profiles, three component mixes, and three ordinary
boss-derived variants. Actual timed boss encounters retain their separate level
definitions and mechanics.

Set `LASTLIGHT_ENEMIES` to a JSON file before starting the application to replace
the entire catalogue. `enemy_catalog()` loads and validates it once; malformed
overrides fail with their source and error. Without an override, the compiled-in
catalogue is used. There is no live file reload. `LASTLIGHT_CONTENT` continues to
configure the other encounter/equipment definitions independently.

## Authoring a type

Every archetype and component record needs a stable `id`; saves refer to them
by id, so keep ids unchanged once players have saves and add ids to any
override written before they were required. `target_urgency` is optional.

An archetype names one reusable record from each component table:

| Table | Authored values |
| --- | --- |
| `bodies` | Personal `health`, NPC `npc_hp`, `npc_ht` |
| `movements` | World-space `speed` |
| `attacks` | Personal `damage`, `windup`, NPC skill/damage dice/add, `structure_damage_multiplier` |
| `defenses` | Personal armor, NPC armor/dodge, `contact_armor_fraction` |
| `rewards` | Coins and personal XP |
| `visuals` | Sprite, RGBA tint and relative scale |

For example, this archetype uses the original shambler body, runner movement
and attack, and armored defense. Adding this record and a positive wave weight
requires no Rust edit:

```json
{
  "id": "swift-plated-scavenger",
  "name": "Swift plated scavenger",
  "base_enemy_kind": "Shambler",
  "body": "shambler",
  "movement": "runner",
  "attack": "runner",
  "defense": "armored",
  "reward": "armored",
  "visual": "armored",
  "unlocked_by": null
}
```

For component-authored special behavior, an attack record may include
`"structure_damage_multiplier": 2` and a defense record may include
`"contact_armor_fraction": 0.5`. The multiplier is an integer in 1–8 (default 1);
the fraction is finite and in 0–1 (default 1). Shipped `demolisher` and
`siege-pack` attacks use multiplier 2; the `armored` defense uses fraction 0.5.
Thus a shambler-tagged archetype can gain demolition or reduced contact armor
from its components, while a Demolisher-tagged archetype does not gain those
traits merely from its legacy kind. Timed boss-specific mechanics remain
separate from these ordinary profiles.

An archetype may also author `"target_urgency"`: yards added to an NPC
defender's target score, so negative values are engaged sooner (finite,
−100–100). When omitted it defaults from `base_enemy_kind` (Runner −3,
otherwise 0), so older catalogues keep their targeting; the shipped Runner-tagged
archetypes author −3 explicitly. It is a resolved, saved snapshot field like the
others.

Component IDs are local to their table. Archetype IDs are persistent identities;
keep them stable when rebalancing. Components contain values, not references to
other components: inheritance and reference cycles are unsupported. Every record
rejects unknown fields, including attempted inheritance fields.

`VisualSprite` accepts `Shambler`, `Runner`, `Brute`, `Armored`, `Demolisher`,
`Breacher`, `Caller`, and `SiegeBrute`, reusing the existing sprite assets. Selecting
a boss sprite does not grant boss behavior. Boss-derived packlings, prowlers and
husks have their own lower health, rewards, combat profiles and presentation size.

## Adding a new enemy

**Data-only variant** (new numbers, mix of existing behaviours) — no Rust edit:

1. Reuse or add records in `bodies`, `movements`, `attacks`, `defenses`,
   `rewards` and `visuals` (each validates on its own).
2. Add an `archetypes` record: a new stable `id`, `name`, the nearest legacy
   `base_enemy_kind` (category, visual fallback and kill-tally key only), the six
   component IDs, optional `target_urgency`, and `unlocked_by` (`null` or a boss
   kind for boss-derived ordinaries).
3. Give it a positive `weight` in each `waves` template it should appear in.
4. Pick a `visual.sprite` from the existing `VisualSprite` set; until per-archetype
   art is data-driven, the frontend draws the sprite with the authored tint/scale.
5. Run `cargo test --locked -p horde_core` — catalogue validation, snapshot
   validation and the save round-trip tests cover the new record.

Every per-kind number the simulation uses (speed, damage, windup, armor, contact
armor fraction, structure damage multiplier, NPC stats, rewards, targeting
priority) comes from the resolved profile; `sim.rs` has no per-`EnemyKind`
numeric branches.

**New behaviour** (something the profile cannot express, e.g. a ranged attack or
an aura):

1. Add the parameter to the relevant component (or the archetype) with a serde
   default that reproduces current behaviour, copy it into `ResolvedEnemy`, and
   bound it in both `AuthoredCatalog::resolve` and `ResolvedEnemy::validate`.
2. Read it through `enemy_profile(e)` in the simulation; branch on the value,
   not on `EnemyKind`.
3. If old saves' snapshots need a value other than the serde default, bump
   `SAVE_VERSION` with a step in `sim/migrations.rs` and a fixture test.
4. Only add an `EnemyKind` variant for a new legacy category with its own
   fallback art; the compiler then lists every frontend `match` to update
   (`KindMap` keys, kill tallies, sprite choice). See ARCHITECTURE.md on why these
   enums stay exhaustive.

`EnemyKind` still selects behaviour in two places on purpose: the timed boss
spawns through the Brute default profile, and the Caller boss summons the Runner
default. Both resolve their numbers from the catalogue's `defaults`.

## Waves and boss unlocks

Each `waves` record has a contiguous zero-based `level` and positive-weight
`entries` containing `archetype_id` and `weight`. Shipped templates correspond to
the five encounter templates; larger level indexes cycle through the authored
templates. Entries are normalized to stable archetype-ID order, so moving JSON
records does not change weighted selection ordering.

`wave_pool(level, unlocked_bosses)` excludes an archetype with `unlocked_by` until
that exact boss kind is in the campaign’s unlocked-boss history. Successful boss
completion records the kind through `Campaign::first_clear`; historical
`first_clears` also supply the compatibility unlock set. First kills
unlock `breacher-pack`, `caller-pack`, and `siege-pack` for subsequent ordinary
spawns, including later attempts in earlier encounters and worker expeditions.
Child battle completion merges first-clear history back into the shared campaign
before later pools are built. All shipped templates
contain these entries, with weights increasing later. These are ordinary kills
and must not award boss completion rewards or trigger victory. Every template
must contain an unrestricted entry, preventing an empty opening pool.

Pool construction consumes no RNG. The simulation sums the returned weights,
draws once from its existing serialized RNG, and resolves the chosen ID. Spawn
rates, live enemy limits and timed bosses remain encounter/director settings.

## Validation limits

JSON input is capped at 4 MiB. Each component/archetype table and the template
count allow 1–4096 records; each template allows 1–4096 distinct entries. IDs use
1–64 lowercase ASCII letters, digits, `_`, `-`, or `.`. Names are nonempty and at
most 120 bytes. Duplicate IDs, duplicate default kinds, duplicate entries,
unknown references, gaps/duplicates in wave levels, and unsupported fields fail
before play. Exactly one unrestricted default must match each legacy kind.

| Values | Accepted bounds |
| --- | --- |
| Personal HP / damage / armor | 1–100000 / 1–10000 / 0–1000 |
| NPC HP / HT / skill / dodge / armor | 1–1000 / 3–30 / 3–30 / 0–20 / 0–100 |
| NPC damage dice / add | 1–20 / −30–100 |
| Structure damage multiplier / contact armor fraction | integer 1–8 / finite 0–1 |
| Target urgency | finite −100–100 |
| Speed / windup | 0.01–10 / 0.5–60 |
| Coins / XP | 1–1000000 / 0–1000000 |
| Tint channels / visual scale | 0–1 / 0.25–3 |
| Weight / total template weight | 1–1000000 / at most `u32::MAX` |

Floating-point values must be finite. Even unused components validate, so an
invalid spare part cannot become a delayed runtime failure.

## Runtime and persistence integration

`EnemyCatalog::resolve(id)` returns a complete `ResolvedEnemy`, including all
personal/NPC stats and its visual profile. Store that snapshot on the spawned
enemy and serialize it alongside current HP, position, timers and identity.
Every authoritative enemy stat read must use that snapshot, including rewards,
movement, structure-damage multiplier, contact armor fraction, contact attacks and NPC rules resolution. The visual profile is also
part of the snapshot for coherent resumption.

Call `ResolvedEnemy::validate()` when validating a save. It checks numeric and
identity bounds without requiring the archetype to exist in today's catalogue.
A live or saved enemy therefore keeps its original stats when the external
catalogue is changed or its archetype is removed. Only future spawns use the new
catalogue. `resolve_kind(kind)` resolves the configured unrestricted default for
explicit legacy spawns or migration of old saves that have no snapshot.

The API is engine-free and does not perform spawning, rendering, kill accounting,
unlock persistence, or random selection. Those remain simulation/presentation
responsibilities; a catalogue unlock marker alone cannot record a boss kill.


`ResolvedEnemy` includes `structure_damage_multiplier`,
`contact_armor_fraction` and `target_urgency`; save validation checks their
bounds along with the other snapshot fields. A changed component affects future
spawns, not an actor already saved with a resolved profile. Legacy records
without the first two use their documented neutral defaults during
deserialization; save format v3 migration adds `target_urgency` to older
snapshots from their saved `kind`.

Fallen actors move into persistent per-attempt corpse records with their
resolved appearance and stable identity. They remain through save/resume and
render in batches after settling; they are no longer active wave participants,
contact blockers or reward sources. This presentation lifetime does not raise
the live-enemy limit.

See [SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md) for the complete startup
override/package paths and [VALIDATION.md](VALIDATION.md) for actual regression
and gameplay evidence. No country-specific balance proof is inferred here.

Latest movement and injury semantics: faster approaches can overtake moving peers through saved within-lane displacement; intact walls/contact still queue. Conscious zero/negative-HP NPC actors remain Critical under the user-retained GURPS rules, while actual terminals stop/reap/pay in the resolving fixed tick. See [OVERTAKING_AND_CRITICAL.md](OVERTAKING_AND_CRITICAL.md). Corpse/loot locations use actual Enemy.y(); [ITEM_QUALITY.md](ITEM_QUALITY.md) defines independent quality rolls and already-banked receipts, and [PROGRESSION_SCALING.md](PROGRESSION_SCALING.md) defines frozen lane counts/population and every-victory threat.
