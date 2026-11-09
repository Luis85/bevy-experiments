# Life on Island Village

Island Village is a 3D farming and village simulation. Twelve residents keep their
own food, materials, tools and clothing economy running while you find supplies,
make a home and tend a garden. Your character still needs food, water, rest and
warmth; death is permanent.

## Start and continue

Run these commands from `npc-framework/`:

```powershell
cargo build --locked -p game_menu -p actor_testbed -p npcctl
.\target\debug\game_menu.exe
```

`cargo run --locked -p game_menu` also starts the game. The title, character setup,
island, pause menu and return to title share one window and process.

- **New game** opens **A new beginning**. Enter your character's name, then choose
  **Start on the island**. Names can contain up to 32 characters and cannot reuse
  a resident or reserved world name.
- **Oversee the village** in setup starts a new island in inspector mode without
  beginning the visitor's life. When a save is available, the title also offers
  **Oversee the village** to inspect it directly. Choose **Play character** in the
  colony ledger when you want to arrive.
- **Continue** resumes the current compatible local island. It shows the saved
  character, day and season. Continue is unavailable when there is no usable save.
- **How to play** explains the basic controls. **Back** returns to the previous menu.

The title and setup screens pause the simulation. Opening the application does not
begin a visitor's life or overwrite the saved island. New game keeps a numbered
backup when a save already exists; it uses the world's loaded immutable rules.
Continue preserves the existing world, possessions, crops and construction.

## Find your feet

The top strip shows the day, season, daylight, simulation speed, coins and your
health, food, water, energy and warmth. Low meters need attention. **Your first homestead** suggests a
useful next action based on what you actually know and carry. Urgent needs take
priority over building. This guidance grants no materials or rewards.

Use the bottom toolbar to open one workspace at a time:

| Workspace | What you can do | Key |
|---|---|---|
| **Pouch** | Inspect possessions, eat, drink, wear warm clothes, rest | I |
| **Craft** | Gather from discovered sources and use workshop recipes | C |
| **Build** | Inspect a house site, claim it and build a cottage | B |
| **Garden** | Select a plot, crop and individual bed; plant, water and harvest | G |
| **Trade** | Buy or sell nearby goods and offer your surplus | T |
| **People** | Select a known neighbour, greet them and ask for advice | N |
| **Explore** | Walk in a world direction or return to a known place | M |

Walk close to resources, workshops, people and plots to discover them. Actions use
your character's knowledge. A remembered source may have changed since you last
visited; the action checks availability again on arrival. If a neighbour moves
away before you reach them, select them again to approach their latest known place.
**Explore** stays available after your first discovery. Its short walks are six
metres north, east, south or west; its destination list includes your home and
places you have learned. Discovered fixed places remain known after short-term
perception expires, but their unseen stock and occupancy are not revealed.

The workspace shows actual ingredient counts, costs and availability. Work takes
simulation time: a higher speed shortens the real wait without changing the amount
of work. A gathering action collects **one item**; repeat it when you need
more. **Stop / X** cancels your current movement or work. Reserved but unspent
construction materials remain yours.
Feedback distinguishes a requested action from a completed one. A visible success
describes an observed outcome; requesting an action is not a grant of its result.

## Build your first cottage

1. Gather clean water at the well and food, such as berries, before a long job.
   Use them from **Pouch** when needed.
2. Explore the house lane south of the village. Open **Build**, select a discovered
   house site and use **Walk to this site** if its ownership is not yet known.
3. Choose **Claim this site** on an available plot. You may own one house site and
   one garden plot. Claiming land supplies no building materials.
4. Obtain **8 planks and 4 logs**. With the shipped recipes, six gathered logs are
   enough: use two logs in two carpenter workshop actions, producing four planks
   each, and keep the other four logs. Buying materials is also possible.
5. Return to **Build** and choose **Build this cottage**. Your character walks to
   the door, reserves real materials and completes the work. The shipped cottage
   takes 600 simulation ticks, about 30 seconds at normal speed.

After selecting a site, its requirements and work controls appear first. **Change
site** returns to the selector. Garden uses the same selected-site pattern so its
bed state remains close to its actions.

The translucent cottage shows the fixed building footprint on the selected plot.
Green means the observed ownership permits the site, red means it is occupied,
and amber means you must visit to inspect it. This is a preview, not free placement;
materials and action validation still determine whether construction can begin.

When the cottage is ready, **Go home and rest** uses its door and **Tend your garden**
opens the adjoining beds. The pause menu preserves construction progress and its
material reservations, including across **Save and title** and Continue.

In cold weather, bring one unreserved log within 1.5 metres of your household door.
The unlit hearth consumes it automatically at the next household check and heats
the home for half a simulation day. **Pouch** shows your own home's heat and fuel
state beside the rest controls. Opening the panel does not light the fire or take
fuel. A cottage provides shelter; continued heating still needs real logs.

## Grow and trade

Open **Garden**, select your plot, choose a crop and select a bed. **Plant … here**
uses one carried seed item; **Water this bed** uses one clean water. Growth starts
after watering. The bed reports its stage and remaining time.

