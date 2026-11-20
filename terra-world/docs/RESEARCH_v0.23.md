# Terra World 0.23 — research and applied patterns

Research reviewed on **30 October 2026**. Primary documentation informs design;
the [validation record](VALIDATION.md) establishes what Terra actually executed.
The [implementation plan](IMPLEMENTATION_PLAN_v0.23.md) records scope and owners.
No external service was added as a dependency for this pass.

## Research questions

1. How can first-time and returning players understand which person or world
   object an action affects, its unmet requirements and the next useful step?
2. How should a persistent economy distinguish a displayed opportunity from
   admission against current authoritative state?
3. How can native session, rename and backup operations share ownership without
   confusing a lock marker, launch admission and committed progress?
4. What makes a source-free desktop startup check bounded throughout execution?

## Sources and application

| Primary source | Relevant guidance | Terra decision and boundary |
|---|---|---|
| [Xbox Accessibility Guideline 109: objective clarity](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/109) | Keep current goals and useful progress/next-step information available, including for players returning after time away. | Keep existing live-state Activities and commitments. Improve action readiness and recovery explanations without adding a second completion ledger. Automated fixtures do not establish uncoached usability. |
| [XAG 114: UI context](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/114) | Context and labels should make a control's purpose and required input understandable. | Name the actual consuming person and reviewed resource/territory. A selected wildlife body must not imply that food is consumed for it. |
| [XAG 115: errors and destructive actions](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/115) | Explain detected input problems and their correction; support review/correction before consequential changes. | Present concrete stock, wallet, reach, permission and expiry blockers. Preserve displayed target context through activation and execution; changed context asks for renewed review before mutation. |
| [XAG 112: UI navigation](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/112) and [XAG 113: UI focus](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/113) | Use consistent navigation and predictable visible focus, including modal containment. | Retain established native management focus, scroll areas, text scaling and protected review navigation. Verify affected workflows at reference and minimum window sizes. |
| [XAG 107: input](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/107) | Offer accessible input methods and avoid depending on demanding input mechanics. | Preserve keyboard/controller navigation paths and ordinary button activation. Physical controller, wheel, narration and assistive-device qualification remain separate. |
| [Bevy tab-navigation example](https://bevy.org/examples/ui-user-interface/tab-navigation/) | Bevy exposes explicit focus, navigation groups/order and visible focus feedback. | Work within the existing Bevy input/focus and activation event model. Do not introduce a separate pointer-only action path. The example is a pattern, not evidence that Terra is accessible. |
| [PlayFab Economy concurrency control](https://learn.microsoft.com/en-us/xbox/playfab/economy-monetization/economy-v2/tutorials/etags-and-concurrency-control) | A previously read state can change before a write; conditional writes reject stale expectations. | A read-only readiness result is not a reservation. Actual supply commands rerun the same preparation against current authority; concurrent last-capacity deliveries cannot rely on earlier UI success. |
| [PlayFab idempotent transactions and retries](https://learn.microsoft.com/en-us/xbox/playfab/economy-monetization/economy-v2/tutorials/idempotent-transactions-and-retries) | Retry identity prevents duplicate execution of one logical operation; it differs from stale-state protection. | Retain Terra's existing command sequencing/replay discipline. Native display tokens protect reviewed intent locally; they do not replace server authority or add a cross-session transaction protocol. |
| [Rust File locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock) | Nonblocking locks report contention; participating handles retain ownership until released. Interactions with ordinary file reads/writes depend on the platform. | Reuse the stable managed-world file lease for rename and native backup. Keep the guard through final publication. Never delete the marker to unlock, and do not claim protection from arbitrary nonparticipating writers. |
| [Microsoft LockFileEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex) | Windows specifies locking flags, lifetime and limits, including behavior distinct from mapped views. | Preserve portable Rust ownership semantics and explicit platform limits. Reading the Windows contract does not qualify a Windows executable or filesystem. |
| [SQLite Online Backup API](https://www.sqlite.org/backup.html) and [backup lifecycle reference](https://www.sqlite.org/c3ref/backup_finish.html) | SQLite offers a consistent database snapshot through a defined backup lifecycle, with completion/error rules. | Preserve the backend online backup path. The native wrapper additionally needs a stable profile/settings snapshot, so its managed backup uses the maintenance lease. A SQLite snapshot alone cannot freeze a separate native metadata file. |
| [Python subprocess](https://docs.python.org/3/library/subprocess.html) | Pipe lifetime, waiting and buffering require care; unrestricted buffered output is unsuitable for unlimited producers. | Drain a pipe into a bounded buffer, count overflow immediately, share a finite deadline with output closure, and perform bounded failure cleanup. POSIX background descendants that survive normal help make the check fail. Windows descendant certainty remains explicitly narrower. |

## Chosen patterns

### Reviewed intent, then current authority

The native label and the eventual command are separated in time. A robust
interaction must retain the target the player saw, then independently check
whether that target is still eligible. Comparing only command syntax is
insufficient when the same `Consume` command refers to a changing active body.
Likewise, a nearest-object fallback is useful for discovery but cannot replace
an explicit reviewed resource or territory during a consequential action.

The selected design uses small native context records and process-local review
identity. It preserves existing command serialization. Failed context checks
leave needs, stock, wallet, ownership and movement unchanged. Domain and server
validation remain the final authority even after a successful native check.

### One preparation seam for a truthful next action

Supply delivery has several independently changing conditions: ownership,
expiry, carried goods, recipient visibility/capacity, reward-wallet capacity and
reach. Copying those checks into multiple UI predicates creates drift. A shared
read-only query delegates to the same preparation used by actual commands.
The command prepares again, then commits the resulting values atomically.

Capacity must reflect the complete exchange. Craft inputs can free a slot;
multiple outputs can fill it again; an individual stack can reach its bound
even when other item slots remain. Guidance evaluates bounded candidate batches
against those existing rules and prefers an achievable alternative. It never
creates goods, issuer funds, a new currency or an unearned reward.

### Operation ownership is broader than a gameplay process

The 0.22 session lease protected play and final metadata, but maintenance writers
must participate for that protocol to be complete. Rename needs the same
ownership while re-reading and publishing a profile. A native backup needs it
while reading both profile and progress and publishing the wrapper. A separate
maintenance guard must not initialize a world or create a launch witness.

The operation can create a stable private coordination marker. That marker is
neither evidence of an active process nor proof that a checkpoint exists.
Published-but-unconfirmed outputs retain the prior release's truthful error and
retention semantics. Offline/native maintenance and backend online backup remain
distinct supported operations.

### Bound startup execution and output

Checking an output file's length after a child exits is a report limit, not a
resource limit during execution. The improved verifier retains at most its
accepted help limit plus the first overflow byte and uses pipe backpressure.
The deadline includes pipe closure because descendants can retain it after
the leader exits. A second review exposed redirected descendants that avoid
that EOF test; POSIX success now also requires absence of the owned group.

This check exercises command-line startup only. It does not open a game window,
play a saved world, authenticate the publisher or certify every Windows child
process. The passive path continues to execute no binary.

## Alternatives not selected

- Adding a remote service, distributed lock or new transaction ledger would add
  complexity without resolving the local displayed-target and maintenance gaps.
- Automatically returning/reassigning funds, dissolving populated communities or
  replacing stale supply offers would require explicit economic/governance policy.
- Updating Bevy, Rust or external dependencies would introduce unrelated churn;
  the exact existing dependency lock remains the baseline.
- A broad visual redesign would discard a documented native interaction system
  before the observed task failures were corrected.
- A Windows cross-build or software renderer would not substitute for the actual
  intended-PC create/play/save/reopen/recovery and physical-device proof.

## Evidence limits

These are researched design decisions and documented APIs. Acceptance requires
executed domain, native-system, process/file and packaging tests plus current
native captures. Linux software rendering supplies UI evidence for its actual
window/input conditions only. Full accessibility, uncoached journeys, sustained
balance, physical hardware performance, storage power-loss behavior, public
accounts, WAN capacity and production content remain in the complete gap register.
