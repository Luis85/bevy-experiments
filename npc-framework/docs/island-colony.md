# Island Village

For the title-to-cottage player journey, start with [Life on Island Village](island-life.md).
This guide covers the colony ledger, terrain editor and developer entry points.

Run from `npc-framework/`:

```powershell
cargo build --locked -p island_game
.\target\debug\island_game.exe
```

The normal title offers **New game** and **Continue**. In play, the bar at the top
switches between **Live** (key 1, your character), **Oversee** (key 2, this guide's
colony view) and **Shape** (key 3, terrain tools). These modes share the same local
island, saved as `<home>/island-village/island.world.json` (see [the coastal world](living-world.md)). During
play it saves every minute, on **Ctrl+S** (or **F9**), and when leaving through the
pause menu or closing the window. A save you ask for is confirmed by a short notice,
bottom left, that fades after a few seconds; the minute's own save says nothing. A
second window cannot open the same save. Save failures appear on screen and stay,
whichever save failed; a failed close-save leaves the window open so you can retry.

For an inspector-first world, choose **New game**, name the potential visitor and
select **Oversee the village**. The colony runs without beginning that visitor's
mortal life. With an existing save, **Oversee the village** is also available on the
title. **Live** in the top bar performs the actual arrival when needed.

`cargo run --locked -p island_game` is the development entry point. Running the
already-built executable avoids recompiling a different Cargo feature combination.

If the Windows renderer exits with `DeviceLost`, Vulkan was verified during the
November 8 colony iteration. Select it for the current PowerShell session:

```powershell
$env:WGPU_BACKEND = "vulkan"
.\target\debug\island_game.exe
```

This is a process/session setting, not a global driver or Rust configuration change.

**New game** keeps the previous save as a numbered `.previous-N.json` backup and
uses the world's loaded immutable rules. **Continue** keeps the current island.
Backups are never overwritten. To change the rules of a new world, use a separate
save path with `--island-rules`, or `--island-new`.

For a disposable world without persistence:

```powershell
cargo run --locked -p island_game -- --scenario living_world --island-mode inspector
```

Use `--island-mode character` or `editor` for the other entry points. Add
`--island-save C:/NPC/my-island.world.json` to create or resume a separate persistent
island. Existing saves retain their construction rules. Use a new path to start a
fresh world with edited rules; incompatible saves are preserved and refused.
Add `--island-rules C:/NPC/worlds/living_world.json` to use your edited preset for a
new local island, or `--island-new` to start fresh with a backup at the same path.

## The Oversee screen

Oversee shows the island between two thin bars; everything else opens because you
asked for it, in one fixed place, and closes again. (Design:
`docs/superpowers/specs/2026-11-11-island-ui-redesign.md`; building with it:
[`island-ui.md`](island-ui.md).)

- **Top bar**: Menu, the three modes, then the village totals: residents and beds,
  morale, food, clean water, logs, planks and the civic fund. A triangle after a
  number means it is low (less than one food or water per resident, or fewer logs
  or planks than open sites still need); an arrow means it moved by more than a
  tenth since yesterday. Rest the pointer on a total for the exact reading and,
  a moment later or with **Alt**, the detail; click it to open the ledger page
  that explains it. At the right: today's weather (in a world that has weather),
  the date and time with a line for the day's progress (click for the
  **Calendar**), and the clock: pause, 1×, 3×, 10×. While paused a **Paused** pill
  shows under the bar as well.
- **Alerts**, top left: at most three rows, critical and warning only, each with
  its shape, a word for its severity and one line of detail. Click a row to frame
  the place and select who or what it is about. **Hide** folds the stack to its
  header, which keeps the counts; it also folds while the build menu or the right
  column is open, and **Show** then closes those. The last line opens the
  **Journal**.
- **Bottom left**: **Overlays** (**O**) and **Places** (**M**). Places frames a
  village or the whole island. Overlays lists **Trades**, which shows or hides the
  key to the residents' marker colours above the dock; the other overlays are
  listed but not in the game yet, and say so.
- **Bottom centre**: the **Build** categories (**B** opens the build tray above
  them).
- **Bottom right**: the ledger's pages, **Village** (**F1**), **Economy** (**F2**),
  **People** (**F3**), **Projects** (**F4**, with the number of open sites) and
  **Journal** (**F5**). **F11** reopens the last page.
- **The right column** holds one thing at a time: a ledger page, or the inspector
  of the resident or building you selected. It is closed until you open it. A new
  selection replaces the ledger; opening a page hides the inspector; **Esc** or
  the close button empties the column and clears the selection.

**Esc** undoes one step per press: keyboard focus, an open list or the calendar,
the building in hand, the build menu, the right column, and only then the pause
menu. Nothing on screen explains keys in running text any more: a key is shown as
a small cap beside its control or in the control's tooltip.

At an Interface size that leaves less than 1024 by 700 of room (140% in a
1280×720 window, for example) the layout is compact: the mode switch shows icons
with the word on the active mode only, the totals move to a row of their own
under the bar, today's weather is an icon inside the date, the alert stack
keeps its header only, and the ledger dock is one **Ledger** tile (**F11**) that
opens the last page; the ledger's own tabs and **F1** to **F5** reach the rest.

