# Terra World 0.23 — comprehensive product review

Review began **30 October 2026**, against the exact delivered **0.22.0** source.
The user requested product-wide review, current research, a comprehensive plan,
dedicated implementation and end-to-end delivery. The
[plan](IMPLEMENTATION_PLAN_v0.23.md) was written before dispatch;
[research](RESEARCH_v0.23.md) connects primary guidance with decisions.
[Validation](VALIDATION.md) records actual execution and its limits.

## Product assessment

Terra connects a reusable geographic world SDK to a substantial native reference
game: source provenance, streamed terrain, RTS/possession, finite economy,
resources, communities, transport, regional persistence and multiplayer
authority share domain rules. The strongest next improvement is consistency
across the player's complete action and persistence path.

The principal defects were concrete: Nature and Activities escaped part of the
displayed-body discipline added in 0.22; missing explicit world targets could
become different objects; guidance omitted capacity blockers authority enforced;
launcher maintenance did not acquire the session lease; and startup output was
bounded only after execution. These affect intent, recoverability and progress.

The selected pass closes those scoped gaps while preserving the established
visual identity, finite funds, source/catalog identity, saved schemas and exact
dependencies. The [G01–G18 register](REVIEW_GAPS_v0.23.md) retains broader product
requirements. This is a framework increment with a playable reference, not a
claim of finished AAA presentation or public MMORPG operations.

## Independent ownership and method

| Owner | Perspectives | Evidence |
|---|---|---|
| Player specialist | Onboarding/return, action meaning, selection/control, accessibility, focus, layout and recovery copy | [Player review](verification/v0.23-review/player-experience.md), actual activation/dispatch fixtures and current native captures |
| World/framework specialist | Goals, acquisition, supply, economy, community policy, simulation, authority and SDK seams | [World review](verification/v0.23-review/world-framework.md), shared preparation and finite conservation/continuation fixtures |
| Reliability specialist | Metadata concurrency, backup/recovery, privacy, file ownership, subprocess bounds, delivery and portability | [Reliability review](verification/v0.23-review/reliability-delivery.md), process/file adversarial checks |
| Integrator | Scope/research, compatibility, cross-module review, native journeys, canonical acceptance, preservation and delivery | This review, plan, validation, native and delivery records |

Specialists rotated across boundaries. World inspected maintenance/recovery;
reliability inspected economic preparation and native intent; player inspected
readiness/usability; the integrator reviewed process lifetime, target identity
and evidence. Source findings, rendered observations and executed tests remain
distinct. Historical images were context only; the baseline was freshly captured.

## Assessment across product perspectives

| Perspective | Finding and selected disposition | Remaining proof |
|---|---|---|
| Product fit and scope | Strengthen complete tasks using established domain/provider/renderer seams before expanding architecture. | Defined target experience and measurable production/global-scale requirements. |
| First-session onboarding | Creation/review/Activities exist, but a plausible next action could be impossible because readiness differed from authority. | Uncoached first-action/comprehension evidence on the intended PC. |
| Returning-player continuity | Live commitments rebuild from saves; maintenance races threaten continuity. Shared ownership and resumed supply fixtures improve it. | Uncoached return after setbacks/absence and sustained careers. |
| Interaction and consent | Consumption and frontier fallback paths incompletely retained targets. Exact displayed context now survives activation/queue/dispatch. | Broader latency/device journeys as surfaces expand. |
| Accessibility and information design | Current Nature capture has generic actor wording; operational panels are dense. Named targets and written blockers use existing scroll/focus conventions. | 200% typography, measured contrast, narration, localization and assistive users. |
| Gameplay progression | Supply, Home/Community and Road/Company remain connected; acquisition could stall on inventory limits. | Balance targets across beginner/advanced strategies and long sessions. |
| Economy/value | Finite escrow and atomic delivery existed; readiness omitted receiver/wallet capacity. Shared preparation preserves rejection and recovery. | Sustainable treasury/labor/capital policy and explicit offer retirement. |
| Community governance | Safe solo dissolution and explicit membership/leadership remain; territory actions gain reviewed identity. | Populated/asset-owning succession, treasury and property disposition. |
| Living simulation | Fixed time and bounded work remain; inactive owners no longer receive executable-looking guidance. | Sustained population/ecology scenarios with measured budgets. |
| Navigation/vehicles | Existing contact/corridor/vehicle recovery matrices remain; travel retains reviewed destinations. | Dense moving traffic, imported geometry and physical wheel/controller use. |
| Geography/data truth | Provenance and authored/synthetic distinction remain; all source data is preserved. | Licensed production data, NoData/datum/topology and constrained service placement. |
| Rendering/performance | Actual native pixels are inspected on software Vulkan; selected findings do not justify a graphics rewrite. | Physical GPU frame/loading/residency tails and content budgets. |
| Framework/developer experience | Additive readiness reduces adapter duplication and reuses command preparation; nine crate boundaries remain. | Downstream integrations and explicit future migration policy. |
| Multiplayer/authority | Servers recheck current state; display tokens are local intent protection, not remote reservations. Existing sequence/TLS/privacy rules remain. | WAN delay/loss/reconnect, soaks, enrollment and deployment capacity. |
| Security/privacy | Maintenance reuses private path/lease rules; desktop inspection stays passive unless execution is requested. | Publisher signing, public threat model, moderation/account recovery. |
| Reliability/recovery | Native backup needs coherent profile plus regional progress; maintenance holds the lease through publication and refuses busy worlds before output. | Target-filesystem crash/power-loss and offsite drills. |
| Concurrency | Sequential rename was already retained by final-settings rereads; overlapping read/replace intervals were the actual defect. | Older/nonparticipating writers and arbitrary administrative replacement remain outside this protocol. |
| Delivery/support | Exact extracted inventory validation remains; help output and pipe/process lifetime are now bounded during execution. | Windows clean-machine proof, installation/update/rollback and signing. |
| Observability/operations | Typed blockers and scoped subprocess outcomes make failures actionable; prior attempts remain evidence. | Deployment owners, retention/alerts, storage tail budgets and RPO/RTO. |
| Content/art/audio | Original validated models, datasets and authoring assets remain intact. | Production art/audio, equipment/animation breadth, rights and DCC workflows. |

