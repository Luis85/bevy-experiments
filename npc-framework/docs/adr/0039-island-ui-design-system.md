# 0039: One design system for the island interface, and the Oversee shell

Date: 2026-11-11. Status: accepted for phases 1 and 2 of
`docs/superpowers/specs/2026-11-11-island-ui-redesign.md`.

## Context

The island interface grew panel by panel. Each module carried its own colour
constants, hotkeys were written into labels, explanation sat in permanent hint
text, status was a coloured dot, and the ledger covered a third of the window by
default. The redesign spec fixes this in seven phases. The owner chose parchment
panels over dark chrome and approved a CC0 icon set; the spec's other
recommendations stand.

## Decision

**One token table, read by role.** `testbed_view::game_ui::tokens` holds every
colour, the type scale, spacing, radii, elevation, layers and motion. Widgets
read `Theme::roles(surface)`, never a colour constant, so the optional **Dusk**
panel style is the same code with another table. A unit test recomputes the
contrast pairs of the spec's section 6.2 from the constants.

**One widget kit, adopted screen by screen.** `game_ui::widgets` builds panels,
buttons, tabs, rows, tables, chips, bars, key caps, toggles, sliders, toasts and
confirms as plain Bevy UI nodes, and one system paints them from their state.
The older `game_ui::button` keeps its own paint, so screens convert one at a
time; a screen is never half converted.

**Icons by concept, embedded.** Callers ask for `Icon::Planks`, not a file.
Pictograms are Maki 8.0.0 and Temaki 5.13.0 (CC0), fetched by pinned commit and
rasterised to two white atlases that are embedded like the font; nine glyphs
with no suitable pictogram are drawn from nodes. The fetch script needs only the
Python standard library and stops unless both licence texts say CC0. No new
workspace dependency was added; `deny.toml` and `arch_fitness` are untouched.

**Interface size is automatic by default.** UI scale is a factor from the window
(1.0 at 1280×720, 1.5 at 1080p, 2.0 at 1440p) times the player's 80 to 140%.
The host stores the setting and writes `InterfaceScale`; the view owns the
conversion to `UiScale`. Layout decisions use the effective size and its class
(Wide, Standard, Compact).

**The Oversee shell.** One top bar in every mode (menu, mode switch, weather,
date, speed); while overseeing it also shows seven village totals read from
existing accessors. Three bottom docks open Overlays and Places, the build
categories and the ledger's five pages. Alerts are a stack of at most three
rows; everything else is the ledger's Journal page.

**One right column, closed by default.** The ledger, the resident inspector and
the building card share one place and never show together. The rule is a pure
reducer (`island_shell::model::ColumnState`): a page opens or closes the ledger,
a new selection replaces it with the inspector, and Close or Esc empties the
column and clears the selection.

The camera frames a selection in the part of the view the column leaves free,
whichever of the three holds it. The alert stack folds to its header while the
column or the build menu is open.

**Esc is one step per press**: keyboard focus, then a popover, the building in
hand, the build menu, the right column, and only then the pause menu. Each step
consumes the key.

**New Oversee keys**: F1 to F5 for the ledger's pages, O for overlays, M for
places, F to follow the selected resident, Alt for tooltip detail. F11 still
toggles the ledger. With `--dev-keys` the function keys stay with the developer
panels.

## Consequences

- All of this is view or host state: nothing is saved in a world, hashed or read
  by the simulation. Replays and goldens are unaffected.
- Labels changed ("Live" instead of "Live · 1", an icon-only Menu button), so
  scripts that click widgets by their text need the new names; every control
  carries a `Name` of the form `Game: <name>` for that.
- Until phase 3 the resident inspector, the building card and the build menu
  keep their old dark style inside the new layout, and Live, Shape, title, pause
  and Settings are unchanged apart from the shared top bar.
- Weather is shown because the regional world now exposes it
  (`village::weather::today`); a world without weather has no weather chip.
- The spec's first Dusk caption and control-edge colours fell short on Dusk
  cards (4.49:1 and 2.6:1). They were retuned to `#BEAC8E` and `#A08A70`; the
  spec's token table records the change and the contrast test has no exception.

## Phase 3 additions

- **Rows come from a model.** The ledger pages and the inspector's tabs build
  `colony_panel::Entry` values in pure functions; one reconciler
  (`reconcile_rows`) turns them into kit rows by key. A panel that lists things
  uses this rather than spawning rows of its own, so its content is unit-tested
  without a window. Special bodies are `Extra` variants, not new row systems.
- **One action queue per owner.** A row's action is a `colony_panel::Action`
  variant even when the inspector owns the row; the ledger's `process` applies
  it. The inspector keeps its own small request list for its header and tabs.
- **The build tray is not a column panel.** It sits bottom centre and is
  mutually exclusive with the right column. This replaces the spec's picture of
  a tray that coexists with an open ledger: at 1024 wide the two overlap.
