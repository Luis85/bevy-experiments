# Playing and building on Terra World

The native demo combines geographic streaming, a living settlement, direct actor
control and RTS orders with construction, farming and shared ownership. The same
domain commands run locally or on the authoritative reference server. The world
and its settlement are configurable building blocks for another game.

## A first session

```sh
cargo run -p terra-demo -- --camp-view --save saves/my-world.json
```

Wait for the status to report that the settlement is ready. The initial regional
view requests 49 chunks with 65 × 65 height samples each. The local player is
grounded onto loaded, walkable land and starts with **30 wood, 20 stone, 8 wheat
seeds and 250.00 credits**. Buy bread and water from the market as needed. Your
account owns five bodies: an avatar, two crew members, a utility rover and a
survey helicopter. The offline terrain, buildings and vegetation are synthetic
demonstration content. The
coordinates still refer to a real location on Earth.

Two authored starter deposits are placed near the original camp: renewable wood
and finite stone. Their labels and source status distinguish game resources
from claims about a real location's geology. Loaded world tree points also
become wood resource nodes. Resource counts persist, so returning to a chunk
does not refill a harvested node.

The right inspector groups actions into **Command**, **Build**, **Market**,
**Industry** and **Factions** tabs. **F10** opens world settings and **F11** hides
or shows the inspector. The bottom bar reports the last command result, current
controls, resident/rendered chunk counts and population. Scroll inside a panel
to reach its remaining controls. Your player ID appears in the top-right session
status and in **Factions**; use that ID when another player invites you.

### Command a crew or take control

1. Stay in **F1 RTS** mode. Click a body or its **Your crew** button. A click on an
   unowned resident inspects it; it does not grant control.
2. Drag a box over owned units to select a group. **Shift** adds to the selection;
   **Ctrl+A** selects your owned group. Right-click loaded terrain to send a move
   order. Units use small formation offsets and their available road routes.
   Aircraft RTS orders stay horizontal at their current/cruise clearance;
   possession supplies explicit altitude control. This is not a flight planner.
3. **Tab** focuses the selected body. **X** stops selected units, while **Rest
   units** cancels movement and restores biological energy.
4. Select a crew member and press **F2** for third-person possession or **F3** for
   first person. Move with **WASD**, look with right-drag, and hold **Shift** for
   the current body's sprint/speed cap. **C** changes first/third-person view.
5. **F1** or **Esc** releases direct control and returns to RTS. The selected
   body remains the active interaction context; choose your avatar explicitly
   if you want to build or trade from its location.
6. Select the survey helicopter and possess it. **R** ascends and **F** descends;
   WASD remains view-relative horizontal movement. These are kinematic controls,
   not a simulated rotor or aircraft cockpit.

A player's identity, inventory, wallet and faction permissions remain the same
when possessing another owned body. Each body has its own position and needs.
Selecting an owned unit makes it active for interaction without replacing its
order. Trade, gather and build reach follows that body, not the camera. The
reference shares inventory among owned units; separate backpacks are a game
policy you can add.

The RTS camera uses WASD to pan, the wheel to zoom, and middle-drag or **Alt +
right-drag** to rotate. **O** opens an overview. **World → Move active actor to
this region** explicitly relocates the active body in a local sandbox. GPS
presets otherwise move only the viewer and release possession. The relocation
shortcut is disabled in multiplayer.

### Buy supplies and maintain needs

1. Select your avatar or a crew member. Open **Market** or press **E**. The panel
   shows the nearest shop, distance, finite till and each item's price/stock.
2. Click **Order active actor to market** if needed. Wait for the displayed
   distance to fall within 20 m, or possess the actor and walk there.
3. Leave quantity at one and click **Buy** under bread or water. Buying removes
   that quantity from shop stock and debits your wallet. The quantity button
   cycles one, five and ten items.
4. Use **Eat bread** or **Drink water** when the corresponding need has fallen.
   One item is consumed from shared inventory; a fully satisfied need rejects
   unnecessary consumption. Vehicles cannot eat or drink.
5. Sell gathered wood or crafted planks through the same panel. Selling credits
   your wallet and debits the shop's till. The shop cannot pay more than it owns.

