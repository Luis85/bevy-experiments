# Product review and improvement pass — 0.14

Review date: **2026-10-28**. This review follows the accepted
[0.13 product review](PRODUCT_REVIEW_v0.13.md) and
[gap report](REVIEW_GAPS_v0.13.md). Their evidence remains historical and
unchanged. The current [gap register](REVIEW_GAPS_v0.14.md) preserves all
**G01–G18** identifiers from the original
[0.12 open-gaps report](OPEN_GAPS_REPORT_v0.12.md).

**Source and native acceptance passed.** All **26 frozen-source gates** completed
with **828 distinct Rust tests**, **895 passing executions**, zero failed or
ignored tests and **347 unchanged inputs**. The exact accepted executable passed
Company interaction, both restored-world launches and controlled process
interruption/restart confirmation. The [gate record](verification/v0.14-final/gates.json),
[native record](native-v14-verification.json) and [validation](VALIDATION.md)
identify the inputs, executable and environment. Desktop packaging and extracted native checks are documented separately in the
[package record](desktop-package-v0.14.json) and
[release record](release-verification-v0.14.json).

## Product outcome

This increment improves four moments where the previous reference experience
could lose continuity. A returning company owner receives an explanation of the
actual service and a precise next action. An artist can turn exported models
into a validated immutable pack through an explicit recipe. An operator can
retain the database together with the catalog, configuration and source inputs
needed to reopen it. Support can examine bounded diagnostic samples written
before an interrupted process exits.

Each addition uses an existing product contract. Guidance reflects real
ownership, company funds and physical obligations. Asset authoring uses the
runtime importer and its admission limits. Recovery validates the exact local
host state. Diagnostics remain outside gameplay authority. Gameplay save
version **1**, regional schema **1**, wire protocol **10** and TLS
**terra-world/10** are unchanged; see [Compatibility](COMPATIBILITY.md).

The result remains a regional SDK and playable reference game. The source
changes do not establish a complete campaign, a production character/vehicle
content library, qualified Windows delivery, full accessibility or a managed
online service. The remaining conditions are explicit in the gap register.

## Player experience: returning to useful work

### Entry and discoverability

Activities adds **Run your transport company**. The suggested journey still
puts critical needs and accepted personal work first. Selecting Company
explicitly keeps the player's chosen focus. Before a service is ready, its
steps follow actual registration, funding, recruitment, route comparison and
vehicle assignment. They distinguish personal cash from company cash and use
the ordinary paid commands.

For an established service, the briefing identifies the company, driver,
route, actual next stop or return pickup, completed paid deliveries and
available company funds. Attention states take priority; within that group, a
loaded obligation comes before an empty waiting service. An idle spare driver
cannot hide a problem in an assigned service. A facility outside the current
view is unknown, rather than evidence that it has been destroyed.

The applied polish keeps the existing Activities hierarchy, placing the primary
action before detailed financial and operational explanation. Accepted-executable
confirmation exercised focus, scroll reachability, enlarged text and cancellation
in that layout. The native evidence and its limits are described below.

### Actions and recovery

A guidance action opens the reviewed Fleet section, driver, vehicle, route and
facility. An expired rental opens the exact vehicle in Garage. Opening these
panels does not resume service, deposit funds, release a load or return a
vehicle. The player still reviews and authorizes the ordinary command.

For a held service, the handoff opens Recovery. Final copy review changed the
instruction to name that panel's **Resume delivery / return** control, so the
guidance and the destination now describe the same action.

The language separates **Pause safely**, which finishes the current obligation,
from **Hold in place**, which keeps the load and stops it immediately. It
explains that rental time can continue while payroll is stopped, that returns
preserve obligations, and that available stock, queues and public or buyer funds
are finite. This makes existing physical recovery easier to find without
claiming new route-planning behavior.

Cross-review corrected two misleading explanations. A full destination now
separates an empty pickup, a loaded delivery and a failed return/refund; it does
not suggest returning cargo that does not exist. A payroll hold can arise from
the employee wage-wallet limit, so guidance no longer promises that another
company deposit always fixes it. These decisions and their regressions are
visible in [Company guidance](../crates/terra-sandbox/src/guidance/company.rs),
[domain tests](../crates/terra-sandbox/tests/company_guidance.rs) and the
[Guidance guide](GUIDANCE.md).

### Trust in displayed actions

The read-only handoff binds the reviewed owner, assignment, status, next stop
and commitment. Ordinary movement and payroll accrual do not invalidate that
intent unnecessarily. The native layer also binds the displayed player and
restore revision, so matching object IDs after a restored world cannot silently
reuse stale guidance. It clears unrelated Fleet and Garage confirmations;
subsequent mutations still perform their ordinary authority and quote checks.
The integration and physical review fixture are in
[the native frontier](../crates/terra-demo/src/frontier.rs).

