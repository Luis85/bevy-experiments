# Cross-review — recovery compatibility and gameplay session consistency

Reviewer: player-experience owner, 29 October 2026. Reviewed peer source and
regressions only; compilation and execution are centrally owned by the
integrator. No peer source edits or cargo runs were performed.

## Persistence/schema admission

Reviewed `terra-persistence/src/lib.rs` `validate_schema`, `read_partition` and
checkpoint transaction admission; `operations.rs` read-only opening; and the
three added schema/duplicate/positive-backup regressions.

- **No source blocker found.** The expected column contract matches the actual
  application-generated schema-1 CREATE TABLE statements, including SQLite's
  `INTEGER PRIMARY KEY` singleton nullability representation. Declared type
  spelling is normalized rather than comparing CREATE TABLE SQL text.
- Schema admission is shared by read-only opening, complete checkpoint decoding
  and the write transaction boundary. An already-open world cannot silently
  accept a later external rewrite. Added constraints reject without resetting a
  store, replacing live state or acknowledging another generation.
- Hidden/generated columns and foreign keys are explicitly rejected. This
  avoids accepting additional authoritative expressions/cascade semantics under
  the same schema number. External equivalent-looking table rewrites are not a
  supported migration route; the stricter structural contract is documented.
- Positive backup/read-only/writable-reopen/restore/next-checkpoint coverage
  addresses the player's essential recovery journey. It also checks that the
  original source generation/state remain intact. There is no format migration
  and no reset of accepted existing application-generated schema-1 worlds.
- The duplicate-row fixture exercises the supported rejection path before
  corruption can become an accepted checkpoint. In that fixture the schema
  barrier rejects first; it is not independent branch-coverage evidence for the
  additional decoder duplicate-key check. Describe the check as defense in depth
  rather than claiming that fixture executes both guards separately.

Product consequence: previously damaged or externally rewritten stores can now
fail admission earlier and more explicitly. Preserve the original; use a known
valid backup or an explicit repair/migration workflow. The patch does not prove
repairability of a corrupted file, power-loss behavior, latency tails, Windows
storage behavior or deployment RPO/RTO.

## Gameplay dormant-session boundary

Reviewed `settlement/commands.rs::apply_settlement`,
`living/commands.rs::set_controlled_motion`, and the new settlement/living tests.

- **No source blocker found.** The dormant check is the first operation in both
  public entry points. It cannot deduct funds/goods, allocate a settlement,
  fulfill an escrow or reposition a body before refusing the action.
- Error wording names the useful next step: resume the owner's session. This
  agrees with the existing guarded command wrapper and prevents a direct SDK
  path from presenting contradictory behavior.
- Seven settlement command families are checked through both direct and
  wrapped routes. Each candidate is serialized before rejection and compared
  afterward, then the same command is admitted after explicit resume. The
  fixtures make upgrading and delivery feasible instead of testing only already
  invalid requests. Catalog validation accompanies the state checks.
- Controlled movement checks moving input and zero-speed repositioning. Both
  reject without changing the principal or body; explicit resume then enables
  ordinary movement. Trusted host placement remains a separate documented API,
  so regional hydration and session restoration do not have to masquerade as
  player intent.

Product consequence: reconnect/resume remains the explicit point at which an
owner can act again; no new price, progression, ownership or network account
contract is introduced. This is an SDK invariant correction and does not by
itself establish a reproduced network exploit, community succession, long
career balance or human reconnect comprehension.

## Own-work correction from delivery cross-review

Delivery identified a duplicate `TextLayout` in the launcher button bundle after
the shared text helper gained wrapping. Corrected by removing the redundant
button component and the redundant recovery-path insertion. Audited remaining
sites: editable native fields retain their independent no-wrap EditableText
bundle. The new regression constructs actual buttons on all twelve screens and
activates footer controls, so bundle initialization and action identity are both
within its execution scope. Execution still belongs to the integrator.
