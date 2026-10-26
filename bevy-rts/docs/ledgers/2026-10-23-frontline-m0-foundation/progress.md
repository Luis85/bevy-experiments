# SDD ledger — plan: docs/superpowers/plans/2026-10-23-frontline-m0-foundation.md
Controller worktree: C:/fl/a (branch m0/phase-a); main = C:/Projects/bevy-experiments (ff/merge via git -C).
Spec: docs/superpowers/specs/2026-10-23-frontline-rts-design.md (reachable).

## Pre-flight scan
| Pair / task | Produces vs consumes | Finding |
|---|---|---|
| T1 | all crates, deps, stubs | .gitignore lacks /.superpowers/ (SDD workspace) — ruling R1 |
| T2 | Fx/Angle/Pcg32 | self-consistent; ron dev-dep present for fx_rejects test |
| T3 | ids, PlayerCommand | no fl_math dep, consistent with table; can run parallel to T2 |
| T4→T5 | LinkCtx minimal→extended; ContentDb stub→full; register_object_kind("unit") | T4 file list omits db.rs but Interfaces say ContentDb stub lives in db.rs — ruling R2 |
| T4→T7 | DefComponent::instantiate(SpawnCtx, EntityWorldMut) | consistent |
| T5→T6 | DefRegistry::with_builtins, fingerprint, load_dirs | consistent |
| T6→T7 | SimAppExt (+register_command in T7), SimEvents clear before apply_commands | ordering inside ApplyCommands needs a sub-set; implementer's call |
| T6→T9 | SpatialRules/SpatialPlugin stub, MovementSystems (T7) | consistent |
| T7→T8 | Selectable/Hull/Mobility keys only | consistent |
| T8→T10/T11/T13 | base_mod_dir, base.testbed (10 units) | consistent |
| T10→T15 | fl_cli/tests/fixtures/bad_mod used by T15 test | T15 runs after T10 (phase C) — ok |
| T10→T16 | both edit fl_cli lib/main | sequential (B then C) — ok |
| T12/T13/T14/T15 | separate fl_client files; lib.rs ClientPlugins from T1 | ok; EguiPlugin only added by T14 |
| T16 criterion bench in fl_sim | needs content but fl_sim cannot dev-dep fl_catalog per T1 table | ruling R3 |
| T13 | SimHost uses std Duration + bevy::platform Instant | allowed by disallowed-types |

Ruling: R1 T1 adds `/.superpowers/` to .gitignore — SDD workspace must stay untracked — cost if wrong: none.
Ruling: R2 T4 may create src/db.rs with a minimal ContentDb stub that T5 fills — plan Interfaces already say so; file list omission is a typo — cost: none.
Ruling: R3 criterion bench (T16) loads fl_sim's own tests/fixtures/core_mod content via load_content(&[], ..) instead of base content — avoids a dev-dep cycle not in the T1 table — cost: bench unit mix differs from fl_cli bench (both still scale scenario).

