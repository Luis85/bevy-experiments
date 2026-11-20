# Independent review of native action identity and dispatch

Reviewer: reliability and delivery specialist. Scope: WP01–WP02, with the
native consumers of WP03 supply readiness. This is an independent source and
test-design review, completed after the player specialist's dormant-owner
guard and final copy changes were present. It does not replace the coordinated
Rust test run, native screenshots or device qualification recorded by the
release owner.

**Result: no blocking action-boundary defect found in the reviewed candidate.**
The displayed target survives the actual activation, queue and dispatch path;
stale or unavailable identities fail before native command submission. The
implementation keeps domain command validation authoritative.

## Reviewed surfaces

- [Shared active-person identity and Command inspector](../../../crates/terra-demo/src/actor_summary.rs)
- [Frontier display contexts, tokens, resolvers and actions](../../../crates/terra-demo/src/frontier.rs)
- [Actual Activate observer, label scheduling and integration fixtures](../../../crates/terra-demo/src/interface.rs)
- [Native action dispatch and pending body transitions](../../../crates/terra-demo/src/interaction.rs)
- [Authoritative supply preparation](../../../crates/terra-sandbox/src/settlement/supply.rs)
- [Guidance intent comparison](../../../crates/terra-sandbox/src/guidance.rs)

The review traced the existing control and sandbox scheduling around these
files, not only the new helper functions. No player implementation file was
edited by this reviewer.

## Boundary assessment

| Boundary | Assessment |
| --- | --- |
| Rendered labels → activation | `refresh_displayed_actions` runs with `update_labels`. The real button observer captures the displayed review token, instead of resolving a new target when activated. Guidance refresh precedes the label update; availability follows it. |
| Queued activation → dispatch | `act_reviewed` requires both the originally captured token and a newly derived matching context. A raw, unbound frontier action that requires review is rejected by the dispatcher. |
| Token lifetime | Tokens come from a process-wide atomic compare-and-exchange counter. Checked addition prevents wraparound; exhaustion yields no new actionable review. Replacing `UiState` cannot make an earlier token refer to a new display. Relaxed ordering is adequate for uniqueness because the ordinary UI resource owns the context. |
| Owner, body and restore | Every reviewed context includes owner, restore revision and active body identity. Consumption additionally requires a living, owned person. A missing or dormant principal cannot produce a current review context. |
| Same-frame selection changes | A preceding Next resource, Next territory or Next contribution changes the current context, so a following old-token action cannot act on the newly selected destination or amount. A refreshed display also invalidates the old queued token. |
| Pending control or restore | Queued control-mode, actor-cycle, possession and release changes, plus queued sandbox active-body, vehicle-entry/exit and restore operations, prevent reviewed action submission. Dispatch occurs before sandbox command application. |
| Disconnection | A present network status with `connected == false` blocks reviewed native submission. Local standalone sessions do not require a network-status resource. |
| Resource identity | An explicit resource selection resolves only that ID with the selected item kind. Missing or incompatible selections return unavailable; they do not borrow a nearby patch. The retained context includes selection, kind, ID, item and position. |
| Territory identity and value | An explicit territory selection resolves only that ID. The context retains the full territory, chosen contribution and computed contribution amount. Changed treasury/project state may conservatively require another review, rather than silently changing the displayed operation. |
| Contract identity | Native contract actions retain the full displayed contract and selection. Suggested guidance retains its command intent and supply-contract snapshot. A newly eligible fallback request must become a newly displayed review before an old click can act on it. |
| Shared supply readiness | Native availability and preparation use the common readiness result. Its result is a present-condition check; the eventual domain command still prepares against authoritative current state. |

The player specialist added the dormant-owner guard while this peer review was
in progress. The final implementation was inspected in both
`ConsumeTarget::active_person` and frontier `action_context`. The corresponding
integration fixture now makes the owner dormant **after** the actual Activate
event, then verifies that dispatch adds no command and does not alter world
state. This closes the availability concern considered during this review.

## Consumption semantics and player copy

Nature and Suggested actions deliberately name and retain the active person,
even while the user inspects a different body. Command consumption continues
to require that the one selected owned body is also the active person. These
are distinct, explicit entry points with the same owner/body/restore checks;
the implementation does not borrow the inspected body's needs or silently
consume for a replacement active body.

Labels identify the active person and resource or territory. Missing explicit
destinations explain that the selected target is unavailable in the current
view and leave Next controls available to choose another. Supply readiness
explains blockers and possible recovery without presenting absent replicated
state as proof of global deletion. The native screenshots and responsive
layout checks remain the release owner's separate visual evidence.

## Regression quality

The reviewed tests exercise more than context comparison in isolation:

- `every_native_frontier_consumption_retains_the_displayed_active_person`
  fires the actual Activate observer for berries, cooked fish, water and
  Suggested consumption. It then changes body, restore revision, player,
  ownership, queued actor control, queued active-body selection or dormancy.
  It asserts no additional command, complete world-state equality and an
  explicit review message.
- `native_frontier_activation_cannot_cross_a_queued_patch_choice_or_a_refreshed_display`
  covers the real observer/queue/dispatcher path for a prior chooser action
  and for a display refreshed after activation. A newly reviewed click then
  names the exact replacement patch in the submitted command.
- `nature_and_suggested_consume_for_the_named_active_person_while_inspecting_another_body`
  applies the resulting ordinary command and checks the correct inventory
  decrement, changed active-person needs and unchanged inspected-person needs.
- `disconnected_and_unbound_frontier_actions_cannot_submit_native_consumption`
  checks both the disconnected case and attempted dispatch without a review.
- Resource and territory regression matrices cover missing/replaced IDs,
  selection and value changes, restore changes and successful unchanged
  contexts. Supply tests cover wallet-capacity blockers, return availability,
  expiration and successful delivery through the shared readiness path.
- Existing Command tests continue to cover consumption target changes, queued
  body transitions and owned-selection Stop/Rest behavior.

The source review accepts these as meaningful regressions. Their pass/fail
status belongs to the retained coordinated native test results; this report
does not infer execution from their presence in the tree.

The coordinator subsequently retained the full current native candidate run:
**238 tests passed, 0 failed, 0 ignored**, exit code 0, including the reviewed
frontier and real Activate fixtures. The exact command and scope are in
[native-integration.json](../v0.23-preflight/native-integration.json), with
[the complete log](../v0.23-preflight/native-integration.log). This candidate run
is separate from final canonical acceptance and native visual qualification.

The later [actual canonical receipt](../v0.23-release-acceptance/completed-gates.json)
completed all 44 gates and again passed the full 238-test native suite, with no
failed or ignored tests and no gate timeouts. The
[accepted native provenance audit](../v0.23-native/confirmation-provenance-accepted.json)
separately binds final direct-process captures to the accepted executable and
unchanged implementation map; it does not relabel the earlier candidate
gameplay's missing child-process association.

## Deliberate limits

Review tokens preserve presentation identity. They are not authentication
credentials, one-use transactions or remote reservations. Separate repeated
activations may submit separate ordinary commands; authoritative inventory,
capacity, reach, ownership, expiry and other checks continue to decide each
result. The patch does not redesign the network protocol or reserve another
client's view of a world.

Resource gathering remains an explicitly bounded “up to” amount against the
same reviewed patch, with current resource quantity and command validation.
The review does not require a new resource incarnation solely because its
remaining quantity changes. Full territory snapshots intentionally choose
more conservative review invalidation for value-changing project actions.

No claim is made here about screen-reader compatibility, uncoached player
comprehension, Windows execution, controller hardware or graphical-driver
qualification. Those require their own recorded execution or participant
evidence.
