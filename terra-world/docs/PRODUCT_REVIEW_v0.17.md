# Product review and improvement pass — 0.17

Review date: **2026-10-29**. The accepted [0.16 product review](PRODUCT_REVIEW_v0.16.md)
is preserved exactly. The [current gap register](REVIEW_GAPS_v0.17.md) retains
G01–G18 and separates this increment's completed repairs from wider product
qualification.

**Automated and native acceptance passed:** 43 final gates, **928 distinct Rust
tests**, **1,014 passing test executions**, zero failed and zero ignored, on
**367 unchanged implementation inputs**. Five subsequent direct native
confirmations used the exact accepted executable and exited 0.
[Validation](VALIDATION.md) records the evidence and its limits. The
[native](native-v17-verification.json), [desktop](desktop-package-v0.17.json) and
[release](release-verification-v0.17.json) receipts bind the completed acceptance.

## Product outcome

This pass makes the regional reference more dependable for playtesting and
iteration. A lost storage worker cannot leave the world falsely pending forever
or turn an uncertain commit into an apparent success. Driving actions retain the
assignment or rental terms the player actually reviewed. Testers can enable,
find and interpret bounded local reports through the normal menu.

These changes use the existing economy, ownership, source identity, character
customization and persistence rules. Save and regional schemas remain 1; wire
remains 10 with ALPN `terra-world/10`; the asset schema remains 1. No prices,
balances, rewards, hidden progression state or ownership authority were changed.
The preference compatibility caveat is separate from world-save compatibility:
older executables can reject the new optional diagnostic preference and fall
back to graphics defaults with a warning. See [Compatibility](COMPATIBILITY.md)
and [Playtesting](PLAYTESTING.md).

Three subagents owned player journeys, diagnostics and reliability. Reviews
crossed those boundaries. The release owner integrated the user-facing recovery
path, preserved competing error details, operated actual native journeys, and
assembled the frozen evidence. The
[product-perspectives review](verification/v0.17-review/product-perspectives-final.md)
and [review folder](verification/v0.17-review/) retain the findings and their
scope.

## Reliability and recovery

The storage worker is asynchronous. A lost completion may mean the database
already committed, even though the running game has not acknowledged it. The
repair therefore does more than clear a busy flag: it latches terminal failure,
retains the last acknowledged status, closes submission and makes polling,
finish, maintenance, backup and export consistently require validated reopening.
Ordinary completed SQLite failures still use the existing retry behavior.

A real lost-checkpoint-completion regression verifies the complete committed
archive and metadata at generation 2, reopens that database and successfully
writes generation 3. Lost hydration and idle request-channel loss have separate
regressions. Existing rollback, contention, corruption, crash and recovery tests
remain. All 38 persistence tests pass in the final gate.

Cross-review found that advice to reopen had no matching local session exit.
Terminal local failures now expose **Close failed world** in the loading/session
surfaces and use the existing final-barrier/error/handoff path. Stale or forged
close intents do not close a normal, network-owned or unrelated screen context.
Normal recoverable errors keep their retry/restore actions. The footer names
the actionable route. A failed final barrier cannot be reported as a successful
save.

Source and executed regressions establish this contract. The final native
screens do not inject worker loss. Storage latency, a wedged OS write, backup
retention, confidentiality and deployment recovery objectives remain open
qualification. See [worker review](verification/v0.17-review/reliability-implementation.md),
[Operations](OPERATIONS.md) and [Recovery bundles](RECOVERY_BUNDLES.md).

## Player actions, economy and guidance

Rental return used to retain a boolean confirmation without the complete
reviewed context. It now binds the rendered principal, restore context,
vehicle/model, selected and origin garage, bond and full refund breakdown.
Confirmation must actually have been displayed; the retained review is consumed
once. A changed quote or subject requires a fresh review. A routine prepaid
countdown alone does not invalidate unchanged refund terms. The authoritative
quoted-refund command still decides whether a return succeeds.

