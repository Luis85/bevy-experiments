# Island Village and scene AI supervision — verification evidence

The scene is opt-in (`ai_village` / `island_village`); the existing demo registries, replay corpus and
protected baselines are unchanged. New island component, command, recipe and tree names deliberately
give this scene its own content manifest. All authoritative economy/calendar/needs/death state is integer
simulation state. CLI sessions, credentials, wall clocks and processes remain host data.

## Behavior and process checks

- `cargo test --locked -p testbed_content --test island_village`: **14 passed**
  (`target/island-final-focused.log`). Includes a full action-API gather/sell/buy/eat/drink/rest loop,
  conservation of money, atomic failed trades, preserved traded tool identity/wear, impassable water,
  permanent death and diary pickup, bounded page reading, new identities/notebooks/credential revocation,
  dead-world resume, privacy-safe recorded diary publication, snapshot hash equivalence, and all nine
  residents surviving a shortened 29-day year while every production stage runs.
- `cargo test --locked -p npc_agent_io --lib scheduler`: **7 passed**
  (`target/ai-scheduler-final-focused3.log`). Fake subprocesses cover exact session selection, stdin
  prompt delivery, idle/busy/pause handling, cooldown/backoff/hourly limits, process ownership, target
  locks, timeout cleanup and changed-actor/session cancellation.
- `cargo test --locked -p testbed_view --lib village::tests`: **2 passed**
  (`target/island-village-ui-tests.log`), checking native semantic controls/focus, held-book privacy and
  unchanged sim hashes. Later view changes are also covered by the final gate and native captures.
- `cargo test --locked -p testbed_view --lib`: **68 passed, 1 ignored**
  (`target/island-view-final-tests.log`). Adds regression coverage retaining the nine building and four
  resource roots during visual cleanup, and tests real host installation before scene entities exist.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed
  (`target/island-clippy.log`); the final full gate covers the final source revision.

Codex CLI **0.145.0** and Claude Code **2.1.281** local help were used to confirm resume/print/stdin
arguments. No real AI session/provider was launched during implementation or verification. The scheduler
requires an explicit operator link to a specific conversation UUID and actor. An original CLI PID is
only a liveness guard; it is never terminated. An interactive CLI needs an explicit finished-turn signal.

## Native evidence and final gate

`target/island-scheduler-e2e-result.json`: **ok: true**. An owned real `npcctl serve` host ran the island
with a five-second test idle policy. `npcctl scheduler bind` linked an owned Python fake runner to the
actual connected visitor. It received the exact configured UUID and continuation prompt, launched once,
waited while the game was paused, resumed supervision after `world/reset`, kept the UUID out of status,
and accepted `unbind`. The host and its owned continuations exited. No real provider ran.

The first native startup capture exposed missing island scenery/HUD: view plugins were installed before
the scene had spawned `VillageState`. View installation now checks the already-prepared world selection
as well; the native UI tests use that real early-install order. This was a presentation initialization
fix; simulation tests were already green.

`target/island-native-final.json`: **ok: true**, with eight PNGs under
`target/island-native-e6e971253a29/npc-out/shots/`. Every PNG was opened: day and 480-logical-pixel day,
scheduler setup at both sizes, permanent death/New life, the successor holding/reading the old diary,
night lighting and day 22 winter terrain. The scene camera frames the island, all nine workplaces and
four resource sites persist, native controls work, and the owned app exits. The long seasonal fast-forward
uses operator `npc/step`; its profiler budget breaches are not ordinary frame benchmarks or budget waivers.

`target/island-menu-result.json`: three opened captures under `target/island-menu-71a985b911/` show the
Island Village preset, New agent selection and scrollable scenario list. The world editor exposes its
AI scheduler page. Menu control behavior is additionally covered by workspace tests.

The confirmed batch also fixes retained static visuals, respects explicit camera overrides, uses supported
ASCII separators and starts the island with diagnostic F1 overlay hidden so it does not overlap its HUD.
No new shipping rasters were created; existing bundled models/textures retain their attribution.

The fresh Impeccable finish review opened all eleven PNGs and returned **fix**, with one material issue:
the queued successor announcement persisted after arrival and into later death. It otherwise accepted
the scoped incumbent style, native controls, terrain/lighting, diary, scheduler setup and narrow layouts.
The announcement now clears when the actor ID changes. A reconnect hint is derived only for a living
successor in an explorer session while its controller is absent. `cargo test --locked -p testbed_view
--lib village::tests` passes all **3** tests, including arrival and later-death regression coverage
(`target/island-review-fix-tests.log`). The initial full gate was intentionally interrupted for this
review correction; the final run below is authoritative.

`target/island-review-fix-native.json`: **ok: true**, with all eight scene PNGs recaptured under
`target/island-native-e9a5fd68e312/npc-out/shots/`, opened and copied over the same review paths. The
same reviewer reopened the eight captures and returned **ship**, scoring the listed successor-message
fix **resolved**. This verdict covers that fix, alongside the original review's retained decisions.
The documenter compared the result with the incumbent implementation and recorded evidence in
`island-village-ui-documentation.md`; no new project-wide design system was invented for this extension.

Final full gate and main release results follow below.

The first completed full run (`target/island-verify-final.json`) found one compatibility regression:
`game_menu::explorer::tests::no_valid_world_disables_start` failed because additive scene installation
silently supplied a valid world to an invalid-only folder. Its other **1304 tests passed** (2 leaky,
12 skipped), and format, Clippy, architecture, replays, goldens and dependency policy all passed.
Installation now adds the scene only when an existing folder contains at least one valid preset;
invalid-only folders preserve their error state. A new regression also checks upgrading a usable folder
and preserving the exact bytes of user-owned presets. The existing menu assertion is unchanged.

## Final full gate

`cargo xtask verify` returned **`"ok": true`**, exit **0**
(`target/island-verify-confirmed.json`, `target/island-verify-confirmed.exit`). This final run includes
the preset-installation correction and all review fixes; no concurrent release build or native test host
ran beside it.

| Check | Result |
|---|---|
| Format | All files formatted |
| Clippy | No warnings |
| Workspace tests | 1,306 passed (2 slow, 3 leaky), 12 skipped |
| Doc tests | 1 passed, 10 ignored |
| Architecture | 7 passed |
| Replays | 8 verified: 1 session, 3 testbed, 4 village |
| Goldens | 6 snapshots checked with `INSTA_UPDATE=no`; none pending |
| Dependency policy | Advisories, bans, licenses and sources passed |

No verification tool was skipped. Reported test skips/ignored cases and nextest's leaky/slow classifications
are retained above; no assertions, protected files, snapshots, replays or existing budgets were relaxed.

## Boundaries

Physical keyboard/mouse input, OS screen-reader announcements and authenticated provider continuation
remain operator integration checks. The supervisor watches actor work and authorized tool requests; it
does not inspect a model's internal reasoning. Missing/malformed links or unverifiable runner locks fail
closed and remain visible in status. Provider transcripts are discarded. The user's pre-existing MCP
host (PID 57336) and unrelated Terra workspace changes are preserved.
