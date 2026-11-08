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

Weights are relative among eligible tiers, not percentages. Rank comes from the frozen victory-threat progression snapshot. The minimum rank limits random drops only; shops continue selling the authored base quality of each unlocked purchasable definition (loot-only definitions are never sold; see "Loot-only definitions" below). A guaranteed opening purchase never requires rolling Rare equipment.

## Compatibility and real effects

Definitions store `base_quality`. Instances store optional `quality`; missing or `None` means the authored base quality. Existing Common/Uncommon/Rare definitions retain those bases; historical Exceptional and Masterwork labels map to Epic and Legendary. Other starter/field definitions use Common. Existing definition unlock tiers, prices and numerical profiles remain unchanged.

Effective quality uses `rolled_multiplier / base_quality_multiplier`, after item-level scaling. Consequently both `None` and an explicit roll equal to the base quality preserve **every old levelled statistic and resale value**. Old saves gain no free numerical upgrade from a new color label.

The stat ratio affects Personal ranged/melee basic damage, armor, item health and stamina bonuses, and NPC armor/basic damage. NPC dice and attack cadence stay fixed; damage adds approximate the ratio of expected basic damage before protection, respecting minimum damage and supported `−30..100` adds. Personal skills never transfer into NPC attack statistics. Integer rounding means some small bonuses share a breakpoint.

`Campaign.effective_item(id)` and complete loadout comparisons expose actual quality-and-level statistics. `item_quality(id)` resolves the effective tier. `item_sale_value(id)` includes the value ratio. `buy_price(definition)` remains the current-level price of the fixed base-quality shop offer. There is no quality reroll, free weapon grant, or purchase-time quality lottery.

Protected starter sets remain level one with no quality override. Weapon proficiency, ownership, unique equipment reservations and `npc_only` restrictions still apply. The pistol cap remains a per-trigger cap after all effective quality/level damage, pellets and weak-point bonuses.

## Deterministic drops and receipts

Drop eligibility and definition selection retain their existing combat-RNG draws. The additional quality roll uses a separate SplitMix counter key composed from the game seed, attempt ID, defeated actor ID and a quality-purpose tag. It consumes **no additional combat RNG state**. Checkpoints preserve rolled quality; parallel settlement copies it together with level and definition exactly once.

**Relative quality (decision 7, W2-07).** An attempt deployed from wave 2 on (`drop_tier` Some) rolls with `loot::drop_quality`, which calls `QualityCatalog::roll_relative` on the dropped definition's `base_quality`. It uses the same key, tag and rank weights as the absolute roll, but reads each tier as a step from Common (Poor −1, Common 0, Uncommon +1, Rare +2, Epic +3, Legendary +4) and returns `quality::shift(base, step)`, clamped to Poor..Legendary. A higher-tier drop therefore no longer mostly downgrades: a tier-4 (Legendary base) drop is at worst Epic, and a Common-base drop is exactly the absolute roll. With a floor (`ELITE_QUALITY_FLOOR`, Uncommon, for elite drops once they land in W2-35) every tier whose outcome lies below the floor weighs 0 and the roll uses the renormalised total; if nothing is left the result is `max(base, floor)`. At rank 3 this gives a Common base Uncommon 80.1% / Rare 17.0% / Epic 2.8%, and an Uncommon base Uncommon 69.7% / Rare 24.2% / Epic 5.2% / Legendary 0.9%. `Item.quality` and `LootEvent.quality` stay absolute, so owned items keep their stats and value and `quality.json` is unchanged. A legacy attempt (`drop_tier` None) keeps the absolute roll.

**Loot-only definitions (W2-08).** The catalogue holds 68 definitions. The 39 appended at indices 29-67 are `loot_only`: drop tiers 1-4 each hold every weapon class and at least three items of every other slot. The tier-2 to tier-4 Ranged items are `quality::apply` of a tier-1 exemplar at Rare, Epic or Legendary, and the Melee, Outfit and Accessory items are sidegrades of their slot anchor. `ItemDefinition::purchasable()` (a non-starter tier that is not loot-only) gates the shop rows, `buy_price`, `buy_item` (and so `Command::BuyItem`), the recommender and the smoke and fixture buyers. A loot-only definition is refused with `content::LOOT_ONLY_REASON`, "This equipment is only recovered as loot", and the shop keeps its 24 rows. Content validation requires a purchasable tier-1 item for every slot, so a loot-only item never stands in for the opening shop. A `drop_tier` Some attempt draws from `Content::drop_pool`, every definition from index 4 on with a tier from 1 to the encounter's level + 1 that is not NPC issue, loot-only items included (the uniform pick until the weighted DropTable, W2-09). A legacy attempt (`drop_tier` None, ruling E11) draws only from the 29 v1 definitions, so its picks stay byte-identical.

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
