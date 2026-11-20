# Terra World native interface

## Direction

Preserve the forest-green identity while making the rendered world the main surface. This is a desktop game with operational controls. The interface recedes during first- and third-person control, and expands for construction, trading or world inspection. No marketing copy, decorative metric cards or ornamental animations.

## Tokens

- Surface: deep forest `#101c1a`, high-opacity for legibility over changing terrain.
- Secondary surface: `#1b3029`; hover `#2a483b`; selected `#385b43`.
- Primary text: warm ivory `#eef1e4`.
- Secondary text: pale sage `#b2c4b5`.
- Action accent: meadow `#b3dc8e`; trade/warning accent: warm sand `#e6c38b`.
- Error: muted coral `#f09c89`, always accompanied by a written reason.
- Font: bundled DejaVu Sans. Main title 25px, section title18px, body13px, secondary12px; no text smaller than11px.
- Spacing: 4/8/12/16/24px. Buttons at least32px tall with8px horizontal padding. Tight related controls, larger section separation.
- Corners: 5px controls,8px panels. Use surface separation without stacked borders and shadows.

## Surface composition

The 64px top bar exposes RTS, third-person and first-person modes, world drawer, atlas and inspector toggles. The 256px world drawer is hidden initially. The 300px inspector shows Activities, Command, Nature, Territory, Build, Market, Industry, Factions, Driving, Garage or Fleet; only the chosen panel is present. Activities opens first with next-step guidance and funded supply work.

The footer carries actionable status, key guidance and selected/controlled-actor
context, including named needs. Group values are explicitly the lowest among
selected owned biological bodies; vehicles and inspect-only bodies do not dilute
them. Empty or unavailable selection stays explicit instead of borrowing the
active body's identity. Only the exact admitted owned vehicle displays its
driver's needs, and received remote context is labeled. Loading and storage
feedback retain priority. Its height is **96px × text scale**, with a fixed
12px bottom margin. The usable world viewport reserves **96px × text scale +
26px** from the bottom: 122px at 100% text and 170px at 150%. The world drawer,
inspector and performance pane share that bottom boundary. Atlas captions and
scroll tracks retain another 6px and 14px respectively; floating driving panels
retain another 15px. World hit testing and the controller terrain cursor use the
same viewport, so increasing text does not leave interaction space underneath
the enlarged footer. The [shared layout](crates/terra-demo/src/interface.rs)
updates these bounds when text scale changes. Source attribution is available
in the world drawer and the source label remains visible.

F12 opens a separate 300px performance pane in place of the world drawer. Frame
intervals, local CPU stages, terrain tiers, AI decisions and network bytes belong
there. The pane labels its measurement scope and scrolls at the minimum window.
World settings expose quality, atmospheric sky and the possession focus policy
before geographic and layer controls.

Default camera is an oblique settlement overview. Selection uses outlined ground rings, a clear current mode label, and a drag rectangle. First-person hides the possessed body's exterior mesh. Third-person follows with terrain clearance. Switching control does not replace player identity, inventory or ownership.

A body-focused return to RTS restores zero yaw with the overview offset and
pitch so the selected body stays in view after a rotated possession camera.
A Release requested by a location or atlas action preserves its explicit
spectator destination and orientation.

## Workflow rules

- RTS: left select, Shift add, drag select, right-click order; selected owned units are listed.
- F1 returns to RTS; F2/F3 enter third/first person on an owned selection; Escape opens the session menu; Tab focuses selection.
- People use view-relative movement and sprint. Registered road vehicles use W/S pedals, A/D steering, Shift brakes and Space handbrake. E opens the relevant interaction or uses an assigned transport stop; C switches camera.
- Entering possession hides world/inspector panels once. Explicit E/J/N/T/F10/F11 actions remain usable while possessed. Returning to RTS restores previous panels; switching first/third person does not repeatedly hide a reopened panel. Text entry takes priority. The focus policy can be disabled.
- Graphics controls never change authority, source heights, stock or simulation speed. Unsupported atmospheric rendering falls back visibly; Performance disables that path intentionally.
- Build placement has pointer priority over unit selection and reports reach, surface and ownership failures.
- Trade exposes stock, buy/sell prices, distance and the player's wallet. Failed transactions explain the unmet condition.
- Needs are named food, water and energy, with numeric values; color is supplementary.
- The default Activities view retains actor context in the footer. Command
  repeats the same detailed identity and needs, explains action targets, and
  keeps complete names while compact HUD names are bounded. In-world labels
  wrap at words or characters for long single-token names.
