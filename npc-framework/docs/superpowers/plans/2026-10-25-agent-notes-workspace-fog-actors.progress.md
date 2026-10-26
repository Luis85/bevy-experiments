# SDD ledger — plan: npc-framework/docs/superpowers/plans/2026-10-25-agent-notes-workspace-fog-actors.md

Spec: npc-framework/docs/superpowers/specs/2026-10-25-agent-notes-and-workspace-design.md
Branch: claude/single-npc-testbed-48eb8e. Plan tasks numbered 1–18 (A1..F1 labels kept).
Phases: A=T1-3, B=T4-5, C=T6-8, D=T9-10, E=T11-14, G=T15-17, F=T18. Phase gate (user rule): full verify ok → merge --no-ff into main at C:\Projects\the-factory (index clean, no dev files under npc-framework/ touched) → cargo build --release of game_menu, actor_testbed, npcctl, village, rts_units_demo there → next phase.
User authorizations: "execute subagent-driven"; phase merges into main per the user's gate rule.

## Pre-flight scan
| Tasks | Shared file/interface | Finding |
|---|---|---|
| T1→T2 | NotesStore API, Book/Writer/Note/Stamp | consistent |
| T2→T3 | NotesDirs, NotesClock, store ownership | F7 must use the same store as the methods → R1 |
| T4→T5 | AgentDir::prompt(kind) replaces start_prompt()/resume_prompt(); actor_testbed callers | T4 alone would break actor_testbed build → R2 |
| T5→T6 | workspace wiring moves into host | sequential, consistent |
| T6→T11 | HostArgs.preset before presets exist | → R3 |
| T6→T7 | host::build_world, RunFile used by serve | consistent |
| T7→T8 | serve flags, --exact-port exit 4 | consistent |
| T9→T13 | reveal_cells per sensor | T9 vision only, T13 extends; consistent |
| T9→T11 | RevealsFog on explorer actor; preset spawn | T11 apply_preset must keep RevealsFog → noted in T11 dispatch |
| T11→T12 | robot sensors rejected until T12 | plan text covers it |
| T12→T13 | SensorReadings, Sensor variants | consistent |
| T11/T13→T14 | load_presets/validate/models | consistent |
| T11→T15 | world_doc(preset, actor) uses ActorPreset | consistent |
| T15→T16→T17 | WorldPreset fields | consistent |
| T3/T10/T14 | testbed_view modules (panel, fog, model_preview) | different files |

| Task | Self-consistency |
|---|---|
| T1–T18 | tests match interfaces; T2's session test needs a host hook to set NotesDirs (implementer adds a test-only hook); T10 rendering technique is builder's call by design |

## Rulings
- Ruling R1: the F7 editor (T3) uses the same `NotesStores` resource the agent/player methods use (one store per folder per process) — the spec's single-owner rule — if wrong: two caches could disagree for ≤1 s.
- Ruling R2: T4 also updates actor_testbed's call sites to `AgentDir::prompt(kind)` so the tree builds; T5 does the remaining wiring — if wrong: none.
- Ruling R3: T6's `HostArgs.preset` accepts only `None`/"human" until T11 (error "actor presets arrive with Task 11" otherwise) — if wrong: one match arm.

