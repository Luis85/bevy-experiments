# Playing and building on Terra World

The native demo combines geographic streaming, a living settlement, wildlife,
resource gathering, funded supply work and community territories. You can command
a crew in RTS mode or possess one owned body, then use the same inventory and
money for trade, crafting, construction and farming. The same domain commands
run locally or on the authoritative reference server. This guide describes the
reference game; its populations, yields and economy are authored game content.

## A first session

```sh
cargo run -p terra-demo
```

Choose **New world**, create your character, then choose a source, world settings
and storage mode. **Regional database** checkpoints automatically; managed
**Portable JSON** saves on F5 and clean exit. **Continue** restores the selected
managed world. [GAME_FLOW.md](GAME_FLOW.md) describes the complete flow, source
coverage and recovery behavior. For an explicit SDK-style direct launch:

```sh
cargo run -p terra-demo -- --camp-view --save saves/my-world.json
```

That direct `--save` path uses explicit F5/F9 rather than automatic startup
restore or window-close saving; the session menu also offers save-and-return or
save-and-quit. Wait for the settlement-ready status before acting. The default
frontier requests 49 chunks with 65 × 65 height samples each; selected view size
and bounded source coverage can change that neighborhood. The local player is
grounded onto loaded, walkable land and starts with **30 wood, 20 stone, 8 wheat
seeds and 250.00 credits**. Buy bread and water from the market as needed. Your
account owns five bodies: an avatar, two crew members, a utility rover and a
survey helicopter. The offline terrain, buildings and vegetation are synthetic
demonstration content. The
coordinates still refer to a real location on Earth.

Authored starter deposits include renewable timber, berries, fiber and water,
plus finite stone and ore. Loaded features suggest additional habitats and
resource positions; fish require appropriate water-edge coverage. Source tree
points also become wood nodes. The panel describes the selected node's actual
capacity and recovery. Counts persist, so returning to a chunk does not refill a
harvested deposit. These yields are not a geological or botanical survey.

The inspector opens on **Activities [J]**. Its next-step guide and supply
noticeboard give you a first objective. **Nature [N]** locates resources and
wildlife; **Territory [T]** manages community claims. **Command**, **Build**,
**Market**, **Industry** and **Factions** supply the other tools. **F10** opens
world settings and **F11** hides or shows the inspector. The bottom bar reports
the last command result, current controls, chunk counts and population. Scroll
inside a panel to reach its remaining controls. Your player ID appears in the
top-right session status and in **Factions**; use it for invitations.

A practical first objective is to accept a supply request in **Activities**, find
or buy its goods, deliver them, then found a neighborhood. You can also build a
private home first. The simulation continues while an in-world panel is open.
In RTS mode, **Space** pauses the local clock unless reassigned to another RTS
action. **World → Pause** is always available locally; **Escape** opens the
session menu and pauses local play when no edit is active. A possessed vehicle
uses Space for its default handbrake. The multiplayer host owns its clock.

For road work, **Driving [V]** shows finite passenger/freight/service offers;
**Garage [G]** covers vehicles, fuel, repairs and prepaid rentals; **Fleet [Y]**
adds hired drivers, routes, depot inventory and loaded-work recovery. These
activities use the same wallet, physical world and persistence as building and
settlement play. Follow [TRANSPORT.md](TRANSPORT.md) for personal driving and
[COMPANIES.md](COMPANIES.md) for commercial routes, payroll and recovery.

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
5. **F1** releases direct control and returns to RTS. **Escape** cancels an
   immediate edit or opens the session menu; it does not release possession. The selected
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

1. Select your avatar or a crew member. Open **Market**. **E** opens a nearby
   resource if it is closer than the shop; otherwise it opens the market. The panel
   shows the nearest shop, distance, finite till and each item's price/stock.
2. Click **Order active actor to market** if needed. Wait for the displayed
   distance to fall within 20 m, or possess the actor and walk there.
3. Leave quantity at one and click **Buy** under bread or water. Buying removes
   that quantity from shop stock and debits your wallet. The quantity button
   cycles one, five and ten items.
