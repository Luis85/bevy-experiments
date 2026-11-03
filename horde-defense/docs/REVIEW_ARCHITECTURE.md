# Native application architecture review

The simulation remains a standalone `horde_core` Cargo package. Its world units,
fixed ticks, rules, random state, reward ownership and persistence payload do not
depend on Bevy, egui, audio, OS input or a renderer.

## Findings that drove the refactor

The original native frontend advanced the game, scheduled autosaves, wrote files
and spawned audio players from `EguiPrimaryContextPass`. A render-frame guard
prevented one obvious double-tick case, but ownership still depended on a UI
layout callback. The installed `bevy_egui 0.43.1` source invokes context schedules
from `Context::run_ui`; egui may request additional layout passes. Consequently,
a UI function is an unsuitable authoritative clock or transaction boundary.

The installed Bevy 0.20 main schedule runs `PreUpdate`, its fixed-update loop,
then `Update` and `PostUpdate`. `FixedUpdate` is explicitly the schedule for
fixed-step gameplay. These local crate APIs, rather than remembered examples
from earlier Bevy versions, informed the implementation. Egui remains responsible
for native controls and presentation, with no custom input backend in core.

## Ownership and interfaces

`runner.rs` supplies a small application service layer:

- `Runtime` owns typed command queues, same-frame intent deduplication, held and
  one-shot input, save requests, autosave timing and pending sound cues.
- `GameCommand` names legal operations such as deployment, equipment changes,
  training, tactical pause, building, country orders and recall. Commands call
  the public core APIs; they do not duplicate combat or reward procedures.
- `RuntimeNotice` reports a command's exact result, a durable save result or a
  finished main attempt. The UI changes navigation and messages from those
  results instead of assuming a purchase or write succeeded.
- `RuntimeHost` exposes only the game/runtime pair, current settings, save paths
  and a notice receiver. The existing `Frontend.game` aggregate is retained as
  a facade so the refactor does not require rewriting every presentation helper.
  It can later become a separate `GameSession` Bevy resource without changing
  the engine-free core or the typed command protocol.
- `load_profile` centralizes startup reads and backup recovery selection. Runtime
  saves use the core's validated atomic generation procedure; settings are
  coordinated by the same application service.

Production systems are named and scheduled explicitly:

| Schedule | System | Responsibility |
|---|---|---|
| `PreUpdate` | `runner::apply_commands::<Frontend>` | Apply queued semantic intents once; acknowledge requested saves after commit. |
| `FixedUpdate` at 60 Hz | `runner::advance::<Frontend>` | Advance authoritative whole ticks and consume one-shot controls once. |
| `Update` | `runner::persist::<Frontend>` | Commit due autosaves outside rendering and fixed combat procedures. |
| `Update` | `runner::play_sounds::<Frontend>` | Consume bounded authoritative sound cues and spawn native audio players. |
| egui context pass | `ui::draw` | Read state, render controls, publish input snapshots and queue commands. |

`flush(&mut host)` and `step(&mut host)` are the same services used by production
systems and headless widget/schedule tests. They require no OS window or GPU.
Their boundaries make input and persistence behavior testable without pretending
that those tests establish graphical quality.

## Clock, input and pause contract

Set Bevy's `Time<Fixed>` to 60 Hz explicitly. The core still receives one
`Game::tick` call per authoritative 1/60 second. Observation speeds 1×, 2× and
4× execute one, two or four complete ticks per fixed opportunity. They never
multiply damage, skip actions or provide extra actions per simulated second.

A `ControlFrame` distinguishes held actions from pressed edges. Held fire may
remain active across fixed ticks; a clicked reload, ability or melee intent is
consumed by the first eligible tick. If no fixed opportunity occurs in one
render frame, the pressed intent waits for the next opportunity. Repeated egui
layout passes merge edges and deduplicate commands by native frame identity,
so they cannot issue a second purchase or simulation step.

Disabled input and tactical pause clear pending combat actions. Paused time
creates no hidden combat or autosave catch-up. Core pause also freezes parallel
battles and strategic outbreaks. UI selections may still be made while paused;
only legal queued preparation or construction operations mutate the game.

## Persistence and feedback contract

Each successful persistent command marks the runtime dirty. Banking during
combat requests periodic autosaves after ten simulated seconds. Multiple save
requests share one coherent write. A failed save is reported, automatic retry
is throttled, and a manual retry remains possible. A Quit notice exits only
when both the campaign generation and current settings have committed.

Results, menus and settings navigation must use `RuntimeNotice` acknowledgements.
The UI must not call `Game::save`, `Settings::save`, `Game::tick` or spawn combat
audio from a layout callback. Startup review fixtures are isolated test data;
they do not authorize renderer-driven mutation in ordinary play.

## Regression strategy

Runtime tests cover a purchase across repeated UI layout passes, one pressed
reload across 4× observation and many fixed ticks, paused-state conservation,
equivalent whole-tick outcomes at 1×/4×, successful durable quit, failed-save
quit rejection, and invoking the actual named fixed schedule without egui.
Existing event-driven widget tests should flush queued intents after a rendered
frame, preserving assertions about purchase outcomes and saved deployments.
Core simulation and rules tests continue to run independently.

These are architecture and automated behavior checks. They do not replace native
input/playthrough evidence, asset inspection, accessibility perception or a
measured target-hardware frame-time benchmark.

## Completed native organization and safety follow-up

The facade and draw/publication entry point now remain in `ui.rs`; presentation
screens live in `ui/menu.rs`, `camp.rs`, `character.rs`, `armory.rs`,
`progression.rs`, `battle.rs`, `results.rs`, `preferences.rs` and `operations.rs`.
`inspector.rs` reads actual enemy/structure state and authoritative resolution
logs. `ui/review/mod.rs` owns explicitly staged startup/practice scenarios; fixture
simulation does not run inside an ordinary widget layout callback.

Held/pressed input publication forces personal combat to 1× even after an NPC
or world observation used 4×. A successful deployment also resets the view speed.
Frame-scoped guards cover UI firing toggles and construction hotkeys as well as
the semantic command queue. The terminal-attempt controller stops remaining
fixed catch-up until the next enabled session input.

The armory filters definitions once, sorts references to owned item records,
and renders only visible rows through egui's row virtualization. It does not
clone or draw an unbounded stash every frame. Comparison uses complete core
loadout metrics; a sale names affected presets/survivors before a separate
confirmation. The root game coordinator reconciles idle survivor equipment.

Campaign and settings writes both use synchronized temporary files. Before
replacing either existing campaign generation, the application validates each
one through `Game::decode_save_generation`. Unreadable original bytes receive
unique synchronized recovery copies alongside the save; copy failures stop
replacement. The frontend reports the recovery-copy paths. Fresh in-memory
fallback therefore cannot silently destroy the original unreadable generations.

`LASTLIGHT_PRACTICE=1` chooses an independent persistent profile beneath
`practice/<scenario>` in the save directory. A valid practice save resumes
instead of being restaged; ordinary campaign paths are untouched. Automated
screenshot fixtures use fresh temporary profiles and mute sound; human practice
retains configured audio. The `peak` fixture is expressly a paused render scene,
separate from simulation timing or minimum-spec performance evidence.

The latest verified application pass at handoff ran **26 tests**, all passing,
and application Clippy with warnings denied. This includes actual widget
confirmation/remapping workflows, a 1,004-item visible-row/filter case,
independent practice save/resume, corrupt-generation preservation, fixed
schedule execution and one-shot input conservation. Native screenshots, OS
input and the combined workspace gate are recorded separately by the final
playtest pass; these headless tests make no graphical-playthrough claim.
