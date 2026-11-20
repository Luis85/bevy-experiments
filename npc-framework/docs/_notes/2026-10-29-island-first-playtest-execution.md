# Island Village first-playtest execution — 2026-10-29

> Historical evidence, recovered on 2026-11-20 from the leftover branch
> `audit/island-first-playtest-verified` (commits `63417af`, `dc303f7`, `bf48345`).
> It describes the testbed-hosted Island Village tier that ADR 0044 removed:
> `testbed_content/src/village`, `docs/island-village-playtest.md`,
> `docs/ai-village.md` and `scripts/playtest/live_smoke.py` no longer exist.
> Main took `63417af`'s code in `a3d6fdb` but not `dc303f7`'s owned process
> containment or `bf48345`'s CLI identity handoff; see
> [unported agent-supervision work](2026-11-20-unported-agent-supervision.md).

This records the initial implementation pass. The later comprehensive review,
polish and current verification are in
[the followup report](2026-10-29-island-first-playtest-followup.md).

Readiness is **OFFLINE READY / LIVE SETUP BLOCKED**, **not GO**. The final
`cargo xtask verify` returned JSON **`ok:true`**, and independent review found no
remaining source P0/P1. Authenticated same-conversation continuation has not
passed. Native confirmation was interrupted by the operator's Escape stop of
Computer Use; those evidence gaps remain explicit.

## Scope and preservation

Started at `a1b65ba65d470bf37d2fcee8edc1998e178ce5a6`, on
`codex/island-first-playtest`, preserving the supplied untracked execution plan
and unrelated Terra changes. Another user chat subsequently began the ExcaliburJS
world-projection work in the same checkout and ran Cargo. Its files were not
edited, reset or copied into this task's isolated verification checkout.

The task-only changes are now on `codex/island-first-playtest-verified` in the
attached `island-first-playtest` worktree. The original checkout remains intact,
including the task's earlier edits and the other chat's ongoing work. Final
verification uses this worktree's own Cargo output directory. No push, PR, merge,
oracle rebaseline, dependency upgrade or global configuration change occurred.

Wave A added bounded perceived merchant demand/offers, checked actor-perspective
prices/refusals, activity/recovery, island onboarding, and guarded MCP-only Codex
workspace configuration. B added privacy fixes and seeded economy evidence.
C added native activity, need urgency, last-action and responsible-party feedback.
D added read-only scheduler preflight and repair/lock/PATH regressions. E added
registered out-of-process rehearsals and the tester run sheet. The coordinator
integrated shared constants, host-only refusal details, pending-control pause
information, native/provider harnesses and this evidence.

Two privacy defects were reproduced red before fixing: insufficient funds exposed
the merchant's exact wallet; distant sell validation disclosed full stock before
checking reach. Successful trades, integer state, registrations and state encoding
are unchanged. Last-refusal messages live only in host statistics, are bounded,
and are excluded from serialized statistics, snapshots and hashes. Legacy journal
reason codes remain intact. Registered control replies add `paused` so an AI can
distinguish a running host's next-tick claim from an operator pause.

## Observed offline checks

All commands ran inside `npc-framework/` with one coordinator Cargo builder.
These results precede the final isolated gate:

| Command | Observed result |
|---|---|
| `cargo test --locked -p testbed_content --test island_agent_information` | 5 passed |
| `cargo test --locked -p testbed_content --test island_village` | 22 passed after both reproduced privacy fixes |
| `cargo test --locked -p testbed_content --test island_economy_soak -- --nocapture` | 3 tests passed; 6 normal-day runs, 12 seasonal fixtures, 3 depleted-bowl fixtures |
| `cargo test --locked -p npc_agent_io --lib scheduler` | 10 passed |
| `cargo test --locked -p npcctl --bin npcctl scheduler::tests` | 2 passed |
| `cargo test --locked -p npcctl --test island_playtest -- --nocapture` | 2 passed before additive pause-feedback assertions |
| `cargo test --locked -p testbed_content -p testbed_view --lib` | content 88 passed/1 ignored; view 75 passed/1 ignored |
| `cargo test --locked -p testbed_view --lib` after concrete-refusal feedback | 76 passed/1 ignored |
| `cargo build --locked --release -p actor_testbed -p village -p rts_units_demo -p game_menu -p npcctl --bins` | passed in 5m52s for integrated capture batch |