## Implemented design decisions

### Displayed intent survives a later world change

Nature and direct Activities consumption retain the actual active person shown
to the player, using the same identity discipline as Command. Selection and
inspection remain separate. Resource, territory, guidance and supply actions
retain concrete context; an explicit missing target becomes unavailable instead
of falling through to a nearest object.

Activation captures the displayed review identity. Dispatch compares current
player/restore/body/target context and refuses pending transitions before
submitting an ordinary domain command. Authority still rechecks eligibility.
Rejected context changes preserve goods, wallet, needs, ownership and movement.
No command serialization or cross-session reservation was introduced.

### The suggested next step uses execution's rules

`supply_contract_readiness` exposes typed Accept, Deliver and Return conditions.
Delivery shares the preparation actually committed by authority: carried goods,
receiver stock/capacity, reward-wallet capacity, ownership, expiry and reach.
Missing receiving data is described as unavailable in the current view rather
than permanent global deletion.

The UI explains recovery: obtain goods, approach the destination, wait for stock
capacity, spend/transfer existing credits, or explicitly return the request.
Returning does not spend carried goods. Expired public offers are not advertised
as acceptable before actual maintenance refreshes them. A ready inspection
cannot reserve final capacity against another command.

Gather/Craft guidance uses complete bounded inventory exchange. Every recipe
output counts and consumed inputs can free space. It chooses a feasible smaller
batch or real alternative instead of repeating an impossible action. There is
no second inventory ledger or synthetic reward. Inactive owners receive a
resume instruction with no executable action.

### Maintenance shares ownership with play

Rename and native regional backup acquire a metadata-only guard for the stable
world lease. They reread the selected profile under ownership and retain the
guard through publication. Maintenance does not initialize progress, create a
launch witness or assert a checkpoint; the stable marker is coordination data.

Busy backup fails before destination creation. The backend online SQLite backup
remains available to explicit callers, but its database snapshot cannot freeze
a separate native profile. Existing publication uncertainty and retained inner
recovery artifacts keep their meaning. JSON worlds gain rename/lease safety;
this pass adds no native JSON backup format.

### Startup inspection bounds running work

The verifier replaces unlimited temporary output spooling with a bounded pipe
buffer and immediate overflow detection. The finite deadline includes output
closure because descendants can retain a pipe after leader exit. Peer review
found redirected descendants that avoid that test; POSIX success additionally
requires absence of the owned process group.

Cleanup requests and observed exits are distinguished. Windows success retains
explicitly narrower descendant evidence. This is an opt-in command-line startup
check, not a game-window/device test. Passive inspection runs no binary, and
manifest consistency does not authenticate a publisher.

## Peer findings and corrections

| Finding | Resolution |
|---|---|
| Dormant owners could still receive Accept/Return/Gather/Craft guidance through early branches | Gate all guidance tracks on active ownership; cover ready, missing, expired and unclaimed supply without mutation. |
| Valid help could leave a redirected background descendant | Reject a surviving POSIX group after leader exit/EOF, terminate it and retain a real-process regression; qualify Windows evidence. |
| New token allocation used a deprecated atomic method | Checked compare/exchange preserves the minimum Rust contract. |
| Plan wording implied native JSON backups existed | Correct scope before execution: regional backup, JSON rename/lease safety and existing manual archival policy. |
| Native Performance settings displayed the stored sky preference as if it were effective | Show “Off · Performance” and the retained preference separately in world, launcher and pause settings; confirm launcher and pause pixels after rebuilding. |
| The completed-contract count used plural copy for one; territory requirements exposed an item identifier | Use singular grammar and display “stone blocks” while retaining the original item key. |

