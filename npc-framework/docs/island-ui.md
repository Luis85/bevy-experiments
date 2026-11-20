# Island Village interface: tokens, widgets and icons

How to build island UI with the shared design system. The design itself is
[`superpowers/specs/2026-11-11-island-ui-redesign.md`](superpowers/specs/2026-11-11-island-ui-redesign.md);
the decision record is [ADR 0039](adr/0039-island-ui-design-system.md). Code lives in
`content/island_view/src/game_ui/`.

Status: phases 1 to 3 of the spec are in (tokens, widgets, icons, interface size,
the Oversee shell; then the resident inspector, the building card, the build
tray and the ledger pages). Live, Shape, title, pause and Settings still use the
older `game_ui::button` style until their phases.

## Rules that do not bend

- A widget takes its colours from its **surface** (`Surface::Chrome` for bars,
  docks, tooltips and toasts; `Surface::Paper` for panels that are opened and
  read). Never pass a colour to a widget; never write a hex value in a screen.
- **Colour never speaks alone.** Status is a shape (`Icon::status`), a colour and,
  where there is room, a word. Amber is never text on paper.
- **No key in a label.** A binding is a key cap (`keycap`, or `ButtonSpec::key`)
  or a tooltip's `key`.
- **Explanation goes in a tooltip**, not in permanent hint text. A disabled
  control keeps its place and says what is missing (`reason`, and the same words
  in its tooltip).
- Views never write simulation state. The widget builders take the caller's
  action component; the caller owns the observer and submits recorded commands.

## Tokens (`game_ui::tokens`)

| Group | Use |
|---|---|
| Colour constants (`BARK_900` … `INFO_LIGHT`) | Only inside `tokens.rs` and the role tables. Screens use roles. |
| `Roles` (`CHROME`, `PARCHMENT`, `DUSK`) and `Theme` | Role colours per surface. `Theme::roles(surface)` is what every widget reads; Dusk is the same table with other values, so nothing needs a second code path. |
| Type scale (`DISPLAY`, `TITLE`, `HEADING`, `SUBHEADING`, `BODY`, `DENSE`, `CAPTION`) | Size and line height. Nothing is smaller than `CAPTION` (12 px). One weight: hierarchy comes from size and ink level. |
| `SPACE`, `RADIUS_*`, control heights, `TARGET_MIN` | Spacing 2 to 32; radius 4 / 6 / 10; pointer targets at least 28 × 32. |
| `SHADOW_1..3` with `shadow()`, `layer::*`, `motion::*` | Elevation, `GlobalZIndex` layers (chrome 30, panel 32, popover 34, tooltip 40, toast 42, modal 52) and durations. |
| `contrast()`, `luminance()`, `over()` | WCAG 2.1 maths. The tests in `tokens.rs` recompute every pair of the spec's section 6.2; a palette edit that breaks a ratio fails the build. |

The Barlow Regular font has no arrows, ticks or shapes. Use an icon; a string
with a missing character falls back to a box.

## Widgets (`game_ui::widgets`)

Builders take `&mut World`, a parent and what to show, and return the entity (or a
small struct of the parts a refresh needs). One system paints every kit control
from its state: default, hover, pressed, focus (a 2 px outline, sea blue on
paper, honey on chrome), disabled, and `on` (lit, selected, checked).

| Widget | Builder | Notes |
|---|---|---|
| Text | `label` (one line), `paragraph` (wraps) | Takes a `Type` step and a `Tone`. |
| Panel, chrome bar, card, divider | `panel`, `chrome`, `panel_header`, `card`, `divider` | `panel`/`chrome` are bundles for the root node. |
| Buttons | `button` with `ButtonSpec::new(label, Kind::Primary \| Secondary \| Quiet \| Danger, surface)` and `.size()`, `.icon()`, `.key()`, `.reach()` | One primary per view. Danger only inside a confirm. |
| Icon button | `icon_button` | Always give it a `Tooltip` with its name and key. |
| Dock tile | `dock_tile` | Icon over a caption, key cap in the corner, optional count badge. |
| Segmented control, tabs | `segmented` + `segment`, `tabs` + `tab` | Set the lit one with `set_state(entity, true, false)`. |
| Rows and tables | `list_row`, `card_button`, `table_header`, `table_group`, `table_row`, `table_cell` | Numbers right-aligned; first column icon and name. |
| Stat chip, badges | `stat_chip` + `set_mark`, `badge`, `count_badge` | A chip's mark is a status shape or a trend arrow. |
| Bars | `progress_bar`, `need_bar`, `set_bar`, `set_need` | A need below "fine" gains a triangle or a diamond. |
| Key cap | `keycap` | One cap per binding: "Ctrl+S", "Click". |
| Slider, toggle | `slider`, `toggle` | The slider is Bevy's headless `Slider`; observe `ValueChange<f32>`. A toggle's label names the thing, not the state. |
| Messages | `toast`, `inline_confirm`, `modal` / `modal_panel`, `reason` | A failed toast has a critical edge and stays. |
| Tooltip | `game_ui::tooltip::Tooltip` component | See below. |

