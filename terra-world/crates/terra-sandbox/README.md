# terra-sandbox

Persistent, command-driven sandbox gameplay over an immutable geographic world.
The server or hosting game owns player identity and movement. Commands carry
intent; they cannot assign inventory, hitpoints, growth, ownership or loot.

## Implemented loop

1. Register a player; the demo supplies wood, stone and wheat seeds.
2. Claim land near the player; claims cannot overlap.
3. Gather bounded resources from registered world tree features or authored
   deposits. Refreshing a source chunk does not refill depleted nodes.
4. Craft planks and stone blocks, build a house/workbench/fence, and set a home.
5. Craft tools at an owned nearby workbench.
6. Plant wheat on owned land; temperature/moisture influence illustrative growth.
7. Harvest once, then craft more seeds from wheat to sustain farming.
8. Damage/demolish player-built owned buildings. Sourced tree point features
   can be removed through persistent tombstones; original data stays unchanged.

Every target is checked against a 20 metre **3D** interaction range. Another
player's property cannot be built on, harvested, gathered from or demolished.
Inventory debits and credits are planned together using checked integer
arithmetic; failed commands leave the complete state unchanged. Demolition
does not refund building costs, preventing a demolition/refund duplication loop.

## Integration

```rust
use terra_core::{GeoPosition, WeatherSample};
use terra_sandbox::{Catalog, Command, SandboxState};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let position = GeoPosition::new(48.137, 11.575, 520.0)?;
let catalog = Catalog::builtin();
catalog.validate()?;
let mut sandbox = SandboxState::new();
sandbox.add_player(1, position)?;
let plot = sandbox.apply(1, Command::ClaimPlot {
    center: position, radius_m: 30.0,
}, &catalog)?;
sandbox.apply(1, Command::Plant {
    plot_id: plot.created_id.unwrap(), position, crop: "wheat".into(),
}, &catalog)?;
sandbox.advance(1.0, &WeatherSample::default())?;
sandbox.save("saves/world.json")?;
# Ok(()) }
```

`Catalog::from_json` loads configurable item IDs, recipes, blueprints and crops.
For a catalog with different item IDs, register players with
`add_player_with_inventory(id, position, inventory, &catalog)`; `add_player`
intentionally supplies the built-in demo's wood, stone and wheat seeds. Starting
equipment comes from trusted game policy, never from a player's command.
`snapshot_for(player, radius)` returns only that player's inventory and nearby
objects. Each object collection is capped at 64 and the view marks truncation.
Network commands additionally need replay/sequence checks and rate limits.

## Persistence and limits

- Versioned JSON save; strict ownership/reference/range checks on load.
- `load_with_catalog` additionally checks that saved blueprint and crop
  parameters still match the game's catalog. A balance/catalog revision that
  changes existing assets requires an explicit migration rather than silently
  changing housing capabilities, footprint sizes or harvest yields.
- A same-directory temporary file is flushed then atomically renamed.
- JSON parsing enables exact `f64` round trips, including GPS coordinates
  produced by local-frame calculations.
- Maximum 16 MiB input, 128 players, 10,000 entries per world collection,
  64 inventory item types and 1,000,000 units per item.
- Commands have bounded quantities (craft 1–20 batches, gather 1–100 units,
  damage 1–25 per action). The hosting server must enforce action frequency.
- Environmental advancement is O(resource nodes + farms), with no loop per
  skipped simulation tick. A step is limited to one simulated day.
- Source registration retains only bounded semantic references and resource
  depletion state. It never modifies provider cache files.

## Deliberate extension points

This foundation implements rules and persistence, not a complete survival game.
Tools are crafted inventory items; equipment bonuses/uses and durability are
game-specific extensions. Housing records a home and ownership; interior
instances, respawning and AI households are not assumed. Farming uses an
illustrative growth model, not crop science. Demolition removes structures,
without fracture physics or debris. Placement checks ownership, reach and simple
footprint overlap; the host should add terrain support, water, slope, collision,
zoning and road access constraints through its environment/physics systems.

Generated resource deposits are gameplay content, not geological claims.
World feature removal uses stable source IDs and affects the overlay only.
Only sourced **tree points** can be removed in this version: native sourced
buildings, roads, water and land-cover polygons remain immutable. A loaded
chunk can contain a clipped fragment of a much larger upstream geometry, so
owning that fragment cannot prove ownership of the whole feature. Future
polygon editing needs chunk-scoped edits or complete-geometry ownership.
Global source refreshes may change upstream IDs and need migration policy.

## Factions and shared property

Player-created factions own **gameplay** property. They are independent of real
countries, political borders, legal ownership and historical sovereignty.

`CreateFaction` creates a leader-owned organization. Leaders issue an
`InviteToFaction`; the invited player must accept with `AcceptFactionInvite`.
Joining without an invitation and self-promotion both fail without changing
state. `SetFactionRole` lets the leader grant or revoke Builder privileges.
Leadership cannot be changed through a role grant.

| Role | Permissions on faction property |
|---|---|
| Leader | Invite players, grant roles, transfer personally owned land into the faction, and all Builder actions |
| Builder | Build, plant, gather, harvest, damage/demolish structures, and edit permitted tree points |
| Member | Use shared housing as a home and craft at a nearby shared workbench; no property alteration |

`TransferPlotToFaction` requires the caller to own the personal plot **and** lead
the target faction. Transfer applies atomically to the land and all contained
player-built structures and crops. New structures/crops inherit the land's
faction owner. The original `owner` field remains the creator/founder for audit;
it does not bypass current faction permissions. A demoted Builder cannot
demolish even a structure they created on faction land. Their existing assets
remain valid and owned by the faction.

The system supports 64 factions and 128 registered players. Each player has at
most eight memberships or outstanding invitations, and snapshots include at
most eight full rosters, prioritizing the player's organizations. Nearby foreign
ownership still identifies the owning faction by ID when a roster is truncated.
All faction and ownership fields are persisted and validated on load. Additive
serde defaults preserve earlier personal-only version-1 saves.

Faction bank inventories, departure/expulsion, leadership succession, diplomacy,
warfare, territory capture and historical boundary simulation are extension
points. Personal inventory remains personal; faction ownership grants access to
property without implicitly sharing every player's materials.