4. Use **Eat bread** or **Drink water** when the corresponding need has fallen.
   One item is consumed from shared inventory; a fully satisfied need rejects
   unnecessary consumption. Vehicles cannot eat or drink.
5. Sell gathered wood, berries or crafted goods through the same panel. Selling
   credits your wallet and debits the shop's till. The shop cannot pay more than
   it owns. Prices update from actual stock, so read the current quote before
   trading.

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
cannot complete a delivery. Input shortages receive priority; outgoing and
incoming reservations prevent duplicate dispatch from overcommitting stocks.
Stalled couriers try bounded detours and can return undeliverable cargo to its
supplier. The logistics panel shows routes, phases, retries and returns.

Residents buy and consume supplies using finite pocket money. Productive
settlements periodically pay eligible residents from the market's existing till,
allowing some of that money to circulate back through shops. The fallback seed
has 24 citizens and up to eight traffic vehicles; imported routes can reduce the
traffic count. Wildlife is now managed by the separate habitat system, and the
HUD reports the current authoritative population.

The first player owns the demonstration producers. **Order actor here** brings
its active body near the selected site's service point; **Pause / resume owned
production** requires ownership and 20 m reach. New houses do not automatically
become businesses. Site recipes are authored reference rules; shop prices respond
to stock around saved reference prices. NPC pay and contract rewards transfer
existing credits. General taxation, loans, a citizen labor market and
player-authored factory blueprints remain outside these production rules.
Commercial employee contracts and payroll are implemented separately in
[COMPANIES.md](COMPANIES.md). See
[LIVING_WORLD.md](LIVING_WORLD.md) for the exact production quantities and limits.

### Gather, fish and observe wildlife

Press **N** to open **Nature**. The resource selector cycles berries, wild flax
(fiber), iron ore, fish, timber, stone and water. **Next patch** selects another
registered node of that type. The panel shows your carried amount, the patch's
remaining stock, three-dimensional distance, recovery policy and required tool.
Use **Go to patch** or walk there while possessed; gather within 20 m. Each button
press requests up to five available units. A move order needs known, dry support
near the target and remains subject to obstacles and the 2 km order limit.

| Resource | Reference ecology capacity | Recovery per unit | Equipment |
|---|---:|---:|---|
| Timber / `wood` | 96 | 180 world seconds | Ordinary gathering |
| Stone | 120 | None; finite | Ordinary gathering |
| Berries | 36 | 90 world seconds | Ordinary gathering |
| Wild flax / `fiber` | 48 | 120 world seconds | Ordinary gathering |
| Fish | 24 | 900 world seconds | Fishing rod |
| Iron ore | 72 | None; finite | Tool or iron tool |
| Fresh water | 200 | 30 world seconds | Ordinary gathering |

These are the built-in **ecology** definitions. Older starter deposits, source
tree nodes and custom game resources can have different values; use the selected
patch's displayed state. Depleted nodes remain visible, and finite deposits do
not refill when you leave and return. Fishing is a stock-based gathering action
at a water-edge resource, with no casting minigame or simulated fish schools.

Gather fiber and craft planks, then use **Craft fishing rod**. Cook fish at a
permitted workbench and consume it through **Eat cooked fish**. Berries can be
eaten directly, and gathered water can be drunk. Iron processing requires a
workbench: gather ore with a tool, smelt ingots using wood, then craft an iron
tool. The iron tool also satisfies ore gathering, but does not currently add
durability, faster extraction or combat bonuses. Sell surplus resources and
crafted goods at the market or use them for a matching supply request.

**Observe next wild animal** focuses one authoritative animal. Deer, boar, hares,
foxes and ibex have distinct models and gait/idle poses, and choose grazing,
roaming, fleeing, drinking or rest from their needs, activity schedule and nearby
disturbance. Approach carefully or watch from a distance. Observing is inspect
only: the animal does not become your active body or gain your inventory, and
you cannot possess unowned wildlife. Select an owned crew member to resume
direct control. There is no hunting, taming, animal harvesting or predator combat
in this release.

### Complete funded supply work

