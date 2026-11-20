# Living world, ecology, territory and economy

Terra World 0.9 connects geographic source chunks to a bounded, persistent game
simulation. `terra-sandbox` owns transactional rules and saved data. The Bevy
adapter resolves loaded surfaces and presents the world. The multiplayer host
applies the same commands and publishes bounded, recipient-specific views.

The implemented loop is gathering and production → trade or supply work →
credits and community XP → housing, farming and territory progression. Wildlife
uses the same physical bodies and terrain policy as other living units. Map data
can suggest where habitats, roads or services belong; populations, resource
yields, businesses and progression are authored game systems.

The quantities and rules below correspond to the
[living model](../crates/terra-sandbox/src/living/model.rs),
[ecology definitions](../crates/terra-sandbox/src/ecology/model.rs),
[settlement model](../crates/terra-sandbox/src/settlement/model.rs),
[economy maintenance](../crates/terra-sandbox/src/settlement/economy.rs) and
[catalog](../crates/terra-sandbox/src/catalog.rs) shipped with this release.

## Principal, body and inventory

A player ID is a server-assigned gameplay principal, or the trusted local player
in a single-player host. A living unit is a body with a geographic position,
archetype, movement order and needs. These identities stay separate:

- `LivingAccount.player_id` identifies the account; `PlayerState.inventory`
  stores its shared items, and `LivingAccount.money` stores its credits.
- `avatar_unit` identifies the original avatar.
- `active_unit` supplies the physical context for trade, gathering, construction,
  contract delivery and territory-center reach checks.
- `possessed` selects direct input or autonomous orders for that body.

Possession requires ownership, selects the body and cancels its previous order.
Selecting an owned body in RTS mode changes the interaction context while
retaining its order. Releasing control returns to RTS and retains that active
body; it does not teleport interaction back to the avatar. Inspecting an unowned
resident or wild animal grants no control and leaves the prior owned active body
as the interaction context.

All owned units use one principal inventory and wallet. Needs and positions
remain per body. NPC pocket money and courier cargo are separate server-owned
fields. Separate backpacks, payroll for the owned RTS crew and shared faction
banking remain additional game policies. Commercial employee contracts, wages,
depots and household spending use the separate
[transport-company layer](COMPANIES.md). Private bays can now buy and sell actual
shop goods with company money when the avatar is within 20 m of both sites.
Warehouse fleet sales reserve the buyer's finite cash at loading and pay after
physical delivery; internal transfers move private stock between owned bays with
no public reward. Source refund and private destination space remain reserved
until completion or return. These are distinct inventories from linked public
producer/shop stock; opening a bay never grants ownership of that stock.

`Command::Living { action: LivingCommand }` includes:

| Action | Validation and behavior |
|---|---|
| `SelectActive { unit_id }` | Owned body; retain order and use its interaction position. |
| `Possess { unit_id }` | Owned body; clear route and velocity, then enable direct input. |
| `Release` | Stop manual velocity and retain the active body in command mode. |
| `MoveUnits { unit_ids, destination }` | 1–32 unique owned units; each within 2 km of the destination; small formation offsets and bounded routing. |
| `StopUnits` / `Rest` | Validate the complete group before changing any body; rest restores biological energy. |
| `Buy` / `Sell` | Known item, quantity 1–100, shop reach, available stock and payer funds. |
| `Consume { item }` | One bread, berries, cooked fish or water from shared inventory; active biological body must need it. |
| `SetProduction { site_id, enabled }` | Personally owned site within 20 m; pause/resume processing. |

The first player in the authored demo owns its production sites. Faction roles
govern sandbox plots, houses, crops and workbenches; these reference production
sites retain personal ownership.

## Credits, prices and funded activity

Money is an unsigned integer in **hundredths of a credit**: `25000` means
**250.00 credits**. `MAX_MONEY` is 1,000,000,000 minor units. New accounts receive
a one-time authored starting allowance. Loading or resuming an account does not
reissue it. A custom host can choose a different admission policy.

