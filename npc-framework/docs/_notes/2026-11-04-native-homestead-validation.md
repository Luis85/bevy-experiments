# Native Home & Farm — validation evidence

Date: 2026-11-04. **Implementation and the final polishing pass are complete.** The native widget journey, corrected visual confirmation, release build and full frozen-source workspace gate are green. `cargo xtask verify --pretty` exited **0**, **`ok:true`**, with **1,749 tests passed**.

## Scope

The native Island Village view now offers Home & farm (F2), explicit human control, discovered plot/bed selection, claim/build/plant/water/harvest, remembered-worksite approach, resource gathering, named recipes, actual inventory/food/rest/market controls and human release/stop. It uses the shared action resolver and recorded commands, anchored to `VillageState.played`. A foreign controller is never silently replaced. Native control pauses host AI nudges and restores only its own unchanged policy.

There are no new provider calls, credential reads, OS mouse/keyboard actions, global settings changes, dependencies or protected oracle edits in this increment. Read-only panel observation uses the same actor-scoped data as AI clients. The browser remains an observer.

Design and boundaries: [plan](../superpowers/plans/2026-11-04-native-homestead-controls.md), [ADR 0034](../adr/0034-native-homestead-player-input.md), [native play guide](../living-homestead.md).

## Focused checks

- The initial launcher regression failed at runtime on the original missing UI: [native-farm-ui-before.log](../../target/native-farm-ui-before.log).
- The initial seven-widget run passed the actual claim/build/side-bed plant/water/growth/harvest journey. Its paused-world fixture lacked the host's `SimControl`; that explicit fixture resource was added. [native-farm-ui-first.log](../../target/native-farm-ui-first.log) preserves the original result.
- The first complete view suite passed 110 tests with one keyboard-fixture failure and one ignored test. That headless fixture lacked UI visibility propagation, so it now uses the same visible-node stand-in as existing notes-panel keyboard tests. No production visibility or keyboard guard was weakened. [native-farm-view-tests.log](../../target/native-farm-view-tests.log).
- Final `cargo test --locked -p testbed_view --lib -- --nocapture`: **111 passed, 0 failed, 1 ignored**, including six native-action bridge tests and eight widget/model tests. [native-farm-view-final.log](../../target/native-farm-view-final.log).
- Actual widget activation covers read-only opening/browsing, stable control identities, disabled activation/foreign-controller refusal, recorded control, paused-world resume, F2/Tab/Escape and foreign-field shielding. The staged widget journey declares initial position/materials and proves the selected side bed is used, exact cottage inputs consumed, water spent and three grain harvested once. It is not unaided material-acquisition evidence.
- The bridge separately proves a zero-cash hero walks and gathers real wood through recorded shared actions, observes the resulting receipt, respects remote registration, cancels delayed work after replacement/stop/control loss, rechecks missing materials at arrival and restores scheduler policy without overwriting a later writer.
- Host activation cleanup had an actual runtime RED: paused assigned-hero `HomesteadAct` input survived the existing control boundary. [native-farm-host-before.log](../../target/native-farm-host-before.log). The single whitelist-arm fix passes **all 38 explorer_host tests**: [native-farm-host-after.log](../../target/native-farm-host-after.log). Its three saved claim/build/plant cases preserve real already-growing crops, other actors' pending inputs and generic snapshot/hash restoration. Existing crop growth is not cancelled by host activation.

## Final feedback and camera regressions

The bounded bridge review found an actual apply-time feedback gap. Two same-tick accepted plant inputs spend only one real seed and correctly refuse the second occupied-bed input, but the panel retained an acceptance label. [native-farm-feedback-before.log](../../target/native-farm-feedback-before.log) preserves that runtime RED. Exact latest-command, actor/epoch/event scoping now passes **8/8 bridge tests**, including newer-input and foreign-event isolation: [native-farm-feedback-after.log](../../target/native-farm-feedback-after.log). No generic API or simulation rule changed. A subsequent focused Clippy was interrupted to respect shared-machine builder limits; its interrupted artifact does not establish a verdict.

The new camera geometry regression also failed at runtime: the old right-HUD-only framing put the hero at **(321, 368)** behind the farm panel at **1024×768**. [native-farm-camera-before.log](../../target/native-farm-camera-before.log). The combined correction accounts for both panels, excludes controls from world label placement, hides the redundant right HUD below 1,200 logical pixels, and preserves old spectator framing while the farm is closed. New farm UI separators use ASCII supported by the incumbent font; request/progress captions use the clicked human-readable action. Final native view/Clippy/release checks are running on this correction.