### Profession targets and village priorities

The profession table lists every trade of a settlement with how many work it
now, the overseer's wanted number, and, when they differ, why in one
sentence: a seat is missing ("build another pottery"), nobody here has the
aptitude, or residents will move at their next review. **+** and **-** set the wanted number
(`SetProfessionTarget`); clearing it lets the village decide again. A target
is a wish, not an order: residents move toward it through the usual trade
changes, one review at a time, never in the middle of a task, and a trade at
its target takes nobody new, not even an empty essential seat. A worker over
the target takes any open seat of a trade below its own target at the next
review, the last worker of an essential trade too (a target of 0 empties the
trade). The inn is kept one keeper per inn and has no
target.

Priorities set **low**, **normal** or **high** on housing, storage,
workplaces (all, or one trade's), the food reserve, the fuel reserve, coats
and the boat's orders (`SetVillagePriority`). High builds sooner and first,
or keeps twice the reserve; low waits twice as long, builds last, or keeps
half; each row says what its level does. Normal everywhere is the village as
it runs without priorities. (ADR 0043.)

## Inspect and manage

The **ledger** opens in the right column from its dock (or **F1** to **F5**, **F11**).
**Village** shows population, beds, wellbeing and the colony policies.

**Economy** opens on **Goods**: every tradeable good for the whole island
(**All**) or one settlement, in three groups (**Food and water**, **Materials**,
**Tools and clothing**), with the stock residents and stores hold, the market's
reference price, for food and water the **Days** the stock lasts at the
simulation's own estimate of a day's use (a dash for other goods: their day of
use is what is traded, not what is eaten), and the change against the last game
day (sampled by the view every 200 ticks; it starts empty in a new session). A
good is marked **Low**, with a triangle, when food or water lasts less than two
days (in a settlement too), when the island has none of it, when open building
sites still need more of it than there is, or, for food and water, when the
island holds less than one unit per resident. Choose a row for the good's **detail card**
at the foot of the ledger, where it stays in view however far the table is
scrolled: how much each settlement holds, the reference price, what open sites
still need, how much is used a day (food and water), **who makes it** (up to
four residents with their trade, and how many more) and the change since
yesterday in words; choose the row again to close the card. **Jobs** switches the page to the professions table: workers and
open workplaces per trade.

**People** is the roster of everyone living on the island. Click a resident in
the world or in the roster to open the **resident inspector**.

### The departed

The **People** page ends with **Departed** once somebody has died: one plain
sentence each, newest first ("Bram, smith. Died of cold on day 12, aged 9
years."), from the simulation's own record; a settlement's filter applies to
them as to the roster, and a long list shows its latest twelve and counts the
rest. A body stays where it fell for a day and is then laid to rest; until then
the line says so. From that moment the resident is no longer in the world: an
inspector that was open on them closes with a note ("Bram has been laid to
rest. They are remembered on the People page."), and alerts about them keep
their text and the place where it happened. The Journal notes a burial ("Bram
was laid to rest") and a sleeper who was woken before morning, by a need or by
an order ("Ada was woken before morning"); both are routine notes, never
alerts.

### Families, the calendar and sorting goods

The **Village** page has **Families** under its totals: households, couples,
children and elders, and what has changed since the island was founded (births,
arrivals, comings of age, deaths; the causes of death are in the line's
tooltip). Those numbers count the whole island since its founding; a line
under them gives this year's births, arrivals and deaths from the simulation's
yearly tally.

The **Calendar** (the date in the top bar) adds, where the world has weather and
a supply boat, today's weather, the days ahead ("Tomorrow clear · in 2 days
rain") and when the merchant boat calls; in the living world also the village
gatherings, today's first ("Midwinter fire in 5 days, 19:26–22:48", with how
many came last time), and who is in mourning. While it is open the Paused pill
gives way to it.

On the **Economy** page each header of the goods table sorts the goods within
their groups by that column; choosing it again reverses the order, and an arrow
marks the sorted column. At the Compact size the last header reads "Change" and
a chosen good's detail card is two lines.

### Residents and the inspector

The roster has one row per living resident: a status shape, name, profession and
settlement, then what they are doing, their mood and their coins. The shape is a
green circle while food, water, warmth and rest are all fine, an amber triangle
when one of them is below the level at which the resident acts on it, and a red
diamond when one is below the level that interrupts everything else; the row
then also says "in need" or "in danger". **Name**, **Place**, **Job**, **Mood**
and **Wealth** sort the roster (mood lists the lowest first, wealth the richest;
ties go by name), and **All** or a settlement's name filters it. The settlement
choice is shared with the Economy page. Choosing a row selects that resident,
brings the camera to them and hands the right column to their inspector.

The inspector takes the right column (344 wide, 320 in a narrower window). Its
header has a ring in the colour of the resident's trade marker with the trade's
icon, their name, then trade and settlement, and icon buttons: **Follow**
(**F**) and **Close** (**Esc**), and before them, for a resident chosen from the
roster, **People**, which leads back to it. Under it, one line says how they are, as a
shape, a colour and words ("Doing fine", "In need: warmth", "In danger: food"),
and what they are doing now, in the same words the roster and the tag over their head use.
That tag, shown over the selected resident, is a small bark plate with the same
status shape, their name and what they are doing. The Overview tab has, under
the needs, **Family and bed**: their stage of life and age as the simulation
words it ("Adult · 34"), a partner or the one they are courting, parents and children by name, and the bed
they sleep in ("Bed 1 of 2 in Harbour cottage 1"), with "asleep" or "resting"
while they are; then **Friends**: their five closest bonds with the tier of
each ("Ada · Friend", the played character marked "(you)"). Spans are said in time words ("They will look at their trade
again in about 2 hours."), never in ticks.
Four tabs follow:

- **Overview**: bars for food, water, warmth, rest and morale, each with its
  shape and percentage (rest the pointer on one for what its level means); their
  traits as short words; what they carry, coins first; **Why now**, the one
  sentence on what they chose, with a link to the Why tab; links to their home
  and their workplace, which open that building's card; and **Change job or
  home**, which leads to the last tab.
- **Day**: today's schedule as a strip from sunrise to sunrise (work, meals,
  free time, sleep, shifted by their own hours) with a marker at the present
  moment, and the next changes by clock time.
- **Why**: **Chose**, what they chose and what made them choose it; **Passed
  over**, up to three things that would have come first, with the reason each
  did not; **Checked and fine**, the needs that were checked and found quiet;
  and **Plan**, the production plan they are working to with the steps left.
- **Home and job**: their qualifications, the vacancies they may be asked to
  take and the homes they may move into. These are the recorded commands that
  used to sit on the roster page (**Job & home for …**); they belong to the
  resident now, one step from their roster row.

The reasons come from the resident's latest decision record. A resident who is
carrying on with something is explained by what began it ("Still resting at
home. It began because they were tired…"). A need that is low although the
resident is not acting on it is called out: "Drinking: thirsty, but no water is
carried, for sale at a price they can pay, or in reach." Records are kept for
the current session only; a resident who was already busy when the session
opened is described from the present state until they next decide.

**Follow** keeps the camera on the resident while they move; zooming and
orbiting stay yours, and panning by hand lets go. **Esc**, the close button, or
a click on empty ground closes the inspector; a click on another resident
switches to them; opening a ledger page hides it and keeps the selection.
Inspecting never changes the simulation. The inspector is part of Oversee only;
Live and Shape do not show it. The alert stack folds to its header while it is
open.

The top bar's day and time open the read-only **Calendar** in every mode: the
season and the day within it, the days until the next season, sunrise and
sunset, and one line each on growth and cold for this season and the next.

**Alerts**, top left, lists what needs attention, newest first; the ledger's
**Journal** page has everything, grouped by day, with the filters **Important**,
**Critical** and **Everything**. Every alert has a severity, shown by a shape, a
colour and a word:

| Severity | Shape | Alerts |
|---|---|---|
| Critical | red | a death, starvation, dangerous thirst, freezing |
| Warning | gold | a resource patch running out, food or clean water below one unit per resident, a resident without a home, a collapse from exhaustion, a building site waiting for materials for a day or more |
| Note | green | a change of occupation, an arrival, trades, a finished cottage, levy day, the first tool of each kind made this session, a building project started, finished or cancelled |

The stack shows critical and warning rows only. On the Journal page the notes of
one game day fold into a single line, such as "6 job changes" or "9 notes ·
show" with "5 trades, 3 job changes, 1 levy day" under it; select it to unfold
that day's notes below it, and again to fold them. The page's three filters
switch between **Important** (the default), **Critical** (emergencies only) and
**Everything** (every row, nothing folded). **Hide** folds the stack to its
header, which then says how many rows need attention; **Show** opens it again.

Repeats about the same resident within a game day fold into one row with a
count, and all of a day's trades are one row. Select an alert (mouse, or **Tab**
then **Enter**) to move the camera there and select the resident. The feed keeps
the latest 40 rows and drops routine notes first, so a flood of job changes
never pushes out a death or a warning. It belongs to the current session and is
not saved. Character play shows only a few fading lines, for critical alerts.

A site that is still short of materials a whole day after its last delivery is one
warning row, "Harbour cottage 3 has waited 2 days for materials (planks 3/8 · logs
4/4)"; the row is reworded each further day and goes when the site is supplied,
finished or cancelled. Selecting a project or site row frames the site and opens
its building card.

Starvation, dangerous thirst and an emptied resource patch are reported from the
simulation's own journal, at the moment they begin: a resident is "starving" from
the first tick at the critical level (health starts to fall 600 ticks later), and a
patch is "depleted" when its last unit is taken. Someone already starving or
thirsty when the session opens is listed once at that point. A resident without a
home and a village-wide shortage have no such event and are found by looking at
the village twice a second. No condition is reported by both routes.

At the zooms where residents carry a coloured dot, a small **Resident markers**
key appears bottom left and names the trade of each colour present on the
island. **Hide** folds it; it is not shown in Live or Shape, nor while no marker
is drawn (at night, with everybody indoors).
Planning explanations distinguish an unfunded goal, missing current prerequisites
and a bounded search limit. A search-limit message does not mean a goal is impossible;
the shared work policy continues to operate.

- Set a household food reserve of one to four days. This feeds funded demand;
  it creates neither goods nor purchasing power.
- Enable or disable social gatherings. Relationships grow when residents meet
  physically and both are available for leisure. Sleep, committed work, its work
  approach and critical needs take priority. Housing, provisions and companionship
  influence morale.
- Assign a resident to a household with an available bed. This changes where
  they go to rest, without teleporting them.
- Commission a home for a resident on an available house plot. The resident must
  procure real logs and planks and complete the shared construction action.
- Cold homes need real fuel. A household steward buys wood and burns it at home;
  heat improves warmth recovery. A living member within 1.5 metres of the door
  can supply one unreserved log for an unlit hearth in cold weather, heating it for
  half a simulation day. The player Pouch shows own-home heat and carried fuel.
  A building alone supplies shelter, not free heat.

The village has twelve professions and connected food, timber, mining, tools,
hunting, leather, thread and clothing chains. Money and goods move through physical
actions. NPCs protect emergency needs, plan funded work and recover at home.

**Space** pauses; **- / =** changes speed (the clock in the top bar shows the
active rate); **.** advances one tick while paused.
WASD or arrow keys pan in inspector/editor mode; **Q/E** orbit and the wheel zooms.
Dragging with the middle mouse button orbits and tilts, and resting the pointer
at a window edge pans (**Settings** has an **Edge pan** switch and a **Pan
speed**). Keys are shown as key caps beside their controls and in tooltips, in
every mode.
Commands queued while paused apply when you resume or step.

**Tab** and **Shift+Tab** reach every control without the mouse. In Oversee the
order is the top bar (menu, modes, totals, the date, then the clock), the
alerts, the right column (the inspector or the ledger), the build tray, the
Trades key, the left dock and its open list, the build dock, then the ledger dock;
in Shape it is the top bar, then the tool rail, then the brush bar. The focused control has a gold border and
**Enter** or **Space** activates it; **Esc** or a click on the world hands the
keyboard back, after which Space pauses again. A mouse click on the bar or the
palette never takes keyboard focus. In Live, Tab still opens the Pouch;
**Shift+Tab** from the world enters the ring at its end (the hotbar, then the
needs, the goal and the top bar), and tabbing off either end returns to the
world.

## Build

Oversee can place any building the world's rules define; the village then builds
it. Nothing is free: residents carry real materials to the site and the civic fund
pays them on delivery. The rules of a building (cost, size, ground, clearance) are
data; see [the economy guide](island-economy-authoring.md#buildings).

### The build tray

A category on the **Build** dock, or **B**, opens the build tray: a panel above
the docks, across the middle of the window. The same category, **B** or its
close button closes it again. The categories are the ones the running world's
buildings fall into: **Homes**, **Work**, **Storage** and **Market**, and
**Farms** where the world has a building with crop plots (the farmers' own
workplace is offered with it there). An empty category has no tile.

The tray lists the category's buildings as cards: a picture, the name, the cost
as an icon and a count for each material, and one line on what the building is
for ("A home with 2 beds"). They stand in one row; more than fit scroll sideways
with the wheel or the rail under them. A card the simulation's catalogue marks unavailable
looks washed out and gives the reason in place of that line ("About 300 coins:
the fund may spend 120 and the building levy can raise only 80 more."); the card
stays reachable so its reason can be read. At the right the tray shows the island's stock of the materials
the building under the pointer (or in hand) costs, each marked with a circle
when there is enough and a triangle when there is not, and what the civic fund
has to spend. Opening the tray closes the right column; opening a ledger page
closes the tray.

Choose a card and move over the island: the card says **In hand**. The ghost
snaps to the one-metre cells: a translucent box over the body, an outline on
every cell it would cover, and a gold, door-high marker on the entrance cell.
**R** turns it a quarter. A tag follows the cursor: a tick and "You can build
here", or a cross and "Cannot build here" with the simulation's reason as a
sentence under it ("The ground is too steep here."), then the building, its cost
and its footprint, and the keys: click to place, **R** to turn, **Esc** to put
down. The ghost is green or red as well, but the verdict never depends on the
colour. The answer is the simulation's own placement check, asked when the cell,
the turn or the building changes and twice a second otherwise.

A click on an accepted ghost records one `PlaceBuilding` command and opens a
construction site; the building stays in hand for the next one. A click on a
refused ghost records nothing. If the world refuses a placement after all (two
clicks in one tick, or someone walked onto the spot), the tag gives that reason.
The tag sits right of the cursor in the left part of the window and left of it
from the middle on. While the world is paused the command waits for the next
tick. **Right-click** or **Esc** puts the building down; **Esc** again or **B**
closes the tray. The tray, like every panel, is in the Tab ring.

### Sites and buildings in the world

A construction site shows its footprint as a bed of earth inside a staked rail,
with a doorway on the entrance side. Delivered materials lie beside the doorway
(log bundles for timber, crates for anything else). When everything is in, a
scaffold stands; the plank walls then rise with the work while the pile is used
up. The finished building replaces the site: homes and workplaces use the same
models as the founding ones in the settlement's roof colour, and a new home shows
window light at night and chimney smoke when its hearth burns. A stockpile is a
fenced yard and a barn a timber shed; both show their stock as props, more of them
the fuller the store. A market stall lays its goods out in front. The buildings a
world founds through the same rules are drawn the same way: a field's open
granary is a fenced yard (its crop plots are not drawn yet) and the trading post
is a stall. The hen house, an open workplace of a trade only the data defines,
is a fenced yard too; its hens are counted, not drawn. Sites and buildings follow the same fog rules as every other place in
Live.

### Who pays for a site

Every building site has a payer. A village site (placed in the build tray or
commissioned by the planner) is paid by the civic fund: the fund buys its
materials at the quote when a hauler hands them over at the site. A played
character's homestead upgrade is paid from that character's escrow: the price of
an order and its carrying fee, and the builder's wage for each stage. Both kinds
are supplied and built the same physical way, by residents who walk to the door,
and the village's own sites come first in a resident's work hours. The planner
never counts, funds, prioritises or cancels a homestead upgrade.

A village site whose own settlement has none of a material in store and nobody
in a trade that makes it is supplied by the nearest other settlement that makes
or stores it (`civic::unmet_elsewhere`): that settlement's plan funds the work,
and its residents carry the units over when no site of their own needs a trip.
The boat does not buy what open sites still lack, and the last woodcutter (or
carpenter) of a settlement stays while a site it is to supply waits for logs
(or planks). See `docs/island-economy-authoring.md`, "Materials from another
settlement". See
[ADR 0040](adr/0040-homestead-work-and-the-payer.md).

### Cold, coats and the inn

A resident without a coat whose Warmth falls below `tuning.freezing_warmth`
goes home to get warm before anything but a lower critical need. Out in the
cold without a coat it starts no outdoor errand below `tuning.exposure_warmth`
and waits at home. From `tuning.coat_lead_days` before winter everybody without
a coat buys one; the civic fund pays for a coat a resident cannot afford, and
tailors make one for everybody still without. The inn is a service trade: an
inn without a keeper is staffed by a resident from a trade with workers to
spare, the keeper stays, and the civic fund tops the day's takings up to the
data's wage floor while it is the settlement's only inn. A tired or cold resident
far from home (`tuning.inn_detour_m`) rents a bed at a nearer staffed inn when it
can pay. See
[ADR 0041](adr/0041-cold-first-needs-and-service-trades.md).

### The boat and the trading post

The merchant boat calls every three days and ties up at the berth, the sea cell
nearest to the harbour's centre (`view::boat().berth`), and the trading post
stands beside it at the waterfront (it is the boat's stall: storage is not
carried out to it) (a waterfront building may stand on the coast however far that is from
its settlement's centre). The boat is the island's outlet and source. While it is
in, or due that day, residents of every settlement carry unsold stock of the goods
it buys to the trading post in their work hours (`life.haul boat`); the boat buys
what the port's stores and stalls hold above the island's stock target (the
settlements' targets, every traded good covered by `tuning.raw_stock_cover`) and
a store's kept level, up to its limit of a good per visit and what its purse can
pay, paying each owner at once. What open building sites still lack is not
surplus: the island's offer leaves it out, and the port's stores and a resident's
trip to the post keep back what its settlement is to supply (`civic::site_hold`). It does not buy fiber or thread: fiber is a
harvest the tailors spin through winter and spring. It sells the civic fund what
the port lacks of its kept level, or what the island lacks of its stock target
while the good is dear (above its base price), laid in the harbour's market
stall near its centre (`merchant.market`; founded with the village) and sold on
at the market price. What the fund cannot pay for is laid there on consignment
(owner `merchant::MERCHANT_OWNER`): a resident who buys it pays the boat's
purse, and the boat never buys its own goods back. Its purse, calls, stay,
price spread and limits are data (`economy.merchant`); a share of each
visit's trade (`tuning.boat_duty_percent`) goes to the civic fund as duty.

As it leaves, the boat announces what it will buy at its next call
(`view::boat().next_order`: good, units, unit price above the island's, units
bought so far): three goods it buys that a trade of the island makes, rotating
call by call, never fiber or thread. The order is demand in the harbour's
production plan, so its makers work to it; residents carry it to the trading
post when the boat is due, and the boat pays from its purse. What is not
delivered expires when it leaves.

### The civic fund

The fund takes the daily levy, the relief and building levies and the boat's
duty; it pays the destitution floor, building materials, coat grants and the
inn keepers' wage floor, and hands back what lies above its reserve and what
open sites hold as a dividend. Floor grants never take the coins held for open
sites: when the floor needs more, the relief levy raises it from coins above
the floor. Sites pay their materials from `civic::site_budget`.

### Wildlife

Deer graze, walk short distances inside their habitat's home range (12 m),
keep 1.5 m apart, stay on open land (no water, field or building) and gallop
away from a resident who comes within 8 m, then settle and graze again. An
animal being hunted stands still. `view::wildlife()` lists each animal's id,
kind (`deer`, `stag`), position, target, state (`grazing`, `walking`,
`fleeing`, `dead`) and speed in mm a tick. A hunter keeps the animal it
stalks and re-paths to it only once it moved 3 m; that animal does not flee
from that hunter, only from everybody else.

### Adding to the catalog

Everything the island makes, trades and builds is data in the world's
`living_world.economy` block, validated at load; no code names a good or a
trade (ADR 0042).

- **A good:** an entry in `goods` with `key`, `display`, `base_price`,
  `stock_limit`, an `item` (weight and stack) and its `tags`. Tags give it a
  role: `food` (with `food_value`), `drink`, `fuel`, `clothing`, `tool`,
  `staple`, building materials `timber` and `lumber`, `metal`, `ore`,
  hunt yields `meat` and `hide`, garment inputs `leather`, `yarn`, `fiber`,
  foods `baked`, `cooked`, `foraged`, and `grain`. A gathered good adds a
  `patches` entry (and a resource site in a settlement).
- **A recipe:** an entry in `recipes` with `key`, `display`, `station`,
  `inputs`, `outputs`, `duration`, optional `tool` and `skill`, listed in the
  `recipes` of the profession that crafts it.
- **A trade:** an entry in `professions` whose `role` is any new key
  (lower-case letters, digits, `_`: `"potter"`), with its `display` name, its
  `station`, `outputs`, `wants`, `recipes`, `tool`, `aptitudes`, `skill`,
  `endowment`, the actor `template` a founder of it uses, and what it does for
  the village: `hunt { tool, yields }`, `builds`, `sows`,
  `service { wage_floor }`, `essential`. The key is the trade's identity
  everywhere: jobs, labour, workplaces, the planner, views, the journal
  (`RegionalOccupationChanged` carries `to_key` and `to_trade`, the display
  name) and saves. Every behaviour comes from these fields and the recipes:
  a trade gathers the outputs that have a `patches` entry, crafts those its
  recipes make, and sells what its customers `want` or `keep`. To have
  residents take it, give it a workplace building (`kind: workplace` with
  its `profession` and `station`) and either a site in each settlement's
  `sites` (staffed from the founding) or let the planner build one when its
  goods run short; list it in other trades' `aptitudes` (a flexible founder
  may take any trade); and give it customers (`wants`, `keeps`, or the
  boat's `buys`). A trade nobody founds needs no founder entry.
- **A building:** an entry in `buildings` with `cost`, `work_ticks`,
  `footprint`, `kind` (home, workplace of a profession, storage, stall),
  `solid` and `placement`; the planner and the build menu read it. A
  storage building with a `field` is a farm or garden plot (each founding
  field is laid out in every settlement: the shipped world has grain and
  turnips). A workplace with `livestock` keeps animals as counted stock that
  its trade's recipes feed (the inputs) and collect from (the outputs); the
  shipped hen house is the hen keeper's (`herder`), see
  [Livestock](island-economy-authoring.md#livestock).
- **A prop:** an entry in `props` with `id`, `display`, `footprint`
  (`width`, `depth` in cells, 1 to 4), `solid` (blocks walking; default
  true) and `model` (the model the view draws, relative to the asset root).

What a data-only trade falls back to where the game has no data for it: the
view draws its workplace with the generic workshop model sized to the
footprint (an open yard, `solid: false`, with the yard marker), its workers
with the generic crafting pose while they craft and the gathering pose while
they gather, and its marker in the generic trade colour; a founder without
a template of its own uses the `template` named in data (any shipped one).
None of these is a failure.

A world whose data leaves a role the logic needs empty (no `fuel`, no
`drink`, no garment, nobody who `builds`, ...) is refused at load with a
sentence naming it.

### Coats at the founding

Two founders in three come wearing a coat, and each settlement is founded
with its tailors' workshop built and each tailor holding one batch of coat
materials: the first winter's rush is a third of the island. Anybody who
carries a coat and wears none puts it on at once, in any season.

### The Projects page

The **Projects** page of the ledger has **Building projects**:

- the civic fund as two numbers: **To spend**, the coins that can pay for
  materials now, and **Held for open sites**, what it keeps back from the daily
  dividend for them. Rest the pointer on them for the fund's own sentence: its
  coins, its reserve for destitution grants and, when it has nothing to spend,
  that nobody is paid for materials until it has;
- one row per open site: its name, a progress bar, a chip per material with what
  has been delivered ("planks 3/8"), and what it is waiting for in words
  ("Waiting for materials for 2 days; the civic fund cannot pay for them yet",
  or "Ada is building"). Choosing a row frames the site and hands the right
  column to its card;
- **Abandon …** under each row, which asks first: "Abandon Harbour cottage 3?"
  with **Keep building** (nothing happens) and **Abandon it** (records
  `CancelBuilding`). Delivered materials go to the nearest store with room or
  lie at the entrance; coins already paid stay paid;
- **What the planner sees**, folded until chosen: per settlement, the residents
  without a bed, free beds and unstored surplus at its last review, and its last
  decision;
- **Recent decisions**, folded until chosen: the planner's latest five decisions
  of this session with their reasons, from the `VillageCivicProject` journal
  effect, such as "Harbour deferred a cottage: the civic fund cannot pay for the
  materials yet". The log starts with what the journal still holds when the
  session opens and is not saved.

Under the building projects, **Homes residents build themselves** says how many
residents are building a home of their own ("No resident is building a home of
their own.") and lists the commissions that can be given.

### Building cards

Click a site or a building in Oversee (not a resident, who takes precedence;
anywhere on a house counts, its roof and outer walls included), a project row, or a home or workplace link of the resident inspector, for its card
in the right column. The header has the building's picture, its name, what kind
of thing it is and where, and two icon buttons: **Show on the island**, which
frames it again, and **Close** (**Esc**). A site says how it stands as a shape,
a colour and words (**Collecting materials**, **Waiting** after a day without a
delivery or while the fund cannot pay, **Ready to build**, **Under way**,
**Finished**) with a progress bar, then its materials, its builder and the planner's reason; a home shows its
beds, who lives there and whether the hearth is lit; a workplace its trade and
who works there, and an inn also who is inside now; a store or a stall its stock, how full it is and what it has
sold. Where a building has more than one kind of fact the card has tabs:
**Overview**, **Residents** (a home) or **Workers** (a workplace), and **Stock**
(a store or a stall). A new card opens on Overview. An inn's card counts its
guest beds on Overview ("Guest beds 1/4 taken") and, on **Workers and guests**,
lists each bed with its guest and the time the rented rest ends ("Bed 1 · Odo,
until 06:30", "Bed 2 · free") and who is inside now. A click on open ground closes the card too. The card records nothing.

The resident inspector, the roster and the captions name the same things: a
resident is "building Harbour cottage 3", "carrying planks to Harbour cottage 3",
"fetching logs from Harbour stockpile 1", "storing grain at Harbour barn 1",
"restocking Harbour market stall 1 with bread" or "selling bread at Harbour
market stall 1".

Developer launches have no top bar and therefore no build menu; they still draw
sites and buildings, show cards, and accept the two commands over `npc/command`.

## Enter the island

Choose **Live** in the top bar (or press **1**), or enter through the title's New/Continue flow.
The reserved visitor begins a mortal life only on first arrival. Inspector mode
does not leave an unseen visitor starving before you join.

**WASD** moves your character relative to the camera; right-click also orders a
walk. Normal player mode has **Pouch, Craft, Build, Garden, Trade, People and Explore**
workspaces. **F2** opens Pouch or closes a workspace. Gathering, crafting, food,
water, rest, trade, cottage building and grain/berry gardening use real possessions
and the same action validation as NPCs. Click the world or close a panel to return keyboard
focus to movement.

Returning to Inspector changes the view immediately. Committed gathering, crafting,
construction, hunting or rest keeps its authority and reservations until it settles;
native control then detaches if you are still inspecting. Returning to character
play before completion keeps the activity. **Stop** is the explicit cancellation
action. An existing character remains in the living world with normal needs.

Death remains permanent. **Begin a successor life** starts a successor instead of
resetting the village. **Keep watching the village** dismisses the death panel;
**Life choices · L** reopens it later. Native input will not take a character held
by another controller.

## Shape the landscape

Choose **Shape** in the top bar (or press **3**). Shape is a small editor: the
tools stand on a rail at the left, the brush has one bar at the bottom, and a
tag follows the cursor. Hold the left mouse button and drag across the island
to work.

| Tool | Key | Effect |
|---|---|---|
| Raise / Lower | R / F | Lift or sink the ground from its current height, 0.25 m per pass; hold still to keep going |
| Flatten | T | Level the ground to the height under the cursor where the drag began |
| Smooth | G | Ease each cell toward its neighbours |
| Paint | P | Lay grass, soil, stone or water; heights are kept |

Under the tools the rail has **Undo** and **Redo** and, below them, how many
strokes can still be taken back ("12 of 32"). Redo is dimmed while there is
nothing to lay again, Undo while the history is empty. **Ctrl+Z** undoes the
last stroke and **Ctrl+Y** (or **Ctrl+Shift+Z**) redoes it; the last 32 strokes
of this session are kept, and a new stroke forgets what was undone.

The brush bar holds three things:

- **Size**: a slider from one cell to the world's largest brush (17 m across in
  the living world), with **[** and **]** as its keys and the width in metres
  beside it.
- **Edge**: **Hard** (the default: the full effect everywhere) or **Soft** (full
  in the centre, a quarter at the rim). **H** switches.
- **Surface**: grass, soil, stone and water as four swatches, named in their
  tooltips. Choosing one takes up Paint; **V** steps to the next. The swatches
  are dimmed while another tool is in hand.

The tag beside the cursor names the tool and the ground under it, with its
height to a tenth of a metre ("Raise · ground at 6.3 m", "Paint · stone at
2.0 m", "Lower · water"). The outline shows the cells under the brush. Where the
simulation would refuse the dab the outline turns red and dashed, the tag gains
a cross and the simulation's reason as a second line, and its edge turns red;
the same happens for five seconds after the simulation refuses a recorded edit.
A short notice above the brush bar says what else needs saying: a stroke that
reached its size limit, a stroke taken back or laid again, a refusal, or that
edits are waiting for a paused world. A successful edit says nothing: the
ground shows it, and so does the depth under Undo. Cell coordinates are not
shown. Nothing in Shape is destructive beyond a stroke that Undo takes back, so
Shape asks no confirming question; saving is the island's own (Ctrl+S, the
pause menu) and its notice appears as in the other modes.

Every edit is a recorded absolute command. The view reads the current heights,
computes each cell's target and records one command per dab: the `Terraform` disc
when the whole brush ends at one value, otherwise one `TerraformCells` batch that
lists every changed cell with its own height and surface. The simulation checks a
batch as a whole and applies all of it or none. Undo and redo are one further batch
each, restoring the captured values exactly, so a replay reproduces them. A stroke
is limited to 600 dabs and 4,096 cells (so that its undo fits one batch) and dabs
are rate-limited to ten a second; edits queue while the world is paused and apply
when it resumes. The outline shows the cells under the brush and turns red when
the simulation would refuse the dab; the reason appears beside the cursor, as does
the reason for any recorded edit the simulation refused. A refused dab changes
nothing, and a refused undo or redo leaves the stroke where it was in the history,
to be tried again. Water stays at or below sea level and dry ground at or above
it: paint a surface first to cross the shoreline. Developer launches keep a
Landscape tab that points at the same tools.

`ISLAND_VIEW_SHAPE_BATCH=off` makes the living world record one `Terraform` per
changed cell (at most six a tick), for comparison; see
[the measurement](island-atmosphere.md#shape-strokes).


The map uses one-metre cells. In the normal living world dry ground ranges from 0 to
12 metres and water from -2 to 0; dry ground cannot be below sea level, and water
cannot be above it. Editing protects residents, worksites, resources, home entrances and
plot sites, and rejects changes that disconnect passable land. Terrain changes
update navigation and persist in the save. The undo history is view-side and
lasts for the session; it is not saved with the world.

### Plots and props

Shape also lays out farm and garden plots and props, each a recorded command
with the simulation's reason when it is refused. A plot (`PlacePlot`: the
`field` or the smaller `garden`) appears finished and empty, ready to sow; it
cannot lie on water, over a road, on a building, a resource site or another
plot. A prop (`PlaceProp`: barrel, crate stack, fence piece, flower bed,
signpost, flag, wheelbarrow, or any prop the world's catalog adds) stands on
grass or stone near a settlement, never on water, a road or bare soil, and
never where it would cut off ground; a solid one blocks walking, and nothing
is built over any of them. Undo is the inverse command (`RemovePlot`,
`RemoveProp`); a plot that holds seed or goods, grows a crop or is being
worked cannot be taken back. Both are part of the exported world (plots in
`buildings`, props in `props`). (ADR 0043.)

## Author the simulation

The shipped rules are in `content/island_content/worlds/living_world.json`; the game
installs them as `<home>/island-village/living_world.json`. Edit that file before a
**New game**, or pass `--island-rules FILE` with a new `--island-save`. Saved worlds
keep their original rules. The developer menu's Worlds editor no longer edits island
rules. See [NPC AI authoring](island-ai-authoring.md) for exact units, JSON and
validation, and [economy authoring](island-economy-authoring.md) for the production
model.

The AI explorer no longer hosts an island (ADR 0044). An AI agent reaches the island
through `island_game --remote` (`npc/*`, `agent/*`, `world/state`).

The normal living world provides 42 established residents in three settlements,
housing commissions and profession assignment to open vacancies.
It does not add births, migration, a zoning system or
an unrestricted object-placement editor: the build menu places the buildings the
rules define, where the rules allow. Resident AI authoring edits priorities,
thresholds and planner settings; tree topology remains Rust-authored, with no general
behavior-graph editor. In the living world, Shape's patch commands place and
remove resource patches (`PlaceResourcePatch {good, x, z, richness}`, richness 25
to 400 percent of the authored capacity, at most four patches of a good per
settlement, the nearest settlement within 60 m; `RemoveResourcePatch {patch}`,
never the last of its good in its settlement), and `frontier::shape::export_world`
writes the current terrain, patches and buildings as a `living_world` block in the
shipped schema (the host writes the file). Trees and rocks are view dressing, not
simulation data, so there is no prop command. There are no other island scenarios.

The [simulation research brief](research/2026-11-09-simulation-research.md) records
why steady-state survival and bounded replenishment recovery do not establish
automatic recovery from a permanent specialist loss. The
[experience](research/2026-11-09-experience-research.md) and
[implementation](research/2026-11-09-implementation-research.md) briefs cover the
UI, discovery, lifecycle, performance and terrain choices. Their proposals are
distinct from the [actual validation record](_notes/2026-11-09-island-player-flow-validation.md).
