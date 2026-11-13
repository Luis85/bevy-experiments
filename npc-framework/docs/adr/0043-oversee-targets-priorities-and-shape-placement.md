# 0043: Oversee targets and priorities, Shape plots and props

Date: 2026-11-13. Status: accepted for the living world (`living_world`).

## Context

Roadmap phase 4 gives the overseer a profession table with +/-, village
priorities, and Shape tools to place farm and garden plots and props. The
interface had the controls but no simulation command behind them. Every
player effect must be a recorded, validated, journaled content command, and
"normal" must stay today's behaviour exactly.

## Decision

**Wishes, not orders.** Both Oversee controls are standing state in one
hashed resource, `frontier::oversee::Oversight` (`living_oversight_v1`, in
saves), read by the rules that already decide.

- `SetProfessionTarget { settlement, profession, wanted }` sets the wanted
  headcount of one trade in one settlement (`wanted: null` clears it; at
  most `MAX_TARGET`, 40; a service trade such as the inn is refused, since
  it is staffed one keeper per building). The labour review
  (`labor::consider_assignment`) reads it: a vacancy of a trade under its
  target in the worker's own settlement counts like a pulled one (it skips
  the shortage and margin evidence, ranks just under an empty essential
  trade, and is open to a settled specialist whose own trade can spare it:
  over its own target, more than one worker, or not essential); a trade at
  or over its target takes nobody new; a worker of a trade at or under its
  own target stays unless its work is blocked; a worker of a trade over its
  target is surplus. Cooldowns, the review interval, the return rule and the
  safe boundary are unchanged, so nobody leaves mid-task and moves come one
  review at a time. The essential-coverage floor still keeps the last worker
  of an essential trade whose goods are wanted. The journal reason of such a
  move is "Answering the overseer's wanted headcount".
- `SetVillagePriority { settlement, topic, profession?, level }` sets
  `low`, `normal` or `high` on `housing`, `storage`, `workplace` (all
  workplaces, or one trade's with `profession`), `food_reserve`,
  `fuel_reserve`, `coats` or `boat_orders`; `normal` removes the entry. The
  civic planner waits half the usual reviews (at least one) for a high need
  and twice for a low one, and answers a high need before the others
  (`oversee::order_projects`: higher level first, otherwise its own order:
  homes, fields, stores, seats, trades); fields follow `food_reserve`. The
  stock target of a food or the fuel good is `stock_target_days` doubled at
  high and halved at low, and so is the winter fuel reserve of a home. Coats
  at low are made to order only; at high every coatless resident's coat is
  stocked. The boat's open order counts double at high and not at all at low
  in the port's production plan (the boat still buys what is offered).

**Shape places plots and props.** `PlacePlot { plot, x, z, rotation }` lays
out a catalog building that has a `field` (the shipped `field`, and the new
`garden`, which the planner never commissions) finished and empty, on
`PlaceBuilding`'s ground rules and never over a road; `RemovePlot { plot }`
is its undo, refused for a founding field and while the plot holds seed,
goods or a crop or somebody works it. Props are catalog data
(`economy.props`: `id`, `display`, `footprint`, `solid`, `model`); the
shipped set is drawn by manifest-listed models: barrel, crate stack, fence
piece, flower bed, signpost, flag and wheelbarrow. `PlaceProp { prop, x, z,
rotation }` puts one with its body starting at cell `x, z` on grass or stone
(not water, a road or bare soil), on free cells near a settlement, without
cutting off ground; a solid one blocks walking (`GridBlocker`) and every
prop holds its cells against later buildings. `RemoveProp { prop }` is its
undo. Plots export as `buildings`, props as the new `living_world.props`
list, and both are laid out again when the world is built.

## Consequences

- The UI reads `oversee::profession_table`, `oversee::priorities`,
  `props::catalog` and `props::props`; every command refuses with one plain
  sentence (`CommandError::Unavailable`).
- With no target and every priority normal the simulation is unchanged:
  every new branch is behind an entry the overseer made.
- The living world's content hash changes (new resource, component, commands,
  props and the garden in the catalog). No Actor Testbed or village state
  changes.
