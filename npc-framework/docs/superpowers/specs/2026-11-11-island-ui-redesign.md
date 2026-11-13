# Island Village: UI redesign

Date: 2026-11-11. Status: proposal, design only. No code, asset or key binding has
been changed. Visual reference: [`2026-11-11-island-ui-mockups.html`](2026-11-11-island-ui-mockups.html)
(open it in a browser; ten tabs, each a 1280×720 frame). Numbers in this document
and in the mockups are the same tokens; where they differ, this document wins and
the mockup is the bug.

The brief: "a comprehensive overhaul and redesign of the UI; the UI shall follow
best practices and patterns of modern games in this genre", for a Banished and
Stardew Valley crossover with three modes (Live, Oversee, Shape). Icons from a CC0
set are approved by the owner (section 9). Downloading them is a separate step that
this spec describes and does not perform.

Paths are relative to `npc-framework/`. `V/` is `content/testbed_view/src/`.

## 1. What is wrong today

Each finding names the capture that shows it. Captures are under `target/npc-out/`.

| # | Problem | Seen in | Genre practice it breaks |
|---|---|---|---|
| C1 | The Village ledger is open by default and covers the right 30% of the window (about 384 of 1280 px, top to bottom) whatever the player is doing. | `inspector/inspector-working-1280x720.png`, `view-followups/oversee-economy-1280x720.png` | Management screens open on request and close again (Timberborn "Manage Settlement", Banished Tools and Reports, Oxygen Not Included management tabs). |
| C2 | With a resident selected, three floating panels stack on the left (alerts header, inspector, earlier the marker key) and the ledger stays on the right. Less than half of the window shows the world. | `inspector/inspector-working-1280x720.png` | One detail panel per selection (RimWorld inspect pane, Timberborn object detail). |
| C3 | No persistent village summary. Population, food and water are three lines of text inside the ledger's Village tab; nothing shows them once another tab or the inspector is open. | `view-followups/oversee-ridge-feed-legend-1280x720.png` | A top resource and population bar is the genre's constant (Timberborn, Manor Lords, Farthest Frontier). |
| C4 | Hotkeys are written into labels: "Menu · Esc", "Live · 1", "Resume · Space", "Pouch · I", "Undo · Ctrl+Z". The label and the shortcut have the same weight and the label text changes with state ("Pause · Space" / "Resume · Space"). | every capture | Shortcuts are shown as small key caps or in tooltips. |
| C5 | No icons anywhere. Goods, needs, professions, tools and alert severities are words or a coloured dot. | every capture | Icon plus number for resources; icon plus label for tools. |
| C6 | No tooltips. Explanation is permanent hint text: four lines at the bottom of the ledger ("F11: ledger · Tab: bar and panels…"), three lines under the build palette, two under the Shape palette, one under the Live prompt. | `build-menu-try/ghost-invalid-1280x720.png`, `view-wiring/shape-after-raise-drag-1280x720.png`, `phase4a/live-hud-1280x720.png` | Layered tooltips and a help screen carry explanation; the HUD carries state. |
| C7 | Scrolling is two buttons and a sentence: "Scroll up", "More options below", "Scroll down" (and "More above" / "More below", "Newer" / "Older"). The same row appears in the ledger, the alert feed, the build palette, Shape and Settings. | `inspector/residents-by-mood-1024x768.png`, `view-followups/settings-1280x720.png` | A scroll rail, the wheel and Page keys. |
| C8 | The palette is near-black (`#0D0F14`) with gold headings and slate buttons. It reads as a developer tool over a bright, saturated low-poly world, and gold is used for headings, focus, status text and help text at once. | every capture | Warm, material surfaces (Stardew Valley's wood and paper: own reading; Against the Storm's 2022 devlog reports that the "'handcrafted', warm feeling" was what players missed most in the cleaner preview, and puts texture back). |
| C9 | Status is colour only: a green, amber or red dot on roster rows, green or red trend text, a green or red ghost. | `inspector/residents-by-mood-1024x768.png`, `view-followups/oversee-economy-1280x720.png` | Colour is a back-up for a shape, a symbol or a word (Game Accessibility Guidelines; Xbox Accessibility Guideline 102). |
| C10 | The inspector's "Why" is an eleven-line paragraph at 13 px with bullets inside it. Needs, traits, schedule and the full decision record are all on one card. | `inspector/inspector-working-1280x720.png` | Progressive disclosure: state first, reasoning one step away. |
| C11 | The build palette is a text list in a floating panel ("Cottage · 8 planks, 4 logs / A home with 2 beds") with a status sentence, a hint sentence and a cursor note that repeats the keys. The cursor note shows world coordinates. | `build-menu-try/ghost-invalid-1280x720.png` | Category bar, item cards, ghost with cost and a one-line verdict. |
| C12 | Settings are pairs of buttons named after their row ("Master −", "Master +", "Ambience −", "Ambience +") and three state buttons ("Edge pan: off"). | `view-followups/settings-1280x720.png` | Sliders, toggles and tabs. |
| C13 | The Live HUD spends four separate boxes on chrome: a needs strip under the bar, a guidance card, a two-line prompt box and a full-width toolbar. The prompt box is shown even when nothing is in reach ("Move near a resource, plot or neighbour · E interact"). | `phase4a/live-hud-1280x720.png` | Needs in a corner, a hotbar, and a prompt only when something can be used. |
| C14 | The title and pause screens put a black box over the island; pause offers ten buttons in four rows, three of which duplicate the top bar. | `package-test/run/title.png`, `phase4a/pause-menu-1280x720.png` | A short single-column menu over a dimmed world. |
| C15 | The paused state is shown only by one button turning green and changing its text. | `inspector/inspector-working-1280x720.png` | A visible "Paused" marker. |
| C16 | One font weight, sizes 13 to 16 px only, so headings are told apart by gold alone. | every capture | A type scale with clear steps. |

What works and stays: one top bar across modes with the mode switch and the clock;
the calendar popover; alert severities and folding; the roster's sort and filter;
the placement ghost asking the simulation; disabled controls that say why; the
keyboard Tab ring; the confirm before abandoning a site.

## 2. What the genre does

Each row names the pattern, the game and the source. "Own reading" marks a statement
from the author's knowledge of the shipped game that was not checked against a
source in this pass. Source pages were read through a summarising fetch tool, so
the descriptions below are paraphrases unless they are in quotation marks, and a
quotation is only as exact as that tool's own quoting.

### 2.1 HUD anatomy