## Real native widget rehearsal — initial full loop green

[playtest-native-homestead-1e13adff45/summary.json](../../target/playtest-native-homestead-1e13adff45/summary.json) is **`ok:true`**, **346.278 seconds**, **11,866 simulation ticks**, owned host stopped, no provider invocation and **22 PNGs with exact 1440×900 / 1024×768 dimensions**. It records the inspected pre-correction release executable's SHA256; it is initial evidence, not the final corrected-source visual verdict.

- Initial position **(-5 m, -16.5 m)** is explicitly staged beside a house site. Plot discovery is not claimed unaided. The hero starts with **0 coins and an empty pack**; no building materials, seeds, water or coins are supplied.
- Actual native widgets claim the discovered plot, show the missing **8 planks + 4 logs** with Build disabled, gather **6 real logs**, and run the observed `village_saw` recipe twice to make **8 planks**, retaining **4 logs**.
- The same controls discover/gather seed and clean water, build the actual cottage, select **bed 2**, plant, water, wait for real growth, harvest **3 grain**, rest at the new home and release native control. Actual material consumption, bed selection and crop transitions are asserted.
- The completed actor is alive at its cottage door with **100% energy**, **3 grain**, and 0 coins. Total world currency stays **2,400** across wallets, dropped purses and community fund.
- Root inspected **10 representative PNGs** spanning missing-inputs, real crafted supplies, built cottage, ripe bed and released controls at both exact sizes. They confirm the old camera overlap and unsupported separators addressed in the single product correction above. Final confirmation will use only six representative images while rerunning the complete physical widget journey.

The harness uses owned BRP widget `Activate` events, view-only scroll/window changes and explicit debug-clock advancement at 0×. It never controls the OS mouse/keyboard. It checks inherited display, enabled state, scroll clipping, actual reflected Window/Camera dimensions, completed PNG structure and exact IHDR dimensions before accepting a capture.

Earlier harness failures remain honest and unchanged:

- [playtest-native-homestead-b1dff99e9d/summary.json](../../target/playtest-native-homestead-b1dff99e9d/summary.json): stopped at tick 2 before perception had revealed its staged plot. A bounded ordinary 64-tick warm-up fixes only the fixture timing.
- [playtest-native-homestead-2df46b3ec4/summary.json](../../target/playtest-native-homestead-2df46b3ec4/summary.json): acquired the real 6 logs/8 planks, then its unconditional exploration-arrival expectation hit a physically blocked destination. The repaired bounded route recognizes a fresh own navigation refusal and tries its next relative direction, preserving every acquisition/crop assertion. Its six historical image filenames also misstate their pixel sizes because sequential asynchronous window-field writes raced; they remain evidence only at their actual IHDR sizes. Atomic window updates and strict geometry checks produced the successful exact-size run above.

## Final checks and preserved recovery history

Final `cargo clippy --locked -p testbed_view -p testbed_content --all-targets -- -D warnings` is clean: [native-farm-final-clippy.log](../../target/native-farm-final-clippy.log). The corrected complete native view suite passes **114 tests, 0 failed, 1 ignored**: [native-farm-final-view.log](../../target/native-farm-final-view.log), including the camera geometry regression, all eight bridge tests and all eight farming-widget/model tests.

The confirmed native candidate was frozen across **520 recorded files**, fingerprint `f94c3710e57c0ce21215bdaf5dc9795367f7e0777e5b100d77c0a255e7240737`: [native-farm-before-fixture-candidate.json](../../target/native-farm-before-fixture-candidate.json). The final verification candidate below differs only in test-fixture isolation.

`cargo build --locked --release -p actor_testbed -p game_menu --bins` passes in **5m14s**: [native-farm-final-release.log](../../target/native-farm-final-release.log). The confirmed `actor_testbed.exe` SHA256 is `9c24a3a06185213e2d6d39214307b6f160b0fcec4bf27707e233d8e9c909dca2`.

The single selective confirmation is **green**: [playtest-native-homestead-bd8b024187/summary.json](../../target/playtest-native-homestead-bd8b024187/summary.json), **142.743 seconds**, the full no-material-grant physical widget loop through **11,866 ticks**, unchanged **2,400** total coins, correct release SHA256, no provider invocation, and owned host stopped. Root inspected **all six corrected PNGs** (cottage built, bed 2 ripe and controls released at both exact sizes). The hero remains outside the farm panel, redundant narrow-window HUD is hidden, farm separators render, and request captions are readable. Lower form actions use the scrollable body in smaller windows; this is debug-widget/visual QA, not a claim of OS pointer or screen-reader testing. No further product polishing round followed.

