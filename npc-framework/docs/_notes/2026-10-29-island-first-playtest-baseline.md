# First-playtest baseline — 2026-10-29

Execution starts at `a1b65ba65d470bf37d2fcee8edc1998e178ce5a6` on
`codex/island-first-playtest`. The supplied plan was untracked. Existing Terra
changes are outside scope and preserved. No reset, merge, push or PR is authorized.

Acceptance is the seven-part contract in
`../superpowers/plans/2026-10-29-island-village-first-playtest.md`: ten-minute cold
setup; first sale within ten real minutes and needs cycle within one normal day;
observable play; exact-session continuation; persistence and separate permanent
death recovery; sustainable production/currency and measured 20 TPS; reproducible
evidence and an honest readiness verdict. Targets are not baseline achievements.

## Reproduced path and gaps

An owned release `npcctl serve` created disposable Island Village worlds with
isolated actors/worlds directories, `--port 0 --paused`. Registration and
`enter_world` succeeded. `take_control` queued while tick remained zero. The
operator called `npc/resume`; the agent subsequently obtained control and a normal
observation, including affordances. No agent stepped the host to obtain control.
Owned hosts were stopped; the pre-existing npcctl process was not touched.
Explorer mode rejects `--seed`; seeded fixtures use scenario/test construction.
`agent/affordances` is not a method; affordances come from `agent/observe`.

Reproduced/source-backed blockers to address:

- Merchant demand was absent with an empty pack, obscuring first income.
- Buy/sell refusals collapsed different failures into generic advice.
- Insufficient buyer funds exposed the buyer's exact private wallet in a refusal.
- The native header duplicated hardcoded prices and lacked compact activity and
  responsibility feedback.

Missing evidence: unaided provider play/continuation, registered complete economic
cycle, a fresh integrated native capture batch, day/season reliability summaries.
Existing unit tests, fake continuation and historical captures do not fill these.

## Ownership and frozen additive contract

Wave 1 A owns village agent information, explorer prompt/workspace rendering and
new agent tests. B owns village AI/economy/catalog and economy tests. C owns village
view/world rendering and focused view tests. Coordinator owns shared exports,
commands/host/menu integration, all Cargo and processes. Wave 2 D owns the two
scheduler modules; E owns new registered-client tests/tooling and run sheet.
Read-only independent review follows integrated rehearsal/native/live evidence.
At most three workers plus coordinator; no nested teams or competing builders.

The existing village observation gains activity (`kind`, busy flag, recovery) and
bounded perceived nearby merchant offers: actor-perspective buy/sell, kind,
integer unit price, stock/demand, availability and concrete validation reason.
Catalog prices, traded kinds and bed cost/duration are the single source. Existing
fields remain compatible. Hidden locations, wallets and inventories stay private;
only own state and currently perceived/reachable market data are exposed.

## Fixtures and prerequisites

Baseline command shape, inside `npc-framework/`:

```powershell
target/release/npcctl.exe serve --explorer-home TARGET/agents --actors-dir TARGET/actors --worlds-dir TARGET/worlds --agent Baseline --world Island --world-preset island_village --port 0 --paused
```

`TARGET` is a new ignored disposable directory, never a private existing home.
Credentials are read privately from its generated metadata, never copied into
evidence. The operator controls pause/time; registered actor decisions use normal
agent tools. Primary native viewport is 1280×720 logical, with actual desktop,
480-logical width and representative DPI checks required separately.

The user selected installed Codex CLI and explicitly authorized a **new disposable
conversation**, existing included usage only, maximum ten minutes or two turns
(initial plus one continuation), whichever comes first. Installed help reports
Codex CLI 0.145.0 and supports `exec resume UUID -`. No model/permissions/global
configuration changes or private-session inference are authorized. Login is a
prerequisite; usage mode and authentication must be checked without printing secrets.

Final evidence/results and GO/NO-GO decision will be recorded in the execution
note after tests, captures, smoke and independent review. Full `cargo xtask verify`
must report JSON `ok:true`; quick checks are insufficient.
