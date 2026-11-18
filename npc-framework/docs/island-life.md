# Life on Island Village

Island Village is a 3D farming and village simulation. Forty-two residents in three settlements keep their
own food, materials, tools and clothing economy running while you find supplies,
make a home and tend a garden. Your character still needs food, water, rest and
warmth; death is permanent.

## Start and continue

Run these commands from `npc-framework/`:

```powershell
cargo build --locked -p island_game
.\target\debug\island_game.exe
```

`cargo run --locked -p island_game` also starts the game. The title, character setup,
island, pause menu and return to title share one window and process.

- **New game** opens **A new beginning**. Enter your character's name (up to 32
  characters; it cannot reuse a resident's or a reserved world name), then choose
  how to begin. Each choice says what it is in one sentence:
  - **Live**: start on the island as a traveller.
  - **Oversee**: watch and guide the villages first, without beginning the
    visitor's life. Choose **Live** in the bar at the top (or press **1**) when
    you want to arrive.
  - **Shape**: the same, with the terrain tools in hand.
- **Continue** leads the title when there is a usable save, with the saved
  character, season and day on its second line; without one it is absent and
  **New game** leads. With a save the title also offers **Oversee the village**.
  The island says "Island loaded" when it opens.
- **How to play** opens the **Guide**: five short pages (Controls, Oversee, Live,
  Shape, The economy). **Back** or **Esc** returns to the previous menu.
- **Settings** (title and pause menu) has four tabs:
  - **Sound**: master, ambience and effects volume, and Mute.
  - **Display**: the interface size (Auto follows the window, Fixed does not;
    80–140% either way) and windowed or borderless display.
  - **Controls**: camera pan speed (50–200%), whether the pointer at a window
    edge pans the Oversee and Shape camera (**Edge pan**), and the list of keys.
    Keys cannot be changed yet.
  - **Accessibility**: Reduce motion, the tooltip delay (Short, Normal, Long),
    **Show tips** with a button that shows the first-run tips again, and the
    panel style (Parchment or Dusk).

  A slider can be dragged, stepped with the buttons at its ends, or moved with
  the arrow keys when it has the focus. Changes apply at once and are kept in
  `island-settings.json` beside the island save. It is host data, never part of
  the world; a missing or damaged file falls back to defaults field by field.
- The first time you enter a mode, a small **tip** stands beside what it
  explains: four in Oversee (the totals, the build dock, clicking a resident,
  the alerts), one in Live and one in Shape. **Got it** dismisses a tip for good.
  Nothing pauses and nothing is blocked. **Show tips** in Settings switches them
  (and the hint under your Live goal) off; the button under it brings them back.

The title and setup screens pause the simulation. Opening the application does not
begin a visitor's life or overwrite the saved island. New game keeps a numbered
backup when a save already exists; it uses the world's loaded immutable rules.
Continue preserves the existing world, possessions, crops and construction.

## Find your feet

The bar across the top is the same in every mode: **Menu**, the three ways to
play (**Live**, **Oversee**, **Shape**, keys 1, 2 and 3, with the active one lit),
today's weather where the world has weather, the date and time, and the clock
(pause on **Space**, **1×**, **3×**, **10×**). Keys are shown as small caps beside
their controls and in tooltips, not written into labels; while paused, a **Paused**
pill shows under the bar. The interface size follows the window unless
**Settings** says otherwise (see [`island-ui.md`](island-ui.md)). Select the day and time to open the **Calendar**: the season and the day
within it, how long until the next season, sunrise and sunset, and what this
season and the next mean for growth and cold, and, where the world has them,
tomorrow's weather and the supply boat. It only shows; **Esc**, a second
click or a click on the world closes it.

Live adds four things to the island, and nothing else stays on screen:

- **The goal**, top left. The journey's name, the next useful step as a title
  and a sentence, its progress where the step has a count, and one button that
  names what it does ("Open Build", "Go and do it"). It is based on what you
  actually know and carry; urgent needs take priority over building, and it
  grants no materials or rewards. The arrow in its corner folds it to its
  title; it also folds by itself while a workspace is open.
- **Your needs**, bottom left: Health, Food, Water, Energy and Warmth as an
  icon, a word and a bar, then your coins. A bar below 45% turns amber and
  gains a triangle; below 20% it turns red and gains a diamond. The exact
  percentage is in the tooltip. Selecting a need opens the Pouch.
- **The hotbar**, bottom centre: the seven workspaces and **Stop**, each with
  its key cap. The open workspace's tile is lit; its tile or key closes it
  again.
