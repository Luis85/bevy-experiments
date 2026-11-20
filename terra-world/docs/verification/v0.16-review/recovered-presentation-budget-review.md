# Recovered-world presentation budget review

Reviewed on 2026-10-29. Status: the preserved native measurement is consistent
with the implemented admission policy; the corrected, scoped
[native acceptance record](../../native-v16-verification.json) passed.

## Decision and preserved evidence

The recovered world filled the Performance preset's 64 imported-instance slots.
Additional eligible or distant actors retained procedural presentation. This is
intended budget degradation, not unfinished loading or an asset failure.

The [final telemetry](../v0.16-native/v16-confirm-recovered-world-telemetry.json)
was preserved without rerunning the game or changing the measurement. Its
SHA-256 is
`1330f2a90aff2a1c83f211a9aef3b4e434d8b5bc60363ed4d820e421630905b1`.
The [completed process](../v0.16-native/v16-confirm-recovered-world-process.json)
exited with code 0 using accepted binary SHA-256
`6ba842ceaa3a9efe88603ae81db48053606185655827b3b5cf657dbe5c296434`.
All 366 implementation inputs remained unchanged, with matching starting and
finished canonical source-map digests.

| Final observation | Value |
|---|---:|
| Rendered terrain chunks / pending | 49 / 0 |
| Eligible actors / ready imported instances | 73 / 64 |
| Imported-instance limit | 64 |
| Deferred eligible actors / distant actors / hidden actors | 9 / 18 / 0 |
| Procedural fallback actors | 27 |
| Resident models / installed models | 5 / 7 |
| Loading instances, loading models, retiring models | 0 each |
| Failed models, rejected instances, unmetered instances | 0 each |

The counts reconcile exactly: `73 = 64 + 9` and `27 = 9 + 18 + 0`.
Selected and reserved usage are identical. Limits are valid, draining is false,
and there are no reported world, simulation, persistence, pack or capture errors.
Regional persistence is restored and unblocked; the capture is a graceful exit.
The 49 chunks describe the rendered view, not the count of saved regions in the
separate pre-open recovery comparison.

| Reservation | Selected and reserved | Configured limit |
|---|---:|---:|
| Instances | 64 | 64 |
| Vertices | 326,004 | 400,000 |
| Triangles | 108,668 | 200,000 |
| Primitives | 1,628 | 2,048 |
| Animation nodes | 2,302 | 4,096 |
| Animation channels | 583 | 1,024 |
| Texture pixels | 0 | 8,388,608 |
| Estimated bytes | 12,139,716 | 134,217,728 |

## Source and existing test review

[`ActorSceneAdmission::select`](../../../crates/terra-bevy/src/render_budget.rs)
applies visibility and distance eligibility, then bounded, stable selection.
Performance permits 64 imported instances, with a 650 m entry distance and 150 m
incumbent exit hysteresis. Deferred count is eligible count minus admitted count.
Models load on demand, so five resident models in a seven-model installed pack
is valid.

[`sync_actor_scenes`](../../../crates/terra-bevy/src/render_scenes.rs) retains or
restores the procedural mesh when no replacement is admitted.
[`update_diagnostics`](../../../crates/terra-bevy/src/render_pack.rs) counts bound
actors without an active imported scene as fallback instances. These semantics
agree with the documented
[aggregate admission policy](../../ASSET_PACKS.md#aggregate-scene-admission-and-procedural-degradation).

The final [render test log](../v0.16-final/bevy-render.log) records passing
regressions for stable capacity selection, hidden/distant and geometry-budget
fallback, outgoing-instance accounting, retained-subasset lifetime limits, and
actual scene degradation that restores the procedural mesh without changing the
body identity or pose. This review read those existing results; it added no tests
and ran neither Cargo nor the game.

## Acceptance-helper scope correction

The initial native helper incorrectly required `deferred_instances == 0` in its
generic settled-telemetry check. That condition belonged to the small driver
fixture and falsely rejected the larger recovered view. No native receipt had
been published when the mismatch was identified.

The corrected helper retains generic checks for valid limits, no draining,
zero loading/retiring/unmetered work, and no failures or errors. It explicitly
requires zero deferral for the driver fixture's 50 ready third-person instances
and 48 ready first-person instances with two intentionally hidden actors.
The recovered view has its own 64/73/9/18/27 observation checks, identical
selected/reserved accounting, and checks against all eight reservation limits.
The accepted native record retains these recovered counts and their scope.

Only the acceptance-helper scope was corrected; the frozen implementation,
configured budget and captured evidence were unchanged. This supports bounded
presentation degradation in the reviewed local run. Reservation estimates are
not measured GPU memory, and this result does not claim that every actor used
an imported model or certify hardware performance.
