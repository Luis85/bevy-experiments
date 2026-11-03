# Item quality authoring and save contract

The six quality tiers are **Poor, Common, Uncommon, Rare, Epic and Legendary**. They are independent of the item's instance level and its definition's campaign unlock tier. Names, RGBA palette, drop weights and stat/value multipliers come from `crates/horde_core/quality.json`; `LASTLIGHT_QUALITY` selects another startup catalogue. No Bevy or presentation types enter the core.

| Quality | Palette | Base weight | Added weight per rank | Minimum drop rank | Stat multiplier | Value multiplier |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Poor | Gray | 80 | 0 | 0 | .80 | .50 |
| Common | White | 650 | 0 | 0 | 1.00 | 1.00 |
| Uncommon | Green | 220 | 2 | 0 | 1.12 | 1.70 |
| Rare | Blue | 45 | 1 | 0 | 1.30 | 2.80 |
| Epic | Purple | 5 | 1 | 2 | 1.55 | 5.00 |
| Legendary | Orange | 1 | 1 | 5 | 1.90 | 10.00 |

Weights are relative among eligible tiers, not percentages. Rank comes from the frozen victory-threat progression snapshot. The minimum rank limits random drops only; shops continue selling the authored base quality of each unlocked definition. A guaranteed opening purchase never requires rolling Rare equipment.

## Compatibility and real effects

Definitions store `base_quality`. Instances store optional `quality`; missing or `None` means the authored base quality. Existing Common/Uncommon/Rare definitions retain those bases; historical Exceptional and Masterwork labels map to Epic and Legendary. Other starter/field definitions use Common. Existing definition unlock tiers, prices and numerical profiles remain unchanged.

Effective quality uses `rolled_multiplier / base_quality_multiplier`, after item-level scaling. Consequently both `None` and an explicit roll equal to the base quality preserve **every old levelled statistic and resale value**. Old saves gain no free numerical upgrade from a new color label.

The stat ratio affects Personal ranged/melee basic damage, armor, item health and stamina bonuses, and NPC armor/basic damage. NPC dice and attack cadence stay fixed; damage adds approximate the ratio of expected basic damage before protection, respecting minimum damage and supported `−30..100` adds. Personal skills never transfer into NPC attack statistics. Integer rounding means some small bonuses share a breakpoint.

`Campaign.effective_item(id)` and complete loadout comparisons expose actual quality-and-level statistics. `item_quality(id)` resolves the effective tier. `item_sale_value(id)` includes the value ratio. `buy_price(definition)` remains the current-level price of the fixed base-quality shop offer. There is no quality reroll, free weapon grant, or purchase-time quality lottery.

Protected starter sets remain level one with no quality override. Weapon proficiency, ownership, unique equipment reservations and `npc_only` restrictions still apply. The pistol cap remains a per-trigger cap after all effective quality/level damage, pellets and weak-point bonuses.

## Deterministic drops and receipts

Drop eligibility and definition selection retain their existing combat-RNG draws. The additional quality roll uses a separate SplitMix counter key composed from the game seed, attempt ID, defeated actor ID and a quality-purpose tag. It consumes **no additional combat RNG state**. Checkpoints preserve rolled quality; parallel settlement copies it together with level and definition exactly once.

After successful authoritative insertion, the attempt emits an informational `quality::LootEvent`:

```text
item_id: unique instance ID
definition: catalogue index
level: frozen encounter item level
quality: resolved six-tier quality
name: captured item name
tick: fixed simulation tick
at: actual defeated actor [x, y()], including passing displacement
```

`Attempt.loot_events` and `Report.loot_events` retain the most recent 128 receipts. This bounds feedback history, not inventory. Each receipt represents an item already banked in the armory. Presentation may draw its colored outline at the death position and animate it upward, but must never award the item or coins again. A real-time cache keyed by source attempt and instance ID can make feedback readable at accelerated observation speeds.

Save validation bounds receipt count, IDs, definitions, levels, names, ticks and finite battlefield positions. The catalogue requires all six canonical tiers, finite visible RGBA colors, bounded nonnegative weights, an eligible rank-zero distribution, and strictly increasing positive stat/value multipliers. Unknown tiers or malformed metadata reject the generation.

Regression coverage includes all old definitions at levels one and seven, explicit base-quality identity, numerical upgrades and resale value, legendary pistol caps, NPC-issued restrictions, deterministic roll/checkpoint behavior, unchanged combat RNG draws, and exactly-once parallel transfer. The opening 15 country/seed purchase routes, five-template campaign proofs and eight-stage earned routes remain independent of rare-drop equipment.