Independent reports: [economic authority](verification/v0.23-review/world-reliability-cross-review.md),
[maintenance/recovery](verification/v0.23-review/reliability-cross-review.md),
[native intent](verification/v0.23-review/player-reliability-cross-review.md)
and [readiness/player goals](verification/v0.23-review/world-player-cross-review.md).
Executed corrections and final outcomes are recorded in Validation.

## Current-run native baseline

Fresh captures from the exact delivered 0.22 executable cover Activities,
Nature, empty Territory and pause/save at 1440×900. Activities shows its current
request and funded reward. Nature shows resource/reach information but only
generic consuming-actor wording. Territory has an explicit empty state. Pause
keeps save/review/exit controls reachable. The baseline saved and exited zero.

These images support visible-context findings. Source and tests support timing
and capacity defects that cannot be proved by pixels. The
[native record](verification/v0.23-native/README.md) separates baseline, candidate
and final confirmation with image/process/binary identity. Current source changed
during the baseline run; its binary remained exact 0.22 and is never treated as
0.23 acceptance. Native review is developer-operated software rendering, not an
uncoached study or physical-device qualification.

## Current-release journey and closeout polish

The candidate was operated through creation, saved 150% controls, a small managed
regional world, acceptance of the four-wood Riverside request, travel and
delivery. Completion visibly increased the wallet from 250.00 to 256.13 credits,
granted 40 XP and recorded one completed contract. Founding Review23 then moved
50 credits into its treasury and granted 10 XP; a further explicit 25-credit
contribution left the personal wallet at 181.13 and treasury at 75.00.

The same journey exercised the named Nature recipient and unavailable supplies,
resource reach recovery, territory selection/requirements, minimum-window
navigation, pause/save, returning-player continuity, rename, regional backup and
separate recovery. A specialist inspected all 52 candidate images. One bounded
copy correction batch addressed the three visible findings above; no subsequent
visual redesign or rules change was made.

Candidate launcher PID 11234 and its exit 0 are recorded. Gameplay captures observe
PID 12147, but no retained child process receipt independently binds that PID to
its parent or executable. The candidate also records unchanged source inputs
without retaining their full identity. These limits are explicit in the
[candidate audit](verification/v0.23-native/candidate-evidence-audit.md); its
screenshots establish observed behavior, not the final accepted binary.

The rebuilt final game was therefore launched directly. PID 14073 reopened the
recovered managed world, confirmed the completed contract, 50 XP, 181.13 personal
credits and the Review23 treasury at 75.00, then saved and exited0. PID 14392
independently confirmed the launcher copy and exited0. Their ten final captures
cover reference and minimum sizes with saved 150% text. Both processes retained
the same 381-input source digest and exact executable identity throughout.

The [saved-state inspection](verification/v0.23-native/state-inspection.md)
establishes complete logical equality between original and backup at checkpoint 23.
After resumed simulation, the recovered world advanced to checkpoint 35 while
preserving the named progression and treasury values. Its whole state is not
claimed to equal the earlier checkpoint. The immutable inspection correctly
refused an intervening active WAL before succeeding after clean exit.

Distance-blocked delivery is shown through travel guidance followed by a delivery
action after arrival. Receiver/wallet-capacity refusal, expired/missing targets,
same-frame transitions and busy maintenance are established by actual domain,
native activation/dispatch and separate-process tests; no screenshot is claimed
to demonstrate every adversarial case. This developer-operated finite journey
does not close uncoached career or physical-device research.

## Canonical implementation closeout

All **44 gates passed**, with **1,070 distinct Rust tests**, **1,171 Rust test
executions** and **50 Python tests**. No failed/ignored tests or gate timeouts
were recorded. The accepted executable is byte-identical to the direct final
native confirmation, and all 381 frozen inputs and accepted evidence remain
unchanged. [Validation](VALIDATION.md) supplies the actual terminal receipt,
crate counts and finite regional TLS workload. Artifact extraction and archive
preservation are separately bound in the delivery record.

## Compatibility, preservation and next priorities

Release **0.23.0** retains wire **11**, TLS **terra-world/11**, domain/regional/
asset schemas **1**, Rust minimum **1.97.1**, pinned compiler **1.99.1** and Bevy
**0.20.0**. Public readiness types are additive. Same-schema recovery does not
authorize source/catalog substitution or constitute migration.

Exact 0.22 current review and validation are archived; its root manifest is
retained under the new review evidence. Every baseline member, original asset,
dataset, authoring source and historical receipt is accounted for by preservation
and delivery records. Failed or partial runs are never relabeled.

Next priorities remain actual Windows/physical-device proof, uncoached complete
careers, physical runtime/storage budgets, populated-community/economic policy,
production content/licensed data, migration, trusted upgrades and a defined
public-service/operations deployment. The full gap register names each closure.