**Food**, **Water** and **Energy** bars belong to the selected biological body.
Low needs reduce its movement speed. Food and water follow the simulation clock;
physical exertion and rest affect energy. This sample does not kill characters
from starvation. Rest is an explicit unit order, and neither a meal nor resting
creates currency.

### Watch production and delivery

Open **Industry** and cycle **Next facility**. It shows inputs, outputs, stored
goods, cycle progress, owner and running/paused state. Seven authored sites
connect these chains:

- Well + seed store → field → grain mill → bakery → market.
- Managed woodlot → sawmill → market.
- Grain also replenishes the seed store, closing the planting-input loop.

Six couriers collect real supplier stock and carry it to another facility or
shop. Cargo is removed at pickup and added at arrival, so a blocked vehicle
cannot complete a delivery. Residents buy and consume supplies using finite
pocket money; eight traffic vehicles patrol road routes. The fallback settlement
also includes 24 residents and four animals. Short imported road networks can
reduce traffic population.

The first player owns the demonstration producers. **Order actor here** brings
its active body near the selected site's service point; **Pause / resume owned
production** requires ownership and 20 m reach. New houses do not automatically
become businesses. Prices and site recipes are fixed reference rules; there are
no wages, tax, loans or player-authored factory blueprints yet. See
[LIVING_WORLD.md](LIVING_WORLD.md) for the exact production quantities and limits.

### Build your first home

1. Select your avatar, press **Tab** for a close view and open **Build**. Click
   **Claim a 40 m housing plot**. The plot is centered on your active body and
   cannot overlap any existing claim.
2. Click **Planks · 2 wood → 4 planks** three times. Each action consumes 2 wood
   and produces 4 planks, giving the 12 planks required for a house.
3. Click **Blocks · 2 stone → 1 block** four times. Each consumes 2 stone and
   produces one stone block.
4. Click **House · 12 planks + 4 blocks**. Move the cursor onto free ground inside
   your plot and within 20 metres of your active body, then click. **Esc** cancels.
5. Click **Set nearby house as home** while nearby. The HUD records
   the home ID. A home is an ownership/gameplay record in this version; interior
   rooms and respawning are game-specific additions.
6. Press **F5** to save. The status message reports success or an actionable
   rejection. **F9** restores that session from the configured save path.

The placement preview helps you aim. The authoritative command performs the
final checks: complete footprint inside permitted land, no overlap with another
player-built structure or crop, sufficient materials and 20 m three-dimensional
reach. The local Bevy host also checks loaded support, water, slope, source
buildings, facility footprints and living bodies. The placement preview uses
the same environmental admission checks as the local command. Rejection spends
no materials and creates no partial object.

### Continue the resource loop

Craft another batch of planks and place a workbench. A nearby personally owned
or faction-shared workbench enables the tool recipe. Craft a further batch of
planks for a tool and fence. Tools are inventory items; equipping tools, damage
bonuses and durability are extension points.

Gather actions choose a nearby available wood or stone node and request up to
five units. They fail if the target is beyond reach or on land you cannot alter.
The domain API supports up to 100 units in one gather command. Sourced tree nodes
hold 20 wood and regenerate one unit per 30 simulated seconds; authored deposits
can have different capacities and regeneration settings. The demo's starter wood
regenerates, while its stone is finite.

## Recipes and construction costs

These values come from [`Catalog::builtin`](../crates/terra-sandbox/src/catalog.rs).
[`catalog.example.json`](../crates/terra-sandbox/catalog.example.json) shows the
data format a host can replace.

| Recipe | Input per batch | Output per batch | Additional requirement |
|---|---|---|---|
| `planks` | 2 wood | 4 planks | None |
| `stone_blocks` | 2 stone | 1 stone block | None |
| `tool` | 2 planks + 2 stone | 1 tool | Permitted workbench within 20 m |
| `wheat_seed` | 1 wheat | 3 wheat seeds | None |
| `flour` | 2 wheat | 3 flour | Permitted workbench within 20 m |
| `bread` | 2 flour + 1 water | 4 bread | Permitted workbench within 20 m |

