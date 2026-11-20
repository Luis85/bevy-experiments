# Wildlife and natural resources

Terra's ecology is an authored gameplay layer over geographic terrain and mapped
land use. A forest polygon suggests where a woodland habitat can be placed; it
is not evidence of deer counts, berry yields or mineral deposits. All new
habitats and resource metadata carry an authored flag. Original source chunks
stay immutable.

## Player activities

The reference catalog adds a small connected outdoor progression:

1. Gather berries for food, fallen timber for construction, and wild flax for
   fiber. Stone outcrops provide finite stone.
2. Craft a fishing rod from two planks and four fiber. Fishing spots require
   that rod and remain subject to ordinary physical interaction reach.
3. Bring fish to a workbench and cook it with wood. Raw fish is a resource;
   cooked fish is food.
4. Use a basic tool or an iron tool to mine finite iron deposits. At a workbench,
   refine three ore and two wood into an ingot, then combine two ingots and two
   planks into an iron tool.
5. Use resources for crafting, construction and funded delivery work, or sell
   them to a shop that has a price and enough money.

These tools are capability requirements in this iteration, not durability or
combat equipment. Gathering remains a bounded authoritative transaction; a
failed tool, reach, stock or inventory check changes nothing.

## Actual wildlife bodies

Five species have distinct saved metadata: deer, boar, hare, fox and ibex. The
reference species definitions choose group size, habitat, roaming distance,
flee distance, decision interval, activity hours and recovery interval. They
are configurable game balance, not ecological field estimates.

Animals are ordinary `LivingUnit` bodies with `ActorKind::Animal`. Metadata in
`EcologyWorld.animals` identifies species, home habitat, current behavior and
age. They use the same ground/water/slope/obstacle resolver as other actors;
there is no separate decorative population pretending to be simulation.

The behavior controller chooses grazing/foraging, roaming, fleeing, drinking or
resting. People and vehicles trigger avoidance; hares also avoid nearby foxes.
Activity hours and needs influence rest, foraging and drinking. This does not
implement hunting, combat, predation, individual reproduction genetics, disease,
or a scientific food-web model.

Behavior decisions run at a configurable physical-time interval, normally once
per second. Existing movement still runs at the host's fixed step. Calendar
speed changes nutrition and recovery timing without multiplying physical animal
speed. Owned animals are left to their owner's controller rather than being
commandeered by the wildlife AI.

## Natural-resource lifecycle

| Resource | Reference capacity | Regrowth interval per unit | Required tool |
|---|---:|---:|---|
| Fallen timber | 96 | 180 simulation seconds | None |
| Stone outcrop | 120 | Finite | None |
| Berry thicket | 36 | 90 simulation seconds | None |
| Wild flax / fiber | 48 | 120 simulation seconds | None |
| Fishing water | 24 | 900 simulation seconds | Fishing rod |
| Iron deposit | 72 | Finite | Tool or iron tool |
| Freshwater source | 200 | 30 simulation seconds | None |

The `EcologyConfig.resources` definitions own these initial values. Resource
nodes use the existing `ResourceNode` inventory, depletion and regrowth path.
`SandboxState::advance` performs bounded arithmetic for regrowth; ecology does
not independently refill the same node.

New nodes use stable geographic identifiers and a persistent region ledger.
Reloading a chunk does not refill depleted ore or overwrite partially harvested
berries. New authored resources use `source_feature: None`: depleting a berry
patch or fishing spot cannot hide an entire source forest or lake. Individual
source-tree removal retains the existing source-feature tombstone rules.

## Bounded population and work

Default upper bounds are 32 habitat records, 64 managed animal bodies, 192
natural-resource records and 256 registered ecology regions. Existing source
resources also share the sandbox's overall resource-node budget. The ecology
keeps space in the living actor budget for new player admission.

The nearby demonstration seed has five small groups. It adopts the earlier
demo's existing deer instead of leaving them on a second behavior implementation.
Streamed source chunks can add woodland, meadow, shoreline and elevated-terrain
habitats within the same budgets. The local seed has reserved budget so distant
chunk completions cannot consume all starter activities.

Habitats have carrying capacity. If a game removes an animal through its own
authoritative lifecycle, ecological metadata is reconciled and the habitat can
recover a body after its configured interval. Recovery is capped at one body per
habitat per update, with a global budget of four spawn attempts per behavior
pass. A large elapsed-time update cannot cause an unbounded birth loop. New
bodies require the host's resident-ground resolver to admit their position.

Population simulation is local and bounded. The implemented
[regional persistence layer](REGIONAL_PERSISTENCE.md) can retire and restore
resource, habitat and body records within one authority while retaining aggregate
limits and frozen dormant state. Continuous world-scale ecology, distant
aggregate populations and migration between authoritative servers remain
extension work for the hosting game.

## Reusable integration

The domain APIs are independent of Bevy:

- `register_ecology_chunk(chunk, catalog)` adds habitat definitions and sampled
  resource positions; it never spawns bodies before the living world exists.
- `seed_ecology_near_with_environment(origin, catalog, resolver)` seeds the local
  reference scenario. The resolver returns a valid dry surface or `None` for
  unavailable or inadmissible terrain.
- `advance_ecology_with_environment(real_dt, sim_dt, hour, resolver)` runs before
  `advance_living_with_environment`. Ecology chooses intent; the normal living
  resolver commits physical movement.
- `validate_ecology_gather(player, node)` is called by the ordinary Gather command
  before its transaction commits.
- `EcologyWorld::validate` checks full saved references and policy bounds.
  `EcologyView::validate` checks a bounded partial replica without requiring
  absent area-of-interest entities.

The no-resolver variants intentionally use an identity resolver and are useful
for flat fixtures or a host with a separately guaranteed surface. Games with
streaming terrain should use the environment variants.

`EcologyWorld::snapshot` only includes metadata for bodies and resource nodes
already in the enclosing public snapshot. `EcologyView::retain_visible` keeps
those references coherent after transport-budget reductions and exposes
truncation. A replica is read-only simulation state, not a complete world save.

Existing saves without the ecology field deserialize to an empty default layer.
New ecology policy is saved with the world; changing capacities or species rules
for existing habitats requires an explicit migration rather than silently
changing saved yields or carrying capacity.