After the human explicitly chose to wait for a slot under the shared two-builder limit, a slot became available. The first complete `cargo xtask verify --pretty` exited **1**, **`ok:false`** because Nextest could not compile its test executables: missing dependency artifacts (`E0463`, including `npc_framework`) cascaded into Rust 1.99.1 internal compiler errors in unchanged integration tests. Its original result is preserved in [native-farm-verify-before-cache-repair.json](../../target/native-farm-verify-before-cache-repair.json) and [transcript](../../target/native-farm-verify-before-cache-repair.stderr). Formatting, workspace warning-denied Clippy, 7 architecture checks, 8 replays, dependency policy and documentation tests passed in that run; it does **not** establish workspace-test or overall success.

Root verified the resolved non-junction `target/debug` directory and absence of other NPC builders, then used Cargo's package-scoped **development-profile-only** cleanup for affected local/dependency artifacts. The release, saves, captures and source were preserved: [native-farm-cache-repair.json](../../target/native-farm-cache-repair.json), [cleanup transcript](../../target/native-farm-cache-clean.log). `cargo test --locked --workspace --no-run -j 4` rebuilt the test artifacts as Root session **54490**: [native-farm-cache-rebuild.log](../../target/native-farm-cache-rebuild.log). There was no compiler/dependency upgrade, source/test edit, oracle replacement or policy bypass for this repair.

Artifact recovery is **green**: `cargo test --locked --workspace --no-run -j 4` exited **0**, all workspace test executables rebuilt in **51m54s**, without a source change or recurring dependency/compiler diagnostic.

The next complete gate executed all **1,749** tests: **1,744 passed, 5 failed**, 12 skipped, with 12 slow/4 leaky classifications. [native-farm-verify-before-fixture-retry.json](../../target/native-farm-verify-before-fixture-retry.json) preserves that `ok:false` result. All native farming tests and the six other required gate steps passed. Four failures were existing subprocess-sensitive AI-client fixtures; one was an explorer argument-validation fixture reading a shared worlds directory missing `plain_explorer`.

The explorer test now supplies an owned worlds directory for its invalid-actor branch, preserving every original argument branch, exit code, expected diagnostic and no-created-agent assertion. It changes only [explorer_serve.rs](../../apps/npcctl/tests/explorer_serve.rs), not game behavior, presets or policy. The four client cases were reviewed without changes to their production logic, assertions or deadlines. All **five cases pass serialized**, **108.058 seconds**: [native-farm-five-serialized.log](../../target/native-farm-five-serialized.log). Lower concurrency addresses the demonstrated fixture contention without skipping checks.

The final candidate is still **520 files**, now fingerprint `7f25d24cf13066a44d8139b983d43c9aadcd49762b8658c043eea90550f39742`. Its sole difference from the confirmed native candidate is that test-fixture file. The old native candidate remains in [native-farm-before-fixture-candidate.json](../../target/native-farm-before-fixture-candidate.json). All production files and the confirmed release remain unchanged.

The complete final `cargo xtask verify --pretty` exited **0**, **`ok:true`**: [native-farm-verify.json](../../target/native-farm-verify.json), [transcript](../../target/native-farm-verify.stderr). Process-only build concurrency was four and test concurrency **two**, restored by the wrapper afterward. No assertion, deadline, protected oracle or test selection was weakened.

| Required gate | Exact final result |
|---|---|
| Formatting | All files formatted |
| Workspace Clippy | No warnings |
| Nextest | **1,749 passed**, **12 skipped**, **1 slow**, **5 leaky** |
| Documentation tests | **1 passed**, **0 failed**, **10 ignored** |
| Architecture fitness | **7 passed**, **0 failed**, **0 ignored** |
| Replays | **8 verified**: session 1, testbed 3, village 4 |
| Goldens | **6 snapshots**, none pending, `INSTA_UPDATE=no` |
| Dependency policy | Advisories, bans, licenses and sources all pass |

Every required step ran; no tool skip was reported. Nextest's slow/leaky classifications and skipped/ignored tests remain recorded rather than being relabeled as complete coverage. All 520 final candidate files and the captured release hash are confirmed in [native-farm-candidate-confirmation.json](../../target/native-farm-candidate-confirmation.json). Only documentation closeout followed the passing gate. Owned native fixtures were stopped; this increment used no real provider or physical input.
