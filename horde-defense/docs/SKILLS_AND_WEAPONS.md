# Skills, weapons and ordnance

This is the authoring contract for the current personal-combat progression. The
shipped skill catalog contains **177 nodes in one connected DAG**. Seven labels—Foundations, Body development, Firearms, Survival, Ordnance, Fortification, Companions and world—focus the camera within that whole tree; they do not filter it into separate purchase screens. Mara and recruits retain their independent NPC training
and combat profiles.

## Content files and startup overrides

| Purpose | Repository default | Environment variable | Packaged file |
| --- | --- | --- | --- |
| Skill DAG | `crates/horde_core/skills.json` | `LASTLIGHT_SKILLS` | `content/skills.json` |
| Ordinary enemy components/waves | `crates/horde_core/enemy_catalog.json` | `LASTLIGHT_ENEMIES` | `content/enemy_catalog.json` |
| Grenade/Molotov definitions | `crates/horde_core/ordnance.json` | `LASTLIGHT_ORDNANCE` | `content/ordnance.json` |
| Encounters and equipment | `crates/horde_core/content.json` | `LASTLIGHT_CONTENT` | `content/encounters.json` |

Every authored entry needs a stable `id` (items, levels, structures, skill
nodes, enemy archetypes); saves reference them, so never rename or reuse one,
and add ids to any override file written before ids were required. Without an
override the default JSON is compiled into the core. Each catalog
loads and validates once at startup; restart to apply edits. Packaged launchers
set the variables to their adjacent content files. Invalid or unreadable
overrides fail visibly rather than silently substituting defaults. Skill and
enemy input limits are 4 MiB; ordnance input is limited to 1 MiB. Use an isolated
`LASTLIGHT_SAVE_DIR` when changing progression or equipment compatibility.

## Skill nodes

Add a record to the top-level `nodes` array, for example:

```json
{
  "id": "field_conditioning",
  "name": "Field conditioning",
  "description": "Increase personal maximum stamina by 10.",
  "price": 30,
  "prerequisites": ["pistol_training"],
  "branch": "Survival",
  "position": [660, 320],
  "icon": "body",
  "effects": [{"kind": "StaminaAdd", "value": 10}]
}
```

The position is a graph coordinate, not a battlefield coordinate. Use a unique
position across the entire catalogue. Prerequisite edges are real purchase requirements,
not decorative grouping lines. Purchases require all prerequisites and authored victory/liberation milestones, plus the payment returned by the core;
selecting a node or following a recommendation spends nothing.

Node IDs contain 1–64 lowercase ASCII letters, digits or underscores. They are
saved as stable strings, not graph indices: keep IDs stable and retain learned
IDs when updating content. Catalogs permit 1–256 nodes; each node has 1–16 effects
and at most 16 distinct prerequisites. Names/descriptions/branches/icons have
80/1000/48/48-byte limits. Coordinates must be finite and within ±10000. Missing,
duplicate, self-referencing or cyclic prerequisites are rejected. Positions must be globally unique; the only prerequisite-free root is pistol_training, and every node must connect to it. Prices are
1–1000000 except the innate zero-price `pistol_training`.

Effects use tagged JSON. Numerical additions are `DamageAdd`, `HealthAdd`,
`StaminaAdd`, `MeleeAdd` (1–100) and `ArmorAdd` (1–20). Multipliers are
`DamageMultiplier`, `EngineeringMultiplier`, `OrdnanceDamageMultiplier`,
`OrdnanceRadiusMultiplier`, `BurnDurationMultiplier` (1–2), plus
`ReloadMultiplier`, `CadenceMultiplier`, `OrdnanceCooldownMultiplier` (0.5–1).
`GrenadeCharges` and `MolotovCharges` add 1–5. `WeaponTraining` names a class.
Capability unlocks are data-keyed: `{"kind":"Unlock","value":"grenade"}`.
The implemented unlock IDs are `piercing`, `sweep`, `heal`, `brace`, `grenade`
and `molotov` (`skill_catalog::unlocks`); any node may grant any of them, and an
unknown ID is rejected at load. Learned unlocks are saved as the
`SkillModifiers::unlocks` ID set (save v3 migrated the former boolean flags). A
genuinely new capability needs its ID added to `unlocks::ALL` plus the code that
reads `SkillModifiers::unlocked(id)`; it never needs a new `SkillEffect`
variant. The combined multiplier kernel is also bounded
to 0.01–100. Additive/multiplier effects stack in catalog order, so preserve that order when a
numerically identical replay matters.

Three effect kinds carry the per-class perks and counters:

```json
[
  {"kind":"ClassModifier","value":{"class":"Shotgun","stat":"Cadence","value":0.8}},
  {"kind":"ArmorPenetration","value":4},
  {"kind":"HazardDamageMultiplier","value":0.6}
]
```

