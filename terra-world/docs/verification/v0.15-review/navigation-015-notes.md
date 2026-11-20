# Navigation increment 0.15

## Implemented problem

The one-body stationary-obstacle planner rejects both two-corner choices when additional parked bodies occupy the two flanks. The reproduced reference car advances only 7.302238 m along a 60 m route after 120 physical seconds and repeatedly retains straight retreats. The accepted 0.14 autonomy source hash was checked byte-for-byte against the restored release ZIP. See `navigation-015-baseline.json` and `navigation-015-baseline.log`.

## Implementation

- Preserve the original one-body choices and behavior first.
- Only an ordinary body-sweep conflict can seed a cluster fallback; source/traffic failures cannot.
- Expand route-aligned outer bounds over at most four encountered stopped bodies. Moving, self, duplicate and unknown members reject.
- Complete first corner, second corner AND reconnection to the existing target pass source envelopes, geometric traffic clips and body sweeps before commitment.
- Corners remain within 64 m; the whole three-leg proposal stays within 160 m.
- Across every substep and retry of one vehicle update, additional cluster corridor admission attempts share a budget of 12.
- Only two waypoints are added. The existing 129-point limit and one-pending-maneuver patrol behavior remain. No save/wire/domain formats, authority positions, headings, ownership, money, inventories or delivery obligations are modified by planning.
- Physical motion still uses the ordinary handling integrator, full live source/contact admission and existing arrival thresholds.

## Files

- `crates/terra-sandbox/src/living/autonomy.rs`
- `crates/terra-sandbox/src/living/contacts.rs`
- `crates/terra-sandbox/tests/navigation_clusters.rs` (new)
- `docs/CONTACTS_AND_ROUTING.md`
- `docs/VEHICLE_DYNAMICS.md`

## Focused evidence

Five integration cases passed in `navigation-015-focused.log`:

1. Three parked bodies: physical arrival, complete source envelopes, contact-free steps, steering/step/path bounds, all goods/money and parked positions preserved, exact saved continuation.
2. Unsupported flanks: preserve the existing obligation through hold, then resume after support returns and compare exact save continuation.
3. Moving crossing vehicle: no static route rewrite around a moving body's instantaneous location; both bodies retain ordinary handling/contact priority.
4. Only the opposite flank is supported: use the supported side without crossing unknown terrain.
5. Patrol: preserve its circuit, cursor/direction while the temporary prefix is pending, with exact save/resume.

The contact-member unit case also passed (`navigation-015-contact-test.log`). Existing autonomous/traffic/recovery regressions are being checked in `navigation-015-regressions.log`; final outcome will be appended below.

Main measured result: 14.5 physical seconds / 145 steps; 75.4886578608 m traveled; 0.267630317 m from the unchanged goal; 18.6569461 m maximum lateral deviation; 0.800000002 m maximum 100 ms step; 3.3678093° maximum heading change; 3 maximum route points; 10,512 support queries total and 5,760 maximum per step; 143 exact resumed steps. All 10 verification flags true.

## Final gate integration

Run:

```sh
cargo test -p terra-sandbox --test navigation_clusters autonomous_vehicle_passes_a_cluster_without_moving_parked_bodies_or_losing_its_load --locked --offline -- --exact --nocapture
```

Parse the single `NAVIGATION_CLUSTER_REPORT ` JSON line. Keys: `case`, `obstacle_count`, `physical_step_seconds`, `physical_steps`, `elapsed_physical_seconds`, `elapsed_limit_seconds`, `distance_travelled_m`, `remaining_goal_distance_m`, `farthest_lateral_distance_m`, `max_step_distance_m`, `max_heading_delta_deg`, `max_path_waypoints`, `source_queries`, `max_source_queries_per_step`, `fixture_source_query_ceiling_per_step`, `exact_resumed_steps`, `scope`, `verification` (10 booleans).

All new tests will also run in the workspace headless gate. The final source-frozen run owns the accepted measurements; current logs are development evidence.

## Limits

This is an authored flat SDK fixture with reference NPC traffic bodies, no authored road graph, and an initial 12-wood inventory and 100-unit pocket balance per body. It is not a paid company-service, fueled registered-model, road-network, hardware, global planner or long-run throughput benchmark. Existing traffic/reservation/company tests cover their separate contracts. The baseline used unrestricted flat support; final measurement restricts support to east ±40 m and north/south bounds and counts source work. A rejected/over-budget geometry still holds safely; source coverage and dynamic occupancy must remain valid at each actual step.

## Final focused outcome

All four integration harnesses passed, with 36 passing tests and zero failed/ignored; the new contact-member unit case also passed. `navigation-015-regressions.log` SHA-256: `c9f5f987796dce1eb059ab6d4466031bc880deb3136664f9064788b62c95e058`. Original fixture returns and main cluster report are present in that log.