- **Build categories are derived**, never listed per world: a building with crop
  plots is a farm building, and the Farms group exists only where the catalogue
  has one.
- **Job and home commands belong to the resident**, on the inspector's Home and
  job tab. The People page lists and filters; it arranges nothing itself.
- **Unavailable is a state the tray supports and no data sets.** The catalogue
  has no rule that makes a building unavailable; the card, its reason line and
  the refusal are in place for when it has.
- Parchment stays the default at every time of day (owner's decision); Dusk is
  a Settings choice and is never switched on automatically.

## Phase 4 and 5 additions

- **Live's HUD is four pieces of chrome and one column.** Goal (top left),
  needs (bottom left), hotbar (bottom centre), prompt (above the hotbar, only
  when there is something to say), and the workspace in the same right column
  Oversee uses. Nothing else is permanent: outcomes are toasts.
- **A slot without data stays out.** The spec's goal tracker has a step count
  and pips, the working pill a progress bar, the pouch a weight bar, and the
  HUD a held item. The observation has none of these, so none is drawn; they
  are listed as simulation features to add, not approximated in the view.
- **The workspace keeps the row model.** `player_hud::model::rows` still builds
  the rows; a row gained `inputs` (chips) and `reason` (the sentence under a
  disabled button). The view decides nothing about what can be done.
- **Choosing is not using.** A pouch item is selected first and acted on by a
  named button. The old one-click "use" rows are gone.
- **Shape's state text is filtered in the view, not changed in the tool.**
  `ShapeTools.status` keeps its strings (tests and the latency log read them);
  `shape_tools::model::status_text` decides what becomes a notice.
- **A refusal is never colour alone.** In Shape it is a dashed outline, a cross
  in the tag and the reason in words, as in the build tray.
- **Shape has no destructive action** beyond a stroke that Undo takes back, so
  it asks no confirming question. A future "reset terrain" would use the
  kit's inline confirm.

## Phase 6 and 7 additions

- **The host's screens use the kit too.** Title, new game, pause, settings and
  the guide are built from `game_ui::widgets` in
  `apps/actor_testbed/src/frontend/ui.rs`. The screen is still rebuilt on a
  revision change; a slider is the exception and is filled in place, because a
  rebuild would take the thumb from under the pointer.
- **One door for host notices.** `testbed_view::island_toast` (tone, text,
  lasting, retry) puts a host notice on the shell's toast lane;
  `take_island_toast_retry` reports its "Try again". The host decides what to
  say (`hud_notice`, `toast_of`); the view only shows it.
- **Tips are host settings, shown by the view.** `IslandTips` (show, seen,
  dismissed) is filled by the host from its settings file and drained by it.
  Without a host there are no tips. Nothing about tips is in the world.
- **Names outlive labels.** Where a button's words changed ("Save and quit",
  the "−" and "+" of a slider), the control keeps the name automation knew
  ("Quit game", "Master −").
- **No save slots.** The spec has one island save with numbered backups; the
  feedback for saving and loading is specified and built, slots are not.
- **Reduce motion has one consumer: the critical-need pulse.** A need below
  the interrupt level gains a red outline that pulses once every two seconds
  (spec 5.6); Reduce motion makes it a steady edge (`hud::pulse_alpha`). It is
  the only motion of the interface; a later animation reads the same switch.
- **Tooltips stand above toasts** (`layer::TOOLTIP` 42 over `layer::TOAST`
  40): the Paused pill covered the totals' tooltips in the native capture.
- **Under a host menu only the menu explains itself.** Tooltips stay gated
  while `FrontendGate` is up, except on controls under a `FrontendUiRoot`;
  the title, pause and Settings buttons carry a tooltip that says what
  follows. The card is a root node like the island's panels, and the
  player HUD hides every root that is not a `FrontendUiRoot` while a menu
  is open, in the same frame the card was built: the card is exempt
  (`TooltipRoot`). The unit test runs both systems in frame order.
- **A lasting toast's button is on the Tab ring** (`FocusRegion::Toast`,
  after the bar in every mode). A toast's timer counts at most 0.1 s a frame,
  so a loading hitch does not eat "Island loaded".

## Live depth (second Live pass)

- **Live reads the simulation's live accessors directly**
  (`village::view::{goals, activity_progress, repeat_order, need_reason,
  pack, held_tool, relationships, gift_tastes, homestead_tiers,
  upgrade_site, gatherings, mourning, asleep_until, day_summary}`) through
  one module, `player_hud/depth.rs`. The actor-scoped observation stays the
  source of the workspaces' choices; the accessors add what it does not
  carry. A slot whose accessor answers nothing is left out, never filled.
