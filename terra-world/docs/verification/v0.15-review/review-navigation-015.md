# Independent navigation review — increment 0.15

Reviewer: content_increment_015. Read-only review after implementation; no navigation source or tests modified by reviewer.

## Verdict

No P1/P2 implementation defect found in the scoped bounded-cluster change. The source and focused evidence support the stated authored-fixture improvement. This review does not establish arbitrary traffic throughput, company economics, licensed-map coverage or target hardware performance.

## Source inspected

- crates/terra-sandbox/src/living/autonomy.rs: complete planner diff against accepted 0.14 ZIP, substep loop, all detour call sites, admission, commit and patrol waypoint handling.
- crates/terra-sandbox/src/living/contacts.rs: complete planner diff, live contact snapshot/update/sweep and new cluster bounds/member checks.
- crates/terra-sandbox/src/living/simulation.rs: public timestep validation and ordinary handling invocation.
- crates/terra-sandbox/tests/navigation_clusters.rs: all five new integration cases.
- docs/CONTACTS_AND_ROUTING.md and docs/VEHICLE_DYNAMICS.md: cluster contract and scope claims.
- /tmp/terra-build/navigation-015-notes.md, baseline JSON/log, final regressions log and reported measurements.

## Findings checked

### Work bounds and source admission

`cluster_budget` is initialized once per `advance_vehicle`, before the physical substep loop, and the same mutable counter reaches lookahead, clearance retry and blocked-motion detour calls. It decrements before each cluster corridor attempt, including failures. The public caller enforces finite physical deltas in 0–5 seconds. Thus the claimed 12 additional cluster corridor attempts cover all substeps/retries of that vehicle update. This is deliberately not a bound on unchanged one-body probes or all source queries for the world.

Each cluster is at most four encountered stopped bodies; corner distance is at most 64 m and total proposed three-leg length at most 160 m. Invalid member identity, duplicate/self member, moving member or unsupported corner rejects. The fallback does not enumerate the whole population. Contact queries retain the existing spatial index and bounded sweeps.

`admit_detour` checks traffic clipping, the complete source envelope and all living-body sweeps for every candidate leg. Cluster proposals include the original target as a third point; success is required before only the first two points are committed. An unsupported source/traffic failure does not become a body cluster member. Actual curved movement continues through the unchanged live handling, source/contact admission and accounting path.

### Moving bodies and route state

New cluster members reject horizontal speed above 0.1 m/s, matching existing one-body policy. Velocity snapshots update after accepted sequential motion. Moving crossing bodies retain ordinary contact/stop/yield behavior; no position/heading is assigned by the planner.

Move inserts two navigation points at the current cursor within the existing 129-point bound. Patrol only admits one maneuver while its temporary path is empty, leaves circuit/cursor/direction untouched, and drains the temporary prefix before resuming the circuit. No serialization, private finance, inventory, ownership or delivery code was changed. Existing shared handling still returns admitted distance for accounting.

The extracted one-body admission helper is equivalent to the accepted 0.14 loop, including hypothetical heading/reverse calculation and zero velocity after each probe. Ordinary one-body behavior runs first and is not silently replaced with a more permissive cluster route.

## Evidence assessment

I independently read the accepted archive at `/workspace/scratch/0fdc9aa7bc4c/input-v014/terra-world-0.14.0.zip` and confirmed its autonomy source SHA-256:
`6f0363623a397d0e5b0b8778d97503489fb63f77dfdde164aaadf8adc101e276`.

I independently hashed the retained logs:
- baseline log: `c9537ca51adfcb3fe3cadf1c0a85f815fc3032e2623cd4f21413589d43d15ea6`
- final regressions log: `c9f5f987796dce1eb059ab6d4466031bc880deb3136664f9064788b62c95e058`

The baseline log reproduces failure to arrive within 120 physical seconds, with about 7.302238 m forward progress on the authored 60 m route. The final log records 36 passing integration cases across four harnesses, including five new cluster cases. The new contact-member unit case is separately reported as passing by the owner. Reviewer did not rerun Cargo while other workstreams owned it.

The main result is 14.5 physical seconds, 145 updates of 0.1 s, 75.4886578608 m travelled, 0.267630317 m remaining, 18.6569461 m maximum lateral deviation, 0.800000002 m maximum step, 3.3678093 degrees maximum heading change, 3 maximum path points, 10,512 source queries total, 5,760 maximum queries per update, and 143 exact resumed updates. The tests independently check admitted sweeps, bounded step/heading/path, unchanged parked bodies, goods/money conservation, valid full state and exact serialized continuation.

The documented caveat matters and is present: baseline used unrestricted flat SDK support; final measurement further restricts support to a rectangle. This makes the final case stricter in coverage, but it is still a synthetic reference NPC vehicle fixture without road graph, paid company operation or fueled registered model. Claims appropriately stop short of those broader conclusions.

## Nonblocking verification recommendations

1. Add a narrow negative case where both outer corners are supported but only the reconnect corridor is unsupported. The source correctly checks it; this case would protect the explicit complete-reconnect contract from later accidental removal.
2. Add a focused maximum-5-second update case or direct planner budget assertion to protect shared allowance across substeps/retries. Current measured arrival runs use 0.1-second updates, so their source-work ceiling does not itself prove the cross-substep budget contract.

These are coverage opportunities, not observed product failures or P1/P2 findings. They were sent to the owner and parent. Existing functional acceptance should remain the primary release evidence and final frozen gates should own accepted measurements.
