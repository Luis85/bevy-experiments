# Network specialist cross-review — 0.20 restore UX

Source-reviewed the current flow/interaction/ReviewedRestore/region restore path.
No peer source edits or Cargo execution were performed. Native visual/input
qualification remains unavailable in the executor.

## Finding sent to UX owner / integrator

P1: the first candidate's bare `Action::ConfirmRestore` carries no identity of
the review whose button created it. A queued `Session → Restore → old
ConfirmRestore` can cancel one review, capture another target, then authorize
that new target with the old confirmation in the same bounded action loop.
The phase guard establishes only that some review is current. Require a unique
monotonic review token captured in the button action and matched at consumption,
invalidate on cancel/consume, and test delayed/queued old confirmations after a
new review. Queue-time backend snapshot validation does not solve mismatched
user consent when the wrong current review has been selected.

The inspected intermediate interaction signature also used immutable
`Option<Res<GameFlow>>` while its new Load branch requested mutable restore;
reported to the owner as a compile preflight correction (may still have been
under active editing).

## Sound boundaries retained in the candidate

- Cancel is the explicit initial RestoreReview focus, and Escape goes to Session.
- Review retains world/source context, principal, restore revision and configured
  paths; backend rechecks these before normalizing to a Load.
- JSON captures bounded exact bytes and compares the actual read used for
  installation; it cannot silently accept a later snapshot read after comparison.
- Regional expected generation is checked inside validated store restore before
  restoring state/cursor. Existing metadata/domain/source admission stays.
- Review is owner-local only; host authority and unavailable/pending storage reject.
- Plain SDK Load remains the trusted host path; native entrypoints request review.

These observations are source reasoning, not proof of native pixels, Tab/physical
controller traversal, disk fault recovery or all acceptance tests. Final owner
corrections and execution belong in the integration record.

## Correction re-review

The owner corrected confirmation to `ConfirmRestore(u64)`, with a checked
monotonic review serial allocated only when capturing a new review. The displayed
button retains that serial. Consumption rejects mismatched serial or wrong phase
before focus mutation, takes the review once and leaves the review phase after
confirmation. Cancel clears the current review; another capture increments the
serial, so delayed prior confirmation cannot select its new target. Overflow
refuses a new review rather than reusing an old identity. The real session test
now queues precisely Session → Restore → oldConfirm and asserts no restore and
preserved Cancel focus, then proves current confirmation succeeds and repeat
cannot restore again. This closes the reported consent-identity source blocker.
The mutable GameFlow system parameter is also corrected in interaction::actions.
No further material source blocker identified in this scoped cross-review.
Cargo execution still belongs to the integrator; this verdict does not invent
runtime, rendered or physical-device evidence.