- Industry exposes recipe flow, finite stocks, jobs and delivery progress. Source versus authored placements are explicit.
- Nature exposes depletion, renewal, required equipment and access rights. Watching wildlife is inspection; the active owned body remains the interaction context.
- Territory exposes public stock, treasury, current infrastructure and every next-tier requirement. Contributions and upgrades state their 20 m reach.
- J/N/T open Activities/Nature/Territory. Shared-resource access and construction permissions must remain distinguishable.
- F5/F9 use the configured persistence mode: explicit JSON save/load by default, or checkpoint/restore for an opt-in regional store. The World drawer names autosave and restore semantics; F12 shows generations, residency and checkpoint costs. A blocked write uses written error/recovery feedback and pauses authoritative mutation. Network authority remains named and server-owned controls are disabled.

## Verification

The native launcher has Home, Your worlds, Backups, Recovery review, Character,
World settings, Review, Join, Settings and Controls screens. Keep the form's primary action visible,
names editable and keyboard focus in view. An in-game overlay preserves local
world-clock metadata while pausing virtual time; network menus only suppress
local intent. Saving has its own pending/error state and leaving waits for the
actual commit. Driving and Garage join the in-world inspector; the driving HUD
shows speed/gear, fuel/condition and the current destination. Technical counters
stay in diagnostics.

Inspect one batch covering RTS overview, possession, commerce, industry, atlas and the minimum window; fix material findings together and confirm once. Functional command, authority and persistence assertions use automated tests and saved state in addition to screenshots.

## 0.7 operating surfaces

Fleet [Y] joins the inspector with Company, Drivers and Routes subpages. Display
company and personal balances together, separate cumulative fares/payroll from
spendable cash, name the selected employee's real assignment and give explicit
blocked/resume guidance. A useful passenger-loop suggestion fills an editable
draft from real stops; it never creates a route until the user chooses Create.

Controls in both menus expose capture, duplicate/reserved-key errors, cancel,
sensitivity/deadzone/inversion, reset and explicit save. Dynamic in-world key
hints use the active mapping. Menus, management and RTS have controller paths with explicit context.
F6/right-stick click gives management a visible focus outline and reveals the
selected action through actual scroll bounds. The terrain cursor remains inside
the usable world view. Political atlas drawing stays mouse/keyboard; physical
hardware coverage must not be implied by virtual input tests.

While driving, an unobscured right corner holds a 208-pixel north-up road/route
map. Hide it behind an open inspector/world drawer. Cache road guidance and
raster updates at bounded intervals. Native wheel animation inherits the
smoothed terrain-aligned visual root and hides when an external actor scene
replaces the procedural mesh. Presentation never changes contacts or motion.

World setup shows storage near source selection. Pause names the world and last
durable checkpoint once; regional counters remain in diagnostics.

## 0.9 source and depot workflows

Multiplayer exposes an optional matching local source configuration in the Join
form, with plain explanation of fixed data and source availability. Data/terrain
errors are reported before leaving the form where practical and admission checks
all source identity fields. Source labels retain real versus synthetic attribution.

The Depot page exposes stock, quantity, reachable shop, actual buy/sell quote and
useful freight drafts. Missing buyers, funds, stock, storage or vehicle capacity
produce clear disabled reasons. Transferring private cargo does not display a
public fare. On-screen drafts accept normal typing as well as controller input.


## 0.10 guidance and compact presentation

Activities shows the current goal selector and concrete action before detailed
explanation. Step counts express the current workflow, not permanent checklist
completion. An unobtrusive second source-banner line keeps the next goal and J
shortcut visible when immersive control hides the inspector. A changed action
requires a fresh review instead of applying an unseen transaction.

At small heights or enlarged text, the inspector replaces its fixed page grid
with Choose activity. Opening the chooser deliberately reveals navigation;
selecting a page closes it. F6 focuses the visible chooser, and content retains
its scroll area. Wide windows keep the direct tab grid. The measured banner
height is excluded from world input; scaled footer reservations stay shared.

Asset diagnostics show loaded, pending and fallback instances with a compact
content-identity preview. Exports retain the complete digest. Original woodland
models match the geometric environment; human appearance and vehicle controls
remain recognizable. Pack errors explain repair or procedural fallback.

## 0.11 market and action handoffs