- **The prompt**, above the hotbar, only while there is something to say. With
  something in reach it is a key cap and what the key does ("E Gather Water",
  "E Talk to Ada"). While your character walks or works it says so ("Walking,
  then: gather one item") with **Stop** beside it. Nothing in reach and nothing
  under way: no box. Beside a two-pane workspace it moves above your needs.

Short notices (a requested action, an outcome, an emergency in the village: a
death, starvation, thirst, freezing) appear top centre and fade; routine news
stays in the Oversee feed.

A workspace opens in the right column, one at a time:

| Workspace | What you can do | Key |
|---|---|---|
| **Pouch** | See what you carry; choose an item to eat, drink, wear or offer it; rest | I |
| **Craft** | Gather from discovered sources and use workshop recipes | C |
| **Build** | Inspect a house site, claim it and build a cottage | B |
| **Garden** | Select a plot, crop and individual bed; plant, water and harvest | G |
| **Trade** | Buy or sell nearby goods and offer your surplus | T |
| **People** | Select a known neighbour, greet them and ask for advice | N |
| **Explore** | Walk in a world direction or return to a known place | M |

Its header has the workspace's icon and name, a badge where there is something
to tell ("1 workshop known", "No merchant in reach") and Close. Craft and Trade
are two panes wide where the window allows it: what you carry on the left,
recipes or offers on the right. In the Pouch an item is chosen first; what can
be done with it (Eat, Drink, Wear, Offer for sale) then appears under the
list, so nothing is used by a stray click. A recipe shows what it takes as
chips ("Plank 1/3", with a tick when you have enough and a diamond when you
do not), the workshop and the time at 1× speed. A recipe you cannot make keeps
its place with its button disabled and the reason beneath ("You need 2 more
plank."); **Can make / All** narrows the list to what you can make now.

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
more. **Stop** (**X**) cancels your current movement or work. Reserved but unspent
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

Building work for the village is not available to your character yet. The sites
that Oversee's build menu or the village planner opens (see
[Build](island-colony.md#build)) are supplied and raised by residents only: Live
has no action to deliver materials to such a site or to work on it, and the
**Build** workspace lists your own house site, not theirs. You can watch a site
fill in, and what you sell to residents or lay in a store may end up in it.

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

## Homes and who is indoors

Residents go inside. Somebody who comes to their own door to rest, sleep, eat,
warm up or wait out a storm steps through the doorway and is no longer drawn
outdoors; a guest who rents a bed at the inn does the same. Nobody with a bed
sleeps in the open. A home's windows and door lantern glow at night while it is
heated or somebody is in, and its chimney smokes.

To look inside while overseeing, do any of these:

- click a home or the inn: its card opens and the building is cut away;
- select somebody who is at home, from the People page;
- bring the camera close over a house (within 15 m).

The roof is taken off and the walls on your side are lowered. Inside are the
beds (one for every bed of the home, a smaller one for a child), a table and
the hearth, which burns when the home is heated, and whoever is in: lying in
their own bed when they rest or sleep, otherwise up by the hearth, at the table
or inside the door. A child's bed shows a small sleeping figure at night. At
night the hearth lights the room, so you can see who sleeps there; a home
without a fire has embers and a candle. Close the card, clear the selection or
move the camera away and the house is whole again. At most three buildings are
open at once, and a home the fog only remembers stays closed.

Somebody asleep lies in their own bed; somebody resting awake, or up, is by
the hearth, at the table or inside the door. A house's door stands open while
somebody steps through it.

In Live, the homes of residents open the same way when your character is in
one. Your own homestead cottage opens too. It is a hut two metres square: its
three beds are one bunk of three berths, and there is no room for a table.

## The island around you

- **Workplaces** stand on the ground the village keeps clear for them, as large
  as the homes: the mill's sails turn over a door you could walk through.
- **Fields** show every plot: pale bare soil, dark soil with pale green lines
  when sown, green rows that grow taller, and gold (grain) or red rows when
  ripe. In winter the plots rest under frost. Each village has a grain field
  and a smaller turnip field: turnips are sown from spring into early autumn,
  keep through winter and are eaten when bread runs short.
- **The hen house** beside each farm is a fenced yard. Its hen keeper feeds the
  hens grain or turnips once a day and sells their eggs; hens left unfed
  dwindle, well-fed ones breed up to eight. There is no hen model yet: the yard
  is drawn empty.
- **The merchant boat** sails in before it lands, lies broadside off the
  harbour's trading post while it is in, and sails out when it leaves.
- **Weather** is today's: rain falls in thin streaks under a grey sky, a storm
  is darker with lightning, snow drifts down in a pale haze. A drought, a
  blight or a harsh winter tints the air.

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

In **People**, choose a known resident by name and profession and **Talk**, or ask about a home,
gardening, water or trade. Conversations happen face to face. Familiarity reflects
your known relationship with that resident; the village's ordinary social life
continues around you. The selected conversation appears first; **Change neighbour**
returns to the resident list. Nearby animals are not conversation partners. A
leisure meeting needs available participants: sleeping, committed work and urgent
needs take priority over the proximity-based familiarity gain.

## A day in Live mode (living world)

These parts of Live mode belong to the regional `living_world`, the only island.

**Sleep to morning and the day summary.** From six tenths of the day on, at the
door of your own home (within 4 m), **Sleep** puts you to bed until the next
morning. Nothing is skipped: the host may run the clock faster while you sleep
(`view::asleep_until`), and **Wake** gets you up early. Without a bed of your
own, rent one at the inn. Each morning shows a summary of the day before: coins
earned and spent, goods gathered, made and received, what you bought and sold by
trade and for how much, the residents you met, the
lowest each need fell to, and whether you slept in a bed.

**Repeat work.** **Repeat** does the same work again, 1 to 50 times: gathering
at one resource patch (you walk back to it if you are out of reach, as a single
gathering would), a recipe at a workshop, or planting, watering or harvesting
the beds of your own plot. It stops by itself when the count is used up, a need
turns critical, the inputs run out, the pack is full or the target is gone, and
says which.

**Friends.** Talking (once a day), trading and gifts raise your bond with a
resident; three days without contact lower it. The tiers and what each gives:

| Tier | From | What it gives |
| --- | --- | --- |
| Stranger | 0 | nothing yet |
| Acquaintance | 5,000 | a warmer greeting |
| Friend | 20,000 | 10% off what you buy from them, a gift back about every 4 days, help on your homestead |
| Close friend | 50,000 | 20% off, a gift back about every 2 days, help, an invitation to the next gathering |

**Residents notice you.** A resident with free time (its evening, or a moment
with nothing else to do; never at work, asleep or in a meeting) who sees you
within 6 m turns to you and says hello, once a day each: the line follows your
bond tier. If you sold one of its goods today and you stand in its settlement,
it says "You've been busy at my trade, <your name>." instead. The greeting adds
half a talk (500 points) to the bond and appears as a speech bubble and a
message. This is the resident tree's `Player nearby` branch (`life.player_within`
and `life.greet`, see [authoring](island-ai-authoring.md)).

**Gifts and tastes.** Give 1 to 5 units of bread, berries, a cooked meal, fish,
logs or planks within 1.5 m. Each resident likes one gift and dislikes another
(6,000 and −2,000 points; anything else 2,000). Only the first gift of a day
counts. You learn a resident's tastes after the first gift, or once they are a
friend; the People panel then shows them.

**Homestead tiers.** At your cottage door, start an upgrade in order: a storage
chest (40 units), a second plot, a tool shed (your gathering takes 80% of the
time) and a larger cottage (the family house body, one more bed). Each costs
planks and logs (the tool shed an ingot) and work at the door.

**Deliver, order, hire.** An upgrade is a small building site at your door,
paid from your own coins:

- **Deliver** what you carry yourself: everything it still needs is handed
  over while you stand at the door.
- **Order** a material: the price of the day plus a 20% carrying fee is held
  in escrow. A resident of the settlement with spare goods, or one who buys
  them at a store, walks them to your door; only then is it paid. The site
  says who is on the way ("Oak is bringing 2 log."). When nobody has any to
  spare, your order counts as demand for the village's makers: a carpenter
  saws the planks you ordered.
- **Hire** a builder (4 coins a stage, held in escrow): a resident of the
  settlement, a carpenter first, walks to your door in working hours and
  builds there in stages. You may also work on it yourself by standing at the
  door; a builder who arrives while you work joins you, and both paces add.
  The site says what the builder does: `hired` (waiting for working hours),
  `walking` (on the way), `waiting` (on its way but standing still, between
  stages) or `working` (a stage at your door, about 180 ticks).
- A friend may walk over in their free time, at most once a day: they bring a
  missing material, work a stage at your door or harvest a ripe bed.
- **Cancel** returns what was delivered and every coin still held. A resident
  already carrying goods for you keeps them: it bought them with its own coins.

**The chest.** Once the storage tier stands, **Store** and **Take** move goods
between your pack and the chest at the door. A friend's harvest goes into the
chest when it has room.

**Gatherings.** On the second day of autumn and the fourth of winter, late in
the afternoon, the village gathers in the square of each settlement (a harvest
feast, a midwinter fire). Residents with their needs above half walk there and
gain morale; attending one is a goal. When somebody is laid to rest, those
close to them mourn for three days.

## Pause, save and change modes

You may pause or change speed while living as a character: **Space** pauses and
resumes, **-** and **=** step through 1×, 3× and 10×, exactly as in the other
modes. This is a single-player world, and a faster clock never changes what one
tick does. While paused, your character waits; press **Space** again (or
**Resume** in the bar) to continue.

**Menu** pauses the whole village. **Esc** first closes an open workspace, then
opens the pause menu; **Esc** again resumes. The menu says who you are, the
season and day, and how long ago the island was saved. Choose **Resume**,
**Save game**, **Settings**, **How to play**, or, under the rule, **Save and go
to title** or **Save and quit**. The last two save before leaving play. A save
failure keeps the application open with a recoverable error. The three ways to
play are not in this menu: they are in the top bar, in every mode.

A save remembers which version of the island made it. Continuing a save from
an older version says so plainly and asks you to start a new island; nothing is
upgraded or migrated. A save from a newer version asks you to update
(`village::island_save`, ADR 0042, ADR 0044).

Normal play uses `<home>/island-village/island.world.json`, with the world rules
beside it as `living_world.json`. Saves of earlier versions, in any location, are
not read; see [the coastal world](living-world.md) for regional play. During
play it autosaves every minute; **Ctrl+S** or **F9** saves immediately. The minute's
own save is silent. A save you ask for is confirmed once, in every mode, by a
toast under the top bar: **Island saved**, with a tick, for three seconds. A
failed save, whichever kind it was, is a toast with a diamond, the reason and
**Try again**, and it stays until the next attempt. A second
window cannot own the same save. Backups use `.previous-N.json` and are never
overwritten. Save data includes the world's rules; editing a preset does not change
an existing world.

Switch between the three ways to play at any time from the top bar or with
**1** (Live), **2** (Oversee) and **3** (Shape). All three use the same living
island. **F11** also switches from character play to overseeing, where it opens
the ledger's last page. In Oversee a resident's inspector, a building's card and
the ledger share one column at the right, and the build tray opens above the
bottom docks; [the colony guide](island-colony.md#the-oversee-screen) describes
them. Choose **Live**
to return; if your character has not arrived yet, Live begins the arrival, and
after a death it reopens the successor choices. Inspecting does not freeze an existing character's needs; use **Menu**
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
| Middle mouse button, drag | Orbit and tilt the camera |
| Shift+Tab from the world | Enter the ring of controls at its end: the hotbar, then the needs, the goal and the top bar. Tab walks it forwards; off either end returns to the world, and so does Esc |
| I / C / B / G / T / N / M | Open or close the corresponding workspace |
| F2 | Open Pouch, or close the current workspace |
| Tab / Shift+Tab | Tab opens Pouch when no workspace is open; both navigate its controls when one is |
| Enter / Space | Activate a focused control |
| X | Stop movement or work |
| Esc | Close workspace, then pause; Esc again resumes |
| 1 / 2 / 3 | Live as your character / Oversee the villages / Shape the land |
| Space | Pause or resume time (activates a control when one has keyboard focus) |
| - / = | Slower / faster: 1×, 3×, 10× |
| Ctrl+S / F9 | Save during play |
| L while watching after death | Reopen life and successor choices |

Click the world or close the workspace to return keyboard focus to movement. In
Oversee and Shape, WASD or arrows pan, Q/E orbit, and the wheel zooms; a
middle-mouse drag orbits and tilts, the pointer at a window edge pans (unless
Settings turns Edge pan off), and Tab walks the top bar and the panels. Space,
`-` and `=` drive the same clock in every mode. The pause menu also works in
these modes. See [Colony management and terrain editing](island-colony.md) for
the Shape tools and their keys.

## Developer entry points

The developer menu (`cargo run --locked -p game_menu`) serves the framework
examples (Actor Testbed, village, RTS demo, editors, AI explorer), not the island.
For a separate local playtest, use a different save path (`--front-end` keeps the
title; a bare `--island-save` opens the developer host):

```powershell
.\target\debug\island_game.exe --front-end --island-save C:/NPC/playtest.world.json
```

Add `--island-rules C:/NPC/worlds/living_world.json` when creating a new world from
edited rules. Use a new save path to adopt those rules.
`cargo run --locked -p island_game -- --scenario living_world` is the developer host,
without this player frontend.
See [NPC AI authoring](island-ai-authoring.md) and
[architecture ownership](architecture-island.md).

This increment provides the connected title-to-home-and-garden loop. It retains
the existing 3D style and survival rules. It does not add an appearance creator,
marriage, festivals, animal husbandry or a general furniture-placement system.
Key remapping, gamepad input and arbitrary behavior-tree graph editing are
also separate work. The current NPC authoring interface edits validated routine
priorities, thresholds and planner settings; it does not edit arbitrary tree topology.

The final improvement direction is documented in the
[experience](research/2026-11-09-experience-research.md),
[simulation](research/2026-11-09-simulation-research.md) and
[implementation](research/2026-11-09-implementation-research.md) research briefs.
Those briefs distinguish source evidence and proposals from implementation;
[the validation record](_notes/2026-11-09-island-player-flow-validation.md) records
which checks were actually performed.
