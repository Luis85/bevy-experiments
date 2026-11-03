# UI/UX refinement review — 2 November 2026

This pass preserves Lastlight's established field-outpost palette, Barlow
headings, native controls and fixed eleven-lane arena. It refines the existing
flow and feedback; combat rules, purchases, rewards and simulation clocks remain
owned by the core and typed application runner.

## Evidence reviewed

The 25 native captures under `target/playtest-review/screenshots/` cover the
previous completed build, including minimum-window variants. The review used a
whole-flow contact sheet plus full-size inspection of minimum personal/NPC
combat, operations, world selection and the result screen. The GDD, expansion,
playtest guide and prior independent review were read alongside current input,
HUD, preparation and persistence interfaces. These are historical baseline
captures, not silently replaced regression oracles.

## Changes and their purpose

### First deployment

The starting-country action moves directly below the selected country's name,
before optional population/modifier detail. Search sits above the map; a
nonempty query shows its matching country buttons before the map. This makes a
small-country choice and the primary action reachable in the minimum viewport.

A fresh battle presents short contextual guidance based on actual shots,
reload/contact history and paused state. It teaches the fixed defender, all-lane
aiming, renewable magazines and the shared contact line. No deliberate defeat
is requested. Guidance is dismissible and controlled by a saved preference;
older settings default to showing it. The `intro` review fixture exposes the
real fresh-deployment state without purchased equipment or blueprints.

Unavailable construction does not occupy the first attempt's button or binding
legend. Pressing its configured shortcut still pauses safely and explains the
first-boss blueprint gate. Owned blueprints retain the complete construction
flow. Locked abilities likewise leave the active combat controls until owned;
the camp remains the place to inspect their progression.

### World time and keyboard focus

Every camp tab shares pause/resume, current expedition/alert counts and relevant
observation speeds in the footer. Escape operates on the whole world from camp,
with the configured Pause binding. Resuming an active main defense returns to
its battlefield, so manual combat cannot run invisibly behind preparation.
Text edits, open popups and the feedback-details window keep keyboard ownership;
typing a remapped pause letter into country search does not pause the campaign.

The operating page no longer duplicates the global clock controls inside its
scrolling content. The clock is available while inspecting equipment,
character development or any other camp tab. Personal combat remains at 1×;
NPC/world observation uses complete 1×, 2× or 4× simulation steps.

### Combat readability

The tactical sidebar puts actions and the current NPC decision ahead of optional
local lore. NPC policy is available in a collapsing section, while observation
controls remain immediate. This reduces the minimum-window scroll needed to
reach reload, selected-lane melee and the current legal decision.

The ammunition column adds a compact action-progress stroke using authoritative
remaining reload/recovery/cooldown values. Its height is fixed. No health value
or legal completion is eased toward a future value. A text-and-color danger
readout identifies contact lanes, nearby pressure or a recent source-lane hit.
Detailed threat information remains available by hover and the existing object
inspector. The arena receives no transition transform or animated layout shift.

### Results and feedback

Results explicitly distinguish attempt proceeds already banked from the current
camp wallet. They show actual available development, local-country progress,
Mara's first-attempt availability and a route to recovered equipment. The
existing preparation, retry, next-defense and recommendation choices remain
literal actions. Reopening the report remains read-only and does not pay again.

Routine acknowledgements expire after five seconds. Errors, recovery notices
and a pending save remain visible until addressed or dismissed. Long details
open in a bounded read-only window, preserving full filesystem/error text
without expanding the arena footer. A fixed-height status line avoids moving
targets when an acknowledgement expires. Practice remains labeled independently
of a transient success message.

### Motion and atmosphere interfaces

The active tab receives a short 140 ms accent transition; the view header has a
160 ms accent. Content and controls are present immediately. Routine feedback
may fade during its final 250 ms. Reduced effects makes these changes immediate
and disables the fade; it never changes how much simulation time is advanced.
Combat movement and atmosphere are separate renderer/audio work, not UI timers.

`Frontend::ambience_target` projects the actual main battle, a visible observed
worker or the selected/active stage into the audio target. It supplies current
mode, context and saved volume gains without audio I/O or random access. The
runner's observed-field bridge selects ordinary worker sounds only for the
explicitly observed field; other jobs retain their critical-alert scope.

## Regression targets and verification boundary

Eleven focused regressions were added for camp-wide worker pause/resume, text
focus, locked construction, the minimum-window starting action, feedback expiry
and sticky errors with unchanged game state, settings compatibility, actual HUD
timing/damage, retained-result navigation and pure ambience selection, truthful personal-mode clock controls and neutral
feedback after error dismissal. Existing
construction/remapping assertions retain their purpose using an owned-blueprint
fixture; a new case checks the unavailable first-attempt path separately.

The final application commands passed from `horde-defense/`:

```powershell
cargo test --locked -p horde-defense --no-fail-fast
cargo clippy --locked -p horde-defense --all-targets -- -D warnings
```

The application run passed **64 tests**, with zero failures or ignored tests;
Clippy passed with warnings denied. It includes the UI regressions alongside
renderer animation, audio mixing, cue priority, storage and input contracts.
The `LASTLIGHT_REVIEW_MOTION` startup hook only unpauses an explicitly named,
non-practice review fixture; the fixed runner still owns every actual tick.
It never changes ordinary profile startup behavior.

New native capture and listening evidence are separate from this completed
source/test pass. No new visual or listening validation is inferred from
compilation, prior captures or headless widget tests. Human comprehension,
audio mix and full-session balance remain playtest evidence.

The final combined workspace gate passed 185 tests and strict Clippy. A fresh visual review identified that the long NPC action could wrap into its fixed-height HUD area. The correction uses native epaint one-row ellipsis inside the original allocation, retaining complete sidebar/tooltip text. The new normal/minimum regression preserves all strict height, visibility and authoritative-state assertions; initial panel sizing is warmed before comparing established layouts. The earlier reload-visibility test also continues to pass. Final native confirmation is recorded in [VALIDATION.md](VALIDATION.md).