Market shows one selected seller, total prices for the chosen quantity, shop
stock and carried stock. Unavailable trades explain their concrete blocking
condition in the same row. Choose market cycles nearby loaded sellers; guidance
opens the exact seller it selected. Missing explicit sellers require a fresh
choice instead of silently redirecting a purchase.

Price snapshots refresh with labels. Input handlers compare the displayed quote
with current authority before enqueueing, and the host checks it again before
mutation. A price, quantity, player, restore or seller change requires review.
Depot and roadside actions follow the same visible-quote contract. Longer market
rows remain in the inspector's existing scrolling body so enlarged text retains
readable prices and reachable controls.

Market and production travel reuse the goal planner's physical stopping-point
checks. Exiting a vehicle either places the avatar at a safe supported point
beside it or explains why the vehicle must move. Failed actions preserve the
current selection and authority state.


## 0.12 Fleet finances and section navigation

Fleet has Company, Finances, Drivers, Routes, Recovery and Depot sections. Keep
company cash and employee/route counts in the shared summary. Current tabs use
the existing selected palette. When the inspector has little vertical space,
show the current section with a chooser; selecting a section closes it. A section
change resets the inspector scroll, while rescaling text preserves position.

Finances leads with the cash result before funding and after known owner-paid
costs. Before a company exists, show one registration prompt and an immediately
reachable **Manage company** action; keep the shared empty summary short. Show
the detailed cash movements, owner funding, category totals, coverage and recent
entries only after a company exists. Use exact two-decimal money and an explicit
minus sign. Deposits must never appear as earnings. Imported history, unpriced goods and vehicle costs
before assignment remain understandable in the same view. Avoid a profit label
when a complete cost basis does not exist. Detailed rendering reservations stay
in diagnostics; budget deferral must not make a gameplay actor disappear.

## 0.13 route decisions and community review

Keep route editing controls and Create reachable before the longer Before
dispatch explanation. The first-departure preview names observed supply, current
receipt and payroll-only coverage, then identifies unknown costs and unreserved
funding. A retail comparison names the seller, available quantity and hypothetical
cost for the requested full load. Detailed explanations share the existing scroll
body and never turn into a second transaction or a profit guarantee.

Factions names invitations and the selected community, displays a paged roster,
and makes role restrictions explicit. Leave, remove, leadership handover and
permanent plot sharing prepare a consequence review in the inspector's scroll
body. It names the exact community/person/property and the retained public
assets. Management focus moves to the new confirmation when a review opens;
ordinary navigation remains under player control afterward. Changed player,
restore, selection, roster or ownership requires a fresh review. Fleet financial
and destructive actions follow the same identity discipline.

## 0.14 continuing Company guidance

Preserve Activities' compact selector, goal and primary handoff before detail.
The Company goal names the actual driver, assignment, next stop and current
blocker. Loaded obligations and the distinction between payroll pause and
continuing rental time remain explicit. Open the reviewed Fleet section and
identities; a changed service or restore requires a refreshed suggestion.

Diagnostic capture configuration remains an explicit developer/operator CLI
choice. Routine player surfaces do not gain sampling counters or file controls.
The existing F12 view retains its gameplay-independent role.

## 0.15 recovery and model-specific presentation

Your worlds offers Make backup beside a supported, checkpointed local world.
Backups and recovery remains available when the original profile cannot be
opened. A backup must be fully verified before a named recovery review; the
primary action says it creates a new world. Back cancels that review. Progress,
failure and completion use the existing written status surface and scroll/focus
rules. Keep the exact completed backup or recovered profile reachable even when
the recent list is full. Opening a recovered world remains a separate action.

One background operation runs at a time. Navigation is allowed during work,
but a stale listing, changed metadata or changed selection cannot silently
retarget confirmation. Ordinary players see the world title and useful next
action; hashes, paths and storage mechanics belong in the relevant review or
operator documentation. A failed attempt must retain the original and every
published backup.

Imported vehicle appearance follows the actual public model. Wheel travel and
steering reflect admitted motion; stopped or paused vehicles must not appear to
drive. First-person keeps the existing cockpit and hides the possessed exterior.
When an imported model has a fixed livery, Garage explains that limitation and
does not offer a paid paint change whose result it cannot display. Other native
vehicle customization and authority remain unchanged.

Both floating driving cards yield while World, Inspector or Performance is open.
Restore them together when those drawers close; retain the ordinary footer and
its controls. Enlarged text uses the existing scroll regions without leaving
fragments of the hidden driving card visible beside a drawer.