**Keyboard reach.** `Reach::Tab(index)` makes a control a stop in its panel's Tab
group that a click focuses. `Reach::Ring` (the `KeyFocus` marker) makes it a stop
of the session ring that a click does *not* focus, so Space still pauses after a
click on the bar or a dock. The ring's regions and their order are in
`island_shell/focus.rs`.

**State.** `set_state(world, entity, on, marked)` and `set_enabled`. `marked` is a
second emphasis: a total a building in hand draws on, an unavailable card.

**Unavailable.** `set_state(card, on, true)` gives a card the washed-out look and
`reason` puts the sentence where the purpose line was; the same sentence leads
the tooltip. The control stays in the Tab ring so the reason can be read, and
the owner refuses the action with the same words. The build tray does this from
`PaletteItem::unavailable`, which nothing sets today: the catalogue has no rule
that makes a building unavailable.

**Adopting the kit in an old screen.** Old `game_ui::button` controls keep their
own paint and can sit beside kit controls while a screen is converted. Convert a
whole panel at a time: swap the root for `panel` or `chrome`, texts for
`label`/`paragraph`, buttons for `button`, and replace any state-colour code with
`set_state`. Remove hint paragraphs; move them into tooltips.

### Tooltips

```rust
world.entity_mut(chip).insert(
    Tooltip::new("Planks", "Running low: open sites still need 13.")
        .icon(Icon::Planks)
        .value("11 in the villages")
        .detail(["Harbour 6 · Alderwood 5 · Stone Ridge 0", "Click: open Economy"]),
);
```

The node needs `Hovered` (every kit control has it). Level 1 (title line and one
sentence) shows after 400 ms of rest; level 2 (the `detail` lines) after a further
1.2 s, or at once while Alt is held. Keyboard focus shows them after 600 ms.
`TooltipDelay` holds the player's Short / Normal / Long choice. A tooltip never
holds a control, never covers its anchor and stays inside the window. A card
whose lines wrap is given its full width when it is placed; measured at its
one-line width it came out a line short and clipped its last line. A control
that takes keyboard focus as its screen opens (the morning account's Good
morning) has no tooltip: its card stood over the rows it explains.

Counts of goods are said through `game_ui::words::count(n, kind)` ("1 log",
"2 logs", "3 bread"); never format a count and a good by hand.

## Rows from a model (`colony_panel::Entry`)

A panel that lists things builds a `Vec<Entry>` in a pure function and hands it
to `colony_panel::reconcile_rows(world, body, old_rows, entries)`, which builds,
updates or removes one row per `key` and returns the new key-to-entity map. The
ledger pages and the resident inspector's tabs are both made this way, so a
model can be unit-tested without a window and a row keeps its entity (and its
hover, focus and scroll place) while its text changes.

| An entry is | Built with | Drawn as |
|---|---|---|
| a heading or a line of text | `Entry::text(key, text, heading)`, `Entry::strong`, `Entry::caption` | a `HEADING`, body or caption paragraph |
| a command | `Entry::action(key, text, action)` | a secondary button; `Entry::link(..)` is a quiet row with an icon and an arrow |
| a choice | `Entry::choices(key, [(label, action)..])` | a segmented control for filters and switches, buttons otherwise; a confirming choice is the `Danger` kind |
| a table line | `Entry::table(key, columns, trend, header)` with `.with_icon()`, `.with_action()` | a table row; with an action it is selectable |
| numbers at a glance | `Entry::stats(key, [(icon, label, value)..])` | a grid of stat tiles |
| a resident or a journal line | `Entry::marked(key, text, status, action)` | a two-line row with a status shape |