| Blueprint | Materials | Hit points | Footprint radius | Capability |
|---|---|---:|---:|---|
| `house` | 12 planks + 4 stone blocks | 200 | 4 m | Can be set as a home |
| `workbench` | 4 planks + 2 stone | 80 | 1 m | Enables workbench recipes |
| `fence` | 2 planks | 40 | 0.7 m | Simple constructible structure |

Crafting is immediate. Quantities use checked integer arithmetic. A failed
debit, overflow, missing workbench or invalid quantity leaves state unchanged.
Inventories remain personal to the player principal when land becomes
faction-owned; shared property does not share every member's materials. The five
bodies owned by one principal share that principal's inventory and wallet.

## Farming and time

Click **Plant wheat**, then choose free permitted ground near your player. A
planting consumes one wheat seed and creates a crop with its own progress and
moisture. Click **Harvest ripe wheat** when it is ready. Harvest removes that
crop and grants four wheat exactly once. Crafting one wheat into three seeds
lets you replant without depending on the starter inventory. A nearby workbench
also lets you turn harvested wheat into flour and bread. Buy water from the
market for the bread recipe, then consume or sell the result.

Wheat requires 180 effective growth seconds in the built-in catalog. Temperature
and moisture change the growth rate, so elapsed seconds are not a promise of a
fixed harvest time. Rain increases moisture, dry time reduces it, and unsuitable
temperatures slow or stop growth. This is an illustrative game model, not an
agronomic prediction. The domain advances crops in bounded work even when a host
advances a large simulated interval.

The demo starts with an accelerated UTC clock. **Space** pauses/resumes; **World → Time speed** cycles local speeds. Calendar acceleration affects food/water depletion, production and crop/resource
progress; actor velocity remains in physical metres per second. Advancing the
season changes the environment clock. It does not reverse inventory changes or
simulate all intervening ecology as a historical reconstruction.

In multiplayer the server owns time. The reference uses sampled deterministic
weather at the configured spawn for resource/crop advancement; a game with
regional climates should supply regional environment samples. Loading a save
resumes saved simulation time. It does not automatically grant offline growth.

## Ownership, damage and source edits

Personal claims block other players' construction, gathering, harvesting and
demolition. A claim can be 5–100 m in radius through the API; the desktop shortcut
uses 40 m. Interaction range remains 20 m even inside a larger plot, including
vertical distance, so a nearby aircraft cannot reach a target far below it.

**Damage permitted structure −25** affects the nearest structure you may alter.
Zero hit points remove it. Demolition grants no material refund. Destroying a
house clears every player's home reference to it. The domain caps each damage
action at 25; servers also enforce an action-rate limit. This is owner-controlled
demolition, not a combat/raiding system or structural fracture simulation.

Player-built structures are mutable overlay objects. Original provider chunks
remain unchanged. The API's `RemoveFeature` can hide an owned sourced **tree
point** and removes its resource node through a persistent tombstone. Reloading
the source does not resurrect it. Sourced buildings, roads, water and land-cover
polygons remain immutable in this version: a visible chunk may contain only a
fragment of a larger source geometry. Removing those safely needs a more complete
editing/ownership policy than ownership of a visible fragment.

## Factions and shared gameplay property

A faction is a player-created group with three roles. It is independent of
country borders, legal real-world land ownership and political claim datasets.

| Action on faction property | Leader | Builder | Member |
|---|:---:|:---:|:---:|
| Use a shared house as home | Yes | Yes | Yes |
| Craft using a nearby shared workbench | Yes | Yes | Yes |
| Build, plant, gather, harvest | Yes | Yes | No |
| Damage/demolish structures or remove permitted tree points | Yes | Yes | No |
| Invite players and grant/revoke Builder role | Yes | No | No |
| Transfer own personal plot into this faction | Yes | No | No |

### Share a settlement with a second player

Start a server and two clients with distinct session files:

```sh
cargo run -p terra-net --bin terra-server -- --save saves/server.json
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/alice-session.json --camp-view
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/bob-session.json --camp-view
```

1. Alice opens **Factions** and creates a named faction. Creating it makes
   Alice its Leader. The faction panel shows its ID and Alice's role.
