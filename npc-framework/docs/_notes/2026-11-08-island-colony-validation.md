# Island colony implementation, review and verification

Status: implemented; the complete workspace gate passed. Platform-specific limits
and the exact verification scope are recorded below.

## Scope and ownership review

The current checkout contains the new opt-in `island_colony` showcase. Work stayed
inside NPC Framework. No dependency upgrades, protected-oracle edits or global
configuration changes were used. The separate Prepare NPC framework playtest chat
supplied prior evidence and a late-winter navigation trace; it had no overlapping
source work to integrate.

The architecture review led to separate colony model, housing, social, arrival,
command and summary modules. Read-only construction intent is interpreted by the
resident action layer. Production owns plan invalidation through the composition
bridge. The native control coordinator owns desired/applied control transitions;
ledger projections, widgets, camera framing and rendering are separate. Content
integrity validation is independent of host file I/O. See
[ownership and extension contracts](../architecture-island.md).

An independent static reviewer reported four material defects, then reviewed the
corrections and marked all four resolved:

| Finding | Correction |
| --- | --- |
| Launcher treated a persistent island as disposable | Retained loopback endpoint; matching PID and saved acknowledgement required; failure preserves the host |
| Missing snapshot entities/components could be rewritten | Required construction identities/resources and loaded content references validated before replacing the file |
| Paused mode switches raced queued attach/detach | One receipt-based coordinator shared by ledger and F2, scoped to actor/world identity |
| Emergency construction cancellation depended on optional GOAP | One shared owned-work teardown, with policy-present and policy-absent regressions |

Additional reviews fixed inherited cottage membership, successor social/housing
initialization, duplicate material financing, routine-order validation, authored
rules reaching fresh local worlds, and a JSON-array editor round-trip defect.

## Simulation evidence

The exact Ivo/Jun corner positions from the prior playtest reproduce a clear
smoothed segment whose rounded 70 mm first step touches a blocked building cell.
Configured resident recovery now requests a bounded physical detour. Two tests
verify paid procurement completes and the recovery resumes from a snapshot.
No teleport, free goods, remote settlement or shared-navigation rewrite is used.

The 72,000-tick audit on seeds 7 and 42 kept 12 residents alive, conserved 2,400
coins and continued food production and trade on the final day. It sampled every
300 ticks and observed no critical Hunger or Thirst. Those seeds produced the
same observed trace; this is not evidence of diverse randomized initial worlds.

The explicit 336,000-tick seed-7 audit spans fourteen elapsed days, late autumn,
winter and spring. It samples every 120 ticks and applies no interventions.
Before/after logs are `target/colony-extended-before.log` and
`target/colony-extended-after.log`. Preventive clothing replacement uses real
durability, a conservative two-day cold-exposure horizon and finite budgets.

| Metric | Before preventive replacement | After |
| --- | ---: | ---: |
| Living residents at every sample | 12 | 12 |
| Conserved total coins | 2,400 | 2,400 |
| Critical Hunger / Thirst / Energy actor-samples | 0 / 0 / 0 | 0 / 0 / 0 |
| Critical Warmth actor-samples | 3,235 | 1,605 |
| Minimum equipped coats during winter | 2 | 6 |
| Cold actor-samples without a coat | 9,449 | 4,188 |
| Late-winter coats made, ticks 144,000–216,000 | 4 | 6 |
| Final three-day trades | 160 | 226 |
| Maximum sampled consecutive blocked movement | 0 ticks | 0 ticks |

Short stalls between samples are possible. Winter clothing shortages remain;
the test demonstrates improvement and bounded survival, not indefinite stability
or perfect provisioning. The late-winter hide, leather, spear and coat chains all
replenished. The final spring window continued food production and actual trade.

## Native verification and polish

The initial BRP-widget rehearsal completed policy changes, recorded terraforming,
arrival, release and saved terrain in
`target/playtest-island-colony-3b5c6bd5ff/summary.json`. Fourteen screenshots covered
1280×720 and 1024×768. Its independent visual review requested a correction batch:
usable-area camera framing, annotation reduction, visible scrolling, legible night
lighting, plain action feedback, typography/status placement and clearer coasts.

The correction adds a dedicated framing adapter, selected-only details by default,
explicit diagnostic overlays, real scrollbars/page controls, cool night fill,
supported production text, consistent panel typography and coastline/plot cues.