Press **J** for **Activities** and cycle **Next supply request**. Each offer names
an item, quantity, delivery destination, reserved credit reward, community XP and
remaining world-time window. Requests are generated from actual producer input
shortages and retail stock, with at most 16 open/accepted requests in the world.
The market pays the reward into escrow when the offer is posted.

1. Choose a request you can supply and click **Accept this contract**. You can
   accept up to three at once. Acceptance refreshes the deadline; the default
   window is 72 world hours, affected by the host's calendar speed.
2. Gather, craft or buy the complete quantity. Compare purchase costs with the
   offered reward; a retail replenishment request is not a promise of profit if
   you buy every ingredient at retail prices.
3. Click **Order active actor to destination** or travel there under possession.
   The request's displayed distance is measured from your active owned body.
4. Within 20 m, click **Deliver goods and collect reward**. The command removes
   the goods from shared inventory, adds them to the named stock, pays the
   reserved credits and grants XP atomically. A full destination rejects the
   transaction without consuming your goods.

**Return accepted contract** makes the same funded request public again. Its
reward stays in escrow. Expired accepted work returns to the public noticeboard
with a refreshed window during economy maintenance; your unsubmitted inventory
is unaffected. A fulfilled request disappears exactly once. Empty boards can
mean stock targets are met or the issuer lacks funds. Continue gathering,
building, farming or trading while the simulation advances.

Community ranks are Newcomer, Contributor (50 XP), Steward (250 XP) and Pathfinder
(1,000 XP). XP belongs to the player principal and persists with the world; it
does not change wildlife population or represent a factual social statistic.

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
player-built structure, crop or occupied living body, sufficient materials and
20 m three-dimensional reach. Living-body admission uses the same actual
registered vehicle dimensions and reference human/animal shapes as movement,
including orientation and vertical separation. It protects rented and company
vehicles from construction through their noses or tails. The local Bevy host
also checks loaded support, water, slope, source buildings and facility footprints. The placement preview uses
the same environmental admission checks as the local command. Rejection spends
no materials and creates no partial object.

### Continue the resource loop

Craft another batch of planks and place a workbench. A nearby personally owned
or faction-shared workbench enables the tool recipe. Craft a further batch of
planks for a tool and fence. Tools are inventory items; equipping tools, damage
bonuses and durability are extension points. The ecology system checks tool
ownership for ore extraction and fishing rods for fish gathering.

Quick Gather actions choose the nearest available wood or stone node within
three-dimensional reach, skipping protected plots and territories. They request
up to five units. The authority rechecks current rights and quantity before
transferring anything.
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
| `fishing_rod` | 2 planks + 4 fiber | 1 fishing rod | None |
| `cooked_fish` | 2 fish + 1 wood | 2 cooked fish | Permitted workbench within 20 m |
| `iron_ingot` | 3 iron ore + 2 wood | 1 iron ingot | Permitted workbench within 20 m |
| `iron_tool` | 2 iron ingots + 2 planks | 1 iron tool | Permitted workbench within 20 m |

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

The default demo starts with an accelerated UTC clock; world setup can choose
another rate. **World → Pause** toggles the local clock, and **World → Time speed**
cycles local speeds. Space is the default RTS pause shortcut only when no
configured RTS action uses it; in possession it remains a body control. Calendar
acceleration affects food/water depletion, production and crop/resource
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
3. Bob checks the named invitation in **Invitations to you**, then clicks
   **Accept shown invitation**. He joins as Member; knowing a faction ID alone
   never authorizes joining.
4. Alice chooses **Grant builder role** and enters Bob's ID.
5. Within 20 m of her personal claim's center, Alice chooses **Review sharing
   nearest plot** and reads the named plot and faction before choosing **Confirm
   community change**.
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

The **People and access** roster lists player IDs and current roles, with eight
entries per page. **Invitations to you** names the community and leader before
you accept or decline; use **Choose next invitation** when several are pending.
Only the current leader can invite, revoke invitations, grant roles, remove
members or hand over leadership. The panel explains your role and disables
management actions that require a different role.