2. Alice chooses **Invite a player by ID** and enters Bob's assigned player ID,
   shown in Bob's top-right session status or **Factions** panel. Bob must already be registered on this server.
3. Bob clicks **Accept a pending invitation**. He joins as Member; knowing a
   faction ID alone never authorizes joining.
4. Alice chooses **Grant builder role** and enters Bob's ID.
5. Near her personal claim, Alice chooses **Transfer nearest plot to faction**.
   Alice must personally own that plot and lead the selected faction. The plot,
   all its player-built structures and its planted crops transfer atomically.
6. Bob can now build or farm on that land using his own inventory. Both players
   can set the shared house as home or use the shared workbench.
7. Alice can choose **Set member role** to revoke Bob's alteration
   rights. Bob can still use shared housing and the workbench, but cannot damage
   even a faction structure he originally built.

**Select next faction** cycles your memberships. Commands apply to the selected
faction; invitations list registered player IDs, not display names. A player can
have at most eight memberships or pending invitations, and a world supports up
to 64 factions and 128 registered players.

The original creator/founder remains in each asset's `owner` field for audit.
`faction_owner` controls current shared permissions. A faction transfer is
one-way in this reference; there are no leave/kick, leadership succession,
faction-bank, war, diplomacy or territory-capture commands yet. Adding these
requires explicit rules for existing homes, plots and invitations.

The same two-player transition can be exercised without a desktop or network:

```sh
cargo run -p terra-cli -- sandbox-demo --output saves/rules-demo.json
```

## Geographic exploration and layers

| Control | Effect |
|---|---|
| F1 / F2 / F3 | RTS / third-person possession / first-person possession |
| W / A / S / D | Pan RTS camera; move possessed body relative to view |
| R / F | Raise/lower RTS camera; ascend/descend possessed air body |
| Right mouse drag | Look while possessed; pan the political atlas |
| Middle drag or Alt + right drag | Rotate RTS camera |
| Mouse wheel in world | RTS zoom; third-person follow distance |
| Click / drag box / Shift | Select actors / group / add to selection |
| Ctrl + A in RTS | Select owned group |
| Right click ground in RTS | Move selected owned group |
| X / Rest units | Stop / restore biological energy while resting |
| Shift | Faster camera movement or possessed-body sprint/speed cap |
| C / Esc | Switch first/third person / release possession |
| E | Open nearby market interaction |
| Tab / O / M | Focus selection / landscape overview / political atlas |
| F10 / F11 | Toggle world drawer / inspector |
| Mouse wheel over a side panel | Scroll that panel |
| Mouse wheel in atlas | Zoom map |
| Space | Pause/resume local simulation |
| F5 / F9 | Save/load local world |
| Esc while placing or drawing | Cancel placement/drawing |

**World → Enter GPS coordinates** accepts `latitude, longitude` in decimal degrees. The
presets jump the explorer to Munich, Zugspitze, Hamburg or Cape Town. The
streamer changes its nearby working set and frees old chunks; long-distance
jumps also rebase the local render frame. Global positions remain geographic
`f64` values rather than accumulating floating-point error in render transforms.

The **World** drawer has layer switches for terrain, water, vegetation,
buildings, roads, weather and actors. Layers can be shown or
hidden through layer controls. A display toggle does not erase a source chunk,
delete an owned building or revoke a faction permission. Host games can use
these data types independently of the supplied renderer.

## Historical worlds and political editing

Use the included explicit era examples:

```sh
cargo run -p terra-demo -- --config configs/historical-1950.json --atlas
cargo run -p terra-demo -- --config configs/historical-1980.json --camp-view
```

Both packs and their dated political fixtures are synthetic. They demonstrate
the mechanics of historical loading; they do not reproduce real 1950 or 1980
borders, buildings or land use. For an imported dataset, every pack declares its
source, license and validity interval. Missing dates or tiles produce a coverage
message, without substituting today's terrain or map features.

The source epoch fixes the initial terrain and semantic features. The simulation
clock subsequently controls day/night, seasons and which time-bounded political
features are visible. Political queries preserve overlapping countries, regions
and disputed claims as source assertions. They do not choose a geopolitical
winner or convert those claims into gameplay ownership.

