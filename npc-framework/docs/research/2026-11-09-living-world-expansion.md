# Living-world expansion research — 9 November 2026

This research informs the new opt-in regional island. It is not validation evidence.
The previous 64-metre island remains a saved construction contract.

## Primary evidence

| Source | Finding and design consequence |
| --- | --- |
| [Banished: pathfinding problems](https://shiningrocksoftware.com/2013-11-21-more-bugs-pathfinding-problems/) | Straight-line distance produced poor workplace, home and supply choices. A coarse connectivity graph made walking-distance estimates practical. Regional sourcing must consider reachability and travel cost. |
| [Banished: game-code design](https://shiningrocksoftware.com/2018-11-09-game-code-design/) | Hard-coded professions/items and distributed decisions complicated extension. Keep authored definitions, a single resident arbiter and explicit domain owners. |
| [Factorio: Noise Expressions 2.0](https://www.factorio.com/blog/post/fff-390) | Macro-shapes, several noise scales and starting-area constraints offer artistic control. Author settlement positions and routes; use deterministic variation around them. |
| [Factorio: New terrain, new planet](https://www.factorio.com/blog/post/fff-401) | Overlapping placement rules excluded assets entirely. Deliberate distribution and clustering made subregions distinct. Test biome palette coverage and transitions. |
| [Red Blob Games: original terrain implementation](https://www.redblobgames.com/maps/terrain-from-noise/) | Elevation and moisture are useful biome inputs. This repository needs integer equivalents, not the tutorial's floating-point implementation. |
| [Eco: Worlds of Consequence](https://strangeloopgames.com/worlds-of-consequence/) | Interactions between ecology, economics and player action create meaningful consequences. Harvesting, regeneration, work demand and restoration should form a visible causal loop. |
| [Stardew Valley 1.6 changelog](https://www.stardewvalley.net/stardew-valley-1-6-update-full-changelog/) | Environmental events, contextual dialogue and seasonal appearances give familiar places reasons to be revisited. Regional activities should accompany visual variety. |
| [Bard Tree architecture](https://bardtreelimited.com/architecture/overview), [orchestrator](https://bardtreelimited.com/architecture/orchestrator) | Explicit ownership and module registration apply. Distance-based authoritative scheduling does not: camera position cannot affect this deterministic simulation. |

These applications are design inferences, not claims of equivalent implementations.

## Baseline audit

Expanded geography still uses a 64 by 64 one-metre grid, one resource location per
kind, and six plots. Terrain and editor validation assume those dimensions.
Resources refill daily and wildlife habitats top up to a fixed population. The
production allocator pools residents globally and several searches use linear
distance. Colony integrity assumes founding residents never change professions.

The generic SpatialGrid already accepts and saves arbitrary bounded dimensions.
Named content registration, recorded commands and snapshot APIs can express the
new mechanics without generic-core changes. New state must be registered, with
explicit fixed-width integer hash encodings and profile-only registration.

## Direction

Build a 256 by 256-metre island containing Harbour, Alderwood and Stone Ridge.
Each settlement has homes separate from workplaces, renewable subsistence, and
specialized surplus. Existing physical inventory, crafting, trading, movement
and survival rules remain the foundation. Employment is separate from identity,
residence and current location. No hidden material transfers connect villages.

The exploration loop is preparation, travel by landmarks, discovery, useful
work or trade, and returning to changed conditions. Woodland depletion and
restoration should be visible. Replanting consumes real inputs and matures over
time. Habitat capacity and resource growth must respond to disturbance, with
persisted harvest, regrowth and restoration history.

## Ownership and compatibility

`frontier/world_spec` owns immutable construction. `settlements` owns settlement
identities and membership. `terrain` owns heightfield cells and edits. `routes`
provides read-only reachability/cost. `ecology` owns regeneration and ecological
state. Labor uses the existing resident tree; actions own movement and physical
execution. Production retains sole ownership of its plan. Views project state.

The optional LivingWorldSpec is frozen into construction. Missing means the old
profile. New Game uses a separate save file; Continue always reconstructs the
saved profile. New hash names are registered only for this profile. Existing
replay/snapshot oracles and old constructors are preserved. Migration, if later
requested, must create a validated new save while preserving the original.

This increment remains heightfield ground navigation. Stacked floors, tunnels
and traversable bridges over other traversable surfaces require a separate
layered-navigation design. More visual height does not establish that support.

## Acceptance and risks

Verify deterministic repeated runs and save/resume; preserve old replay hashes;
run every settlement through a full seasonal cycle without hidden supplies;
prove money/material conservation and physical delivery; test blocked routes
and reservation release; compare harvested and restored ecosystem histories;
test atomic terraform edits and actor-scoped discovery. Native screenshots and
playthroughs must demonstrate regional identity and usable interactions.

Profile construction, simulation ticks, route queries, terrain updates and save
size before scaling further. Principal risks are global supply assumptions,
long-distance survival, role-dependent validation, restoration exploits and
view code that assumes the original terrain dimensions. Finite ore also needs
a renewable clothing or recovery alternative before indefinite sustainability
can be claimed: the incumbent hunting chain consumes metal spears.