## 0.16 commitments and recognizable copies

Activities keeps the current goal and primary handoff ahead of detail. Goal
selection expands to six explicit choices, highlights the selected goal and
collapses after selection. Your commitments names accepted work, the world-clock
deadline and the relevant owner funds/obligations. Review request opens the exact
shown request; it does not travel, deliver or pay. Extra details use the existing
scroll body at enlarged text.

Your worlds offers Rename beside each valid managed profile and shows creation
time in UTC with seconds. The name editor displays the selected character/source,
preserves editable-field focus, and returns to the named entry after Save name.
The recovery review offers a name for its new copy; progress names that chosen
copy before the original backup and checkpoint. Back cancels without a write.
Long names wrap within buttons; the original forest palette and native focus
controls remain consistent across these screens.

An animated person's profile choices must survive ready/fallback and standing/
seated handoffs. The visual driver follows the vehicle, and first-person hides
both exterior and rider. Paused content must initialize in the correct pose and
remain still. No duplicate person should remain visibly at the boarding point
while the existing authority identifies that person as manually driving.

## 0.21 supply requests, startup and source edits

[Supply maintenance](crates/terra-sandbox/src/settlement/economy.rs) retains one
central issuer: the first shop in the authoritative shop ordering supplies both
the valuation and the escrow. Shops may have different
assortments. If that issuer has no price for another shop's requested item, skip
the request before consuming an offer slot, contract ID or funds. Continue to
consider supported shortages in the same bounded maintenance batch. Production
priority, the four-offer batch limit and finite issuer funding remain unchanged.
Activities presents the requests actually posted; an unpriced shortage does not
gain a guessed reward or an alternate payer. Ordinary merchant quotes retain
their own seller identity and review requirements.

[Population bootstrap](crates/terra-bevy/src/living.rs) and
[native loading](crates/terra-demo/src/flow.rs) use the same player-centered geographic
neighborhood as the stream scheduler, including longitude wrapping and clipped
polar rows. Startup needs up to nine relevant pages; a complete six-page polar
neighborhood can be sufficient. The source and nearby-terrain percentages count
only those relevant resident keys, and terrain readiness requires geometry from
the current immutable source instance. Visible geometry retained during source
replacement cannot satisfy that replacement's readiness. The legacy rendered
chunk count remains a diagnostic count. Account/network admission, pending or
blocked storage, and supported dry ground still govern entry and population
initialization. Missing or underwater terrain does not become valid spawn ground
because a loading percentage reaches 100%. Screenshot verification retains its
full displayed-neighborhood requirement.

[Mesh integration](crates/terra-bevy/src/render.rs) applies feature removal,
source-resource depletion and regrowth according
to each resident chunk's exact effective set of removed feature IDs. Rebuild only
chunks whose effective set changes, including every resident fragment of a shared
feature. A quality change can still rebuild the whole resident region. Keep at
most two tracked mesh jobs, retain the existing exponential failure delay from
500 ms up to 30 seconds, and preserve visible geometry while replacement work
is pending or retrying. A completion must match the current source instance,
quality revision, LOD and exact edit set before it installs; an edit or restore
while work runs must remain represented in pending work. Detailed work/failure
counts belong in F12 and retained diagnostics. Reduced job counts establish less
rebuild work; frame-time and GPU claims require measurements on the target system.

## 0.24 reviewed operations

Industry preserves the explicitly chosen facility when it leaves the current
view. The next action names Pause or Resume and the facility; owner/reach
recovery appears as written context. Next facility is the deliberate way to
choose another target. Normal production progress does not invalidate a review.

Fleet operations retain the displayed driver, assignment, route, vehicle, depot
and amount that determine their meaning. Routine motion, payroll and rental
countdowns are not reviewed operands. Changed selection or eligibility needs a
new action; the interface never silently substitutes the first loaded object.

Build presents the permitted target's name/ID and current health. Review damage
opens consequences in the existing inspector scroll body with Cancel before
Confirm and initial management focus on Cancel. It names destruction/home effects
when applicable. A changed target, health, ownership, actor, restore or reach
expires confirmation. No structure gives a disabled review action and a concrete
building/permission recovery step. No hold-only confirmation or new modal is added.

Keep the forest palette and current 100–150% text behavior. Review at 1440×900 and
1100×700 with 150% text; full 200% reflow remains a separate product requirement.