## Game design, economy and autonomy

This increment does not rebalance prices, grant free cash, reserve advertised
stock or rewrite accepted commitments. Setup guidance identifies registration,
company funding and hiring as distinct costs. Financial explanations retain the
limits of the existing planning and accounting rules, including legacy worlds
without a complete historical ledger. See [Companies](COMPANIES.md) and
[Settlements and economy](SETTLEMENTS_AND_ECONOMY.md).

The final gates reran all three commercial careers and reproduced their prior
measurements. The [managed passenger career](verification/v0.14-final/novice-career-report.json)
completed 65 deliveries over 3,600 service seconds, then completed its load and
returned the rental. Its gain after known cash costs was 694.48 credits, with
five explicit wait/resume interventions. The
[contributed-stock](verification/v0.14-final/career-report.json) and
[purchased-stock](verification/v0.14-final/purchased-career-report.json) warehouse
cases retained losses of 84.69 and 161.49 credits. Living/management costs and
finite funding remain material limits. These fresh results support one managed
strategy; they do not establish indefinite replenishment, broad balance or
unattended autonomy.

The prepared-source review fixture reaches company setup using the ordinary
starting wallet and paid commands. It starts near the garage and completes its
ordinary approach in one 0.05-second step, then drives for 2.8 seconds to a real
passenger pickup before holding the load. The
[journey record](verification/v0.14-native/company-review-journey.json) makes
that short setup explicit. It supplies a reproducible later-state UI review,
not a substantial walking benchmark or an uncoached player study.

Vehicle and walking behavior retain their existing bounded handling, obstacle
checks and explicit recovery commands. Better goals can reveal those controls,
but multiple moving obstacles, narrow junctions, blocked alternate exits and
missing/restored terrain still need wider declared scenario coverage. The
accepted basic community departure, removal and handover behavior is also
retained; unavailable-leader succession, dissolution and treasury policy remain
separate product decisions. These distinctions keep G03–G06 open at their
original scope.

## Artist and developer experience

The authoring path is now concrete: inspect an exported GLB, map named clips and
bindings in a recipe, build a new pack directory, validate the resulting runtime
manifest and preview it in the native game. `terra-assets inspect` reports the
actual nodes, clips, durations and importer budgets. `terra-assets build`
snapshots its inputs, computes content-named files and runs the complete runtime
validator before publication.

Artists no longer need to coordinate model hashes and runtime manifests by
hand. The authoring report retains the recipe identity and declared provenance.
Identical input bytes and recipes produce identical output bytes. Two bindings
can share model bytes while retaining different animation policies; unused
clips and invalid hierarchies still count against admission and allocation
limits. Duplicate model bytes do not create an exception to declared model
budgets.

The destination must be new. Failure cleanup removes only files created by the
current build, and the runtime manifest is published after its dependencies.
This avoids updating source exports or an existing pack in place. It is not a
promise of whole-directory atomicity across power loss. The native runtime
continues to keep its procedural actor visible until an admitted replacement is
ready; animation integration remains presentation-only.

The included recipe uses the five original wildlife GLBs and their existing
idle, walk, run, feeding and drinking clips. Their default runtime identity is
unchanged. The guide supplies units/axes, supported export settings, named clip
mapping and corrective advice for rejected inputs. It gives an example that a
developer can reproduce, while preserving character customization. The final
[recipe build](verification/v0.14-final/asset-build.json) and
[runtime validation](verification/v0.14-final/asset-authored-validate.json) accepted
the same five-model pack identity. In the accepted executable,
[telemetry](verification/v0.14-native/v14-confirm-prepared-company-telemetry.json)
recorded 22 ready authored-model instances, with zero failed models or rejected
instances. This is a working native handoff, not only a manifest example.

Trace: [authoring guide](ASSET_AUTHORING.md),
[authoring API](../crates/terra-bevy/src/asset_authoring.rs),
[original wildlife recipe](../crates/terra-bevy/assets/wildlife-authoring.recipe.json),
[authoring tests](../crates/terra-bevy/tests/asset_authoring.rs) and
[runtime asset contract](ASSET_PACKS.md).

Author-declared provenance does not establish publisher trust or legal rights.
Representative human/customization content, vehicles and cabins, seated and
boarding transitions, attachments, audio, DCC presets and hardware-level
material/LOD budgets remain unfinished production work. A successful wildlife
build cannot qualify all those content classes.

## Operations, reliability and support

### A recoverable local unit

`terra-worldctl bundle-create`, `bundle-verify` and `bundle-restore` retain and
validate a complete recovery unit for the reference local regional host. It
contains a consistent committed SQLite snapshot, exact built-in catalog,
resolved configuration, prepared terrain files and an optional bounded local
political input. Procedural worlds bind the supported generator revision.
Saved authored politics remain in exact host metadata, separate from the base
source bytes.