`.with(Extra::..)` asks for one of the special bodies: `Meter` (a need bar with
its status), `Chips`, `Card` (a card of facts under a title), `Group` (a caption
band between table rows), `Fold(open)` (a heading that folds what follows),
`Badge(status, word)` on a table row, `Strip` (the day strip), `Site` (a project
row: progress bar, material chips, state in words). `.with_tip(Tooltip)` gives
any row its two-level tooltip. An action is a variant of `colony_panel::Action`;
the owning panel's observer queues it, and the ledger's `process` turns it into
view state or one recorded command. Add a variant rather than a second queue.

## The right column and the tray

- One right column (`island_shell::model::ColumnState`): a ledger page, the
  resident inspector or a building card, never two. Widths come from
  `column_width(column, class)`; place the panel between
  `island_shell::content_top(world)` and `content_bottom(world)`. A new selection
  (`ColumnEvent::Selected`) replaces the ledger; a page hides the inspector and
  closes a card; closing the column clears the selection.
- The build tray (`island_build/palette.rs`) is the one panel outside the column:
  bottom centre, above the docks (`tray_rect(window_width)`). Its cards are one
  row that scrolls sideways: `panel_scroll::attach_rail_x` gives the rail, and
  the tray's own `wheel` system turns the wheel into sideways movement. Opening it closes
  the column, and opening a page closes it, because it would cover the column's
  lower part in a narrow window. The camera does not treat it as a side panel.
- Build categories are derived in `island_build::model::Group::of(def, farms)`
  from the catalogue: a building with crop plots is a farm building, and only a
  catalogue that has one gets a Farms group (the farmers' workplace joins it).
  The dock shows a tile per non-empty group; nothing is hard-coded per world.
- The tag beside the cursor is `model::tag(item, reason)`: a verdict in words, the
  simulation's reason as a sentence, the cost and footprint. Keep the verdict
  out of colour alone.
- At the Compact class the top bar's totals move to a second row
  (`model::totals_on_second_row`); `content_top` includes it, so anything placed
  under the bar follows without knowing. The ledger dock shows one Ledger tile
  there (`model::ledger_tiles`), because five build tiles leave no room for five
  more.
- The toast lane (the Paused pill and notices) is centred in the window. A
  two-pane Live workspace would cover it, so `model::toast_lane` then centres
  it between the goal tracker and the workspace; an open calendar hides the
  pill. A new wide panel has to be added to that rule.
- A ledger entry with `Extra::Card` is pinned: the ledger draws it in a fixed
  foot under the scrolling rows, so facts about a chosen row stay in view.

## Live and Shape

- Live's chrome is `player_hud/hud.rs`: `spawn` builds the goal tracker ("Live
  goal"), the needs ("Live needs"), the hotbar ("Live hotbar") and the prompt
  lane ("Live prompt") from kit parts; `place` puts them for the window and the
  bar's height; `refresh(world, Facts)` fills them from one observation. The
  pure parts (`need_status`, `need_sentence`, `hotbar_rect`, `prompt`,
  `goal_button`) are unit-tested without a window.
- The workspace is `player_hud/workspace.rs`: one kit panel in the right
  column ("Player workspace"), `width(tab, class)` for one or two panes (an
  empty pouch is one line above the list, `pouch_layout`), and a
  reconciler for `player_hud::model::Row`. A row with `inputs` draws chips; a
  disabled row with a `reason` draws the sentence beneath; `compact` rows share
  a line (the Can make / All switch, the directions).
- The prompt stands above the hotbar; beside a workspace wide enough to cover
  it (two panes), `hud::prompt_lane` moves it above the needs, into what the
  workspace leaves free.
- Live shows outcomes with `island_shell::notice(world, text)`: the same toast
  lane as Oversee. Notices said together wait their turn (`model::queue_notice`,
  at most four; a waiting one shortens the shown one to two seconds). Do not
  add permanent status lines to the HUD.
- Live's depth is `player_hud/depth.rs` (reads of `village::view`, the
  `LiveRequest` command door, outcome sentences from the journal) and
  `player_hud/rest.rs` (the "Sleeping veil", a vignette with a strip in the
  prompt's place and nothing in the middle third, and the "Day summary"
  card). A left click on your own cottage in Live selects it and opens Build
  (`player_hud::home_click`); the homes view cuts the selected home away.
  The goal tracker shows "Step N of M", pips (`w::pips` / `w::set_pips`) and
  what the current goal unlocks; a goal done is said once as a toast. The
  working pill has a bar where `activity_progress` knows a total and "3 of
  10" for a repeat order. A need's tooltip carries `need_reason`; a click
  opens the pouch filtered to what helps (`Action::NeedHelp`). A workspace
  row may carry `meter` (a bar with words) and `pips`; `card: true` draws
  any row as a detail card with buttons.
