# Terra World 0.12 validation record

Validation date: **2026-10-28**. Source release **0.12.0**, wire protocol **9**, TLS
ALPN **terra-world/9**, domain save version **1** and regional database schema
**1**. All **20 fresh automated gates passed**, with **747 distinct passing Rust
tests**, zero failures and zero ignored tests. All **327 source inputs** remained
unchanged, and every published command log and accepted JSON report matched its
captured hash. Final native acceptance also passed in three clean sessions of the
exact gate-built executable, with 14 retained captures.

The [product review](PRODUCT_REVIEW.md) explains the player/developer outcome;
the [gap register](REVIEW_GAPS_v0.12.md) traces the changes to source and tests.
The [0.11 validation record](VALIDATION_v0.11.md) remains unchanged historical
evidence. Its totals, timings and screenshots do not validate this release.

## Executed automated evidence

The [final gate record](verification/v0.12-final/gates.json) covers the complete
sequence from **16:11:55 to 16:22:51 UTC**. The
[source manifest](verification/v0.12-final/source-hashes.json) and
[distinct test identities](verification/v0.12-final/distinct-tests.json) establish
which files and cases were accepted. The 803 passing executions below deduplicate
to 747 `(crate, harness, case name)` identities:

| Crate | Distinct passing tests |
| --- | ---: |
| `terra-bevy` | 107 |
| `terra-cli` | 4 |
| `terra-core` | 33 |
| `terra-demo` | 102 |
| `terra-history` | 13 |
| `terra-net` | 119 |
| `terra-persistence` | 35 |
| `terra-sandbox` | 290 |
| `terra-stream` | 44 |
| **Total** | **747** |

The Cargo-reported executable is `/tmp/terra-build/target/debug/terra-demo`.
Its SHA-256 is:

```text
4b1a7ae522a51e321dad33fbfb7e3b83f098c3dd3d1b025825f39034ada2de95
```

**Native confirmation passed:** [native-v12-verification.json](native-v12-verification.json)
identifies this exact executable, the actual environment, 14 captures and three
clean save/quit sessions, including exact company-state continuation after reopen.
All 327 source inputs remained unchanged. Pre-polish candidate observations in the
product review remain separately identified development evidence.

## Reproduce the complete automated sequence