Root integration found that the first recovery design excluded the political
input used by ordinary native menu worlds. The implementation now captures that
dependency and validates both the base dataset and authored saved state. A
`--no-default-politics` launch option preserves an intentionally politics-free
configuration on native reopen; explicit political inputs are still honored.
This closes a real mismatch between a headless restore and the default game.

Restore creates a new destination with rebased runtime paths. Verification
checks inventory digests, catalog/source identity, SQLite integrity, complete
domain and local host metadata, preserving the exact committed generation and
cold regional records. An expected bundle identity retained separately can
detect substitution of the complete set. It is not encryption or publisher
authentication.

The final [bounded recovery rehearsal](verification/v0.14-final/recovery-bundle.json)
passed all 15 verification flags: generation 2, 1,029 nonempty regions, 4,096
resources, two principals, owned bodies, one faction and shared property. It
removed the original database/configuration/source tree and retained complete
state, cold records, source bytes and authored politics on restore. A damaged
copy rejected without restored output. Creating and restoring the 12-file set
took 649.445 and 368.490 ms on this machine, including validation. These are
maintenance measurements, not deployment RPO/RTO; distant records do not extend
terrain coverage.

Two additional rehearsals used the actual reviewed worlds. The
[prepared Company input](verification/v0.14-native/v14-prepared-recovery-summary.json)
and [ordinary menu-created world](verification/v0.14-native/v14-menu-recovery-summary.json)
retained byte-exact complete exports after their original test inputs were
removed and restored. Both opened with the accepted executable from absolute
restored configurations in an unrelated working directory. The default menu
world rendered 49 chunks and retained its political dependency; the prepared
world remained deliberately politics-free. Both confirmation processes exited
successfully.

Cross-review additionally protected absent WAL/SHM/journal names before claiming
output paths, rejected restore destinations inside the retained bundle before
copying, and made generated archive member names portable while continuing to
reject untrusted backslashes. Unix permissions are private; platform behavior
still needs Windows execution.

Trace: [Recovery bundles](RECOVERY_BUNDLES.md),
[recovery implementation and regressions](../crates/terra-bevy/src/recovery.rs),
[recovery rehearsal](../crates/terra-bevy/examples/recovery_bundle_probe.rs) and
[Operations](OPERATIONS.md).

Deployment still needs retention, confidentiality, offsite transfer, named
recovery owners, matching application availability and rehearsed RPO/RTO.
Server databases, live sources, historical packs and custom catalogs require
their own dependency and metadata policies. The local recovery unit is useful
without claiming that broader service readiness.

### Retained evidence after interruption

Periodic native diagnostics retain the latest 16 successful bounded samples
across runs. A background writer admits one in-flight and one queued sample;
busy periods skip new samples instead of accumulating a backlog. Real-time
capture continues while virtual gameplay time is paused. Each sample identifies
its run, sequence, timestamp and running or exit state.

Publication writes and synchronizes a temporary file before replacement. Failed
publication retains the last complete report. Final export follows the world
checkpoint barrier, and a failed requested final export produces an error exit.
The output guard now checks ancestors as well as the leaf, so creating a
report directory cannot occupy a future database journal, save or session path.
Final cross-review extended that protection to screenshot and political-export
parents and checked every export kind against current and future multiplayer
session paths before startup. Three focused regressions in
[output guards](../crates/terra-demo/src/output.rs) cover those additions;
[startup](../crates/terra-demo/src/main.rs) applies the checks. Unrelated files
and invalid existing diagnostic histories are rejected.

The final [process-interruption rehearsal](verification/v0.14-native/v14-interruption-event.json)
sent SIGKILL to the owned test process and retained 15 complete samples, all
explicitly marked `running`. Restarting with the same diagnostic path retained
14 prior samples plus two new ones, bounded to 16. The
[continuation](verification/v0.14-native/v14-interruption-continuation.json)
resumed the world, exited 0 and ended with `graceful_exit`, with no world,
simulation or storage error. The authored pack again had 22 ready instances and
zero failed models. This verifies the local interruption/restart path; it does
not simulate power loss.

Reports contain timings, counts and errors rather than full inventories,
balances or resume capabilities. Local paths and source identifiers may still
need review before sharing. A surviving `running` sample tells support where
available evidence stopped. It does not certify a healthy exit; a stale or
missing sample is unknown health. The implementation does not provide crash
dumps, external alerts, continuous traces or a durable audit service.

Trace: [Diagnostics](DIAGNOSTICS.md),
[retained writer and tests](../crates/terra-demo/src/telemetry_capture.rs),
[capture integration](../crates/terra-demo/src/performance.rs) and
[Performance](PERFORMANCE.md).

