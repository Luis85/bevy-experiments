# NPC life and shared environment increment

Date: 2026-11-01. Scope: Island Village, preserving the independent workspaces and existing replay oracles.

The next playtest should make residents understandable inhabitants of the same physical world as the player. A client chooses actions; an NPC chooses them through a deterministic behavior tree. Neither gets a separate survival or production implementation.

## Implementation sequence

1. **Shared actions.** Audit every autonomous effect against the client action surface. Add voluntary home and ground rest and recovery of dropped diaries. Route NPC consumption, trade, gathering, rest and recovery through the same application-time validation. Reuse generic movement and crafting. Keep private host notebooks and operator lifecycle separate from physical gameplay.
2. **NPC life tree.** Register an island-only named tree using the existing runtime. Prioritize actionable water and food needs, recovery/rest, return home at night, profession production and idle. Preserve sorted Effects application, physical resources, finite currency, reservations, mortality and snapshots. External control must suspend autonomous decisions and release must restore them. Empty shops must not deadlock their own producers.
3. **Environment discovery.** Expose known resource kinds, gather durations and current sensed/reached stock and renewal times. Expose recipes with ingredient/output quantities at known workstations. Never reveal current stock through stale perception. Give clients their own home and role to use the same actions as residents.
4. **Readable observers.** Publish a closed, public current decision and resource timings in the read-only world projection. Show these in existing native captions and browser inspection without changing the view's authority or inventing private reasoning. Preserve the existing visual identity and responsive layout.
5. **End-to-end verification and polish.** Add parity, priority, external-control, environment privacy and snapshot/determinism regressions. Run focused checks, then one coordinated full `cargo xtask verify --pretty` after both active chats freeze. Build and inspect native/browser captures through background debug APIs only. Record exact evidence and limitations; do not use paid provider sessions or inspect provider history.

## Work ownership

- `npc_life_tree`: NPC tree, decision application and life-tree tests.
- `action_parity`: shared validated commands and parity tests.
- `environment_contract`: independent environment information tests and review.
- Primary: action/observation registration, environment projection, native/browser integration, documentation, final gate and polish.
- The separate AI-client chat owns client history/metrics UI and explorer settings; coordinate one combined final gate and at most two Cargo builder chains.

## Acceptance

- Residents have an actual registered `Brain` and shared `TreeDef`; decisions are inspectable.
- A player/AI client can initiate every resident physical task through public actions and the existing recorded-command seam.
- Needs and resources have the same costs, reach, capacity and cancellation rules regardless of controller.
- Residents sustain their physical supply chains; depleted resources renew deterministically and seasonally.
- External control, death, save/resume and observation cannot create duplicate autonomous work or change deterministic outcomes.
- Existing protected replays/goldens remain untouched. Island content hashes intentionally change with the new registered tree/actions.
- The final report distinguishes simulation tests, visual captures and any unverified live-provider behavior.

## Review-driven corrections

- A real enabled/disabled named-recipe client test exposed missing headless alias handlers and inconsistent world/actor capability checks. Register the six island aliases through the existing public handler and normalize their crafting capability in content policy gates.
- The existing accelerated-year survival assertion caught a genuine bakery supply gap: the miner retained money and water but died waiting for bread. Diagnose physical procurement/batch timing, improve finite paid ingredient logistics, and keep the original survival/currency/production assertions unchanged.
- The first full gate exposed stale actor-offer caching in the registered rehearsal. Current offers now replace old ones; static public workplace products/demand support perception-only search and reacquisition. Keep the original search/time/outcome limits and verify all three real-host rehearsals.

## Executed closeout

All five implementation/verification increments are complete. The final gate
`cargo xtask verify --pretty` returned **ok:true** in
`target/npc-life-verify-final.json`: **1,585 tests passed**, 12 skipped,
one slow/five leaky classifications, doc tests 1 passed/10 ignored; architecture
7 passed, protected replays 8 verified, goldens 6 unchanged and deny all passed.
No optional verification tool was skipped.

All three registered HTTP/MCP rehearsals pass with the original assertions:
Herp's first sale at tick 3564 and full income/needs/save-resume at tick 4430.
The environment directory/privacy suite passes 9/9. All five maintained release
apps built successfully. Native day/night and desktop/narrow browser inspection
passed; no provider session or physical input was used.

See [full evidence and limits](../../_notes/2026-11-01-npc-life-closeout.md)
and [the shared action API guide](../../npc-life-and-environment.md).
