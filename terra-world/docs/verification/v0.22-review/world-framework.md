# World, simulation and framework review — 0.22

Baseline: Terra World 0.21.0, reviewed on 29 October 2026. This is an initial
source/document/research review, before dispatch or implementation. No fresh
Cargo execution or current native screenshot inspection is claimed here.

## Product conclusion

The existing geographic, authority, living-world and delivery boundaries are
well aligned with a reusable game framework. This increment should retain those
boundaries and close concrete player lifecycle gaps. A speculative distributed
rewrite, a second economy ledger or a replacement movement model would add risk
without proving the player's continuous loop.

The strongest bounded missing behavior is **safe dissolution of an unused solo
faction**. The current rule makes a newly created solo faction permanent: its
only leader cannot leave, and no dissolution command exists. Creating several
experimental communities can exhaust the principal's affiliation slots even
when those communities have never owned land, public works, money or stock.
The restriction is documented, so this is a missing lifecycle capability rather
than a claim that the current implementation violates its declared contract.

## Perspective findings

| Perspective | Grounded position | Recommendation |
|---|---|---|
| First and returning play | Activities and commitments derive from real state, retain identities and distinguish costs from profit. The default selected-actor/needs gap remains in G04. | Coordinate the player's compact selection summary with the existing owned-body and partial-view semantics; do not create another completion ledger. |
| Community lifecycle | `create_faction` consumes a bounded affiliation slot; `end_faction_membership` forbids any leader's departure. No command dissolves even an empty solo group. | Add narrowly eligible dissolution with clear consequences, sole-leader authority and stale-review rejection. Preserve all occupied/shared communities. |
| Finite economy | Supply offers use finite issuer escrow and per-destination/item uniqueness. 0.21 guards heterogeneous price tables. Personal trades prepare all receiving bounds and recheck displayed totals. | Preserve these invariants. Broader long-session balance remains observation work; no evidence supports adding free income. |
| Transport | Low-frequency transport actions clone bounded player/living/transport ledgers and validate the candidate before committing. The apparent intermediate writes in pickup/finish are inside this transaction. | Do not report them as partial-mutation defects. Keep physical arrival, escrow and reservation proofs in regression coverage. |
| Simulation clock | Native living advancement runs in `FixedUpdate`; physical movement and accelerated calendar time are separate. Cold regional environmental records intentionally freeze. | Keep fixed authority and visual presentation separate. Exercise slow-frame/restore cases where touched; do not change economic time to wall time. |
| Navigation | Ground shortcuts inspect complete corridors with a per-command sample budget; autonomous contacts remain authoritative. | Existing bounded geometry cases are meaningful. Dense/moving traffic and source geometry need an expanded workload, not an unqualified autonomy claim. |
| Streaming/rendering | 0.21 exact source/edit contexts avoid rebuilding unrelated chunks. Native admission counts current relevant integrated sources. | GPU pipeline visibility, hardware frame tails and hung worker containment remain separately measured. Do not infer rendered readiness or FPS from CPU mesh integration. |
| Regional geography | Clipped/wrapped startup and explicit unknown terrain are implemented. Shared plots and territories remain in the global ledger; structures/farms can be cold. | Any deletion policy must account for global ownership and cold references. Never treat a client or regional visible subset as a complete ownership census. |
| Extensibility | Pure crates remain independent of the demo; providers, population settings and authoritative commands are clear extension seams. The SDK is pre-1.0 and enums are exhaustive. | Document new public enum variants as source-facing compatibility changes; update the protocol deliberately if introducing a command. Avoid mass `non_exhaustive` changes during polishing. |
| Content and operations | Original assets, explicit provenance and bounded examples are retained. The gap register already separates Windows/GPU/data/production-content proof. | Keep those conditions visible; shipping source must retain historical evidence and source licensing. |

## Proposed package WF01: close unused-community lifecycle

### Exact policy