- Live outcomes (gifts, tiers, greetings, invitations, help, repeat orders,
  homestead work) are harvested from the journal by `depth::outcomes` and
  said on the toast lane; `island_alerts::model::classify` puts them in the
  Journal as Social and Homestead notes.
- Shape's resources are `shape_tools/resources.rs`: the rail's Resource (N),
  Remove (E) and Export tiles, the "Shape resources" panel (goods from
  `economy_spec::registry(..).spec.patches`, a richness stepper), the
  "Resource cursor tag" with the simulation's verdict, ground markers and
  patch chips (`layout_chips`: a picture at island zoom, the name on hover or
  close up, never one under another or under the first-run tip). Placing
  and removing are recorded commands outside Undo.
  Export sets `WorldExportRequest`; the host (`frontend/export.rs`) writes
  the file and answers on the toast lane.
- Shape's plots and props are `shape_tools/placing.rs`.
  - The rail's Place tile (L) opens the "Shape plots and props" panel. Its
    grids come from the catalog: buildings with a `field`, and
    `economy.props`. A new entry needs no code.
  - The panel has Turn (R), Remove placed, and a "Placing cursor tag" that
    shows the simulation's verdict (`PlacePlot::check`, `PlaceProp::check`,
    `Remove*::check`). The ghost is a cell rectangle, green or red and
    crossed, with a ring on the cell it faces.
  - Placements take part in Undo and Redo. An entry is stamped with the
    newest stroke recorded before it (`History::issued`), so
    `placement_first` decides which of a stroke or a placement goes first.
  - Undo of a placement records `RemovePlot` or `RemoveProp` with the
    identity it was given, found again by kind, cell and turn. Undo of a
    removal places the thing again. A refused step goes back where it was.
  - Only one Shape tool is held at a time: a terrain brush, a resource tool
    or a placing tool. `shape_tools::other_tool` rests the brush.
- Oversee's Economy page has three tables (`economy::EconomyPage`): Goods,
  Jobs and Priorities.
  - With a village chosen, Jobs is `oversee::profession_table`: Now, Wanted
    and Open per trade. Under each trade is a row with −, + and "Village
    decides" (`Action::Target` → `SetProfessionTarget`), plus the
    simulation's reason when now and wanted differ.
  - A service trade shows the command's refusal instead of controls. A
    control the simulation would refuse is dimmed (`sync`).
  - Priorities is `oversee::priorities`: a Low, Normal, High segment per
    topic (`Action::Priority` → `SetVillagePriority`), lit from
    `oversee::level`, with the simulation's `effect` sentence under it.
  - The whole island ("All") has neither: it asks for a village.
- Shape is `shape_tools/palette.rs`: the rail ("Shape tools"), the brush bar
  ("Shape brush"), the notice lane and the cursor tag ("Shape cursor tag").
  Widgets queue `shape_tools::Action`s; `shape_tools::model` holds the words
  (`tag`, `size_text`, `history_text`, `status_text`, `tool_key`). The size
  slider observes `ValueChange<f32>` and queues `Action::SetRadius`.
- Focus regions: Live is the bar, `FocusRegion::LiveHud` (goal and needs) and
  `FocusRegion::LiveHotbar`; Shape is the bar and `FocusRegion::Palette` (rail,
  then brush bar). An open Live workspace keeps its own Tab loop.