`TradePrice.buy` is the customer's price and `sell` is the amount the shop pays.
Prices come from the host. A transaction validates both inventories and both
balances before committing. Selling cannot overdraw a shop, buying cannot create
stock, and rejection leaves the transaction unchanged. Reach is the active
body's horizontal geographic distance combined with altitude difference, limited
to 20 m. Camera travel grants no additional reach.

The village exchange quotes the catalog's food, materials, seeds, tools and new
natural-resource products. A quote does not guarantee stock: newly supported
items can have zero stock until supplied through gathering, production, sales
or contracts.

Every 60 simulation seconds, bounded settlement maintenance adjusts prices from
saved reference anchors. Buy quotes vary from 80% to 150% of their anchors as
stock rises or falls around a target of 40 units. Sell quotes are capped relative
to the anchor and decrease under surplus. Repeated maintenance does not compound
previously adjusted prices. This is a stock heuristic, not an order book or an
economic forecast. Current quotes remain visible in the Market panel.

AI citizens buy and consume bread or water when their route reaches the shop.
Their payment enters its till. After a simulation hour, if production has made
progress since the previous pay check, eligible citizens with less than 10.00
credits receive up to 1.50 credits each from the market's existing money. Payment
stops when the till is empty. This is finite sample income circulation; it does
not model employment contracts, wage debt or a labor market.

### Supply contracts

`SettlementWorld.contracts` holds at most 16 funded offers. Maintenance may post
at most four new offers per pass, prioritizing missing producer inputs over shop
replenishment. Each records an item, quantity, target service position, reward,
XP, claimant and deadline. The issuer's money is removed when the offer is
posted and held as that contract's escrow.

- A player can accept three requests at once. Acceptance refreshes the default
  259,200-second (72 world hour) window.
- `FulfillContract` requires ownership of the request, an unexpired deadline,
  the complete quantity and 20 m reach to the destination. It debits personal
  goods, credits destination stock, pays escrow and grants XP atomically.
- A full producer store or invalid target rejects fulfillment. It does not
  discard the goods or pay a reward without delivery.
- Cancellation returns the funded request to the public board. Expiry releases
  the claimant and refreshes its public window during maintenance. Escrow stays
  attached to the request in both cases; inventory is not confiscated.
- Fulfillment removes the request, so a repeated command cannot receive a
  second reward. When stock remains low and funds permit, maintenance can post
  later work.

Production-input offers award 40 XP; retail requests award 25 XP. Reward amounts
derive from the current authoritative quote, requested quantity and, for
production input, a bounded travel allowance. A reward is not a guaranteed
margin after buying every item at retail. Newcomer, Contributor, Steward and
Pathfinder ranks occur at 0, 50, 250 and 1,000 XP. XP belongs to the player and is
not spent by territory upgrades.

There is no taxation, lending, auction matching, general banking, inflation
model or autonomous price-discovery system. Balances, including contract escrow
and territory treasury, are persisted and validated.

## Food, water and energy

`Needs` stores satisfaction in `[0, 1]`: one is fed/hydrated/rested; zero is
depleted. Food and water decrease with simulation time. Exertion and rest use
physical simulation steps, so accelerating the calendar does not multiply body
velocity. Bread restores 0.50 food, berries 0.20 food and 0.06 water, cooked fish
0.55 food, and water 0.60 hydration, capped at one. A fully satisfied relevant
need rejects unnecessary consumption.

Biological movement capacity decreases with the lowest need, with a 35% minimum
speed factor. Vehicles retain their vehicle profile and cannot eat. The sample
does not implement starvation death, medical physiology or combat. Nominal
direct-control caps before biological need modifiers are 7 m/s for a human,
40 m/s for a road vehicle, 10 m/s for a ship and 20 m/s for reference air bodies.
Registered vehicles additionally use their model, upgrades, condition, fuel,
license, parking brake, rental expiry and present driver’s needs. They accept
pedal/steering intent rather than arbitrary nonzero velocity intent. These are
bounded gameplay limits, not surveyed vehicle specifications; see
[TRANSPORT.md](TRANSPORT.md).

## Wildlife and renewable/finite resources