An existing active principal may dissolve a faction only when they are its sole
member and current Leader, and the faction has **no shared land plot and no
settlement territory**. Existing invitations are explicitly revoked as part of
dissolution. No stock, cash, house, crop, body or individual membership in any
other group moves. No refunds are generated. The monotonic ID allocator is not
rewound. The operation frees the removed faction's affiliation slots, including
its outstanding invitations.

The policy deliberately does not implement unavailable-leader takeover,
elections, dissolution of a populated group, reclaim of shared assets, or
treasury withdrawal. Those require a separate design.

### Why the ownership proof is sufficient only on authority

`RegionPartition::partition_validated` retains every plot and territory in the
global state. `SandboxState::validate` requires each shared structure to be
contained in a plot with the same faction owner and each farm to match its plot.
Therefore an authoritative no-plot/no-territory check covers valid cold property
without scanning all cold pages. Defensive loaded-structure/farm/home checks can
make error messages explicit, but cannot replace the global proof.

A network replica's plots and territories are spatially filtered/truncated. Its
UI must not claim a complete no-assets proof from local absence. The server must
recheck eligibility immediately before deletion, and a fresh native review must
state that the authority checks current property. If local UI cannot know a
blocker, it may submit a reviewed intent and display the authoritative refusal;
it must not silently dissolve an occupied group.

### Implementation slices

1. Domain: add `Command::DissolveFaction { faction_id }`, a reusable read-only
   readiness helper and an all-checks-before-mutation authoritative method in
   `factions.rs`. Keep dormant-owner rejection in `SandboxState::apply`.
2. Native: use the existing CommunityAction/review framework in
   `terra-demo/src/communities.rs`; name the exact faction and invitation count,
   prepare an explicit confirmation and cancel action, retain player/restore/
   roster identity, and recheck property eligibility. Keep the existing scroll
   and management focus behavior. Distinguish dissolution from Leave.
3. Network/delivery: add command round-trip/authority coverage, deliberately
   advance protocol and ALPN if command-shape policy requires it, and preserve
   save/regional formats when no stored representation changes.
4. Docs: explain narrow eligibility, zero refund/asset transfer, invitation
   revocation and remaining G06 scope. Preserve the 0.21 documented restriction
   in archived release evidence.

### Acceptance criteria

- Sole leader can dissolve an empty faction, then create another without ID
  reuse; other communities and player state remain byte-equivalent.
- Other member, invitee, outsider, dormant owner, unknown ID and multi-member
  factions reject without changing any state.
- Any faction plot or territory rejects, including far-away territory and cold
  shared house/farm behind a retained global plot.
- Outstanding invitations are released exactly once; repeated command/replay
  cannot remove a newly created faction or return invented money.
- Shared property, public treasury, project stock, personal wallet/inventory and
  commitments are conserved through successful/rejected action and save/resume.
- Native Cancel emits no command; changed player, restored state, selected
  faction, member roster, invitation list or newly acquired property invalidates
  the review or causes authority refusal without mutation.
- A remote partial view does not label absent nearby property as a full-world
  eligibility proof. Native panel remains usable at 1100×700 and 150% text.

## Other concrete follow-up candidates

### WF02: retained-body observer policy consistency

`terra-bevy/src/living.rs::advance_population` currently collects every owned
body as an ecology observer without filtering dormant owners. The server's
observer path must be compared before changing this: scheduled ecology's own
comment says disconnected accounts are not implicitly near observers. A local
native world normally has one active owner; embedding a multi-principal world
can retain dormant bodies. A shared active-owned-body selector and a regression
would be a small bounded consistency improvement if the adapters differ.
This is a review candidate, not an executed reproduction.

### WF03: complete population placement on constrained geography

`ground_initial_population` grounds individual facilities and bodies when a
supported candidate exists, otherwise retains the seed position. The initial
avatar has a supported-ground gate, but the entire authored settlement is not
transactionally admitted against one complete placement plan. A small island,
edge of prepared coverage or dense source buildings can therefore need stricter
scenario placement design. Do not silently move or delete established saved
bodies in a polishing pass. Future acceptance should use constrained source
fixtures and distinguish inaccessible authored services from missing source.

### WF04: benchmark-supported world budgets

