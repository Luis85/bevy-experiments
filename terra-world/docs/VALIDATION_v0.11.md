# Terra World 0.11 validation record

Validation date: **2026-10-28**. Source release **0.11.0**, wire protocol **8**,
TLS ALPN **terra-world/8**, domain save version **1**, regional database schema
**1**. The fresh automated sequence contains **700 distinct passing
Rust tests**, with zero failures and zero ignored tests. These results establish
correctness within the bounded SDK and reference-game workloads described below.

The [gate report](verification/v0.11/gates.json) and its command logs are the
current automated evidence. The [documentation-led review](REVIEW_GAPS_v0.11.md)
traces the corrections and remaining acceptance work. The
[0.10 validation](VALIDATION_v0.10.md) remains historical; its measurements and
screenshots are not relabeled as 0.11 observations.

## Automated gates and reproduction

| Crate | Distinct passing tests |
|---|---:|
| `terra-bevy` | 99 |
| `terra-cli` | 4 |
| `terra-core` | 33 |
| `terra-demo` | 100 |
| `terra-history` | 13 |
| `terra-net` | 117 |
| `terra-persistence` | 35 |
| `terra-sandbox` | 266 |
| `terra-stream` | 33 |
| **Total** | **700** |

[Distinct test identities](verification/v0.11/distinct-tests.json) use crate,
harness and case name. Cases repeated across Bevy feature configurations or the
separate career measurement count once. Empty harnesses, examples, the asset CLI,
storage probe and native interactions add no Rust test cases.

