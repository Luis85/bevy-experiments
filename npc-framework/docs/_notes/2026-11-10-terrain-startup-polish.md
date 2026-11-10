# Terrain startup and empty-chunk validation — 2026-11-10

## Scope and cause

This follow-up fixes regional terrain appearing after the title scenery and the
48 `contains no vertices` warnings from water-only chunks. It preserves the prior
living-world expansion, terrain topology, simulation rules, immutable saves and
asset attribution.

The original fixed-viewport capture is retained in
[`terrain-startup-ce6a9602e7`](../../target/terrain-startup-ce6a9602e7/).
Its title stayed at tick zero without a save or camera change while terrain
appeared later. Instrumented runs then showed all 208 nonempty terrain meshes
uploaded while render pipelines were still pending. CPU assets alone were not a
sufficient readiness signal.

## Changes

- `SceneReadiness` acknowledges actual terrain GPU buffers, visible material
  pipelines, shared render pipelines, GPU preprocessing bindings and a completed
  render graph. The next extraction transfers that presentation-only result to
  the host. Readiness and real compilation errors latch for initial startup;
  headless and legacy hosts have no readiness resource.
- The title stays opaque and explains preparation until that acknowledgment.
  World-entry widgets and their action handlers stay gated; How to play and Quit
  remain usable. A shader failure produces friendly recovery copy and retains
  diagnostic details in the log. No startup timer advances the simulation or
  writes a save.
- All 256 terrain chunk identities remain present, but the 48 water-only chunks
  allocate no empty mesh. Recorded edits add/remove their owned mesh and bounds
  as land appears/disappears. Seasonal changes retain existing nonempty handles.

Independent final review checked installed Bevy 0.20 APIs, extraction/render
ordering, mesh ownership, frontend actions and native screenshots. It found no
actionable production correctness or UI issue.

## Verification

All Cargo commands used process-local `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=line-tables-only`,
`CARGO_PROFILE_TEST_DEBUG=line-tables-only`, and `NEXTEST_TEST_THREADS=2`.

| Check | Result |
| --- | --- |
| `cargo test --locked --workspace --lib -- --test-threads 2` | Exit 0; 614 passed, 2 existing clipboard tests ignored |
| `cargo build --locked --workspace --all-targets` | Exit 0, 3m 31s |
| `cargo fmt --all -- --check` | Exit 0 |
| `python scripts/playtest/capture_terrain_startup.py --lifecycle --confirm-layouts` | Exit 0, final native summary `ok: true` |
| `cargo run --locked -p xtask -- verify --pretty` | Exit 0; JSON `ok: true`, all seven stages passed |

The [full gate JSON](../../target/npc-out/terrain-startup-fix/verify-final.json),
[stderr](../../target/npc-out/terrain-startup-fix/verify-final.stderr.log) and
[JUnit](../../target/npc-out/terrain-startup-fix/verify-final.junit.xml) retain the
exact results: 1,960 nextest tests passed (6 slow, 5 leaky), 14 skipped; one doc
test passed and 10 were ignored; seven architecture tests passed; eight replays
verified; six snapshots had no pending changes; and all dependency-policy checks
passed. No tool was skipped and no filter, timeout or baseline was changed for
this run. Recorded gate steps total 3,162,354 ms (52m 42s); nextest execution was
2,056.813 seconds. No production source changed after the passing gate.

The 12 new regression tests cover empty-chunk asset/entity lifecycle, renderer
readiness/error handling and frontend opacity, entry gating, clock/hash/save
purity, usable exit and headless behavior. Build and library logs are under
[`terrain-startup-fix`](../../target/npc-out/terrain-startup-fix/).

## Native evidence

The final passing fixture is
[`terrain-startup-4118953992/summary.json`](../../target/terrain-startup-4118953992/summary.json),
with its complete [native log](../../target/terrain-startup-4118953992/native.log).
It used the normal renderer and a disposable save/settings directory, with BRP
enabled explicitly for observation and recorded operator commands.

Startup used the unchanged native viewport: 1280×720 logical, 2560×1440 physical
at the current Windows scale. The PNGs were opened and inspected:

- [Opaque loading title](../../target/terrain-startup-4118953992/shots/start-00.png)
  and [transition capture](../../target/terrain-startup-4118953992/shots/start-01.png):
  preparation copy and disabled world entry, with no incomplete terrain visible.
- [Revealed title](../../target/terrain-startup-4118953992/shots/start-02.png):
  coherent terrain and enabled New game after 208/208 meshes, zero pending
  pipelines and render frame 59. Another inspected run reached readiness at
  frame 86; frame count or elapsed time is not the gate.
- Post-readiness [1280×720](../../target/terrain-startup-4118953992/shots/ready-layout-1280x720.png)
  and [1024×768](../../target/terrain-startup-4118953992/shots/ready-layout-1024x768.png)
  captures confirm readable, unclipped title layout. These resizes occurred only
  after startup readiness and cannot explain the initial reveal.

All startup samples stayed at tick zero and created no save. Separate RPC reads
are bracketed by readiness reads so a transition between frames is not mistaken
for an atomic inconsistent snapshot. A ready renderer may safely precede the
following UI update's reveal.

The lifecycle fixture entered Oversee through native widgets. Two validated,
recorded radius-zero grass connectors at `(110,48)` and `(111,48)` reached the
nearest empty chunk. Its target cell `(112,48)` then became land, water and land
again. Revisions were exactly 1–5 at explicit ticks 14–18, with renderable mesh
counts `208, 208, 209, 208, 209`; all 256 `(name, entity)` identities remained
unchanged. Saved cells and heights matched every accepted command. The three
[land](../../target/terrain-startup-4118953992/shots/land-first.png),
[water](../../target/terrain-startup-4118953992/shots/water-again.png) and
[land-again](../../target/terrain-startup-4118953992/shots/land-again.png) captures
were inspected at 1024×768. They document the live paused UI; lifecycle geometry
is established by command receipts, saved cells and mesh/identity queries, not
by a close-up pixel comparison behind that menu. Quit game exited successfully.

The final log contains **zero empty-mesh warnings, errors or panics**. It retains
two separate existing warnings: an `agent_io` profiler budget warning during
instrumented reads and a fog-grid reset warning after New.

Three earlier diagnostic attempts remain recorded as failures, not passing
runs: the first assumed an empty chunk directly touched land; the second
sampled separate RPCs as if atomic; the third required `ViewVisibility` in the
identity query and therefore excluded intentionally meshless chunks. Only the
diagnostic script changed to resolve these fixture errors; production code did
not change between those captures and the passing confirmation.

## Handoff and limits

The native-tested `target/debug/game_menu.exe` was relaunched visibly with no
arguments or renderer override. At the checked handoff, process **15220** exposed
a responding **Island Village | NPC Framework** window and the remote interface
was off. Its SHA-256 was
`8ae12ecbb22ff5397a21a7f45737949968df438fd1c17842193253583fe58cf4`.
The [handoff receipt](../../target/npc-out/terrain-startup-fix/player-handoff.json)
and [startup log](../../target/npc-out/terrain-startup-fix/player-handoff.stderr.log)
record readiness at render frame 72, with 208/208 meshes, zero pending pipelines,
zero empty-mesh warnings and zero errors. One separate `view.scene` profiler
budget warning was logged during startup; this is not a performance-target pass.

At that title handoff, `living-world.world.json` remained absent and the existing
`local-island.world.json` retained its prelaunch SHA-256:
`C5265DEAF7E2E4F676CBC3D8354F4B6F4FB22520E94436A93A55FC4900DBAC3C`.
No real save was created or overwritten. The game was left open for the user.
The final changed-path audit found no core, protected oracle/policy, dependency,
save, generated-output or credential changes; `git diff --check` passed. No
commit, push, merge or pull request was created.

This is native screenshot/widget/recorded-command validation. Physical input,
screen readers, GPU/FPS targets and other adapters were not tested. The earlier
long-running handoff later logged a `DeviceLost` failure; that separate later
driver failure was not reproduced or diagnosed by this startup fix. The title
may initially be black while its own UI pipeline compiles, as retained in an
earlier capture; incomplete terrain stays hidden until preparation finishes.