## Performance, security, compatibility and delivery

| Perspective | Current assessment | Qualification still required |
|---|---|---|
| Performance and scale | Import budgets, periodic sample size/history and writer admission are bounded; recovery explicitly validates complete cold state. | Target-hardware GPU/frame-tail/residency measurements, authority storage-pause distributions and longer representative workloads. A finite recovery fixture is not a global-capacity test. |
| Privacy and authority | Company advice is owner-scoped; handoffs retain principal/restore context. Recovery includes private local host metadata and rejects unsupported host contracts. | Deployment access, confidentiality, offsite handling, public account recovery, moderation and durable audit ownership. |
| Files and content trust | Immutable authoring, source snapshots, inventory hashes, exact identities and protected output namespaces make local failures explicit. | Publisher authentication, signing, authenticated delivery, version selection and rollback policy. Declared provenance and hashes alone do not establish trust. |
| Compatibility | Save version 1, regional schema 1, protocol 10 and existing content authority remain intact. Frozen-source compatibility and restored-native checks passed; prior reports are archived. | Retain regression gates in subsequent releases; deliberate content migrations still need separate policies. |
| Delivery and business readiness | The source includes reproducible authoring/recovery commands and a reviewable gap register for a regional reference release. | Clean-machine Windows acceptance, supported hardware/device matrix, shipping content/licenses, installer/update/removal policy, operator support ownership and uncoached player evidence. |

## Native UX and accessibility acceptance

The final [Company confirmation](verification/v0.14-native/v14-confirm-prepared-company-process.json)
used executable SHA-256
`836521f99cbfb714fc23214d760f43af4bef1495d2c5007a1d53e879c5e8f531`,
matching the accepted native-build gate. At 1100 × 700 with 150% text it opened
the exact Recovery section, reached the action by focus/scrolling, reviewed a
named return without accepting it, canceled through the unrelated Company
section, checkpointed/restored with F5/F9 and saved/quit with exit status 0.
The [final comparison](verification/v0.14-native/v14-confirm-company-conservation.json)
retained the **entire export byte for byte**. The earlier
[candidate comparison](verification/v0.14-native/v14-company-review-conservation.json)
proved nine unchanged subsystems while streaming could add other environmental
records; these are separate observations.

| Confirmed interaction | Screen evidence |
|---|---|
| Company goal focus with enlarged text | [Company focus](screenshots/v14-confirm-company-focus.png) |
| Recovery action remains reachable | [Recovery action](screenshots/v14-confirm-recovery-action.png) |
| Named return can be reviewed before acceptance | [Return review](screenshots/v14-confirm-return-review.png) |
| Switching section cancels the review | [Canceled review](screenshots/v14-confirm-review-cancelled.png) |
| Checkpoint restore and clean save/quit | [Checkpoint restore](screenshots/v14-confirm-checkpoint-restore.png), [save/quit focus](screenshots/v14-confirm-save-quit-focus.png) |

The [menu-world reopen](verification/v0.14-native/v14-confirm-menu-recovery-process.json)
and [post-interruption restart](verification/v0.14-native/v14-confirm-retained-restart-process.json)
used the same accepted executable and also exited 0. Source inputs remained
unchanged through these confirmations. The native environment was Linux/Xvfb
with Vulkan llvmpipe software rendering; the observations establish this bounded
flow and restored content, not target-hardware frame-rate qualification.

The confirmation includes focused keyboard actions as well as pointer
interaction. Complete keyboard/controller/wheel journeys, reconnect ordering,
assistive access, contrast, motion comfort and localization still require their
own acceptance; these specific interactions do not establish the full matrix.

## Release judgment and follow-up

The source increment closes concrete continuity, authoring and local recovery
failures. Independent review also corrected financial advice, stale restored
context, portability, protected output paths and a default-world dependency
that would otherwise weaken native recovery. Frozen-source tests and native
confirmation establish that these paths work together in the accepted executable.
Desktop package integrity and extracted native execution have their own
[package record](desktop-package-v0.14.json), scoped to the recorded Linux host.

The next player-facing priorities are supported Windows/hardware qualification
and observed new/returning play. Those observations should determine whether
longer goals, economic pacing or navigation failures most often stop progress.
For content, run one representative human/vehicle/boarding slice through the
authoring workflow and select a licensed regional dataset. For operations,
rehearse an independently retained offsite copy against explicit recovery
objectives and name the responsible operator.

Public accounts, WAN capacity, audit retention and alerting remain separate
service work. Global authority partitions should follow demonstrated regional
requirements. The [complete G01–G18 register](REVIEW_GAPS_v0.14.md) provides the
remaining conditions without treating this bounded increment as closure of
those wider product promises.
