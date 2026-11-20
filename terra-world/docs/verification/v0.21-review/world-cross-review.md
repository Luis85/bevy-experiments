# Cross-review of world/simulation changes — 0.21

Reviewer: player-experience specialist, independent of the implementation owner.
Scope: candidate WP04–WP06 source diff, surrounding authority/geometry code and
new regression fixtures. No world-owner source was edited and no competing Cargo
run was started. Test execution belongs to the integrator's shared runs.

## Required correction: WP05 stops short of native loading admission

**P2, confirmed integration defect.** `living.rs::seed_population` now derives
the actual clipped/wrapped neighborhood with `TileKey::neighbors`; a radius-one
polar world with six valid tiles can initialize its population. However,
`terra-demo/src/flow.rs::poll_state` still requires
`min((2 * chunk_radius + 1)^2, 9)` resident and rendered chunks to leave Loading.
`flow.rs::update_status` uses the same rectangular count for its readiness
percentages. Consequently the newly valid six-tile world remains on Preparing
your world, with misleading partial readiness, after successful population and
transport initialization.

**Smallest correction:** use the same actual clipped/wrapped startup-neighborhood
contract in native loading admission and progress. Count relevant resident keys
rather than allowing unrelated loaded pages to satisfy source readiness. Keep
the existing startup cap and account/network/persistence gates. Coordinate the
native hook with the player-experience owner because `flow.rs` is shared with
WP01–WP03. A small shared helper is preferable if it prevents the domain and
native count from drifting again; do not turn this into a global loading-system
rewrite.

**Acceptance:** north/south radius-one six-tile cases leave native Loading only
when their actor/account, render and persistence conditions are also ready;
ordinary radius-one still requires its nine relevant pages; radius zero works;
unrelated residents do not improve source readiness. Progress percentages use
the same denominator and reach 100% for the complete six-tile case. The new
`population_bootstrap.rs` regressions alone establish domain startup, not this
native transition.

This finding was sent to the integrator and world owner before the review file
was written. The integrator then authorized a narrow correction: the world owner
added public `living::startup_neighborhood` and current-source
`RenderTelemetry::integrated_chunks`; this reviewer updated native admission,
progress and the screenshot smoke-readiness membership check. Native loading
uses the active player's position, falling back to runtime focus, the existing
nine-page cap, relevant resident/current-integrated keys, and a nonempty-key
guard. Screenshot readiness retains its existing full displayed-neighborhood
policy. The account/network/persistence blockers remain intact.

Actual native `poll_state` and `update_status` regressions now cover both polar
six-tile worlds, ordinary nine-tile and radius-zero worlds, unrelated resident and
render totals, missing current integrated geometry, player/focus divergence,
invalid neighborhoods, account/connection/actor conditions and consistent 100%
progress copy. These source corrections are complete; this finding remains
pending executed test evidence and final integration acceptance.

## WP04 — heterogeneous merchant requests

No additional blocking defect found in the reviewed change. The checked issuer
price lookup occurs before reward calculation, ID allocation, treasury deduction
and offer-count increment. Unsupported retail items therefore cannot panic or
consume capacity/escrow. Production-input requests retain their earlier checked
issuer valuation and priority. The first market remains the central issuer;
the patch does not introduce a new funding or merchant-ownership policy.

The added regression deliberately mixes supported and unsupported shortages,
checks four admitted supported offers, IDs, issuer identity, untouched goods and
retailer funds, conserved total money, and unchanged requests after JSON resume.
Its claims are appropriately narrower than career balance or infinite funding.

## WP06 — per-source feature-edit mesh admission

No additional blocking defect found in the reviewed production algorithm.

- Each resident key retains its exact immutable source `Arc` and the set of
  removed IDs actually present in that source. Shared IDs in adjacent chunks are
  independently included, so boundary fragments respond together.
- A completion still requires current source identity, global quality revision
  and target feature LOD, and now also requires the exact removed-ID set. An
  unrelated edit leaves useful in-flight work admissible; a changed or restored
  overlay cannot install mismatched geometry.
- Returning to an in-flight job's exact edit context can safely consume its
  queued dirty request because the admitted job then describes the desired
  geometry. The new pending removal happens only after all admission checks.
- Retry state includes the same source/quality/LOD/edit context. Unrelated edits
  preserve its backoff; a relevant context change invalidates the old failure.
- Edit targets and detail targets are pruned against resident keys, and failure
  entries are also pruned against current admission context. Existing jobs retain
  source ownership until completion; the two tracked-worker limit is unchanged.
- The effective-set computation matches `geometry.rs::feature_meshes_lod`, which
  only tests removed IDs belonging to features in the supplied chunk. Passing
  the smaller exact set therefore preserves existing feature omission semantics.

The new fixtures cover one affected page among 49, shared fragments, depletion
and regrowth, unrelated completions, changed in-flight edits, restore context,
return to the same context, quality-wide work and failure backoff. Existing source
replacement/identity, LOD and residency admission cases remain present.

**Limits:** a changed global removed set still scans resident feature lists to
derive exact per-source sets. Per-frame maps and retained source references are
bounded by the resident workload; this is fewer mesh jobs, not proof of lower
frame tails, physical memory use or GPU performance. Arbitrary hung-thread
preemption remains outside scope. GPU-free fixture completions establish their
stated admission/work-count behavior, not native appearance or hardware quality.

## Review conclusion

WP04 and WP06 have no additional blocking source issue in this cross-review.
WP05's required native admission/progress integration is now implemented with
shared geographic and render-identity contracts. All changes remain candidates
pending shared tests and final acceptance; this review does not independently
claim a passing runtime or visual result.