`SandboxState.ecology` owns `EcologyConfig`, habitat definitions, wildlife metadata,
resource metadata, a seed ledger and metrics. `LivingWorld.units` remains the
authoritative source for animal position, velocity, order and needs. The same
environment resolver governs animals and other bodies.

The five reference species are deer, boar, hare, fox and ibex. Data-driven species
definitions set woodland/meadow/shore/highland habitat policy, group capacity,
roaming range, flee distance, decision interval, activity hours and recovery
time. Animals graze/forage, roam, flee nearby people or moving road vehicles,
travel to configured drinking points and rest. These are game behaviors;
foxes do not implement a predator/prey food web.

The default decision pass runs once per physical second. Decisions are limited
to habitats within 1,800 m of registered player positions and require resident,
admissible ground. The metadata and population remain saved when a region leaves
the active set. Population restoration after an authoritative body removal is
slow and capped by habitat capacity, with at most four spawn attempts per
behavior pass. Reloading a chunk does not reset a depleted deposit or create a
second copy of its animals.

`register_ecology_chunk` uses available forest, grass/farm, water and relief
features as suggestions for authored habitat/resource placement. The native demo
also seeds a small authored ecosystem near its settlement. Unknown terrain is
not treated as available land. Each new ecology resource has separate metadata;
depleting a berry patch or ore deposit does not tombstone an entire provider
forest or water polygon.

| Resource kind | Item | Default capacity | Seconds per recovered unit | Requirement |
|---|---|---:|---:|---|
| Timber | `wood` | 96 | 180 | None |
| Stone | `stone` | 120 | 0: finite | None |
| Berries | `berries` | 36 | 90 | None |
| Fiber | `fiber` | 48 | 120 | None |
| Fish | `fish` | 24 | 900 | `fishing_rod` |
| Iron ore | `iron_ore` | 72 | 0: finite | `tool` or `iron_tool` |
| Water | `water` | 200 | 30 | None |