The subsequent confirmation build hit Windows error 5 because the disposable
launcher was still open. Its exact owned process was stopped after verifying its
executable identity. No original/user game or provider process was terminated.

The isolated rehearsal then reproduced a genuine successor-notebook race: a
short operator step could apply `new_life`, and registration could accept the
successor before the next host frame bound its private books. Credential reload
now installs the successor's note binding immediately, preserving old mappings
and files. A deterministic no-host-frame regression passed. The strict registered
rehearsal passed again without sleeps/retries: first sale tick 3003, needs cycle
3669, 150 requests and zero rejections (the running/paused reply check adds a
request). The failure and corrected logs are retained separately under the
isolated worktree's `target/playtest-evidence/`.

The cold registered rehearsal used only returned affordances and perceived or
remembered positions. First sale was tick 3002; the needs cycle finished at tick
3668; 149 requests and zero tool rejections were recorded. These are accelerated
operator-clock fixture ticks, **not real model latency or an observed 1x playthrough**.
Save/stop/resume retained actor/inventory. The separate forced-death fixture proved
saved permanent death, a different successor, revoked old control, fresh private
books and one recovered public Diary page. Both owned hosts stopped.

Seeds 1, 7 and 42 conserved 1800 total currency through a normal day with and
without visitor trade, including redistribution. All nine residents survived.
Bread/water/log buyers were available at all 200-tick samples in these fixtures;
this sampling does not rule out shorter gaps. A carpenter initially above its
bowl target correctly did not overproduce; a separate depletion fixture required
fresh bowls. Free gathering and self-supply remain intentional alternatives.

## Native evidence and limits

Maintained reproduction: `python scripts/playtest/capture_native.py`; prerequisites
are Python 3 standard library and built native release apps. It always creates a
new disposable explorer. Every produced PNG in the baseline and integrated batch
was opened. All fixture state changes used recorded commands; input was BRP
widget/key automation. No physical-input or screen-reader validation is claimed.

Machine: Windows 11, Intel Core Ultra 9 185H; Intel Arc Pro and NVIDIA RTX 1000 Ada
were installed. The desktop capture was 2560×1440 physical, 1280×720 logical at
200% scaling. Captures included 480-logical width and 125% scaling, fog on/off,
paused/unlinked, walking/refusal, notebook, permanent death and successor guidance.
Current KayKit art and UI structure were preserved; no missing textures or new
caption overlap was observed in these captures.

| Sample | Measured unpaused 1x throughput | Profiler frame sample |
|---|---:|---:|
| pre-change native binary | 241 ticks / 12.075 s = 19.958 TPS | 60.142 FPS / 16.627 ms |
| integrated capture binary | 241 ticks / 12.077 s = 19.955 TPS | 59.944 FPS / 16.682 ms |

These are short engineering samples, not a 30–45 minute frame-time soak. Existing
profiler budgets were unchanged. One earlier cold baseline capture timed out and
was retained as a failed attempt. The integrated native harness stopped at diary
pickup because the successor was out of reach; it now requests normal registered
approach/interaction. The confirmation fixes also replace unsupported arrow glyphs
and retain concrete refusal text. Those final native changes were unit-tested but
not recaptured after the operator stopped Computer Use with Escape.

The actual launcher Main → Explore button was inspected, but the Escape stop
interrupted further UI navigation. Do not claim a complete launcher New flow.

## Explicitly authorized provider evidence

The user authorized fresh disposable CLI conversations only, existing included
usage, maximum ten minutes and two turns per smoke. An additional Codex diagnostic
was separately authorized. The user then requested Claude CLI **Haiku / low effort**;
the same caps were retained. Installed help was checked: Codex 0.145.0 and Claude
Code 2.1.281, including `--model haiku --effort low`. Subscription login was checked
without logging credentials. No model/permission/trust/global-setting changes were
made except the explicitly requested per-invocation Haiku/low selection.

| Smoke | Observed outcome |
|---|---|
| Codex, generated project MCP | Original turn exited 0 after 352.416 s; no required game page/control evidence; no continuation linked |
| Codex diagnostic, explicit npc MCP argv | Exited 0 after 32.730 s; register/enter/take-control tool calls observed; no recorded game page; no continuation linked |
| Claude Haiku, `--effort low` | Exited 0 after 50.296 s; register/enter/control/observe/act/notes tools invoked; disposable saved world contains one physical Diary page, proving a recorded game action |