## Host screens, toasts and tips

- Title, new game, pause, settings and the guide live in
  `apps/island_game/src/frontend/ui.rs` and are built from the kit: chrome
  on the title's gradient, paper panels elsewhere, so they follow the panel
  style. `button(...)` keeps the screen's Tab order; `rename(...)` gives a
  control the name automation knows when its words differ.
- A setting is a `settings::Change`. Sliders send `Set...` changes, which are
  `continuous()`: they are applied and stored but do not rebuild the screen;
  `refresh` writes the value back into the slider and its text.
- Host notices go through `island_view::island_toast(world, tone, text,
  lasting, retry)` and `clear_island_toast`; a "Try again" press is read with
  `take_island_toast_retry`. A toast with another button uses
  `island_toast_with(world, tone, text, lasting, label)` (it stays 12 s);
  the same `take_island_toast_retry` reports the press, and the host keeps
  what its button meant (`frontend::export::ExportState`). Do not add
  another notice node.
- New game has two columns: name and the ways to begin on the left, "Which
  island" on the right (`frontend::worlds`: the window's world first, then
  `<slug>-shaped-N.json` exports beside the world rules, newest first, each
  with its date and "N settlements · patches: …"). A file that cannot be
  used is a disabled row with the reason. Tab walks name, worlds, ways to
  begin, Back; the list scrolls past `WORLD_LIST_HEIGHT`. The chosen file
  goes to `session::new_game_on`; the save keeps the whole construction.
- The tooltip card is a root node (`TooltipRoot`). Anything that hides the
  island's roots under a host menu must leave it alone, or menu tooltips
  vanish in the frame they are built.
- A failed save's "Try again" is in `FocusRegion::Toast`, the second region
  of every ring. Menu buttons get a tooltip from `frontend::ui::menu_tip`;
  under a host screen only controls below a `FrontendUiRoot` show tooltips.
- `ISLAND_TIPS_SEEN=all` (also `1` or `true`) in the game's environment
  starts the run with every first-run tip counted as seen, so no tip covers
  a captured scene; the settings file is not written
  (`frontend::settings::TIPS_SEEN_ENV`). Every capture script of a prepared
  save should set it.
- Reduce motion (Settings, Accessibility) sets `ReduceMotion` and
  `AmbientMotion { scale: 0 }` (1 when off); it is kept in the settings file.
- Resident labels the view matches exactly (`resident_inspector::activity_label`):
  the rest leaves "resting at home", "warming up at home", "asleep at home" and
  "asleep at the inn" keep their own words (`own_rest_label`); the generic leaves
  ("finishing the current activity", "working to a plan", "gathering", …) are made
  concrete from the physical action. A new rest label from the simulation must be
  added to `own_rest_label`.
- Two weights of Barlow: `Type::weight` (`Weight::Regular` or `SemiBold`) on
  every step of the scale. `DISPLAY`, `TITLE`, `HEADING` and `FIGURE` (a key
  figure, the bar's totals) are SemiBold, the rest Regular. `widgets::label`
  marks SemiBold text `Strong`, the `typography` system sets the face, and a
  selected tab's label is set SemiBold too. No screen holds a font handle.
- Trades are named from the world's data (`colony_panel::economy::trade_name`:
  the profession's `display`, else its key in words), never from a list of
  known trades.
- Interface sounds come from the kit, not from screens (`game_ui::sound`):
  - **Click** on activating a kit control; **Page** for a tab.
  - **Deny** when a disabled control is pressed. A slider is silent.
  - `ControlSound(Some(..))` overrides a control's sound, `ControlSound(None)`
    silences it.
  - **Hover** at most once every 80 ms (`HOVER_GAP`).
  - **Open** and **Close** when a panel marked `PanelSound` shows or hides:
    the ledger, the workspace, the tray, the card, the inspector, the calendar.
  - Moments that are not a control call `sound::play`:
    - a placed building or patch (**Place**) or a refused one (**Deny**);
    - a host toast (**Confirm** when done, **Deny** when failed);
    - a new critical or warning alert (**Alert**, once however many arrive);
    - the played character's trade (**Coin**);
    - host screens (**Open** over play, **Close** back to it, **Page** between
      menus).
  - First-run tips advance silently: only a press makes a sound.
- Settings' Sound tab has Master, Ambience, Effects, Music and Interface
  sounds, plus Mute. They are kept in the settings file (`music_volume`,
  `interface_volume`) and applied as `AudioLevels`; a mute writes silence on
  every channel.
- Old saves: `island::SaveProblem` tells an older save (header without, or
  with an older, `island_version`) from a damaged one; the title shows
  `island::OLDER_SAVE`. Raise `ISLAND_SAVE_VERSION` when the island's content
  changes in a way old saves cannot follow.
- The Economy page starts with the day's account (`colony_panel::economy::day_lines`):
  the civic fund, the boat's whereabouts, this or the last call's trade, and
  its next order.
- Shape's brush outline is its own gizmo group (`shape_tools::ShapeGizmos`,
  3 px, drawn in front); the Strength switch sets `Brush::step_mm` from
  `model::STRENGTHS`.
