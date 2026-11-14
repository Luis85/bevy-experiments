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

**Relative quality (decision 7, W2-07).** An attempt deployed from wave 2 on (`drop_tier` Some) rolls with `loot::drop_quality`, which calls `QualityCatalog::roll_relative` on the dropped definition's `base_quality`. It uses the same key, tag and rank weights as the absolute roll, but reads each tier as a step from Common (Poor −1, Common 0, Uncommon +1, Rare +2, Epic +3, Legendary +4) and returns `quality::shift(base, step)`, clamped to Poor..Legendary. A higher-tier drop therefore no longer mostly downgrades: a tier-4 (Legendary base) drop is at worst Epic, and a Common-base drop is exactly the absolute roll. With a floor (`ELITE_QUALITY_FLOOR`, Uncommon, which elite drops have used since W2-35; a boss's drop has none) every tier whose outcome lies below the floor weighs 0 and the roll uses the renormalised total; if nothing is left the result is `max(base, floor)`. At rank 3 an elite drop with that floor gives a Common base Uncommon 80.1% / Rare 17.0% / Epic 2.8%, and an Uncommon base Uncommon 69.7% / Rare 24.2% / Epic 5.2% / Legendary 0.9%; an ordinary Common-base drop at rank 3 is the absolute roll, Poor 7.9% / Common 64.2% / Uncommon 22.3% / Rare 4.7% / Epic 0.8%. `Item.quality` and `LootEvent.quality` stay absolute, so owned items keep their stats and value and `quality.json` is unchanged. A legacy attempt (`drop_tier` None) keeps the absolute roll.

**Loot-only definitions (W2-08).** The catalogue holds 68 definitions. The 39 appended at indices 29-67 are `loot_only`: drop tiers 1-4 each hold every weapon class and at least three items of every other slot. The tier-2 to tier-4 Ranged items are `quality::apply` of a tier-1 exemplar at Rare, Epic or Legendary, and the Melee, Outfit and Accessory items are sidegrades of their slot anchor. `ItemDefinition::purchasable()` (a non-starter tier that is not loot-only) gates the shop rows, `buy_item` (and so `Command::BuyItem`), the recommender and the smoke and fixture buyers. `Campaign::buy_price` (and so `shop_offer`) checks only `loot_only`: it refuses a loot-only definition but still prices a tier-0 starter, which `buy_item` refuses first. A loot-only definition is refused with `content::LOOT_ONLY_REASON`, "This equipment is only recovered as loot", and the shop keeps its 24 rows. Content validation requires a purchasable tier-1 item for every slot, so a loot-only item never stands in for the opening shop. A `drop_tier` Some attempt draws from `Content::drop_pool` at its frozen drop tier, every definition from index 4 on with a tier from 1 to the drop tier that is not NPC issue, loot-only items included. A legacy attempt (`drop_tier` None, ruling E11) draws only from the 29 v1 definitions with the uniform pick, so its picks stay byte-identical.

**Weighted drops (W2-09, plan LE-5).** A `drop_tier` Some pick is weighted by `loot::DropTable`: each pool entry weighs `DROP_TIER_WEIGHTS[drop tier - item tier]` (6 / 3 / 1 / 1, so the attempt's own tier weighs most) times `SLOT_WEIGHTS[slot]` (neutral, a wave-4 lever) times `TRAINED_CLASS_WEIGHT` (3) for a Ranged item whose weapon class the defender is trained in: the Personal skill classes, or an NPC's own weapon class. The weights are walked cumulatively in catalogue index order. A normal drop still takes exactly one combat-RNG draw, `below(total)` in place of `below(len)`; an elite-only drop picks with the keyed ELITE_PICK hash modulo the total and draws nothing. On this path an elite kill always drops exactly one item (ruling E8, W2-35), whatever the 3% roll and the every-20th-kill rule give; an elite that also meets the normal rule still drops one, picked by the combat-RNG draw. Bosses and split children are never elites. The drop is lost only when the pool is empty or the armory refuses the item. A legacy attempt (`drop_tier` None) has no elite term. At drop tier 4 with no trained class the current tier takes 90 of 168 (53.6%) where the uniform pick gave it 15 of 63.

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

## Auto-sell threshold

The auto-sell rule ([PROGRESSION.md](PROGRESSION.md), "Auto-sell") sells a drop when its quality is strictly below `AutoSellRule.sell_below`. It compares the item's saved absolute quality (`Item.quality`, the tier its colour shows), never the step the relative roll took, so the threshold means the same for every drop tier. An item without a rolled quality (shop stock and starters) is never a candidate. `sell_below` accepts Common to Legendary; Poor would sell nothing and is refused with `CommandError::AutoSellRuleInvalid`. No threshold sells a Legendary item.

The Equipment Auto-sell panel offers the five accepted values as chips, cheapest first. Each chip paints the shape of the best quality it sells in that quality's palette colour, whatever the marker setting:

| Chip | `sell_below` | Sells |
| --- | --- | --- |
| Poor only | Common | Poor |
| Common and below (default) | Uncommon | Poor, Common |
| Uncommon and below | Rare | Poor to Uncommon |
| Rare and below | Epic | Poor to Rare, with the amber warning "Also sells Rare finds." |
| Epic and below | Legendary | Poor to Epic, with the amber warning "Also sells Rare and Epic finds." |

Relative quality decides how much the default sells. The loot-only tier-2 to tier-4 definitions have a Rare, Epic or Legendary base, and the relative roll moves a drop at most one tier below its base (its only downward step is Poor's −1), so past the first drop tiers few drops are Poor or Common. In the wave-4 measurement (plan §9.7, W4-l) the default rule had sold 2.1% of the items after 60 runs and 1.5% after 150. When the last run's drops still in the stash all sit at or above the threshold and the next threshold would sell something from the stash, the panel says so: "Your recent drops are <Quality> or better; raise the threshold to keep selling". The rule never raises the threshold itself (user decision R58 keeps the default).

## Quality markers

Colour is never the only sign of quality. Settings › Readability & effects has two independent settings, both presentation only:

- **Quality markers**: Colour only (default), Text, Shape, or Text and shape. With Text an item label that showed quality only by colour also names it; with Shape the quality's shape is drawn beside it. The Equipment and History item lists and the Results loot block follow this one setting.
- **Colour-blind quality palette**: replaces the catalogue colours with Okabe-Ito hues that stay apart under the common colour-vision deficiencies (Poor gray, Common the text colour, Uncommon bluish green, Rare sky blue, Epic reddish purple, Legendary orange).

Each quality has one painted shape, never a font glyph: Poor a hollow downward triangle (the only outline), Common a disc, Uncommon a diamond, Rare a square, Epic an upward triangle and Legendary a star. The battlefield loot fly-up always paints its tier's shape as a corner marker, whatever the setting, and only Rare and better drops glint; with reduced effects the fly-up holds still and only fades, and never glints. The shapes are static, so reduced effects needs no second form.