Contextual Interact now follows the active vehicle's accepted assignment even
when a different offer is being browsed. It retains the exact player, restore,
vehicle, job, phase, stop and deadline context, so a stale pickup action cannot
quietly become a delivery. Missing-stop, motion and reach blockers remain
explainable. Tests exercise ordinary acceptance/boarding and stale transitions;
fixture positioning isolates the handoff and does not measure human travel time.

The service hint uses the configured Interact key. Direct market opening no
longer advertises that contextual key. Vehicle focus/re-entry keeps a driving
description instead of reverting to a Worker label. These corrections preserve
the existing six-goal chooser, owner commitments, finite wallets/treasuries,
priced careers and continuing-supply behavior.

The final native driver restored the paid Comet with F9, moved under ordinary
controls, used the rebound K action, parked, switched view and saved/quit.
Screens show **Driving · Third person**, **K use stop**, then **Driving · First
person** with the Driving inspector open. This fixture initially has no accepted
job. No delivery, rental-return confirmation or refund occurred in that native
run; those safeguards rely on their explicit regression evidence.
See [journey review](verification/v0.17-review/player-journeys.md).

## Diagnostics and the tester handoff

The main-menu setting is **Off by default**. Save defaults retains the choice;
older preferences without it remain Off. The choice applies to the next world
or host connection launched from that menu. Graphics-only default updates keep
the saved diagnostic choice, and malformed preferences are not silently
overwritten. There is no automatic upload.

The fixed per-user-data destination is shown in Settings. Capture is first
attempted during the first running update, then every ten real seconds with a
final exit attempt. Sixteen samples, 64 KiB per sample, 2 MiB per file and one
active plus one queued write bound the cost. Busy attempts can be skipped and
are counted. A persistent marked sidecar uses an OS lifetime lock; another
writer cannot take the supported destination while it is owned. Actual process
termination releases ownership. Final acknowledgement follows lock release.

Pause and F12 show completed publication age, pending work and export errors.
Menu summaries are read on entry or explicit Refresh, not every frame. Reports
distinguish flow phase, actual virtual pause, calendar pause/time scale, queued
save/restore and committed checkpoint. A stopped writer no longer claims pending
work indefinitely. Supervisor notices retain separate specific world-save and
diagnostic errors; truncation reserves space for both.

Reports omit raw saves, inventory/balance payloads, raw game logs and private
resume capabilities. Tests explicitly place capability and typed-text sentinels
and check absent inventory/player payload keys; balance/log omission is also
source-reviewed, without inventing separate sentinel tests. Error strings can
contain local paths or source information. The guide tells testers how to find,
copy and review a report before sharing it.

A running or stale sample cannot certify current health. The final sample
records an exit request; process evidence separately establishes exit. Reports
are a short observational tail, not a backup, full-session journal or crash
dump. The new [Playtesting guide](PLAYTESTING.md) supplies an assisted route,
uncoached-session distinction, focused follow-up matrix, recovery steps and
issue template. See [diagnostic review](verification/v0.17-review/diagnostics-implementation.md).

## Native review and final polishing

The candidate journey enabled reports through Settings, saved that preference,
created a customized named world, paused at both review sizes, reached Save and
return by keyboard and observed the retained menu report. A separate candidate
driver exercised the prepared source and seven-model pack. Candidate records
are preserved separately and never inherit final acceptance.

After the correction batch and full gates, five direct processes confirmed the
frozen build. The [final native review](verification/v0.17-review/final-native-review.md)
and its image/process receipts record:

| Final route | Observed or measured result |
|---|---|
| Named world play/save | 49 settled terrain chunks; readable pause/report status; keyboard Save and quit at 1100 × 700 and 150% text. |
| Same named world reopen | Actual restoration; player identity, inventory, money, ownership, name, character choices and clock settings retained. Generation advanced 68 → 85. |
| Prepared paid Comet | Configured K hint and correct driving/view status; seven-model pack; no model failures or rejected instances. |
| Larger regional preset | 121 settled terrain chunks; 107 bodies and 64 wildlife; clean checkpoint/exit with bounded presentation. |
| Menu retained report | Exact prior-session/time/checkpoint shown; exit-request wording and stale-health limitation readable; Continue and Quit available. |

