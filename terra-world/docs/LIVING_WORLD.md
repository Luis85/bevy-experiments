# Living world, control, economy and logistics

Terra World 0.2 connects the existing gameplay overlay to a bounded, deterministic settlement simulation. `terra-sandbox` owns the rules and persistent data; the Bevy adapter owns loaded-surface and scene interaction; the multiplayer host applies the same rules and replicates a public view.

## Principal, body and inventory

A player ID is a server-assigned gameplay principal, or the trusted local player in a single-player host. A living unit is a physical body with a geographic position, archetype, movement order and needs. These identities stay separate:

- `LivingAccount.player_id` owns the account and shared player inventory.
- `avatar_unit` identifies the original avatar.
- `active_unit` supplies the current physical context for trading, building and other reach checks.
- `possessed` determines whether direct input or autonomous orders control that body.

Possession requires ownership. It changes the active body, cancels that body's previous order, and enables direct control. Selecting an owned unit in RTS mode changes the interaction context while retaining its current order. Releasing direct control keeps the active body and returns to command mode; it does not teleport the principal back to the original avatar. Select the avatar explicitly to return to it.

All owned units use the principal's inventory and wallet. Needs remain per body. This is a deliberate reference-game rule rather than a claim that each unit carries an independently replicated backpack. NPC retail money and courier cargo are separate server-owned fields.

Commands are `Command::Living { action: LivingCommand }`:

| Action | Validation and behavior |
|---|---|
| `SelectActive { unit_id }` | Owned body; retain its order; use RTS mode and that body's interaction position. |
| `Possess { unit_id }` | Owned body; clear its route and velocity; begin direct control. |
| `Release` | Stop manual velocity and retain the active body in command mode. |
| `MoveUnits { unit_ids, destination }` | Between 1 and 32 unique owned units; destinations within 2 km; small formation offsets; route toward the destination. |
| `StopUnits { unit_ids }` | Validate the complete group before stopping any body. |
| `Rest { unit_ids }` | Validate the complete group; stop and restore energy over time. |
| `Buy` / `Sell` | A valid catalog item, quantity 1–100, nearby shop, available stock and sufficient payer funds. |
| `Consume { item }` | Consume one bread or water from shared inventory to restore that need in the active biological body. |
| `SetProduction { site_id, enabled }` | Owned site within interaction reach; pause or resume processing. |

The first player to join an authored demo settlement owns its production facilities. Later players can trade but cannot remotely operate someone else's production. Existing faction roles continue to control shared sandbox plots, houses, crops and workbenches; the reference's production sites use personal ownership.

## Credits and finite shops

Money is an unsigned integer in **hundredths of a credit**: `25000` is **250.00 credits**. No floating-point currency arithmetic is used. Balances are bounded by `MAX_MONEY`. New accounts receive a one-time authored starting allowance; reloading or resuming an existing account does not issue another allowance.

`TradePrice.buy` is what the customer pays. `TradePrice.sell` is what the shop pays the customer. These are public server-defined prices, never client-supplied values. A trade validates both inventories and both balances before replacing any of them. Selling cannot overdraw a shop. Buying cannot create stock. Rejected trades leave money, inventory, stock and needs unchanged.

Reach uses the active body's geographic horizontal distance and altitude difference, with a 20-metre three-dimensional limit. A remote camera position does not authorize a remote purchase. The existing construction and gathering commands use the same principal position.

The village exchange stocks bread, water, wood, planks, grain, flour, seeds, stone and tools. AI citizens with a finite pocket balance can buy and consume bread or water when their patrol reaches the shop. Their payment transfers to the shop till. Their consumption removes the purchased goods. Producers and delivery vehicles transfer goods within the authored settlement without inventing intermediate customer payments.

Prices are fixed reference-game prices. There is no auction matching, debt, credit creation, wage market, taxation, dynamic price discovery or macroeconomic model in this version.

## Food, water and energy

`Needs` stores satisfaction from zero to one:

- `food`: one means fed, zero means depleted.
- `water`: one means hydrated, zero means depleted.
- `energy`: one means rested, zero means exhausted.

Food and water decrease with simulation time. Energy changes with physical activity and real simulation steps, so accelerating the season clock does not multiply locomotion velocity. Rest restores energy. Bread restores food; water restores hydration. Fully satisfied needs reject unnecessary consumption.

Biological movement capacity decreases with the lowest need, to a minimum of 35% of the normal speed. The sample does not kill actors from starvation or implement medical physiology. Vehicles retain their vehicle speed profile and cannot consume food as a vehicle body.

Nominal direct-control caps before biological need modifiers are 7 m/s for a human, 14 m/s for a road vehicle, 10 m/s for a ship and 20 m/s for the reference air categories. These are gameplay limits, not physical vehicle specifications.

## Production chains

The authored demo contains seven sites:

| Site | Inputs per cycle | Output per cycle | Simulation seconds |
|---|---|---|---:|
| Field | 1 water + 1 wheat seed | 8 wheat | 90 |
| Well | One finite extraction cycle | 4 water | 30 |
| Managed woodlot | One finite extraction cycle | 4 wood | 60 |
| Sawmill | 2 wood | 4 planks | 60 |
| Grain mill | 2 wheat | 3 flour | 45 |
| Bakery | 2 flour + 1 water | 4 bread | 60 |
| Seed store | 1 wheat | 4 wheat seeds | 50 |