The original harness unnecessarily required a separate notebook entry and stopped
the Claude smoke before linking. It has been corrected to use the recorded physical
Diary action as its criterion. The original failed summary remains intact; no
provider was silently retried and no conversation was recovered from private
history. The next-tick claim wording was also corrected after the Codex diagnostic.
Neither corrected path has been rerun against a provider in this authorization.

**Authenticated same-conversation idle continuation remains BLOCKED / unverified
for both providers.** Haiku's initial valid game action is not a continuation pass.
Custom structured runner arguments preserve explicit MCP or Haiku/low settings;
this does not establish the default provider adapter's authenticated behavior.
All owned smoke host/client roots exited. The original summaries overstated
process-tree verification: they did not independently verify descendants at exit.
A later audit found zero MCP processes matching these disposable hosts' URLs.
No conversation UUID, token, transcript,
Memory or Proposals appears in committed evidence. The longer unaided economic
AI playthrough remains untested.

## Independent review and cleanup correction

The fresh independent reviewer found one P1: root-only Windows termination in
the maintained harness could leave descendants while reporting unconditional
cleanup success. Both maintained harnesses now start only their newly created
processes suspended, assign a noninheritable kill-on-close Windows Job Object,
then resume. Cleanup retains handles for verified job members, terminates only
that job, waits for every handle to signal and checks zero active job processes.
It reports a failed cleanup honestly. An original/user interactive client cannot
be adopted by this helper.

The first fake-process batch caught an asynchronous exit-signaling race. After
hardening handle verification, all three tests passed in 1.673 s, covering timeout,
a normally exited root with a surviving descendant, unrelated sentinel survival,
and injected verification failure. Command:
`python -m unittest discover -s scripts/playtest -p test_owned_process.py -v`.
No provider or game ran in these tests. The reviewer re-read the fixes and passed
the source review with **no remaining source P0/P1**. The final full gate is still
mandatory; fake process tests do not satisfy authenticated continuation.

Two P2 items are explicitly deferred: F7 can overlap the incumbent observer HUD
(the operator stopped visual work; changing that layout would need another native
confirmation), and seasonal fixtures check aggregate production rather than every
stage in each individual season (normal-day, continuous short-year and separate
depleted-bowl tests provide the current stage coverage). Neither limitation is
presented as completed evidence.

## Handoff

Final verification ran with `NEXTEST_TEST_THREADS=4` scoped to the verification
process and restored afterward. This reduces Windows shell contention without
changing a test, assertion, hook, timeout, budget, filter or oracle:

```powershell
$previousThreads = $env:NEXTEST_TEST_THREADS
try {
    $env:NEXTEST_TEST_THREADS = '4'
    cargo xtask verify
} finally {
    if ($null -eq $previousThreads) {
        Remove-Item Env:\NEXTEST_TEST_THREADS -ErrorAction SilentlyContinue
    } else {
        $env:NEXTEST_TEST_THREADS = $previousThreads
    }
}
```

Observed final JSON: **`ok:true`**. Format and Clippy passed with no warnings;
nextest ran **1337 tests, all passed, 3 leaky classifications, 12 skipped**;
doc tests **1 passed / 10 ignored**; architecture **7 passed**; **8 replays**
verified (1 session, 3 testbed, 4 village); **6 goldens**, none pending;
dependency advisories/bans/licenses/sources all passed. Cargo-nextest and
cargo-deny were available; **no tools were skipped**. Raw verdict:
`target/playtest-evidence/full-verify.json` in the isolated worktree.

Earlier verdicts remain retained: the first passed all 1337 tests but reported
two Clippy style diagnostics, which were fixed; the second passed Clippy but
failed the protected `stop_hook_loop_guard` timing fixture (2 blocks versus 3).
Its fixture uses a 60-second timestamp offset, while that parallel run took over
67 seconds. The unchanged test passed alone in 6.65 seconds and passed in the
final complete run in 9.756 seconds. Only test concurrency was reduced; protected
tests, hooks and their limits were never edited or bypassed.

Use `../island-village-playtest.md` for exact launch/rehearsal commands and the
30–45 minute run sheet. Raw local evidence stays under ignored `target/` in the
original source checkout. A bounded JSON evidence manifest accompanies this note.
Before a GO decision: complete the native confirmation/launcher path and one newly
authorized disposable authenticated connection/continuation smoke. The offline
full gate and source review have passed; rerun applicable checks if further
source corrections become necessary.