`ClassModifier` names a weapon class and a `stat`: `Damage` (multiplier 1–2),
`Reload` and `Cadence` (multipliers 0.5–1) or `WeakBonus` (added to the
weak-spot bonus, 0.01–1). Perks fold per class into
`SkillModifiers::class_modifiers`; classes without a perk have no entry.
The Personal profile folds the equipped weapon's class perk after the generic
modifiers: damage × multiplier (rounded, 1–100,000), reload × multiplier (at
least 0.65 s), shot interval × multiplier (at least 0.08 s) and weak-spot bonus
+ addition (at most 1). A perk on another class, or any perk in NPC mode, leaves
the profile untouched, and the recommender counts a perk as acting now only
while the survivor owns a non-NPC gun of its class (Mara's guns do not count).
`ArmorPenetration` (1–20) adds enemy armor ignored by
personal shots: the shot meets `max(0, armor + ranged armor − penetration)`
(a shield-bearer's ranged armor counts only out of contact), while melee,
ordnance and turrets meet full armor. `HazardDamageMultiplier` (0.5–1) scales
hazard damage the survivor takes: `round(raw × multiplier)`, and `raw`
unchanged at 1.0; Mara and recruits in NPC battles take `raw`. An
exploder's fuse and a spitter's spit on the survivor apply it before armor and
Brace, and that damage counts as `hazard_damage_taken`. A node may also
list `counters`, the enemy archetype ids it is authored to answer (at most 8
distinct ids of lowercase letters, digits, `-` or `_`); no rule reads them.

The decision-19 weapon-mastery nodes are appended as catalogue entries 170-177
(the first 169 ids are pinned by `tests/progression_tree.rs`). Each is
milestone gated and every perk acts in Personal play only; prices are
provisional until the wave-4 fit.

| Id | Effect | Prerequisite | Gate | Price | Counters |
| --- | --- | --- | --- | --- | --- |
| `shotgun_pump_drill` | Shotgun cadence ×0.8, reload ×0.8 | `shotgun_training` | 5 victories | 800 | sprinter |
| `marksman_steady_aim` | Marksman damage ×1.15, weak-spot +0.15 | `marksman_training` | 5 victories | 800 | shield-bearer, juggernaut |
| `shield_breaker` | Armor penetration 4 | `stopping_power` | 5 victories | 1,000 | shield-bearer, juggernaut |
| `blast_discipline` | Hazard damage ×0.6 | `hardened` | 5 victories | 1,000 | exploder, spitter |
| `pistol_double_tap` | Pistol cadence ×0.8 | `quick_reload` | 15 victories | 1,200 | – |
| `smg_spray_control` | SMG reload ×0.7 | `smg_training` | 10 victories | 1,200 | splitter |
| `rifle_breath_control` | Rifle damage ×1.15 | `rifle_training` | 20 victories | 1,500 | – |
| `mg_belt_feeder` | Machine gun reload ×0.6 | `machine_gun_training` | 5 victories | 1,500 | sprinter, splitter |

A node with counters opens no later than the first threat rank at which any
archetype it counters has spawn weight (sprinter and spitter from rank 6,
exploder and shield-bearer from rank 8, splitter and juggernaut from rank 10):
its gate is the largest of 5/10/15/20 at or below that rank.

Additional tagged effects encode target ownership/rank, not repeated additive rank bonuses:

```json
[
  {"kind":"BodyDevelopment","value":{"attribute":"Vitality","rank":1}},
  {"kind":"Blueprint","value":"Trap"},
  {"kind":"StructureRank","value":{"kind":"Trap","rank":1}},
  {"kind":"FeatureUnlock","value":"Construction"}
]
```

BodyDevelopment supports ranks 1–1000 in the schema; the shipped five attribute paths have twenty ranks each. StructureRank supports 1–10; each of the three fortifications has a blueprint and ten rank nodes. minimum_victories defaults to 0 and is bounded at 10000; minimum_liberated defaults to 0 and is bounded at 250. Both gates and every prerequisite are checked by can_purchase_node before purchase.

node_payment returns Coins or eligible LegacyBodyPoints. Fresh profiles do not mint personal CP from XP; they purchase body nodes with retained coins. Legacy allocated ranks count as ownership floors, eligible old points can pay a pure body node within the old cap, and legacy_graph_points records that consumption. Coin-trained nodes and graph-spent CP are not refunded as old body allocations. NPC training keeps its own ledger. Details and safe journey reset/continuation are in [JOURNEYS.md](JOURNEYS.md).

Compatibility catalogs must retain `pistol_training`, `piercing`,
`quick_reload`, `sweep`, `second_wind`, `brace` and `engineering`.
The starter must remain free, prerequisite-free and contain exactly
`{"kind":"WeaponTraining","value":"Pistol"}`. New campaigns receive it
automatically; it cannot be bought repeatedly. The historical six-skill save
adapter still resolves the corresponding stable IDs. Old saves lacking the new
training representation preserve their former firearm eligibility through the
legacy adapter.

## Owned firearms and class training

The six exact JSON class names are `Pistol`, `Smg`, `Shotgun`, `Rifle`,
`Marksman` and `MachineGun`. Personal equipment/deployment requires both an
owned unreserved item and training in its class. Training never creates a gun;
buy or loot one separately, then explicitly equip it in Equipment. NPC firearms
continue to use independent Guns training and pinned NPC attack procedures.

Ranged item definitions in `content.json` may supply this `weapon` object:

```json
{
  "class": "Pistol",
  "pellets": 1,
  "spread_radians": 0.0,
  "max_full_health_fraction_per_hit": 0.8
}
```

Personal damage is per pellet; one trigger pull consumes one ammunition unit.
Pellets permit 1–16, total fan width 0–1.2 radians, and a multi-pellet weapon needs
positive spread. The per-target, per-trigger cap is 0.05–1 of maximum health,
including skill and weak-point bonuses. A pistol requires a cap below 1, so it
cannot kill a full-health target in one trigger pull. Other classes can use 1.
Shipped pistols use 0.8. Missing metadata on historical equipment defaults to a
single-projectile Rifle; no item-name heuristic changes NPC resolution.

The battlefield selector chooses the equipped firearm, Grenade or Molotov.
Changing firearm instances is an Equipment task; the selector does not hot-swap
the inventory.

## Area ordnance

Personal skill modifiers unlock each throwable and issue its finite charges per
deployment. The baseline allowance is two per unlocked kind; charge skills add
to it, capped at 128. NPC deployments receive no personal ordnance. Charges,
cooldowns, projectiles and fire zones belong to the saved attempt; reopening a
view or resuming a save does not replenish them.

Select a throwable in combat, then press Fire once over a ground cell. Input
uses a press edge rather than hold-to-fire or the firing latch. The one-yard target grid uses attempt.width_yards and attempt.lane_count; it uses zero-based `[column, lane]` cells; effects center on
`[column + 0.5, lane + 0.5]`. The UI shows a cell and radius preview, charge count
and cooldown, and renders an arc in flight. Selecting the gun returns to ordinary
aiming. Construction remains a separate five-column left-half grid.

The complete top-level definition has `grenade` and `molotov` objects. A grenade
object can be copied from the shipped values:

```json
{
  "flight_ticks": 36,
  "cooldown_ticks": 90,
  "damage": 105,
  "radius": 2.7,
  "edge_damage_fraction": 0.35,
  "burn_damage": 0,
  "burn_ticks": 0,
  "burn_interval": 30
}
```

Times are fixed 60 Hz simulation ticks; radii are world yards. Flight permits
1–600 ticks, cooldown 1–3600, damage 1–100000, radius 0.1–15, edge fraction 0–1,
burn damage 0–100000, burn duration 0–3600 and burn interval 1–600. Floats must be
finite. Grenades require zero burn; Molotovs require positive burn damage and
duration. The shipped Molotov flies for 42 ticks, deals 18 impact damage in a
2.2-yard radius, and burns for 360 ticks at 12 damage every 30 ticks. At most 32
active projectile/fire-zone records coexist.

The core resolves enemy-only radial damage and burning on fixed ticks, using the
normal terminal/reward accounting. Flight records snapshot their resolved
parameters, so existing projectiles and fire zones retain coherent saved state
when authoring files change. Presentation arcs, flames, sound and reduced-effects
feedback do not resolve a second hit.

## Evidence and related contracts

Source: `skill_catalog.rs`, `weapons.rs`, `ordnance.rs`, `campaign.rs`,
`sim.rs` and `ui/battle/` (`attack.rs`, `field.rs`) in the core/game crates. See
[enemy composition](ENEMY_COMPOSITION.md), [view hierarchy](VIEW_HIERARCHY.md),
[dated user overrides](EXPANSION.md) and [survival audio](AUDIO_SURVIVAL.md).

The first defense waits for "Begin defense", retains rewards after retreat, leads to
skills/training and permits immediate retry. Roughly 3–5 runs per early tier is the
requested pacing target; `cargo run --locked --release -p horde_core --example pacing`
measures it (see [the foundation pass](plans/2026-11-04-foundation-pass.md)). Country-specific balance proof, human playthrough,
native GUI and listening evidence must be reported separately in
[VALIDATION.md](VALIDATION.md); this authoring guide establishes no such result.


Current deployment geometry, completion XP and victory threat are defined in [PROGRESSION_SCALING.md](PROGRESSION_SCALING.md). Quality is independent of instance level and unlock tier; its effective ratios and already-banked receipts are in [ITEM_QUALITY.md](ITEM_QUALITY.md). Enemy positions (enemies overlap since W1-69) and critical NPC state follow [OVERTAKING_AND_CRITICAL.md](OVERTAKING_AND_CRITICAL.md).

ItemDefinition.visual_style and the saved EquipmentVisual snapshot describe actual equipped appearance, separate from stats, class eligibility and ownership. [CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md) defines the part/socket/style authoring contract; previewing the rig never creates or equips an item.
