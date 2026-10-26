# Agent play-test friction log (fresh agent C, 2026-10-24)

**Setup:** testbed, unit 33 in Direct mode, real time. The agent read only the docs and reached the goal: key → store → pickaxe → equip → mine. That took about 4,400 ticks and 3.5 minutes.

| Stat | Value |
|---|---|
| Acts | 15 (13 accepted) |
| Latency p50 | about 165 ticks (8 s) per think step |
| Session replay | verified: 46 checkpoints ok |

Issues by priority. **P0** spoils the experiment, **P1** hurts agents, **P2** is polish.

## Observation correctness (P0)
1. **Affordances leak unperceived targets.** Items inside the closed, locked store were offered, and `use_tool_on@24` stayed offered after `Lost{forgotten}`. Affordances must be limited to perceived or remembered targets when a perception provider exists.
2. **Perceived target with no affordance.** The key was perceived at 12.3 m but had no affordance, while unseen items within about 10 m did. An undocumented affordance radius caused this. Offer every perceived target, with a `why_not` of "too far", or document the radius.
3. **Perceived items show only as `item #26`.** Show the item kind (`store_key #26`).
4. **Text observation has no positions for perceived things.** Print `(x m, z m)`.
5. **Own inventory items appear under "You remember".** Drop memory entries for items the unit holds.

## Observation usefulness (P1)
6. **Recent events are stale and noisy.**
   - Filter out `Roll perception.detect`.
   - Always include the controller's own action outcomes.
7. **No success odds.** Show the skill check and odds in the label. Warn on low odds; for example, with no Mining skill the unit rolls against a default of 5.
8. **The ore rock shows no yield or resources.** Without them, a successful mine cannot be verified from the observation.
9. **`Roll.target` is ambiguous** with entity `target` fields. Rename it to `target_number`.
10. **The level-up auto-spend and the unspent bank are unclear.** Explain both in the observation and docs.
11. **No hint for reduced night vision.** Add one.

## Staleness (P1)
12. **`stale_events` default 0 is too brittle.** Any Perceived/Lost event invalidates an observation. Count only relevant events (the unit's own actions, needs, orders, and percepts of targets it is acting on). Alternatively, raise the default and list the interrupting events in the rejection.
13. **Stale rejections cascade.** Provide an atomic multi-step act: order and execute together.

## CLI ergonomics (P1/P2)
14. **Two meanings of "queue"** (CLI `--queue` vs the order arg `queue`). Document both, and warn when `--queue` is used with `order`.
15. **No one-call walk-and-interact.** Add an `interact` convenience verb, or make a too-far affordance auto-walk.
16. **`agent events` returns on the first match only.** Add `--until-idle` / `min_events` / a linger window, and dedupe.
17. **Unknown event kinds are silently accepted.** Reject them with the list of valid kinds.
18. **`list-units` lacks owner and player.** Add both.
19. **`execute_orders` ends as `cancelled` after success.** Report `done`.
20. **The affordance list is about 30 lines.** Add `--brief`, which hides structurally impossible ones such as "door has no lock".

## Real-time behaviour (P2)
21. **Unit drifts after a failed interaction.** After `OrderFailed` the unit moved about 2.6 m away (`PathFound len 1`). Investigate.
22. **`stale_ticks: 200` (about 10 s) is tight for LLM turns.** Document it, or default higher for agent sessions.
23. **Docs ids hold for the default scenario too.** The docs give ids for `testbed:smithy`; say whether they also apply to `testbed`.
