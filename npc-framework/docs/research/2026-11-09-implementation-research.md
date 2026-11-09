# Island Village implementation research — 2026-11-09

This review covers architecture, execution boundaries, persistence, observation
cost, authoring, terrain, testing and startup. It follows the playable baseline;
it is a recommendation record, not evidence that the recommendations have been
implemented. No dependency upgrades, core edits or oracle changes are proposed.

The orchestrator reported 369 focused tests, package Clippy checks and formatting
passing before this review. Native run `playtest-player-flow-6da86f62ea` completed
real gathering, crafting, cottage construction, a berry crop cycle, conversation,
save/title/Continue and quit. No available buyer was found in that run's sampled
window, so it does not establish a completed native sale. See the
[validation record](../_notes/2026-11-09-island-player-flow-validation.md).

## Primary sources and applicability

Read on November 9, 2026. Each source is summarized briefly; recommendations below
are inferences from those sources and the repository, not instructions to copy a
different engine's architecture.

| Source | Relevant lesson and application |
| --- | --- |
| [Bard Tree architecture overview](https://bardtreelimited.com/architecture/overview) | Explicit module registration and service ownership make optional features understandable. The framework already has a facade and composition root; adding a second service locator would duplicate those responsibilities. |
| [Bard Tree orchestrator](https://bardtreelimited.com/architecture/orchestrator) | Scheduling policy and domain rules have different owners. Here, presentation work may be reduced independently, but camera distance must not change authoritative tick outcomes. |
| [Bevy: physics in a fixed timestep](https://bevy.org/examples/movement/physics-in-fixed-timestep/) | Separate input sampling, fixed simulation and visual interpolation; camera updates have their own ordering needs. Retain the project's existing host-owned `NpcDriverSet` contract instead of switching schedules merely to match an example. |
| [Bevy: log diagnostics](https://bevy.org/examples/diagnostics/log-diagnostics/) | Frame, entity, process and render diagnostics answer different performance questions. Extend existing profiling attribution before introducing new indexing, parallelism or save workers. |
| [Glenn Fiedler: deterministic lockstep](https://gafferongames.com/post/deterministic_lockstep/) | Identical initial state and tick-associated input must reproduce exact results; platform and arithmetic differences matter. This supports the existing integer model, sorted iteration and replay checks, not a new networking requirement. |
| [Robert Nystrom: Dirty Flag, author source](https://github.com/munificent/game-programming-patterns/blob/master/book/dirty-flag.markdown) | Derived work can be deferred until needed, but caching adds invalidation obligations and should justify its complexity. First remove calculations for hidden or unused UI; only then consider broader caching. |
| [Rust `File`, including `sync_all` and locking](https://doc.rust-lang.org/std/fs/struct.File.html) | Explicit synchronization exposes write errors that dropping a file cannot report; synchronization can block. Keep durability/error handling while measuring save duration before choosing asynchronous I/O. |
| [Rust `fs::rename`](https://doc.rust-lang.org/std/fs/fn.rename.html) | Replacement behavior and filesystem boundaries are explicit parts of the API. The existing same-directory temporary-file strategy fits this contract; failure cases still need application-level recovery. |
| [SQLite: atomic commit and crash testing](https://sqlite.org/atomiccommit.html#_testing_atomic_commit_behavior) | Persistence confidence comes from reopening after failures at different commit boundaries. Apply that testing principle to the existing snapshot format; this is not a recommendation to introduce SQLite. |
| [wgpu 30.0.1 backend selection](https://docs.rs/wgpu/30.0.1/wgpu/struct.Backends.html) | Backend availability and environment selection are distinct from successful execution on a particular adapter. Test the ordinary entry separately from an explicitly selected Vulkan run. |

Web examples provide design evidence. Actual API checks used the installed
`bevy_render-0.20.0/src/settings.rs`,
`bevy_render-0.20.0/src/diagnostic/mod.rs` and
`bevy_diagnostic-0.20.0/src/frame_time_diagnostics_plugin.rs`. The lockfile uses
wgpu 30.0.1. Installed `WgpuSettings::default` reads `Backends::from_env`; no engine
or toolchain change is needed for the proposed validation.

## Recommended compatible improvements

### 1. Skip observations whose result is not displayed

Confirmed source-level unnecessary work, without a measured frame-rate claim:

- `player_hud/build_preview.rs::present` builds a full player observation every
  active-character frame even when the Build workspace is closed and no plot is
  selected. The HUD separately refreshes at 0.2-second intervals.
- `homestead_panel.rs::refresh` returns early during active player HUD use, but
  `player_hud::active` becomes false under `FrontendGate`; the hidden legacy panel
  then observes every frame behind the title/pause shell.
- `agent_io::observe::observe_with` creates all registered sections, text lines,
  affordances and recent events. This is more work than reading one plot field.

First guard these paths by actual visibility/selection. If sharing a projection
later proves useful, give it one view owner and an explicit actor/entity/epoch
stamp; clear it on world replacement and life change. Actions must still perform
fresh shared validation. This narrow order follows the
[derived-work guidance](https://github.com/munificent/game-programming-patterns/blob/master/book/dirty-flag.markdown).

### 2. Retain identified places without retaining unseen live facts

The native rehearsal observed a learned plank recipe disappear while gathering
six logs. The code explains why:

- `village/environment.rs::observe` enumerates `known_targets`.
- `perception_agent.rs::perceived_or_remembered` supplies transient
  `PerceptionMemory` plus nearby unperceivable objects; the shipped world default
  forgets after 1,200 ticks. Six 240-tick log actions alone exceed that interval.
- `player_actions.rs::known_position` applies the same known-target gate.
- `perception/fog.rs::SeenStatics` already persists identified static positions
  per actor, including stations, but is not consulted by those action paths.

Prefer a narrow island content contract for previously identified immovable
places over a new UI-owned discovery database. Existing `register_known` and
`register_thing_position` provider seams can express it without changing generic
core code or adding a saved component. Check compatibility rather than assuming
it: provider registration is distinct from hashed component registration.

Limit retained identities to appropriate places such as workshops and resource
locations. Do not union every seen static: the record also contains items, and
old sightings do not reveal current inventory, merchant location, availability,
foreign plot ownership or crop state. Use recorded last-known positions; recheck
reach and availability when work applies. Cover forgetting, save/restore,
successor/new-world isolation and unseen-state privacy.

The installed provider signatures are
`KnownFn = fn(&World, &AgentCtx) -> Option<Vec<SimId>>` and
`ThingPositionFn = fn(&World, &AgentCtx, SimId) -> Option<SimPos>`. A small
`village/discovery.rs` can own both island-only functions, registered from the
existing composition root. Filter `SeenStatics` to the chosen immutable place
types and return `SeenState.pos`; leave moving people, wildlife and loose items
to ordinary perception. The existing native `known_position` adapter will use
the registered position provider after its known-target gate. This avoids
duplicating lookup rules in `player_hud` and avoids editing `village/agent.rs`.

### 3. Make inspection's work-transfer contract explicit

`player_actions.rs::release` calls `stop_work` before detaching native control;
Inspector/Editor mode reconciliation uses that path. Consequently, switching
modes cancels committed work, while `prepare_suspend` used for title/save preserves
it. This is a confirmed difference, not automatically a bug: generic controller
detachment intentionally cancels externally queued actions.

Choose one explicit policy. The smallest compatible polish is to disclose that
inspection stops current work and verify reservation recovery. Preserving work
would need a carefully defined authority-transfer contract for craft, gather,
build and rest, not simply removing one `stop` call. Test each activity halfway
through and after return to character mode; do not add a second controller.

### 4. Attribute new view work before optimizing the simulation

`testbed_view::perf_tags` names older view modules but lets player HUD, colony
scene/panel, island control and shared game UI fall into the broad `view` tag.
Add useful existing-profiler categories for those modules. Compare title, closed
HUD, open Build and Inspector with the same world and tick interval. Record
observation count/cost and save duration separately from render time. Existing
[Bevy diagnostics](https://bevy.org/examples/diagnostics/log-diagnostics/) support
that separation; no unmeasured speedup is claimed here.

## Terrain: existing coherence and a targeted native check

The source does not support the hypothesis that actors ignore terrain heights.
`colony_scene.rs::elevate_actors` adds the cell elevation after actor/corpse pose;
ordinary actor transforms are reset from interpolated simulation positions each
frame. `sync_terrain` updates separate terrain meshes on resource changes.

`terrain.rs::Terraform::check` rejects brushes near actors, workplaces, resources,
home entrances, fixed blockers and reserved cottage/garden footprints. Static
structures staying at their original height is therefore deliberate. Navigation
uses dry columns at or below 1,500 mm, with higher ground blocked; it does not
model edge slope or climbing effort.

Test an allowed raised corridor in Inspector/Editor, walk a character across its
boundary, save and reload, and inspect feet/ground alignment. Height currently
samples the authoritative cell while horizontal rendering interpolates; any
visible step or early vertical transition should be assessed from that capture.
Keep any visual smoothing out of authoritative `SimPos`. A slope-aware navigation
model is a separate mechanics feature, not required to fix an unproven defect.

## Persistence, architecture and authoring

The current lifecycle stages a headless world, validates structure/content,
backs up before explicit New, renews transient epochs on replacement, and restores
the prior live world on failed writing (`island/session.rs::new_game`). Local saves
hold an OS file lock; `snapshot::write_atomic` serializes to a sibling temporary
file, synchronizes it and renames it. Existing tests cover incompatible saves,
second ownership, write failure and shutdown veto recovery.

Keep this straightforward synchronous path until save duration demonstrates a
problem. A future worker would need ordered generations, one writer, stale-result
rejection, New/Continue coordination and an acknowledged final flush. It is not a
free performance improvement. The
[Rust synchronization contract](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all)
supports retaining explicit error propagation.

There is no historical local-island migrator. Before a real format change,
specify a versioned transform of a copy, fixtures from supported prior versions,
strict validation and preservation of the original until commit. Current
compatibility rejection is preferable to silently defaulting missing mechanics.
Extend failure-injection coverage at meaningful publication boundaries, following
the [crash-test principle](https://sqlite.org/atomiccommit.html#_testing_atomic_commit_behavior).

The supplied controller postmortem's useful lesson is already reflected in one
resident BT, one host tick driver, shared movement/actions and stable-ID queries.
Domain queries and action executors should remain separate. Keep cross-context
invalidation in the composition layer and avoid new consumers mutating production
or household state. The existing structure fits
[Bard Tree's ownership direction](https://bardtreelimited.com/architecture/overview).

`IslandAiPolicy` validates routine order, need thresholds and bounded GOAP costs;
`behavior.rs::configured_tree` still owns tree topology in Rust. The Worlds editor
edits that policy, not arbitrary behavior graphs. More capable tree/recipe/crop
authoring needs explicit schemas, validation and compatibility tests; do not label
the current policy editor a general graph editor or change persisted construction
silently.

## Startup, packaging and test evidence

The verified native rehearsal explicitly sets `WGPU_BACKEND=vulkan`. The ordinary
host uses Bevy defaults. No new default-backend failure was observed in this
review. Add an ordinary-backend title/New/Continue/quit smoke and record the actual
adapter/backend; retain explicit Vulkan as a diagnostic option rather than
hardcoding a machine-specific workaround. Backend selection is documented by
[wgpu](https://docs.rs/wgpu/30.0.1/wgpu/struct.Backends.html#method.from_env).

`assets.rs::asset_plugin` supports `BEVY_ASSET_ROOT`, otherwise it uses the
compile-time workspace asset path. That is appropriate for the current checkout
launch, but a distributable build needs a relocation/asset-manifest smoke from a
different directory. Preserve licensed assets and notices. Packaging is a future
delivery feature, not evidence of a current checkout-startup defect.

The final polish validation should include:

1. Observation purity and actor/epoch isolation after any projection or discovery
   change; existing legacy observations and content hashes remain unchanged.
2. Save/restore and equal recorded-command hashes across different host-frame
   partitions, pause transitions and renderer presence. Preserve the existing
   fixed-tick/interpolation boundary supported by
   [Bevy's example](https://bevy.org/examples/movement/physics-in-fixed-timestep/).
3. Mid-work mode transitions with the chosen cancellation/preservation policy,
   including reservations and material conservation.
4. Native Inspector/Editor terrain, the ordinary renderer path, and both target
   window sizes. Widget activation and rendered captures remain distinct from
   physical keyboard/mouse and assistive-technology testing.
5. Focused affected tests followed by the existing complete workspace gate and
   protected replay/snapshot checks. Do not replace or weaken an oracle to obtain
   a green result.

This research agent ran no Cargo, benchmark or game process. Only this report was
written; implementation choices and the final verified verdict belong to the
orchestrated improvement pass.
