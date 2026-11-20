# Storage specialist cross-review — 0.20

Reviewed peer gameplay and release changes from source/tests. No peer source was
edited and no Cargo command was run by this reviewer.

## Gameplay expired-offer boundary

The expiry guard precedes every acceptance mutation, after lookup/claimed-state
checks and before count/capacity, deadline renewal or progress insertion. Its
inclusive `elapsed >= expires_at` boundary matches fulfillment. Direct/wrapped
regressions retain full serialized state and finite money; the just-before
boundary succeeds with ordinary unchanged reward/deadline semantics. The second
case advances actual sub-minute clock state, proves the stale offer survives
until bounded maintenance, retains rejection across serialization/resume, then
uses the normal refresh and explicit acceptance. No new economic policy, escrow
funding or community beneficiary is invented. No source-level blocker found.
These are correctness/lifecycle checks, not long uncoached balance qualification.

## Release terminal publication

Separate exclusive completion publication, staged-file fsync and immediate exact
readback address a distinct final-receipt boundary. A failed link or mismatched
alias/completion causes a nonzero process result before success announcement;
existing receipt/symlink targets are preserved. The seven initial Python cases
cover ordinary pass/fail, nonterminal rejection, existing targets, link failure,
changed output and stale alias. Directory fsync, long-term retention, filesystem
hardlink support and Windows execution remain explicitly unqualified.

P2 finding sent to root: packaging currently accepts the passed legacy alias
without enforcing its new `completion_record` marker. A failed terminal-link
publication may therefore leave `gates.json` that the packager will accept despite
the runner returning failure. New marked reports should require a safe exact
`completed-gates.json` terminal sidecar matching the reviewed bytes. Old unmarked
0.19 records can retain their explicitly legacy support. Add a failed-completion
alias regression and namespace-collision tests for any newly bundled sidecar.
Coordinated receipt forgery remains outside local structural checks.

## Shared storage/UX boundary

The storage change touches only hydrate admission. The UX owner is adding a
reviewed-generation guard inside validated restore. The existing metadata/state
validator callback runs before generation/residency/status mutation and is the
correct point for that guard. Scope is reviewed generation, not authenticated
same-generation external content. Final implementation review follows once the
owner publishes its guarded callback/tests.

## Correction and shared-guard re-review

Release owner corrected the marked-alias bypass: `completion_record` must be the
literal safe `completed-gates.json`, loaded through the existing contained-file
admission, and its bytes must exactly match the supplied receipt. Captured
terminal bytes join the evidence namespace before collision checks and later
publication, preserving raw-name protection. Missing, mismatched and unsafe
terminal cases reject before output; positive publication retains identical
alias/terminal evidence. Unmarked historical support remains explicit. No
remaining source-level release blocker found. The owner reports 28 passing
Python tests; this reviewer did not rerun them.

Reviewed-generation restore is implemented in `regions.rs::restore_reviewed`,
inside the `RegionStore::restore_validated` callback. It compares the decoded
candidate's actual generation before metadata admission, store generation or
residency installation. Existing caller validation checks the current world,
principal, restore revision and paths; a stale cached status cannot substitute
for actual stored generation. The new regression intentionally makes cache and
review agree on an incorrect generation, yet rejects before live installation
and restore-revision increment. This composes with the hydration integrity fix;
no persistence/lib.rs edit was needed by the UX owner. No blocker found within
this shared boundary. Same-generation coherent hostile substitution is not
publisher/content authentication and remains outside generation-review claims.