- **Live commands are recorded `LiveAct`s.** `depth::LiveRequest` (verb,
  target, kind, quantity) is checked with `LiveAct::check` for the reason a
  control is disabled, and submitted with `npc_framework::prelude::submit`.
  New verbs (store, take, deliver, order, hire, recipe repeats) slot in as a
  request and a button; no new path.
- **The host owns the night.** While the played character sleeps in Live,
  `frontend::night` raises the clock to `SLEEP_SPEED` (30x) and gives the
  player's own speed back on waking; a speed chosen during the night is
  kept. The simulation skips no tick. The veil and the day's account are
  view (`player_hud/rest.rs`).
- **A taste is shown once it can be known**: after a first gift to that
  resident (the bond's `gifted_day`) or once the bond reaches a tier that
  lends a hand. `view::gift_tastes` itself always answers; the rule is the
  view's.
- **Days left are for food and water only.** `view::daily_use` estimates
  other goods from units traded, which is not consumption; the goods table
  shows a dash there. Under two days of food or water is Low, in a village
  too; the island's shared rules still decide the rest.
- **The goods table has five columns** (good, stock, price, days, change);
  `colony_panel::widgets::widths_for(count, compact)` gives the widths and the trend mark goes in
  the last column.

- **An empty pouch is a line, not a pane.** In the two-pane workspaces
  (Craft, Trade) the pouch pane collapses to one line above the list when
  it is empty, and the list takes the whole width (`workspace::pouch_layout`).
- **A tooltip stands over its anchor's panel.** Its layer is the tooltip
  layer or one above the nearest `GlobalZIndex` of its anchor, whichever is
  higher, so a modal's buttons explain themselves on top of the modal.
- **The upgrade card offers every way to finish.** Deliver, Order (a price
  per missing good with the carrying fee, from `UpgradeSiteView::order_prices`),
  Hire a builder (`hire_cost`) and Cancel; the simulation's refusal is the
  disabled reason. Repeat orders cover gathering, recipes and the garden
  with one ×1 / ×5 / ×10 stepper.
- **Sleep leaves the sleeper in view.** The night is a vignette (edges
  dark, centre clear) and a strip where the prompt stands; E wakes. The top
  bar and the HUD's chrome stay above it, so the clock runs visibly.
- **A bond is knowledge.** People lists everyone the game keeps a bond with
  as well as the faces the observation remembers.
- **Prepared saves for captures.** Screens that natural play reaches only
  after a long while (an evening at one's own cottage, an upgrade under
  way, hard times) are captured from saves made headlessly by
  `testbed_content/tests/island_capture_fixtures.rs` and loaded with
  `--island-save`; `scripts/playtest/capture_ui_live3.py` copies them
  before launching, so a fixture is never written by the game.

- **Resource patches are outside Shape's Undo.** The history holds terrain
  cells; a placement's inverse is the removal of an identity the simulation
  hands out only when it applies the command. Remove takes a patch back.
- **The view asks, the host writes.** Export world is a request resource;
  the host writes the world file beside the world rules and reports it.
- **Menu tooltips stand beside their control** (right, or above when there
  is no room): a menu stacks its buttons, and a card below one covers the
  next.
- **A save owns its world; a world file is only read once.** New game lists
  the world the window opened with first, then every `<slug>-shaped-N.json`
  beside the world rules, newest first; a file that cannot be read or used is
  listed, disabled, with the reason, and never left out. The chosen file goes
  through `session::new_game_on`, which parses it as a `WorldPreset`, applies
  `construction_from_preset`'s guard, stages, validates, backs up and rolls
  back exactly as the shipped world does. The save stores the whole
  construction, so Continue never reads the file again and a deleted or
  edited export cannot change a game in play. The runtime `ActiveWorld`
  follows the construction (and rolls back with it).
- **"Start a new game on it" saves first.** The export toast offers it only
  while this window owns the save, the last save did not fail and no exit is
  under way; the button runs the ordinary save-and-exit, then opens New game
  with the export chosen. The current island is kept as a backup.
- **Counts of goods go through one helper** (`game_ui::words::count`): "1
  log", "2 logs", "3 bread". A building card is titled by the building
  ("Harbour inn"); the trade is a row.
- **Patch chips never overlap.** At island zoom each patch is a picture
  chip; its name shows on hover or close up. A chip with no room drops its
  name, then folds into the chip it would cover ("+2"), and none is drawn
  under the first-run tip or the resources panel.

- **An older save is kept and named, never opened or overwritten.** Every
  local save's header carries `island_version`
  (`island::ISLAND_SAVE_VERSION`); a save that does not restore and names an
  older version (or none) is "saved by an older version": the title says so,
  offers New game, and New game copies it to `previous-N.json` before writing.
  The simulation's typed "made by an older version" result replaces this
  classification once it exists.
- **Shift+Tab from the world in Live enters at the top bar**, at its last
  control, not at the end of the ring (the hotbar).

## Not decided here

A second font weight, a minimap and gamepad support.
