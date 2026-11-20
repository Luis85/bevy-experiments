# Independent player-journey cross-review — 0.17

Review date: 2026-10-29. Reviewer: diagnostics workstream, independently reading
the player-journey changes in `crates/terra-demo/src/driving.rs`, `control.rs`
and their interface/interaction call sites. The authoritative transport action
and rental-refund implementations were also inspected. This reviewer made no
edits to those gameplay files.

## Outcome

No blocking correctness or authority issue was found in the scoped changes.

The rental-return review retains the player and restore context, selected
vehicle and model, originating and selected garage, escrow bond and complete
refund breakdown. An unrefreshed second click cannot bypass the review, changed
terms invalidate it, and dispatch consumes the retained review before another
queued click can reuse it. Time remaining alone is deliberately excluded from
the comparison because it does not change the returned refund. The dispatched
command carries the retained vehicle, garage and total refund; the existing
domain operation still checks current ownership, location, occupancy, work,
cargo, trailer and quoted refund before removing the rental or paying money.

The contextual transport action resolves the active vehicle's accepted job and
retains the principal, restore revision, vehicle, job, assignment phase and
deadline. A later offer browse cannot retarget that queued action. A changed
phase cannot reinterpret a stale pickup as a delivery. Current reach, movement,
ownership and job eligibility still reach the ordinary authoritative command.
Nonserviceable motion, distance and missing accepted work produce explicit
feedback. The direct Service label uses the configured interaction binding.

The shared actor-status formatter is presentation only. It prevents
`FocusSelection` from replacing a known vehicle's first/third-person Driving
status with the generic internal actor role. It does not change the selected
actor, possession request, ownership or camera handoff.

## Executed evidence

The diagnostics reviewer executed the root-scheduled integrated command:

```sh
cargo test --offline --locked -p terra-demo --bin terra-demo
```

All **179 tests passed**, with zero failures and zero ignored tests. The full
record is [`diagnostics-native-tests-017.log`](diagnostics-native-tests-017.log).
The run includes the five new rental/accepted-stop regressions described in
[`player-journeys.md`](player-journeys.md), the updated rental renewal/refund
journey and the restored-possession/focus test.

The helper fixtures explicitly place actors and vehicles to isolate handoffs.
Their results do not measure physical travel, human completion time, economic
balance or uncoached comprehension. The displayed-confirmation guard is verified
through its UI refresh/dispatch contract; actual rendered clipping, focus,
enlarged text and native input require the separately coordinated capture set.