Windows input testing used an owned disposable world in
`target/colony-native-interaction/`. Actual clicks entered Character and Editor;
F2/F11 worked. Mouse input rejected a protected building, painted stone at (14,-9),
scrolled the editor, and raised that brush to 250 mm. Closing the window saved tick
16,061 with terrain revision 2 and the exact edited cell. No external provider ran.

That test also found missing initial hero selection for right-click walking and
lost very brief movement-key taps. Both received coordinator regressions. F9 was
intercepted by the machine's Lightshot shortcut; the accidental overlay was canceled
without saving/uploading it. Ctrl+S was added as the primary local-save shortcut,
with F9 retained as an alternate. Patched movement regressions and available native
confirmation are recorded below.

The final candidate's native movement bridge was confirmed at real host speed 1×,
without debug stepping: the enabled **Walk 6 m north** widget moved the hero from
`[500,-1500]` to `[500,2770]` by tick 82. Evidence:
`target/colony-movement-8f5dca13/summary.json`. An earlier harness attempt correctly
did not move while the world was stopped; the test was corrected to activate an
enabled control in a running world.

Final island visual confirmation is
`target/playtest-island-colony-ffb729ad4b/summary.json`: sixteen valid captures,
including lower ledger content and actual selected-resident detail. The independent
reviewer scored the original visual fix list as six resolved and terrain richness
partially resolved/nonblocking, with disposition **ship** at that scope. The exact
circular brush preview is additionally checked against authoritative edits in tests.

The final launcher/editor matrix passed at both logical window sizes using
`WGPU_BACKEND=vulkan`: `target/colony-menu-dc1326c20a/summary.json`. Two prior default
backend attempts exited on a renderer `DeviceLost` error. The same binary passed
with the process-only Vulkan override; no global graphics setting was changed.
The desktop control helper later failed to activate the window and returned the
desktop background, so the final patched OS-keyboard retest was not claimed.

## Checks

- Focused content checks passed: AI, architecture boundaries, real clothing
  procurement, corner recovery and fifteen colony contracts, including autonomous
  funded house completion and snapshot continuation.
- A focused Clippy pass with warnings denied passed before the last input polish.
- Native suites passed before the final input polish: actor host 75 + 3 tests,
  launcher 160 passed / 1 ignored, view 132 + 2 passed / 1 ignored.
- The patched view suite passed: 139 passed / 1 ignored, including short taps,
  right-click focus/selection, shortcut suppression and circular brush geometry.
- `cargo xtask verify --pretty`: **`"ok": true`**. Formatting and Clippy passed;
  **1,816 Nextest tests passed**, seven architecture checks passed, eight replays
  verified, six snapshots matched with none pending, and dependency advisories,
  bans, licenses and sources passed. No verification tool was skipped.
- Gate caveats: thirteen tests skipped, ten doctests ignored, one doctest passed;
  Nextest classified one test slow and four leaky. The ignored fourteen-day audit
  was run separately and passed. No protected baseline was replaced.
- Final gate JSON: `target/colony-verify.json`; progress:
  `target/colony-verify.stderr`. The GUI candidate records binary hashes and 561
  source/configuration hashes in `target/colony-final-candidate/candidate.json`.
  `target/colony-candidate-source-check.json` confirms unchanged Rust/Cargo inputs;
  the capture harness alone subsequently gained an explicit binary-path option.
- The final delivery build of `game_menu`, `actor_testbed` and `npcctl` passed.
  `target/colony-closeout.json` confirms unchanged Rust/Cargo source/configuration
  hashes after the gate and records delivered executable hashes. The actor was
  relinked; it is not represented as byte-identical to the earlier GUI candidate.
  The actual delivered actor then completed a Vulkan render smoke and clean exit;
  `target/colony-delivery-smoke.png` was inspected. Startup debug profiling reported
  nav/think budget warnings; this is not a controlled performance benchmark.

## Limits

This iteration has twelve established NPC professions, finite households, hearths,
companionship/affinity, cottage commissions and guarded terrain editing. It does
not add births, migration, family conflict, unrestricted object placement, terrain
undo or an arbitrary visual BT graph editor. Resident AI authoring is a validated
routine/threshold/GOAP policy form. Existing historical scenarios remain CLI and
replay fixtures. Saves use strict compatibility; no fictitious migration is claimed
for a historical local-island schema that does not exist.
