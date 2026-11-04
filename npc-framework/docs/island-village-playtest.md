# Island Village first playtest

Use one human-bodied visitor, one explicitly selected provider and the existing
native village at 1x. Observe for 30–45 minutes. A normal day is 24,000 ticks,
20 minutes at 20 ticks/s; use the separate seasonal fixtures for winter.
For the named stream hero, native launch command and on-air run sheet, use
[Herp's stream guide](island-village-stream.md).

## Prerequisites and ten-minute setup

Build from `npc-framework/`:

```powershell
cargo build --locked --release -p actor_testbed -p village -p rts_units_demo -p game_menu -p npcctl --bins
target/release/game_menu.exe
```

The provider CLI must already be installed and signed in. Installation and login
are outside the ten-minute setup target. Choose the provider, a **new disposable
conversation**, and wall-time/turn/spend limits before launching any AI. Never
search private history for a conversation or resume an existing personal one.

1. In the launcher, create a new AI explorer with a new disposable agents folder,
   a human actor and **Island Village**. Do not select a personal agent or save.
2. In **Your AI agent**, choose **Start Codex** or **Start Claude**. The dedicated
   client supplies the current mission and verified game MCP settings on every
   initial/resumed turn. Normal setup requires no copied prompt, manual project
   trust configuration or conversation-ID lookup. Existing operator-owned
   provider configuration stays preserved.
3. Confirm the client is connected in the **F10** panel. The mission teaches
   earning a living, food/water/energy, Diary and permanent death. Use **Stop
   agent** to stop its owned client/turn; the game has its own save/stop lifecycle.
   See [the client guide](ai-agent-game-client.md) for bounded terminal operation.
4. Native Start resumes the world as an operator action. The AI registers,
   enters, requests control and observes. If the operator pauses later, a
   control claim stays queued until the operator resumes. **Return to world**
   preserves pause; the AI client never advances ticks or resumes the world.
5. Confirm the HUD shows the visitor, its zero starting coins and current
   activity. Observe a useful action and its result. Record setup time, tool/model
   latency and every operator intervention separately.

The AI uses only registered game tools and perceived/remembered opportunities.
Merchant prices/demand are public; another actor's wallet, unseen inventory and
live unseen location are private. A rejected action is useful feedback: observe
again, approach a moving trader, choose a different buyer, or wait for production.
Free gathering and self-supply crafting are intentional alternatives. No rule
requires purchases merely to force a particular route; the rehearsal nevertheless
proves that purchases and consumption/rest work.

## Advanced compatibility: manual exact-session continuation

The dedicated client manages this automatically in the normal flow. The steps
below support an independently managed legacy CLI conversation. Use one dispatch
owner for an actor; do not set up a second dispatcher beside the dedicated client.

Let the original one-shot turn finish. Record its exact disposable conversation
UUID privately. Link **only that conversation**, from the operator's shell:

```powershell
target/release/npcctl.exe scheduler bind --home C:/NPC/playtest-agents --agent Rehearsal --provider codex --session-id SESSION_UUID --turn-finished
target/release/npcctl.exe scheduler check --home C:/NPC/playtest-agents --agent Rehearsal
target/release/npcctl.exe scheduler status --url http://127.0.0.1:PORT
```

Use the actual agents folder/name/port. For Claude, explicitly choose
`--provider claude` and verify that installed adapter separately. While an
original **one-shot** client still runs, use `--client-pid PID` instead of
`--turn-finished`; its PID is a liveness guard, never a termination target.
Interactive clients require an explicit finished-turn signal. Do not use `--last`,
alter models/permissions, or infer that a quiet actor means a quiet model.

`check` inspects the local link/workspace/program/lock without launching a turn;
it does not prove provider login or authenticated continuation. `status` is
read-only. Confirm the actor becomes idle, the original turn is finished, and
the same linked conversation resumes and makes another valid game request. Watch
for overlap. Stop at the authorized time/turn/usage cap. A fake runner and the
offline rehearsal cannot satisfy this live-provider gate.

## 30–45 minute run sheet

| Time | Operator task | Record |
|---|---|---|
| 0–10 min | Cold New → prompt → resume → registered control → useful action | setup minutes, provider/version, pauses and interventions |
| 10–20 min | Watch gathering/crafting and a first sale; let the AI choose its route | first sale real seconds and sim tick, goods/price, rejections/retries |
| 20–30 min | Observe food/water/lodging purchase and actual eat/drink/rest | first purchase and needs-cycle ticks, reserves, coins, stuck periods |
| 30–40 min | Observe idle continuation, then save/stop/resume and reconnect | same-conversation continuation, nudge count, recovery interventions |
| 40–45 min | Review Diary, collect bounded evidence, stop owned resources | alive/dead state, unresolved errors, TPS/frame samples and decision |

Brief the AI once: make a living and satisfy needs using game tools, keep a short
Diary, and respect permanent death. Do not give entity IDs, hidden map routes or
shop inventory. Track later hints as interventions; record the original failure
instead of silently retrying until a clean take exists.

At each observation, ask: what is the actor doing; what can it afford; why did
the last action fail; who acts next (operator, AI, scheduler); and how long has
it been stuck? Use the existing profiler to record achieved simulation TPS and
render/frame intervals at 1x, including whether the window was focused. Compare
against a baseline on the same machine. Paused accelerated fixtures measure
behavior, never native TPS or model latency.

Stop early for a wrong/private conversation, overlapping turns, an authorization
or ownership violation, missing save, currency duplication, repeated unrecoverable
needs/production failure, provider error requiring login, or the agreed usage cap.
Do not kill the main actor to demonstrate permanent death. Use the separate
disposable fixture below. Unexpected death during the real playtest remains a
recorded outcome.

## Recovery cues

| State | Responsible party and next step |
|---|---|
| Paused / control queued | Operator resumes the game; agent waits/rechecks control |
| Walking/gathering/crafting/resting | AI waits for completion or uses cancellation/`stop_work`; supervisor waits |
| Target moved / out of reach | AI observes again and approaches the last perceived current target |
| No stock / merchant cannot afford / enough stock | AI chooses another perceived opportunity or waits for production/trade |
| Unlinked | Operator links the exact authorized disposable conversation once |
| Original turn active | Operator waits for its exit or explicitly signals finished turn |
| Running / cooldown / backoff | Supervisor owns the next step; read status and respect its timers |
| Program/workspace/link error | Operator uses `scheduler check`, fixes that local prerequisite and relinks explicitly |
| Host restart | Stop the prior owned host, Resume its immutable save, then register/enter/take control again |
| Permanent death | Use New life; register/enter/take control for the new identity and relink its own conversation explicitly |
| Stale runner lock | Confirm the old turn is finished before rebinding with `--turn-finished`; inspect malformed locks |

Keep earlier life notebooks. A successor gets fresh private books; only the dead
actor's Diary is published in the continuing village. Memory, Proposals,
credentials and conversation data stay private. A save failure keeps the host
alive: resolve the error before closing it. Use launcher Stop/Quit or the host's
normal save/stop flow for owned games; never terminate another process by name.

## Maintained offline rehearsals

Run from `npc-framework/` with a single Cargo builder:

```powershell
cargo test --locked -p npcctl --test island_playtest -- --nocapture
cargo test --locked -p testbed_content --test island_economy_soak -- --nocapture
cargo test --locked -p testbed_content --test island_village
cargo test --locked -p npc_agent_io --lib scheduler
cargo test --locked -p testbed_view --lib
cargo xtask verify
```

After the release build, `python scripts/playtest/capture_native.py` creates a
new disposable native explorer, records normal/fog/narrow/DPI/recovery screenshots
and a short 1x profiler sample under ignored `target/playtest-native-*`. Python 3
standard library and the native renderer are required. Open every PNG; a summary
alone does not establish visual quality. Inputs are BRP widget/key events and
recorded operator fixture commands, not physical or screen-reader testing.
After a full capture matrix, `--confirm-polish` provides a selective six-state
confirmation: normal world, notebook, Herp speech, resident dialogue at desktop
and 480 width, and recovered Diary. It still runs the complete fixture and cleanup,
marks `polish_confirmation:true` in the summary, and is not a new full visual
matrix. It rejects `--baseline`; `--browser-check` keeps its usual checkpoint.
With the sibling browser viewer built and Node installed, add `--browser-check`
to hold an owned native/browser pair after Herp speaks. The fixture writes a
bounded `browser-ready.json` with public URLs and its projection path; create
`browser-done` in that same ignored run folder after inspection to continue.
It stops both owned processes on completion/failure and times out after ten
minutes if no completion marker arrives. This exercises one shared native world.

Only after explicit disposable-session authorization and agreement to its fixed
limits, `python scripts/playtest/client_smoke.py --authorize-disposable` attempts
one new Codex conversation and one event-triggered continuation through the
dedicated game client. It requires an existing subscription login. Its
hard caps are ten minutes and two turns. It never changes global provider
configuration, trust, login, shell or sandbox policy, reads private history, buys
usage or retries another provider conversation. Its model is selected through
the client's bounded configuration/catalog preflight for the owned process.

Codex CLI 0.145 cancels approval requests in noninteractive execution. A cancelled
NPC tool stops the client with `game_tool_cancelled`; SDK completion alone does
not make that a successful game turn. If separately authorized, add
`--approve-game-tools` to this smoke command. This opts the owned Codex processes
into explicit approval of the 16 validated registered NPC tools, including world
actions and the actor's private Notebook. It grants no other server or shell
permission and writes no global configuration. Existing tool filters remain;
explicit conflicting NPC approval preferences stop setup. Disposable-session
authorization alone does not authorize this extra option. Native **Start Codex**
displays the same game-action scope before starting; **Start Claude** does not
receive the Codex-only option.

The disposable fixture sets the idle fallback beyond the smoke budget, limits
dispatch to two turns, and requires real speech, physical Diary pages and
notebook notes on both. After the first turn it records a labeled operator
`SetNeed(Hunger,30000)` threshold event to prove dispatch without idle fallback;
this is a notification fixture, not unaided survival evidence. Exact session
reuse is compared privately and reported only as a boolean. Review its sanitized
summary under `target/playtest-client-smoke-*`; a blocked preflight is not a pass.
`live_smoke.py` remains the legacy manual-link alternative. Choose one rehearsal;
authorization for two turns does not authorize running both scripts.

`island_playtest` starts its own binary on a free port and creates agents,
presets and saves under Cargo's ignored test-temporary directory. It obtains
credentials privately from only its new disposable metadata, then uses
session-bound HTTP requests without unit/controller arguments. Its actor chooses
targets and destinations from returned affordances and perceived/remembered
positions. No source entity IDs, teleports, wallet/need edits or free items are
used in the economic route. The operator harness alone steps the paused clock,
including applying a queued control claim. The test exercises a first sale,
bread/water purchase and consumption, bed/rest, a retained inventory item and
save/stop/resume/reclaim with Herp. It also rejects duplicate gathering,
cancels work without producing a delayed free item, requires registration again
after a host restart and preserves Herp's Diary. It is a scripted registered
client, not a real model.

A separate fixture records operator `SetNeed` commands to force death. It checks
saved permanent death, a new identity/handshake, revoked old control, fresh
private books and recovery/reading of the old public Diary. Its forced state is
never presented as unaided survival. Child guards stop only hosts they created,
including after failed assertions.

Each fixture prints one bounded `ISLAND_PLAYTEST_SUMMARY { ... }` JSON record with
`ok`, fixture name, seed, preset and intervention labels. Success records include
`owned_hosts_stopped:true`; assertion failures also print `ok:false`. Cargo's
exit code is authoritative. Capture stdout in a local evidence file when needed;
do not commit disposable metadata or raw credentials.

`island_economy_soak` covers seeds 1, 7 and 42, NPC-only and an opening scripted
visiting trade over three normal days, and separately accelerated seasonal fixtures. It prints
`ISLAND_ECONOMY_EVIDENCE` JSON for production, population health, stock, finite
currency, transactions and sampled necessity/buyer availability. This operator
fixture intentionally uses known scene identities; it does not replace the cold
registered rehearsal. Initial bowl saturation is checked separately from a
labeled demand/depletion fixture that requires new bowl production.

## Evidence and issue template

Keep a bounded, sanitized evidence manifest beside the coordinator's readiness
note. Include build revision/dirty scope, exact commands/results, seed, immutable
preset identity, machine/viewport/DPI, screenshot and profiler/journal references,
rehearsal summaries, first-sale/purchase/needs-cycle times, stuck duration,
tool rejection/retry counts, nudges, interventions and deaths. Keep raw/private
state under ignored `target/` or the disposable folder; commit only sanitized
summaries. Never include tokens, provider UUIDs/transcripts, Memory or Proposals.

For an issue, record: trigger; expected and observed outcome; seed/preset/build;
sim tick and wall time separately; the last public action/refusal; recovery
attempts; intervention count; and sanitized screenshot/journal references.

**GO** needs the full readiness contract, no unresolved P0/P1 blockers, passing
maintained rehearsal, current native captures, a successful explicitly authorized
real-provider connection/continuation smoke, and `cargo xtask verify` JSON
`ok:true`. Missing live authorization/evidence means **OFFLINE READY / LIVE SETUP
BLOCKED** only when the offline gates pass. Remaining economy, control or
persistence blockers mean **NO-GO**. Record gaps explicitly; do not convert fake
runner, compiled UI or accelerated clock evidence into a live-play claim.