Run from the repository root with Python 3.11+, the pinned Rust toolchain, native
development dependencies and permission to bind localhost:

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir target/verification/local-0.12
```

The evidence directory must be new or empty and is claimed exclusively. Omitting
`--report-dir` uses a timestamped directory under the repository's
`target/verification`, independently of `CARGO_TARGET_DIR`. `--offline` requires
cached dependencies; omit it for a first dependency download. The pinned compiler
is Rust **1.99.1**, and the workspace uses Bevy **0.20.0**. See the
[README](../README.md) for native prerequisites. This run used **Linux x86_64**,
**Python 3.12.14**, **rustc 1.99.1 (58cf8f9c3 2026-10-13)** and **three Cargo build
jobs**, with cached dependencies and no `CARGO_BUILD_TARGET` override.

The [verifier](../scripts/verify.py), [shell wrapper](../scripts/verify.sh) and
[CI workflow](../.github/workflows/ci.yml) share this sequence. CI retains its
evidence directory even on failure. Cargo test/run/build/check gates use
`--locked`, plus `--offline` when requested. No gate accepts a prior standalone
result or skips the procedural settlement tests. The runner opens no native
window. Real TCP/TLS tests and the session workload use localhost sockets.

### Twenty gates

The exact executable command, exit status, duration, log hash and test execution
counts are recorded for each completed gate in `gates.json`. Every row passed.
Durations include compilation and process execution in this shared environment;
they are not renderer frame times or service-level targets.

| Gate | Command or acceptance scope | Seconds | Passing executions |
| --- | --- | ---: | ---: |
| `format` | `cargo fmt --all -- --check` | 1.512 | 0 |
| `headless` | `cargo test --workspace --exclude terra-demo --exclude terra-bevy` | 86.312 | 537 |
| `career-report` | Exact contributed-stock case in `terra-sandbox` / `company_depots`, with `--exact --nocapture` | 3.120 | 1 |
| `purchased-career-report` | Exact purchased-stock case in the same harness, with `--exact --nocapture` | 3.025 | 1 |
| `traffic-recovery` | `cargo test -p terra-sandbox --test traffic_recovery -- --nocapture` | 0.791 | 3 |
| `bevy-headless` | `cargo test -p terra-bevy --no-default-features` | 269.718 | 73 |
| `bevy-render` | `cargo test -p terra-bevy --lib` | 65.087 | 85 |
| `demo` | `cargo test -p terra-demo` | 0.922 | 102 |
| `demo-check` | `cargo check -p terra-demo` | 13.406 | 0 |
| `headless-example` | Run the `terra-bevy` `headless` example without default features | 0.555 | 0 |
| `custom-provider-example` | Run the `terra-bevy` `custom_provider` example without default features | 0.631 | 0 |
| `asset-pack` | Run `terra-assets validate crates/terra-bevy/assets/wildlife-pack/pack.json` without default features | 0.268 | 0 |
| `source-inspect` | Inspect the connected heightmap manifest with `terra-sources` and `--require-complete` | 9.772 | 0 |
| `source-export` | Export that inspected source to a new temporary portable directory | 0.171 | 0 |
| `source-install` | Install the exported package to a second new directory, requiring its expected package/source identities | 0.174 | 0 |
| `source-verify` | Verify the installed package against those same identities and complete coverage | 0.148 | 0 |
| `recovery-probe` | Run the `terra-persistence` `recovery_probe` example with its JSON output path | 10.573 | 0 |
| `session-probe-checks` | `cargo test -p terra-net --example regional_session_load` | 0.157 | 1 |
| `regional-sessions` | Run the combined prepared-terrain/TLS/cold-storage recovery workload below | 132.362 | 0 |
| `native-build` | `cargo build -p terra-demo --message-format=json` | 57.321 | 0 |

The two career filters are deliberately exact. Reproduce them independently with:

```sh
cargo test -p terra-sandbox --test company_depots \
  six_session_warehouse_career_conserves_finite_goods_cash_and_recovers_scarcity \
  --locked --offline -- --exact --nocapture
cargo test -p terra-sandbox --test company_depots \
  purchased_stock_career_accounts_for_acquisition_setup_and_recovery_without_inventing_profit \
  --locked --offline -- --exact --nocapture
```

The final combined workload is configured as follows; the release runner places
the output in its evidence directory:

```sh
cargo run -p terra-net --example regional_session_load --locked --offline -- \
  --clients 16 --cycles 8 --ticks 400 --cold-regions 1024 --transport tls \
  --heightmap crates/terra-stream/data/heightmap-connected/manifest.json \
  --output reports/regional-sessions.json