The well starts with 2,000 extraction cycles and the woodlot with 1,000. Conversion sites require their actual inputs. A stock limit of 2,000 total units provides storage backpressure. Each update processes at most eight completed cycles per site, retaining only bounded catch-up time. Advancing the clock by a large amount cannot trigger an unbounded processing loop.

The existing personal crafting catalog also includes flour and bread recipes requiring a workbench. Wheat harvested from player farms can therefore enter the same item economy. The settlement chain's seeded sites are authored facilities; creating a sandbox house does not silently turn it into a producer.

## Couriers and traffic

Six autonomous couriers service production inputs before replenishing retail shelves. Each delivery has a finite item quantity and two phases:

1. **To pickup:** the courier travels to the supplier. Stock is removed only on actual arrival, and the available amount is checked again.
2. **To dropoff:** cargo is held on the courier. It enters the destination inventory only after arrival. Completed cargo is removed from the courier exactly once.

A failed or blocked movement cannot complete a delivery. Surface/collision resolution happens before movement commits and before the delivery system checks arrival. Full destinations retain the cargo on the vehicle for later delivery.

Road routes come from normalized road polylines where available. The reference builds a bounded road-vertex graph, connects nearby matching vertices, and routes between the nearest vertices. It adds short detours around authored facility footprints and uses service forecourts outside those buildings. Eight traffic vehicles follow road patrols, with simple same-direction following distance. This is a lightweight traffic model: junction priorities, traffic lights, lane changes, motorway routing, complex bridges and a full navigation mesh are not implemented.

The fallback settlement has 24 citizens, eight traffic vehicles, six couriers and four animals. Very short imported road networks spawn fewer vehicles rather than stacking them at the same point. Each player receives an avatar, two crew members, a utility rover and a survey helicopter. The world-wide living-body budget is 512. This is a single bounded demo settlement, not a claim that every imported city receives its actual population.

## Source placement and provenance

Suitable nearby map tags place facilities when available: shops/marketplaces, drinking water, farmland, forest, sawmills, mills and bakeries. Road geometry contributes routes. Missing facility positions use authored offsets from the settlement origin.

Every facility stores `source_feature` when a feature informed its location, while `authored` remains true: the simulated business, inhabitants, opening state, production rate, inventory and money are authored content even when a real map supplied the point. Historical fixtures do not become factual population reconstructions through this simulation.

`Shop::model_position()` and `ProductionSite::model_position()` place fallback buildings north of their service points. Their collision radii and `LivingWorld::facility_footprints()` are shared by the renderer and hosts. Source-linked facilities defer to the provider's real building geometry.

## Hosting and persistence

The public domain entry points are illustrated below. `environment.resolve` is a callback supplied by the hosting game, returning a validated position or `None`:

```rust
state.seed_living_demo(origin, &features, &catalog)?;
state.ensure_living_player(player_id)?;

// Authoritative hosts resolve known terrain, water and obstacles before motion.
state.advance_living_with_environment(
    real_dt,
    simulation_dt,
    &catalog,
    |kind, from, candidate| environment.resolve(kind, from, candidate),
)?;
```

`advance_living` is a convenience for domains without environmental constraints. A gameplay host should use `advance_living_with_environment`. Returning `None` rejects the movement. Both reference hosts validate resident terrain, water travel mode, slopes, bridge support and source-building footprints before committing movement. The CLI retains at most 512 source chunks, populated by its existing streamer or historical pack; missing resident geometry blocks movement and placement. Swept checks also include player construction and authored facility footprints. Games can override the memory-only `WorldEnvironment::resolve_movement` and `allows_placement` hooks with their own physics services. `set_controlled_motion` is a trusted host hook after movement validation, not a command accepting client coordinates.

`SandboxState.living` is serialized alongside inventories, plots and factions. The new field has a default, so an older save without a living layer remains readable and does not automatically acquire money. The chosen host explicitly enables the authored demo. Source-world identity validation still applies: changing procedural terrain revisions is not an implicit migration of an existing world's geometry.

The reference server executable enables the living settlement by default; `--no-living` disables initialization for new worlds. The reusable `Server::bind` API leaves it opt-in through `enable_living_demo`. Restored living state remains authoritative and is not regenerated by that initializer.

## Multiplayer and bounded views

Player identity and command replay protection remain the existing protocol's responsibility. The host maps validated movement to the player's active body and applies living commands through the same authoritative command queue. Disconnecting clears manual possession and velocity while preserving the selected body and wallet for resume. Outstanding RTS orders remain authoritative. The CLI keeps the spawn neighborhood resident so later players can join even after other players travel far away.

Living sessions send sandbox/living snapshots at up to 5 Hz; the principal actor pose remains in the 10 Hz actor stream. Ordinary sessions retain their 1 Hz sandbox view. Living views expose only the requesting player's wallet. NPC money, private cargo inventories and internal path arrays are omitted.

A view initially includes at most 64 nearby/owned bodies, eight shops, 16 production sites, 16 deliveries and eight road routes. Active and original-avatar bodies are prioritized. The complete sandbox view is reduced, if necessary, to a 240 KiB serialized budget, leaving room under the 256 KiB transport-frame limit. Truncation is explicit. A partial view is never a writable full-world save.

`LivingView::validate_for(player_id)` validates incoming partial data before a client installs it. `LivingWorld::from_view` constructs the read-only replica. Full saves additionally validate body ownership, active-body context, cargo reservations, references, capacities and catalog items.

The network transport remains a bounded TCP reference for trusted sessions. Secure public transport, account services and distributed simulation are separate production concerns.