In the atlas, click a location to inspect its names/status and select it. **Explore selected** returns to the local world at that coordinate. In a
local session with a political dataset loaded, **Draw a scenario territory**
lets you click boundary vertices; **Backspace** removes the last vertex,
**Enter** closes the boundary, and **Esc** cancels. An authored territory is
marked as scenario content and may link to your selected faction. It does not
grant construction rights: those still require a valid sandbox claim.

**Export political GeoJSON** writes a file beside the configured world-save
path using the `.politics.geojson` extension. Normal local saves also retain
the current political layer. Drawing is local authoring; to share a revised
scenario, export it and configure the same immutable dataset for every peer.
The reference does not stream live political-geometry edits between clients.

## Save formats and recovery

| File | Producer / consumer | Contains |
|---|---|---|
| Local world save | Desktop F5/F9 | World/source/epoch identity, clock, player, living bodies/needs/orders, money/shops/production/cargo, sandbox overlay and politics |
| Sandbox rules save | `terra-cli sandbox-demo --output`, `SandboxState::save/load` | Pure gameplay overlay, players, inventories, factions and edits |
| Server save | `terra-server --save/--load` | Authoritative world identity, actors/living economy, clock, rules/catalog, commands/resume ownership and full sandbox state |
| Client session | Desktop or CLI `--session` | Server-pinned player resume capability; not a copy of the world |
| Political GeoJSON | Atlas export / `--politics` | Dated source geometry plus authored scenario territories and provenance |

These are intentionally different envelopes. A standalone sandbox rules save
does not contain enough world identity to load directly through desktop F9.
Restore with the producer's corresponding loader. Saves are validated before
replacement and written through a same-directory temporary file and rename.
Local desktop saves are explicit: **F5** writes and **F9** reads. The `--save`
option only selects the path. It does not automatically restore at startup or
save on exit. To continue a local settlement, restart with the same configuration
and save path, wait for the terrain, then press F9. Save before closing the window.
The source/epoch identity prevents accidentally applying a settlement from one
era or base revision to another. Changing existing blueprint or crop parameters
requires a deliberate catalog migration.

In a multiplayer session, F5/F9 cannot replace the server's state with a partial
client replica. The host saves on its timer and clean shutdown. Reuse each
player's session file to recover their identity after reconnecting; a new
anonymous connection receives a new player. Session files carry the authority
to resume that player's property, so use separate files for separate players.

## Common reasons an action is rejected

| Message or symptom | What to check |
|---|---|
| Target is outside interaction range | Move the active owned body closer; moving only the camera does not change reach. Altitude difference counts. |
| Buy/sell button is disabled | Check active body reach, item stock, your balance and shop till. Quantity applies to the whole trade. |
| Possession rejected | Select an owned body. Residents and another player's units cannot be possessed. |
| Movement slows or stops | Inspect needs, rest/eat/drink, loaded terrain, obstacles and destination coverage. There is no full navmesh. |
| You do not own the target/land | Check the plot owner, selected faction and current role. Member does not grant building rights. |
| Overlap | Choose ground clear of other structures/crops and keep the full footprint within one permitted plot. |
| Not enough material | Review inventory and recipe counts; a house needs crafted planks and blocks, not raw wood and stone. |
| Workbench required | Stand within 20 m of your own or faction-shared workbench. |
| Crop not ready | Unpause/advance world time and inspect crop progress; temperature and moisture affect it. |
| Unknown/unloaded terrain | Wait for the chunk or return inside the pack's coverage; the host rejects unsupported placement. |
| Save belongs to another world/era | Restart with the matching config, seed, source revision, epoch and political dataset, or migrate intentionally. |
| No political coverage | Load a dated dataset that covers the simulation date and geographic location. |
| Public data unavailable | Inspect provider diagnostics; use an offline example or your own prepared pack, or retry a strict probe later. |

For game developers, [integration](INTEGRATION.md), [multiplayer](MULTIPLAYER.md),
[data](DATA.md) and [historical formats](HISTORY.md) document the extension seams
behind these workflows. The [validation record](VALIDATION.md) records tested
behavior and remaining environment-specific verification.
