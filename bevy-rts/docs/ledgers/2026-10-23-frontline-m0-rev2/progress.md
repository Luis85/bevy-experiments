# SDD ledger — plan: docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md
Predecessor ledger (rev-1 plan, tasks T1–T7 done + T12 committed): .superpowers/sdd/2026-10-23-frontline-m0-foundation/progress.md (rulings R1–R19 live there).
Controller worktree C:/fl/a (m0/phase-a). main ff'd to 482d66e (+ d3c93d7 spec R19 fix on phase-a).

## Pre-flight scan (rev-2 plan; planner self-reviewed; controller spot checks)
| Item | Finding |
|---|---|
| Planner spec gaps 1–13 | 1 fixed in spec/ADR (d3c93d7). 2–13 resolved by plan as stated in planner-rev2-report.md — ruling R20 accepts them. |
| Lane ownership table | disjoint files per parallel pair; T12 rebase after T4 |
| ≤2 builders | lane1 + lane2 only |
Ruling: R20 accept the planner's resolutions of spec gaps 2–13 (runner thread-spawn exception; phase markers as events timestamped by fl_observe; `Early` reject; content names in payloads; stats_alloc; test-guard counts removed tests from diff; float primitive ban probed; hook guards in settings + agent frontmatter; *.new + bless convention; fl_game loader callback for fl.load*; Class::hashed(1); WorktreeRemove probe) — they follow the spec's intent — cost: spec text lags plan until T30 docs pass folds them in.

