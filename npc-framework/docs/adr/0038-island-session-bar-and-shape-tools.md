# 0038: One session bar, one clock, view-planned terrain tools

Date: 2026-11-10. Status: accepted for the Phase 4a increment. Revised
2026-11-11: terrain strokes record one batch command per dab, and the resident
inspector, calendar and roster are recorded below.

## Context

Island Village has three ways to play one persistent world: a character, the
colony overview and the terrain editor. They shared one ledger panel, the clock
controls existed only in that ledger, and terrain editing was a single absolute
stamp with no undo. The review (`docs/superpowers/specs/2026-11-10-island-village-review-and-roadmap.md`,
P2, P3, P5, P7, P8, P9, P11, P14) asked for a persistent switcher, a shared clock,
alerts, an economy overview, real terrain tools and settings.

## Decision

**Session bar.** In the player session `testbed_view::island_shell` owns a bar
with Menu, **Live / Oversee / Shape** (keys 1 / 2 / 3) and the clock. It is
presentation and input only. Every mode change still goes through
`island_control::request_mode`, so recorded arrival, deferred native detach
while work owns reservations (ADR 0036), foreign-controller refusal and the
successor flow are unchanged. Developer launches have no bar and keep the
ledger's own mode, clock and landscape rows.

**One clock, also in Live.** Space, `-` and `=` and the bar's Pause / 1× / 3× /
10× write the host's `SimControl` in every mode. Character play may pause and
change speed: the world is single-player, and the multiplier decides only how
many ticks run, never what a tick does. The bar consumes those keys in the
player session so the host's own speed keys do not act twice; Space still
activates a control that has keyboard focus.

**Alerts and economy are derived view state.** The alert feed reads applied
journal effects and existing components; the economy table reads holdings,
`MarketState` reference prices and labour vacancies, and keeps its own bounded
stock samples for trends. Both are scoped to the session epoch, start at the
present, and are never saved, hashed or read by the simulation. Trend sampling
is rate-limited (200 ticks) and runs regardless of which panel is open; it does
not change authoritative scheduling.

**Shape tools plan in the view and record one absolute command per dab.**
Raise, Lower, Flatten, Smooth, Paint, falloff and drag painting are computed
from read-only terrain queries into absolute targets. A dab whose whole brush
ends at one value records the existing `Terraform` disc; any other dab records
one `TerraformCells` batch that lists every changed cell with its own height
and surface. The simulation validates a batch as a whole (occupancy, cliffs,
connectivity) and applies all of it or none. Undo and redo are one further
batch each, restoring values captured in a bounded, session-only stroke
history. The view bounds a stroke to 600 dabs and 4,096 cells, so its undo fits
one batch, and rate-limits dabs to ten a second.

`TerraformCells` is a regional (`living_world`) command, added after this ADR
was first accepted. The older
`island_colony` world has no batch command: there an uneven dab still records
one radius-0 `Terraform` per changed cell, at most six a tick.
`ISLAND_VIEW_SHAPE_BATCH=off` makes the living world record the same way for
comparison.

**Settings are host files.** Volume, mute, interface scale and window mode live
in `island-settings.json` beside the island save, parsed leniently field by
field. `--mute` and capture runs stay silent regardless of the file.

## Consequences

A soft or uneven stroke in the living world costs one terrain validation per
dab, not one per cell; a refused dab changes nothing, and a refused undo or
redo stays in the history to be tried again. The legacy world keeps the
per-cell cost, which is why its strokes lag behind the cursor. Undo history
does not survive leaving the session. Key remapping, gamepad input, build
placement, overlays and profession +/- controls remain future work.

## Addendum (2026-11-11): inspector, calendar and roster

**The resident inspector is derived view state.** In Oversee, the selected
resident's card reads existing components, `ResidentTraits`, the world's
schedule blocks and the resident's decision record. `testbed_view::
resident_inspector` keeps the latest `VillageDecision` journal effect per
resident for the session epoch, plus the latest one that is not "carry on", and
falls back to the live, read-only `resident::explain` when the journal holds no
record for the running leaf. The "Why" text is built by a pure function from
that record. Nothing is saved, hashed or read by the simulation, and the card
records no command. Open state is the existing view `Selected` resource: a
click on empty ground or Esc clears it. While the card is open the alert feed
folds to its header and the marker key hides, so the card fits 1280×720 and
1024×768 under the bar.

**Follow is camera intent only.** `island_camera::Follow` carries the
resident's drawn position; the framing system keeps it centred in the area
clear of the ledger and the card. Zoom and orbit stay the viewer's; a manual
pan ends it.

**One activity wording.** The card, the roster and the caption in the world
use `resident_inspector::activity_label`: the running leaf's label, made
concrete by the physical action where the leaf only says "work" or "carry on".

**The calendar reads the simulation calendar.** The clock in the bar opens a
read-only popover computed from `calendar_at`, `DAYS_PER_SEASON` and
`SimConfig::night_permille`. Its growth and cold lines come from the patch
definitions (`winter_regrowth`), the cold rule and the hearth tuning of the
running world; a world that exposes none of them gets no such line.

**The roster sorts and filters in the view.** Sort key and settlement filter
are ledger state. The selected resident's job and home commands moved one step
away from the roster and are still the same recorded commands.

## Addendum (2026-11-11): build menu, projects and building cards

**Placement is planned in the view and decided by the simulation.** In Oversee,
`testbed_view::island_build` offers the buildings of `village::civic::catalog`.
The ghost shows the simulation's own `placement_check` for the cell, turn and
building under the cursor; that check floods the map, so it runs when one of
those changes and twice a second otherwise, never per frame. A click records one
deferred `PlaceBuilding`; the tick that applies it validates it again, and a
refusal is shown with the simulation's reason. Cancelling is the ledger's
`CancelBuilding`, behind a confirmation. No other state is written.

**Sites and buildings are drawn from the civic index.** One visual root per site
or building, spawned and despawned on change of `CivicState`; a site that
becomes a building gets a new root, so the systems that dress homes and
workplaces (window light, smoke, roof colour, stock piles) treat it as new.
Homes and workplaces built later are drawn in the founding layout turned to the
plot; the Startup spawners skip entities that carry `civic::Building`.

**One left column.** The alert feed's body, the resident inspector, the build
palette and the building card share the column under the feed's header; exactly
one shows. Opening the palette closes the resident card, choosing a resident
closes the palette and the building card, and the feed's **Show alerts** closes
whichever is open.

**The decision log and the waiting clock are view state.** The projects panel
lists `VillageCivicProject` effects read from the journal for the session epoch
and each planner's last review from `civic::planner`. "Waiting for N days" counts
whole days since the view last saw a delivery at the site (from its placement
for a site that was already open); the simulation holds no such counter. Neither
is saved, hashed or read by the simulation.

**Building cards use their own selection.** A click that selects no resident is
tested against the footprints of sites and buildings (and the founding homes and
workplaces at their fixed offset); the hit opens a card. The generic `Selected`
resource stays a resident's, so nothing that follows a selected unit changes.