The save/reopen comparison uses private SQLite/profile copies after two ordered,
clean exits. The same 56 saved region rows are present; personal funds remain
25,000 minor units. It compares the declared stable player subset, not complete
equality of advancing simulation state. Raw private state is excluded from
delivery.

The final unobscured F12 capture shows the complete wrapped report destination
at the small window size, resolving the candidate's remaining visual item.
Independent subagents inspected the final diagnostic and driver captures and
requested no material UI correction. The review also corrected desktop recovery
documentation to distinguish ordinary retry from terminal close/reopen and
updated its current gap link.

## Review across product perspectives

| Perspective | Improvement or retained evidence | Remaining qualification |
|---|---|---|
| First-time/returning players | Existing six goals and commitments; assignment-aware contextual action; practical playtest route. | Uncoached understanding, motivation, setbacks and sustained content. |
| Economy/progression | Full reviewed rental terms, one-shot confirmation and authoritative assignment context. | Longer strategies, living/labor/capital costs and replenishment. |
| UX/information design | Correct role/key hints; visible capture age and status; actionable terminal exit; distinct failure messages. | Broader novice studies and dense-scene interaction. |
| Accessibility/input | Actual keyboard save flow, wrapping, scrolling and readable status at 1100 × 700 / 150%. | Physical controllers/wheels, assistive access, contrast, motion comfort and languages. |
| Visual/content production | Original wildlife, Comet and customized Fieldhand pipeline retained under generator/import/runtime gates. | Production rigs, retargeting, facial/equipment/passengers, audio and shipping budgets. |
| Simulation/navigation | Existing bounded handling, narrow/cluster geometry, queue recovery and exact continuation gates all retained. | Arbitrary dense traffic, complex geography and long interactions. |
| Authority/security | No new reward or ownership authority; stale-action and path guards; exclusive diagnostic writer. | Public enrollment/operator/moderation controls and trusted distribution. |
| Reliability/recovery | Terminal uncertainty remains blocked until reopen; lost-acknowledgement durability regression; actual named save/reopen. | Storage latency tails, offsite retention and deployment RPO/RTO. |
| Performance/resources | Bounded capture work/history, existing scene admission and a 121-chunk native working extent. | Target-GPU frame tails/residency, dense population and long storage/transport soaks. |
| Privacy/operations | Local opt-in, bounded summaries, separate save/export errors and explicit evidence age. | Crash diagnostics, external retention/audit/alerts and operational ownership. |
| Multiplayer | Existing real TLS, identity/resume, durable acknowledgement and larger regional workloads rerun. | WAN delay/loss, capacity/soaks, public-service policy and remote occupancy. |
| Developer workflow | Repeatable playtest protocol, preserved source/asset examples, exact gate and package provenance. | Broader DCC presets, content migration and licensed shipping data. |
| Delivery/maintenance | Frozen implementation, preserved baseline/history, extracted Linux smoke and independent archive audit. | Windows clean-machine execution, signing, installation/update/removal policy. |

## Readiness decision

This is a stronger **regional playtest reference**, with concrete failure and
interaction gaps closed and current evidence attached. It is not a claim that
every G01–G18 product area is complete. The most useful next work is executing
the Windows/hardware journey and observing new and returning participants with
the supplied protocol, then choosing economy/content/traffic improvements from
their actual blockers.

[Validation](VALIDATION.md) and the completed receipts are the acceptance
authority. The complete source archive contains current reviews, source,
screenshots, logs and history. The smaller Linux review archive contains its
accepted executable, reviewed player/content payload and automated evidence.
Its source/developer/historical links refer to the companion source project.