### Leaving, removing members and handing over leadership

| Action in Factions | Who can act | Result |
|---|---|---|
| Review leaving this faction | A Member or Builder | Your membership ends. Rejoining requires a new invitation and starts you as a Member. |
| Review removing a member | Current Leader, targeting another member's player ID | That member's access ends, including while they are offline. |
| Review leadership handover | Current Leader, targeting another existing member's player ID | The target becomes the only Leader. The outgoing leader stays as a Builder and may then leave. |
| Review dissolving unused faction | Current Leader and sole member, with no shared land or settlement | Permanently removes the unused faction and revokes its outstanding invitations. No assets or funds move; IDs are never reused. |
| Revoke an invitation by ID | Current Leader | The unaccepted invitation ends and frees the invitee's affiliation slot. |
| Decline shown invitation | The invited player | The invitation ends without joining; no extra confirmation is needed. |

Leaving, removing a member, handing over leadership, dissolving and sharing a plot first
open a review naming the exact community and target. Confirm or cancel that
review at the top of the panel. A different player, restored world, changed
selection, changed roster or changed property ownership invalidates the review.
Confirmation is bound to the exact displayed review. A queued click from an
earlier canceled review cannot confirm a replacement, even for the same faction;
the replacement must be displayed and confirmed separately. Cancel appears
before Confirm, ahead of the ordinary faction summary.
The host checks current authority again when it receives the command.

A retained faction always has one leader. Leaders cannot leave or remove themselves
until they transfer leadership to an existing member, including a member who
is currently offline. A sole leader can instead review dissolution while the
faction has no shared plot or settlement territory. The review names the faction
and the number of invitations that will be revoked. Confirming frees those
affiliation slots; Cancel keeps the faction. It never refunds contributions,
deletes property or creates a leaderless community. A replacement gets a new ID.

A shared plot blocks dissolution even if no house has been built. A settlement
blocks it even with zero treasury and no remaining project stock. Far-away and
dormant property count: a multiplayer client's nearby view cannot prove that the
faction has no assets, so the authority checks its complete ownership records
again. A changed roster, invitation list, restore or newly shared property
requires a fresh review or produces an authoritative refusal without mutation.

There is no automatic succession after inactivity, dissolution of a community
with other members or shared assets, or takeover by other members. Arrange a
handover before departing an established community; an offline successor resumes
with the saved leadership, and the outgoing Builder can keep operating public
works until choosing to leave.

When membership ends, a home in that faction is unset, including a home whose
building is in a dormant region. Personal homes and homes in other factions
remain. Bodies keep their saved position; membership changes do not move them.
Personal inventory, money, property and other memberships remain yours. Shared
plots, buildings, crops, territories, public stock and treasury stay with the
faction. Contributions are never refunded by departure or leadership change.

The original creator/founder remains in each asset's `owner` field for audit.
`faction_owner` controls current shared permissions; a departed founder has no
special access. Sharing personal property is still a one-way transfer. The
community treasury below remains a restricted public-works balance with no
general withdrawal. War, diplomacy, elections, territory capture and later
territory ownership transfers remain outside the reference rules. Current
release, wire and save compatibility are recorded in [COMPATIBILITY.md](COMPATIBILITY.md).

The same two-player transition can be exercised without a desktop or network:

```sh
cargo run -p terra-cli -- sandbox-demo --output saves/rules-demo.json
```

## Found a neighborhood and grow a community

Gameplay territories are named, circular claims created through **Territory [T]**.
They reserve a community area and regulate who may claim plots, construct,
plant and gather there. The existing housing plots remain necessary for placing
houses, workbenches and crops; a territory does not automatically construct a
town. These gameplay regions are separate from imported country borders and
authored political-atlas polygons.

1. Select your owned avatar or crew member and stand on loaded, dry terrain with
   room for a 100 m claim. Press **T**. The default owner is you personally.
2. To create shared community land, first select your faction in **Factions**,
   then change the territory owner to that faction. You need a current Leader
   or Builder role. Choose this before founding: there is no later territory
   ownership-transfer command.