```

Running a filtered check is useful during development, but does not replace the
fresh complete sequence or establish the final release totals.

## Evidence structure and integrity

| Artifact under `verification/v0.12-final/` | Meaning |
| --- | --- |
| `gates.json` | Run status/environment, current active gate while running, completed commands, durations, logs, integrity results, distinct totals and native artifact identity |
| `source-hashes.json` | SHA-256 of the enumerated Cargo/toolchain, Rust, configuration, embedded data/assets/fixtures, verification and CI inputs |
| `distinct-tests.json` | Passing `(crate, harness, case name)` identities, deduplicated across repeated executions |
| `career-report.json`, `purchased-career-report.json` | Accepted paired physical-career measurements, including persisted cash accounting |
| `traffic-recovery.json` | Both accepted historical queue replay measurements |
| `asset-pack.json` | Accepted nonempty model-pack validation report |
| `source-inspection.json`, `source-export.json`, `source-install.json`, `source-verify.json` | Exact coverage, source inspection and portable package continuity |
| `recovery.json` | Accepted storage-only recovery measurement |
| `regional-sessions.json` | Accepted real-session/prepared-terrain/recovery measurement |
| `<gate>.log` | Captured stdout and stderr for that command, including unsuccessful or interrupted attempts |

These files are the accepted outputs of the final run. A report is accepted only
after its command and semantic checks succeed. A failed run retains its status
and available evidence; it cannot supply successful release totals.

The runner captures output in a private temporary file, parses and hashes those
exact bytes, then atomically publishes the log. JSON snapshots are also published
atomically. Interruption stops the owned process tree before publishing partial
output and cleaning temporary source packages. A current `active_gate` records
an unfinished command; completion is never inferred from a partial log.

Source hashing covers the enumerated files under `crates`, `configs`, `scripts`,
`.github/workflows` and `.cargo`, plus the workspace manifest, lockfile and
toolchain pin. Build/cache directories and Markdown documentation are excluded.
Finalization requires the same input inventory and hashes, unchanged completed
logs, and unchanged accepted JSON artifacts. Documentation can be finalized after
the code/data freeze without relabeling a different executable as tested.

The [baseline comparison](verification/v0.12-final/baseline-comparison.json)
records 14 added and 38 changed inputs against the delivered 0.11 manifest,
with 275 unchanged and none removed. Original wildlife model/authoring bytes,
connected source data and fixture inputs remain identical. The archived review
and validation files and all 248 checked historical evidence files remain exact.

Cargo's `compiler-artifact` JSON must identify exactly one native demo executable;
its actual path is used for hashing, including an external target directory.
Native acceptance must identify that same SHA-256. A guessed `target/debug` path
or a previous binary is insufficient provenance.

### Earlier incomplete attempts

Two failed attempts are retained separately and contribute no successful release
counts. The [disk-space attempt](verification/v0.12-incomplete-disk-space/gates.json)
stopped during the headless build because Cargo could not write its incremental
query cache: the filesystem reported no space left. The
[household-fixture attempt](verification/v0.12-incomplete-household-fixture/gates.json)
then stopped on two tests whose setup subtracted company cash and created earned
wages without recording the known payroll payment.

The correction records that synthetic five-credit payroll explicitly and uses
authoritative withdrawal/deposit setup for two depot capacity cases. Production
accounting validation was not relaxed. The complete 290-test sandbox crate passed
afterward, and this final directory contains a fresh execution of every gate,
including both formerly failing household cases. No earlier partial result was
imported into the 747-test total.

### Test identity and fail-fast rules

A nonzero command exit, failed test or ignored test stops the sequence. Each
selected test gate must have a nonzero number of passing cases, and every passing
execution must resolve to a known package/harness/case from Cargo metadata and
captured test output. The two career gates must execute only their exact named
case and emit exactly one corresponding measurement.

Repeated executions across feature configurations and the separate measurement
gates count once in the distinct Rust total. Per-gate execution counts remain
visible separately. Empty harnesses, CLI invocations, probes, examples without
tests and manual native interactions do not add distinct cases. Test totals do
not measure gameplay duration, renderer frame time or user capacity.

## Semantic acceptance criteria

### Reconciled company careers

Each career must report six sessions, 24 deliveries and seven in-career restores.
Its persisted accounting must have `since_registration` coverage, reconcile to
actual company cash, retain exactly 64 recent entries and record more than 64
lifetime monetary entries. Operating plus capital flows must equal the result
before financing; subtracting known personal costs must produce the reported
owner-cost result. Opening cash, these flows and owner financing must reconstruct
the final wallet exactly.

The purchased career must record a positive acquisition payment equal to its
`inventory_purchases` total, with no unpriced contributed stock. The contributed
career must leave acquisition cost unknown and disclose 96 unpriced input units.
The [underlying tests](../crates/terra-sandbox/tests/company_depots.rs) additionally
assert physical transfer/arrival, finite source and buyer commitments, exact
money/goods conservation, funded recovery, scarcity, idle behavior, and complete
and regional save continuity. The report parser verifies the resulting cash
relationships rather than accepting a successful process alone.

### Historical traffic recovery

Exactly two measurements must identify `rear-trap` and `occupied-exit`. Each must
return the original 12-wood load within the unchanged 120-second replay window,
conserve 770 item units and 150,000 minor credits, and establish exact every-step
conservation, source return and save/resume equivalence. The
[replay tests](../crates/terra-sandbox/tests/traffic_recovery.rs) retain the original
saved fixtures and verify the actual delivery event and empty courier at transfer.
The unsupported-source regression must continue holding rather than bypassing
terrain admission.

### Content acceptance and source continuity

The asset CLI must emit an accepted report with models. Importer and renderer
regressions separately cover malformed content, animation/geometry/image bounds,
aggregate reservations, load failure and retirement/fallback behavior; CLI
acceptance alone does not establish rendered performance.

The connected source must report schema 1, a nonempty file inventory and exactly
81 requested/fully covered tiles with no missing tiles. Export, install and
verify must return inspection content identical to the original. Package identity
must remain identical, and installation/verification receive both expected
package and source digests. Source-distribution regressions cover bounded declarations,
portable member names, links/traversal, existing destinations, corrupt bytes and
incomplete coverage. The [source workflow](SOURCE_BUNDLES.md) defines the
quiescent-directory and provenance contract.

### Storage and real-session recovery

The storage-only probe must identify this release, contain at least 1,024
nonempty regions and pass every emitted verification flag. It has no live clients
or renderer and remains separate from the session workload.

The combined report must describe 16 real clients, eight churn cycles, 400 active
steps per cycle, checked initial crew placement, prepared-source ingestion, 1,024
cold overlay regions and 4,096 cold resource records. The full run has 4,840 fixed
steps, including half-disconnected phases and source interruption/restoration.
Acceptance requires 64 exact offline-owner checks, 64 resumed-owner checks, 128
restarted-owner checks and 16 authenticated recovered-owner checks. There must be
128 durable command acknowledgement samples and actual ping samples.

Cargo, craft inventory, durable sequences and cold records must remain exact;
every real session must receive snapshots. The failed SQLite write must leave
authority/tick/generation unchanged and retry exactly. The separate
`session-probe-checks` gate prevents an expected writer lock from masking an
additional error or authority work. Source removal must stop movement and its
restoration must resume it. Backup recovery must reproduce full state, private
metadata, generation and unchanged source bytes, then admit the same authenticated
principals. Prepared source identity must equal the inspected/distributed source;
the backup generation must match its recovery receipt. Recovery/session sidecars
must equal their captured stdout reports before publication.

## Measured results from the final run

The following measurements come from the accepted reports in `v0.12-final`.
They repeat the complete scenarios against the frozen source; development logs
and previous release measurements are not substituted for these results.

**Accounting:** both careers completed six sessions, 24 physical deliveries and
seven in-career restores over **2,967.45 physical seconds**, followed by an exact
regional accounting roundtrip. Each sold 96 water and conserved **13,900.00
credits, 224 water and 40 bread**, including household consumption. The recorded
cash comparison in the [contributed](verification/v0.12-final/career-report.json)
and [purchased](verification/v0.12-final/purchased-career-report.json) reports is:

| Cash observation, in credits | Contributed harvest | Purchased stock |
| --- | ---: | ---: |
| Stock acquisition | Unknown; 96 unpriced units | 76.80 |
| Goods receipts | 28.80 | 28.80 |
| Operating result since registration | −14.69 | −91.49 |
| Depot access capital payments | 50.00 | 50.00 |
| Result before owner financing | −64.69 | −141.49 |
| Result after known personally paid costs | −84.69 | −161.49 |
| Owner deposits / withdrawals | 205.00 / 126.38 | 205.00 / 49.58 |
| Final company cash | 13.93 | 13.93 |

The earlier delivery-window field remains −4.69 credits because setup and
acquisition precede that window. Both careers pay 15.49 in wages, 3.00 for two
litres of recovery fuel and one deliberately induced 15.00 callout; neither buys
a repair. Recruitment, two bays and registration add 80.00 in known setup costs.
The purchased input costs 76.80 against 28.80 sale receipts. Prices, wages and
deadlines were not changed to make this losing retail-to-wholesale strategy
profitable. [Company accounting](COMPANIES.md) explains the periods and categories.
The contributed career retains 1,570 monetary events and the purchased career
1,571; each exposes its latest 64 entries while reconciling exact lifetime totals.

[Accounting regressions](../crates/terra-sandbox/tests/company_accounting.rs)
cover imported opening balances, all applicable cash paths, recent-history
retention with exact lifetime totals, owner-cost attribution, rental bonds,
atomic compound transactions, malformed history and counter exhaustion. A full
ledger cannot silently reset or accept unrecorded payroll/withdrawals; pickup-local
escrow refunds remain possible without a company cash posting. Fractional-payroll
checks retain equivalence after removing unnecessary journal clones on ticks with
no payment due.

**Walking and traffic:** the original rear-trap and occupied-exit saves physically
returned their loads after **34.0** and **49.5 seconds** in the
[final replay report](verification/v0.12-final/traffic-recovery.json).
Supported pedestrian shortcuts preserve ordinary speed and movement authority;
unknown/blocked corridors keep the original route. Native and host adapters share
the domain planner. These cases establish the specific corrections, not universal
deadlock resolution.

**Presentation and source handoff:** the complete renderer suite exercised retained
outgoing resources, lazy-load failure, priority replacement, lowered limits and
scene-to-procedural fallback. Source export/verify/install preserved the authored
two-bundle fixture's seven files, **115,316 bytes** and 81 covered level-15 tiles.
Independent review closed platform-invalid/reserved filename aliases and forged
aggregate declarations that previously could defer rejection until excessive
reads. The [asset report](verification/v0.12-final/asset-pack.json) accepted all
five wildlife models with pack identity
`sha256:406c12494416b472ca5bee80a809e0ef30a45e382240cab01faa9081cd838d6d`.
The [inspection](verification/v0.12-final/source-inspection.json),
[export](verification/v0.12-final/source-export.json),
[install](verification/v0.12-final/source-install.json) and
[verification](verification/v0.12-final/source-verify.json) reports preserve exact
inspection content and these two distinct identities:

- Source: `heightmap-set-v1:1100816335f7077227658413231028310009b25af7c8f5797e578b088cd151a3`.
- Portable package: `blake3:3b32bc568e5a06f630d0eedf77ada455d1806985250cafc349becb8ce7cf3c76`.

**Operations and interface:** the workload now checks the exact injected storage
failure separately from unrelated domain errors. Fleet adds a compact six-section
chooser, visible selection and a Finances report with imported/unpriced scope and
recent entries. The 102-test demo gate passed; final native review confirmed the
polished empty state, keyboard action, compact navigation and finance result at
1100 × 700 with 150% text.

### Current operational measurements

The [combined session report](verification/v0.12-final/regional-sessions.json)
completed **4,840 fixed steps in 131.962 wall seconds** with **16 actual local TLS
clients, 182 living bodies (80 owned), 5,248 resource nodes and 25 resident prepared
source chunks**. Its 1,024 cold regions contain 4,096 of those resource records.
Every required conservation, source interruption, failed-write/retry, private-state
backup and authenticated-continuation check passed.

| Recorded distribution | Samples | p95 | p99 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Advancing host update | 4,840 | 46.691 ms | 57.483 ms | 91.151 ms |
| Durable command acknowledgement | 128 | 108.879 ms | 127.518 ms | 128.297 ms |
| Local TLS ping roundtrip | 6,464 | 58.887 ms | 73.111 ms | 137.605 ms |

Storage paused authority on 2,119 update calls. The verified recovery copied
generation 165, 1,051 nonempty regions and 1,686,295 checkpoint payload bytes;
authenticated continuation ended at generation 169. Process RSS, including the
host and local clients, grew from **36.04 MiB before measured admission to
100.27 MiB after recovery/resume**. These finite samples do not establish a memory
plateau or a guaranteed tick deadline. [Scaling](SCALING.md#012-combined-prepared-terrain-and-cold-world-acceptance)
retains the detailed distributions, actual socket totals and workload exclusions.

The separate [storage-only report](verification/v0.12-final/recovery.json) verified
1,027 nonempty regions, 6,144 resources and 47 bodies, with zero connected clients,
no renderer and no terrain import. Its 1,708,853-byte generation-two snapshot
backed up in **51.606 ms**, recovered in **48.574 ms** and restored a working set
with 1,023 dormant regions in **28.441 ms**. Exact full state, private metadata,
generation, dormant records and source bytes were preserved; corruption rejected
without an output. These timings are not measurements of populated-world throughput.

## Native review and qualification boundaries

The [final native record](native-v12-verification.json) and its
[observations](verification/v0.12-final/v12-native-observations.json) confirm three
distinct processes using the exact executable above, each exiting cleanly with
code zero. The review used Linux x86_64, X11/Xvfb and software Vulkan through Mesa
llvmpipe, keyboard/mouse input, 1440 × 900 and 1100 × 700 windows, and 150% text.
The 14 retained captures document the inspected flows; these are interface and
continuation checks, not physical GPU performance measurements.

The [empty-state keyboard capture](screenshots/v12-empty-confirm-keyboard.png)
shows a concise prompt and immediately reachable **Manage company** action with
visible focus. Keyboard activation opened Company. All six Fleet sections were
reachable, selected sections were highlighted, section changes reset their scroll
and resizing retained it. The [body-focused RTS view](screenshots/v12-confirm-rts-company.png)
kept the character in view after the possession-camera handoff.

The [small-window Finances result](screenshots/v12-confirm-finances-small-result.png)
confirmed **40.00 company credits**, **131.88 personal credits**, **−10.00 before
owner funding** and **−30.00 after known owner-paid costs**. Its three-entry
journal reconciled the 20.00 personal registration payment, 50.00 owner deposit
and 10.00 recruitment payment. The final review inspected this persisted paused
company state; the original transaction clicks belong to the separately labeled
pre-polish candidate session. Activities retained 50 community XP, one completed
supply contract and the housing goal; one founded settlement also persisted.

The empty-world session saved and quit at generation **25**. The company session
saved and quit at **320**, reopened to checkpoint **341**, and saved and quit again
at **346**. All **12 named continuity checks** in the record passed, covering
company/accounting, owner identity/account, player inventory, avatar/profile,
progression, territories, owned vehicle/body, world identity, metadata identity
and politics. The [reopened Finances view](screenshots/v12-resume-finances.png)
retained the same amounts. These are exact comparisons of the named state, not an
assertion that every elapsed world field stayed unchanged. Both worlds used
isolated copies of the fictional Riverbend Commons/Mira Vale acceptance world;
the original 0.11 database remained unchanged.

The reviewed scene had five models, **57 ready actors and seven distant
procedural fallbacks**. Resume telemetry ended at **59 ready and five distant
fallbacks** as the distance-dependent selection changed. All sessions reported
zero model, world, simulation or persistence errors. The displayed 9.6 MiB payload
estimate against a 128 MiB budget is not measured GPU allocation. Automated
rejection/overflow and recovery branches remain automated evidence; previous
release captures remain historical.

Cash accounting does not price contributed labor, unknown input costs, preassignment
vehicle capital or depreciation, and does not establish campaign-length balance.
Traffic recovery is bounded by its actual routes, support and replay window.
Imported actor-scene reservations estimate payload, not actual GPU allocation or
frame time; arbitrary game-owned scene overrides remain unmetered. Offline source
distribution preserves the supplied authored region and does not create licensed
worldwide coverage. Cold overlay records do not imply terrain or active residents
at their locations.

Finite local TLS measurements include the server and in-process clients; they do
not certify Internet latency, sustained population capacity or a long-duration
memory plateau. Physical GPUs, controllers/wheels, screen readers and non-Linux
platforms require direct qualification. [Operations](OPERATIONS.md),
[scaling](SCALING.md), [asset packs](ASSET_PACKS.md) and the
[gap register](REVIEW_GAPS_v0.12.md) retain those scope boundaries.