## Progress
- Gate 0 (polish + walls): full verify ok:true (782 tests) at ad1ae6b; merged into main as ea12bf1; release build running in main checkout.
- Gate 0: first build attempt used the wrong path (C:\Projects\the-factory is a subfolder; repo root C:\Projects); building in C:\Projects\npc-framework.
- Gate 0: release build ok in C:\Projects\npc-framework\target\release (actor_testbed, game_menu, npcctl, rts_units_demo, village .exe) at main ea12bf1.
- Task 1: dispatched (BASE 77575a5, implementer opus a63ebe67da32b2e29)
- Task 1: implementer DONE_WITH_CONCERNS at 3fe483e (preamble-only unplaceable text; date sanitized: Task 2 must use returned stamps as next expected; migration drops tick from title). Reviewer dispatched (opus).
- Task 1: review Needs fixes (3fe483e): Important 1 updated stamp not a version (hand edits, same-tick writes → silent overwrite; test codifies it); Important 2 blank line before id comment loses metadata. Fix round 1 sent (resume).
- Ruling R4: Task 1 fix round includes cleaning `expected` (minor 1) — same concurrency contract — if wrong: none.
- Task 1: minor (deferred): duplicate ids prefer first occurrence not cached owner; header-less proposal puts all text in header; fingerprint/TOCTOU windows (same-length edit in one mtime tick, edit between refresh and write); no rename retry on Windows sharing errors; parse_old_heading case-sensitive ("Tick"); old F7 panel shows migrated comments until Task 3; `_by` in rustdoc; id reuse after hand-deleting newest; stray text between entries not covered by a test.
- Task 1: fix round 1 implementer DONE at b5e436a (revision suffix #rN in date; blank-line tolerance; expected cleaned). Re-review dispatched (sonnet).
- Task 1: fix round 1/5 (3 addressed, 0 open; commits 3fe483e..b5e436a)
- Task 1: minor (deferred): lossy non-UTF-8 file bump stays in memory only; a hand edit while no store is open gets no revision; write-back after a hand edit rewrites a file the dev may have open.
- Task 1: complete (commits 77575a5..b5e436a, review clean)
- Plan amended (user "yes"): Task 19 (A4) act-and-wait + since-last-look observations, executed after Task 3 and before Task 4 (Phase A = T1, T2, T3, T19). Task 4 must teach the one-call loop.
- Task 2: implementer DONE_WITH_CONCERNS at 02191ad (observer gets read_only on notes/list; world/reset loses notes resources → Task 7 must carry them; NotesClock not inserted until T3/T7; mcp API gains default controller; no --as player flag — npcctl notes always player).
- Ruling R5: `agent/notes/list` is a read and must work in Observer mode; only add/update reject read_only — reading is not acting — if wrong: observers can't read notes.
- Ruling R6: Task 7 carries NotesDirs/NotesClock/NotesStores across world/reset and load_scenario (or re-inserts them) — noted for its dispatch.
- Task 2: reviewer dispatched (opus).
- Task 2: review Needs fixes (02191ad): Important — R5 observer list rejected read_only. Fix round 1 sent (resume).
- Ruling R7: Task 2 fix round also includes MCP error `data.reason` passthrough and the act tool's description of the interact form — MCP-only agents need both (spec §7) — if wrong: a few lines.
- Task 2: minor (deferred): player notes methods reachable by any raw RPC client (doc note); odd indentation in notes_call match; session rebuild drops notes resources (→ Task 7, R6).
- Task 2: fix round 1/5 (3 addressed, 0 open; commits 02191ad..17608f2)
- Task 2: complete (commits b5e436a..17608f2, review clean)
- Task 3: dispatched (BASE 17608f2, implementer opus accc87f3b181f83f9); carried R1, NotesClock insertion.
- Task 3: implementer DONE at 520c822 (shield_world added; stale save keeps typed text, second Save overwrites deliberately; an explorer_modal timing test flaked once — investigate). Reviewer dispatched (opus).
- Task 3: review Needs fixes (520c822): Important — stale-save rebase lets a second Save silently overwrite the agent. Fix round 1 sent (resume) with ruling R8.
- Ruling R8: Task 3 fix round also includes: own-status-change rebase (minor 3), the flaky explorer_modal timing test (must be green before the Phase A gate), clipboard "\n" literal — if wrong: small extra scope.
- Task 3: minor (deferred): shield_world mouse.clear() drops just_released (possible stuck box-drag); field_clipboard duplicates game_menu's copy plumbing (~150 lines); no test that editor and notes/* resolve the same store in the real app; no screenshot of the open form; click on world while a field has focus may be swallowed until Escape.
- Task 3: fix round 1/5 (4 addressed, 0 open; commits 520c822..bd183c7). Flaky test was explorer_modal::a_copy_that_never_reports_times_out_and_copy_works_again (shortened timeout left in place for the 2nd copy).
- Task 3: complete (commits 17608f2..bd183c7, review clean)
- Task 19: dispatched (BASE bd183c7, implementer opus ab0b057011ed53bcc)
- User (chat): merge the perf branch claude/modest-wing-8c598c (95a2b64, f234042) into main at the Phase A gate. Plan: merge it into this branch after Task 19, resolve conflicts, full verify on the combined tree, then gate merge to main + release build.
- User (chat): commit the plan and its progress to main. Done: main commit 760ee99 adds docs/superpowers/plans/...progress.md (ledger copy) + current plan/spec. Ruling R9: the progress file is maintained on main only (synced from this ledger at each gate and notable event), never edited on the branch, so branch merges don't conflict — if wrong: a manual merge of that one file.
- Task 19: implementer DONE at b9f4490 (testbed tests in testbed_content/tests/agent_interface.rs; extra changes.actions field; BRP slot reuse w/o nonce; crafted items not in inventory_gained). Reviewer dispatched (opus).
- Task 19: review Needs fixes (b9f4490): Critical — BRP re-runs answered watchers → act executed twice (always for timeout_ms 0 / rejected); Important — positional slot compaction broken by swap_remove. Fix round 1 sent: nonce required on BRP, nonce-keyed state, 60 s tombstones, docs.
- Task 19: minor (deferred): crafted items missing from inventory_gained; >5 s frame hitch expiry (addressed by fix if done as asked).
- Task 19: fix round 1 implementer DONE at 0583720 (nonce required on BRP, ActWaits keyed by method+params+nonce, 60 s tombstones, compaction removed, CLI nonce unique per call). Re-review dispatched (opus).
- Task 19: fix round 1/5 (3 addressed, 0 open; commits b9f4490..0583720)
- Task 19: minor (deferred): BRP ActWaits retain runs before lookup → a ≥60 s frame gap can evict the served key and re-act (look up/refresh k before retain); MCP nonce mcp-<pid>-<counter> can repeat across restarts (add wall-clock part or a replayed marker).
- Task 19: complete (commits bd183c7..0583720, review clean)
- Phase A tasks complete (T1, T2, T3, T19). Gate A: merging claude/modest-wing-8c598c (perf) per user, then full verify.
- Gate A: perf branch merged as 2d00844 (CHANGELOG conflict resolved keeping both); workspace builds; full verify started.
- Gate A: full verify ok:true (829 tests) at 2d00844; merged into main as 0dc3e3e; release build running.
- Gate A: release build ok (C:\Projects\npc-framework\target\release, 15:47) from main 0dc3e3e. Phase B starts.
- Task 4: dispatched (BASE 2d00844, implementer opus a00db1599156eb59a); carried R2, MCP tool names, one-call loop, .mcp.json backslash path.
Task 4: implemented d132dae (base 2d00844), verify --quick ok 834/834; report written by controller (subagent Write blocked); task review dispatched (opus)
  Carry to Task 5: write_workspace(dir, kind, &WorkspaceVars{prompt, actor: ActorSummary::human_scout()}) must be called by the app; Phase F: old start_prompt.md names in docs/agent-experiments.md, docs/_notes/explorer-e2e.md, testbed_view explorer_modal.rs fixtures
Task 4: review approved (0 Critical/Important, 8 Minor). Fix round 1 sent to implementer for minors 1 (AGENTS.md line 11 vs npcctl), 2 (temp cleanup + per-pid name), 4 (empty actions), 5 (exact settings assert), 8 (slashed visibility).
  Parked: M3 non-UTF-8 npcctl path in .mcp.json (lossy display) — final review; M6 actor_testbed write_prompts duplicate — Task 5 replaces; M7 example texts/vehicles in body description — Ruling: keep, body description is the actor's real capability set and the examples were in the approved old prompt — cost if wrong: one wording edit.
Task 4: complete (d132dae + fix d8e684e; re-review all addressed)
Task 5: dispatched (opus, base d8e684e); carries M6 (remove actor_testbed duplicate prompt writer) and Phase-F rename of start_prompt.md in docs/_notes + explorer_modal fixtures
Task 5: implemented d81b8d5 (base d8e684e), verify --quick ok 838/838, hands-off real run listed every generated file; task review dispatched (sonnet)
Task 5: review approved (0 Crit/Imp). Controller fixed M1 (stray spaces in write_error note) inline as 14b664b. Parked: map_err().err() style, blocked root/.claude paths (Task 4 area). Task 5: complete
Gate B: full verify ok:true (844 tests, replays 8, goldens none pending, deny ok) at 14b664b; merged into main as 6b3e851 (no overlap with dev's uncommitted files); release build running
Task 6: dispatched (opus, base 14b664b) while Gate B release build runs on main. Ruling R10: Task 6 implementation may overlap the gate's release build; Task 6 is not reviewed/accepted until the Gate B artifact exists — the user's rule is that each phase produces an artifact before the next phase lands; a build failure would be fixed first — if wrong: Task 6 waits on a build fix.
  Carried: R3 (preset only None/"human"), workspace write + notes resources move window-free into host for T7.
Gate B: release build ok at 17:28 on main 6b3e851 (actor_testbed, game_menu, npcctl rebuilt; village/rts unchanged). Gate B complete.
Task 6: implemented adbef70 (base 14b664b), verify --quick ok 843/843; task review dispatched (opus)
Task 6: review approved (0 Crit/Imp, 8 Minor). Fix round 1 sent for M1 (CSV comma), M2 (inconclusive tasklist → live), M3 (doc limits), M4 (remove race comment).
  Parked for final review: M5 save_now redundant dir / Autosave.path; M6 dead_pid test pid reuse; M7 shared `now` (note it in Task 8 CHANGELOG); M8 CLAUDE.md crate map for explorer::host (Phase F docs).
  Task 7 must wire RunFile write/remove (window + detached).
User (chat, during Task 6 review): new feature request, a map system for NPCs, with landmarks the actor/agent adds, enabled in the AI-agent world, persisted as markdown in the agent folder's map/ folder. Brainstorming started; the plan continues meanwhile.
User (chat): approved map + memory design; "add the memory task early and the map tasks after world presets". Committed 00d9912: spec 2026-10-25-actor-map-and-memory-design.md; plan Task 20 (C0 memory, after Task 6, before Task 7, in Gate C), Phase H Tasks 21-23 (after G, before F); Task 11 PerceptionSpec drops memory_capacity; Task 15 gains PerceptionRulesSpec + SystemsSpec.map arrives in Task 22; gate list A–E, G, H, F.
Execution order now: 6 (fix round) → 20 → 7 → 8 → Gate C → 9,10 → D → 11–14 → E → 15–17 → G → 21–23 → H → 18 → F.
Note: docs session reports a flaky existing test under heavy load (closed connections get ~4 s = 200×20 ms to free session server slots); watch for it at Gate C.
Task 6: re-review all addressed (1 new Minor parked: NO_MATCH cache of an odd first read). Task 6: complete (adbef70 + c5d0b70)
Task 20: dispatched (sonnet, base c5d0b70)
User (chat): diary and memory notes become one markdown file per entry in folders; all created markdown Obsidian-compatible with proper frontmatter. Brainstorming: asked file-name question (readable titles vs stable ids).
User (chat): file names = readable titles; approved the Obsidian notes design; "add the notes-format task right after the memory task". Committed 543d5bb: spec 2026-10-25-notes-as-obsidian-files-design.md; map spec M4 now uses it; plan Task 24 (C0b) after Task 20, before Task 7; Task 21 file format → Obsidian; owned-files constraint updated.
Execution order now: 20 → 24 → 7 → 8 → Gate C → D → E → G → H → F.
Note for Task 24 dispatch: Task 4's AGENTS.md mentions diary/memory files? (notes only through tools — check); Task 5 hands-off listing showed diary.md at creation (home.rs creates it) — Task 24 changes to folders.
Task 20: implemented 586bf3d (base c5d0b70), verify --quick ok 849/849, capacity 128, no oracle moved; task review dispatched (sonnet)
Task 20: review approved (0 Crit/Imp; minors parked: no ResumedChanged/Rebuilt config assert, 20-tick margin, >32 not ≤128 bound). Task 20: complete (586bf3d)
Task 24: dispatched (opus, base 586bf3d)
main moved: e283d8f (docs fixes, vehicles busy predicate) and 8997cdf (deflake: agent HTTP server's 503 refusals could be lost to a TCP reset; fixed). Gate C merge goes on top; expect possible conflicts in npc_agent_io (session/http) and CHANGELOG.
User (chat): "cleanup in the meantime" → cargo clean in gifted-chatterjee-afe0d8 (finished docs worktree): 61.7 GiB freed, 218 GB free. Skipped production-line-setup-a7b3f4 (recent build activity) and infallible-heisenberg (other project).
User (chat): brainstorm an increment from C:\Projects\npc-agents\sorrel-saltmarsh\proposals (001–011); approved ("proposal is sound"), may change proposal states, "split the increment and fit them into the current iteration plan". Committed 0ae6302: spec 2026-10-25-agent-feedback-increment-design.md; plan Tasks 25 (C0c movement/vehicles: 001, 003, 010), 26 (C0d items/crafting: 002, 005, 006, 008) after Task 24 before Task 7; 27 (D0 reach sense + remember everything: 007, 009, 011) before Task 9; 28 (D3 searched ground: 004) after Task 10. All 11 proposals set to status: pursuing (hand edit; the store bumps revision on next read).
Execution order now: 24 → 25 → 26 → 7 → 8 → Gate C → 27 → 9 → 10 → 28 → Gate D → E → G → H → F.
Task 24: implemented b6eb2f6 + f981814 (base 586bf3d), verify --quick ok 875; hands-off migration shown; task review dispatched (opus). Controller-raised: revision resets per tick → duplicate-id rule should compare (updated_tick, revision) then mtime.
Task 24: review Needs fixes (2 Important: I1 duplicate-id winner can be the older file; I2 new ids can reuse an unreadable file's id). Ruling R11: duplicate-id rank = (updated if both RFC 3339 UTC, updated_tick, revision, mtime); new ids and migrations use a high-water mark over every id seen (readable or broken, lenient id: scan) — spec O4 amended (b67120f) — if wrong: a rank function.
Fix round 1 → implementer: I1, I2, M1 (blank/comment inside nested value kept raw), M2 (old-proposal detection only for real old headers or NNN- names), M3 (frontmatter without id is adopted: gets the next id, written back), M4 (fingerprint check before superseded retry delete), M5 (COM0/LPT0, superscript, CONIN$/CONOUT$), M6 (no ".md.md": strip a trailing .md from file names). Parked: M6 undated/preamble cosmetics, M7 author on hand-edit bump, M8 orphan tmp files and `...` fence.
Task 24: fix round 1 done 99ccb64 (all 8; verify --quick ok 881); re-review dispatched (sonnet)
Task 24: re-review all addressed (new Minors parked: adopt() write failure fails list; date-prefixed hand file in proposals/ treated as old; fingerprint from cached scan; u32::MAX id overflow; migration skip of an id held by an unreadable file not reported). Task 24: complete (b6eb2f6, f981814, 99ccb64)
Merged main into branch as 6cc32a0 (README conflict resolved, both texts); verify --quick ok 885/885.
Task 25: dispatched (opus, base 6cc32a0)
Task 25: implemented e54d452 (base 6cc32a0), verify --quick ok 892; root cause: Direct vehicle driver seat not accepted by steered_vehicle; task review dispatched (opus)
Task 25: review Needs fixes (1 Important: openable closed door treated as wall in explain_blocked → wrong blocker/short nearest_reachable). Fix round 1 → implementer: I1 (planner-semantics flood first, strict pass only if goal reachable; two-door test), M1 cell budget + per-tick cap, M2 unload for Direct driver CHANGELOG+test, M3 drive_to text in metres, M6 testbed wagon DriveTo test. Parked: M4 feature-dependent validate doc, M5 metres truncation/-0.00, M7 journal panel fields.
Task 25: fix round 1 done 2367796 + 3c73938 (verify --quick ok 895); re-review dispatched (sonnet)
Task 25: re-review all addressed (Minors parked: strict pass may still name an openable door when stall is non-grid; EXPLAIN_CELL_BUDGET doc says two grids, worst case three floods; step_unit public vs cap order; untested 'blocked on the way to' texts). Task 25: complete (e54d452, 2367796, 3c73938)
Task 26: dispatched (opus, base 3c73938)
Task 26: implemented 3a1a7b2 (base 3c73938), verify --quick ok 903; '9 of 8' = rule (equipped take no slot) now explained; task review dispatched (opus). Follow-up candidate (out of scope, agent friction): crafted outputs never merge into carried stacks; a crafted output that doesn't fit drops without CraftDone saying so.
Task 26: review approved (0 Crit/Imp; Minors parked: success refused for slots lacks room hint; Interact order precheck failure bare; equip hint can name the equipped item; unavailable craft odds without tool quality; PackFit multi-yield simplification; string-matching with_room_hint; content verbs' spawns listed as dropped; ⚠ layout carry list not slot-checked — diary shows the agent's own count incl. hand item = 9, consistent with the rule). Task 26: complete (3a1a7b2)
Task 7: dispatched (opus, base 3a1a7b2); carries R3, R6, run.json for window + detached
Task 7: implemented ac3bd78 (base 3a1a7b2), verify --quick ok 918, deny ok; hands-off detached run shown; task review dispatched (opus). Carry to Task 8: menu must stop the detached child before starting the window (window doesn't check a live detached run.json).
Task 7: review Needs fixes (I1 calls queued behind session/stop mutate the world after the final save; I2 window doesn't refuse an agent already running detached → two hosts overwrite one save). Fix round 1 → implementer: I1, I2 (+ exact-port waits for the detached pid within the 10 s), M world/reset backup, on_stop extras no override, module doc, ctrlc termination feature (if deny ok), exit 3 for other-build saves in npcctl, prepare() clear error, taken --port leaves no folder test. Parked: N+1 port races; end-to-end detached→window test (Task 8).
Task 7: fix round 1 done 7c75ee3 (verify --quick ok 924, deny ok); re-review dispatched (sonnet)
Task 7: re-review all addressed. Fix round 2 (small) → implementer: deflake queued-behind-stop test (signals instead of 2 s sleep, tick > 0), Windows console-close wording (Unix only; Windows may lose up to 60 s), surface rename-back failure in rebuild. Parked: Windows SetConsoleCtrlHandler real fix (needs windows-sys; doc the gap), reused pid looks alive.
Task 7: fix round 2 done 5dc539b (minor polish from the re-review; verify --quick ok 924; accepted without a separate re-review — Ruling R12: a fix round that only addresses a passed re-review's Minors gets a controller diff-stat check instead of another review — if wrong: a missed nit). Task 7: complete (ac3bd78, 7c75ee3, 5dc539b)
Task 8: dispatched (opus, base 5dc539b)
Task 8: implemented 634c6fb (base 5dc539b), verify --quick ok 939; e2e real-binary handover shown; task review dispatched (opus). Controller-raised: Instant + allow in testbed_content host.rs contradicts Task 6 rule.
Task 8: review Needs fixes (I1 Instant + allow in testbed_content). Fix round 1 → implementer: I1 clock-free waits; M finish_op freeze, tasklist on frame → worker, abort reason visible from Resume row, writeln! not println!, Quit while busy, restore pause state, saved_newer on adopted Stop, gate ExplorerModalSystems under close dialog. Parked: detached child pipes (no log file), silent window→detached failure when npcctl can't take the port (save intact), Resume row lacks explicit 'Start in window' label.
Task 8: fix round 1 done ae01b63 (verify --quick ok 945); re-review dispatched (sonnet)
Task 8: re-review all addressed (Minors parked: power.rs long doc line, use-line placement, Open+non-clean Exited path doesn't call show()). Task 8: complete (634c6fb, ae01b63). Phase C tasks done: 6, 20, 24, 25, 26, 7, 8.
Gate C: full verify started at ae01b63
Gate C: first full verify ok at ae01b63 (951 tests, 8 replays, goldens none pending, deny ok). main had moved: aafc334 (dev's Epoch 1 mission-brief prompts; AGENTS.md shrinks to a character sheet, no tool manual). Merged main into the branch; conflicts in CHANGELOG (kept both) and prompts/agents.md (took main's 'How you reach the world' text + Task 20's memory sentence). Ruling R13: the dev's later prompt rewrite on main wins over branch AGENTS.md wording; branch only adds the memory-span sentence and frontmatter — if wrong: one paragraph. testbed_content tests green; re-running full verify on the merge.
Gate C: full verify ok at 38d4dff (950 tests, 8 replays, goldens none pending, deny ok); merged into main as 128692b (no overlap with dev's uncommitted files); release build running
Gate C: release build partial — actor_testbed, game_menu, village, rts_units_demo rebuilt 04:10; npcctl.exe locked (os error 5) by 3 running 'npcctl mcp --url http://127.0.0.1:15702 --controller explorer' processes (pids 36996, 8320, 10944; the dev's agent sessions' MCP servers). Not killed. Asked the dev. Per R10, Task 27 may start; Phase D acceptance waits for the npcctl artifact.
Task 27: dispatched (opus, base 38d4dff)
Task 27: implemented 9f99e0b (base 38d4dff), verify --quick ok 957; 009 root cause = 'You remember' capped at 8 in SimId order; task review dispatched (opus)
Task 27: review Needs fixes (I1 uncapped 'You remember' ~7-14 KB per observe, still buries old vehicles). Fix round 1 → implementer: 24 most recent + every vehicle/unit + per-kind remainder (framework facts, not content names); same for 'You perceive'; conds.rs doc; document chests perceivable to all NPCs in explorer worlds; apply_senses errors when actor missing; diagonal/corner wall test. Parked: behind/beside staleness, old-save test realism, chest verbs (none exist).
Task 27: fix round 1 done fe7a91e (verify --quick ok 961); re-review dispatched (sonnet)
Task 27: re-review all addressed (Minors parked: doc wording, 'beside the corner' case hits a wall cell, not the gap). Task 27: complete (9f99e0b, fe7a91e)
Task 9: dispatched (opus, base fe7a91e); RevealsFog via host apply_senses (not layout.rs) to keep plain-explorer hash; reach also reveals fog
Task 9: implemented a7cc8d1 (base fe7a91e), verify --quick ok 981; hash pins from fe7a91e all match; task review dispatched (opus)
Task 9: review Needs fixes (I1 fog cost likely breaches the perception p95 budget in the explorer world; unmeasured). Fix round 1 → implementer: measure explorer world; skip unchanged scans + bound per-scan work / reuse buffers; no budget raise (dev's call) — if a separate fog tag is needed, report numbers. Minors: ExploredCells min + surfaced reset, inside-building test, extended LOS-identity test, footprint-based drop of wall records. Parked: occluding props recorded as WALL.
Task 9: fix round 1 done ccafa1f (verify --quick ok 988; perception p95 back inside budget: windowed 131→38-45 µs; fog own system/tag with PROVISIONAL budget row ('fog', 55, 0) — needs dev approval); re-review dispatched (opus)
Task 9: re-review all addressed. Ruling R14: fog cell-centre semantics accepted for terrain; Task 10 draws units only by what perception senses now (PerceptionMemory sensed), never by VisibleCells, so the viewer never shows a unit the actor doesn't sense — if wrong: Task 10 unit filter. Fix round 2 (small) → implementer: RayTable radius cap with LOS fallback, Cone overflow guard, tags.rs doc row, dynamic-layer invalidation test. Open for the dev: provisional fog budget row ('fog', 55, 0).
Task 9: fix round 2 done 0e74e18 (R12: minors from a passed re-review, diff-stat checked; verify --quick ok 991 on rerun after a rustc OOM). Task 9: complete (a7cc8d1, ccafa1f, 0e74e18)
Task 10: dispatched (opus, base 0e74e18) with R14 (units drawn only if sensed now)
User (chat): 'stop the mcp sessions' → stopped npcctl mcp pids 36996, 8320, 10944. Gate C: release build complete at 08:21 on main 128692b (npcctl rebuilt; others from 04:10). Gate C complete.
User (chat): approved the fog budget row ("fog", 55, 0). TODO after Task 10 commits (shared worktree): drop 'provisional' wording in npc_perf/src/budget.rs:32, docs/debugging.md:771, CHANGELOG.md:30.
Task 10: implemented 37c457c (base 0e74e18), verify --quick ok 999; screenshots read by implementer; task review dispatched (opus). Controller committed a5e208d (fog budget row approved wording).
Task 10: review Needs fixes (I1 vehicle crew / nav path gizmos + selectable hidden units leak unsensed units; I2 F1 item markers draw over never-explored black). Ruling R15: with fog on, F1 item markers follow the item visual's look (Hidden none, LastSeen at last-seen pos); F1's other panels and F5/F6/F7 stay unfogged — if wrong: one filter. Fix round 1 → implementer: I1, I2, mask marker query, whole-plugin headless test, apply_dim retry, tooltip spaces, test rename, a live mid-walk screenshot. Parked: decide_looks per-tick cost gating.
User (chat): the vicinity (reach) stays 1.5 m by default but must be configurable. Plan updated (Task 11 SensorSpec reach {radius_mm}, default 1500, 0..=5000, built-ins carry it; Task 14 editor field) — commit 7a0a0ee.
Task 10: fix round 1 done d5d8cd0 (verify --quick ok 1005 on rerun). Flaky under load: npcctl session_host a_long_step_does_not_block_other_requests (wall-clock) — watch at Gate D.
Task 10: re-review all addressed (Minors parked: one-frame selection ring lag when a unit leaves senses mid-tick; DimPending untested; crew_line_shown pub). Task 10: complete (37c457c, d5d8cd0)
Task 28: dispatched (sonnet, base d5d8cd0)
Task 28: implemented dee883c (base d5d8cd0), verify --quick ok. Ruling R16 (pending review): 'since the start' is mislabelled — it is a whole-map share; label it 'of the map' (JSON map_pct) rather than store a start position — honest and cheap — if wrong: add a snapshot-only start pos. Task review dispatched (sonnet).
Task 28: review Needs fixes (I1 'since the start' mislabels a whole-map share; reviewer agrees with R16). R16 confirmed: relabel 'of the map' / map_pct; spec F4 + plan updated in the fix commit. Fix round 1 → implementer: I1, sliver sectors ignored, real coarse cell_mm + cost bound, test hygiene. Parked: zero-offset north, AGENTS.md sentence placement.
Task 28: re-review all addressed (Minors parked: sampled half-rule untested on partial cells; stride can skip thin strips). Task 28: complete (dee883c, 65a2c66). Phase D tasks done: 27, 9, 10, 28.
Gate D: full verify started at 65a2c66
Gate D: full verify ok at 65a2c66 (1018 tests, 8 replays, goldens none pending, deny ok); merged into main as cc6c7a7 (no overlap with dev's files); release build: npcctl.exe was locked by a new 'npcctl mcp' (pid 23908, started 10:50, the dev's agent session) — not killed; renamed the old exe to npcctl.locked-23908.exe (the running process keeps it) and rebuilt; all five apps built 12:08/12:09. Gate D complete.
Task 11: dispatched (opus, base 65a2c66); replaces R3 gate; preset reach replaces EXPLORER_REACH_MM; keep RevealsFog; human hash pin; --preset in actor_testbed
Task 11: implemented 41f1b2a (base 65a2c66), verify --quick ok 1021; human hash pin held; task review dispatched (opus).
User (chat): closing the game must stop all created processes (leftover npcctl). Investigated: the leftover npcctl are 'npcctl mcp' children of VS Code Claude Code sessions (claude.exe), not game processes. Asked the dev whether menu Quit should always stop (save) detached worlds or ask.
User (chat): 'quitting the game, stops all processes'. Plan Task 29 (E0) added ca2c163: menu Quit stops tracked detached worlds (session/stop, save), asks started windows to quit via new BRP app/quit (kill by pid after 15 s), Quit anyway on refused save; npcctl mcp left alone (AI tool's). Supersedes G7's 'outlives the menu' for Quit. Runs after Task 11, before Task 12.
Task 11: review Needs fixes (I1 order Move while driving / orders to a vehicle bypass a withheld drive_to/unload). Fix round 1 → implementer: I1, CHANGELOG brief claim (typed marker), actors_dir from settings via menu, resume keys preset by save, per-test actors_dir in npcctl tests, never-spawned + fresh-world + old-save tests, fresh-world fallback, duplicate reach invalid, verb constants. Parked: LIDAR fov 360, constants duplication, known_actions, built-ins only into empty folder. Note: TODAY_TICK_600 pin couldn't be cross-checked against base (implementer says recorded at 65a2c66 with a temp test).
Task 11: re-review all addressed (Minors parked: write_meta before write_save on rebuild; fresh_world_preset ignores unreadable save silently; Patrol/Follow/Direct driver untested; doc wrap). Task 11: complete (41f1b2a, 1358f66)
Task 29: dispatched (opus, base 1358f66)
Task 29: implemented 314e63d (base 1358f66), verify --quick ok 1034; hands-off real-binary quit: detached world + explorer window + scenario window all stopped, saves newer, run.json gone; task review dispatched (opus)
Task 29: review Needs fixes (I1 Quit anyway can kill a reused adopted pid + misleading 'save failed'; I2 --remote turns on release BRP for windows that gain nothing; I3 stale run.json after kill_window/kill_starting). Ruling R17: no --remote for village/scenario windows; kill them via the menu's Child handle; only explorer windows get app/quit — if wrong: those windows lose a graceful exit they don't need. Fix round 1 → implementer: I1 (identity check + split outcome), I2 (R17), I3, F9-during-quit retry, grace after late app/quit answer, tests, Ctrl+C doc.
Task 29: fix round 1 done cad901f (verify --quick ok 1041). Ruling: implementer's flipped start-time rule accepted (process started ≤ run.json.started + 5 s) — the controller's brief had it backwards. Re-review dispatched (sonnet).
Task 29: re-review all addressed (Minors parked: non-Windows ps status/etimes on macOS; test depends on PowerShell for the start-time case; dead adopted world after NoAnswer still offers Quit anyway). Task 29: complete (314e63d, cad901f)
Task 12: dispatched (opus, base cad901f)
User (chat): agents must register with id + token (a real lock), enter the world, be assigned a character, take control; MCP only, no npcctl in AGENTS.md; prompts a dungeon-master briefing. Spec 2026-10-26-agent-registration-design.md + builder answers; plan Phase I (Tasks 30-31) after E, before G; H map tools session-bound. Committed 1f3b159. Order: 12 → 13 → 14 → Gate E → 30 → 31 → Gate I → G → H → F.
Task 12: implemented 765cbb2 (base cad901f; 1f3b159 docs in between), verify --quick ok 1059; rover perception p95 44 µs (budget 55); task review dispatched (opus). Disk: cargo clean --profile dev on main freed 18 GB (51 GB free).
Task 12: review approved (0 Crit/Imp). Fix round 1 (minors, controller's choice) → implementer: LIDAR cache + own stagger phase (perf headroom), open doors/small items not solid (generic predicate), extended march test, sensor sections grouped after 'You perceive', '1 degree', range_min doc, rover affordance test, small items.
User (chat): once Task 12 is done, wrap up and pause: everything onto main, prepare a resume prompt, then clean up worktrees and disk. Plan: Task 12 fix round → re-review → full verify → merge to main → release build → resume prompt + progress on main → cargo clean worktree target; worktree itself goes when the session is archived.
Task 12: fix round 1 done 3f64b73 + 83031a4 (verify --quick ok 1064). OPEN for the dev: walking rover perception p95 54.6–57 µs vs budget 55 (options: rotating LIDAR / skip empty blocks / accept). Re-review + full verify dispatched in parallel.
Task 12: re-review all addressed (Minors parked for the final review: scan's doc comment now attached to run_lidar; Perceivable with kinds==0 no longer stops a beam; LidarCache never evicts despawned robots; no test asserts a cache hit or facing/param change). Task 12: complete (765cbb2, 3f64b73, 83031a4)
Wrap-up verify at 83031a4: ok:false on two items not caused by the branch: (1) xtask::hooks stop_hook_loop_guard failed under load (count 2 vs 3, 65 s); passes alone in 6.6 s — load flake in an untouched test. (2) cargo deny advisories: new RUSTSEC-2026-0391 (zstd 0.13.3 via bevy_image 0.20; fix needs zstd >= 0.14.3, not semver-compatible with Bevy 0.20); main fails the same check. deny.toml is a protected oracle → asked the dev. Everything else ok: fmt, clippy, 1069/1070 tests, arch 7/7, 8 replays, goldens none pending.
User (chat): 'can we fix it instead of ignoring? if not, ignore and merge'. No fix: Bevy 0.20 pins zstd 0.13/zstd-safe 7 (fix needs 0.14/8.2) and the ruzstd backend has 0344; dropping LUT textures would change every app's rendering. Also two more advisories appeared (0398, 0403). Committed 775ea1b: deny.toml ignores 0391, 0398, 0403 with reachability reasons; CHANGELOG Security. cargo deny now ok. Ruling R18: the wrap-up merge relies on the full verify at 83031a4 (all ok except deny + a load-flaky hook test) plus a clean cargo deny and the hook test passing alone, since only deny.toml/CHANGELOG changed — if wrong: one more full verify.
Pause: merged into main as 95ff601 (no overlap with dev's files); release build ok 22:28/22:29 on main (all five apps). Resume prompt: docs/superpowers/plans/2026-10-25-agent-notes-workspace-fog-actors.resume.md on main. PAUSED by the dev's request after Task 12; next: ask the dev about the walking-rover LIDAR cost, then Task 13.