| Crop | Seed | Water | Growth at normal speed | Harvest |
|---|---|---|---|---|
| Grain | 1 grain | 1 clean water | 6,000 ticks / 5 minutes | 3 grain |
| Hardy berries | 1 berries | 1 clean water | 9,000 ticks / 7½ minutes | 4 berries |

Choose **Harvest this bed** when it is ready. Harvested goods enter your pouch.
Grain can feed the village's processing chain; berries can be eaten or sold. The
berry crop is available in the Island Colony rules; historical homestead scenarios
keep their grain-only catalog. Growth pauses with the world.
The homestead guidance acknowledges beds already planted or growing, and directs
you to watering or a ready harvest before suggesting another empty bed.

Walk near a merchant and open **Trade** to see real offers and unit prices. Each
button trades one item, subject to current stock, money and reach. Direct
**Buy/Sell** buttons carry the displayed price limit. **Offer … for sale** lists
one carried item at the then-current market price, which can change while it is
listed. It earns coins only when a resident actually buys it.

In **People**, choose a known resident by name and profession and **Say hello**, or ask about a home,
gardening, water or trade. Conversations happen face to face. Familiarity reflects
your known relationship with that resident; the village's ordinary social life
continues around you. The selected conversation appears first; **Change neighbour**
returns to the resident list. Nearby animals are not conversation partners. A
leisure meeting needs available participants: sleeping, committed work and urgent
needs take priority over the proximity-based familiarity gain.

## Pause, save and change modes

**Menu** pauses the whole village. **Esc** first closes an open workspace, then
opens the pause menu. Choose **Resume**, **Save game**, **Save and title** or
**Quit game**. The last two save before leaving play. A save failure keeps the
application open with a recoverable error.

The local save is `local-island.world.json` in the configured agents folder. During
play it autosaves every minute; **Ctrl+S** or **F9** saves immediately. A second
window cannot own the same save. Backups use `.previous-N.json` and are never
overwritten. Save data includes the world's rules; editing a preset does not change
an existing world.

The pause menu's **Inspect village** opens colony management; **Landscape tools**
opens terrain editing. Both use the same living island. **F11** also switches from
character play to the colony tools. Choose **Play character** in the colony ledger
to return. Inspecting does not freeze an existing character's needs; use **Menu**
to pause the world. See [Colony management and terrain editing](island-colony.md).
Switching to inspector or editor preserves committed work. The character's current
activity finishes under its existing authority before native control detaches;
returning to character mode during the activity continues it. **Stop** remains the
explicit cancellation action. Survival interruption and death keep their ordinary
rules.

If your character dies, **Begin a successor life** starts a new mortal person in the
same village. It does not revive the old character or reset the economy. The world,
homes, possessions and diaries persist; they are not automatically handed to the
successor. Resume the simulation before beginning the next life.
Alternatively, choose **Keep watching the village** to dismiss the death panel and
inspect the same world. **Life choices · L** can reopen the successor
options; watching never revives or replaces the dead character.

## Controls

| Input | Character play |
|---|---|
| WASD | Walk relative to the camera |
| Right-click ground | Walk to the selected place |
| E | Use the displayed nearby action |
| Q / R | Orbit the camera |
| Mouse wheel | Zoom; scroll when over a workspace |
| I / C / B / G / T / N / M | Open or close the corresponding workspace |
| F2 | Open Pouch, or close the current workspace |
| Tab / Shift+Tab | Open Pouch when closed; navigate controls when open |
| Enter / Space | Activate a focused control |
| X | Stop movement or work |
| Esc | Close workspace, then pause; resume from the pause menu |
| Ctrl+S / F9 | Save during play |
| L while watching after death | Reopen life and successor choices |

Click the world or close the workspace to return keyboard focus to movement. In
inspector/editor mode, WASD or arrows pan, Q/E orbit, and the wheel zooms. Space
pauses, `-` / `=` changes speed, and `.` steps a paused simulation. The pause menu
also works in these modes.

## Developer entry points

The old launcher toolbox remains explicit:

```powershell
cargo run --locked -p game_menu -- --developer-menu
```

It provides Settings, world-rule editing, actor editing and separately saved AI
explorers. For a separate local playtest, use a different save path:

```powershell
.\target\debug\game_menu.exe --front-end --island-save C:/NPC/playtest.world.json
```

Add `--island-rules C:/NPC/worlds/island_colony.json` when creating a new world from
edited rules. Use a new save path to adopt those rules. Direct `actor_testbed`
scenario commands remain available for development without this player frontend.
See [NPC AI authoring](island-ai-authoring.md) and
[architecture ownership](architecture-island.md).

This increment provides the connected title-to-home-and-garden loop. It retains
the existing 3D style and survival rules. It does not add an appearance creator,
marriage, festivals, animal husbandry or a general furniture-placement system.
Terrain undo, an audio soundscape and arbitrary behavior-tree graph editing are
also separate work. The current NPC authoring interface edits validated routine
priorities, thresholds and planner settings; it does not edit arbitrary tree topology.

The final improvement direction is documented in the
[experience](research/2026-11-09-experience-research.md),
[simulation](research/2026-11-09-simulation-research.md) and
[implementation](research/2026-11-09-implementation-research.md) research briefs.
Those briefs distinguish source evidence and proposals from implementation;
[the validation record](_notes/2026-11-09-island-player-flow-validation.md) records
which checks were actually performed.