Add actual scenario/hardware evidence before changing chunk, mesh, AI or regional
caps. A path to render readiness must include prepared GPU resources or observed
frames, since asynchronous source integration alone is not visible rendering.
Keep unknown first-frame causes in G07 until reproduced. This is qualification,
not a source-only closure.

## Research and applied patterns

Primary sources were read on 29 October 2026. The links support the patterns;
they do not establish that Terra has passed platform or game-specific tests.

1. [Bevy — Run physics in a fixed timestep](https://bevy.org/examples/movement/physics-in-fixed-timestep/).
   The official example separates fixed physical state, per-frame input and
   interpolated visual state. Applied here: retain authoritative fixed updates
   and ensure presentation/quality changes do not mutate economy or motion.
2. [Microsoft PlayFab — Idempotent transactions and retry patterns](https://learn.microsoft.com/en-us/xbox/playfab/economy-monetization/economy-v2/tutorials/idempotent-transactions-and-retries).
   The official guidance treats repeated logical operations as the same request
   and distinguishes duplicate protection from optimistic concurrency. Applied
   here: reuse Terra's command sequencing and current-authority checks; preserve
   reviewed identity and test repeated dissolution/financial actions. This does
   not propose adding PlayFab or copying its retention policy.
3. [Cargo Book — SemVer Compatibility](https://doc.rust-lang.org/cargo/reference/semver.html).
   Adding an exhaustive public enum variant can break downstream matches, and
   pre-1.0 minor releases can carry incompatible source changes. Applied here:
   explicitly identify a new command/API variant and separate source, wire and
   stored-world compatibility. Do not retrofit `non_exhaustive` invisibly.
4. [Rust Reference — non_exhaustive](https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute).
   Future-facing enum design must be intentional; retrofitting it constrains
   downstream construction/matches. Applied here: retain established SDK types
   during this focused increment and document newly introduced seams.

## Remaining review limits

No new long-session balance, uncoached career, dense-traffic, physical GPU,
Windows, wheel/controller, WAN, asset-production or distributed-service claim
is justified by this source review. The current all-area gap register should
retain those owners and explicit acceptance criteria. Fresh focused tests,
cross-review and the canonical release receipt belong to implementation and
closeout, after the integrator dispatches the selected packages.

## Dispatched implementation status

The integrator selected WF01 and WF02 as WP03 and WP04 in the
[0.22 implementation plan](../../IMPLEMENTATION_PLAN_v0.22.md). The following
changes are now implemented in source. This section distinguishes implemented
regressions from executed results; the integrator owns the scheduled Cargo and
native acceptance runs.

### WP03: unused solo-community dissolution

`Command::DissolveFaction { faction_id }` and
`SandboxState::faction_dissolution_readiness` share one authoritative eligibility
path. The principal must exist, be active, be the current roster Leader and be
the sole member. Global plots or territories forbid dissolution, including
territories with zero public balances. Defensive loaded structure/farm and home
references also forbid it. Only the eligible faction and its invitations are
removed; fresh IDs, every personal wallet, public ledger, item, job, dormant
principal and unrelated faction remain untouched.

The existing regional invariant supplies the cold-state proof: plots and
territories remain global, and valid cold shared houses/crops require the same
faction on their containing plot. Tests exercise a rejected far shared property
and a successful empty-faction removal beside unrelated cold property and an
offline home, then check both merge paths. The public readiness documentation
explicitly limits a filtered replica's absence test; the native remote review
repeats this limitation and the server checks full current authority.

The native panel uses the existing review mechanism and now leads with review,
Cancel and Confirm before ordinary faction details. Player-specialist peer
review identified that the inherited generic Confirm action could consume a
later replacement review. The correction uses a process-unique, checked
monotonic review ID. Only label refresh records a valid displayed ID; native
activation queues `ConfirmReviewed(id)`. Cancel, replacement, text entry,
selection change and consumption clear the displayed binding. A stale ID cannot
consume a newer review, and raw unbound confirmation cannot emit a command.
Review IDs are ephemeral, with no saved or network representation.

Implemented regression coverage:

- Domain lifecycle: bounded affiliation-slot recovery, invitation revocation,
  inactive invitee preservation, exact world conservation, monotonic fresh IDs,
  repeated/unknown/unauthorized/inactive/multi-member refusal, save/load,
  shared stock/treasury protection and both regional merges.
- Native review: named consequences, Cancel, duplicate confirmation, changed
  principal/restore/faction/name/roster/invitations/property/dormancy, remote
  partial-view authority rejection, unshown review refusal and replacement or
  reset-UI token safety.
- Actual native activation/dispatch (player specialist): old faction A's queued
  confirmation cannot dissolve later faction B; only displayed B's own bound
  confirmation emits the command.
- Authenticated TLS: two principals, invitee refusal, dissolution snapshots,
  stale invitation and replay refusal, replacement ID, host save/restart,
  identity/sequence resume, multi-member refusal and later valid dissolution.

The integrator advanced the workspace to 0.22.0 and network protocol to 11. This
public exhaustive command variant requires downstream match updates and a
matched client/server version. No new stored field or automatic migration is
introduced.

### WP04: native active-observer consistency

The source comparison confirmed the native/server discrepancy: the server
limits ecology observers to connected owners, while the old native adapter
included every retained owned body. The native adapter now excludes dormant
owners without deleting their bodies or changing simulation rates. A focused
headless Bevy test calls the actual `advance_population` system with a far
active principal and a near dormant principal, verifies zero eligible near
ecology while dormant, resumes that principal and observes active near
decisions, then suspends it again. Clock continuity and retained body data are
checked. The test does not establish dense-world ecology performance or
rendered behavior.

### Verification handoff

Owned Rust files were individually formatted with Rustfmt (edition 2024), and
`git diff --check` passed before handoff. No Cargo execution is claimed in this
source-status section. Focused commands supplied to the integrator:

```sh
cargo test -p terra-sandbox --test faction_dissolution --test faction_lifecycle --locked --offline
cargo test -p terra-bevy --lib --locked --offline dormant_owned_bodies_do_not_activate_native_ecology_but_resumed_owners_do
cargo test -p terra-demo --locked --offline communities::tests
cargo test -p terra-demo --locked --offline native_community_confirmation_cannot_accept_an_unshown_or_replacement_review
cargo test -p terra-net --test faction_lifecycle_tls --features tls --locked --offline
```

Independent source review of reliability's publication and nested recovery
work is recorded in [reliability cross-review](reliability-cross-review.md).
The player specialist's review of this package is recorded in
[world/community cross-review](world-cross-review.md). Final native fit at
1100×700/150%, actual test outcomes and artifact acceptance remain separate
release evidence.

### Observed preflight results after handoff

The integrator ran the following focused gates; this reviewer inspected their
completed logs. These are preflight outcomes, not a substitute for the final
archive's source-bound acceptance receipt.

| Executed scope | Observed result | Evidence |
|---|---|---|
| Domain dissolution and existing faction lifecycle | 5 dissolution tests and 8 lifecycle tests passed; zero failed | [Domain preflight](../v0.22-preflight/faction-domain.log) |
| Authenticated faction lifecycle transport | Both TLS tests passed, including unused-faction dissolution and host restart/resume | [TLS preflight](../v0.22-preflight/faction-tls.log) |
| Complete Bevy library scope with TLS enabled | 140 passed; includes the actual dormant/resumed native observer regression | [Bevy preflight](../v0.22-preflight/bevy-lib.log) |

The native demo gate was being rerun after peer review tightened visible-panel
confirmation binding and replaced a deprecated newer-toolchain atomic helper
with an MSRV-compatible checked compare-exchange loop. Its final result and
rendered 1100×700/150% confirmation fit must be taken from the integrator's
completed evidence, rather than inferred from these already passed domain and
adapter gates.

The independent prepackaging preservation audit is recorded in
[preservation.md](preservation.md) and its complete machine-readable companion.
It found no missing or unexpectedly changed baseline content after the explicit
retention mapping and work-package allowlist. The source was still active; final
archive comparison and original archive modes remain packaging responsibilities.

### Native correction: management focus must enter the review on Cancel

The candidate walkthrough exposed a material interaction issue that source
ordering alone did not establish. At 1100×700 with 150% text,
[capture 56](../v0.22-native/56-candidate-dissolution-review.png) shows the complete
readable dissolution consequence text followed by Cancel. Entering management
focus with F6 in
[capture 57](../v0.22-native/57-candidate-cancel-focus.png) nevertheless outlined
Confirm. `command_input::navigate` explicitly preferred `confirm.or(cancel)` for
a new review, and the inherited focus regressions expected that preference.
The integrator selected the visible Cancel control;
[capture 58](../v0.22-native/58-candidate-dissolution-canceled.png) retains
Review Guild #1 and 250.00 cr, while exposing the smaller “1 members” copy issue.

The integrator authorized one consolidated correction batch. The focus system
now enters a newly opened, visible Factions review on Cancel. If Cancel is not
yet laid out or available, it waits and consumes the triggering acceptance
instead of selecting Confirm as a fallback. Existing transition consumption,
return-to-origin behavior, disabled-confirm recovery and deliberate subsequent
navigation remain. The displayed-review token and authoritative command paths
are unchanged. Singular member/invitation labels were corrected in the summary.

The new regression invokes the actual F6 capture and management navigation
systems with a prepared dissolution, first exposes Confirm while Cancel is
unavailable, then admits Cancel. It checks no default Confirm, no replayed
activation, later actual Cancel activation and exact domain conservation.
Existing focus regressions now exercise deliberate navigation from Cancel to
Confirm and preserve return/expiry behavior. Rustfmt and whitespace checks
passed before handoff. The rebuilt native result is independently reviewed
below; the canonical release gates remain the integrator's separate acceptance.

### Final native lifecycle and focus confirmation

Independent inspection of the rebuilt confirmation closes the observed focus
issue at 1100×700 with 150% interface text. The retained action and named review
are visible in [capture 78](../v0.22-native/78-confirmation-dissolve-action.png)
and [capture 79](../v0.22-native/79-confirmation-dissolution-review.png).
[Capture 80](../v0.22-native/80-confirmation-cancel-default-focus.png) shows the
complete consequences for Confirm Guild #2 and Cancel focused by default.
Enter on Cancel leaves that exact faction, its sole leader, the corrected
“1 member” text and 250.00-credit wallet intact in
[capture 81](../v0.22-native/81-confirmation-keyboard-cancel-preserved.png).
The separately opened [second review](../v0.22-native/84-confirmation-second-review.png)
again [enters on Cancel](../v0.22-native/85-confirmation-second-cancel-focus.png).
The recorded deliberate Tab then reaches
[Confirm](../v0.22-native/86-confirmation-explicit-confirm-focus.png); its explicit
activation is followed by the
[empty faction view](../v0.22-native/87-confirmation-dissolved.png).
Source reinspection confirms the Cancel-only handoff, unavailable-Cancel wait,
transition activation consumption and displayed-review token binding remain
in place; the domain still rechecks eligibility before removing only the
eligible faction.

The reviewer independently matched all eight image hashes and 1100×700 capture
records to PID 212865. Its [final process record](../v0.22-native/confirmation-managed-reopen-process.json)
reports exit 0 and binary SHA-256
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.
Recomputing the current 378-input source map matches both recorded execution
digests, `ec3fb5123a82cd1d4e6f5351d8f04503d305359628a44f61d8e032d9eca5902d`.
The [closed-world save inspection](../v0.22-native/confirmation-managed-save-evidence.json)
matches its referenced candidate record: SQLite integrity is `ok`, generation
advances from 40 to 60, no factions remain and the next domain ID advances from
2 to 3. Account, 25,000-minor-unit wallet, inventory, paused clock, witness hash
and lease-marker hash are unchanged; the inspected private files remain mode
0600. This closes the bounded native create/review/cancel/review/confirm/save
journey. It does not replace the broader automated conservation, cold-property,
stale-review or authenticated-replay coverage, nor qualify power-loss survival.
