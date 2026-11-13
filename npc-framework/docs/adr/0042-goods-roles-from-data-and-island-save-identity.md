# 0042: Goods' roles from data, and the island save's identity

Date: 2026-11-13. Status: accepted for the living world (`living_world`).

## Context

The roadmap's phase 2 asks for a catalog defined in data with no hard-coded
good or trade names left in logic that the data could carry. The economy
block (`living_world.economy`: goods, recipes, professions, patches,
buildings, crops, merchant, tuning) is loaded and validated before freeze
and copied into the world's construction data; most logic reads it through
`frontier::economy_spec::registry`. Literal good names remain in about 400
places, most of them in code the legacy village profiles share, where any
change risks the replay corpus.

Separately, every pass changes the living world's content hash, and an older
save then fails a strict snapshot load with a bare hash mismatch.

## Decision

**A good's or trade's role is data.** Logic shared by the island and the
Actor Testbed asks `village::roles::roles(world)` for a role, never a name:
`drink`, `fuel`, `garment`, `timber` and `lumber` (building materials),
`metal` and `ore`, the hunt (`hunt_tool`, `meat` and `hide` with their counts
per hunt), the garment chain (`leather`, `yarn`, `fiber`), the foods
(`baked`, `cooked`, `foraged`), the sown `grain`, and the trades that hunt,
build, sow and make the garment. In the living world they are resolved once,
at load, from the catalog: good tags (`drink`, `fuel`, `clothing`, `timber`,
`lumber`, `metal`, `ore`, `meat`, `hide`, `leather`, `yarn`, `fiber`,
`baked`, `cooked`, `foraged`, `grain`; one good may carry several, as logs
are both `fuel` and `timber`), a profession's `hunt { tool, yields }`,
`builds` and `sows`, and the profession whose outputs include the garment.
A trade may carry a `display` name (the tailor read as "clothier"). A world
without catalog data (the Actor Testbed's scenarios) uses
`roles::TESTBED`, the names those scenarios were built with, so their
replays do not move. A living world whose data lacks a role is refused at
load with a sentence naming it ("economy: no good is tagged `fuel`, so
hearths have nothing to burn"). `catalog::good_with_tag` and
`catalog::has_tag` answer the same from the tags directly.

**A trade is its data key.** `Role` keeps its named variants (the shipped
trades and the Actor Testbed's fixed table) and gains `Role::Trade(key)` for
any trade only a world's catalog defines. In data, saves and the journal
every role is its key string (`"farmer"`, `"water_keeper"`, `"potter"`);
`Role::from_key` and `Role::key` convert, and a role compares, orders and
hashes by its key (a named role hashes its old tag, so nothing outside the
living world moves; a data trade hashes the shared tag 14 and its key). The
catalog's `professions` list is the set of trades: none is required by name
(the roles above say what the logic needs), aptitudes must name defined
trades, and founders must name defined trades with a site in every
settlement. Living-world logic reads behaviour from the profession entry
and never branches on a named trade: the service trade (`service`, the inn),
an open workplace (its building's `solid: false`, the well and fishing hut),
the trades a flexible founder may take (every catalog trade), the planner's
workplaces (every catalog trade with a workplace building, also when a
settlement has none yet), labour margins and the boat's goods.
`economy_spec::trades`, `is_service`, `open_site` and `trade_label` (the
`display` name, else the role's own name) answer these. Views and journal
lines give the display name (`ProducerView.role`, `LaborSummary.role`,
`view::workplace_name`) and keys for tools (`RegionalOccupationChanged`
carries `from_key`/`to_key` and `from_trade`/`to_trade`). A data trade's
fallbacks are presentation only: generic workshop model, generic crafting
or gathering pose, generic trade colour, the founder template named in data.

**The island save says who made it.** `village::island_save::save_island`
is the kernel's snapshot save plus a stamp in the snapshot header (host data,
outside every hash): `island_save_version` (`ISLAND_SAVE_VERSION`, bumped
with every change to the living world's saved state) and
`island_content_hash`. `check_island_save` answers in types: `Current`,
`OlderIsland { saved, current }`, `NewerIsland { saved, current }`,
`Unreadable { reason }`, each with a sentence for the player.
`load_island_save` loads a current save strictly and refuses every other;
nothing is migrated.

## Consequences

- The host saves through `save_island` and checks with `check_island_save`
  before loading, so the player reads "made by an older version of the
  island" instead of a hash.
- Adding a good, recipe, trade or building is data only
  (`docs/island-colony.md`, "Adding to the catalog"); the proof is
  `tests/island_data_trade.rs` (clay, pots, a pottery and a potter added
  in data).
- What is still named in code is listed in the pass-5 hand-back: data
  defaults, Actor Testbed spawn and labels, and older-island-only paths.
- The living world's content hash changes (new tags and profession fields).