These values apply to built-in ecology definitions; existing source-tree and
starter nodes can differ. `ResourceNode.remaining`, `capacity` and
`regrow_seconds` are the actual harvest state. Tool checks precede the normal
gather transaction and do not consume a tool. The catalog adds fishing rods,
cooked fish, iron ingots and iron tools; exact recipes are in
[GAMEPLAY.md](GAMEPLAY.md#recipes-and-construction-costs).

Ecology hard ceilings are 32 habitats, 64 animal bodies, 192 ecology resource
nodes and 256 registered region keys. Settings can reduce these bounds. The
seed ledger and populations share the reference authority’s aggregate budgets.
[Regional persistence](REGIONAL_PERSISTENCE.md) can retire and restore these
records within one authority without lifting those caps. At a budget limit,
additional content is not seeded. Larger aggregate populations, distant
simulation and cross-server migration require additional host policy.

Native ecology observers are the positions of bodies owned by active principals.
Retained dormant-owner bodies do not activate wildlife nearby or grant it near
decision priority. This matches the server's connected-owner observer policy;
their saved occupancy, needs and orders remain intact. Resuming a principal
restores its ordinary observer influence. Existing loaded ecology clocks and
the frozen cold-region policy are unchanged. A headless Bevy adapter regression
exercises dormant, resumed and dormant-again states; it is a scheduling/authority
check, not a measured frame-rate improvement.

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

The well starts with 2,000 extraction cycles and the woodlot with 1,000. Conversion
sites require actual inputs. A 2,000-unit total stock limit provides storage
backpressure. An update processes at most eight completed cycles per site, with
bounded catch-up time. A large clock step cannot trigger an unbounded work loop.

Player-grown wheat and personally crafted goods use the same item IDs as
industrial stock. Contracts can deliver goods into these stores. Existing player
houses do not become businesses, and a neighborhood/village/city charter does
not automatically create production sites.

## Physical deliveries and road traffic

Six authored couriers serve input shortages before retail shelves. Dispatch
accounts for outgoing reservations and cargo already assigned to a destination.
Each delivery has a finite quantity and a physical body:

1. `ToPickup`: travel to the supplier. Stock is removed only after arrival and
   checked again against what is still available.
2. `ToDropoff`: carry the removed goods on the courier. Destination stock changes
   only on physical arrival, after authoritative movement resolution.
3. `Returning`: retain the same cargo while traveling back to its supplier when
   delivery cannot complete. Goods return to source stock only on arrival.

Stalled travel triggers bounded replanning/detour attempts. An uncollected
pickup can be abandoned safely because no goods have moved. A blocked or full
dropoff can transition to returning cargo. A full return destination leaves
goods on the courier rather than deleting them or pretending delivery occurred.
Metrics expose completed deliveries, replans and returned cargo.

The routing graph comes from normalized road polylines and supported one-way,
access, numeric speed and grade metadata. It connects compatible crossings,
uses service forecourts and includes bounded facility detours. Known road
restrictions and over-budget plans cannot silently become a direct shortcut.
Up to eight seeded traffic vehicles use reference lanes and physical-time
intersection admission with FIFO tickets, exit-space checks and stalled-permit
requeue. Manual vehicles participate in occupancy checks, but road-law guidance
is advisory.

Complete traffic law and signals, lane changes, arbitrary turn restrictions,
surveyed bridge networks and general navmesh pathfinding remain outside this
implementation. Bounded replanning does not guarantee a route through arbitrary
imported geometry. See [CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md).

The fallback seed supplies 24 citizens and five owned bodies per player: avatar,
two crew, rover and helicopter. Imported road/terrain coverage can reduce traffic
or other placements. Wildlife adds habitat-managed bodies within the shared
512-unit living-world budget. This is one bounded reference settlement, not a
claim that every imported city receives an observed population.

## Territory, faction rights and public works

`SandboxState.settlements` stores named `Settlement` claims separately from
political source data. Each claim has a founder, optional faction owner, center,
tier, public material stock and treasury. Founder identity is an audit field;
current faction roles govern permission without a founder bypass.

| Tier | Radius | Maximum contained plots | Cost to reach this tier | XP / infrastructure |
|---|---:|---:|---|---|
| Neighborhood | 100 m | 6 | 20 wood + 10 stone; 50.00 cr deposited into treasury | None |
| Village | 300 m | 24 | 30 planks + 10 stone blocks; 25.00 cr from treasury | 50 XP; 2 houses |
| City | 700 m | 64 | 120 planks + 50 stone blocks + 8 tools; 150.00 cr from treasury | 250 XP; 6 houses + 3 current crop plots |

`Command::Settlement` exposes `Found`, `Contribute`, `Fund`, `Upgrade`,
`AcceptContract`, `FulfillContract` and `CancelContract`. Founding checks reach,
name, materials, credits and ownership before changing state. Contributions,
funding and upgrades require current build permission and 20 m reach to the
center. Upgrades consume public stock and transfer charter credits to a market;
they do not create money. There is no arbitrary public-stock/treasury withdrawal.

Claims cannot overlap other communities or enclose differently owned prior
plots. New plots in a faction territory inherit its faction. Plots and building
footprints still obey ordinary ownership and overlap checks. Members can gather
on unplotted shared community land; the stricter work-plot role gate still applies
inside a faction plot. Upgrades count permitted housing and currently planted
farms inside the existing territory, using the acting player's persistent XP.

The world limit is 32 territories, with at most four founded by one player. Public
stock is limited to 4,000 total units. These circular permission areas do not
change a country's real border, prevent visitors from walking through, generate
an urban layout, implement conquest or supply legal land title.

## Source placement and graphics

Map tags can inform markets, drinking-water points, farmland, woodland, sawmills,
mills and bakeries. A facility stores `source_feature` when a feature suggested
its location; `authored` remains true. Production rates, inhabitants, balances and
opening state remain authored even at a real mapped address. Historical packs
do not turn these systems into an observed historical population.

Fallback facility `model_position()` and `facility_footprints()` are shared by
the renderer and hosts. Source-linked sites defer to provider building geometry.
The reference renderer includes varied heightfield materials, bounded
ground cover, distinct animated species, depleted resource forms, visible cargo
and terrain-draped community markers. It uses procedural reference assets and
finite mesh palettes, rather than a production photorealistic asset pipeline.
See [GRAPHICS_AND_MAPS.md](GRAPHICS_AND_MAPS.md) for its cache and detail budgets.

## Authoritative hosting and persistence

The public domain entry points keep the host in charge of environment queries:

```rust
state.seed_living_demo(origin, &features, &catalog)?;
state.ensure_living_player(player_id)?;
state.register_ecology_chunk(&chunk, &catalog)?;
state.seed_ecology_near_with_environment(origin, &catalog, |position| {
    environment.ground(position)
})?;

// Decide wildlife orders before shared body movement.
state.advance_ecology_with_environment(real_dt, simulation_dt, hour, |position| {
    environment.ground(position)
})?;
state.advance_living_with_environment(real_dt, simulation_dt, &catalog,
    |kind, from, candidate| environment.resolve(kind, from, candidate))?;
// Observe completed production and maintain the funded economy afterward.
state.advance_settlements(simulation_dt, &catalog)?;
```

`environment.ground` and `environment.resolve` above are game-supplied callbacks,
not Terra methods. Return `None` for unavailable or inadmissible ground. The
reference Bevy adapter and server already wire these stages; do not advance them
a second time from a game system. Domain-only convenience methods that omit
physical checks are for hosts deliberately supplying that simpler policy.

Both reference hosts validate resident terrain, water travel mode, slopes,
source-building footprints and bridge support before committing movement. Swept
checks also include player construction and authored facility footprints. The
CLI surface cache holds at most 512 source chunks. Missing data blocks relevant
movement or placement. Games may replace memory-only environment hooks with
their physics/navigation services. Trusted `set_controlled_motion` is not a
command accepting arbitrary client positions.

Living, ecology and settlement fields are serialized inside `SandboxState`,
alongside inventories, plots, resources and factions. New layer fields have
defaults for earlier saves; hosts explicitly initialize missing demo layers.
Saved settings, references, ownership, capacities and catalog IDs are validated.
Restoring an existing layer does not respawn resources, repeat starting grants or
discard cargo/escrow. Catalog/policy changes may need an explicit migration.

The reference server executable enables the living demo by default;
`--no-living` disables initialization for a new world. The reusable `Server::bind`
API leaves demo seeding opt-in through `enable_living_demo`. Restored state stays
authoritative. Local F5/F9, server saves and session-capability files have distinct
envelopes; see [GAMEPLAY.md](GAMEPLAY.md#save-formats-and-recovery).

## Multiplayer views and practical limits

The server applies identity, replay, quantity, reach and ownership checks before
commands. Disconnect clears manual possession/velocity while preserving the
active body, wallet and outstanding owned orders. The host marks the owner
dormant, freezing owned motion/needs and transport commitments until verified
resume; dependency-safe bodies and homes may retire into cold pages. The CLI keeps
the spawn neighborhood resident for later joins.

Living sessions send sandbox views up to 5 Hz, with the principal actor pose in
the 10 Hz actor stream. A view initially contains at most 64 nearby/owned bodies,
eight shops, 16 production sites, 16 deliveries and eight road routes. Active and
avatar bodies are prioritized. Wallets and progress are recipient-specific;
private NPC pocket balances, internal paths and full private inventories are
omitted. Public delivery summaries may reveal the goods visibly being carried.

Ecology metadata follows the included bodies/resource nodes. Settlement views
include relevant claims, open contracts, the recipient's own accepted work and
the authoritative settlement elapsed time. Complete views are reduced if needed
to a 240 KiB serialized budget beneath the 256 KiB frame ceiling, with explicit
truncation. A partial replica is never a full writable world save or evidence
that omitted regions contain no entities.

Validated views reconstruct read-only replicas. These limits bound a single
process and are not MMORPG concurrency benchmarks. The TCP reference includes
optional verified TLS, bounded owner display prediction/reconciliation, durable
regional storage, operational backup/verification and trusted exact-principal
access controls. Public accounts/recovery/moderation, Internet latency/loss
qualification, distributed sharding and production capacity remain host work.
Finite real TCP/TLS churn probes demonstrate specific lifecycle properties;
they do not qualify an MMORPG service. The
[MMORPG foundation guide](MMORPG_FOUNDATION.md) explains these seams and limits.