3. Click **Name and found here**, enter a name of 1–48 printable characters and
   press Enter. Founding consumes 20 wood and 10 stone and transfers 50.00 credits
   from your wallet into the new treasury. A rejected claim spends nothing.
4. Use **Build** to claim housing plots inside the community. New plots in a
   faction territory inherit that faction's ownership and current roles.
   Personal inventory still pays for construction; public stock is separate.
5. Complete supply requests for XP, construct housing, and plant crops. Return
   within **20 m of the territory center** to contribute materials, fund the
   treasury or upgrade. **Go to center** orders your active body there.

The panel lists the next charter's materials, treasury, XP and infrastructure
requirements. **Contribute up to 5 materials** transfers the selected item from
your inventory to the territory's public stock. **Fund treasury with 25 credits**
transfers 25.00 from your wallet. These transfers have no withdrawal action in
the reference game, so contribute what the next charter needs.

| Charter reached | Radius | Plot capacity | Materials consumed | Treasury cost | Community XP | Infrastructure inside the current claim |
|---|---:|---:|---|---:|---:|---|
| Neighborhood | 100 m | 6 | 20 wood + 10 stone from founder | 50.00 cr deposited, not spent | 0 | None |
| Village | 300 m | 24 | 30 planks + 10 stone blocks from public stock | 25.00 cr | 50 | 2 houses |
| City | 700 m | 64 | 120 planks + 50 stone blocks + 8 tools from public stock | 150.00 cr | 250 | 6 houses + 3 planted farm plots |

Upgrade XP is the acting player's accumulated community XP and is not spent.
Houses must be permitted housing structures inside the **current** boundary;
farm counts are currently planted player crop plots, not the authored production
field. Harvesting removes a crop, so replant before upgrading if you need it to
count. Materials are consumed by public works, and the charter payment transfers
from treasury to a market. Each upgrade expands the boundary after validating
the entire new claim against existing property.

Claims never overlap another community and cannot enclose someone else's prior
personal/faction plots. A shared territory also rejects a personal plot crossing
its boundary. Current faction roles are checked on every action: demoting a
founder does not preserve a hidden ownership bypass. Members may use shared
housing/workbenches and gather in unplotted shared territory, while work-plot
gathering, construction, contributions and upgrades require the corresponding
Builder/Leader permissions. A visitor may travel through territory; these claims
are permission regions, not physical walls or legal assertions.

The reference supports at most 32 territories, with four founded by one player.
Dashed terrain-following borders and tier pennants show claims in the world;
the Territory panel supplies the authoritative name, owner and radius. Boundary
visuals use loaded ground and the Buildings layer. There is no conquest,
automatic population migration, taxation, municipal election or urban-layout
generation when a charter changes.

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
| C / F1 | Switch first/third person / release possession into RTS |
| Esc | Cancel immediate editing/placement, otherwise open the session menu |
| E | Open a nearby resource or market context |
| J / N / T | Activities and supply work / Nature / Territory |
| V / G / Y | Driving work / Garage and rentals / Fleet management |
| Tab / O / M | Focus selection / landscape overview / political atlas |
| F10 / F11 | Toggle world drawer / inspector |
| Mouse wheel over a side panel | Scroll that panel |
| Mouse wheel in atlas | Zoom map |
| Space in RTS | Pause/resume local clock unless another RTS action uses that binding |
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
| Local JSON snapshot | Desktop F5/F9; managed portable worlds also save on clean exit and restore through Continue | World/source/epoch identity, clock, player, bodies/needs/orders, economy/cargo, ecology, resources, territory/treasury/contracts/XP, overlay and politics |
| Regional database | Menu regional worlds or `--region-store`; checkpoints and transactional restore | Shared ledgers plus geographic mutable pages, dormant owner/body dependencies, clock and host metadata |
| Sandbox rules save | `terra-cli sandbox-demo --output`, `SandboxState::save/load` | Pure gameplay state including enabled living, ecology and settlement layers, players, inventories, factions and edits |
| Server save | `terra-server --save/--load` | Authoritative world identity, actors, living economy/ecology/territories, clock, rules/catalog, commands/resume ownership and full sandbox state |
| Client session | Desktop or CLI `--session` | Server-pinned player resume capability; not a copy of the world |
| Political GeoJSON | Atlas export / `--politics` | Dated source geometry plus authored scenario territories and provenance |