## Progress
Task 1: implemented a23775b (build-dir template works; cold build 18m; bevy features 2d,3d,ui no audio). Review dispatched (base d3c93d7).
Cleanup: removed stale C:/t/fl/debug (29G, rev-1 shared target) and C:/t/inst (5.7G install scratch).
Task 26: dispatched lane 2 (C:/fl/t26 from a23775b).
Task 2: dispatched lane 1 concurrently with T1 review (Ruling R21: overlap review/impl in lane 1 when files are disjoint — wall-clock — cost: T1 fix may need rebase-free follow-up commit).
Task 1: review Needs fixes — check_deps.sh not updated (report claimed it was); bevy feature aliases instead of explicit leaf list. Minors: CRLF Cargo.toml, lock downgrades unexplained, sccache wrapper hard dependency (doc in T4), redundant [lib] path.
Ruling: R22 explicit bevy leaf feature list lives once in [workspace.dependencies]; crates use workspace = true — single auditable place — cost: none.
Task 1: fix round 1/5 dispatched (base a23775b)
Task 1: minor (deferred→T4 docs): rustc-wrapper=sccache hard-fails without sccache; redundant [lib] path in fl_game.
Task 1: fix round 1/5 (4 addressed, 0 open; commits a23775b..0d1f1f2)
Task 1: complete (commits d3c93d7..0d1f1f2, 1 fix round)
Task 2: implemented 03c0e04 (gate task PASS 27s warm / 167s after xtask change; first run in a build dir 2141s due to per-package feature sets). Review dispatched (base 0d1f1f2).
Ruling: R23 (candidate, apply in T2 fix round) gate task builds once with unified workspace features and filters test execution by package (nextest -E 'package(..)'); per-package isolated headless builds run at gate pr/nightly — keeps task gate within budget in fresh worktrees — cost: task gate doesn't prove headless-feature isolation (arch-check cargo tree still does).
Ruling: R24 `#![deny(clippy::float_arithmetic)]` per crate is added by the rework tasks that own each crate (T5 fl_math, T6 fl_protocol, T7 fl_view, T8 fl_content, T9 fl_sim) — T2 file list excludes crate sources — cost: none.
Task 2: review Needs fixes — Important: arch check_tree default-features/host-only (fail-open via feature-gated deps). Minors: cold budget; shadowing clippy.toml undetected; pending entries unvalidated; deny bevy list hand-maintained; deny all-features=false; no FAIL line before step loop; headless dev-deps unchecked; reverse dependents skipped at task level; missing tests.
Ruling: R25 drop bevy ktx2/zstd_rust features and postcard default features to remove ruzstd (RUSTSEC-2026-0344) and atomic-polyfill ignores — avoid carrying a known vuln ignore — cost: no KTX2 textures until needed (M6 can re-add).
Ruling: R26 fl_feat_x_client may reach fl_sim transitively via fl_feat_x (ADR-0001 says "never directly") — arch-check checks direct edges — cost: feature client crates could misuse sim types; review catches.
Task 2: fix round 1/5 dispatched (base 03c0e04): items 1 (all-features --target all), R23 unified task build, R25, drift any clippy.toml, pending validation, FAIL line, headless dev-dep check + tests.
Task 2: minor (deferred): deny bevy duplicate list hand-maintained (consider xtask test vs Cargo.lock); deny.toml all-features=false.
Task 26: implemented feabf7e (m0/t26-dynlink). Review Needs fixes: probe never crosses the dylib/static bevy_ecs boundary; fl_game --features dynamic never built. Fix round 1/5 dispatched.
Ruling: R27 T26 may change Cargo.lock for its own probe manifest dep swap (fl_content→fl_protocol) — probe-local — cost: none.
Ruling: R28 add .gitattributes (* text=auto eol=lf) in T4 — agents on Windows keep producing CRLF rewrites — cost: one-time renormalize.
Task 26: fix round 1/5 (5 addressed, 0 open; commits feabf7e..a28e79b)
Task 26: complete (commits a23775b..a28e79b on m0/t26-dynlink, 1 fix round). Verdict: dynamic_linking works headless + fl_game builds; windowed untested. Merge into phase-a after T2 fix lands (Cargo.lock overlap).
Task 2: fix 718ed42 (warm gate task 50s; cold fresh build dir 2615s — Bevy compiled twice: clippy check + test build).
Ruling: R29 restore bevy tonemapping_luts/smaa_luts (+ktx2/zstd) — ruzstd stays via bevy_egui anyway; default tonemapper needs LUTs — keep RUSTSEC-2026-0344 ignore with "only embedded trusted LUTs decoded" reason — cost: carrying an advisory ignore.
Ruling: R30 accept cold gate > 300s budget; controller warms each new worktree's build dir with one `cargo xtask gate task` before dispatching an implementer into it — cost: ~40 min per new worktree, done in background.
Ruling: R31 lane-2 worktrees reuse one build dir sequentially: agents set CARGO_BUILD_BUILD_DIR=C:/t/fl/28/2710d09957a748 (T26's warm dir) — only one lane-2 worktree builds at a time, so no lock/feature contention; avoids a 40-min cold build per worktree — cost: workspace crates rebuild per path switch.
Merged m0/t26-dynlink into phase-a; committed the 4 baseline research docs (were untracked).
Task 2: fix round 1/5 (7 addressed + R29, 0 open; commits 03c0e04..c727704)
Task 2: complete (commits 0d1f1f2..c727704, 1 fix round + R29 follow-up)
Task 2: minor (deferred): verify nightly level runs test-headless/arch-check; test-client WARN "no test matched" every run until client tests exist; drift needs git work tree.
Ruling: R32 T27 may add serde_json.workspace=true to probes/remote/Cargo.toml — BRP handler signature needs serde_json::Value, not re-exported by bevy — cost: none.
Ruling: R33 gate base resolution order: --base flag > env FL_GATE_BASE > file .gate-base in worktree root (git-ignored, written by controller at dispatch) > merge-base with main. Controller writes .gate-base = task BASE for every dispatch — check-scope must judge only the task's own diff — cost: none. (Implement in T3 fix round.)
Task 3: implemented 4b2bf08. Review Needs fixes: hunk-local lexing false negative (raw strings); cfg-disabled tests undetected; added low proptest case count undetected. Minors: cancellation ignores paths; tolerance vocab narrow; prop_assume; track delimiter; bless path traversal/atomicity; FL_ROLE wrapper untested; protected-path changes via Bash not caught by any row.
Ruling: R34 new gate row `protected` (task: any changed protected path except *.new = FAIL; pr: allowed only if every touching commit has a `Blessed:` trailer) — closes Bash-route bypass of the Write/Edit hook — cost: lead must use bless+trailer.
Task 3: fix round 1/5 dispatched (base 4b2bf08).
Ruling: R35 .gitignore/.gitattributes belong to the contract track (repo-wide infra config) — cost: none. test-guard base-read fallback only for files absent at base; other git failures fail closed.
Task 27: implemented 8c6dcab. Review Needs fixes: doc verdicts not backed by probe (lit cube, continuous mode, watching/render variants, agent-tool panics, schedule/thread); minors timeout assert, retry loop, panic hang. Fix round 1/5 dispatched.
Task 3: fix round 1/5 (all addressed + R33/R34/R35; commits 4b2bf08..51254b7)
Task 3: complete (commits 97ed4a6..51254b7, 1 fix round + R35 follow-up)
Task 3: minor (deferred): lexer misses br"/cr" raw strings and '\'' length; protected.rs git log pathspec not :(literal); Blessed: matches subject line; split .env( / const CASES / inner cfg in inline src mod not flagged; untested "no commit touches protected file" pr branch.
Task 27: fix round 1/5 (all addressed; commits 8c6dcab..80bbd57)
Task 27: complete (commits 97ed4a6..80bbd57 on m0/t27-remote). Key facts: watching methods must be named `x+watch`; instant handlers run on main thread and stall main world; late replies leak to next call → bridge needs request ids; offscreen screenshot recipe works hidden; HTTP replies chunked. Merge after T4.
Task 12: rebased onto 51254b7 (7613d30); gate task --track client running in lane-2 build dir.
Task 12: review Approved w/ Important: near-plane clipping at zoom_max (distance 400 vs view_height 600). Minors: cascades untuned; scroll unit ignored; weak wasd test. Fix round 1/5 dispatched to fresh implementer (original stopped) in C:/fl/t12.
Task 4: implemented da4b28c (72 xtask tests; gate PASS 135s). Concerns: hooks via `cargo run` wait on build lock → timeout may fail open; live subagent/hook-stdin verification not possible from this session; extras (--only, read-only role); 2 CRLF files remain (fl_protocol/src/lib.rs, fl_sim/src/build.rs → normalize in T6/T9); frontmatter keys unverified.
Review dispatched + claude-code-guide schema check in parallel.
Task 4 schema check (claude-code-guide): MultiEdit not a tool name; guards lack onFailure:"block" (timeouts fail open, default 600s); deny must be exit 2 and internal errors must exit 2; PreToolUse JSON = hookSpecificOutput.permissionDecision; Stop hook should honor stop_hook_active; WorktreeRemove fires only for WorktreeCreate-hook worktrees (dead here); agents should list PowerShell too; subagent input carries agent_type/agent_id. → fold into T4 fix round with reviewer findings.
Task 4: review Needs fixes — hooks fail open (exit 1, timeouts, build-lock waits); unknown agent_type → Lead; gate/guard use session worktree not agent's; xtask bless bypass via `cargo run -p xtask -- bless`; .gitattributes extra.
Ruling: R36 hooks call a prebuilt xtask binary (target/hooks/), fail closed (exit 2, onFailure:block, explicit timeouts), Stop honors stop_hook_active — guards must never fail open — cost: hook binary may lag source by one gate run.
Ruling: R37 agent_type absent ⇒ Lead; present & unknown ⇒ Implementer; test-author --only excludes snapshots — least privilege for unknown subagents — cost: generic subagents get the SubagentStop gate.
Ruling: R38 keep .gitattributes; renormalize the 2 CRLF files in a separate whitespace-only commit — cost: none.
Task 4: fix round 1/5 dispatched (base da4b28c).
Task 12: fix round 1/5 (4 addressed; commits 7613d30..5457dc2). complete (m0/t12-camera 51254b7..5457dc2). Merge after T4 lands.
Task 12: minor (deferred): tune_cascades rewrites config every pan frame; zoom_camera→fit_distance not system-tested.
Task 4: fix cf93ae9 + renormalize 44661fc. Re-review dispatched. Accepted: test-guard WARN NewSeedOverride on renormalized fl_sim/src/build.rs:101 (pre-existing line). Deferred to first repo-rooted session / T30: live hook verification (sim-engineer denial, stdin capture, onFailure honored).
Task 5: dispatched lane 1 (base 44661fc) concurrently with T4 re-review (disjoint files).
Task 4: fix round 1/5 (all addressed; commits da4b28c..44661fc). complete.
Task 4: minor (deferred): planted .git disables guard protection (gate protected row backstops); stdin read error on Stop blocks every time; worktree-remove from inside the worktree fails on Windows (doc); branch-write flag over-match; shell guard misses rm/del/Remove-Item/git mv/rm/sed -i on protected paths; insta env case-insensitivity on Windows; implementer with xtask scope can weaken own hooks; generic subagents gated on SubagentStop (R37 consequence — accepted).
Merged m0/t12-camera (711a383) and m0/t27-remote (96e1f3e) into phase-a; .gate-base → 96e1f3e for T5 (T5 base for review = 96e1f3e + its own commits). TODO: gate pr + ff main after T5 (topology says after T4).
Task 5: review Approved (reference vectors independently verified by Python oracle). Minor follow-up dispatched (rejection-path pin test 4212446 / 27 draws; doc wrap) — test-only + doc, no re-review (Ruling R39: oracle-derived pin tests from an approved review skip re-review — cost: none).
Task 5: minor (deferred→T10): stale doc comment fl_sim/src/rng.rs:10 references derive_seed. test-guard WARN from brief-mandated derive_seed test deletion accepted.
gate pr PASS 665s (WARN test-guard: 10 accepted findings). main ff → ee01867 (Phase I + T5).
Task 5: complete (commits 96e1f3e..45415d9)
Task 6: implemented 32ec154 (gate PASS 58s, tracks contract+sim-core). Review dispatched. test-guard findings: moved unit tests; cmd(0)→cmd(0,5).
Task 8: dispatched lane 2 (C:/fl/t08 from 32ec154) incl. T5-review fl_content minors.
Task 7: dispatched lane 1 (C:/fl/a, base 32ec154) concurrently with T6 review (Ruling R40: start dependents of a contract task during its review when its API is unlikely to change — cost: rework if review changes API).
Task 6: complete (commits 45415d9..32ec154, review clean; both test-guard findings accepted by reviewer).
Task 6: minor (deferred→T28 contract snapshots): no golden-bytes/serde-reflection pin of PlayerCommand/MatchHeader encoding; remap can't distinguish missing table; StringTables::insert allows dup names (debug_assert); duplicate roundtrip test.
Task 7: implemented c772e9d. Ruling R41: view channels encoded with postcard (JSON would make Fx lossy) — follow-up dispatched before review — cost: postcard becomes a normal dep of fl_view.
Task 7: complete (commits 32ec154..88d5e76, review clean).
Task 7: minor (deferred): seq u32 overflow in stamper restore (checked_add); ToRunner lacks serde derives; snapshot entities sortedness unchecked (debug_assert in Publish, T12); postcard trailing bytes accepted in channel decode; tests for hp_frac cap / take_due(MAX) / FromRunner direction. ⚠️ T12/T13 must expose LastSeq for CommandStamper::restore after load.
Task 8: complete (commits 32ec154..e8903d4 on m0/t08-content, review clean). Hash RED evidence: (1<<54)|1 vs |2 pair (brief's (1<<53) pair didn't discriminate).
Task 8: minor (deferred): registry_string_tables_sorted can't detect sort bug; no dir-order independence test; fingerprint value changed (not in commit msg); missing-section error wording. ⚠️→T9: ensure the live sim app's DefRegistry is the frozen one from ContentDb (app_ext lazily inserts an unfrozen registry).
Task 20: implemented 2d0ae38 (m0/t20-clientview). Review Needs fixes: send-side overstep fraction vs receive-side snapshots → backward jitter each tick (plan-mandated wording); TickOutput.journal dropped.
Ruling: R42 interpolate on the receive timeline (t from curr-arrival / effective tick period; t=1 when paused) — the plan's overstep wording is wrong for a threaded runner — cost: one tick of extra visual latency.
Task 20: fix round 1/5 dispatched (JournalBuffer + SimJournalMsg; dt clamp; cache refresh; despawn test). Deferred: fog filter ignores LocalPlayer (M3); PlayerColors insert by T22; no Hull-sized fallback box.
Task 9: implemented 1dfc139 (73 tests; gate PASS 117s; 17 test-guard findings from ports). Review dispatched. Extras: core.state_registry (External), core.state_hash_log (Transient until T10), core.last_seq (Hashed). Classification check after SimTick init.
Ruling: R43 probes are historical evidence; they may stop compiling as APIs evolve (excluded from default-members and the gate). T30 adds a probes/README note "built against <commit>" — cost: probes not re-runnable without checkout.
Task 9: complete (commits 01c7699..1dfc139, review clean; 17 test-guard findings accepted by reviewer).
Task 9: minor (carry→T10): CommandQueue SimHash writes dense CommandTypeId (use write_registry_id if hashed before T11 deletes it); Cycle error wording; "classified twice" panic lacks feature names; no SimBuilder-level duplicate test; debug_spawn_resolves test should assert no rejection; RunConfig literal vs ..Default (expect needless_update). Deferred: generated clippy.toml warns rand::rng/rand::random unresolvable (template noise → xtask owner).
Task 20: fix round 1/5 (5 addressed; commits 2d0ae38..fd50e16). complete (m0/t20-clientview 01c7699..fd50e16).
Task 20: minor (deferred): catch-up clamp applied before speed factor (×8 → up to 40 ticks per 250 ms stall).
Merged m0/t20-clientview; .gate-base → c1f0224 for T10 (T10 review base = this merge).
Ruling: R44 debug.* commands are issued as LocalPlayer; the sim skips the ownership filter for DEBUG_ONLY kinds (still gated by cheats_allowed) — avoids client impersonating other players — carry to T11 (sim) and T21 fix (client) — cost: debug commands can affect any unit when cheats are on (intended).
Task 21: implemented 8e8c196. Review Needs fixes: R44 (issue as LocalPlayer). Fix round 1/5 dispatched. Deferred: egui systems unexercised until T22 windowed smoke.
Ruling: R45 Review Focus 1 ('hash equals run without the bad command') is read as: gameplay state identical; core.last_seq advances for every non-duplicate command per plan (it is Hashed) — tests assert gameplay components equal + LastSeq value — cost: none.
Task 21: fix 6355a20 (R44). Found flaky T20 test despawned_record_removes_placeholder_meshes (2/6 fail; fake runner race) — assigned to T21 implementer (fl_client worktree) before re-review. Task 10: implemented de05d31; review dispatched.
Task 10: review Needs fixes — exclusive core systems block public hooks; add_message exports type_name into MatchHeader string tables. 30 test-guard findings accepted by reviewer.
Ruling: R46 each hook = private core slot (first) + public feature slot; SimBuilder::add_systems targets the feature slot — features can use every hook incl. Apply/Spawn/Housekeeping — cost: one extra set per hook.
Ruling: R47 add_message::<M>(name) with namespaced stable names; dedupe on builder TypeId set — no compiler-dependent strings in headers — cost: API change before any feature uses it.
Task 10: fix round 1/5 dispatched (base de05d31). ADR-0005 gets a Refinements note.
Task 10: minor (deferred): adding_a_stream_does_not_move_wander is weak; clippy template rand::rng warning noise.
Task 21: fix round 1/5 (R44 + toggle + flaky race; commits 8e8c196..cb190cf). complete. Merged into phase-a (8cfc38e); .gate-base moved (T10 fix uncommitted, still covered).
Task 10: fix 296046f (R46/R47). Re-review dispatched. Carry→T11: CommandLog Transient but never cleared (clear per tick or reclassify).
Task 10: fix round 1/5 (3 addressed; commits 8cfc38e..296046f). complete (c1f0224..296046f).
Task 10: minor (deferred): app_mut() can bypass hook slots (review flags use).
Task 19: implemented 4df225b (m0/t19-observe). Review dispatched. Carry→T22: open a per-frame 'frame' span in the client so client systems fold; →T18: emit real fl_sim::phase events + RunConfig::phase_markers (profiler only tested synthetically).
Task 19: complete (8cfc38e..4df225b on m0/t19-observe, review clean).
Task 19: minor (deferred): flood-control window counts frames too (separate tick/frame counters; report suppressed count); no flood/per-phase/per-feature budget tests; cross-thread test sequential (use Barrier); init_headless silent when another subscriber exists; JSON tick stack pops by owner not span id; budget warnings lack tick; log-crate records target "log". ⚠️→T18/T12: switch profiler_sim to phase_markers + runner, assert 8 phases.
Task 11: implemented aea2eae (111 tests; gate PASS 144s; 32 test-guard port findings). Review dispatched.
Merged m0/t19-observe → 36adb7c.
Task 14: dispatched lane 2 early (C:/fl/t14 from 36adb7c) (Ruling R48: base content can proceed in lane 2 while lane 1 does T12/T13 — content format is stable since T8/T9 — cost: small setup-API adaptation if T11 review changes it).
Task 11: review Needs fixes — off-map debug.spawn reported applied. 32 test-guard findings accepted by reviewer.
Ruling: R49 Late/Early rejections don't advance LastSeq (else the correct on-time delivery would be DuplicateSeq); funnels drain message buffers; ScaleScenario MatchSeed carried to T17/T18 — cost: none.
Task 11: fix round 1/5 dispatched concurrently with T12 in the same worktree (disjoint files; explicit --base 296046f).
Task 11: minor (carry→T13 save): assert spawn/despawn request buffers empty at save time (Transient pending gameplay input).
Ruling: R50 a task that changes its own crate's manifest may commit the resulting Cargo.lock delta; check-scope's Cargo.lock→contract finding is accepted when the reviewer confirms the lock delta only reflects that manifest change — cost: manual check (xtask could map lock changes to the touching manifest later).
Task 14: complete (36adb7c..e4073e2 on m0/t14-content, review clean; R50 lock delta verified). Merge after T12 commits.
Task 14: minor (carry→T17): base_testbed_runs should assert positions/headings/live count and snapshot records (after T12); no stat table test.
Task 16: lane 2 worktree from m0/t14-content.
Ruling: R51 step_tick copies SimEvents into TickOutput (events are Transient and cleared next tick, so each output holds exactly its tick's events) — functionally equal to drain — cost: one clone per tick.
Task 16: complete (e4073e2..d422c20 on m0/t16-cli, review clean). --json kept (T24 makes --json global; T24 adds CLI tests). 
Task 16: minor (carry→T24): binary-level success-path test; hash-dump text output test; kinds ordering stability; Box::leak in intern.
Task 12: complete (36adb7c..4f4040d, review clean). Clippy template std::thread::Builder::spawn ban accepted (contract file outside list).
Task 12: minor (deferred): TickDriver::issue doc vs lockstep stamping (soften doc + ADR note); hp_frac MAX for no-Health; query panic tick wording; ReplayDriver::issue panics (T18 decide); scoped-thread APIs unbanned; panic-hook stderr in tests. ⚠️ confirm no mutating system after PublishSet::Snapshot.
Pending merges into phase-a after T11 fix commits: m0/t14-content, m0/t16-cli.
Task 11: fix c758704; gate check-scope FAIL only from controller's wrong --base (296046f predates T19 merge) — accepted (controller error, cost: none).
Merged t14 (b44193e), t16 (611bc39). Lane1 T13 + lane2 T22 (C:/fl/t22) dispatched from 611bc39.
Task 11: fix round 1/5 (4 addressed; commits 4f4040d..c758704). complete (296046f..c758704).
Task 13: review Approved (b155188); 3 test-guard findings accepted. Important cross-task gap: PlayerInfo.faction dense id hashed/saved raw.
Ruling: R52 fix faction-by-name now in T13 follow-up (players.rs authorized) — before save format freezes / any mechanic reads factions — cost: small.
Task 13: minor (deferred): NoMigration wording for newer saves; duplicate tick/allocator fields not cross-checked; pending exec_tick < tick not rejected at load; remapped world not re-saved/stepped in test; clippy template rand warnings.
Task 13: follow-up round (R52 + 2 minors addressed; commits b155188..192152d). complete (611bc39..192152d).
TODO: gate pr + ff main (post-T13 checkpoint) once T15 finishes (avoid 3 builders).
Task 15: implemented 088195c. Review Needs fixes: fresh spawns join grid next Perceive but rebuild-on-load files them immediately → live vs loaded divergence at save boundaries.
Ruling: R53 file entities into the spatial grid in the spawn path (setup_spawn + funnel) and remove in despawn path, keeping PerceiveSet::Spatial for relocations; add a save-right-after-spawn grid equality test — cost: spatial touches spawn.rs.
Task 15: fix round 1/5 to dispatch after gate pr (running in C:/fl/a) finishes.
gate pr PASS 212s (76 test-guard findings, all accepted in per-task reviews). main ff → 192152d (Phase II complete).
Task 15: fix round 1/5 (5 addressed; commits 088195c..3735d01). complete (192152d..3735d01).
Task 17: implemented 3735d01..ccb40ab (4 commits incl. hash-layout fix: absent Hashed state contributes nothing). Review dispatched. Task 18 dispatched lane 1 concurrently.
Task 17: review Needs fixes — publish guard misses deferred commands from Publish systems before the publisher. Hash-layout change judged correct. 22 test-guard findings accepted (pending raw-list confirmation).
Ruling: R54 accept golden test in fl_sim/tests/golden.rs (core_mod duel) instead of fl_test_utils (no serde dep); make hash prefix-free with presence counts now (no stored goldens yet); ADR-0004 refinement note — cost: all hash values change once more (none stored).
Task 17: fix round 1/5 dispatched concurrently with T18 (explicit --base 3735d01).
Task 17: minor (deferred): plan text line ~1031 & populate description stale (fold into T30 docs pass).
Task 22: implemented 5ad8221 (m0/t22-game). Review Needs fixes: client profiler depends on global EnvFilter + duplicated predicate.
Ruling: R55 T22 may export a client-layer helper + span predicate from fl_observe — single source of truth — cost: fl_observe API grows.
Task 22: fix round 1/5 dispatched. Carry→after T18 merge (T25/T30): RunConfig.phase_markers = cfg.profiler and load_debug_config before start_sim.
Task 17: fix round 1/5 (4 addressed; commits ccb40ab..2d4221e). complete (3735d01..2d4221e).
Task 18: implemented 40af03b. Review dispatched. Golden replay crates/fl_test_utils/tests/replays/testbed_wander.replay.new awaits controller bless after review. Carry: fl_observe profiler_sim → real markers (tools track; do in T23/T30); SimCounters ambiguity allowance not added.
Task 18: review Approved (2d4221e..40af03b). 6 test-guard findings accepted; other 4 belong to T17 fix (accepted there).
Blessed golden replay: ab35b19 (Blessed: M0 golden: testbed wander).
Ruling: R56 "every sim log record carries tick" is met by fl_observe's JSON layer (tracks entered sim_tick stack, T19); stderr fmt output may lack it — acceptable — cost: human stderr logs inside systems lack tick.
Task 18: follow-up dispatched (non-vacuous shuffle test, journal rule docs, fl_view emitted doc, no-vacuous verify_replay, golden file presence).
Task 22: fix 87a1550. Note: stress FPS 25-40 at 3000 units observed under heavy machine load (debug build?) — T30 must measure release on an idle machine; perf work likely needed (ruling pending measurement).
Task 18: follow-up fb3ae78 (5 items). Ruling: R57 small test/doc follow-ups after an approved review skip re-review (verify_replay no-vacuous change is covered by its new test) — cost: low. Task 18 complete (2d4221e..fb3ae78).
Task 29: dispatched lane 1 (base fb3ae78).
Task 22: fix round 1/5 (5 addressed; commits 5ad8221..87a1550). complete. Merged → 8c7f0e5; .gate-base → 8c7f0e5 (T29 notified).
Task 23: implemented 3c856ff; review dispatched. Task 24 dispatched lane 2 from m0/t23-bench (R40 pattern).
Task 23: review Approved w/ Important (profiler 240-tick window not stated in output). Ruling: R58 T23 fixes done by the T24 implementer as a separate commit on m0/t24-agentcli (same files, same lane) — avoids a rebase — cost: T23 fix reviewed with T24.
Task 29: implemented 22f98cb; .new artifacts await bless after review. Ruling: R59 remove SCCACHE_BASEDIRS from .cargo/config.toml — cross-worktree sccache hits bake another worktree's CARGO_MANIFEST_DIR into test binaries (stale fixture paths) — keep sccache for registry deps — cost: no cross-worktree reuse of workspace crates. (Apply in T29 fix round.)
Task 29: review Needs fixes — ScaleMix External breaks save/branch (plan-mandated); alloc counters runner-dependent; alloc.bytes gates one-off growth. Test-guard finding accepted.
Ruling: R60 ScaleMix Hashed with codec — gameplay-driving state must be saved — cost: none.
Ruling: R61 fl_game stress uses ScaleScenario::with_units(units) so the mix scales — cost: stress numbers differ from earlier T22 runs.
Ruling: R62 bump REPLAY_FORMAT_VERSION→2 (ReplayStart::Scale layout changed) and re-bless golden — cost: re-bless.
Task 29: fix round 1/5 dispatched (incl. R59 SCCACHE_BASEDIRS removal).
Task 24: implemented b7fbac3 (T23 fixes, R58) + d9f563c. Review dispatched. Found fl_observe bug: observe_layers with profiler:false disables all logs (None layer → LevelFilter::OFF); fl_cli works around (profiler:true). Fix needed in fl_observe (assign in T24 fix round).
Task 24: review Needs fixes — replay_app duplicates verify_replay setup (Important). T23-fix (R58) items verified.
Ruling: R63 defer shared fl_sim::replay_app extraction until T29 (editing replay.rs) merges; schedule as a small lane-1 follow-up — cost: temporary duplication.
Ruling: R64 T24 fix may edit fl_observe to fix "profiler:false disables all logs" (also affects fl_game) — cost: none.
Task 24: fix round 1/5 dispatched.
Task 24: fix round 1/5 (6 addressed; commits d9f563c..45e1173). complete (3c856ff..45e1173 on m0/t24-agentcli; includes T23 commit 3c856ff + T23 fixes b7fbac3). Task 23 complete via R58. Merge m0/t24-agentcli after T29 commits.
Task 25: dispatched lane 2 (C:/fl/t25 from 8c7f0e5).
Task 29: fix round 1/5 (all addressed; commits 22f98cb..a4aad35). Blessed a5a85d6 (budgets.ron, 3 perf snapshots, golden replay v2). Task 29 complete (8c7f0e5..a5a85d6).
Merged m0/t24-agentcli (T23+T24).
gate pr after T23/24/29 merge: FAIL — fl_cli bench_reports_each_size expects entities==units (50) but ScaleScenario default now includes the entity mix (1150). Integration fix + R63 replay_app dispatched (lane 1).
Integration fix 4958312 + R63 6bf03ad; gate pr PASS 170s; main ff → 6bf03ad. Review dispatched; T28 dispatched lane 1.
Integration fix: review Approved (3a300fa..6bf03ad). Minor: CliError::Build→Setup for replay build failures; replay_app tests cover Scenario start only.
Note: repo role agents (sim-engineer, client-engineer, reviewer, perf-analyst, test-author) and bevy-api skill became visible to this session. Ruling: R65 keep dispatching general-purpose agents from this session — role agents carry fail-closed frontmatter hooks resolved against the session's project dir (the-factory worktree), which could block every edit — cost: no automatic hook guards for M0 agents (gate rows protected/test-guard/check-scope still enforce).
Task 28: implemented d9eaf5d; 8 .new artifacts await bless. Ruling: R66 api-toolchain stays 'nightly' (undated) in M0 — avoids another global toolchain install; rustup only moves it on explicit update — cost: API snapshot noise after a nightly update. Review dispatched.
Task 28: review Approved. Ruling: R67 before bless — share format guard via #[path] module (plan-mandated duplication removed), API listing with -ss (no auto traits; churn under R66), record cargo-public-api version — cost: small follow-up.
Task 28: follow-up a887339 (R67). Accepted out-of-list: serde-reflection dev→dep in fl_sim; Codec::trace_enum + players.rs/units.rs edits. Blessed contract baseline. Task 28 complete (6bf03ad..bless commit).
Task 25: implemented 815f0d9 (m0/t25-brp); review dispatched. Carry→T30: gate nightly gpu row builds without debug_remote (screenshot test never runs) and compiles broken probes/dynlink (R43) — exclude probes / add feature in xtask.
gate pr PASS 413s (all rows incl. api). main ff → 68743c7.
Task 25: review Needs fixes — fl.step unbounded; fl.hash stale after fl.step; gate rows skip fl_client brp_ and gpu screenshot (plan-mandated cfg). 2 test-guard findings accepted.
Ruling: R68 T25 may edit xtask gate rows: brp row includes fl_client with features; gpu row builds with debug_remote and excludes probes — cost: longer nightly.
Task 25: fix round 1/5 dispatched (cap fl.step at 600 ticks, latest_hash, compile-time BRP gating, load docs, PendingReplies reset).
Task 25: fix round 1/5 (all addressed; 815f0d9..d5ae651). complete. test-guard NewIgnore on cfg(all(debug_remote, debug_assertions)) test gates accepted. Merged → 68743c7.
gate pr PASS 606s on 3500b33 (merge T25, conflict resolved by controller keeping R61). main ff → 3500b33. Task 30 dispatched.
M1 plan written: 4fb22bd docs/superpowers/plans/2026-10-24-frontline-m1.md (21 tasks; Task 1 = ADR-0014/0015/0016 proposed + ADR-0007/0012 refinements). Planner report: planner-m1-report.md.
Ruling: R69 accept all 13 planner spec-gap resolutions as provisional (mtime polling for hot reload; path budget charges from-scratch cost so caches never change outcomes; terrain_factor widened to [-1,1] with min-speed clamp; Movement.path_terrain_factor guess; bridges = terrain cells via debug.set_terrain until M7; SimQuery::Debug registry; client-side PlayerView stub until M3; temporary closed Order enum in MoveOrder v2 replaced by C7 order queue in M2 prelude; client `replay save`; ADR-0014 fixes layer format; height = per-class slope blocking guess; provisional key bindings until M6; in-house A* for work counting) — controller accepts ADRs at M1 Task 1 review — cost: rework if user disagrees, listed in final report guesses.
Task 30: implemented 5609c06..56e3425 (DONE_WITH_CONCERNS). gate pr PASS 1029s, gate milestone M0 PASS 1149s (before last 2 docs/perf commits), gate task PASS after. Bench release mean/p95 ms: 500u 0.537/1.31, 3000u 4.431/7.96, 5000u 9.335/16.33. Stress 3000 median avg 98.8 fps (machine loaded). No .new to bless. Concerns → final fix wave: release-only failing test unclassified_sim_type_fails_build (needs cfg(debug_assertions)); DEBUG spawn logging ~30ms/tick under default debug.ron; BRP_EXTRAS_PORT documented but unread; idle-machine rerun owed.
Task 30 review + final whole-branch M0 review dispatched in parallel (read-only).
Task 30: review spec ✅, quality Needs fixes (Important: no test pins debug.ron load order; release-gate unclassified test; DEBUG spawn log cost; BRP_EXTRAS_PORT). Minor: empty run dir on content error; M0.json lacks head SHA; md line length. Folded into M0 final fix wave with whole-branch findings.
Final M0 review: Ready with conditions. Critical 0, Important 8 (I1–I8), Minor 18 (final-review-m0.md).
Ruling: R70 (I2) check_save compares feature ids and requires save feature version ≤ build's; per-codec versions + migrations decide loadability; check_replay/check_handshake stay strict; format guard fails a codec version bump without a migration covering the committed version — recorded as ADR-0006 refinement — cost: one more header rule to maintain.
Ruling: R71 (I7) RejectReason gets #[non_exhaustive] + Feature(&'static str) with namespaced codes ("feat.reason"); SimEvent::CommandRejected carries a stable code string, not Debug output — cost: contract change now (pre-freeze) instead of later.
Ruling: R72 (I4) framework enforces Transient: Housekeeping clears Transient components; Transient resources reset via Default unless opted out with a named reason; all add_message buffers cleared at end of step_tick (spec: messages are read in the tick they were written) — cost: features needing cross-tick signals must use Hashed state.
Ruling: R73 (I3/I6/I5/I8/I1) adopt reviewer fixes as written.
Ruling: R74 deferrals: Minor 8 (LockstepDriver save/late-join/batch validation) → fl_net milestone plan (M8); Minor 14 (snapshot schema for non-unit objects) → M4 plan. Minor 15 resolved by contract text: Derived rebuild = one rebuild fn per feature, ordered by feature load order.
Final fix wave: lane 1 (C:\fl\a) Wave A = sim contracts (I1,I2,I3,I4,I6,I7, minors 4,7,9,10,11,12,15,16,17, release-gate test); lane 2 (C:\fl\fixb, branch m0/final-fix-b) Wave B = guards/tools/client/docs (I5,I8, minors 1,2,3,5,6,13,18, T30 Important: debug.ron load-order test, spawn DEBUG log cost, BRP_EXTRAS_PORT, empty run dir, M0.json head SHA). Base 56e3425.
Fix wave B: implemented 56e3425..8ac5de4 (m0/final-fix-b, 7 commits). gate pr: all rows pass except protected (M0.json needs Blessed trailer, commit 4870337) + api (fl_content.txt.new) — lead to bless. Lifecycle debug stress 29.7→0.78 ms with fl_sim::spawn=info. Concern: rand clippy bans inert (rand denied via arch-check). Review dispatched.
Fix wave B: review spec ✅ quality Approved (8 minors; parked: debug.ron test covers startup_config not build, env wiring untested, debugging.md:299 concrete hash, rand dev-dep ban, DefRegistry Component+Resource in API = 0.20 resource-as-component, accepted). Lead: reworded 4870337 with Blessed trailer (now on rebased branch), blessed fl_content API snapshot. gate pr rerun on fixb.
Fix wave B: gate pr PASS 632s on dfbe70c. test-guard 6 findings accepted (filter default changed to fl_sim::spawn=info; remote_port default test replaced; system_order test renamed). Ready to merge after wave A.
Fix wave A: implemented 2acf466..040185a (16 commits). 589/594 tests; 5 failures = .new awaiting bless (formats x2, system_order, perf_counters x3) + budgets.ron.new (spatial.checked, alloc +820B/+3) + fl_sim API .new. No hash/golden change. Disk C: hit 0 bytes during gate (now 74G free). Review dispatched (opus).
Ruling: R75 accept wave A's fl_cli call-site edits (agent.rs, save_cmd.rs) outside its tracks — mechanical consequences of I1/Minor 7 — cost: none.
Ruling: R76 for M1: per-tick steering output must not be a Transient *component* (archetype churn under R72 housekeeping); use a Transient scratch resource keyed/sorted by SimId — carry into M1 Task 11 dispatch — cost: none if honoured.
Ruling: R77 Minor 17 caps stay code constants in core until a rules section for orders exists (M2 prelude C7) — cost: tiny.
Fix wave A: review spec ✅, quality Needs fixes (Important: build.rs rerun-if-changed on missing packed-refs → rebuild every time). Bless recommended for all 7 .new artifacts. test-guard 33 accepted. Parked: Minor 6 (Derived without rebuild unchecked), Minor 10 (SimCounters realloc per tick).
Merged wave B into m0/phase-a: 1099b76 (clean).
Fix wave A: fix round 1/5 dispatched (Important 1 + minors 1,2,3,4,5,7,8,9 + stale .claude docs).
Fix wave A: fix round 1/5 (c2b60e5..c4f18f7) — re-review: all addressed, no new Critical/Important. test-guard 43 accepted. Lead regenerated fl_sim API .new (+#[non_exhaustive] SaveError) and blessed all 8 artifacts: f247a85. gate milestone M0 running.
Parked (final): wave A Minor 6 (Derived w/o rebuild unchecked), Minor 10 (SimCounters realloc/tick), release builds drop late spawn requests silently (debug assert only), .claude/rules/sim.md Banned list lacks new bans; wave B minors 1-7.
gate milestone M0 on f247a85: FAIL test-headless (smoke_shuffle_invariant: seed 0x5eed0002 changes nothing); all other rows PASS incl gpu, brp, perf-file. Fix round 2/5 dispatched to wave A implementer (root cause first).
M1 plan pre-flight + alignment: c1d41ec (70 edits; 29 conflicts, 28 fixed in text). Table: m1-preflight.md.
Ruling: R78 M0 saves must load under M1 (spec §13.2: migrations kept forever from end of M0). Mechanism: a save codec may register a `fill_missing` fn used when a Hashed/Derived state name is absent from an older-feature-version save; default stays LoadError. M1 registers fill for core.terrain (rebuild from the save's map def via content) and core.path_queue (empty). Evidence: an M0 testbed save fixture generated from main@M0 tip, committed as a protected fixture via bless, loaded under M1 in a test — cost: one more codec hook.
Ruling: R79 core.invalid_target = RejectReason::Feature("core.invalid_target"), no new variant (R71).
Ruling: R80 ToRunner becomes #[non_exhaustive] in M1 Task 12, recorded as ADR-0012 refinement.
M1 plan rulings R78-R80 folded: 0ae615d.
Fix wave A: fix round 2/5 d0506b0 — root cause: Bevy shuffle_seed permutes graph nodes, not edges, so only root systems reorder; Housekeeping consolidation removed the M0 roots. Probes now 8 root systems (ambiguous_with_all). Controller reviewed the diff: accepted. Parked: shuffle coverage of real sim systems is partial (Bevy limitation) — note for M1+ determinism reviews.
gate milestone M0: PASS 829s on d0506b0. main ff → 0ae615d; tag m0. M0 COMPLETE.
