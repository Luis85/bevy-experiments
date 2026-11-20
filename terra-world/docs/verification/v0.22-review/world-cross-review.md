# Player specialist cross-review — world/community work

Scope: source review of the new `DissolveFaction` native review and supporting
community UI, before final native acceptance. No runtime or pixel result is
inferred from this review. Initial review found one material consent issue and
one layout/copy concern. Owner corrections and their evidence are recorded
below as they complete.

## Material finding: confirmation must bind a review instance

The initial change reused `CommunityAction::Confirm`. That action took the
currently stored `CommunityReview`, and then checked its current faction,
player, restore revision and assets. Those are useful state checks, but they do
not identify the review which the player actually accepted. An activation queued
for review A can arrive after Cancel and preparation of review B. The unbound
Confirm then checks B's valid context and executes B.

The correction dispatched to the world owner is a monotonically increasing
review ID and a `ConfirmReviewed(id)` native action. The UI retains the last
actually displayed token during the existing label refresh, clears it when a
new review is prepared, and captures that token at native activation. A stale
ID rejects without consuming the newer review. A prepared review cannot be
accepted before its displayed context is available. The existing authoritative
domain command still names the exact faction and rechecks full eligibility.

This applies to the shared community review mechanism, including dissolution,
leave, remove, leadership transfer and property sharing. It does not create a
second domain state or a new save/wire field.

Required regression: prepare/display A, enqueue its confirmation, cancel A,
prepare B, process old confirmation; B remains unexecuted and available for
review. Cover same and different faction/action, unshown B, duplicate
confirmation, changed owner/restore/roster/assets, and a positive displayed
confirmation.

## Layout and consequence copy

The initial dissolution explanation was more than 500 characters and followed
the ordinary faction summary inside a 300px inspector. At 150% text its full
context and both buttons require substantial vertical scrolling. Scrolling is
supported, but the review should lead the page after the existing scroll reset.

Recommended correction: put review text and actions before the ordinary faction
summary; put Cancel first; shorten copy while preserving the exact faction name
and ID, sole-member condition, invitation revocation, no refund or transfer,
authority's unseen-property check, and new-ID consequence. Reuse the existing
scroll/focus behavior and word-or-character wrapping. Native 1100×700/150%
inspection must still prove the final confirmation path is reachable and
readable; source ordering alone does not establish fit.

## Checks that already preserve the intended policy

- `CommunityContext` captures the full faction and player/restore context;
  roster, invitation, leadership or selected-faction changes expire the review.
- `CommunityReview::is_current` adds live dissolution readiness before native
  confirmation, while the actual domain command repeats full authority checks.
- The review explicitly says nearby data cannot establish absence of global
  assets. The remote test retains an unseen shared plot on the host and expects
  authoritative refusal, preserving property.
- Cancel emits no domain command. Successful confirmation consumes the review,
  so repeats cannot duplicate the same operation.
- The chosen scope is unused sole-member factions. The native copy does not
  promise populated-community succession, automatic value redistribution,
  refunds or general asset reclamation.

## Correction status

World owner implemented both recommendations. Source reinspection confirms:

- Process-wide checked monotonic review tokens also survive a `UiState` reset;
  exhaustion refuses instead of reusing consent.
- `refresh_displayed` retains only the current valid token while the Factions
  inspector is visible, so background updates of hidden labels cannot make an
  unshown review confirmable;
  `bind_review_action` captures it during the actual button Activate observer.
- Generic unbound Confirm cannot mutate; a stale ID preserves the newer review;
  current confirmation still rechecks principal/restore/faction/readiness.
- Review text and Cancel/Confirm now precede the ordinary faction summary.
  Shorter copy retains all material consequences and remote scope limits.
- An actual native observer/action-dispatch regression in
  `interface::actor_context_tests` creates two factions, attempts to confirm an
  unshown review, then delivers A's delayed confirmation after replacement with
  B. The positive control applies only the separately displayed B command to
  the domain and preserves A. This supplements the world owner's direct
  lifecycle/review tests and its hidden/closed-inspector binding regression.

No further material source blocker remains. Test code is present; executed
results and minimum-window confirmation remain the integrator's validation
responsibility. `start_input` also clears the retained displayed token when it
retires a review, as confirmed in the owner's final correction.