| Pattern | Game and evidence | What this design takes |
|---|---|---|
| Totals along the top, read left to right; hover for exact numbers and where they are stored | Timberborn: the main screen lists Inventory "from left to right", with an Inventory Detail that expands and shows more on hover; update notes added per-good tooltips and shortened large numbers ([wiki](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen), [update 3 notes](https://tryhardguides.com/timberborn-reveals-new-storage-system-in-update-3-patch-notes/)). Manor Lords: a top bar for the current region whose categories break down on hover, and which switches to construction materials while building ([wiki](https://wiki.hoodedhorse.com/Manor_Lords/Beginner%27s_Guide)). | Six totals in the top bar; each has a two-level tooltip and opens its ledger page; the ones a building costs are outlined while placing. |
| Calendar and forecast beside the clock | Timberborn: "Calendar and Forecast", a top row for the calendar and a second for the coming week's weather ([wiki](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen)). | Weather chip and date chip side by side; the calendar popover gains a forecast strip when weather lands. |
| Speed as a small cluster: pause and three or four rates | RimWorld: a time control at bottom right that pauses or sets normal, 3× or 5× speed ([wiki](https://rimworldwiki.com/wiki/User_interface)). Banished: 1× to 10× and pause ([guide](https://www.gamepressure.com/banished/interface/z06082)). | One segmented control: pause, 1×, 3×, 10×, top right, in every mode. |
| Build tools on a bottom bar; groups open a row above it | Timberborn: the toolbar shows tool groups, and a group's submenu appears centred above the toolbar ([wiki](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen)). Banished: a toolbar of categories (housing, roads, storage, food, resources) ([guide](https://www.gamepressure.com/banished/interface/z06082)). Oxygen Not Included: building categories on the bottom ([guide](https://www.gamepressure.com/oxygen-not-included/main-screen/z39cfa)). | A centred build dock with five categories; a tray of item cards above it. |
| Alerts as a short clickable list that jumps to the place | Timberborn: Alerts lists issues that need attention, each with a count; selecting one cycles through its targets. A separate Journal logs births, deaths and the like ([wiki](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen)). RimWorld: alerts at the right edge for problems that need attention ([wiki](https://rimworldwiki.com/wiki/User_interface)). Banished: an event log whose event types can be muted ([fandom](https://banished.fandom.com/wiki/Town_hall), search summary). | A stack of at most three rows, critical and warning only; notes go to the Journal page. |
| One detail panel for whatever is selected | RimWorld: an inspect pane shows details for the selected pawn, item or structure ([wiki](https://rimworldwiki.com/wiki/User_interface)). Timberborn: Object Detail "shows detail on the selected object". Manor Lords: hovering a building in the construction menu shows cost and requirements. | One right column that holds either the inspector or one ledger page or one Live workspace. |
| Management screens are opened, not docked | Timberborn: Manage Settlement ("various lists for the colony") and Population Management. Banished: Tools and Reports on F2 holds professions, limits and statistics; the Town Hall adds production, inventory and graphs ([defkey](https://defkey.com/banished-city-building-shortcuts), [fandom](https://banished.fandom.com/wiki/Town_hall)). Oxygen Not Included: Vitals, Consumables, Priorities tabs ([guide](https://www.gamepressure.com/oxygen-not-included/main-screen/z39cfa)). | The ledger is closed by default; five dock buttons and F1 to F5 open its pages. |
| Overlays as a toggled set | Oxygen Not Included: overlays on F1 to F11 from a panel at the top right ([wiki](https://oxygennotincluded.wiki.gg/wiki/Overlay)). Timberborn "View Layers". RimWorld toggles at bottom right ([wiki](https://rimworldwiki.com/wiki/User_interface)). | An Overlays button with a small list; the marker key becomes the legend of the active overlay. |
| Overview without a minimap | Timberborn: a Focus control scrolls through districts and, expanded, lists them all ([wiki](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen)). | A Places list (the three villages and "Whole island"). A minimap is deferred (section 13). |

### 2.2 Disclosure, tooltips and onboarding

| Pattern | Game and evidence | What this design takes |
|---|---|---|
| Tooltips in layers, with the deeper layer opt-in | Paradox's Victoria 3 UX diary describes nested tooltips so that anything can be explained without a long text in each tooltip, with a timer lock or an action lock; Crusader Kings 3 lets experienced players switch to the action lock ([dev diary 29](https://forum.paradoxplaza.com/forum/developer-diary/victoria-3-dev-diary-29-user-experience.1506484/), [PC Gamer](https://pcgamer.com/the-crusader-kings-3-team-is-working-hard-to-make-the-game-easier-to-learn)). A design write-up suggests a ceiling of two or three levels ([Ardeljan](https://philip.design/blog/tooltips-in-tooltips)). | Two levels only: a short one on a delay, a longer one on a further delay or while Alt is held. No tooltips inside tooltips. |
| A help system that replaces pop-ups | RimWorld's learning helper replaces the yellow pop-up messages and learning alerts and can be switched off ([wiki](https://rimworldwiki.com/wiki/Learning_helper)). Farthest Frontier added small tutorials tied to individual interface options, then a Guided Journey ([80.lv](https://80.lv/articles/farthest-frontier-s-first-major-update-brings-gridless-build-mode-more)). Dwarf Fortress's Steam release added a tutorial, mouse support and tooltips ([PC Gamer](https://www.pcgamer.com/the-new-dwarf-fortress-ui-looks-so-much-better)). | A goal tracker in Live (exists as guidance), four first-run coach marks in Oversee, and a Guide screen. All can be switched off. |
| Rework for readability, applied everywhere at once | Against the Storm: the old UI had become a mix of styles, readability complaints and new-player problems drove the rework, heavy ornament was removed, players then missed the "'handcrafted', warm feeling" and texture was added back, and the team applies changes across as many elements as possible so that old and new do not mix ([devlog, 26 April 2022](https://eremitegames.com/?p=236833)). Foundation's 1.7 update reworked its menus, icons and HUD elements together ([Polymorph news](https://www.polymorph.games/foundation/news/)). | Tokens and shared widgets first (phase 1), then every surface; no screen keeps the old palette. |
| Mouse-first, with the cost of losing separation | Dwarf Fortress: reviewers welcomed mouse control and tooltips and noted that the new UI lost some of the old separation between play space and menu information ([review](https://www.pcgamer.com/dwarf-fortress-review/)). | Panels have fixed places; nothing floats over the middle of the world. |
| Controller as a redesign, not a mapping | Frostpunk's console edition rebuilt menus around radial selection and rescaled the HUD ([Xbox Wire](https://news.xbox.com/en-us/2019/09/20/frostpunk-console-edition-radial-driven-design-of-gameplay-and-controls/)). | This design stays keyboard and mouse, and keeps the rules that make a later pad pass possible (section 8.8). |

### 2.3 Character-mode HUD

| Pattern | Game and evidence | What this design takes |
|---|---|---|
| Date, season and time in the top right; energy and health as bars in the bottom right; a toolbar along the bottom | Stardew Valley: date and season HUD top right with a dial for the time of day; energy "on the bottom right-hand corner", health beside it; the energy bar shakes when low ([getting-started guide](https://www.thegamer.com/stardew-valley-getting-started-guide/), [wiki, Energy](https://stardewvalleywiki.com/Energy_Bar), search summaries; the wiki page refused an automated read). Toolbar and journal placement: own reading. | The shared top bar already holds date and time top right. Needs sit bottom left (the bottom right belongs to the right column), activities are a hotbar bottom centre, the goal is top left. |
| A prompt only when something can be used | Common to the genre; Xbox Accessibility Guideline 101 shows a button glyph beside the verb ("Collect") as the readable form ([XAG 101](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/101)). | One pill above the hotbar: key cap, verb, object. Hidden when nothing is in reach. |

### 2.4 Accessibility

| Rule | Source | Use here |
|---|---|---|
| Body text contrast at least 4.5:1; large text and graphical elements at least 3:1 | XAG 102 ([link](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/102)); the same thresholds as WCAG 2.1 success criteria 1.4.3 and 1.4.11 ([W3C](https://www.w3.org/TR/WCAG21/), not re-read in this pass). | Section 6.2 lists every text and control pair with its ratio. |
| No essential information by colour alone; back colour with "text or a symbol, pattern or shape"; red and green difficulty affects "around 8-10% of males" | Game Accessibility Guidelines ([link](https://gameaccessibilityguidelines.com/ensure-no-essential-information-is-conveyed-by-a-colour-alone/)). | Four status shapes (section 6.3), arrows for trends, a tick or a cross and a sentence on the ghost. |
| Default text on PC at least 18 px at 1080p (ascender to descender), scalable to 200% without loss; icons scale with text | XAG 101 ([link](https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/101)). | Automatic UI scale by window height (section 5.9): 14 px body becomes 21 px at 1080p. The player's 80 to 140% setting multiplies it. |
| Sentence case, left-aligned blocks, lines of at most 80 characters | XAG 101. | Copy rules (section 10); tooltips are at most 340 px wide. |

Not consulted in this pass and worth the implementer's time: the Game UI Database
(<https://www.gameuidatabase.com/>) for side-by-side screens of the games above.

## 3. Principles and goals

1. **The island is the picture.** Chrome is two thin bars. Everything else opens
   because the player asked or selected something, in one fixed place, and closes
   again.
2. **Always: state. One click: detail. Hover: explanation.** Totals and time are
   always visible. Lists and tables are one click away. Why and how are in tooltips
   and the Guide, never in permanent hint text.
3. **One right column.** The inspector, a ledger page and a Live workspace are the
   same panel in the same place. Only one is open.
4. **The same place means the same kind of thing in every mode.** Top bar: mode,
   totals, time. Top left: what needs attention. Bottom centre: the mode's tools.
   Right: detail.
5. **Colour never speaks alone.** Status is a shape, a colour and, where there is
   room, a word. Keys are key caps. Icons carry a label wherever a glyph alone
   could be misread.
6. **Wood outside, paper inside.** Always-visible chrome is dark warm brown so that
   it recedes by day and does not glare at night. Things that are opened and read
   are parchment with dark ink.
7. **Real numbers, real reasons.** A disabled control keeps its place and states
   what is missing. A refusal says what was refused and why. The UI never implies
   a grant, a progress or a certainty the simulation does not have.

Experience goal per mode:

- **Oversee**: "I can see how the villages are doing at a glance, find what needs
  me in one click, and place a building in three." The camera view is the main
  instrument; the ledger is a reference book.
- **Live**: "I am a person here." Needs, a hotbar, one goal and the thing in reach.
  No management chrome. Workspaces feel like opening a pouch or standing at a bench.
- **Shape**: "A small editor." Tools on a rail, brush settings in one bar, undo
  beside the tools, every refusal explained at the cursor.

## 4. Information architecture

### 4.1 Visibility

| | Oversee | Live | Shape |
|---|---|---|---|
| **Always visible** | Menu; mode switch; residents and beds; morale; food; clean water; logs; planks; civic fund; weather; date and time; speed; alert counts; build dock; ledger dock; Overlays; Places | Menu; mode switch; weather; date and time; speed; five needs; coins; hotbar; current goal (title and step) | Menu; mode switch; weather; date and time; speed; tool rail with undo and redo; brush bar |
| **One click or key** | Ledger pages: Village, Economy, People, Projects, Journal; build tray; overlay list; places list; calendar and forecast; expanded alerts | The seven workspaces: Pouch, Craft, Build, Garden, Trade, People, Explore; calendar; goal detail; pause menu | Surface choice; pause menu; calendar |
| **Contextual** | Inspector (resident, building or site selected); placement ghost and cursor tag; highlighted totals while placing; paused pill; toasts; coach marks; tooltips | Prompt for the thing in reach; low-need markers; critical village alerts as toasts; build preview on a claimed site; death and successor choices; save toast | Brush outline and cursor tag; refusal reason; history depth in the undo tooltip |

### 4.2 Screen map

```
Title
 |- Continue ----------------------------------------------+
 |- New game -> A new beginning (name)                     |
 |     |- Start on the island -----------------------------+
 |     '- Oversee the village -----------------------------+
 |- Oversee the village (only with a save) ----------------+
 |- Settings: Sound | Display | Controls | Accessibility   |
 |- How to play (Guide)                                    |
 '- Quit                                                   v
                                                        In play
 +---------------------------- top bar (all modes) ---------------------------+
 | Menu | Live 1 | Oversee 2 | Shape 3 | totals | weather | date/time | speed |
 +-----------+---------------------------+------------------------+-----------+
             |                           |                        |
           Live                       Oversee                   Shape
   goal tracker (top left)     alerts (top left)         tool rail (left)
   needs (bottom left)         build dock -> tray        brush bar (bottom)
   hotbar -> workspace           -> ghost + cursor tag
     Pouch Craft Build         ledger dock -> ledger
     Garden Trade People         Village Economy People
     Explore                     Projects Journal
   prompt (above hotbar)       inspector (on selection)
   death -> successor choices  Overlays, Places (bottom left)

 From any mode: Menu or Esc -> Pause
   Resume | Save game | Settings | How to play | Save and go to title | Save and quit
 From the date chip: Calendar (season, sunrise and sunset, next season, forecast)
```

New-world naming stays as documented in `docs/island-life.md`: Live, Oversee,
Shape, Pouch, Ledger (short for "Village ledger"), Calendar.

## 5. Layout

Coordinates are logical pixels at 100% UI scale, origin top left. The mockup frames
use exactly these rectangles.

### 5.1 Shared regions (1280×720)

| Region | Rect (x, y, w, h) | Notes |
|---|---|---|
| Top bar | 0, 0, 1280, 44 | Full width, `chrome` surface, 1 px bottom border. |
| Attention column | 8, 52, 316 wide | Alerts in Oversee, goal tracker in Live. Grows downward. |
| Right column | right edge 8, top 52, bottom 648 | 344 wide (inspector), 440 (ledger), 360 (one-pane workspace), 600 (two-pane workspace). Height 596. Never two at once. |
| Bottom docks | y 656, h 56, 8 from the edges | Left dock, centre dock, right dock. |
| Toasts | centred, y 52, h 36 | At most three, newest at the top. |
| Tooltips | beside the anchor | Above every panel. |

### 5.2 Oversee, nothing selected

```
0   +------------------------------------------------------------------------------+
    |[=] [Live|OVERSEE|Shape] | 42/84  97%  114  104  105  11!  0 fund    [Clear] [Autumn, day 6  07:30] [||][1x][3x][10x]|
44  +------------------------------------------------------------------------------+
52  |+-Alerts  #1 !2   Hide-+                                                      |
    ||# Bram is starving    |                                                      |
    ||! Harbour cottage 3...|                                                      |
    ||! Clean water is low..|                       WORLD                          |
    |+ Day 20 . 36 notes  F5+                                                      |
    |                                                    (right column is empty)   |
    |                                                                              |
656 |+Overlays Places+        +Build | Homes Work Storage Market Farms+   +Village Economy People Projects Journal+|
712 +------------------------------------------------------------------------------+
```

- **Top bar**, left to right: Menu (32×32 icon button); mode switch (segmented,
  each segment icon + label + key cap); divider; totals; flexible space; weather
  chip; date chip (season icon, "Autumn, day 6", time, a 3 px day-progress line);
  speed (pause, 1×, 3×, 10×).
- **Totals** (each 28 high, icon 16 + number 14 px): residents with beds ("42 / 84"),
  morale, food portions, clean water, logs, planks, civic fund. A total gains an
  amber triangle when it is below one day's use or below what open sites need, a
  small arrow when it moved by more than 10% since yesterday. Click opens the
  matching ledger page (residents: People; morale: Village; goods: Economy with the
  row selected; fund: Projects).
- **Alerts**: header (32 high: "Alerts", a diamond with the critical count, a
  triangle with the warning count, Hide or Show). Rows are 44 high with a 4 px
  severity edge, an icon, a title, one line of detail and a locate marker; click
  frames the place and selects the resident or site (unchanged behaviour). At most
  three rows, then "+2 more". The last line links to the Journal with the day's
  note count. With no alerts the header reads "No alerts" and has no rows.
- **Left dock** (8, 656, 144×56): Overlays (key O), Places (key M).
- **Build dock** (440, 656, 401×56): a "Build" caption with key cap B, then Homes,
  Work, Storage, Market, Farms. Categories come from `village::civic::catalog`;
  an empty category is hidden, not disabled.
- **Ledger dock** (924, 656, 348×56): Village F1, Economy F2, People F3, Projects
  F4 (with a count badge of open sites), Journal F5.
- **Overlays** opens a 220 px popover above its button: Trades (the existing marker
  colours, now with trade icons), Homes and beds, Open jobs, Resource patches,
  Building grid. One overlay at a time. The legend of the active overlay sits above
  the left dock (the place of today's marker key). Only Trades exists today; the
  others are listed so that the control does not need redesign when they land.
- **Places** opens a list: Whole island, Harbour, Alderwood, Stone Ridge. Choosing
  one moves the camera. View only.

### 5.3 Oversee, resident selected

```
52  |+-Alerts #1 !2 Show-+                                      +--------------------+|
    |                                                          |(o) Ada      [F] [x] ||
    |                                                          |    Farmer . Harbour ||
    |                 [ Ada . gathering fiber ]                |(o) Doing fine  Gathering fiber... |
    |                        (o)                               |Overview|Day|Why|Home and job|
    |                                                          | Food    ====== 96%  ||
    |                                                          | Water   ====== 95%  ||
    |                                                          | Warmth  ==    !41%  ||
    |                                                          | Rest    =====  86%  ||
    |                                                          | Morale  ====== 90%  ||
    |                                                          |Traits  [..][..][..] ||
    |                                                          |Carrying [220][2][2].||
    |                                                          |Why now: one sentence||
    |                                                          |Home  Harbour cottage 1 >|
    |                                                          |Work  Harbour farmstead >|
648 |                                                          |[Change job or home]  Esc Close|
656 |+Overlays Places+   +Build | Homes ...+                   +Village Economy People ...+|
```

- Inspector: 928, 52, 344×596. Header 64 high: a 44 px avatar ring in the trade
  colour with the trade icon, name at 20 px, "Farmer · Harbour" at 13 px, Follow
  (toggle) and Close.
- A status line: a badge with shape and word ("Doing fine", "In need: warmth",
  "In danger: food") and the activity sentence from `activity_label`.
- Tabs: **Overview** (needs, traits, carrying, a one-sentence "Why now" with a link
  to Why, links to home and workplace). **Day** (the schedule strip with a marker
  for now, and the next changes). **Why** (four headed blocks: Chose, Passed over,
  Checked and fine, Plan; the same text the pure `why` function produces today,
  split at its existing sentence boundaries). **Home and job** (the existing job
  and home commands, which today sit in the roster). A **Family** tab is reserved
  for when households and ages land.
- A need below its act-on level gets an amber fill and a triangle beside the
  number; below its interrupt level a red fill and a diamond.
- The alert stack folds to its header while the inspector is open (existing rule).
  Esc, Close or a click on open ground closes it. A click on another resident
  switches. Clicking Home or Work selects that building, and the same column shows
  the building card (header, then Overview, Residents or Workers, Stock tabs as
  apply).
- A world tag over the selected resident repeats the status shape, the name and
  the activity.

### 5.4 Oversee, build

```
    |          [ghost]  +-----------------------------+                                |
    |          [ghost]  |v You can build here         |                                |
    |          [door]   |Cottage  8 planks 4 logs 4x5 m|                               |
    |                   |[Click] Place [R] Turn [Esc] Put down|                         |
500 |    +-Homes   Residents carry the materials and build...                    [x]-+ |
    |    |[Cottage *in hand*][Family house  ][Lodging house (n/a)] | Village stock    | |
    |    |[8 planks 4 logs  ][12 planks 6 logs]                    | Planks 11 !      | |
648 |    +-------------------------------------------------------------------------+ |
656 |+Overlays Places+   +Build | HOMES Work Storage Market Farms+   +Village ... +   |
```

- Choosing a category (or B) opens the **build tray**: 232, 500, 816×148, a panel
  above the build dock. Header 28: category icon and name, one sentence on how
  building works, Close.
- **Item cards**: 188×96, icon 24, name 14 px, cost as icon + number pairs, one
  line of purpose at 12 px. States: default; hover; in hand (honey fill, 2 px edge,
  an "In hand" badge with a tick); unavailable (dashed edge, reason in a sentence).
  More than three cards scroll horizontally with the wheel and with a rail.
- The right 196 px of the tray shows **village stock for the hovered or held
  building**: each material with the island total and a shape (circle enough,
  triangle short), and the civic fund's spendable coins.
- **Ghost**: unchanged geometry (cell outline, translucent body, door marker).
  Valid: teal fill and solid outline. Invalid: no fill, red dashed outline. The
  verdict is never colour alone, because the tag says it.
- **Cursor tag** (tooltip style, 250 wide, to the right of the cursor in the left
  half of the window and to the left in the right half, as today): line 1 a tick
  and "You can build here", or a cross and "Cannot build here" with the
  simulation's reason on line 2; line 2 or 3 the name, the cost and the footprint;
  last line key caps: Click Place, R Turn, Esc Put down.
- While a building is in hand the totals it draws on are outlined in honey in the
  top bar.
- Right-click, Esc or choosing the card again puts the building down. Esc again
  or B closes the tray. Selecting a resident closes the tray (existing rule).

### 5.5 Oversee, ledger (Economy shown)

```
52  |+-Alerts #1 !2 Show-+      [|| Paused  Space]          +--------------------------+|
    |                                                       |[book] Ledger          [x]||
    |                                                       |Village|ECONOMY|People|Projects|Journal|
    |                                                       |[All|Harbour|Alderwood|Stone Ridge] [Goods|Jobs]|
    |                                                       |Good        Stock  Price  Since yesterday|
    |                                                       |-- Food and water --      ||
    |                                                       |Bread         102     3   o steady     ||
    |                                                       |Clean water   104     1   / up 9       ||
    |                                                       |-- Materials --           ||
    |                                                       |Planks  !Low   11     2   / up 7  (selected)|
    |                                                       |...                       ||
    |                                                       |+ Planks: Harbour 6 . Alderwood 5 . Stone Ridge 0 +|
648 |                                                       |+ Made by 3 carpenters. Two sites still need 13.  +|
656 |+Overlays Places+   +Build | Homes ...+                +Village ECONOMY People ...+|
```

- Ledger: 832, 52, 440×596. Header 44: book icon, "Ledger", Close. Tabs 36 high
  with icons. Closed by default; F1 to F5, the dock, a top-bar total or F11 (last
  page) opens it. Opening the inspector replaces it and shows a "People" back link
  in the inspector header when the selection came from the roster.
- **Village**: stat chips in a 2-column grid (Residents, Housed, Beds, Morale,
  Food per resident, Water per resident), policies (food reserve as a stepper,
  gatherings as a toggle), a row per village that filters the other pages.
- **Economy**: a village filter (All, Harbour, Alderwood, Stone Ridge), a Goods or
  Jobs switch. Goods table columns: Good (icon + name + optional "Low" badge),
  Stock, Price, Since yesterday (arrow up-right, arrow down-right or a ring, plus
  "up 7", "down 2", "steady"). Rows 27 high, grouped: Food and water, Materials,
  Tools and clothing. Column headers sort. Selecting a row fills a detail card
  under the table (per-village stock, who makes it, what needs it). Jobs is the
  existing professions table with trade icons.
- **People**: the roster. Sort chips (Name, Place, Job, Mood, Wealth), the village
  filter, rows 44 high: status shape, name, job and village on line 1; activity,
  mood and coins on line 2. Selecting a row opens the inspector.
- **Projects**: the civic fund as two stat chips (to spend, held for open sites),
  one card per open site (progress bar, materials as icon pairs, the waiting
  reason, Abandon with the existing inline confirm), then "What the planner sees"
  and "Recent decisions" collapsed under headings.
- **Journal**: the alert feed's "Everything" view, grouped by day, with the three
  filters as a segmented control. Rows select and frame exactly as alert rows do.
- Scrolling: wheel, a 6 px rail on the right, Page Up and Page Down while the
  ledger has focus. The rail's thumb is focusable so the Tab ring still reaches it.

### 5.6 Live

```
0   +------------------------------------------------------------------------------+
    |[=] [LIVE|Oversee|Shape]                              [Clear] [Autumn, day 6 09:01] [||][1x][3x][10x]|
44  +------------------------------------------------------------------------------+
52  |+-Your first homestead   2 of 6-+     [v Island saved]                         |
    || Keep water in your pack       |                                              |
    || Draw clean water at the well..|                                              |
    || o (o) o o o o   [Show the way]|             [You]                            |
    |+-------------------------------+              (o)   [well]                    |
    |                                                                               |
    |+-Health ======+                                                               |
    || Food   ====  |                                                               |
    || Water  ==  ! |                  [E] Draw water from the well                 |
    || Energy ===== |                                                               |
    || Warmth ===== |                                                               |
656 || Coins     12 |   +Pouch Craft Build Garden Trade People Explore | Stop+      |
712 +------------------------------------------------------------------------------+
```

- **Goal tracker**: 8, 52, 300 wide, `chrome`. Caption (journey name, "2 of 6"),
  title 16 px, one or two lines at 13 px, progress pips (done filled, current a
  honey ring, to do hollow), one button that names what it does ("Show the way",
  "Open Build"). It is the existing `player_journey` projection; urgent needs still
  take its place ("You are thirsty" before any building step). It folds to one
  32 px line while a workspace is open, and can be collapsed by the player.
- **Needs**: bottom left, 216 wide: Health, Food, Water, Energy, Warmth as icon,
  label and an 8 px bar, then coins. Percentages appear in the tooltip, not on the
  bar. Below the act-on level a bar turns amber and gains a triangle; below the
  interrupt level red and a diamond, and the row pulses once every two seconds
  (motion can be switched off). Clicking a need opens Pouch filtered to what
  helps it.
- **Hotbar**: centred, y 656: Pouch I, Craft C, Build B, Garden G, Trade T, People
  N, Explore M, a divider, Stop X. Tiles are 64×44 with icon, label and key cap.
  The open workspace's tile is lit.
- **Prompt**: a pill centred at y 604, only while something is in reach: key cap,
  verb, object ("E Draw water from the well", "E Talk to Ada"). While the
  character is working it becomes a progress pill: "Sawing planks" with a bar and
  "X Stop".
- **Toasts**: top centre. Save confirmations, completed outcomes ("Cottage
  finished"), and the critical village alerts that Live already shows.
- **Workspace**: the right column. One pane (360 wide) for Pouch, Build, Garden,
  People and Explore. Two panes (600 wide) for Craft and Trade: the pouch on the
  left (276), recipes or offers on the right. Header: icon, name, a status badge
  ("At the Harbour carpenter's bench", "No workshop in reach"), Close.
  - Pouch: slots 56×56 with icon, count and a 12 px caption; a weight bar; a detail
    card for the selected item with its actions (Eat, Drink, Wear, Offer for sale).
    Home heat and fuel keep their row beside Rest.
  - Craft: recipe cards with output, duration in seconds at 1×, inputs as chips
    marked with a tick or a diamond and "you have N"; the button names the action;
    a disabled button keeps its place with the reason beneath. A "Can make / All"
    switch.
  - Build, Garden, People keep the selected-site-first pattern from ADR 0036.
- House interiors, when they land, reuse the Live HUD unchanged; the prompt and
  the workspaces are already independent of the camera.

### 5.7 Shape

```
52  |+------+                                                                       |
    ||Raise |                          . . . .                                      |
    ||Lower |                        . . . . . .   [^ Raise . ground at 6.3 m]      |
    ||Flatten                        . . . . . .                                    |
    ||Smooth|                          . . . .                                      |
    ||Paint |                                                                       |
    ||------|                                                                       |
    ||Undo  |                                                                       |
    ||Redo  |                                                                       |
    ||12/32 |                                                                       |
    |+------+                                                                       |
656 |              +Size [ ====o==== ] 4 m | Edge [Hard|SOFT] H | Surface [][][][]+ |
```

- **Tool rail**: 8, 52, 76 wide: Raise R, Lower F, Flatten T, Smooth G, Paint P,
  a divider, Undo, Redo, and the history depth ("12 of 32"). Tiles 64×44. Redo is
  dimmed when there is nothing to redo. Undo's tooltip carries Ctrl+Z and the
  history sentence.
- **Brush bar**: centred at y 656: Size (slider 0 to the world's maximum, key caps
  `[` and `]`, value in metres), Edge (Hard or Soft, key H), Surface (four swatches
  with names in tooltips, key V; dimmed unless Paint is the tool).
- **Cursor tag**: tool and the ground height under the cursor. A refused dab turns
  the brush outline red and dashed and adds the simulation's reason as a second
  line. Cell coordinates move to the developer overlay.
- The right column is free; placement palettes for resources, plots and props can
  use it later without moving anything.

### 5.8 Title, new game, pause, settings, save feedback

- **Title**: the live island behind a left-to-right bark gradient (82% to 0% over
  560 px), no box. Column at x 72, y 132, 360 wide: "Island Village" at 56 px, one
  line of tagline at 16 px, then buttons 320 wide: Continue (56 high, two lines:
  the word and "Ada Traveller · Autumn, day 6 · Harbour"), New game, Oversee the
  village, Settings and How to play side by side, Quit as a quiet button. With no
  usable save, Continue is absent and New game is the primary button. A credits
  line at the bottom left.
- **New game** ("A new beginning"): a parchment panel 440 wide in the same column:
  the name field (36 high, with the existing rules as helper text under it and the
  refusal reason in place), "Start on the island" (primary), "Oversee the village
  first" (secondary), "Back". If a save exists, one line: "Your current island
  will be kept as a backup."
- **Pause**: scrim over the world, a 440 px panel at 420, 128: "Paused" at 28 px;
  subtitle with character, season, day and "saved 2 minutes ago"; Resume (primary,
  key cap Esc), Save game (key cap Ctrl+S), Settings, How to play; a divider; "Save
  and go to title" and "Save and quit". The top bar is hidden under the scrim.
- **Settings**: the same panel, 560 wide, four tabs.
  - Sound: Master, Ambience, Effects as sliders with a percentage; Mute as a toggle.
  - Display: Interface size (Auto, or 80 to 140% as a slider with a live preview
    line), Display (Windowed or Borderless, segmented).
  - Controls: Camera pan speed (slider), Edge pan (toggle), a read-only list of
    keys (remapping remains future work).
  - Accessibility: Reduce motion (toggle), Tooltip delay (Short, Normal, Long),
    Show tips (toggle), Panel style (Parchment or Dusk, see 6.1).
  Changes apply at once, as today; there is no Apply button.
- **Save feedback**: a toast "Island saved" with a tick for a requested save
  (3 seconds). The minute's autosave stays silent. A failed save is a toast with a
  diamond, "Could not save" and the reason, a "Try again" button, and it stays
  until the next attempt (existing rule), in every mode.

### 5.9 Other window sizes and UI scale

Layout decisions use the **effective size**: window logical size divided by the UI
scale. Three classes:

| Class | Effective width | Changes from the 1280 layout |
|---|---|---|
| Wide | 1280 and more | As drawn. From 1440 two more totals appear (tools, warm clothes); the rest of the extra width goes to the world. |
| Standard | 1024 to 1279 | See below. |
| Compact | below 1024, or effective height below 700 | See below. |

**1024×768 (Standard):**

```
0   +----------------------------------------------------------------+
    |[=] [Live|OVERSEE|Shape] | 42  97%  114  104  0     [*] [Autumn, day 6 07:30] [||][1x][3x][10x]|
44  +----------------------------------------------------------------+
52  |+-Alerts--------+                              +---------------+|
    ||rows, 300 wide |           WORLD              | right column  ||
    |+---------------+                              | 320 / 400     ||
    |                                               | h 644         ||
696 |                                               +---------------+|
704 |+Ovl Plc+   +Build| 5 x 56+     +5 x 56 ledger buttons+         |
760 +----------------------------------------------------------------+
```

- Top bar: five totals (residents, morale, food, water, fund); logs and planks
  appear only while low or while a building that costs them is in hand. Key caps
  leave the mode switch (they stay in its tooltips). The weather chip is icon
  only. Budget: menu 32, modes 231, five totals about 290, weather 34, date 165,
  speed 126, gaps and padding 83: about 960 of 1024.
- Docks: tiles 56 wide. Left 126, centre 359 (x 332), right 306 (x 710).
- Right column: inspector 320, ledger 400, one-pane workspace 340, two-pane 560
  (pouch pane 256). Height 644.
- Build tray: 816 wide still fits (x 104).
- Live: hotbar 491 wide (x 266), clear of the needs panel (ends at 224).
- Title, pause and settings are fixed-size and centred or left-anchored.

**Compact** (for example 140% on a 1280×720 window, effective 914×514):

- Top bar: the mode switch shows icons, with the label on the active segment
  only; totals move to a second 28 px row; weather folds into the date chip.
- Docks: the ledger dock becomes one "Ledger" tile; build categories keep labels.
- Right column: 320 wide and scrolls; the alert stack shows its header only.
- Live two-pane workspaces become one pane with a "Recipes / Pouch" switch.
- Nothing is removed from the Tab ring.

**Larger windows and scale.** UI scale = automatic factor × the player's Interface
size setting (80 to 140%).

- Automatic factor = `clamp(round_down_to_0.05(min(width / 1280, height / 720)), 1.0, 2.5)`.
  1280×720 gives 1.0; 1024×768 gives 1.0 (effective 1024×768, Standard);
  1920×1080 gives 1.5 and 2560×1440 gives 2.0 (both effective 1280×720, Wide);
  3840×2160 gives 2.5 (effective 1536×864). 2560×1080 gives 1.5 (effective
  1707×720): bars stretch, docks stay centred, the extra width is world.
- This is what makes body text 21 px at 1080p (XAG 101 asks for 18 px or more).
  Today the scale is the setting alone, so a borderless 1080p window shows 14 px
  text. "Auto" is the proposed default (section 13, decision 4).
- Icons are drawn from a 32 px raster up to an on-screen size of 32 px and from a
  64 px raster above that (section 9.5), so they stay sharp to 2.5×.
- Windows DPI scaling is already folded into logical pixels by the window; the
  automatic factor works on logical size.

## 6. Design tokens

Names are the CSS custom properties of the mockup and the proposed Rust constants
(`BARK_900` and so on) in `V/game_ui/tokens.rs`.

### 6.1 Colour

| Token | Hex | Role |
|---|---|---|
| `bark-900` | `#2B2018` | Chrome surface (top bar, docks, tooltips, toasts). Drawn at 94% alpha over the world (`rgba(43,32,24,0.94)`). |
| `bark-800` | `#3A2C21` | Controls on chrome (segments, boxed chips, secondary buttons). |
| `bark-700` | `#4C3B2D` | Hover on chrome; key caps on chrome; the 2 px frame of parchment panels. |
| `bark-600` | `#6B5443` | Borders and dividers on chrome (decorative). |
| `parchment-100` | `#F5ECD7` | Panel surface. |
| `parchment-200` | `#ECDFC1` | Cards, rows, secondary buttons, bar tracks. |
| `parchment-300` | `#DFCEA8` | Hover and pressed on parchment; key caps on parchment. |
| `border-soft` | `#C8B48A` | Dividers and card edges on parchment (decorative). |
| `border-control` | `#8C744D` | Edges of controls on parchment (buttons, fields, bars, slots). |
| `ink-900` | `#2B1F16` | Text on parchment. |
| `ink-700` | `#55432F` | Secondary text on parchment. |
| `ink-500` | `#6A5642` | Captions and column headers on parchment. |
| `cream-50` | `#FBF5E6` | Text on chrome and on filled buttons. |
| `cream-300` | `#D8C8AA` | Secondary text on chrome. |
| `honey-500` | `#E8B04B` | Active segment or tile on chrome (with `ink-900` text); focus ring on chrome; progress on chrome. |
| `honey-200` | `#F8E3A8` | Selected row, card or slot on parchment. |
| `honey-700` | `#8A5A0B` | Edge of a selected item; active tab line. |
| `moss-600` / `moss-700` / `moss-800` | `#3F6B34` / `#33582A` / `#294A22` | Primary action: default, hover, pressed. |
| `sea-600` | `#1F6A8C` | Focus ring on parchment; in-panel links. |
| `scrim` | `rgba(24,16,10,0.55)` | Behind modals. |

**Dusk** (optional alternative panel style, for players who want dark panels or
find parchment bright at night): `parchment-100/200/300` become `#3A2C21`,
`#4C3B2D`, `#5A4636`; `ink-900/700/500` become `cream-50`, `cream-300`, `#BEAC8E`;
`border-soft` and `border-control` become `bark-600` and `#A08A70`; the status
colours switch to their `-light` set; `honey-200` becomes `#5C4A22` with a
`honey-500` edge; focus is `honey-500` everywhere. Chrome is unchanged. Because
every widget reads role tokens, Dusk is a table swap, not a second design.

Changed 2026-11-11 after implementation: the Dusk caption was `#B8A688` (4.49:1
on the Dusk card `#4C3B2D`, 4.5 needed) and the Dusk control edge `#8F7A62`
(2.6:1 there, 3 needed). They are now `#BEAC8E` (4.8:1 on the card, 6.1:1 on the
panel) and `#A08A70` (3.2:1 on the card, 4.1:1 on the panel).

### 6.2 Contrast (WCAG 2.1 relative luminance, computed for this palette)

| Foreground on background | Ratio | Needed |
|---|---|---|
| `ink-900` on `parchment-100` / `-200` / `-300` | 13.6 / 12.1 / 10.3 | 4.5 |
| `ink-900` on `honey-200` / `honey-500` | 12.6 / 8.2 | 4.5 |
| `ink-700` on `parchment-100` / `-200` / `-300` | 8.0 / 7.1 / 6.1 | 4.5 |
| `ink-500` on `parchment-100` / `-200` | 5.9 / 5.3 | 4.5 (on `parchment-300` it is 4.48, so `ink-500` is not used there) |
| `cream-50` on `bark-900` / `-800` / `-700` | 14.6 / 12.4 / 9.8 | 4.5 |
| `cream-300` on `bark-900` / `-800` / `-700` | 9.6 / 8.2 / 6.5 | 4.5 |
| `cream-50` on `moss-600` / `-700` / `-800` | 5.7 / 7.5 / 9.2 | 4.5 |
| `cream-50` on `crit-ink` (danger button) | 6.7 | 4.5 |
| `sea-600` link text on `parchment-100` / `-200` | 5.1 / 4.5 | 4.5 |
| `honey-500` focus ring on `bark-900` / `-800` | 8.1 / 6.9 | 3 |
| `sea-600` focus ring on `parchment-100` / `-200` | 5.1 / 4.5 | 3 |
| `border-control` on `parchment-100` / `-200` | 3.8 / 3.4 | 3 |
| `good-ink`, `crit-ink`, `info-ink` on `parchment-100` | 5.3 / 6.2 / 5.4 | 3 (4.5 as text: pass) |
| `good-ink`, `crit-ink`, `info-ink` on `parchment-200` | 4.7 / 5.5 / 4.8 | 3 (4.5 as text: pass) |
| `warn-ink` on `parchment-100` / `-200` | 3.6 / 3.2 | 3 (fills and icons only; never as text) |
| `good-light`, `warn-light`, `crit-light`, `info-light` on `bark-900` | 9.0 / 9.9 / 6.2 / 8.3 | 4.5 |
| the same on `bark-700` | 6.0 / 6.7 / 4.2 / 5.6 | 3 (icons and fills; `crit-light` is not used as small text on `bark-700`) |
| `cream-50` on chrome at 94% over white, sand, grass, sea | 12.3 / 12.7 / 13.1 / 13.4 | 4.5 |

Rules that follow from the table: text on parchment is always an ink token; amber
is never text; bar tracks are `parchment-200` (not `-300`) so that every status
fill keeps 3:1 against its track; `border-soft` and `bark-600` are decoration and
never the only edge of a control.

A unit test in `tokens.rs` recomputes these ratios from the constants and fails
below the "Needed" column, so a later palette edit cannot quietly break them.

### 6.3 Status: colour plus shape

| Meaning | Shape (Maki glyph) | On parchment | On chrome | Words used |
|---|---|---|---|---|
| Fine, enough, valid, done | circle (`circle`); a tick for "done" and "can" | `good-ink #0F6E5A` | `good-light #63D6C0` | "Doing fine", "steady", "You can build here" |
| Low, in need, warning, waiting | triangle (`triangle`, or `caution` at 20 px and above) | `warn-ink #B26A00` | `warn-light #F7C64A` | "Low", "In need: warmth", "waiting for planks" |
| Danger, critical, refused, missing | diamond (`diamond`); a cross for "cannot" | `crit-ink #A3262E` | `crit-light #FF7A66` | "In danger: food", "Cannot build here", "you have 0" |
| Note, information | square (`square`), or `information` for hints | `info-ink #2A5DB0` | `info-light #9DBDF5` | "36 notes", "Saw planks first" |
| Unchanged, empty, to do | ring (`circle-stroked`) | `ink-500` | `cream-300` | "steady", empty pip |
| Trend | `arrow` turned 45° up or down | good or crit | good or crit | "up 7", "down 2" |

Colour-vision check (Machado 2009 simulation at full severity, CIE76 distance
between simulated colours; unsimulated pairs are 45 or more apart): on chrome the
closest pairs are fine and note for tritanopia (ΔE 24) and fine and danger for
protanopia (29); on parchment they are fine and note for tritanopia (13), fine and
danger for protanopia (16) and for deuteranopia (30). These hues
were chosen to keep a lightness and a blue-yellow difference between the green and
the red, but a green and a red of similar lightness cannot be made safe by hue
alone, which is why the shape is mandatory and the word is added wherever there is
room. Trade marker colours (the existing thirteen) are identity, not status; with
icons the trade is also shown by its glyph.

### 6.4 Type

One family, the bundled Barlow Regular (`assets/fonts/Barlow-Regular.ttf`). Only
the Regular weight ships, so hierarchy comes from size and ink level, never from
synthetic bold.

| Token | Size / line | Use |
|---|---|---|
| `display` | 56 / 60 | Title screen name. |
| `title` | 28 / 34 | Menu and modal titles. |
| `heading` | 20 / 26 | Panel titles, resident name. |
| `subheading` | 16 / 22 | Card titles, goal title, large buttons. |
| `body` | 14 / 20 | Default text, buttons, rows. |
| `dense` | 13 / 18 | Tables, secondary lines, chips, tooltips. |
| `caption` | 12 / 16 | Captions, key caps, badges, dock labels. Nothing is smaller. |

Numbers in tables are right-aligned. Text blocks are left-aligned and at most 80
characters wide (tooltips 340 px, panels 440 px).

Barlow Regular as bundled has no arrows, ticks, triangles or other symbols. Its
usable non-letter glyphs are `× · • − … ‹ › ° ½` and a few more (checked against
the font's character map). Every arrow, tick and shape in this design is therefore
an icon or drawn in code, never a text character. A string that contains a
character the font lacks would fall back to another face or to a box.

### 6.5 Spacing, size, radius, elevation, layers, motion

| Group | Tokens |
|---|---|
| Space | 2, 4, 6, 8, 12, 16, 24, 32. Panel padding 12; card padding 8 or 10; gap between related controls 4 or 8; between groups 12 or 16; screen margin 8. |
| Control heights | Button small 28, medium 36, large 44; icon button 32; dock tile 64×44 (56 wide at Standard); segment 30 on chrome, 26 on parchment; chip 28 on chrome, 24 on parchment; text field 36; list row 28 (one line) or 44 (two lines); table row 27; tab 36. |
| Pointer targets | At least 28 high and 32 wide; small buttons inside rows extend their hit area to 32. |
| Icons | 12 (inside badges), 14, 16 (default, with text), 20 (dock tiles), 24 (cards, slots), 32 (avatars, large). |
| Radius | `r-sm` 4 (key caps, bars, swatches); `r-md` 6 (buttons, chips, cards, rows, tooltips); `r-lg` 10 (panels, docks); pill = half the height (toasts, badges). |
| Borders | Chrome 1 px `bark-600`; panel 2 px `bark-700`; control 1 px `border-control`; selected 2 px `honey-700`; alert row 4 px left edge in the severity colour. |
| Elevation (`BoxShadow`) | `shadow-1` 0 1 2 at 30% (world tags); `shadow-2` 0 4 12 at 35% (panels, docks, tooltips); `shadow-3` 0 12 32 at 45% (modals). Shadow colour `#18100A`. |
| Layers (`GlobalZIndex`) | World tags 10; chrome and docks 30; panels and trays 32; popovers 34; tooltips and cursor tags 40; toasts 42; scrim 50; modal 52. Today's values (26 to 36) move to these names. |
| Motion | Hover colour 80 ms; press 60 ms; panel or tray open 160 ms (fade and 8 px slide from its edge); close 120 ms; tooltip fade 100 ms; toast in 200 ms, stays 3 s (6 s for alerts), out 400 ms; modal 200 ms with the scrim; low-need pulse 600 ms every 2 s. All ease-out. "Reduce motion" turns slides and pulses off and keeps fades at 80 ms. Animation is view time, never simulation time. |

## 7. Components

All are plain Bevy UI nodes. "On chrome" and "on parchment" are the two contexts;
a widget takes its colours from the context, not from its caller.

| Component | Anatomy and size | States |
|---|---|---|
| **Panel** | `parchment-100`, 2 px `bark-700` frame, `r-lg`, `shadow-2`, padding 12. Header 44: icon 20, title `heading`, Close icon button. | Open, closed. Never draggable. |
| **Chrome bar / dock** | `bark-900` at 94%, 1 px `bark-600`, `r-lg` (the top bar has no radius), `shadow-2`. Dock padding 5, gap 4. | n/a |
| **Card** | `parchment-200`, 1 px `border-soft`, `r-md`, padding 8 or 10. | Default; hover `parchment-300`; selected `honey-200` with 2 px `honey-700`; unavailable: transparent with a dashed `border-soft` edge and `ink-500` content. |
| **Button, primary** | `moss-600`, `cream-50` label, optional key cap at the right. One per view at most. | Hover `moss-700`; pressed `moss-800`; focus ring; disabled as below. |
| **Button, secondary** | Parchment: `parchment-200`, 1 px `border-control`, `ink-900`. Chrome: `bark-800`, 1 px `bark-600`, `cream-50`. | Hover `parchment-300` / `bark-700`; pressed `border-soft` / `bark-900`; focus ring. |
| **Button, quiet** | No fill, `ink-700` / `cream-300` label. For Back, Put down, Hide. | Hover adds the secondary hover fill. |
| **Button, danger** | `crit-ink` fill, `cream-50` label. Only inside a confirm. | Hover darkens 8%. |
| **Disabled (any button)** | `parchment-200` fill, `border-soft` edge, `border-control` label on parchment; `bark-800` and `bark-600` on chrome. Stays in the Tab ring. The reason is a `caption` line with the information icon directly under or beside the button, and is the button's tooltip. | n/a |
| **Focus ring** | 2 px outline, 2 px offset (`Outline`), `sea-600` on parchment, `honey-500` on chrome. Shown for keyboard focus only (as today: a mouse click on the bar or a dock does not take focus). | n/a |
| **Icon button** | 32×32, secondary colours, icon 16. Always has a tooltip with its name and key. Toggle on: `honey-500` fill, `ink-900` icon. | Hover, pressed, focus, on. |
| **Dock tile** | 64×44 (56×44 Standard): icon 20 over a `caption` label, key cap 14 high in the top right corner, optional count badge. | Hover `bark-700` with edge; active `honey-500` with `ink-900`; focus ring; disabled `bark-600` content. |
| **Segmented control** | Joined segments, 30 high on chrome, 26 on parchment; one is on. On chrome the on segment is `honey-500` with `ink-900`; on parchment it is `bark-700` with `cream-50`. | Hover on the others; focus ring on the focused segment. |
| **Tabs** | 36 high, `dense` label with optional icon 14; active: `ink-900`, `parchment-200` fill, 3 px `honey-700` line beneath. Left and Right move between tabs when one is focused. | Default `ink-700`; hover `ink-900`; active; focus ring. |
| **List row** | 28 or 44 high: status shape 16, text, right-aligned secondary text, optional chevron arrow. Divider `border-soft`. | Hover `parchment-200`; selected `honey-200` with a 3 px `honey-700` left edge; focus ring. |
| **Table** | Header 28 (`ink-700`, 1 px `border-control` beneath, sort arrow on the sorted column); rows 27; group rows 24 (`parchment-200`, `ink-500` caption). First column icon + name. | Row hover, row selected (`honey-200`). |
| **Stat chip** | Chrome: icon 16 + number `body` + optional unit `caption` + optional shape or trend 12; 28 high. Parchment: same at 24 high inside a 1 px `border-soft` box. A boxed chip on chrome (weather) uses `bark-800`. | Hover shows the tooltip and a `bark-800` fill; "in use" (while placing) a 1 px `honey-500` edge. |
| **Badge** | 20 high pill: shape 12 + word or count, `caption`. Count badge on a tile: 16 high, `honey-500`, `ink-900`. | n/a |
| **Key cap** | 18 high, min 18 wide, padding 0 4, `r-sm`, `caption`. Parchment: `parchment-300`, `border-control` edge, `ink-700`. Chrome: `bark-700`, `bark-600` edge, `cream-300`. On a honey or moss fill: a 14% tint of the text colour with a 45% edge. Combinations are one cap ("Ctrl+S"). Mouse actions are a cap with the word ("Click", "Right-click", "Wheel"). | n/a |
| **Progress bar** | 10 high, track `parchment-200` with `border-control` edge (or `bark-700` on chrome), fill `bark-700` (or `honey-500` on chrome), value text at the right ("200 / 600"). | n/a |
| **Need bar** | 8 high, same track; fill `good`, `warn` or `crit` by threshold; a triangle or diamond 12 after the bar when not fine. Label and icon before it. | Fine, low, critical. |
| **Tooltip** | `bark-900` solid, 1 px `bark-600`, `r-md`, `shadow-2`, padding 8 10, `dense`. Level 1: title line (icon, name, optional value, key cap at the right) and one sentence; max width 280. Level 2 adds a block under a 1 px divider in `cream-300`, max width 340, with facts and the available actions as key caps. Never contains a control. | Level 1, level 2. |
| **Cursor tag** | A tooltip that follows the cursor (placement, brush). Line 1 is a verdict with a tick or a cross. A refused state has a `crit-light` edge. | Valid, refused. |
| **World tag** | `bark-900`, `r-md`, `dense`, `shadow-1`: shape, name, activity. Over the selected resident and over open sites that are waiting. | n/a |
| **Toast** | 36 high pill on chrome: shape or tick 16, one sentence, optional small button. | Info, success, failure (`crit-light` edge, stays until resolved). |
| **Inline confirm** | Replaces the row or card it belongs to: a question, one line of consequence, a secondary "keep" button and a danger button that repeats the verb ("Abandon it"). Used for abandoning a site. | n/a |
| **Modal** | Scrim, panel 440 wide with `shadow-3`, title `title`, body, buttons right-aligned or stacked. Used for pause, new game over an existing save, and quitting with a failed save. Focus is trapped inside; Esc is the safe choice. | n/a |
| **Slider** | Track 6 high, 132 wide or more; thumb 16 round; value text at the right; Left and Right step it, Shift for ten steps. | Hover, focus ring on the thumb, disabled. |
| **Toggle** | 36×20 pill, thumb 14. Off: `parchment-300` with `ink-500` thumb at the left. On: `moss-600` with `cream-50` thumb at the right. Position, not colour, shows the state; the label names the thing, not the state. | Off, on, focus, disabled. |
| **Text field** | 36 high, `cream-50` fill, 1 px `border-control`, padding 0 10; helper or error line beneath in `caption`. | Focus ring; error with a diamond and the reason. |
| **Scroll rail** | 6 wide, `parchment-300` track, `border-control` thumb, 4 from the panel edge. Appears only when the content overflows. Thumb is draggable and focusable. | Hover thumb `ink-500`. |
| **Item slot** | 56×56, `parchment-200`, 1 px `border-control`, icon 24, count at the bottom right, `caption` name beneath. Empty: dashed `border-soft`. | Hover, selected (`honey-200`, 2 px `honey-700`), focus. |
| **Goal tracker** | Chrome card 300 wide: caption line, `subheading` title, `dense` text, pips, one small button. | Expanded, folded (32 high), collapsed by the player. |
| **Popover** | A small panel anchored to its button (calendar, overlays, places), `shadow-2`, closes on Esc, a second click or a click outside. | n/a |
| **Empty state** | One sentence that says what is absent and what would fill it, and one button if there is an action: "No building is under way. Choose Homes below to place one." | n/a |

## 8. Interaction rules

### 8.1 Selection

- Left click on a resident, a building or a site selects it and opens the
  inspector. Residents take precedence over the building under them (existing).
- Left click on open ground clears the selection and hands the keyboard back to
  the camera. A click on any panel never reaches the world.
- A selection made from a list (roster, alert, project row) also frames the
  camera on it. Follow is a toggle in the inspector header and ends on a manual
  pan (existing).
- Selecting changes nothing in the simulation.

### 8.2 Hover and tooltips

- Hover changes the fill within 80 ms. Hovering a world object in Oversee shows
  its world tag after 300 ms.
- Tooltip level 1 after 400 ms of rest (Short 200, Long 800). Level 2 after a
  further 1.2 s of rest on the same element, or at once while Alt is held. Moving
  to another element within 300 ms of a tooltip closing shows the next one at
  once.
- Keyboard focus shows level 1 after 600 ms; Alt shows level 2.
- Everything a tooltip says is also reachable without hover: in the panel the
  element opens, or in the Guide. Tooltips never hold controls.
- Tooltips stay inside the window and never cover their anchor.

### 8.3 Keys

Key caps show the binding beside the control; no label contains a key. Existing
keys keep working. Proposed additions are marked.

| Scope | Keys |
|---|---|
| All modes | 1, 2, 3 modes; Space pause; `-` and `=` speed; `.` one tick while paused; Esc ladder (8.4); Ctrl+S and F9 save; Tab and Shift+Tab focus; Enter or Space activate; Alt tooltip detail (new). |
| Oversee | WASD or arrows pan; Q and E orbit; wheel zoom; middle drag orbit and tilt; B build tray; R turn the building in hand; F1 to F5 ledger pages (new); F11 ledger, last page (kept); O overlays (new); M places (new); F follow the selected resident (new). |
| Live | Unchanged: WASD, E, Q and R orbit, I C B G T N M workspaces, F2, X stop, L life choices. |
| Shape | Unchanged: R F T G P tools, V surface, `[` `]` size, H edge, Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z. |

F1 to F5 are free in the player session because the developer keys are already
gated out of it (`DeveloperKeys`). If the owner prefers letters, the fallback is
V, G, P, J, L (section 13, decision 3).

### 8.4 Esc, one step per press

1. Give keyboard focus back to the world if a control has it.
2. Close a popover (calendar, overlays, places).
3. Put down the building in hand, or cancel the stroke in progress.
4. Close the build tray.
5. Close the right column (inspector, ledger or workspace) and clear the selection.
6. Open the pause menu. In the pause menu, Esc resumes.

### 8.5 Right-click, drag, wheel

- Right-click: in Live, walk to the point (existing). In Oversee, put down the
  building in hand (existing); otherwise nothing. No context menus in this pass.
- Drag: Shape strokes; slider thumbs; the scroll rail thumb; middle-drag camera.
  No dragging of items or windows.
- Wheel: zoom over the world; scroll over a panel or the build tray (existing rule
  in Live, extended to every panel).

### 8.6 Focus order

The Tab ring (`V/island_shell/focus.rs`) stays the single keyboard path. Regions
in order:

- Oversee: top bar (menu, modes, totals, weather, date, speed); alerts; right
  column; build tray; left dock; build dock; ledger dock.
- Live: unchanged entry (Tab opens Pouch; Shift+Tab from the world reaches the top
  bar); inside a workspace: header, left pane, right pane; then the hotbar.
- Shape: top bar; tool rail; brush bar.
- A modal traps focus. Closing a panel returns focus to the control that opened it.
- Within a region the order is reading order. Left and Right move inside segmented
  controls, tab strips and the docks.

### 8.7 What pauses the game

| Pauses | Does not pause |
|---|---|
| Title and new-game screens; the pause menu and everything opened from it (Settings, How to play); the death panel as it does today | The ledger, the inspector, Live workspaces, the build tray and placement, the calendar, tooltips, toasts, coach marks, Shape tools |

Coach marks are passive (no modal, no pause). The player pauses with Space, and
the paused state is always shown twice: the lit pause segment and a "Paused" pill.
Commands queued while paused apply on resume or step (existing).

### 8.8 Gamepad readiness

No pad support is added. These rules keep it possible:

- Every action is reachable through the focus ring; nothing is hover-only.
- Focus regions have a spatial order, so a later pass can map the D-pad to "next
  in direction" and the shoulder buttons to "next region".
- Key hints are produced by one function from a binding name to a key cap. A pad
  build swaps the glyph table. Kenney's Input Prompts (CC0, 1,500 files) is the
  candidate glyph set for that pass and would need its own approval
  (<https://kenney.nl/assets/input-prompts>).
- Targets are at least 28×32; lists never need a precise pointer.

### 8.9 UI scale

One setting, "Interface size": Auto (default, proposed) or 80 to 140%. It scales
everything: text, icons, spacing, world tags. It applies at once and is stored in
`island-settings.json` as today. Layout class is recomputed from the effective
size when the window or the setting changes.

## 9. Iconography

### 9.1 Decision

Approved by the owner: use CC0 icons. Chosen: **Maki** as the base set and
**Temaki** as its supplement. Both are 15×15 single-colour SVG pictograms on the
same grid; Temaki's README presents it as following Maki's design guidelines, and
it is the one with the colony and craft glyphs (anvil, bread, needle and spool,
bow, tree stump, garden bed, scaffold, hand pump, pickaxe, campfire).

| Set | Source | Licence, as stated by the source | Pin |
|---|---|---|---|
| Maki | <https://github.com/mapbox/maki> | `LICENSE.txt`, first line: "CC0 1.0 Universal" (<https://raw.githubusercontent.com/mapbox/maki/main/LICENSE.txt>). GitHub labels the repository "CC0-1.0 license". | tag `v8.0.0`, commit `070ed0af1012da8dee83593bb0a33e4592ed68f5` |
| Temaki | <https://github.com/rapideditor/temaki> | README: "To the extent possible under law, the author(s) have dedicated all copyright and related and neighboring rights to this software to the public domain worldwide." `LICENSE.md`: "CC0 1.0 Universal" (<https://raw.githubusercontent.com/rapideditor/temaki/main/LICENSE.md>). | tag `v5.13.0`, commit `ff12eb30f7f91b4c0e17594f8c548be29aa03f4b` |

Checked on 2026-11-11: every file named in 9.3 exists at those two commits (each
was requested by its pinned raw URL and loaded as an image; `home`, `bread` and
`paint` were also read as text and have `viewBox="0 0 15 15"`). `v8.0.0` is Maki's
latest published GitHub release; a later `v8.0.1` tag exists and was not checked.
The glyphs were looked at in a browser to choose the mapping; no file was saved.

Considered and not chosen:

| Option | Why not |
|---|---|
| Kenney Game Icons (CC0, 105 files; <https://kenney.nl/assets/game-icons>, mirror with `Kenney_gameIcons.zip` at <https://opengameart.org/content/game-icons>) | Interface controls only (arrows, pause, gear, save); no goods, trades or needs. Useful, but a third set. Its file names could not be confirmed without downloading the archive. |
| Kenney Board Game Icons (CC0, 250 files; <https://kenney.nl/assets/board-game-icons>, `kenney_board-game-icons.zip` at <https://opengameart.org/node/134027>) | The preview sheet shows dice, cards, pawns, hexagons and a handful of resources; no food, tools, clothing or weather. |
| game-icons.net | Mostly CC BY 3.0: attribution licences are not approved. |
| An icon font | Tinting and sizing are no easier than images in Bevy UI, glyph coverage of the CC0 fonts is poor, and a second font file is still a new asset. |
| Icons drawn in code | Kept for the nine glyphs in 9.4 only. Drawing sixty pictograms from rectangles would look crude and cost more than the pipeline below. |

The mockups do not contain these files. Their icons are inline placeholders drawn
for the HTML in the shape of the chosen glyphs.

### 9.2 Download

Nothing has been downloaded. When the implementer does it, follow the existing
convention (`scripts/fetch_living_assets.py`, `assets/living-world-manifest.json`):
fetch each file by its pinned raw URL, record URL, bytes and SHA-256, never run
downloaded content.

- Per-file URL pattern:
  `https://raw.githubusercontent.com/mapbox/maki/070ed0af1012da8dee83593bb0a33e4592ed68f5/icons/<name>.svg`
  and
  `https://raw.githubusercontent.com/rapideditor/temaki/ff12eb30f7f91b4c0e17594f8c548be29aa03f4b/icons/<name>.svg`.
- Whole archives, if preferred: `https://github.com/mapbox/maki/archive/refs/tags/v8.0.0.zip`
  (GitHub names it `maki-8.0.0.zip`) and
  `https://github.com/rapideditor/temaki/archive/refs/tags/v5.13.0.zip`
  (`temaki-5.13.0.zip`). Only the 92 files below are kept.
- Licence texts: `LICENSE.txt` from Maki and `LICENSE.md` from Temaki at the same
  commits, saved as `assets/licenses/Maki-8.0.0-CC0.txt` and
  `assets/licenses/Temaki-5.13.0-CC0.txt`.

Target layout:

```
assets/
  ui/icons/src/maki/<name>.svg        41 source files, unchanged
  ui/icons/src/temaki/<name>.svg      51 source files, unchanged
  ui/icons/icons-32.png               atlas, white on transparent, 32 px cells
  ui/icons/icons-64.png               atlas, white on transparent, 64 px cells
  ui-icons-manifest.json              path, url, source, author, license, bytes, sha256 per file;
                                      plus the atlas cell index of each icon and the two PNG hashes
  licenses/Maki-8.0.0-CC0.txt
  licenses/Temaki-5.13.0-CC0.txt
scripts/fetch_ui_icons.py             fetch, verify, rasterise, write the manifest
```

`assets/CREDITS.md` gains a section "Interface icons" with two rows (folder,
source, author "Mapbox and contributors" / "Rapid editor contributors", CC0, the
change "rasterised to white PNG atlases at 32 and 64 px").

### 9.3 Mapping

M = Maki, T = Temaki. File names are without `.svg`. "Label" says whether the
glyph may appear without a text label (alone) or must always have one.

**Goods**

| Good | File | Label |
|---|---|---|
| Bread | T `bread` | alone |
| Cooked meals | T `hotpot` | with label |
| Berries | T `grapes` | with label |
| Grain | T `plant` | with label |
| Flour | T `spice_bottle` (closest: a jar) | with label |
| Fish | M `aquarium` | alone |
| Raw meat | T `meat` | alone |
| Clean water | M `water` | alone |
| Logs | T `tree_stump` | alone in the top bar (tooltip), with label elsewhere |
| Planks | M `fence` (closest: boards) | with label |
| Ore | T `boulder1` | with label |
| Ingots | T `anvil` (closest) | with label |
| Fiber | T `grass` | with label |
| Thread | T `needle_and_spool` | with label |
| Hides | M `slaughterhouse` (closest: an animal) | with label |
| Leather | M `shoe` (closest: a leather good) | with label |
| Warm clothes | M `clothing-store` | alone |
| Bowls | T `vase` (closest: a vessel) | with label |
| Axes | M `logging` | with label |
| Pickaxes | T `pick_hammer` | with label |
| Hammers | T `tools` | with label |
| Hoes | T `tools` (no hoe glyph) | with label |
| Hunting spears | T `dagger` (closest: a blade) | with label |
| Fishing rods | T `ice_fishing` (hook and fish) | with label |
| Coins | drawn in code (9.4) | alone |

**Professions and their buildings**

| Trade | File | Building | File |
|---|---|---|---|
| Farmer | M `farm` | Farmstead; Field | M `farm`; T `garden_bed` |
| Miller | M `windmill` | Mill | M `windmill` |
| Baker | M `bakery` | Bakery | M `bakery` |
| Well keeper | T `well_pump_manual` | Well | T `well_pump_manual` |
| Woodcutter | M `logging` | Timber yard | M `logging` |
| Carpenter | T `tools` | Carpenter workshop | T `tools` |
| Miner | T `pick_hammer` | Mine head | T `mineshaft_cage` |
| Smith | T `anvil_and_hammer` | Smithy | T `anvil_and_hammer` |
| Innkeeper | M `beer` | Inn | M `beer` |
| Hunter | T `bow_and_arrow` | Hunting lodge | T `bow_and_arrow` |
| Tanner | M `shoe` | Tannery | M `shoe` |
| Tailor | T `needle_and_spool` | Tailor shop | T `needle_and_spool` |
| Fisher | T `fishing_pier` | Fishing hut | T `fishing_pier` |
| (homes) | | Cottage; House | T `cabin`; M `home` |
| (stores) | | Stockpile; Barn; granary | T `storage`; T `barn`; T `silo` |
| (trade) | | Market stall; Trading post | M `shop`; M `harbor` |
| (any) | | Building site | T `scaffold` |

**Needs and totals**

| Concept | File |
|---|---|
| Health | M `hospital` (a plus) |
| Food | M `restaurant` |
| Water | M `water` |
| Energy, rest, beds | M `lodging` |
| Warmth; lit hearth | T `campfire`; T `fireplace` |
| Morale, wellbeing | M `heart` |
| Residents, population | T `pedestrian` |
| A resident, People | T `portrait` |
| Households, families | T `row_houses` |
| Without a home | T `sleep_shelter` |
| Jobs, vacancies | T `briefcase` |
| Civic fund | T `town_hall` (the fund), with the coin for amounts |

**Modes, navigation, build categories**

| Concept | File |
|---|---|
| Live | T `pedestrian` |
| Oversee; Follow | T `binoculars` |
| Shape | M `mountain` |
| Homes | M `home` |
| Work | T `anvil_and_hammer` |
| Storage | M `warehouse` |
| Market | M `shop` |
| Farms | T `garden_bed` |
| Ledger; How to play | M `library` |
| Village page | M `village` |
| Economy page | T `storage` |
| Projects page | T `scaffold` |
| Journal page | T `bulletin_board` |
| Overlays | M `viewpoint` |
| Places; locate | M `marker` |
| Whole island | T `island_trees_building` |
| Resource patches (overlay) | T `tree_needleleaved` |
| House interior | T `room` |
| Merchant boat | M `ferry` |

**Live workspaces**

| Concept | File |
|---|---|
| Pouch | T `handbag` |
| Craft; workplace link | T `toolbox` |
| Build | T `brick_trowel` |
| Garden | T `garden_bed` |
| Trade | T `money_hand` |
| People | T `portrait` |
| Explore | T `compass` |
| Stop | M `roadblock` |
| Goal, tip | T `bulb` |

**Status, alerts, weather, time, actions**

| Concept | File |
|---|---|
| Fine | M `circle` |
| Low, warning | M `triangle`; M `caution` at 20 px and above |
| Danger, critical | M `diamond` |
| Note | M `square`; hints M `information` |
| Unchanged, empty | M `circle-stroked` |
| Trend, sort, go to, back, raise, lower, undo, redo | M `arrow`, rotated or mirrored |
| Disclosure triangle; play | M `triangle`, rotated |
| Close; cannot | M `cross` |
| Goal reached | M `star` |
| Snow, winter, cold | T `snow` |
| Wind | T `windsock` |
| Rain | M `water` (one or three, drawn at 12 px) |
| Spring; autumn | M `garden`; M `park` |
| Time, duration | T `clock` |
| Settings | M `hardware` |
| Sound | T `speaker` |
| Abandon, remove | M `waste-basket` |
| Locked, not available | T `lock` |
| Paint | M `paint` |

**The 92 files.**

Maki (41): `aquarium`, `arrow`, `bakery`, `beer`, `caution`, `circle`,
`circle-stroked`, `clothing-store`, `cross`, `diamond`, `farm`, `fence`, `ferry`,
`garden`, `harbor`, `hardware`, `heart`, `home`, `hospital`, `information`,
`library`, `lodging`, `logging`, `marker`, `mountain`, `paint`, `park`,
`restaurant`, `roadblock`, `shoe`, `shop`, `slaughterhouse`, `square`, `star`,
`triangle`, `viewpoint`, `village`, `warehouse`, `waste-basket`, `water`,
`windmill`.

Temaki (51): `anvil`, `anvil_and_hammer`, `barn`, `binoculars`, `boulder1`,
`bow_and_arrow`, `bread`, `brick_trowel`, `briefcase`, `bulb`, `bulletin_board`,
`cabin`, `campfire`, `clock`, `compass`, `dagger`, `fireplace`, `fishing_pier`,
`garden_bed`, `grapes`, `grass`, `handbag`, `hotpot`, `ice_fishing`,
`island_trees_building`, `lock`, `meat`, `mineshaft_cage`, `money_hand`,
`needle_and_spool`, `pedestrian`, `pick_hammer`, `plant`, `portrait`, `room`,
`row_houses`, `scaffold`, `silo`, `sleep_shelter`, `snow`, `speaker`,
`spice_bottle`, `storage`, `tools`, `toolbox`, `town_hall`, `tree_needleleaved`,
`tree_stump`, `vase`, `well_pump_manual`, `windsock`.

### 9.4 Concepts with no suitable glyph

| Concept | Cover |
|---|---|
| Pause | Two bars, drawn in code (two nodes). |
| Speeds 1×, 3×, 10× | Text. Clearer than one, two or three triangles, and the font has `×`. |
| Menu | Three bars, drawn in code. |
| Tick (done, can, saved) | Two rotated bars, drawn in code (`UiTransform` rotation is already used for the trend bar). |
| Coin | A ring and a dot, drawn in code. Both sets only have banknotes, which do not fit the setting. |
| Sun (clear, summer) | A disc with eight short rays, drawn in code. |
| Cloud, fog | Three overlapping rounded nodes, drawn in code. Storm: cloud plus M `caution`. |
| Flatten, Smooth (Shape tools) | A bar under two small triangles; a wave. Drawn in code, always with the label. |
| Hoe, flour, hides, leather, ingots, bowls, spears | Closest glyph from the table, always with the name beside or beneath it. |
| Ages, generations | Text ("Child", "Adult", "Elder") in a chip. |
| Temperature | Words ("Mild", "Cold", "Freezing") with T `snow` when cold. |
| Save | Text and the key cap; the toast uses the tick. |
| Item pictures in Pouch | Glyph plus caption now. Later, optionally, thumbnails rendered from the KayKit models the game already ships (no new asset), see section 13. |

### 9.5 Pipeline, sizes and tinting

- Bevy does not load SVG. The fetch script rasterises each source once to white
  (`#FFFFFF`) on transparent with antialiasing, at 32×32 and 64×64 with a 1 px
  (2 px) transparent margin, and packs two atlases of 10 columns. Any rasteriser
  that fills simple paths will do; the sources are single-colour paths with no
  strokes, gradients or text. Options that add no workspace dependency: the
  `resvg` command-line tool installed outside the workspace, or a small scanline
  fill in the Python script. The PNGs are committed, as every other asset is, so
  building the game never needs the rasteriser.
- Loading: embed the two atlases with `include_bytes!` beside the font in
  `game_ui` (the same pattern as `load_font`), so a packaged game needs no loose
  files. An `Icon` enum names every glyph; `icon(Icon::Bread, size, tint)` returns
  an `ImageNode` with the atlas, the cell `rect` and `color: tint`. In Bevy 0.20
  `ImageNode.color` "is multiplied by the color of each pixel", so white art takes
  any token colour (checked in the 0.20 source in the local registry; confirm the
  atlas and `rect` fields there before coding, as the project rules ask).
- Sizes on screen: 12, 14, 16, 20, 24, 32 logical px. Use the 32 px atlas while
  size × UI scale is 32 physical px or less, the 64 px atlas above.
- Tint: the text colour of the context by default (`cream-50` on chrome,
  `ink-900` on parchment); `cream-300` or `ink-500` for secondary; a status token
  for status shapes and for the fill of a need icon only when that need is not
  fine; `ink-900` on honey; never an untinted white icon on parchment.
- Rotation and mirroring (arrows, play) use `UiTransform` or `flip_x`.
- Code-drawn glyphs live in the same `icon()` function, so callers do not know
  which kind they get.
- If the icon files land after the widgets: `icon()` returns an empty node while
  the atlas is absent. Dock tiles, tabs, cards and rows already carry labels; the
  top-bar totals then show short labels instead (People, Mood, Food, Water, Logs,
  Planks, Fund) and the status shapes fall back to code-drawn circle, rotated
  square and plain square, with the word. No layout depends on an icon being
  present.

## 10. Copy

Rules:

1. Sentence case everywhere. No full stop on labels; full stops on sentences.
2. Buttons are verbs that name the result: "Saw planks", "Abandon it", "Show the
   way". Never "OK", "Yes", "Take the next step".
3. No key names in strings. The key cap component adds them.
4. One fact per line. The dot separator joins at most three like facts ("Farmer ·
   Harbour"). Counts come before nouns ("8 planks").
5. A refusal has two parts: what cannot happen, then why, in the simulation's own
   words. A disabled control says what is missing and how much ("Needs 8 planks.
   You have 3.").
6. State, not instruction, on the HUD. Instructions belong to the goal tracker,
   the prompt, tooltips and the Guide.
7. Use the game's own nouns, the same in every mode and in the docs: Live,
   Oversee, Shape, Ledger, Pouch, Journal, civic fund, building site.
8. Do not promise what the simulation has not done: "Requested", "On the way",
   "Finished" stay distinct (existing rule).
9. Time is written as the calendar shows it ("Autumn, day 6", "07:30"); durations
   in seconds at 1× or in days. Never ticks.
10. Residents are "they" unless the world gives them something else.

| Where | Now | Proposed |
|---|---|---|
| Top bar | "Menu · Esc" | Menu icon; tooltip "Menu" with key cap Esc |
| Top bar | "Live · 1", "Oversee · 2", "Shape · 3" | "Live", "Oversee", "Shape" with key caps 1, 2, 3 |
| Top bar | "Resume · Space" / "Pause · Space" (a button that changes text) | A pause segment that lights, plus a "Paused" pill with key cap Space |
| Under the bar | "Live begins your character's arrival on the island." | The Live segment's tooltip: "Live · Begin your character's life on the island." |
| Alerts | "Village alerts · all quiet" | "No alerts" |
| Alerts | "Day 20 · 36 notes · show / 28 trades, 8 job changes" | "Day 20 · 36 notes in the journal" |
| Alerts | "Harbour cottage 3 has waited 2 days for materials (planks 3/8 · logs 4/4)" | "Harbour cottage 3 is waiting for planks" / "2 days · planks 3/8 · logs 4/4" |
| Any list | "Scroll up", "More options below", "Scroll down", "Newer", "Older", "More above", "More below" | Removed; a scroll rail |
| Ledger foot | "Inspect residents and set colony policies while the village lives on." | Removed |
| Ledger foot | "F11: ledger · Tab: bar and panels · Esc: menu / WASD, arrows or pointer at an edge: pan · Wheel: zoom / Q/E or middle-mouse drag: orbit · PgUp/PgDn: tilt" | Guide, Controls page; one coach mark on first entry |
| Village tab | "42 residents · 42 housed / 42 households · 84 beds / Morale 97% · 70 social encounters" | Stat chips: "Residents 42", "Housed 42 of 42", "Beds 84", "Morale 97%" |
| Village tab | "Stock, prices & professions" (a button) | The Economy tab |
| Projects | "Civic fund: 0 coins to spend on materials / Nothing is held back: no site is open." | "To spend 0" and "Held for open sites 0" as chips; empty state "No building is under way. Choose Homes below to place one." |
| Roster | "Job & home for Ada" | "Change job or home" (in the inspector) |
| Inspector | "Now: gathering fiber" | Badge "Doing fine" and "Gathering fiber at the Harbour fields" |
| Inspector | "Would have come first: • Stocking up on provisions: nothing they want is for sale at a price they can pay." | Heading "Passed over", row "Stocking up on provisions" / "Nothing they want is for sale at a price they can pay." |
| Inspector | "Checked first: not thirsty, not hungry, not tired, not cold, not bedtime yet, no meal due, no free time now." | Heading "Checked and fine": chips "Thirst", "Hunger", "Tiredness", "Cold", "Bedtime", "Meal", "Free time" |
| Build | "Cottage · 8 planks, 4 logs / A home with 2 beds" | Card: "Cottage", icon pairs "8" and "4", "A home with 2 beds" |
| Build | "Cottage: move over the island, R turns, click places, Esc or right-click puts it down." | Cursor tag key caps: "Click Place", "R Turn", "Esc Put down" |
| Build | "Cottage · entrance at -24, -53 · R turns" | "Cottage · 8 planks · 4 logs · 4 × 5 m" |
| Build | "Cannot build here: the entrance is blocked by a building" | "Cannot build here" / "The entrance is blocked by a building." |
| Build | "Put down · Esc", "Turn · R", "Close · Esc" | In the cursor tag as key caps; Close is the × button |
| Shape | "Raise · Stone 6.30 m · cell 73, 6" | "Raise · ground at 6.3 m" |
| Shape | "Paint surface · V next" | "Surface" with key cap V |
| Shape | "Smaller · [", "Larger · ]", "Edge: soft · H" | "Size" slider with key caps; "Edge" Hard or Soft with key cap H |
| Shape | "Landscape updated · 23 edits applied" | "12 of 32" under Undo; refusals at the cursor |
| Live | "Move near a resource, plot or neighbour · E interact" / "WASD moves · E interacts · choose an activity from the toolbar." | Nothing while nothing is in reach; otherwise "Draw water from the well" with key cap E |
| Live | "Take the next step" | The step's own verb: "Show the way", "Open Build", "Walk to the well" |
| Live | "Health 100%  Food 99%  Water 99%  Energy 99%  Warmth 100%  0 coins" | Five labelled bars; percentages in tooltips; "Coins 0" |
| Toast | "Island saved · Ctrl+S" | "Island saved" |
| Settings | "Master −", "Master +" | "Master volume" slider, "60%" |
| Settings | "Edge pan: off", "Sound: on", "Window: windowed" | Toggle "Edge pan"; toggle "Mute"; "Display" Windowed or Borderless |
| Pause | "Your village is paused." / "Other ways to play this island" / "Quit game" | "Ada Traveller · Autumn, day 6 · saved 2 minutes ago" / removed / "Save and quit" |
| Title | "Follow the coast, discover three villages, and make a home in a changing world." / "Your story starts with a place to call home." | "Three villages on one coast. Make a home among them." |

## 11. Implementation plan

Seven phases. Each ends with captures at 1280×720 and 1024×768 in a new folder
under `target/npc-out/` with a `summary.json`, as the earlier increments did, and
with `cargo xtask verify` printing `"ok":true`. Module names are as of
2026-11-11; two other changes are in flight in `V/` and `content/testbed_content/`,
so re-read the files before starting.

### Phase 1: tokens and shared widgets

- Change: `V/game_ui.rs` becomes `V/game_ui/` with `tokens.rs` (colours, type,
  space, radius, layers, motion), `button.rs` (kinds and contexts replacing
  `GameButton { primary }`), `keycap.rs`, `chip.rs`, `bar.rs`, `tabs.rs`,
  `segmented.rs`, `icon.rs`, `toast.rs`. `V/panel_scroll.rs` gains the rail style
  and loses the paging buttons and the "More options below" text; Page keys and a
  focusable thumb keep keyboard reach. The colour constants repeated in
  `V/colony_panel/widgets.rs`, `V/resident_inspector/mod.rs`, `V/island_alerts/`,
  `V/shape_tools/palette.rs`, `V/island_build/` and `apps/actor_testbed/src/frontend/ui.rs`
  are replaced by tokens.
- Assets: `scripts/fetch_ui_icons.py`, `assets/ui/icons/**`,
  `assets/ui-icons-manifest.json`, two licence files, `assets/CREDITS.md`.
- Reuse: `game_ui::text`, `panel`, the `paint` system's hover, pressed, disabled
  and focus logic, `attach_panel_scroll_when_needed`.
- Add: a view-only component gallery screen for captures (the content of mockup
  tab 10), reachable by a capture flag, never from the player menu.
- Accept: the gallery capture matches mockup tab 10 at both sizes; the contrast
  unit test passes; every existing view test passes unchanged; no screen still
  shows `#0D0F14` panels; a string scan test finds no label containing " · " plus
  a key name.

### Phase 2: Oversee HUD shell

- Change: `V/island_shell/mod.rs` and `model.rs` (top bar: totals, weather and
  date chips, speed segments, key caps, paused pill; a pure `Totals` projection
  that reuses the numbers of `V/colony_panel/model.rs`), new `V/island_shell/dock.rs`
  (three docks), `V/island_shell/focus.rs` (regions and order of 8.6),
  `V/island_alerts/mod.rs` (stack style, three rows, journal line),
  `V/colony_panel.rs` and `colony_panel/` (closed by default, 440 right column,
  five tabs, Journal page from the alert model's "Everything" filter),
  `V/island_atmosphere/legend.rs` (marker key becomes the Trades overlay legend).
- Reuse: `island_control::request_mode`, the calendar popover, the alert model
  and its folding, the economy sampling, roster sort and filter, every existing
  action component and observer.
- Accept: captures of Oversee default (right column empty), alerts expanded and
  folded, each ledger page, paused; F1 to F5, F11, O and M work and the old paths
  still work; the Tab ring test covers the new regions; a replay recorded while
  opening and closing every new panel is identical to one recorded without (no
  command, no hash change).

### Phase 3: inspectors and build menu

- Change: `V/resident_inspector/mod.rs` (header, status badge, four tabs; the
  job and home commands move here from the roster view; `model.rs` and its pure
  `why` text are untouched apart from splitting at existing sentence boundaries),
  `V/island_build/card.rs` (building and site cards in the same shell),
  `V/island_build/palette.rs` (tray above the dock, item cards, stock column,
  cursor tag), `V/island_build/mod.rs` only where it positions UI, the Projects
  page in `V/colony_panel/`.
- Reuse: `island_build/model.rs`, the simulation's `placement_check` cadence (on
  change and twice a second), `PlaceBuilding` and `CancelBuilding` exactly as
  recorded today, the inline abandon confirm, `island_camera::Follow`.
- Accept: retake the `inspector/*` and `build-menu-try/*` capture sets; a ghost
  verdict is readable in a greyscale copy of the capture; one click on a valid
  ghost records one `PlaceBuilding` and nothing else; a refused placement shows
  the simulation's reason in the tag.

### Phase 4: Live HUD and workspace

- Change: `V/player_hud/mod.rs` (needs cluster, hotbar, goal tracker, prompt pill,
  toasts, workspace in the right column, two-pane Craft and Trade),
  `V/player_hud/model.rs` only for presentation fields (icon per row, reason
  strings already exist), `V/player_hud/life.rs` (death and successor as a modal
  in the new style), `V/homestead_panel.rs` (developer panel: tokens only).
- Reuse: `player_journey` guidance as is, `player_actions`, the selected-site
  pattern, `FrontendGate`, the rule that a hidden workspace builds no
  observations, outcome trackers.
- Accept: retake `phase4a/live-*`; each of the seven workspaces captured at both
  sizes; no prompt box while nothing is in reach; a recipe with a missing input
  shows a disabled button, a diamond and a sentence; the profiler's view tags show
  no new work while every workspace is closed.

### Phase 5: Shape

- Change: `V/shape_tools/palette.rs` (rail and brush bar), `V/shape_tools/mod.rs`
  only for the cursor tag text.
- Reuse: `shape_tools/model.rs` (tools, stroke planning, undo history),
  `picking.rs`, the recorded `Terraform` and `TerraformCells` commands.
- Accept: retake `view-wiring/shape-*`; every key in the Shape table still works;
  a refused dab shows a dashed red outline and the reason; undo depth is visible.

### Phase 6: title, new game, pause, settings

- Change: `apps/actor_testbed/src/frontend/ui.rs` (title column over a gradient,
  new-game panel, pause panel), `frontend/settings.rs` (four tabs, sliders,
  toggles, Auto interface size, Reduce motion, Tooltip delay, Show tips, Panel
  style; lenient field-by-field parsing stays), `frontend/mod.rs` only if the
  automatic scale needs the window size.
- Reuse: the frontend state machine, save and backup logic, `FrontendUiRoot`,
  `PlayerMenuRequest`.
- Accept: retake `package-test/run/title.png`, `phase4a/pause-*` and
  `view-followups/settings-*`; a missing or damaged settings file still falls
  back to defaults; at 1920×1080 with Auto, measured body text is 18 px or more
  from ascender to descender; a failed save shows the persistent toast in every
  mode.

### Phase 7: tooltips and onboarding

- Change: new `V/game_ui/tooltip.rs` (two levels, delays, Alt, focus), tooltip
  content for every icon-only control and every total; new `V/island_shell/coach.rs`
  (four first-run marks in Oversee: totals, build dock, "click a resident",
  alerts; shown once, stored in `island-settings.json`); How to play becomes a
  Guide with pages (Controls, Oversee, Live, Shape, The economy) in
  `frontend/ui.rs`.
- Reuse: the existing Live tip logic in `island_shell/model.rs` (`live_tip`), the
  calendar's season facts for the date tooltip.
- Accept: every icon-only control has a level-1 tooltip (a test walks the Tab
  ring and fails on a missing one); tooltips appear on keyboard focus; "Show tips"
  off hides coach marks and the goal tracker's hints but not the goal; no tooltip
  leaves the window at either size.

### What must stay unchanged

- Views never write simulation state. Every effect is a recorded command through
  the existing paths: `PlaceBuilding`, `CancelBuilding`, `Terraform`,
  `TerraformCells`, the colony policy and assignment commands, shared player
  actions, mode changes through `island_control::request_mode`, the clock through
  the host's `SimControl`.
- All new state (open panel, tab, sort, tooltip, coach marks seen, layout class,
  scale) is view or host state: not saved in the world, not hashed, not read by
  the simulation. Replays and goldens do not change; if one does, the change is
  wrong.
- The keyboard Tab ring reaches every control; a mouse click on chrome does not
  take keyboard focus; Esc and a click on the world hand the keyboard back.
- Both window sizes, 1280×720 and 1024×768, are verified for every phase,
  including Windows DPI scaling.
- `FrontendGate` still suppresses gameplay input and view work under menus.
  Hidden workspaces build no observations. Trend sampling does not depend on
  which panel is open.
- Developer launches (no top bar) keep working; the developer toolbox is out of
  scope.
- Existing keys keep working; new keys are additions.
- No new workspace dependency. No protected file is touched.

## 12. Verification done for this document

- Current UI: read from the captures named in section 1 and from the modules
  named in section 11. No build was run.
- Mockups: rendered in a browser at 1280×720; a script confirmed that no element
  leaves its frame and no panel clips its content, that the file makes no network
  request and has no script, and that it uses no character missing from the
  bundled font.
- Contrast and colour-vision numbers: computed, not estimated (section 6).
- Icon licences and file existence: read from the sources on 2026-11-11
  (section 9.1). No icon file was downloaded or added.
- Not verified: how the palette looks over the real 3D scene at night and at
  dusk; rendering of `BoxShadow` and `Outline` at fractional UI scales; the real
  glyphs at 12 and 14 px (the mockup uses stand-ins).

## 13. Decisions for the owner

**Owner decisions, 2026-11-11:** parchment panels are chosen ("Parchment"); a CC0 icon
set is approved. The remaining rows below proceed as recommended unless the owner
objects: the owner was told so and has not objected.

| # | Decision | Recommendation |
|---|---|---|
| 1 | **Panel style**: parchment panels with dark chrome, or dark panels throughout. | Parchment with dark chrome as drawn; ship Dusk as an option in Accessibility. Review one night capture after phase 1 before committing the rest. |
| 2 | **Ledger form**: a 440 px right column that leaves the world visible, or a large centred management screen. | The right column. It reuses the existing docked ledger, keeps one place for detail and lets the player click the world while reading. Add a wider table view later only if the Economy page needs more columns. |
| 3 | **New Oversee keys**: F1 to F5 for ledger pages, O for overlays, M for places, F for follow, Alt for tooltip detail. | Adopt. Fallback letters for the pages are V, G, P, J, L. |
| 4 | **Automatic interface size** as the default, multiplied by the existing 80 to 140% setting. | Adopt; it is what brings text to a readable size on a 1080p or 1440p borderless window. |
| 5 | **A second font weight**: add Barlow SemiBold (same family and licence, one more file) for headings and numbers. | Yes, after phase 1. The design works with Regular alone; SemiBold would let the totals and titles stand out without more size. It is a new asset and needs approval. |
| 6 | **Minimap**. | Not now. Places plus zoom covers a single island; revisit if the world grows. |
| 7 | **Pause menu content**: remove "Oversee the village" and "Shape the landscape" from it. | Remove; the top bar has them in every mode. |
| 8 | **Item pictures in Pouch**: glyph with caption, or thumbnails rendered from the existing 3D models. | Glyphs now; consider rendered thumbnails as a later polish item, since it needs no new asset. |
| 9 | **Gamepad**. | Keep out of scope; hold to the readiness rules in 8.8. |
| 10 | **Speed steps**: keep 1×, 3×, 10×, or add 2× or 5×. | Keep three; the segmented control takes a fourth without redesign if measurements allow a faster rate. |

## 14. Sources

Genre and patterns:

- Timberborn wiki, main screen: <https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen>
- Timberborn update 3 notes (top-bar tooltips, shortened numbers): <https://tryhardguides.com/timberborn-reveals-new-storage-system-in-update-3-patch-notes/>
- Unity case study on Timberborn's runtime UI: <https://unity.com/resources/timberborn>
- RimWorld wiki, user interface: <https://rimworldwiki.com/wiki/User_interface>
- RimWorld wiki, learning helper: <https://rimworldwiki.com/wiki/Learning_helper>
- Banished interface guide: <https://www.gamepressure.com/banished/interface/z06082>
- Banished wiki, Town hall: <https://banished.fandom.com/wiki/Town_hall>
- Banished keyboard shortcuts: <https://defkey.com/banished-city-building-shortcuts>
- Oxygen Not Included main screen guide: <https://www.gamepressure.com/oxygen-not-included/main-screen/z39cfa>
- Oxygen Not Included wiki, overlays: <https://oxygennotincluded.wiki.gg/wiki/Overlay>
- Against the Storm devlog, "Updated UI preview and Deeds Update teaser", 26 April 2022: <https://eremitegames.com/?p=236833>
- Manor Lords official wiki, beginner's guide: <https://wiki.hoodedhorse.com/Manor_Lords/Beginner%27s_Guide>
- Foundation news (1.7 UI update, 1.0 release): <https://www.polymorph.games/foundation/news/>, <https://www.polymorph.games/foundation/news/2025/01/31/foundation-full-release/>
- Farthest Frontier 1.1 (Guided Journey): <https://80.lv/articles/farthest-frontier-s-first-major-update-brings-gridless-build-mode-more>
- Going Medieval beginner's guide (settler panel, job panel): <https://guides.magicgameworld.com/going-medieval-tips-and-tricks-a-beginners-guide/>
- Dwarf Fortress Steam UI: <https://www.pcgamer.com/the-new-dwarf-fortress-ui-looks-so-much-better>, review <https://www.pcgamer.com/dwarf-fortress-review/>
- Frostpunk console interface: <https://news.xbox.com/en-us/2019/09/20/frostpunk-console-edition-radial-driven-design-of-gameplay-and-controls/>
- Stardew Valley getting-started guide: <https://www.thegamer.com/stardew-valley-getting-started-guide/>; wiki, Energy: <https://stardewvalleywiki.com/Energy_Bar>
- Victoria 3 dev diary 29, user experience (nested tooltips): <https://forum.paradoxplaza.com/forum/developer-diary/victoria-3-dev-diary-29-user-experience.1506484/>
- PC Gamer on Crusader Kings 3 tooltips: <https://pcgamer.com/the-crusader-kings-3-team-is-working-hard-to-make-the-game-easier-to-learn>
- Philip Ardeljan, "Tooltips in tooltips": <https://philip.design/blog/tooltips-in-tooltips>

Sources reached only through search summaries, not read in full: the Banished
wiki and shortcut pages, the Stardew Valley pages, the Paradox diary, the PC Gamer
and Ardeljan articles, the Farthest Frontier, Foundation, Going Medieval, Dwarf
Fortress and Frostpunk pages. Their statements in section 2 are limited to what
the summaries quoted.

Accessibility:

- Xbox Accessibility Guideline 101, text display: <https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/101>
- Xbox Accessibility Guideline 102, contrast: <https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/102>
- Game Accessibility Guidelines, colour: <https://gameaccessibilityguidelines.com/ensure-no-essential-information-is-conveyed-by-a-colour-alone/>
- WCAG 2.1: <https://www.w3.org/TR/WCAG21/>

Icons:

- Maki: <https://github.com/mapbox/maki>, licence <https://raw.githubusercontent.com/mapbox/maki/main/LICENSE.txt>
- Temaki: <https://github.com/rapideditor/temaki>, licence <https://raw.githubusercontent.com/rapideditor/temaki/main/LICENSE.md>, preview <https://rapideditor.github.io/temaki/docs/>
- Kenney Game Icons: <https://kenney.nl/assets/game-icons>, <https://opengameart.org/content/game-icons>
- Kenney Board Game Icons: <https://kenney.nl/assets/board-game-icons>, <https://opengameart.org/node/134027>
- Kenney Input Prompts: <https://kenney.nl/assets/input-prompts>

Project:

- `PRODUCT.md`, `docs/island-life.md`, `docs/island-colony.md`, ADR 0036, ADR 0038,
  `docs/superpowers/specs/2026-11-10-island-village-review-and-roadmap.md` (P1 to P14).
- Bevy 0.20 `bevy_ui` source in the local registry (`ImageNode`, `BoxShadow`,
  `Outline`, `UiScale`, `UiTransform`).