## Progress
Scope change (from originating session, user's words): build M0–M9; per-milestone plans written just before each milestone; final report only at the end.
Ruling: R4 T1 review runs concurrently with T2/T3 implementers in C:/fl/a (disjoint crates; plan allows T2∥T3) — saves ~wall-clock — cost if wrong: T1 fix could collide with T2/T3 commits (index lock), retry.
Task 1: minor (deferred): check_deps.sh fails open if `cargo tree` errors (2>/dev/null, empty output → deps OK); add non-empty guard.
Task 1: minor (deferred): check_deps allowlist has bevy_ecs_macro_logic (needed) — add comment.
Task 1: complete (commits a655efc..0090f47, review clean)
Task 3: minor (deferred): Tick::next uses `+1` (overflow after ~6.8 years at 20Hz).
Task 3: complete (commits 0090f47..03c3f58, review clean)
Task 2: review approved (0 Critical/Important). Minors: table bits not pinned; int serde visitor names rounded value; length vs length_sq boundary; rotate_towards tie undocumented.
Ruling: R5 Fx/Angle serde must be lossless for binary formats (is_human_readable → raw i64/u16) — postcard command payloads must be bit-exact; reviewer ⚠️ flagged lossy f64 serde — cost if wrong: small extra code.
Task 2: fix round 1/5 dispatched (R5 + pin table bits + doc tie) — base 0d4f8eb
Task 2: minor (deferred): int serde visitors report rounded value for >2^53 literals; length vs length_sq can differ at boundary.
Ruling: R6 fl_math gets postcard as dev-dep to test R5 raw serde branch — only real non-human-readable format at hand — cost: none.
Task 2: fix round 1/5 (3 addressed, 0 open; commits 0d4f8eb..da0fa1c)
Task 2: complete (commits 03c3f58..da0fa1c, review clean)
Observability requirement folded in (spec §15 commit 8b37b86; plan 11bec9c): T18–T21 added; T5/T6/T7/T13/T15/T16/T17 amended.
Task 4: complete (commits da0fa1c..7f675ab, review clean)
Task 4: minor (deferred→T5): `remove` names not checked against registry at parse time (no line number for typos); parse_object doesn't validate kind registered.
Task 4: minor (deferred): kind-mismatch branch untested; duplicate_component_name_panics reuses a type; needless clone in resolve_chain.
Ruling: R7 resolved `name` inherits the nearest ancestor's name before falling back to id — variants of a named concrete parent read naturally — cost: a template's display name could show for an unnamed child (mitigated: templates carry no names in base content).
Ruling: R8 T5 may enable serde's "rc" feature in the workspace dep so BTreeMap<String, Arc<dyn DefComponent>> serializes for the rules hash — cleanest route — cost: none.
Ruling: R9 T5 also fixes T4 minors 1–2 (remove-name registry check with line numbers; reject unregistered kind) — strictness constraint is binding — cost: small.
Ruling: R10 T6 implementer starts while T5 is in review (fl_content API surface stable; T6 doesn't edit fl_content) — wall-clock — cost: rework if T5 review changes public API.
Task 5: complete (commits 11bec9c..24d0421, review clean)
Task 5: minor (deferred): link pass skipped when any inheritance chain fails (hides unrelated link errors); missing-rules-section error names bare "rules.ron"; LinkCtx separator via scope.contains('/'); comment why abstract set recomputed; test gaps (dep cycle, load_after-only order, rules-hash change on rules/faction); mods.rs mixes 3 jobs.
Ruling: R11 RON struct names stay the Rust type names (MapDef/ScenarioDef/VisualDef/Faction…) or omitted; T8 authors to match — avoids churn — cost: slightly uglier authoring.
Ruling: R12 kind override (e.g. a mod restyling a base unit's VisualDef) deferred to M6 art pipeline — not needed before real assets — cost: mods can't restyle until then.
Ruling: R13 when a new object kind (building) arrives (M4), extend rules_hash to hash it — reviewer ⚠️ — carry into M4 plan.
Ruling: R14 enable bevy_ecs "debug" feature workspace-wide (readable system names for ambiguity errors + profiler attribution) — implementer concern — cost: rebuild, small memory.
Task 6 deviations accepted: SimEntity hooks on_insert/on_discard; SimTickEnd set inside Hash (others .before(SimTickEnd)); ClearSimEvents set (apply_commands .after(ClearSimEvents)); schedule built in build_sim_app. → carry to T7/T18 dispatch.
Task 6: complete (commits 24d0421..ccae65d, review clean)
Task 6: minor (deferred): ambiguity test expected-string too broad; fingerprint doesn't cover hashed comps/systems (feature-set agreement for replays later); register_* after build silently lost; add_sim_event dedupe via contains_resource; MapInfo public fields.
Ruling: R15 T7 adds SimIdAllocator.next to compute_state_hash and a spawn-order-independence hash test — determinism contract — cost: none.
Ruling: R16 MapInfo is inserted by scenario setup (T7 setup_map / ScaleScenario), not CoreFeature — no Default map — cost: none.
Ruling: R17 T12 (camera) starts early in worktree C:/fl/t12 off 2eed139 — depends only on T1 stubs — cost: none. main ff'd to 2eed139.
PAUSE (baseline upgrade from originating session): no new dispatch until spec+plan updated. In flight: T7 (a/phase-a), T12 (t12).
Baseline upgrade: research docs copied to docs/research/. User approved (asked directly): repo-local CLAUDE.md+rules, .claude hooks, role agents+skill+.mcp.json; global installs: nextest, insta, deny, public-api, mutants, sccache, bevy_brp_mcp (installing in background → C:/t/install.log).
Architect agent dispatched: spec rev2 + ADRs + impact report (architect-rev2-report.md). Next: planner revises M0 plan (rework tasks + B tasks), then resume.
Task 12: BLOCKED interim (target-dir contention: bevy_pbr build slow; installs + T7 building). Leaving its background build to finish; agent resumes itself. Ruling: R18 after the baseline upgrade every worktree gets its own build dir (B9; research says avoid one shared target dir) — cost: one cold Bevy build per worktree.
Architect rev2 done: spec + ADR 0001–0013 committed 86b236a. Report: architect-rev2-report.md (impact §2 = rework list).
User decisions (asked directly): D1/D2 defer (Windows-only determinism; no instruction-count tier); nightly toolchain approved (rustdoc JSON for cargo-public-api only).
Ruling: R19 build.build-dir per worktree goes in the REPO .cargo/config.toml (template {workspace-path-hash}), not ~/.cargo/config.toml — avoids user-level config change — cost: none if cargo supports the template (probe).
Task 7: complete (commits 2eed139..482d66e, review clean)
Task 7: minor (carry→rev2 T7-port): commands before map keep unbounded target/radius; tiny positive speed/turn truncates to 0 → unit stuck (validate per-tick >0); KMH_PER_M_PER_TICK integer truncation at other Hz; failed DebugSpawn logged as applied; setup errors lack unit id + partial setup; CommandLog unbounded.
Task 12: stopped idle agent; commit 99e42d6 on m0/t12-camera; full gate unconfirmed → run before merge.
Tools installed: nextest 0.9.149, insta 1.49.0, deny 0.20.2, public-api 0.52.0, mutants 27.1.0, sccache 0.18.0, bevy_brp_mcp 0.23.0.