Reproduce the sequence from the repository root with Python 3.11+, the pinned
Rust toolchain, native development dependencies and permission to bind localhost:

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir target/verification/local-0.11
```

The evidence directory must be new or empty; omit `--report-dir` for a timestamped
destination. `--offline` requires dependencies already in the Cargo cache; omit
it when fetching them on a first installation. See the [README](../README.md)
for native prerequisites. This recorded run used Rust **1.99.1**, Bevy **0.20.0**,
Linux x86_64 and three Cargo build jobs.

The [verifier](../scripts/verify.py), also exposed through
[verify.sh](../scripts/verify.sh) and [CI](../.github/workflows/ci.yml), runs format
validation, the headless workspace, the exact six-session career test, complete
Bevy headless and renderer suites, native demo tests and check, both headless
examples, bundled asset acceptance, recovery and the native build. The procedural
settlement case ran inside the fresh complete Bevy suite: no earlier standalone
result was imported or skipped. TCP/TLS checks use real localhost sockets.
Wall-clock durations in the logs include this shared build environment and are
not frame-time or capacity measurements.

### Evidence capture and failure handling

The [first 0.11 attempt](verification/v0.11-incomplete-capture/README.md) correctly
failed because a zero-exit Bevy command had an incomplete retained log. Its
partial results are excluded from this release count. Every gate then ran again.

The final runner captures subprocess output in a private temporary file, parses
and hashes those exact bytes, and publishes complete logs atomically. Finalization
checks published log hashes and the source manifest. Selected test gates must
identify their executed cases; the career gate must run the exact named case and
emit one valid measurement. Cargo's compiler-artifact message identifies the
native executable, including builds outside the default target directory.

[Independent capture checks](verification/v0.11/verifier-capture-review.log)
exercised real SIGINT, child termination/reaping and retained stdout/stderr while
the visible log was replaced 20 times. Atomic JSON checks observed 376 complete
reads during 40 writes and eight replacements. Another check preserved an actual
exit code of three and detected a replaced published log against the captured
hash, finalizing failure. These checks assess the runner; they add no Rust tests
and do not substitute for the fresh gates.

## Source and executable provenance

The [source manifest](verification/v0.11/source-hashes.json) records SHA-256 for
**313** Rust, Cargo/toolchain, configuration, data, asset, fixture, verification
and CI inputs. The final runner confirms all remained unchanged and all retained
command logs match their captured hashes. Documentation and evidence are written
after that compiled-input freeze.

The [baseline comparison](verification/v0.11/baseline-comparison.json) identifies
changed and added inputs against the delivered 0.10 source archive. The original
wildlife GLBs, manifest and generator, connected-heightmap example, embedded
source/TLS fixtures and archived review/validation documents remain unchanged.
This is a byte comparison, not a claim that old screenshots validate the new
renderer.

The native build's exact executable SHA-256 is:

```text
a1e3391a81327c2a05b9e31bd442d8e9cb27b0b99cd09b6bc1f8e84cc5f15107
```

Its Cargo command, executable path and hash are in the gate report. Building this
artifact establishes build provenance; actual play/save/resume confirmation is
recorded separately below.

## Correctness covered by the fresh run

### Guidance, markets and household exchanges

Guidance tests cover available bread, berries and cooked-fish paths, finished
goods before unavailable processing, permissions, real costs and continuation
after completed work. Opening guidance still does not consume food, issue a
vehicle exit or award progress automatically.

Market opening retains the named seller. The native UI retains the displayed
item, quantity, price, player and restore context. `TradeQuoted` rechecks the
authoritative total, three-dimensional reach, stock, payer funds and receiving
limits before changing money or goods. Stale prices reject atomically, including
after enqueue and over TCP; replay remains rejected. Legacy `Buy`/`Sell` retain
their current-price API behavior. Depot and roadside actions also retain their
displayed quotes. Roadside selection uses an eligible Garage and can offer a
needed repair when no fuel is available. Household purchases skip unusable local
sellers while preserving finite stocks, wallet/till limits and conservation.
See [GUIDANCE.md](GUIDANCE.md) and [COMPANIES.md](COMPANIES.md).

### Successful restore and physical vehicle admission

JSON and regional restore tests queue an otherwise valid quoted trade and control
action behind Load, with a different saved principal and matching shop/price.
Successful replacement retires the old command queue. Native schedule tests use
33 actions in each UI/control queue and verify one-time retirement of backlogs,
interaction, drag, order and placement state. Startup and ordinary revisions keep
input. Failed JSON restore preserves the current principal's subsequent actions;
regional failures retain their existing pause/recovery policy. Host sequence and
reconnect persistence are unchanged.

Dismount now checks a supported avatar footprint and complete nearby corridor
before changing possession. Garage purchases/rentals require a supported,
unoccupied model-sized bay with walking clearance; rejection preserves funds,
fuel, bond and identifiers. Native and host adapters use authoritative source
support. Traffic admission uses the projected outgoing orientation. Existing
bounded handling, terrain, body-contact, physical-delivery and conservation
assertions remain. An occupied exit can legitimately reject; these tests do not
establish unrestricted navigation. See [VEHICLE_DYNAMICS.md](VEHICLE_DYNAMICS.md)
and [INTEGRATION.md](INTEGRATION.md).

### Content admission, animation and connected sources

The [asset acceptance report](verification/v0.11/asset-pack.json) validates the
original five-model wildlife pack. Preflight decodes every embedded PNG rather
than trusting its header, bounds dimensions/pixels and decoder allocations, and
rejects undeclared extension payloads while retaining opaque `extras` metadata.
Both declared and instantiated scene geometry count toward per-model admission,
including repeated mesh nodes. The validator exposes both counts. The original
pack has no textures; importer regressions exercise valid and corrupt embedded PNGs.

Animation checks preserve full virtual-time advancement across slow render frames,
normalize applied STEP/final rotation keys, and retain speed-driven clips,
independent binding policies, pause/blend behavior, visibility and procedural
fallback. These are presentation changes; saved authority and character appearance
remain unchanged. Per-model limits and bounded decoding are not aggregate scene
or GPU-memory budgets. See [ASSET_PACKS.md](ASSET_PACKS.md).

Prepared-source tests retain seam, content-identity and exact union-coverage
checks. Default focus now finds jointly covered tiles when individual child
centers are unusable, including at the antimeridian. The bounded search uses exact
coverage and leaves explicit/saved focus choices intact. The unchanged synthetic
regional example does not establish global geographic coverage or source accuracy.
See [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md).

## Fresh career and recovery measurements

The [career report](verification/v0.11/career-report.json) comes from the exact
acceptance test executed in this run:

| Fixture observation | Result |
|---|---|
| Sessions / physical deliveries | 6 / 24: 12 transfers and 12 warehouse sales |
| Save/resume checkpoints | 7, including loaded cold restore and payroll scarcity |
| Simulated physical time | 2,967.45 seconds |
| Water sold / customer purchases | 96 / 96 |
| Goods revenue / payroll | 28.80 / 15.49 credits |
| Paid recovery service | 18.00 credits: 15.00 callout + 3.00 for two litres of fuel |
| Operating cash change excluding owner transfers | **-469 minor units (-4.69 credits)** |
| Conserved money / water / bread, including consumption | 13,900.00 credits / 224 / 40 |
| Exhausted source | WaitingGoods; no repeated idle income or payroll |

Revenue less payroll and recovery service reconciles the operating result:
**28.80 − 15.49 − 18.00 = −4.69 credits**. This is a finite deterministic scenario,
including paid fuel recovery and a cancelled/retried transfer. Its operating loss is a balance finding; it does not
establish a profitable long career or representative multiplayer prices.

The fresh [recovery report](verification/v0.11/recovery.json) records
**1,027 nonempty regions**, **6,144 resources** and **47 living bodies**, including **1,023 dormant regions**. Backup/recovery preserved exact full state, private metadata,
generation and dormant records. Corrupt source data produced no accepted output,
and the verified backup's bytes remained unchanged. Timings and payload size in
that report are machine/workload observations.

The storage probe has **zero clients, no renderer and no imported terrain**.
It does not establish active-population throughput or player capacity. Full
validation, region/payload caps, a single authority and host-specific private
metadata validation remain. Copy retry deadlines do not preempt filesystem I/O
or full validation. See [OPERATIONS.md](OPERATIONS.md) and [SCALING.md](SCALING.md).

## Native experience and confirmation

The [native report](native-v11-verification.json) records **18 current captures**
and two **exit-zero sessions** of the exact executable above, with commands,
logs and telemetry. Mira Vale resumed Riverbend Commons from a copy of the
0.10 acceptance world; the original remained unchanged. Keyboard/mouse review
used Linux X11/Xvfb, Mesa llvmpipe software Vulkan, **1440×900 then 1100×700** and
**150% text**, including [visible keyboard focus](screenshots/v11-market-keyboard-focus.png).

The displayed purchase of **five water for 4.10 credits**, then sale of **one for
0.30**, changed the wallet **205.68 → 201.58 → 201.88 credits** and left **four
water**. Committed snapshots confirmed combined money and water conservation.
Ordinary walking reached the market and boarding range. Normal boarding, a
**short straight throttle/brake drive** and a
[supported exit](screenshots/v11-safe-exit.png) left the pickup parked with zero
motion; saved engine speed was **800 RPM** after idle ticks.

An independent [source and geometry review](verification/v0.11/native-exit-geometry.json)
confirmed all nine avatar support samples, the saved 0.025 m terrain offset,
approximately 0.364 m of lateral footprint clearance and no overlaps against
106 other resident bodies. Both headings were zero in this native case; oblique
placement remains covered by the automated geometry cases.

F5/F9, keyboard-revealed [Save and quit](screenshots/v11-save-quit-focused.png)
and [reopening](screenshots/v11-resumed-world.png) preserved the checked account,
inventory, character, avatar/vehicle pose and vehicle record, progression,
territory and source identity exactly. Checkpoints were **209** at first clean
exit, **227** after reopening and **230** at second clean exit. The **50 XP**,
completed contract and continuing **Claim a housing plot** goal remained.

Both sessions reported **five models, 64 ready instances**, no loading, fallback,
rejected or failed instances, and no world, simulation or storage error. World
time, needs and NPCs advance during play; whole-state equality is not asserted.
Character/world creation was not repeated. Other corrected branches have fresh
automated evidence, not claimed native interactions. Earlier captures
retain their original release/development labels.

## Remaining product boundaries

The full suites retain construction, farming, ownership, survival, ecology,
transport, company commitments, history/source identity and regional/network
foundations. Matching protocol **8** peers must still use compatible executable
and source versions; the protocol number alone certifies no mixed-version pair.

Dense saved traffic, long-career balance, whole-scene graphics budgets, production
art/audio, broad source distribution and representative live-client/actor soaks
remain separate acceptance work. No target-GPU performance, Windows/macOS,
screen-reader, localization or physical controller/wheel certification follows
from these gates. The historical occupied-exit diagnostic is not claimed solved
without a fresh replay. The [gap register](REVIEW_GAPS_v0.11.md) keeps those limits
distinct from the verified corrections.
