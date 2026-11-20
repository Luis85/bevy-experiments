# 0.20 player-experience review and implementation plan

Current six-goal guidance, commitment pricing, menu scrolling, neutral controller rearm and managed backup flow remain useful foundations. Windows, actual pixels/Tab traversal, physical devices and uncoached progression remain acceptance gaps; this environment's Unix-socket restriction prevents fresh native review.

Selected gap: both in-world F9/Load and paused-menu Restore currently dispatch destructive Load immediately. Add one shared review page with Cancel as initial focus, exact snapshot path/generation description and discard consequence. Confirm must bind current source-world identity, principal, restore revision, configured paths and the reviewed checkpoint. JSON admission retains and compares bounded exact snapshot bytes; regional generation is checked inside validated store restore before its cursor/state changes. Both requests keep the simulation paused, preserve capture suppression and prevent queued stale confirmations. Plain SDK Load retains its existing host behavior.

Acceptance: direct/world and paused requests enqueue no Load before confirmation; cancel retains current state; explicit confirmation restores the reviewed snapshot only; context/snapshot/generation changes reject without state mutation; repeated confirmation cannot replay; controls capture cannot request restore. These are source and headless behavioral checks, not native visual or hardware qualification.

## Implementation and review corrections

Added private, non-serialized ReviewedRestore admission and LoadReviewed intent;
regional expected-generation validation executes inside restore_validated before
cursor installation, JSON checks the bounded bytes actually parsed. Native
world/paused routes share one review page and Cancel focus. NET peer caught
missing per-review confirmation identity; fixed with checked monotonic serial,
button-bound confirmation and queued cancel/new-review/old-confirm regression.
NET peer also caught the in-world system's immutable resource parameter; corrected
to ResMut only on the mutation-owning system. Diagnostics recognizes reviewed
loads. No Cargo test execution is claimed by this owner.

Central commands: `cargo test --locked --offline -p terra-bevy --no-default-features reviewed_`;
`cargo test --locked --offline -p terra-demo ordinary_session_keeps_save_restore_retry_and_quit_after_rejecting_terminal_close`;
then full demo suite (captures/terminal/input behavior) and Bevy default suite.
Three new engine tests cover six JSON/context rejection cases with complete-state
checks, JSON/regional success plus queue retirement, and a cached-status-matching
but actual-generation-mismatching store rejection. Existing real session journey
now covers no mutation before consent, Cancel, shared in-world request helper,
stale queued confirmation, explicit confirmation and repeated-confirm suppression.
Native pixels/real Tab traversal remain unqualified.

## Integration fixture correction

Initial focused execution found two new assertions assumed regional fixture
restore_revision starts at zero. The fixture imports an existing regional store
and runs initialize_regions during app.update, which legitimately restores it and
increments the serial once before the test begins. Existing queue-barrier tests
already capture this baseline. New reviewed-success and reviewed-rejection tests
now do the same: successful consent must advance exactly once from the observed
installed revision, and rejection must preserve it exactly. Full-state and queue
checks remain; production code was not changed. Tests remain integrator-owned.
