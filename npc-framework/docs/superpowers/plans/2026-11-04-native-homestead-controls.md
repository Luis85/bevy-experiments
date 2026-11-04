# Native Home & Farm controls

Date: 2026-11-04. Deliver the existing small homestead loop through native game controls, beginning with the hero's normal inventory and preserving shared player/AI rules.

## Implementation

- Add a collapsible Home & farm panel to the native Island Village view, discoverable by a persistent button and F2. Keep the established dark/gold village UI, keyboard traversal, visible focus, scrollable content and world-input shielding. Support smaller windows without hiding required controls.
- Anchor the acting character to `VillageState.played`. Take/release human control explicitly through recorded commands; refuse a foreign controller. Pause host AI nudges during native ownership. Observe through the same actor-scoped providers as AI clients, including remembered geometry and protected foreign farm state.
- Route button actions through the existing shared action handlers. Navigation before work is an explicit recorded walk followed by fresh validation at arrival. Cancel pending work on interruption, control loss, death, actor replacement or world reset. Never grant materials or mutate simulation state from the UI.
- Provide plot selection, door travel, claim/build and individual bed selection with plant/water/harvest states and real costs. Show carried resources and construction/growth feedback. Include observed gathering sources, named workstation recipes, food/water/rest, and actual market opportunities so the complete zero-money loop is playable without CLI calls.
- Keep ordinary right-click movement usable while human control owns the hero. Explain pause, missing resources, unknown places, contested ownership and full storage using actual shared action results.

## Verification and closeout

- Add causal headless bridge/widget regressions covering recorded control, scope/privacy, delayed reach checks, cancellation and a working plot/crop journey. Preserve replay/golden/protected tooling.
- Build the native application and inspect representative native captures through authorized background debug/widget paths, without physical Windows input or provider usage. Batch inspection and at most one correction/confirmation pass.
- Run focused tests and warning-denied Clippy while iterating, then freeze source and run `cargo xtask verify --pretty` once at the end. Record exact results, limitations and owned-process cleanup. Update the play guide and changelog.

## Executed status

Implementation and the single final product correction are complete. The shared bridge preserves control ownership and actor-scoped knowledge; native widgets support the complete material-backed cottage/crop loop. Corrected full native-view tests pass **114/114**, with one separate ignored test; focused warning-denied Clippy and the native/menu release build pass. The host cleanup regression passes all **38** existing integration-target tests.

The actual native widget rehearsal gathers six real logs, crafts eight planks, builds, plants/waters bed 2, harvests three grain, rests and releases control without material grants or provider usage. Its declared initial location is staged. One complete initial matrix and the final six-image confirmation at exact 1440×900 and 1024×768 sizes are recorded with honest earlier fixture failures and owned-host cleanup.

The confirmed production source remains unchanged. The final 520-file candidate is `7f25d24cf13066a44d8139b983d43c9aadcd49762b8658c043eea90550f39742`, differing from the native capture candidate only in an explorer test's owned-worlds-directory isolation. Development-profile artifact recovery passed; all five existing fixture failures passed serialized without production, assertion or deadline changes. The complete final `cargo xtask verify --pretty` exited **0**, **`ok:true`** with four build jobs and two test workers: **1,749 tests passed** (12 skipped, 1 slow, 5 leaky), documentation tests 1 passed/10 ignored, architecture checks 7 passed, replays 8 verified and snapshots 6 unchanged. Formatting, warning-denied Clippy and dependency policy pass; every required gate step ran. Implementation, verification and the final polishing pass are complete. Exact artifacts, preserved failures and limits are in [the validation note](../../_notes/2026-11-04-native-homestead-validation.md). Earlier homestead/API evidence remains historical.
