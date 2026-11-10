# The coastal world

Normal entry selects `living_world`: a finite 256×256 metre island with Harbour,
Alderwood and Stone Ridge, 42 residents, distinct homes and workplaces, regional
terrain and a shared physical economy. It extends the one-window flow described
in [Island life](island-life.md). Survival and permanent death still apply.

At startup, the title shows **Preparing the island** until the terrain and its
render pipelines are ready. World-entry controls stay disabled during preparation;
How to play and Quit remain available. Preparation does not advance the world or
write a save. If preparation fails, the title offers a clear restart message.

## Explore and participate

Open **Explore** for the published settlement chart. Destinations are authored public
information; the live marker is your character. It does not reveal unseen people,
stock or private property. Travel uses ordinary walking. Carry food, water and warm
clothing on longer journeys.

**Craft** shows discovered sources and workshops. Fishing requires a real rod.
Make one at a carpenter's workshop from one plank and two thread, or buy one.
The rod is retained when catching fish. Two fish, one water and one log produce
two cooked meals at a bakery; raw fish cannot be eaten. Tailors can weave a warm
coat from eight thread and four fiber, a renewable alternative to leather clothing.

Harvesting depletes stocks and disturbs habitats. Renewable sites recover gradually;
ore does not regrow. Approach a disturbed site and open **Explore** to restore it.
Planting a tree cutting uses one log and one water; habitat restoration uses two
fiber and one water. Work takes 240 ticks. Ingredients and site are reserved, then
consumed only on completion. Stop releases unspent materials. Growth matures later;
restoration does not instantly refill stock. Wildlife uses a breeding refuge and
habitat-dependent capacity instead of a daily population reset.

Building, farming, hunting, trade and neighbours retain shared physical actions.
Housing and garden plots are distributed across the world. Colony commissions
require actual procurement and construction.

## Oversee and shape

Colony **Overview** offers settlement camera destinations. Select a resident under
**Residents** to inspect qualifications, assignment, reason, cooldown and pending
work. Requests are checked against capacities, qualifications and tools. An assignment
waits for a safe work boundary; it does not teleport the person, grant equipment or
erase their current batch. Automatic reconsideration responds to shortages and
disruptions, with essential coverage and cooldowns preventing constant switching.

Terraform uses the same recorded command on the larger heightfield. Dry elevations
range from 0 to 12 metres, water from −2 to 0, and brush radii from 0 to 8 metres.
Homes, workplaces, actors, access and connectivity constrain edits. Navigation is
on a ground heightfield, with no stacked floors or navigable tunnels.

## Saves and authoring

Normal play uses `living-world.world.json` in the configured agents folder and
`living_world.json` in its worlds folder. The old `local-island.world.json` is
preserved. Open it explicitly with:

```powershell
.\target\debug\game_menu.exe --front-end --island-save "C:\path\local-island.world.json"
```

Continue reconstructs the saved immutable profile. New Game in that loaded session
uses the same profile. Editing preset files does not migrate existing saves.
Explicit lower-level legacy constructors remain legacy; select living-world rules
when creating a regional world.

The preset declares dimensions, arrival, settlements, homes and sites. Separate
content contexts own mechanics, as documented in [ADR 0037](adr/0037-regional-living-world.md).
Reference prices and the community fund are shared world institutions. There are
no separate village currencies or hidden deliveries. Research and test results
remain separate evidence; a larger world alone does not prove indefinite survival.

## Validation scope

The [validation record](_notes/2026-11-09-living-world-validation.md) contains the
latest seed-7 audit: all 42 residents remained alive and housed for 336,000 ticks
(14 days through winter into spring), conserving 8,400 coins. Native confirmation
includes three real village journeys and 54 captures at both supported test sizes.
Physical input and GPU/FPS performance targets were not verified; a full year or
indefinite survival is not established. Six additional inspected images confirm
the shading and editor diagnostics corrections. The
[startup follow-up](_notes/2026-11-10-terrain-startup-polish.md) confirms preparation
gating, coherent terrain reveal and water/land chunk updates without empty-mesh
warnings. Its full gate returned `ok: true`, including 1,960 passing tests and all
architecture, replay, snapshot and dependency-policy stages. That record retains
skipped/ignored counts and limitations.