These are intentionally different envelopes. A standalone sandbox rules save
does not contain enough world identity to load directly through desktop F9.
Restore with the producer's corresponding loader. Saves are validated before
replacement and written through a same-directory temporary file and rename.
Managed menu worlds resume through **Continue** or **Your worlds**. Regional
storage checkpoints on cadence, source residency changes and clean exit;
portable JSON saves on F5, session-menu save and clean exit. **F9** restores the
latest successful checkpoint/snapshot and discards later changes. A storage
failure keeps the session open with retry/restore feedback.

For an unmanaged direct launch, `--save FILE` only selects the explicit JSON
path. Restart with the same configuration and path, wait for terrain, then use
F9 to restore. F5 and the session-menu save actions write it; simply closing an
unmanaged window does not imply a JSON save. `--region-store FILE` instead
creates/resumes the transactional regional lifecycle directly. See
[GAME_FLOW.md](GAME_FLOW.md) and [REGIONAL_PERSISTENCE.md](REGIONAL_PERSISTENCE.md).
Source/epoch identity prevents applying a settlement from another era or base
revision. Changing existing blueprint or crop parameters requires a deliberate
catalog migration.

In a multiplayer session, F5/F9 cannot replace the server's state with a partial
client replica. The host saves on its timer and clean shutdown. Reuse each
player's session file to recover their identity after reconnecting; a new
anonymous connection receives a new player only when the host permits new
registration. A resume-only host requires an existing valid unsuspended
capability. Session files carry the authority
to resume that player's property, so use separate files for separate players.

## Common reasons an action is rejected

| Message or symptom | What to check |
|---|---|
| Target is outside interaction range | Move the active owned body closer; moving only the camera does not change reach. Altitude difference counts. |
| Buy/sell button is disabled | Check active body reach, item stock, your balance and shop till. Quantity applies to the whole trade. |
| Gather is disabled | Check the selected patch's remaining stock, tool requirement, 20 m reach and plot/territory permission. Try another patch when a finite deposit is depleted. |
| No fish patch appears | The loaded region may have no usable water-edge coverage. Explore a suitable region; the demo does not invent an observed fish population. |
| Deliver contract is disabled | Accept the request first, carry its full quantity, and move your active body within 20 m of the destination. Check its world-time deadline. |
| Territory upgrade is disabled | Return within 20 m of its center and review public stock, treasury, your XP and current houses/farms. Carried materials are not already contributed. |
| Territory would cover another owner's property | Choose a different center or expansion area. A larger charter cannot confiscate an existing plot or overlap another territory. |
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

For game developers, [integration](INTEGRATION.md), the
[MMORPG foundation guide](MMORPG_FOUNDATION.md), [multiplayer](MULTIPLAYER.md),
[data](DATA.md) and [historical formats](HISTORY.md) document the extension seams
behind these workflows. The [validation record](VALIDATION.md) records tested
behavior and remaining environment-specific verification.

## Management by keyboard and controller

F6 or right-stick click opens management focus. Arrows, Tab or D-pad navigate
enabled controls; Enter/South activates; PageUp/PageDown or shoulders changes
inspector pages. Selected actions scroll into view. Names and coordinates accept
normal typing or the bounded on-screen keyboard. Enter/Start applies and
Escape/East discards. Exiting focus captures that input so an actor or atlas
draft cannot act on it behind the panel.

In RTS, the controller's left stick moves a visible terrain cursor. A selects or
drag-selects, RB adds selection, X orders movement, B stops/cancels placement,
Y possesses and LB cycles actors. Right stick pans and triggers zoom. Construction
uses the same cursor and authority checks. Atlas drawing remains mouse/keyboard.
See [CONTROLS.md](CONTROLS.md) for the complete mappings and hardware boundaries.