- The merchant boat's next order (`BoatView::next_order`) is said through
  `island_shell::model::boat_order_lines`, in the calendar's outlook and in
  the Live Trade tab; nothing when the list is empty.
- A builder's states on the homestead card: hired, walking ("on the way to
  work"), waiting ("between stages of work"), working ("building at your
  door"); a friend walking is "on the way to lend a hand". A new state falls
  back to the simulation's own sentence (`UpgradeWorkerView::text`).
- A tooltip on a row stacked in a list stands beside the list
  (`Tooltip::beside()`, the menus' `place_beside`), never over the next row.
  The alerts use it.
- First-run tips are `island_shell/coach.rs`: `Tip` (text, mode), `current`
  (which one shows) and `place` (where, inside the window at every size; the
  totals and alerts tips stand right of the alert column and under the toast
  lane, clear of the Paused pill and the journal line). A
  new tip needs an entry in `Tip::ALL` and its name in `IslandTips::NAMES`
  (appended: the order is the settings file's bit order).

## Icons (`game_ui::icon`)

Ask for a concept, not a file:

```rust
icon::icon(world, parent, Icon::Planks, 16.0, Tone::secondary(Surface::Paper));
```

- `Icon` names every concept of the spec's mapping table (goods, trades,
  buildings, needs, modes, pages, status, weather). `Icon::of_good(key)`,
  `Icon::of_trade(role)`, `Icon::of_season(name)` and `Icon::status(status)` look
  one up from data.
- Sizes on screen: 12, 14, 16, 20, 24, 32. The tint is the `Tone`.
- `glyph().alone == false` marks a closest-match glyph (planks, flour, hides…):
  always show its name beside or beneath it.
- 92 pictograms come from Maki and Temaki (both CC0), rasterised to two white
  atlases (`assets/ui/icons/icons-32.png`, `icons-64.png`) that are embedded in
  the binary. Nine glyphs (pause, menu, tick, coin, sun, cloud, fog, flatten,
  smooth) are drawn from plain nodes. Callers cannot tell the difference.
- Without an image store (headless tests) an icon is an empty node of the right
  size. No layout depends on an icon being present.

**Adding an icon.** Add the file name to `SETS` in `scripts/fetch_ui_icons.py`
(only files from the two pinned commits), run `python -I scripts/fetch_ui_icons.py`,
add the name to `ATLAS` in `icon.rs` in the manifest's order and add the concept
to the `icons!` table. The tests check that every concept resolves to an existing
file, that every fetched file is used and that `ATLAS` matches
`assets/ui-icons-manifest.json`. The script verifies both licence texts open with
"CC0 1.0 Universal" and refuses to continue otherwise; it uses only the Python
standard library (its own scanline rasteriser and PNG writer), so no SVG tool has
to be installed. A new set or a non-CC0 licence needs the owner's approval first.

## Interface size (`game_ui::scale`)

UI scale = automatic factor × the player's percentage (80 to 140).

- Automatic factor: `clamp(floor_to_0.05(min(width / 1280, height / 720)), 1.0, 2.5)`.
  1280×720 and 1024×768 give 1.0, 1920×1080 gives 1.5, 2560×1440 gives 2.0.
- The host stores `ui_scale` and `ui_scale_auto` (default `true`) in
  `island-settings.json` and writes `InterfaceScale`; `game_ui` turns it into
  Bevy's `UiScale`. Settings has **Interface size: automatic / fixed** beside the
  percentage. Developer launches have no setting and keep the scale they had.
- `scale::layout(world)` gives the window in design pixels and its `LayoutClass`:
  Wide (1280 and more), Standard (1024 to 1279), Compact (smaller, or under 700
  high). Lay out from this, never from raw window pixels.

## Panel style

Parchment is the default. **Panels: dusk** in Settings (stored as
`"panel_style": "dusk"`) swaps the paper role table for the dark one; chrome is
unchanged. Because widgets read roles, a screen needs no code for it.

## The gallery

`island_game --ui-gallery tokens|controls|data|messages|icons` opens a developer screen with every widget and icon in every state,
and a Parchment / Dusk switch. It is for screenshot checks and is never offered in
a menu. `scripts/playtest/capture_ui_redesign.py` captures it and the Oversee
shell at 1280×720 and 1024×768 into `target/npc-out/ui-redesign-p2/`;
`capture_ui_phase3.py` captures the inspector, the cards, the build tray with
its ghost tag and the ledger pages into `target/npc-out/ui-redesign-p3/`.

## Capture scripts and control names

The scripts in `scripts/playtest/` drive the real game over BRP. They find a
control by the text on it or, failing that, by its `Name`: every kit control is
named `Game: <name>`, icon-only ones too ("Game: Menu", "Game: Follow").
`PlayerFlow.click("Panel name > Control")` looks inside one named panel, for
names that exist twice ("Village alerts > Hide"). Rows that hold several texts
(a roster row, a build card, a project row) are read with `button_rows(panel)`
and activated with `press_row(prefix, panel)`. The ledger is opened with
`open_page(page)`, because the tile of the open page closes it. Give a new panel
root a `Name`, and keep control names stable: they are the scripts' handles.
`capture_ui_phase45.py` also sends keys: a `bevy_input::keyboard::KeyboardInput`
message per press and release through `world.write_message`, which is what the
window itself sends, so `ButtonInput<KeyCode>` and every key system see them.
`capture_ui_phase67.py` (title, menus, Settings, Guide, tips, toasts, Dusk;
`--only menus,oversee,shape,live,again`) walks each menu's Tab ring with real
Tab presses, measures the UI scale at 1920x1080, and checks that every tooltip
it raises lies inside the window at 1280x720 and 1024x768 (`tooltips_inside`).
`capture_ui_live2.py` covers Live's depth (`--only people,live,calendar,oversee`).
`capture_ui_live3.py` starts from prepared saves (`target/npc-out/fixtures/`,
written by `cargo test --locked -p island_content --test island_capture_fixtures
-- --ignored --test-threads 1`): `cottage-evening` (a built cottage at dusk,
three known neighbours, one a friend), `upgrade-underway` (a larger cottage
half carried in, the rest ordered and a builder hired, nobody yet on the
way; a chest with goods), `hard-times` (a critical need, no building
affordable), `village-life` (a death this morning, a guest at the inn, an
awake neighbour in reach) and `larger-cottage-done` (every tier built, the
last by orders, haulers and a hired builder).
`capture_ui_live4.py` covers Shape's resources and export from them too;
`capture_ui_live5.py` (`--only worlds,summary,gift,upgrade,inn`) and `capture_ui_live6.py` (`--only settings,worlds,summary,upgrade,alerts`) and `capture_ui_live7.py`
(`--only settings,worlds,summary,upgrade,builder,alerts`; it steps the clock 20 ticks at a time to
catch the builder's short stage at the door, and hovers alerts from the `village-alerts` fixture)
and `capture_ui_live8.py` (adds `builder-door` and `boat`)
cover New game on an
exported world, the gift and its tastes, the physical upgrade and the inn's
guests. It covers the menu tooltips, the homestead tiers, sleep
from the prompt to the day's account, the chest, the critical-need pulse and
Reduce motion, the unaffordable build card and a failed save's "Try again".
A capture copies the save it uses: the game writes into the file it opened.
Both use only controls and keys of the game; every world change is a recorded
command those controls submit.
