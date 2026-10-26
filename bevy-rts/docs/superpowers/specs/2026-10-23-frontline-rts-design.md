# Frontline: a 3D isometric WWII RTS on Bevy 0.20 — Design

Status: approved; revisions 2 (baseline upgrade), 2.1 (M0 refinements), 2.2 (final M0 review refinements), 2.3
(M1 contracts), 2.4 (M1 implementation refinements) and 2.5 (M2 prelude contracts; its ADRs proposed until the
controller accepts them under R85) applied · Date: 2026-10-23
Working title "Frontline" (placeholder, easy to rename; crate prefix `fl_`)

The research behind this design lives in `docs/research/`:
- `blitzkrieg-mechanics.md`: rules and formulas reverse-engineered from the original's published source
- `architecture.md`: prior art, crates, determinism
- `assets.md`: free assets and licenses
- `bevy020-api-reference.md`: Bevy 0.20 APIs, each checked with a `cargo check` probe
- `baseline-recommendations.md`: the synthesis behind revision 2 (groups A–D), drawn from
  `architecture-best-practices.md` (critique C1–C24, patterns N1–N22), `workflow-and-verification.md` and
  `agent-dev-environment.md`

Architecture decisions are recorded as ADRs in `docs/adr/` (MADR 4.0). Contract paths change only through an ADR (§13.2).

---

## Revision 2 (2026-10-23, baseline upgrade)

The user approved adopting every recommendation of the three research reports, in stages:
- **Group A** (architecture contracts) and **Group B** (verification and erosion gates) land in **M0**.
- **Group C** becomes the **M2 entry requirements** (§9.2).
- **Group D** is later work, each item with a trigger (§9.3).
- **B8** (a GitHub remote and 3-OS CI) is **skipped** by user decision. CI stays local: `cargo xtask gate …`.
- Repo-local Claude config (CLAUDE.md, rules, hooks, role subagents, a project skill, `.mcp.json`) and the global
  installs (cargo-nextest, cargo-insta, cargo-deny, cargo-public-api, cargo-mutants, sccache, bevy_brp_mcp) are approved.

What changed, and why:
- **A1** Two crates per feature (`fl_feat_x` + `fl_feat_x_client`) replace the `client` Cargo feature. `GameFeature`
  splits into `SimFeature` and `ClientFeature`, and features are sorted topologically (§5.1, §5.2; ADR-0001).
  *Why:* Cargo unifies features across the workspace, so `--workspace` gates compiled the sim with client code and could
  never prove it headless; per-half lint bans were impossible.
- **A2** A threaded `SimRunner` owns the sim World. Commands go in; `Arc<SimSnapshot>` and `SimEvents` come out. The
  snapshot schema lives in the new `fl_view` crate. A `TickDriver` trait has Local, Lockstep, Replay and Test
  implementations (§5.3; ADR-0002). *Why:* a synchronous sim on the main thread breaks 60 fps at a 15 ms tick, and the
  old "sync step" coupled the client to sim component types. This also settles the old §14 SubApp question.
- **A3** Keyed counter-based RNG, `roll(seed, tick, sim_id, stream, n)`. Sequential RNG resources are banned in the sim
  (§5.4; ADR-0003). *Why:* with one shared stream, adding a feature shifts every later draw.
- **A4** Every runtime ID comes from a stable-string registry: sorted names, then dense IDs. Replays and saves carry the
  string tables (§5.4, §5.5; ADR-0004). *Why:* registration-order IDs depend on the build.
- **A5** The sim schedule becomes eight phases with named hook sub-sets, explicit `ApplyDeferred`, a single spawn funnel,
  an explicit executor, and housekeeping (hash, `clear_trackers`, message rotation) in the driver (§5.6; ADR-0005).
  *Why:* auto sync points make visibility timing depend on unrelated features; manual stepping skipped housekeeping.
- **A6** Every sim component and resource must be classified (hashed+saved, derived, transient; resources may also be
  external). A startup check enforces it (§5.4; ADR-0006). *Why:* unregistered state escaped the hash and saves.
- **A7** "Sort everything by SimId" narrows to order-sensitive operations; `par_iter_mut` is allowed for entity-local
  updates (§5.4; ADR-0005). *Why:* the old rule cost time and blocked safe parallelism; the branch test enforces it.
- **A8** A save/load skeleton: versioned postcard codecs per component, typed migrations, relationships as `SimId`, a
  byte-identical round trip and a branch test (§5.8; ADR-0006). *Why:* campaigns, reconnects and late join need it, and
  retrofitting is how saves rot.
- **A9** A lockstep-ready command envelope (`exec_tick`, per-player `seq`, `(player, seq)` apply order, input delay 1 in
  single player), postcard on the wire, and one `MatchHeader` for handshake, replay and save. The rules hash gets a
  canonical binary encoding (§5.4, §5.5; ADR-0007). *Why:* single player and multiplayer share one path, and a peer with
  different code is caught at handshake.
- **A10** Debug and testbed actions are `debug.*` commands gated by `cheats_allowed`. "Reveal all" is a client render
  override. BRP reaches the sim only through custom `fl.*` methods (§5.5, §11, §15; ADR-0008). *Why:* client-side sim
  mutation broke replays and the commands-only anti-goal.
- **A11** Hot reload: value-only changes apply in place; an ID-set change rebuilds the match from a snapshot; reloads are
  replay events (§6.4; ADR-0012). *Why:* live entities hold dense IDs that would go stale.
- **A12** The scale target is an entity mix: 3,000 units ≈ 8–12k sim entities plus projectiles in flight (§2 G7, §5.7).
  *Why:* squads, crews and wrecks multiply the entity count.
- **B1–B7, B9, B10** replace §12 and §13 with the verification baseline: `cargo xtask gate`, Stop/SubagentStop hooks,
  generated clippy bans, `xtask arch-check`, cargo-deny, shuffle runs and a system-order snapshot, protected tests,
  work-counter perf gates, contract snapshots, ADRs and ownership, and the agent environment (§12, §13; ADR-0009,
  ADR-0010, ADR-0011, ADR-0013). *Why:* make erosion and agent shortcuts fail mechanically instead of in review.
- **B10** `fl.screenshot` uses an offscreen camera; benches switch the window to continuous updates (§15 C).
- **B8** skipped (§13.4). Cross-OS determinism and instruction-count trends have no local home yet (open questions for
  the user, recorded in the architect report).
- **C1–C11** are M2 entry requirements (§9.2). **Group D** items are listed with triggers (§9.3).

Unchanged: the intent, goals and anti-goals, the hard determinism and dependency rules, the never-copy-Nival rule, asset
licensing, the content model (§6.1–6.3), the game rules (§7), and §15's observability requirements.

---

## Revision 2.5 (2026-10-25, M2 prelude contracts)

The M2 prelude plan's contract task (`docs/superpowers/plans/2026-10-25-frontline-m2-prelude.md`, Task 3) records
these contracts for Group C (§9.2) and the M1 carry-overs (ruling R145; `R<n>` are controller rulings in the M1 and M2
prelude SDD ledgers). Where this list and an older paragraph of this spec (or of revisions 2.1 to 2.4) disagree, this
list wins. The ADRs and refinements named here are `proposed`; the controller accepts them at that task's review under
the user's delegation (R85), and the plan's final report lists them for ratification.
- **Core vocabulary (ADR-0017, C5):** registered conditions (table `"conditions"`) with counted grants and tokens in
  the Hashed `core.conditions`, and `requires` expressions (`&&`, `||`, `!`, parentheses) compiled at link time.
  Registered stats (`"stats"`, `-1,000,000 ≤ min ≤ max ≤ 1,000,000`) and modifiers `(stat, Add | AddPct | Mul |
  Override, Fx, source)` in the Hashed `core.modifiers`, evaluated override → Σ add → × max(0, 1 + Σ add_pct) →
  each mul, clamping after every step, in a total order within each bucket (source key bytes, source `SimId`, op,
  value), with value bounds and at most 16 modifiers per stat so no intermediate overflows `Fx`. The Derived
  `EffectiveStats` cache stores the base it was computed from and is recomputed for dirty units at the start of
  `ThinkSet::Orders` and in `ResolveSet::Stats`; a read re-checks the base and never observes a stale value (R149,
  R151). Tags (`"tags"`, at most 128) as a `TagSet(u128)` and class expressions as `all`/`none`/one `any` mask.
  Effect and trigger verb registries (`[Name(args), …]`); effects applied in `ResolveSet::Effects` in `(source
  SimId, emission order)` order, and effects requested after that slot wait in the Hashed `EffectQueue` for the
  next tick. Command buttons with `requires`, evaluated on the client against the `core.conditions` view channel
  (§5.2, §5.4, §6.1, §10).
- **Unit behaviour and the entity graph (ADR-0018, C7):** `OrderQueue` of `QueuedOrder { kind, target, param }`
  (registered kinds, table `"orders"`) replaces M1's closed `Order` enum, keeps the state name `core.move_order` and
  becomes codec v3, migrated from the frozen v2; at most `MAX_WAYPOINTS` (32), a code constant. A Hashed
  `ActivityStack` (at most 8 frames) run by `run_activities` in `SimIndex` order (`Continue | Done | Push`; a pushed
  child ticks in the same tick, a parent resumes on the next, at most 32 root orders begun per unit per tick, the
  queue cap, so M1 timing never changes), cancelled top-down with one cancel step per frame. `RelatedTo<R>(SimId)` (Hashed) and `Related<R>` (Derived, sorted by
  `SimId`) instead of `ChildOf`; a despawned target's sources are stripped by the funnel after features apply their
  `LifecycleSet::Death` rule. Ownership stays the `Owner(PlayerId)` component, not a relationship, although C7 lists
  it. Hashed keys into Derived layouts re-plan when their layout rules change (R129) (§5.4, §5.6, §5.8, §7).
- **Event model (ADR-0019, C7):** Messages for gameplay events; component hooks for indices and invariants only;
  observers only for order-independent reactions (a marker insert, a push into a buffer its consumer sorts), never
  randomness, `SimId` allocation, spawns or shared writes; no entity disabling in the sim (§5.4).
- **Content load phases, patches and provenance (ADR-0020, C6, C2):** discover → parse (a lossless RON-subset tree,
  `ContentTree`, with spans, because ron 0.12's `deserialize_any` loses bare identifiers) → migrate (`mod.ron`
  `format_version`) → patch (`*.patch.ron`: `Set`, `Merge`, `Remove`, `Add`, `AddTag`, `If`; pass `Default` then
  `Final`, each in mod load order, then file order) → inherit → typed deserialize → feature post-process hooks → link
  → freeze → rules hash, unchanged for unpatched content. A whole-object `override` takes effect in the patch phase
  at its mod's turn and discards earlier patches of that object. Path keys `<doc>/<field path>`, per-leaf origins,
  and a provenance ledger (`<mod>/provenance/*.prov.ron`, closed vocabulary, every tunable leaf covered, staleness
  judged per mod against its own pre-patch documents, never hashed; test fixture mods exempt); deprecated keys
  renamed during migrate (§6.1, §6.4).
- **Golden replay versioning (ADR-0021, C8):** replay format 4 (`ReplayStart::Scale { version, params }`, named
  semantic checks, a journal summary; format 3 still reads); `Change-Class: R|D|F|N|E|X` trailers on every code or
  content commit; `SIM_VERSION` bumped by every class F change, at most once per task (2 → 3 in Task 13, → 4 in
  Task 15); MCR entries in `docs/design/changelog-mechanics.md` written before the code; re-bless only through
  `fl_cli rebless --reason MCR-nnn`; the `change-class` and `golden-policy` gate rows, with golden bytes changing
  under class R or E only in the commit that changes the replay format and only with hash streams identical to that
  commit's parent (§5.5, §12.5).
- **Verification from M2 (ADR-0022; C1, C3, C4, C9, C10, I2, M17, R139, R140):** mechanic specs under
  `docs/design/mechanics/` (protected) with `AREA.NAME.N` rules, `cargo xtask spec-lint` and `trace` with `@req`
  tags; a test author with acceptance tests and a 10–20 % holdout (`holdout` feature; best-effort secrecy through
  the `guard-read` and Bash hooks, and tools that report holdout as counts only); Wilson at
  α = 1e-4 in two tiers with `FL_SEED_BASE` rotation at nightly; copy-mode `cargo xtask mutants` (per task at review,
  in-diff at pr, sharded at nightly, triaged at milestone); review tiers P/A/B/C and `cargo xtask mini-gate`; a
  `release-determinism` row; the task gate minus `xtask/heavy-tests.txt` with level budgets (§12, §13).
- **Runner (ADR-0002 M2 prelude refinement):** `FromRunner` is `#[non_exhaustive]`, and `ToRunner::reply_id()`
  lets the threaded runner answer an unsupported message with `SimReply::Error` (M3, R123) (§5.3).
- **Saves (ADR-0006 M2 prelude refinement):** `Codec::migrate_with(version, fn(Old, &LoadCtx) -> Result<T,
  String>)` and `LoadCtx::current_id(table, name)`; core version 3 (Task 13: the order-queue migration and a
  `core.activity_stack` fill) and 4 (Task 15: path queue and path follow v2); relationship codecs save `SimId`s
  (§5.8).
- **Commands (ADR-0007 M2 prelude refinement):** `MAX_COMMAND_UNITS` moves to `fl_protocol`, where the client reads
  it; a handler with no map set up rejects with `RejectReason::Feature("core.no_map")` instead of
  `core.no_valid_units` (M14) (§5.5).
- **Path requests (ADR-0015 M2 prelude refinement):** two FIFO path lanes, command-issued requests (enqueued in
  `Phase::Input`) before background ones, with at least one background request processed per tick when its lane is
  not empty (M5); a flow unit falls back to a single path only after 10 consecutive ticks without moving (M4); a
  value reload that changes nav content invalidates every followed path (M1); `core.passability` takes
  `since_version` (M8); `core.paths` answers `{"state": "immobile"}` for a unit without `Mobility` (M7) (§5.7, §11).

## Revision 2.4 (2026-10-25, M1 implementation refinements)

M1 implementation refined the contracts below without a new decision. Each line only points at the accepted ADR
text or controller ruling (M1 SDD ledger, `R<n>`) that holds the detail; where this list and an older paragraph of
this spec disagree, the named ADR or ruling wins.
- **Terrain and maps (ADR-0014):** a map grid has at most 4,096 cells per side, checked once before any allocation
  (R89); layer PNG paths are relative to the map file, without `..` (R90); `NavGrid::build` takes `MapInfo.cell` and
  `PathRules` live in `fl_sim::nav` (R92). An M0 save loads under M1 through `Codec::fill_missing`, and every
  milestone's integration task commits an old-save fixture from the previous milestone tag (R81, R83).
- **Pathfinding (ADR-0015):** the A* endpoint, side-cell and expansion-count semantics (R94); smoothing by galloping
  plus binary search with a pinned bound on line-of-sight cells (R93); one HPA* entrance node per border cell (R96);
  partial plans and their extra in-cluster search (R97); a direct-line plan's cost is an octile lower bound (R99);
  the same-cluster detour check and the guaranteed cost bound `optimal + crossings × 16 × c_max` (R136); the
  per-request buffers and graph densification stay parked until their trigger (R98, R125).
- **Flow fields and caches (ADR-0015):** cell-centre flow steering with a line-of-sight lookahead (R111); read-miss
  rebuilds are not charged to the path budget (R112, corrected by R124); the flow cache evicts only at the tick
  boundary, never on a read, with `Path.flow_cache_hard_max` (R124, R129); after a rebuild-load re-plan, units drive
  on single paths (R130).
- **Orders (ADR-0007 M1 refinement, §5.5, §7):** every wire-supplied `Fx` is clamped or validated at handler entry
  (R104); a queued rotate faces from the last queued move target (R105); the follow re-plan rule (R106); a queued
  follow blocks the queue behind it (R107); formation slot spacing wins over `formation_max_spread_m` (R110).
- **Steering and movement (§7):** the idle push cap per `idle_every` period (R113); the slide order whole →
  unpushed → x → y → stay (R114); movers push idle units aside (R115); units stranded on blocked cells re-plan every
  10 ticks (R116); where `move.blocked` and `steer.pairs` are budgeted (R117, R127).
- **Hot reload (ADR-0012 M1 refinement):** `HashInterval` is not refreshed by a reload (R120); a def component removed
  from content stays on live units (R121); no reload in a lockstep match (`NetDriven`, or no `LocalDriver`) (R133); an
  ID-set rebuild refreshes the client's snapshots and views and keeps the AI host, replay capture, input log, journal
  and scenario (R131, R135); the terrain view follows `debug.set_terrain` through the journal, so only in debug
  builds, until an always-on sim event lands before M7's bridges (R134).
- **AI seam (ADR-0016):** the AI host thinks again whenever the newest tick changes, including a restart at an
  earlier tick after a load (R91).
- **Client input (§10):** the rotate order is R and the camera snap is Home, both gated by `InputGate` (R87);
  selections over 256 units are split into commands of at most 256 units in `SimId` order (R88).
- **Verification (ADR-0009, ADR-0010):** the perf baseline is taken on an idle machine with the previous milestone's
  binary interleaved (R128); `flat_3000` keeps non-path counters gated (R125); determinism tests never read the
  journal (R139); proptests use a fixed seed, committed regressions and `PROPTEST_CASES` (R140), and heavy whole-match
  proptests cap their case count (R142); every M1 system is ordered, so the shuffle runs guard schedule changes
  (R138).

## Revision 2.3 (2026-10-24, M1 contracts)

The M1 plan's contract task (M1 Task 1) records these contracts; where this list and an older paragraph of this spec
(or of revisions 2.1 and 2.2) disagree, this list wins. The ADRs and refinements named here were accepted at that
task's review (rulings R69, R84).
- **ADR-0014** Terrain is a built-in `*.terrain.ron` kind (`TerrainDef { id, name, pass, blocks, color }`), and maps
  name a height PNG and a terrain PNG (one pixel per cell, row 0 north) that `fl_content` alone decodes into integer
  `MapLayers` covered by the rules hash. `MoveClass` moves to `fl_content`. Movement profiles are `(class, size)`;
  blocking is per class (terrain `blocks`, a slope over `Movement.max_slope_m`, off-map cells), with clearance per
  size. The speed formula of §7 gains a clamp to `[Movement.min_speed_frac, 1]`, and `terrain_factor` widens to
  [−1, 1]; path costs use a per-class `Movement.path_terrain_factor`. Terrain is Hashed state (`core.terrain`), and
  bridges are terrain cells until M7 (§6.2, §7).
- **ADR-0015** Path requests enter a Hashed FIFO `PathQueue`, processed under `Path.budget_per_tick`; each request is
  charged its cold cost and applied in request order, and stale requests are discarded at charge 0. HPA* uses
  16 × 16-cell clusters per profile with fixed entrance and tie-break rules; group moves use flow fields with lazily
  built tiles. `NavGrid`, `HpaGraphs` and `FlowCache` are `Derived` and outcome-neutral (a cache read never observes
  a miss, R84), rebuilt by core's one rebuild; a terrain change invalidates only the clusters it touches. Per-tick per-unit results live in a Transient
  scratch resource, never a Transient component (§5.7).
- **ADR-0015** Debug queries: `SimBuilder::register_debug_query` and `SimQuery::Debug { name, args }` →
  `SimReply::Debug(Value)`, read-only JSON with `Fx` as raw `i64`, used by the overlays, BRP `fl.debug_query` and
  `fl_cli query` (§5.3, §11).
- **ADR-0016** `fl_view::ai::{PlayerView, AiController, NullAi, ai_controller}`; the client builds `PlayerView` from
  the snapshot until M3 moves it into `Publish`, and `fl_client::ai::AiHost` hosts `Controller::Ai(id)` players,
  issuing their drafts through `LocalDriver` as that player (§5.5, §8).
- **R78** `Codec::fill_missing` fills state an older-feature-version save lacks; core is version 2 in M1 and fills
  `core.terrain`, `core.path_queue` and `core.path_follow`, so M0 saves load under M1 (§5.8, ADR-0006 M1
  refinement).
- **R77, R79** New payloads `core.rotate { toward }`, `core.follow { leader }` and `debug.set_terrain { min_cell,
  max_cell, terrain }` (no `FORMAT_VERSION` bump; command ids remap by name). `MAX_WAYPOINTS` bounds every queued
  order kind (`core.queue_full`), and a target the rules forbid is `RejectReason::Feature("core.invalid_target")`
  (§5.5, ADR-0007 M1 refinement).
- **R80** Hot reload polls mod-file modification times instead of using Bevy's `file_watcher`; value reloads reach
  the runner as `ToRunner::Control { .., RunnerControl::ReloadContent(..) }`, answered `SimReply::Reloaded`, and
  `ToRunner` becomes `#[non_exhaustive]`. A value reload runs every feature's `Derived` rebuild; ID-set rebuilds load
  with `LoadOptions { drop_unknown_units: true }`; the client captures replays (`replay save`), and
  `DefComponent::reload` keeps state ratios (§6.4, ADR-0012 M1 refinement).

## Revision 2.2 (2026-10-24, final M0 review refinements)

The final M0 review (`final-review-m0.md`) refined these contracts; where this list and an older paragraph of this
spec (or of revision 2.1) disagree, this list wins.
- `MatchHeader.features` is in feature load order (the dependency topological order, core first, ties by id), not
  sorted by id (§5.5, ADR-0007).
- **R70** Save loading needs the same feature ids, each saved at a version no newer than the build's; codec versions
  and migrations decide loadability, and the format guard fails a codec bump without a migration from the committed
  version; replays and the handshake stay strict on feature versions (§5.5, §5.8, ADR-0006, ADR-0007).
- **R71** `RejectReason` is non-exhaustive with `Feature("<feature>.<reason>")`; rejections are reported by stable
  code (`"core.late"`, feature codes verbatim), never by `Debug` name (§5.5, ADR-0007).
- **R72** The framework enforces `Transient`: Housekeeping removes `Transient` components and resets `Transient`
  resources (unless kept with a named reason), and `step_tick` clears every message buffer at the end of the tick
  (§5.4, §5.6, ADR-0006).
- **R74** `Derived` state has one rebuild per feature (`SimBuilder::register_rebuild`), run after a load in feature
  load order (§5.8, ADR-0006).

## Revision 2.1 (2026-10-24, M0 implementation refinements)

M0 implementation refined the contracts below without changing a decision. Each line names the controller ruling
(SDD ledgers `R<n>`) and the ADR it refines; where this list and an older paragraph of this spec disagree, this list
wins.
- **R5** `Fx`/`Angle` serde is lossless in binary formats (raw integers when not human-readable) (ADR-0007).
- **R15** `SimIdAllocator.next` is hashed state, so spawn order cannot hide in the allocator (ADR-0004, ADR-0006).
- **R20** The planner's resolutions of spec gaps: the runner's one allowed `thread::spawn`; phase markers are
  `fl_sim::phase` events that `fl_observe` timestamps; an `Early` rejection; content names in payloads; `Class::hashed(1)`;
  `*.new` files plus `xtask bless`; an `fl_game` loader callback behind `fl.load*` (ADR-0002, ADR-0005, ADR-0006,
  ADR-0007, ADR-0008, ADR-0009).
- **R19, R31, R59** Build output goes to a per-worktree `build.build-dir = "C:/t/fl/{workspace-path-hash}"` in the repo
  `.cargo/config.toml`; lane worktrees may reuse one build dir in turn; sccache keeps no `SCCACHE_BASEDIRS`, since a
  cross-worktree hit bakes another worktree's `CARGO_MANIFEST_DIR` into test binaries (ADR-0013).
- **R23** `gate task` builds the workspace once with unified features and filters tests by package; the per-package
  headless builds run at `gate pr` and above (ADR-0009).
- **R26** A `fl_feat_x_client` crate may reach `fl_sim` transitively through `fl_feat_x`; `arch-check` checks direct
  edges (ADR-0001).
- **R33, R34** The gate's diff base is `--base` > `FL_GATE_BASE` > `.gate-base` > merge base with `main`; the
  `protected` gate row fails changed protected paths unless each touching commit carries `Blessed:` (ADR-0009).
- **R36, R37** Hooks call a prebuilt xtask binary and fail closed; a subagent with no `agent_type` is the lead, an
  unknown one an implementer (ADR-0009).
- **R41** The runner's view channels encode with postcard, never JSON, so `Fx` stays exact (ADR-0002).
- **R42** The client interpolates on the receive timeline (time since the current snapshot arrived over the effective
  tick period; `t = 1` while paused), one tick behind the sim (ADR-0002).
- **R43** `probes/` crates are historical evidence pinned to their commits, excluded from the gate (ADR-0013).
- **R44** `debug.*` commands are issued as the local player; the sim skips the ownership filter for `DEBUG_ONLY` kinds,
  which stay gated by `cheats_allowed` (ADR-0008).
- **R45, R49** Every accepted or rejected non-duplicate command advances `core.last_seq` (hashed), except `Late` and
  `Early` rejections, so the correct on-time delivery of the same command is not a `DuplicateSeq` (ADR-0007).
- **R46** Every hook sub-set has a private core slot that runs first and a public feature slot (ADR-0005).
- **R47** Message buffers are added by name, `add_message::<M>("<feature>.msg.<x>")`, and classified `Transient`
  under it (ADR-0004, ADR-0005).
- **R51** `step_tick` copies the tick's `SimEvents` into its `TickOutput` (ADR-0002, ADR-0005).
- **R52** Hashed and saved state names factions by their content id, never a dense id (ADR-0004).
- **R53** Entities are filed into the spatial grid in the spawn path and removed in the despawn path, so a world
  loaded right after a spawn equals the live one; `Perceive::Spatial` handles relocations (ADR-0006).
- **R54** The state hash is name-keyed and prefix-free: counts of present `Hashed` resources and components, each
  written as its name key; absent state contributes nothing (ADR-0004).
- **R56** "Every sim log record carries the tick" holds in `log.jsonl` (the JSON layer tracks the `sim_tick` span);
  human stderr output may omit it (§15 A).
- **R60** `ScaleMix` is `Hashed` with a codec: state that drives gameplay is saved (ADR-0006, ADR-0010).
- **R61** `fl_game --stress N` runs `ScaleScenario::with_units(N)`, the full entity mix (ADR-0010, §5.7).
- **R62** `REPLAY_FORMAT_VERSION` is 2 (`ReplayStart::Scale` layout) and the golden replay was re-blessed (ADR-0007,
  ADR-0011).
- **R65** In M0 the controller dispatched general-purpose implementers instead of the role agents, whose frontmatter
  hooks resolve against the session's project directory; the gate rows still enforce the rules (ADR-0013).
- **R66** `xtask/api-toolchain.txt` names the undated `nightly` in M0 (ADR-0011).
- **R68** The `gpu` and `brp` gate rows build `fl_client`/`fl_game` with `debug_remote` and exclude `probe_*`
  (ADR-0009).

---

## 1. Intent

We're building a clone of the RTS **Blitzkrieg** (Nival, 2003) in **Rust on Bevy 0.20**, with a **3D isometric** view.
The first iteration reproduces Blitzkrieg's mechanics and systems faithfully. That gives us a solid, proven foundation
to extend later with systems Blitzkrieg never had: **campaigns, tech trees, base building and colony management**.

The first playable build is a **skirmish map that doubles as a testbed**. It is not a throwaway demo: every mechanic
gets built, tuned and tested on it.

**The most important quality is extensibility.** New units, weapons, factions and maps go in as data. New mechanics and
game systems go in as new plugins, without editing existing ones. If something works but makes extension hard, it is a
failure.

## 2. Goals, non-goals, anti-goals

**Goals**
- G1. Reproduce Blitzkrieg's core mechanics, following the formulas in `docs/research/blitzkrieg-mechanics.md`, delivered in milestones (§9).
- G2. Extensible by construction: content lives in data (RON), mechanics live in plugins, and registering a new feature takes one line.
- G3. A deterministic, fixed-tick simulation whose only input is player commands, so lockstep multiplayer and replays can be added later without a rewrite. Single player already uses the lockstep command envelope (§5.5).
- G4. A skirmish testbed with strong debug tools: spawn anything, switch faction, overlays, pause/step, speed control, hot reload of stats. Debug actions that change the sim go through commands (§11).
- G5. Free assets: CC0 first, CC-BY with credits. Primitive shapes stand in until art lands, and swapping art never touches code.
- G6. The sim can be tested headless (no window, no GPU), with automated determinism checks.
- G7. **Large scale**: thousands of units. Target: **3,000 units, 4 players, a 2×2 km map**, measured as an entity mix: **3,000 units ≈ 8–12k sim entities** (squad members, crews, wrecks) **plus projectiles in flight**. The sim tick takes ≤ 15 ms on the dev machine for that mix (20 Hz leaves a 50 ms budget), and the game renders at ≥ 60 fps on a mid-range PC. Later layers such as colony management must fit in the same tick budget. Scale is a design input from M0, not a later optimization (§5.7).

**Non-goals for iteration 1.** Each of these needs an extension seam now (§8).
- No AI opponent: the player commands both sides. Adding an AI-driven faction later must be easy.
- No networking code, only the architecture that makes lockstep possible later. (An in-process loopback lockstep driver for tests is architecture, not networking.)
- No campaigns, tech trees, base building or colony management yet.
- No mission scripting language (Lua etc.) yet. Data-driven triggers are enough for the testbed.
- No map editor. Maps are RON files plus image layers.

**Anti-goals: things that make this a failure even if it technically works**
- Adding a mechanic or system means editing core crates or a central enum. Unit types must never be a closed Rust enum.
- Sim logic reads render or input state, or uses wall-clock time, floats or unordered iteration. Any of these breaks determinism.
- Input or debug code changes unit state directly instead of issuing commands.
- Blitzkrieg-specific assumptions are baked into the core, such as "no building produces units" or "the only resource is supply RU".
- The "Blitzkrieg" name or branding is used, or Nival's code or data files are copied.

## 3. Constraints

- **Engine:** Bevy **0.20.x**, released 2026-10-08, pinned exactly (`bevy = "=0.20.0"` or the latest 0.20 patch). Sim crates (`fl_sim` and every feature's sim crate) use only `bevy_app` and `bevy_ecs`. `fl_math`, `fl_protocol` and `fl_view` use no Bevy at all.
- **Toolchain:** Rust stable **≥ 1.97.1**, which is bevy 0.20's declared MSRV; pinned in `rust-toolchain.toml`. Edition 2024. A dated nightly is installed only for `cargo-public-api`'s rustdoc JSON; nothing builds with it (§12.7).
- **Location:** `C:\Projects\bevy-experiments`, as its own git repository. The folder currently sits inside the git repo at `C:\Projects`. We run `git init` here and add the folder to the parent repo's ignore list, with the user's OK.
- **Windows path length:** deep build-script paths hit linker error LNK1104 (MAX_PATH). Intermediate build output goes to a short, **per-worktree** path set in the repo `.cargo/config.toml` (ruling R19; no user-level config change): `build.build-dir = "C:/t/fl/{workspace-path-hash}"`. The repo config does not hard-code a shared `target-dir` (§13.3). Keep crate names short.
- **Platform:** Windows 11 is the dev machine. Avoid Windows-only code; Linux and macOS should build.
- **CI:** local only, by user decision (B8 skipped). `cargo xtask gate <level>` is the single entry point (§12.1).
- **Licensing:** Nival published the original's source (github.com/nival/Blitzkrieg) under a non-commercial license. We reproduce its **rules and formulas** (game mechanics aren't copyrightable) and never copy its code, data files or assets. Our stats are our own numbers, using theirs only as a reference for realistic scale.
  **Source use rule (user decision):** agents *may read* the nival/Blitzkrieg source to understand a mechanic or fill a research gap. They must **never copy** its code, data files or assets, and never paste verbatim stretches of it into our repo. Findings go into `docs/research/blitzkrieg-mechanics.md` in our own words, with file references.
- **Assets:** CC0 preferred. CC-BY only with a credits entry. Nothing licensed NC, ND or SA, and nothing unlicensed. `cargo-deny` applies the same rule to crate licences.
- **Determinism:** fixed-point math in the sim, keyed RNG, ordered order-sensitive operations (§5.4).
- **Shaders:** Bevy 0.20 custom shaders are **WESL** (`.wesl`); GLSL was removed. This applies to fog of war and terrain materials.

## 4. Decisions made by the user

- Bevy 0.20 (not 0.2.0).
- The first playable build is a skirmish map that serves as the testbed for iterating on systems and mechanics.
- No AI at first, but adding an AI-driven faction later must be easy.
- Multiplayer will be needed in later stages.
- Iteration 1: reverse-engineer Blitzkrieg and implement its mechanics and systems. Later: campaigns, tech trees, base building, colony management.
- Free assets wherever assets are needed.
- Subagents run research, design, implementation and testing in parallel.
- The project lives in `C:\Projects\bevy-experiments`.
- Agents may read the original source for rules only, never copying code or data (§3).
- Iteration 1 ships a **small test roster**: two factions (Germany, USSR) and about 12 unit types, enough to exercise every mechanic. The full historical roster comes later as data-only work.
- **Large scale**: thousands of units, with room for colony management in the same tick later (G7, §5.7).
- **Baseline upgrade (2026-10-23):** adopt every recommendation in `docs/research/baseline-recommendations.md`, staged (A and B in M0, C as M2 entry requirements, D later with triggers).
  - B8 (remote + 3-OS CI) is skipped; CI stays local.
  - Repo-local Claude config is approved: `CLAUDE.md` plus path-scoped rules, hooks in `.claude/`, role subagents, a project skill and a project `.mcp.json` for `bevy_brp_mcp`.
  - Global installs are approved: cargo-nextest, cargo-insta, cargo-deny, cargo-public-api, cargo-mutants, sccache, bevy_brp_mcp.

Everything else below is the designer's call, marked as such where it is a judgement.

---

## 5. Architecture

### 5.1 Workspace layout (designer's call)

```
bevy-experiments/
├─ Cargo.toml                  # virtual workspace; [workspace.dependencies]; [workspace.lints]; bevy pinned;
│                              #   default-members exclude probes/
├─ .cargo/config.toml          # rust-lld on Windows MSVC, `xtask` alias; no shared target-dir (§13.3)
├─ .config/nextest.toml        # "agent" profile: compact output, JUnit
├─ deny.toml                   # cargo-deny: licences, advisories, bans (bincode, bevy_dynamic_plugin), dup bevy_*
├─ OWNERS.toml                 # path → track; contract paths (§13.2)
├─ CLAUDE.md  .claude/ .mcp.json   # agent config (§13.3)
├─ xtask/                      # cargo xtask gate | arch-check | test-guard | check-scope | gen-clippy | bless | api
├─ probes/                     # throwaway Bevy API compile probes; never a default member
├─ crates/
│  ├─ fl_math/                 # Fx fixed point, FxVec2/FxVec3, Angle, trig tables, keyed RNG `roll`, Pcg32 for
│  │                           #   tools only. No bevy.
│  ├─ fl_protocol/             # Tick, PlayerId, SimId, interned type IDs, the PlayerCommand envelope, command
│  │                           #   payload data types, MatchHeader, string tables. No bevy.
│  ├─ fl_view/                 # NEW: SimSnapshot, SimEvent, view channels, PlayerView (later), and the runner
│  │                           #   channel protocol (TickInput, TickOutput, SimQuery). No bevy.
│  ├─ fl_content/              # RON schemas, def-component registry, mods, inheritance, interning,
│  │                           #   link pass, validation, canonical rules hash. Pure `load_dirs()` usable headless.
│  ├─ fl_sim/                  # the simulation: SimTick phases, SimBuilder + registries, SimRunner + step_tick,
│  │   └─ map/ orders/ movement/ combat/ vision/ spatial/ lifecycle/ save/ runner/
│  ├─ fl_catalog/              # the list of linked SimFeatures; shared by fl_game, fl_cli, fl_test_utils
│  ├─ fl_observe/              # headless logging setup, profiler layer, journal sink (§15)
│  ├─ fl_client/               # presentation: camera, input→commands, selection, view/interpolation, LocalDriver
│  │   └─ camera/ input/ selection/ view/ fog/ ui/ audio/ vfx/ debug/
│  ├─ fl_game/                 # binary: GameBuilder; wires SimRunner to the client; the ClientFeature list; testbed
│  ├─ fl_cli/                  # headless binary: data-check, run-scenario, replay-verify, inspect, bench, …
│  └─ fl_test_utils/           # ScenarioBuilder, TestDriver helpers, fixture content, assertions, fitness tests
├─ features/                   # optional mechanic crates — the extension path for new systems
│  ├─ infantry/{sim,client}    #   fl_feat_infantry + fl_feat_infantry_client: squads, formations, prone, garrison
│  ├─ artillery/{sim,client}   #   indirect fire, spotting, ranging, counter-battery, crews
│  ├─ supply/{sim,client}      #   resources, depots, trucks, resupply, re-manning, morale
│  ├─ engineering/{sim,client} #   mines, wire, trenches, repairs, bridges, entrenchment
│  └─ towing/ air_support/ experience/ weather/ ...
├─ assets/
│  └─ base/                    # the base game is itself a mod
│     ├─ mod.ron rules.ron credits.ron
│     ├─ factions/ units/ weapons/ squads/ buildings/ terrain/ maps/ scenarios/ visuals/
│     └─ models/ textures/ sfx/ ui/ fonts/
├─ mods/                       # third-party data mods (same layout as assets/base)
└─ docs/                       # research, specs, plans, adr/, perf/
```

**Dependency rules.** Crate boundaries enforce these, and `cargo xtask arch-check` checks them one package at a time
(§12.2), because a workspace-wide `cargo tree` reflects unified features and hides violations.
- `fl_math` depends on nothing in the workspace. `fl_protocol` → `fl_math`. `fl_view` → `fl_math`, `fl_protocol`. None of the three depends on Bevy.
- `fl_content` → `fl_math`, `fl_protocol`, and `bevy_ecs` (for `DefComponent::instantiate`).
- `fl_sim` and every feature sim crate `fl_feat_x` use `bevy_app` + `bevy_ecs` only, plus `fl_math`, `fl_protocol`, `fl_view`, `fl_content`. **Never** `bevy_render`, `bevy_window`, `bevy_input`, `bevy_time`, `bevy_winit`, `wgpu`, the `bevy` umbrella crate, or `Time`.
- `fl_feat_x` may depend on another `fl_feat_y` only if `y` is in its `deps()` (§5.2).
- `fl_client` and every `fl_feat_x_client` use the full `bevy`, plus `fl_view`, `fl_protocol`, `fl_math`, `fl_content`. A `fl_feat_x_client` may also depend on its own `fl_feat_x` (for view, def and command payload types). **No client crate depends on `fl_sim` directly**, and none depends on another feature's sim crate.
- `fl_game` is the only crate that depends on both client crates and `fl_sim`: it assembles them.
- `fl_catalog`, `fl_cli`, `fl_test_utils` and `fl_observe` are headless: the sim rules apply to them.
- Feature sim crates don't re-export `fl_sim` types.

Start with these crates. Promote an `fl_sim` submodule to its own crate only when compile times or ownership call for
it. Core movement, combat and vision stay in `fl_sim` because almost every feature depends on them. Everything
Blitzkrieg layers on top goes in `features/`, so the extension path gets used from day one.

### 5.2 The feature contract (the extension point)

A feature is two crates and two traits (ADR-0001):

```rust
// fl_sim — implemented by fl_feat_x
pub trait SimFeature: Send + Sync + 'static {
    fn id(&self) -> &'static str;                       // stable, e.g. "infantry"; also the stable-name namespace
    fn version(&self) -> u32;                           // bump when sim semantics or save codecs change
    fn deps(&self) -> &'static [&'static str] { &[] }   // other SimFeature ids ("core" is implicit)
    fn build(&self, sim: &mut SimBuilder);              // registries + systems
}

// fl_client — implemented by fl_feat_x_client
pub trait ClientFeature: Send + Sync + 'static {
    fn sim_feature(&self) -> &'static str;              // id of the SimFeature it presents; must be present
    fn build(&self, app: &mut App);                     // visuals, UI panels, sounds, overlays, view decoders
}

// fl_game: one line per feature
GameBuilder::new()
    .sim_features(fl_catalog::features())               // InfantryFeature, ArtilleryFeature, …
    .client_feature(InfantryClient)
    .client_feature(ArtilleryClient)
    .content_mod("assets/base")
    .run();
```

- The traits use methods, not associated consts, so catalogs can hold `Vec<Arc<dyn SimFeature>>`. Each feature type also exposes `pub const ID: &str` for other features' `deps()` lists.
- **Ordering.** `GameBuilder` (and `build_sim_app` headless) sort features topologically by `deps()`, breaking ties by id. `core` is always first. Cycles, missing dependencies, duplicate ids and a `ClientFeature` without its `SimFeature` are errors. Bevy has no plugin dependency solver, so this is ours.
- **Identity.** The sorted `(id, version)` list goes into the `MatchHeader` (§5.5).
- **`SimBuilder`** is the sim extension API, and a contract (§12.7). It wraps the sim `App` and exposes typed registration. An `app_mut()` escape hatch exists for Bevy-level setup; review flags its use.

A feature can, through `SimBuilder`:
- Register **def components**: data keys that RON unit files can use (§6).
- Register **command kinds** with their handlers (§5.5). Payload data types live in the feature's sim crate under a `protocol` module, so its client crate can encode them.
- Add sim systems to the **phase hook sub-sets** (§5.6).
- **Classify every component and resource** it adds, with a save codec for hashed state (§5.4, §5.8).
- Register **RNG streams**, **journal records** and **view channels** (snapshot extensions and events).

And through `ClientFeature`:
- Client plugins: panels, overlays, VFX, sounds, and decoders for its view channels.
- **Command-panel buttons**, declaratively: id, icon, hotkey slot, the command kind, and which def component enables the button. From M2, buttons also carry a `requires` condition expression (C5).

**Plugin hygiene.** Registries are built in `build()` and frozen (sorted, dense IDs assigned) in `finish()`. Plugins never
reach into each other's internals; only contract types cross crates. Every crate sets `default-features = false` on Bevy
and lists its features.

A new mechanic is two new crates plus new RON keys on the units that use it. No core edits.
Rust dynamic plugins (DLLs) are explicitly out: Rust has no stable ABI, and `bevy_dynamic_plugin` is dead (and banned in
`deny.toml`). Features are compile-time crates.

### 5.3 Simulation / presentation split (designer's call; ADR-0002)

- **The sim lives in its own Bevy `World`**, built by `fl_sim::build_sim_app(features, content, setup) -> App`. It never shares a World with the client.
- **One tick function.** `fl_sim::step_tick(world, TickInput) -> TickOutput` advances exactly one tick (the driver steps are in §5.6). Every mode uses it; nothing else runs `SimTick`.
- **`SimRunner`** owns the sim World, in one of two modes with the same API:
  - **Threaded** (the client): a dedicated thread. `TickInput { tick, commands }` arrives over a channel, and `TickOutput` goes back.
  - **Inline** (tests, `fl_cli`, a future server): called synchronously on the caller's thread.
- **Outputs per tick** (`TickOutput`, defined in `fl_view`):
  - `tick`, and the state `hash` when one was due
  - `snapshot: Arc<SimSnapshot>`, double-buffered and recycled once the client releases it
  - `events: Vec<SimEvent>` raised during the tick, in order. The client drains every tick's events, so none are lost when several ticks run in one frame.
  - in debug builds, the tick's journal entries (§15 B)
- **`fl_view` is the client's only read model.**
  - The snapshot holds, per entity and sorted by `SimId`: unit type, owner, pose and heading, turret angles, HP fraction, state flags, and per-team visibility bits; later, visibility-grid deltas (M3).
  - Values stay in `Fx`/`Angle`/integers; the client converts to `f32`.
  - Features extend it with registered **view channels** (string-keyed, like def components). Their data types live in the feature sim crate's `view` module and are plain data, never `Component`s.
  - The sim's `Publish` phase writes it. `Changed<T>` filters are allowed there (and only there) to skip unchanged records.
- **Debug queries.** `SimQuery` (inspect entity, world summary, journal since tick, perf, save) is answered by the runner between ticks, read-only. The inspector, BRP and `fl_cli` use the same queries.
  - **Named debug queries** (ADR-0015). Features register read-only queries with `SimBuilder::register_debug_query(name, fn(&World, &Value) -> Result<Value, String>)`. `SimQuery::Debug { name, args }` answers `SimReply::Debug(Value)`, or `SimReply::Error` for an unknown name or a failed query. Replies are JSON with `Fx` as its raw `i64`; they are never hashed and never read by sim logic. `core.debug_queries` lists the registered names; M1 adds `core.passability`, `core.terrain_at`, `core.clusters` and `core.paths`. The overlays, BRP `fl.debug_query` and `fl_cli query` use them.
- **`TickDriver`** decides *when* to tick and *which commands* go into each tick:
  - `LocalDriver` (client): wall-clock accumulator, pause, single step, speed ×0.25–×8, input delay 1.
  - `LockstepDriver`: advances only when every peer's commands for the tick are present. M0 ships it over an in-process loopback `NetSession` with no sockets, to prove the path; a real transport is later work (§9.3).
  - `ReplayDriver`: feeds a recorded command stream as fast as possible, optionally stopping at tick T.
  - `TestDriver`: explicit steps, input delay 0 by default.
- **Each client frame:**
  1. The `LocalDriver` accumulates fixed steps and sends a `TickInput` per due tick, carrying the commands stamped for it.
  2. The client receives `TickOutput`s, keeps the last two snapshots, and drains events into VFX and sound.
  3. The view interpolates between the two snapshots using the overstep fraction. Interpolation already delays the view by one tick, so running the sim on its own thread adds no visible latency.
- **Tick rate:** 20 Hz (designer's call, classic RTS sim rate). A configurable constant.
- **Entity identity:** the sim uses `SimId(u32)`, assigned sequentially by the spawn funnel (§5.6). Commands, saves, replays, snapshots and hashes reference `SimId`, never Bevy `Entity`. The client maps `SimId → Entity` for its visual entities.
- **Failures.** A panic in a sim system re-panics in tests, CI and `fl_cli`. In a release client, the runner stops the match, marks it desynced and reports to the client; it never continues silently.

### 5.4 Determinism rules

- **Fixed-point math.** `Fx` wraps `fixed::I32F32`. Maps are ≤ 2 km and speeds and ranges fit easily. Trig uses lookup tables; sqrt is integer-based. Arithmetic is checked in debug builds.
- **Numbers in content.** RON files may contain decimals. They are converted to `Fx` once at load, which is deterministic.
- **Randomness: keyed and counter-based** (ADR-0003).
  - `fl_math::roll(seed, tick, sim_id, stream, n) -> u64` is a stateless hash of its inputs. `Rolls` is a short-lived local cursor over one `(seed, tick, sim_id, stream)` key that increments `n` per draw and offers `range_u32`, `chance`, `range_fx` and `unit_fx`.
  - `stream` is the stable 64-bit hash of a registered, namespaced stream name (`"combat.dispersion"`), never a dense ID, so adding a stream doesn't move any other stream's draws. The stream registry rejects duplicate names and hash collisions.
  - World-level draws use `SimId(0)`. Each stream name has one owner and purpose; `n` separates draws within it.
  - **Banned in sim crates:** any RNG state in a resource, component or `Local` (`Pcg32`, `rand`, `bevy_rand`). `Pcg32` stays in `fl_math` for content-time tools and the client.
  - Randomness is therefore no longer order-sensitive, and parallel phases need no special RNG rule.
- **Collections.** No `std::collections::HashMap`/`HashSet` or `bevy_platform` hash maps in sim state or in iteration that affects state. Use `BTreeMap`, a sorted `Vec`, or `IndexMap` with a fixed hasher.
- **Ordering (narrowed, ADR-0005).**
  - **Order-sensitive operations follow `SimIndex` (ascending `SimId`):** writes to shared state (resources, other entities), spawns, despawns and `SimId` allocation, budgets and queues (path requests, time-sliced work), and cross-entity reads of state written in the same tick.
  - **Entity-local updates** (a system reads shared data and writes only the iterated entity's own components) may iterate in any order, including `par_iter_mut`.
  - Cross-entity parallel work keeps the pattern: a *pure* phase writes per-entity results into a buffer indexed by `SimId`, then a sequential phase applies them in `SimId` order.
  - `SimIndex` (a sorted `Vec<(SimId, Entity)>`) is maintained by component hooks.
  - The branch test (§5.8), shuffle runs and (later) executor parity enforce this.
- **Executor.** `build_sim_app` sets the executor explicitly: single-threaded in M0. A multi-threaded executor is enabled only after ambiguity checks pass and an executor-parity test exists. Never rely on whichever `bevy_ecs` features got unified in.
- **Bevy hidden-state hazards.**
  - No `Local<T>` in sim systems unless it is registered and classified like a resource.
  - No gameplay logic on `Changed`, `Added` or `RemovedComponents`: change ticks are not hashed or saved. Use explicit dirty flags in hashed state.
  - Observers in sim crates only for order-independent reactions (a marker insert, a push into a buffer its consumer sorts), as the event-model ADR (ADR-0019) rules: Messages for gameplay events, hooks for indices and invariants only, no entity disabling.
  - No `Entity` in hashed or saved state, commands, journal or replays.
- **Banned in the sim:** `Time`, the wall clock (`Instant`, `SystemTime`), threads (`thread::spawn`), `ParallelCommands`, `std::env::var`, and float physics crates. Clippy enforces the list (§12.2).
- **Component classification (mandatory, ADR-0006).** Every sim `Component`, and every sim `Resource`, is registered as exactly one of:
  - `Hashed`: authoritative state, included in the state hash and in saves (with a versioned codec, §5.8).
  - `Derived`: rebuilt from hashed state on load (the spatial cell, effective-stat caches, `SimIndex`). It names its rebuild system.
  - `Transient`: cleared every tick; never hashed or saved (per-tick buffers, counters, the journal).
  - `External` (resources only): fixed at build from the `MatchHeader` (content, config, the cheats flag). The header identifies it, so the per-tick hash skips it.
  - At startup in debug builds and in every test, `build_sim_app` walks `world.components()` and fails on any type from a sim crate (`fl_sim::`, `fl_feat_*` but not `*_client`) that isn't classified.
- **State hash.** Computed by the driver after `SimTick`, not inside the schedule. Every N ticks (default 20; every tick in tests), xxh64 over: the tick, every `Hashed` resource by name, the entity count, then per `SimIndex` entry its `SimId` and every `Hashed` component by name. Registry-backed IDs are hashed by their **stable name key**, never their dense ID (ADR-0004). This catches desyncs in tests now and in multiplayer later.
- **Rules hash.** xxh64 over a **canonical binary encoding** of the resolved content (ADR-0007): a format-version prefix, then postcard bytes of mods in load order `(id, version)`, each content table sorted by id, components within an object sorted by name, maps as sorted maps. `Fx` is its raw `i64`; any presentation `f32` is its IEEE bits. Never `Debug` or RON text, and no `#[serde(flatten)]` in hashed content. Stored in the `MatchHeader`.
- **Stable-string registries for every runtime ID** (ADR-0004): command kinds, def components, sim components and resources, content kinds, rules sections, RNG streams, journal records and view channels (and, from M2, stats, conditions, tags, effects). Each registers a namespaced `&'static str`, is frozen in `finish()` by sorting the names, and assigns dense IDs. Content IDs (units, factions) follow the same rule. Replays and saves carry the frozen string tables; loading maps strings to current IDs and rejects (or migrates) unknown ones.

### 5.5 Commands, factions and controllers

- **The command envelope** (ADR-0007) is the **only** input to the sim:
  ```rust
  pub struct PlayerCommand {
      pub exec_tick: Tick,     // the tick it applies on: the driver's next tick + input delay
      pub player: PlayerId,
      pub seq: u32,            // per player, strictly increasing over the match
      pub units: Vec<SimId>,
      pub kind: CommandKind,   // (CommandTypeId, postcard payload)
      pub queue: bool,         // Shift-queue
  }
  ```
  - Within a tick, commands apply in `(player, seq)` order. A duplicate `(player, seq)` is rejected.
  - **Input delay** is a driver parameter: 1 tick in single player (`LocalDriver`), adaptive 2–6 in multiplayer later, 0 by default in `TestDriver`. Single player and multiplayer share this path.
  - A command whose `exec_tick` is earlier than the next tick to run is rejected as late.
  - Handlers tolerate dead or foreign `SimId`s: filter, then log.
  - **Wire format: `postcard`.** bincode is discontinued and banned in `deny.toml`.
- **`CommandKind` is open.**
  - Core set: Move, Attack, AttackGround, Swarm (attack-move), Stop, StandGround, Rotate, Follow, Guard.
  - Core payloads fixed so far: `core.move { target }`, `core.stop`, `core.rotate { toward }` and `core.follow { leader }` (M1). The debug payloads are `debug.spawn`, `debug.wander`, `debug.set_owner`, `debug.invulnerable`, `debug.set_hp`, `debug.teleport` and `debug.set_terrain { min_cell, max_cell, terrain }` (M1). Attack, AttackGround, Swarm, StandGround and Guard get their payloads with the milestones that need them. Adding a payload type needs no `FORMAT_VERSION` bump; saves and replays remap command ids by name (ADR-0007).
  - Features register their own kinds: Enter/Leave, Load/Unload, Formation, LieDown, Deploy/Pack, Resupply, LayMines, …
  - A kind is stored as `(CommandTypeId(u16), payload)`. Its ID comes from the sorted command-name registry (§5.4).
  - **Payload data types** implement `fl_protocol::CommandPayload` (`NAME`, plain serde data). Core payloads live in `fl_protocol`; feature payloads live in the feature's sim crate. **Handlers** (`unit_can`, `apply`) are registered in the sim with `SimBuilder::register_command`. Clients encode by name through the string table they receive with the match header.
  - Unknown kinds and undecodable payloads are rejected.
- **Validation in the sim.** A command is dropped and logged if the player doesn't own the units, or if a unit can't do that kind (it lacks the def component that enables it). Rejections carry stable codes: a target the rules forbid (a blocked cell, a missing leader, a rect outside the map) is `core.invalid_target`, and a queued order of any kind onto a unit holding `MAX_WAYPOINTS` (32) orders is `core.queue_full` (ADR-0007).
- **Debug commands** (ADR-0008). Testbed actions that change the sim are command kinds in the `debug.*` namespace: `debug.spawn`, `debug.set_owner`, `debug.invulnerable`, `debug.set_hp`, `debug.teleport`, `debug.wander`, `debug.set_terrain`, and so on.
  - They are accepted only when the match header's `cheats_allowed` is true; otherwise they are rejected and logged.
  - They may target any player's units. They are recorded in replays like any other command.
  - Lockstep peers must agree on `cheats_allowed`, because it is in the header.
- **`MatchHeader`** (in `fl_protocol`) identifies a match. The replay header, the save header and the future lobby handshake all use it:
  ```rust
  pub struct MatchHeader {
      pub format_version: u32,
      pub build_id: String,                  // crate version + git revision (+dirty); "dev" if unknown
      pub sim_version: u32,                  // fl_sim::SIM_VERSION; bump policy from M2 (C8)
      pub features: Vec<(String, u32)>,      // (id, version), sorted by id
      pub mods: Vec<(String, String)>,       // (id, version), in load order
      pub rules_hash: u64,
      pub cheats_allowed: bool,
      pub seed: u64,
      pub string_tables: StringTables,       // frozen registries, name lists in dense-ID order
  }
  ```
  - **Lockstep handshake** (later): everything must match exactly, `build_id` included.
  - **Replay verification:** features, mods, `sim_version` and `rules_hash` must match. A `build_id` mismatch is a warning. String tables remap IDs.
  - **Loading a save:** features and mods must match, or have migrations; per-component versions migrate (§5.8).
- **Factions and players.** A `Faction` is content: id, name, color, roster. A `Player` in a match has a faction, a team and a **controller**: `Human`, `Ai(ai_id)` or `None`. Visibility and ownership are per player, and teams share vision.
- **Controllers.**
  - The human controller turns input into commands.
  - A future AI controller runs **outside the sim** on a designated peer, reads a per-player `PlayerView` (in `fl_view`, only what that player can see) and emits the same `PlayerCommand`s through the normal delayed path. It gets no privileged access (§9.3).
  - Scripted test scenarios issue commands the same way.
- **Testbed.** A hotkey switches which player the local human controls; that is client-local. "Reveal all" is a **client render override**: the client draws every snapshot entity regardless of the local team's visibility bits. It never changes sim state or the hash.
- **Replays.** A replay is a `MatchHeader`, the scenario, the command stream, periodic state hashes, and any content-reload events (§6.4). It is stored as postcard; `fl_cli replay dump --json` is the readable view. Replaying reproduces the match exactly.

### 5.6 Sim schedule (ADR-0005)

`SimTick` runs eight phases in a fixed, closed, documented order:

| Phase | What runs there | Hook sub-sets (public) |
|---|---|---|
| `Input` | Housekeeping: message rotation, transient clears. Then decode, validate and apply this tick's commands | `Housekeeping`, `Validate`, `Apply` |
| `Think` | Orders → activities, target acquisition, unit-level reflexes | `Orders`, `Behaviour` |
| `Act` | Movement, firing intents | `Move`, `Fire` |
| `Resolve` | Projectiles, the effects pipeline (Generate → Modify → Apply from M2), conditions, modifier recompute | `Projectiles`, `Effects`, `Stats` |
| `Economy` | Resources, production, construction, research. Time-sliced; empty in iteration 1 | `Main` |
| `Perceive` | Spatial-index maintenance, vision | `Spatial`, `Vision` |
| `Lifecycle` | Death → wreck, the **spawn funnel**, despawn | `Death`, `Spawn`, `Despawn` |
| `Publish` | Snapshot, `SimEvents`, `PlayerView` (later) | `Snapshot`, `Events` |

- **Schedule settings.** `ambiguity_detection: LogLevel::Error`, `auto_insert_apply_deferred: false`, and an explicit executor (§5.4).
- **Explicit sync points.** Each phase ends with one explicit `ApplyDeferred`. Commands queued during a phase become visible in the next phase, whatever other features add.
- **Features add systems only into hook sub-sets.** Two sim systems share a sub-set only if ambiguity detection accepts it. Prefer `configure_sets` ordering to cross-crate `.before(system)`. Use `MessageMutator` for "modify in flight" chains.
- **The spawn funnel.** Every in-tick spawn is a `SpawnRequest` Message. `Lifecycle::Spawn` handles them in request order, assigns `SimId`s and instantiates defs. Nothing allocates `SimId`s in hooks, observers or unordered `Commands`. Scenario setup and save loading spawn through the same funnel code, outside `SimTick`.
- **Messages** are the default for in-sim gameplay events. They are rotated once per tick in `Input::Housekeeping`; a consumer reads in the tick the message was written.
- **Driver steps** (`step_tick`, outside the schedule):
  1. Put the tick's commands, sorted by `(player, seq)`, into the command inbox.
  2. Run `SimTick`.
  3. If a hash is due, compute the state hash and record it.
  4. Take the published snapshot, events and journal entries into the `TickOutput`.
  5. `world.clear_trackers()`.
  6. Advance the clock.
- The phase list, the sub-sets and the system order inside `SimTick` are part of the determinism contract. An insta snapshot of the built system order makes every change visible in review (§12.3).

### 5.7 Scale architecture (user decision: large scale)

Target from G7: 3,000 units (≈ 8–12k sim entities plus projectiles in flight), 4 players, a 2×2 km map (500×500 cells at
4 m), sim tick ≤ 15 ms, ≥ 60 fps.

- **Spatial index:** a uniform grid hash, maintained incrementally. A unit changes its cell membership only when it crosses a cell boundary, so the index is not rebuilt every tick. Each cell keeps its members sorted by `SimId`, which keeps query results deterministic. Range queries (target acquisition, splash, suppression, vision sources) go through it, never through all-pairs scans. The cell component is `Derived`.
- **Vision:**
  - *Incremental* per-player visibility with **reference counts per cell**. A vision source adds to the cells it sees and subtracts when it moves or dies.
  - Recomputation happens only when a source crosses a cell, changes height or stance, or the blocking map changes locally.
  - Line of sight uses precomputed shadowcasting templates per radius, checked against the absorption and height layers.
  - Updates are time-sliced across ticks with a per-tick budget.
  - Grids are stored per *team*, because allies share vision.
- **Pathfinding:**
  - **HPA*** with clusters (about 16×16 cells) from M1, per movement class plus clearance.
  - Path requests go through a **per-tick budgeted queue** (ADR-0015). A unit waits a tick or two rather than blowing the budget. Results are applied in request order.
  - **Cold-cost charge.** Each request is charged the work a from-scratch computation does (nodes expanded plus line-of-sight cells), whether or not a cache answered it. A request whose unit is gone, or no longer waits on it, is discarded at charge 0. The queue and each unit's path state are `Hashed`.
  - **Flow fields** for large group moves (`Path.flow_min_group` or more same-profile units) to one destination, cached per (destination cluster, movement profile): an abstract Dijkstra over the HPA* graph plus per-cluster tiles built lazily, in `SimIndex` order, before movement.
  - **Derived caches.** `NavGrid`, `HpaGraphs` and `FlowCache` are `Derived` and never change outcomes, because charges are cold costs and every cached value is a pure function of `Hashed` state; a cache read never observes a miss, because a read computes a missing value first (R84), so eviction may be any deterministic policy. Core's one `Derived` rebuild rebuilds them after a load.
  - Dynamic obstacles (wrecks, destroyed bridges, buildings) invalidate only the clusters they touch. In M1, bridges are terrain cells changed by `debug.set_terrain`.
- **Target acquisition and other AI-ish per-unit work:** staggered by `SimId % N` and computed in the parallel pure phase.
- **Data layout:** hot sim components stay small and `Copy` (pose, velocity, HP, weapon timers). Cold data (def references, names) lives in content lookups. Avoid per-tick allocations; reuse buffers. Transient flags use SparseSet storage.
- **Rendering at scale:**
  - Placeholders and models share meshes and materials, so Bevy can batch and instance automatically.
  - Distance-based LOD, with lower animation rates or static poses for far infantry.
  - Only visible-on-screen entities update their transforms from the snapshot.
  - The `Publish` phase writes only records that changed (§5.3). The M2 render baseline (C11) adds per-instance data and `VisibilityRange` LOD.
- **Scale scenario (entity mix).** It exists from M0 and is the benchmark input. At the 3,000-unit size it has:
  - 3,000 unit entities that wander (placeholders)
  - 6,000 follower entities (stand-ins for squad members and crews) that follow a leader, exercising cross-entity reads
  - about 1,000 projectile entities in flight, spawned and despawned through the spawn funnel at a steady rate, exercising churn

  The 500 and 5,000 sizes scale the same mix. Real squads and projectiles replace the stand-ins as features land.
- **Budgets are enforced by deterministic work counters**, not wall clock (§12.6; ADR-0010). Path nodes expanded, LOS cells visited, spatial-query candidates, entities spawned and allocations per tick are budgeted and snapshotted on the scale scenario. Wall-clock tick time is measured at milestones only.
- **Later layers** (economy, colony management) must register their systems with the same budget discipline: time-sliced, spatially indexed, never per-tick all-entity scans, and with work counters.

### 5.8 Save and load (ADR-0006)

- **Layout** (postcard bytes):
  ```
  header  MatchHeader                                   // §5.5, with string tables
  sim     { tick, sim_id_allocator, pending_commands[],
            resources[(name, version, bytes)],           // Hashed resources, sorted by name
            entities[SimId → [(component, version, bytes)]] }   // sorted by SimId, then component name
  ```
- **Codecs.** Every `Hashed` component and resource registers a versioned codec: `sim.register_component::<Suppression>("infantry.suppression", Class::Hashed { version: 1 })`. Bytes are postcard.
- **Migrations are typed.** Keep the old shape (`mod v1 { pub struct Suppression {…} }`) and `impl From<v1::Suppression> for Suppression`. Migrations are kept forever, or until an explicit save-break ADR.
- **No `Entity` in saves.** Relationships (squad, garrison, transport, towing, crew, ownership) are stored as `SimId`.
- **Loading** builds a fresh sim from the header's features and content, spawns entities in `SimId` order through the funnel code, restores resources and pending commands, then runs every `Derived` rebuild system. String tables remap dense IDs; unknown names are errors.
- **Fitness tests** (in `fl_test_utils`, run by the gates, §12.3):
  - **Round trip:** `save(load(save(w))) == save(w)`, byte for byte.
  - **Branch test:** run A to tick M. Run B to tick N < M, save, load into a fresh world, run to M. The state hash must match every tick. Loading re-creates archetype tables in a different order than the live run's history, so this also catches code that depends on table iteration order.
- **Scope in M0:** the format, the codec and migration registry, codecs for core components and resources, the two fitness tests, and `fl.save`/`fl.load` plus `fl_cli save`/`load`. Each feature adds its codecs as it lands.
- `moonshine-save` is not used for sim state: it has no versioning. It is fine for client settings.

---

## 6. Content and data

### 6.1 Def components (designer's call: OpenRA-style "bag of traits")

A unit, building or other object is a RON file with an id, an optional `inherits`, and a **map of named def
components**. The set of component names is **open**: plugins register them with
`sim.register_def_component::<ArmorDef>("Armor")`. The loader deserializes each value through a registry-driven,
type-erased `DeserializeSeed` (`erased-serde`). This needs a custom `AssetLoader` built after all plugins register;
`bevy_common_assets` can't do it, but is fine for fixed-schema files.

```ron
// assets/base/units/_templates.unit.ron
Unit(
    id: "base.abstract.medium_tank",
    abstract: true,
    tags: ["vehicle", "armored", "tracked"],
    components: {
        "Selectable": (),
        "Hull":     (hp: 160, footprint: (3.0, 6.0), hitbox_shrink: 0.5),
        "Mobility": (class: Tracked, road_speed_kmh: 40.0, terrain_factor: 0.5, turn_rate_deg_s: 40.0),
        "Vision":   (range_m: 350.0, eye_height_m: 2.5, camouflage: 0),
        "Crew":     (count: 5),
    },
)

// assets/base/units/de/pz3.unit.ron
Unit(
    id: "base.de.pz3",
    inherits: Some("base.abstract.medium_tank"),
    name: "Pz.Kpfw. III",
    components: {
        "Armor":   (front: (40, 55), side: (25, 32), rear: (20, 30), top: (10, 14), bottom: (10, 12)),
        "Turrets": [ (traverse_deg_s: 30.0, guns: [ (weapon: "base.kwk_5cm_l42", ammo: { "ap": 50, "he": 40 }, primary: true) ]),
                     (hull_mount: true, arc_deg: 30.0, guns: [ (weapon: "base.mg34", ammo: { "ball": 2000 }) ]) ],
        "Supply":  (reload_cost_per_round: 300),
    },
    remove: [],
)
```

Rules:
- **Inheritance resolves at load time, per component.** A child's component replaces the parent's whole. `remove: [...]` drops inherited components. Abstract templates are never spawned.
- **Strict parsing.** Every def struct uses `#[serde(deny_unknown_fields)]`, and an unknown component name is an error. Errors report the file and the field path.
- **Defs are separate from runtime components.** A def is authoring-friendly: floats, string refs, **real-world units** (metres, km/h, seconds, mm of armor). `DefComponent::instantiate(&self, &SpawnCtx, &mut EntityWorldMut)` converts it once at spawn into runtime components (`Fx`, ticks, grid cells, interned IDs). Shape it like Bevy's `Template::build_template` so a later BSN adapter is trivial.
- **IDs.** IDs are strings namespaced by mod (`"base.de.pz3"`). After all mods load, they're sorted and interned to dense `u16`/`u32` IDs, so every machine assigns the same numbers (§5.4).
- **Cross-references** (unit → weapon → shell, squad → members, faction → roster) are resolved in a link pass with path-aware errors.
- **Global tuning constants** live in `assets/base/rules.ron`: cover values, splash coefficient, dispersion ratios, camouflage timers, morale coefficients, and so on. No magic numbers in code.
- From M2, the loader gains explicit load phases, patch operations and `format_version` migrations (C6), and every value gets provenance (C2); ADR-0020 records the phases, the patch order, path keys and the provenance ledger.

### 6.2 Other content types

- **Weapon:**
  - Range min/max, aim time, burst size, rate of fire, relax time, dispersion at max range, trajectory class, AA ceiling.
  - A list of **shells**, each with: piercing ± random, damage ± random, splash radii (small, large), damage type (health, morale, smoke), track-break chance, projectile speed.
  - Follows the schema in the mechanics research §3.
- **Squad:** members (unit refs) and formations (multipliers plus a slot layout).
- **Building:** HP, armor per side, fire slots (position, direction, arc, cover, sight multiplier), entrances, a footprint passability grid and a vision-blocking grid.
- **Terrain type** (`*.terrain.ron`, ADR-0014): `TerrainDef { id, name, pass, blocks, color }`, that is, a passability factor `pass` in [0, 1], the movement classes it blocks, and its render color. Sight blocking (M3), entrenching (M7) and a render material join it later.
- **Faction:** id, name, color, roster.
- **Map:** `map.ron` (size, players, starting units, buildings, bridges, objectives/triggers) plus PNG layers for height and terrain type (ADR-0014).
  - `terrain: Some(MapTerrain(..))` names a 16-bit (or 8-bit, value × 257) grayscale height PNG and an RGB terrain PNG next to the map file, the height range in metres, and an RGB → terrain-id legend.
  - One pixel per cell; pixel row 0 is the north edge. The height PNG is 16-bit or 8-bit gray, the terrain PNG 8-bit RGB (or RGBA with alpha 255); other formats are errors, samples are read raw (colour chunks ignored), and a wrong image size, a colour missing from the legend or a duplicate legend colour is a content error naming the file. At most 255 terrain kinds may be loaded.
  - Only `fl_content` decodes the PNGs, once at load, into integer `MapLayers` (a sorted terrain-id palette, per-cell type indices and `u16` heights), in a field RON can't set; the rules hash covers every pixel. The sim and the client read `MapLayers`, never a PNG.
  - A map without `terrain` is open ground: pass 1, blocks nothing, height 0.
- **Scenario:** map + players + controllers + seed. The testbed loads a scenario. The scenario's seed is the match seed unless the match setup overrides it.

### 6.3 Visuals are separate from rules

- **Visual defs.** Each unit has a visual def in `visuals/`, keyed by unit id. It holds:
  - an optional glTF model path and a placeholder primitive spec
  - scale and rotation offsets
  - turret and barrel node names
  - animation clip names
  - a faction recolor flag
  - icon, sounds, and a credit id

  This follows `docs/research/assets.md` §8.
- **The sim never reads visual defs.** They are not part of the rules hash.
- **Spawning visuals.** The client spawns the glTF scene (in 0.20: `WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path)))`) when a model is set and loads. Otherwise it spawns the placeholder in faction color: cuboid hull + turret + cylinder barrel, capsule infantry, and so on.
- **Credits.** `credits.ron` lists every CC-BY asset and its required attribution. The in-game credits screen reads it.

### 6.4 Mods and hot reload

- **Mod manifest.** `mod.ron` holds `{ id, version, depends, load_after }`. Overriding an existing ID requires `override: true`, so accidental collisions are errors.
- **Hot reload** (dev builds; ADR-0012 and its M1 refinement):
  - It is testbed-only: the match must have `cheats_allowed`, and it is never allowed in a lockstep match.
  - **Watcher.** Content is loaded with `std::fs`, not the AssetServer, so the testbed polls mod-file modification times (`debug.ron`: `hot_reload`, `hot_reload_poll_ms`) instead of using Bevy's `file_watcher`. The console command `reload` forces one. A file that fails to load is reported, and the match keeps running.
  - Editing a RON file reloads the content and compares the **ID sets** of every registry and content table with the running match.
  - **Values only** (same ID sets): the driver applies the new `ContentDb` in place at a tick boundary. It sends the runner `ToRunner::Control { id, control: RunnerControl::ReloadContent(..) }`, answered with `SimReply::Reloaded { tick, rules_hash }`; `ToRunner` is `#[non_exhaustive]`. Stat components on live entities are re-instantiated through `DefComponent::reload`, keeping their state (HP ratio, ammo ratio), and every feature's `Derived` rebuild runs.
  - **ID set changed** (an id added, removed or renamed): the driver rebuilds the match from a snapshot. It saves (§5.8), builds a fresh sim with the new content, and loads the save through the string tables with `LoadOptions { drop_unknown_units: true }`. Entities whose type no longer exists are dropped and logged; survivors keep their `SimId`s. Neither path rewrites a running match's terrain: edited map layers show in the next match.
  - Every reload is recorded in the replay as a `ContentReload { tick, rules_hash }` event. The client captures the running match's replay (console `replay save <path>`). Such a replay is marked non-reproducible; `replay-verify` reports that instead of a divergence.
  - Visuals and models also reload.
  - Hot reload changes the rules hash, which is another reason it is testbed-only.
- **Data checking.** `fl_cli data-check <dirs>` validates all content and is part of every gate.

---

## 7. Game rules: the Blitzkrieg model (iteration 1)

Formulas and behaviour come from `docs/research/blitzkrieg-mechanics.md` §1.3–1.9. Every constant goes in `rules.ron` or a def.

- **Map and terrain**
  - A cell grid (designer's call: 4 m cells; the builder may adjust) with a height layer and a terrain-type layer.
  - **Speed** = `road_speed × clamp(terrain_pass + (1 − terrain_pass) × unit_terrain_factor, Movement.min_speed_frac, 1)` (ADR-0014). Roads have pass = 1. `unit_terrain_factor` lies in [−1, 1] (trucks are negative), so the clamp keeps the speed positive.
  - Each movement class (foot, wheeled, halftrack, tracked) has its own blocking: rivers and swamps are impassable, and deep forest is foot-only.
  - Bridges are destructible objects that connect cells. Until they arrive (M7), bridges are terrain cells (`base.bridge`) that `debug.set_terrain` can turn into water (ADR-0014).
- **Movement**
  - Grid pathfinding per movement class, with clearance for unit size: HPA* plus flow fields, with budgeted requests (§5.7). The `pathfinding` crate's A* can handle low-level refinement.
  - Local separation and steering. Units closer than their pair distance (both footprint radii, `max(width, length) / 2`, plus `Steering.separation_margin_m`) push apart by `(r − d) × separation_gain` each. Moving units steer every tick, idle units every `Steering.idle_every` ticks (1..=64, staggered by `SimId`). A push is capped at `max_push_frac × road speed` per tick it stands for: one tick for a moving unit, `idle_every` ticks for an idle one (R113). A step tries the whole move (drive step plus push), then the step without the push, then the whole move's x part alone, then its y part alone, else the unit stays (R114). A move counts only if every cell it sweeps is passable for the unit's profile: no diagonal corner cut and no hop over a thin wall. A mover's push on an idle unit is weaker than its step, so idle units in its way are pushed aside (R115).
  - Tracked and wheeled units turn the hull before moving.
  - Group moves keep relative formation offsets: each unit's slot is its offset from the group's centroid, scaled so the farthest is at most `Movement.formation_max_spread_m` from the target, clamped to the map. Slots then keep a minimum spacing, the largest footprint side among the units plus `Steering.separation_margin_m`: a slot closer than that to an earlier one (in `SimId` order) moves to the nearest free point of a lattice of that pitch around it. The spacing wins over `formation_max_spread_m`, so a dense group spans about √n × pitch (R110).
  - Shift queues move, rotate and follow orders (stop is never queued). A queued rotate faces its point from the last queued move's target, else from the unit's position.
  - Follow keeps the follower's offset from its leader, shortened to `Movement.follow_distance_m` (exactly on the leader: that distance behind its heading). A follower with no path plans at once; otherwise it re-plans at most once per `Movement.follow_repath_ticks`: while driving when its goal (leader + offset) has drifted more than one cell, when stopped if it has drifted more than `max(Movement.arrive_m, cell / 2)` (so separation nudging an idle leader re-plans nothing), and after a failed plan always. A follow ends when its leader is gone.
- **Firing cycle**
  - Aim time, then a burst of N rounds at the rate of fire, then relax time.
  - Aim and relax times are multiplied by a random factor in [1, 1.2].
  - Tanks don't fire their main gun while moving; machine guns can.
  - Turrets rotate at their own traverse rate. Hull-mounted guns fire only within their arc.
- **Hit resolution**
  - Impact = aim point + a random point in an ellipse.
  - Radius = `dispersion × distance / range_max`. Elongation depends on trajectory class: cannon about 3:1 along the line of fire, howitzer about 1.5:1, and so on.
  - It's a hit if the impact lands inside the target's footprint rectangle, shrunk by `hitbox_shrink`. There is no separate to-hit roll.
- **Armor side**
  - Front, side or rear, from the attack direction relative to the target's rectangle.
  - Top for arcing trajectories and for attackers above the target. Bottom for mines.
- **Penetration:** piercing ~ U(piercing ± random) against armor ~ U(min, max) for the side hit. It penetrates if piercing ≥ armor **and** the target isn't saved by its cover roll.
- **Damage**
  - On penetration: HP −= U(damage ± random).
  - **HE splash** on a miss: units inside the small radius whose side armor is ≤ `armor_for_area_damage` (15) take damage × `area_damage_coeff` (0.3). Standing infantry inside the large radius take it too.
- **Track break:** a non-lethal hit on a tracked unit may break its tracks (per shell chance). The unit is immobilized until repaired.
- **Destruction:** a destroyed unit becomes a wreck, which blocks movement and despawns after a timeout.
- **Ammo:** counted per gun, per shell type. A unit requests resupply below 1/3.
- **Target acquisition**
  - Auto-engage the best visible target in range, scored by weapon-vs-target suitability and threat. Ties break by (distance, SimId).
  - Re-checks are staggered by `SimId % N`.
  - Stand Ground means no auto-engage and no chasing. Swarm means moving while engaging.
- **Vision**
  - A per-player tile visibility grid, recomputed every few ticks.
  - Sight radius plus a height bonus. Forests and buildings block sight via per-cell absorption.
  - **Camouflage:** after idling, the distance at which a unit is detected shrinks by its camouflage value. Firing reveals the unit with a per-class probability.
  - Enemies that leave sight are remembered as ghosts at their last known position, then forgotten after a timeout.
  - Teams share vision.
- **Infantry**
  - Squads of individual soldiers that move and fight as a group, with selectable formations (multipliers on speed, dispersion, rate of fire, cover).
  - Soldiers go prone automatically when fire lands nearby: cover 0.5, speed ×0.4, for a few seconds.
  - Grenades at short range against vehicles (side/rear armor, tracks).
  - Can garrison buildings and trenches through fire slots. Occupants get the building's protection and leave when it drops to 10% HP.
- **Artillery**
  - Howitzers and mortars fire indirectly at any target a friendly unit can see, or at the ground (less accurate).
  - Minimum range.
  - Ranging an area reduces dispersion inside it.
  - Counter-battery: each shot from a gun shrinks a "sound circle" that shows the enemy roughly where the gun is.
  - Guns need crews. A gun without a crew becomes neutral and can be captured.
- **Supply**
  - A generic **resource** system; Blitzkrieg's RU is the first resource.
  - Depots: the main depot is infinite and capturable. Field depots are destructible and must chain back to the main depot.
  - Supply trucks carry RU. They auto-resupply units in their area, re-man squads and gun crews, and return to a depot to refill.
  - Being outside depot range lowers **morale** (0.1–1), which worsens dispersion, relax time and aim time.
- **Engineers** (later milestone)
  - Lay mines (AP/AT) and clear them.
  - Build barbed wire, trenches and AT obstacles.
  - Repair vehicles, buildings and bridges, and build bridges at preset spots.
  - Vehicles and guns can entrench: cover 0.5, but immobile.
- **Towing:** a vehicle can tow a gun only if its towing force ≥ the gun's weight. Hooking and unhooking take time, as do deploying and packing.
- **Air support:** recon, bombers, attack aircraft, fighters and paratroopers, called in from the map edge with cooldowns. AA guns have a ceiling.
- **Experience:** 4 ranks, gained from kills (by the killed unit's value). Each rank multiplies speed, relax time, dispersion, sight, range and rotation.
- **Others:** snipers (camouflaged while moving), officers with binoculars (cone vision while stationary), smoke shells, weather (shorter ranges, no aircraft), medics, storming buildings (close combat).

Where the research lists a gap (no model for tank crew casualties, morale decay code, depot radius, rank multipliers,
formation names), the builder fills it with a documented guess in `rules.ron` and marks it as a guess. From M2 the
provenance ledger records it as `Guess` (C2).

---

## 8. Extension seams for later systems (built now, used later)

- **AI-driven faction:**
  - Builds on the controller abstraction (§5.5) plus a per-player observation API (`PlayerView` in `fl_view`: visible units, own units, known map), built in the `Publish` phase from M3. Until vision exists, `PlayerView` is a stub the client builds from the snapshot (`PlayerView::from_snapshot`; ADR-0016).
  - An AI controller runs **outside the sim** on a designated peer and emits `PlayerCommand`s through the normal, delayed path; its commands are recorded in replays. It may use floats, threads and async planning, can't desync, and can be swapped without changing the rules hash. Unit-level reflexes (auto-target, prone under fire) stay in the sim (§9.3).
  - `Controller::Ai` exists from day one, backed by a do-nothing AI used in tests: `fl_view::ai::{AiController, NullAi}` (id `"null"`), hosted by `fl_client::ai::AiHost`, which thinks once per new snapshot tick and issues the drafts through `LocalDriver` as that player (ADR-0016). Where AI runs in a lockstep match is decided with the first AI faction (§9.3).
- **Multiplayer (lockstep):**
  - The groundwork is already in place: the command envelope with execution ticks and sequence numbers, a deterministic sim, state and rules hashes, the `MatchHeader` handshake data, `TickDriver` with a loopback `LockstepDriver`, saves for late join, and replays (§5.3–5.5, §5.8).
  - A later `fl_net` crate only has to exchange commands behind a `NetSession` trait. Candidates: a thin custom lockstep, or `lightyear`'s input-only mode, decided by a spike (§9.3).
- **Campaigns:**
  - A **meta layer** above the match. A `Campaign` state owns persistent data (core units with experience, unlocked units, player rank) and launches scenarios. It never lives in the sim `World`, and has its own small versioned save.
  - A match takes a `ScenarioSetup` and returns a `MatchResult` (outcome, surviving units, stats). Persistent units enter through the normal def path plus their saved-state codecs (§5.8).
  - Objectives and triggers are data in `map.ron` (condition → action lists). Lua scripting can come later.
- **Tech trees:**
  - Content-defined unlock graphs over unit and weapon IDs.
  - **Stat modifiers** are a core concept. Experience ranks, morale and formations are all modifier sources, and a tech tree becomes another one. The modifier and stat spec is an M2 entry requirement (C5).
- **Base building and production:**
  - Buildings are already entities with def components (HP, armor, slots, footprint).
  - Production adds `Producer`/`Constructible` def components, a build-order command kind, and resource costs, on top of the generic resource system and grid footprint placement. Their systems go in the `Economy` phase.
- **Colony management:**
  - More meta/economy features: population, production chains, and logistics built on the resource system and supply-truck logistics.
  - Nothing in the core assumes "combat only".

---

## 9. Milestones (iteration 1)

Each milestone ends with a playable testbed build, a green `cargo xtask gate milestone` and a short demo checklist.
Every milestone also adds journal records (§15 B), log targets, profiling spans and work counters (§15 D, §12.6) for the
mechanics it introduces; a mechanic without them is not done.

### 9.1 Milestones

- **M0: Foundation** (contracts first; Groups A and B)
  - **Workspace:** the crates of §5.1, including `fl_view`, `xtask` and `probes`; per-worktree build dir, rust-lld, sccache.
  - **Math and protocol:** `Fx` math with keyed RNG `roll`; the command envelope, `MatchHeader`, string tables and command payload data types.
  - **Content:** loader with the def-component registry, inheritance, link pass, `data-check`, stable-string registries and the canonical rules hash.
  - **Sim:** `SimTick` with the eight phases and explicit sync points; `SimBuilder`, `SimFeature`/`ClientFeature` and topological ordering; the spawn funnel; component classification and its startup check; `step_tick` with hashing and housekeeping in the driver; `SimRunner` (threaded and inline) and `TickDriver` (Local, Replay, Test, loopback Lockstep).
  - **Commands:** the envelope with input delay, `(player, seq)` order, and `debug.*` commands gated by `cheats_allowed`.
  - **Save/load skeleton:** codecs for core state, the round-trip and branch tests.
  - **Client shell:** an isometric orthographic camera (pan with WASD, arrows and screen edge; mouse-wheel zoom; rotate); a flat test map; units spawned from RON shown as placeholders, driven by `fl_view` snapshots with interpolation.
  - **Testbed:** debug console and spawn panel (`bevy_egui`) issuing commands; `ScenarioBuilder`; twin-run, shuffle runs, the system-order snapshot and replay round trips.
  - **Scale:** the incremental spatial grid; the entity-mix scale scenario at 500, 3,000 and 5,000 units with work-counter budgets; a render stress test of 3,000 placeholders at ≥ 60 fps.
  - **Verification baseline (§12, §13):** `cargo xtask gate`, the hooks, generated clippy bans, `arch-check`, cargo-deny, `test-guard`, protected paths, contract snapshots, ADRs 0001–0013, `OWNERS.toml`, and the agent environment.
  - **Early probes:** `dynamic_linking` with `bevy_ecs`-only sim crates; `bevy_remote` custom methods and an offscreen screenshot (§13.5).
  - **Observability** foundation (§15).
- **Iteration 1 roster** (small test roster, user decision). Germany and USSR, each with:
  - Tanks: light, medium, heavy
  - Guns: AT gun, howitzer, mortar, AA gun
  - Infantry: rifle squad (with officer), HMG team, sniper
  - Trucks: supply truck, engineer truck
  - Aircraft from M8
  Each unit type arrives with the milestone whose mechanic it exercises.
- **M1: Control and movement**
  - Terrain grid (height, terrain types, road, forest, river, bridge), the speed formula, and movement-class blocking.
  - Pathfinding with clearance, steering, hull rotation.
  - Selection: click, box, Shift-add, double-click for the same type, control groups.
  - Commands: Move, Stop, Rotate, Follow, Shift-queue.
  - Testbed controls: pause and still give orders, game speed, switch the controlled player.
  - Overlays for paths, passability and cells.
  - Content hot reload under the §6.4 policy.
- **M2: Direct-fire combat** (starts once the M2 entry requirements, §9.2, are met)
  - Weapons and shells defined in data; turrets.
  - Firing cycle, dispersion and hit test, armor facing, penetration, damage, HE splash, ammo, track breaks, wrecks.
  - Target acquisition, plus Attack, AttackGround, Swarm and Stand Ground.
  - Overlays for ranges and armor facing, and a combat log.
  - Basic VFX (tracer, impact, explosion) and sounds.
- **M3: Vision**
  - Per-player visibility grid, line-of-sight blocking, height bonus.
  - Camouflage and reveal, ghosts and forgetting.
  - Fog drawn on the terrain by a WESL material extension. Enemy visuals hidden in fog; "reveal all" overrides that in the client.
- **M4: Infantry and buildings** (`fl_feat_infantry` + client)
  - Squads, formations, prone and suppression, cover, grenades.
  - Garrisoning buildings and trenches.
  - Building damage and destruction.
  - Enter/leave; loading into and unloading from transports.
- **M5: Artillery, supply, morale** (`fl_feat_artillery`, `fl_feat_supply` + clients)
  - Indirect fire with spotting, minimum range, ranging, counter-battery circles.
  - Crews and capturable guns.
  - Resources, depots, trucks, resupply, re-manning.
  - Morale.
- **M6: HUD and art**
  - Command panel with a QWER/ASDF/ZXCV hotkey grid, built from buttons that features contribute.
  - Info panel (armor values, weapon damage and piercing), 4 status bars, minimap with fog, icons.
  - First real CC0/CC-BY models through the visual-def pipeline; credits screen.
- **M7: Engineering, towing, entrenchment** (`fl_feat_engineering`, `fl_feat_towing`).
- **M8: Air support and AA** (`fl_feat_air_support`).
- **M9: Experience, snipers, binoculars, smoke, weather, medics, storming buildings.**

M0–M3 are the critical path. From M4 on, the work is mostly independent feature crates, which suits parallel subagents.

### 9.2 M2 entry requirements (Group C)

These land before any M2 mechanic, as an "M2 prelude". They shape how mechanics are specified, tested and composed.
Delivered by `docs/superpowers/plans/2026-10-25-frontline-m2-prelude.md` (contracts: ADR-0017 to ADR-0022 and revision 2.5).

- **C1 Machine-checkable mechanic specs.** `docs/design/GDD.md` indexes one spec per mechanic: EARS requirements with stable `AREA.NAME.N` IDs, formulas, parameter tables with RON paths, and acceptance criteria. `xtask spec-lint`; `xtask trace` maps requirements to tests through `@req` tags.
- **C2 Provenance ledger.** `assets/base/provenance/*.prov.ron` with a closed vocabulary: Original, Manual, ResearchDerived, OursScaled, Guess, Tuned. `data-check` enforces it, and a tuning-tables doc is generated from it.
- **C3 Separate test-author agent.** It writes protected acceptance tests from the spec alone, plus a **held-out** 10–20% that runs only at milestones and nightly.
- **C4 Statistical test policy.** Oracles come from the spec or an f64 reference, never measured from the code. `assert_rate_within_wilson` at α = 1e-4. Two tiers: formula level (10⁵ draws) and scenario level (200–500 seeds). Nightly runs rotate seeds. A failing seed set may be rerolled once.
- **C5 Core vocabulary.** OpenRA-style **conditions** (granted tokens plus `requires` expressions); a **modifier and stat spec** with a deterministic stacking order and a cached effective stat (an ADR); **tag bitsets plus class expressions**; **effect and trigger verb registries**. Command-panel buttons carry `requires`.
- **C6 Content load phases and patch ops:** discover → parse → migrate (`format_version`) → patch (Set, Merge, Remove, Add, AddTag, If) in passes → inherit → typed deserialize → feature post-process hooks → link → freeze. Provenance per path.
- **C7 Unit behaviour and entity graph.** An in-house **order queue + activity stack**, state as table-stored enums, SparseSet markers only for rare states. **Custom relationships** for squad, garrison, transport, towing, crew and ownership (not `ChildOf`). An **event-model ADR**: Messages for gameplay, hooks for invariants, observers only for order-independent reactions in the sim.
- **C8 Golden replay versioning.** `SIM_VERSION` bump policy, semantic assertions next to hashes, `fl_cli rebless --reason MCR-n` with a report, and change classes in commit trailers (refactors must stay bit-identical).
- **C9 Mutation testing.** `cargo-mutants --in-diff` per merge and per task (time-capped), a full sharded run nightly, missed-mutant triage at milestones. Coverage stays diagnostic only.
- **C10 Risk-tiered review**, so not every task gets two full LLM reviews, and a per-mechanic mini-gate inside each milestone.
- **C11 Render scale baseline.** One material per unit type, per-instance data via `MeshTag`/`GpuComponentArrayBuffer`, `VisibilityRange` LOD.

### 9.3 Later work, with triggers (Group D)

| Item | Trigger |
|---|---|
| AI controllers outside the sim on a designated peer, with `PlayerView` in `fl_view`; layered utility AI, HTN and influence maps, time-sliced | The first AI faction |
| Lockstep net crate: spike `lightyear` 0.31 against a thin custom session behind a `NetSession` trait | The multiplayer milestone |
| Desync tooling: per-phase and per-component hashes, `fl_cli desync-diff`, `track_location` in debug builds | With the net crate, or the first unexplained desync |
| Remote + 3-OS CI (B8, skipped by user decision): split sim/client jobs, cross-OS golden replays, WARP/lavapipe screenshots | The user creates a remote, or before the net crate (cross-OS determinism must be proven before multiplayer) |
| Campaign meta layer as a separate state with its own save | The campaign milestone |
| Economy core: stocks, flows, aggregate networks, sleeping entities, reservations (in the `Economy` phase) | The base-building or colony milestone |
| Vertex-animation-texture infantry plus animation LOD | More than ~1,000 animated soldiers on screen |
| `.bsn` visual prefabs; a DefComponent → `Template` adapter | Bevy ships the `.bsn` loader |
| `bevy_lint` | It supports Bevy 0.20 |
| Scripting tier (Lua or WASM), commands-only API | A campaign scripting need |
| F8 bug bundle → `/triage-bug` → minimized regression replay | The first playtest outside the dev loop |
| `fl_cli sweep` with common random numbers and Lanchester refits | The first balance-tuning MCR |
| Runtime experiment flags, inside the rules hash, both arms tested | The first experimental mechanic |
| Feedback ledger | The first playtest |
| Projectile pools as SoA resources | Profiling shows spawn/despawn churn |
| Entity update skipping ("movement intentions") | Tick budget pressure from idle or travelling units |

---

## 10. Client and presentation

- **Camera**
  - Orthographic and isometric: about 45° yaw, about 30–35° pitch.
  - Pan with WASD, the arrow keys and the screen edge; zoom with the mouse wheel; rotate with Q/E. Designer's call: free rotation with an optional snap.
  - Clamped to the map bounds.
- **Terrain:** a heightmap mesh, vertex-colored by terrain type at first, textured later.
- **Units:** visual entities follow the `fl_view` snapshot with interpolation. Turret and barrel nodes rotate to the snapshot's angles. The client never reads sim components.
- **Selection**
  - Single-click picking uses Bevy picking. `MeshPickingPlugin` isn't in `DefaultPlugins`, so add it. Handle clicks with the `On<PointerClick>` observer.
  - Box select projects unit positions to the screen.
  - Selection rings are drawn with gizmos or a mesh.
- **Input → commands:** context-sensitive right-click (attack an enemy, move to ground, enter a building). Ctrl forces attack, Alt forces move, Shift queues. Commands go to the `LocalDriver`, which stamps `exec_tick` and `seq`.
- **UI**
  - `bevy_ui` for the game HUD. Use the 0.20 `ui_widgets::Button` with `On<Activate>`, not the deprecated prelude `Button`.
  - `bevy_egui` for debug and testbed tools.
  - Fonts: Black Ops One and Oswald (OFL).
- **Fog:** the visibility grid is uploaded to an `R8Unorm` texture that an extended terrain material samples (WESL).
- **Audio:** Bevy audio, positional where it's cheap. CC0 sounds from Freesound and Kenney.

## 11. Testbed and debug tools

Anything that changes sim state is a command (ADR-0008); everything else is a client or driver control.
- **Spawn panel:** pick a unit id, a player and a position (click to place); it issues `debug.spawn`.
- **Cheats:** "invulnerable", "set HP", "teleport" and "set owner" issue `debug.*` commands. They work only in matches with `cheats_allowed` (the testbed sets it).
- **Player switcher:** changes which player the local human controls. Client-local.
- **Reveal all:** a client render override (§5.5). It doesn't touch the sim.
- **Time controls:** pause, step a single tick, speed from ×0.25 to ×8. These are `LocalDriver` controls: they change when ticks run, never what a tick does, so they aren't commands. Orders given while paused are stamped for the next tick and applied when time resumes.
- **Overlays:** passability, height, paths, weapon ranges and arcs, sight radius, visibility grid, armor facing, and the last N shots (dispersion ellipse, impact, penetration roll). They read the snapshot, view channels or debug queries. Sim-side data the snapshot doesn't carry comes from named debug queries (§5.3, ADR-0015): the M1 path, passability and cell overlays poll `core.paths`, `core.passability` and `core.clusters`, and BRP `fl.debug_query` and `fl_cli query` reach the same queries.
- **Inspector:** the selected unit's sim components (read-only), through a `SimQuery` to the runner. Def values are edited live through hot reload (§6.4).
- **Scenarios, saves and replays:** save the current setup as a scenario RON file; save and load the match (§5.8); record and play back a replay.
- **Combat log:** every shot, with all its rolls, from the journal.

## 12. Verification baseline

Verification is mechanical: the same `xtask` commands run for agents, hooks and the user. All of it runs locally
(B8 skipped, §13.4). ADR-0009 and ADR-0010 record the decisions.

### 12.1 One entry point: `cargo xtask gate <level>`

- **Levels:** `task` (every task, and the Stop/SubagentStop hook), `pr` (before a branch merges into `main`), `nightly` (run on the dev machine by a scheduled task or by hand), `milestone`.
- **Agents and hooks run exactly these commands.** A `Stop`/`SubagentStop` hook runs `gate task` on the crates that changed and blocks with a compact failure summary until it passes. Retries are capped, then it escalates. The check must be able to pass, or the hook loops.
- **Evidence, not assertions.** Task reports paste the `gate task` output; reviewers read it instead of re-running.
- **Budget:** `gate task` stays under 3–5 minutes on the dev machine. Sim crates never compile `bevy_render`, the scenario suite runs for the touched area plus a fixed smoke set, and slow checks move to `pr` or `nightly`.

| Check | task | pr | nightly | milestone |
|---|---|---|---|---|
| `fmt --check`, `clippy -D warnings` (all targets) with the sim lints | touched crates | workspace | | ✅ |
| `xtask arch-check`, per-package headless builds and tests | ✅ | ✅ | | ✅ |
| `cargo deny check` | | ✅ | | ✅ |
| `xtask test-guard`, `xtask check-scope --track <t>` | ✅ | ✅ | | |
| `data-check` | ✅ | ✅ | | ✅ |
| nextest (agent profile): unit, property and scenario tests | touched crates + `fl_sim` smoke | workspace | ✅ long proptest | ✅ |
| Component-classification check | ✅ | ✅ | | ✅ |
| Twin-run, `shuffle_seed` runs, system-order snapshot | 1 seed | 3 seeds | | ✅ |
| Replay round trip, save round trip, branch test | smoke | full | | ✅ |
| Golden replays | | ✅ | | ✅ re-bless review |
| Work-counter budgets and snapshots | ✅ | ✅ | | ✅ |
| Contract snapshots (public API, serde formats) | if contract touched | ✅ | | ✅ |
| Client input → command tests | if client touched | ✅ | | ✅ |
| BRP integration test and screenshots (dev GPU) | | | ✅ | ✅ |
| Criterion wall clock (median of 5, interleaved) and soak | | | | ✅ |
| From M2 (C3, C4, C9): held-out suite, rotating seeds, cargo-mutants | `--in-diff`, capped | `--in-diff` | full, sharded | triage |

### 12.2 Determinism and erosion lints (B2)

- **Generated `clippy.toml` per sim crate.** Clippy reads only the nearest `clippy.toml` and doesn't merge them, so `xtask gen-clippy` writes one per crate from a template; `gate` fails if they drift.
  - Sim crates (`fl_protocol`, `fl_view`, `fl_sim`, every `fl_feat_x`) ban the types `f32`, `f64`, std and `bevy_platform` `HashMap`/`HashSet`, `Instant`, `SystemTime`, `ParallelCommands`, `bevy_time::Time` and `fl_math::Pcg32`, and the methods `SystemTime::now`, `Instant::now`, `thread::spawn`, `rand::rng`, `rand::random` and `std::env::var`.
  - `fl_math` and `fl_content` carry floats legitimately at their edges (`Fx::from_f64`, `to_f32`, presentation colours). They get the collection, time and thread bans, and scoped `#[expect(…, reason = "…")]` on the conversion functions only.
  - The root `clippy.toml` keeps the workspace-wide collection and time bans for client crates.
  - The generator emits only paths that resolve in the crate's dependency graph (or marks the entry `allow-invalid`), so an unused ban never turns into a warning under `-D warnings`.
- **`clippy::float_arithmetic = "deny"`** in every sim crate's lints, with a scoped `#[expect]` only in `fl_math`'s conversion functions. It catches float literals and arithmetic that `disallowed-types` misses.
- **`xtask arch-check`** (`cargo_metadata`):
  - a table of allowed crate edges (§5.1)
  - `cargo tree -p <crate> -e normal,build` run **one package at a time**, failing on `bevy_render|bevy_window|bevy_input|bevy_time|bevy_winit|wgpu|bevy_internal|bevy$` in any sim or headless crate
  - per-package headless test runs (`cargo nextest run -p fl_sim -p fl_feat_infantry …` with no client crate selected), which proves the sim compiles headless
  - It replaces `scripts/check_deps.sh`.
- **`cargo-deny`:** licences (no NC, ND or SA), advisories, `multiple-versions = "deny"` for `bevy_*`, and bans on `bincode` and `bevy_dynamic_plugin`.
- **Never install a "log and continue" fallback error handler** in tests or `fl_cli`; failures must surface.

### 12.3 Determinism checks, moved left (B3)

- **Ambiguity detection at `Error`** on `SimTick` from M0 (§5.6).
- **Twin-run:** every scenario test builds two sims from the same inputs and compares the state hash every tick.
- **Schedule shuffle:** `ScheduleBuildSettings::shuffle_seed` (bevy_ecs `debug` feature) with the single-threaded executor. One seed per task gate, three per `pr`. Hashes must equal the unshuffled run.
- **System-order snapshot:** a test dumps the built `SimTick`'s phases, sub-sets and topologically sorted system names, and `insta` snapshots it. Every schedule change, including a parallel agent's, shows up as a reviewed diff.
- **Replay round trip:** record → serialize → load → replay gives an identical hash stream.
- **Save/load parity:** the round-trip and branch tests (§5.8).
- **Executor parity** joins once a multi-threaded executor is enabled.

### 12.4 Test layers

- **Pure unit tests** for `fl_math`, `fl_content` and the combat formulas, using `proptest` for formulas and paths. Expected values come from the spec or an f64 reference oracle in `fl_test_utils`, never from running the implementation.
- **Headless sim tests** with `ScenarioBuilder` and `TestDriver`, asserting on journal events and final state. Examples:
  - "Pz III vs T-34, frontal, at 300 m penetrates about X% of the time", measured over many seeded trials (statistical policy from M2, C4).
  - "A truck slows down off-road."
  - "A squad goes prone under fire."
  - Sim logic is written test-first. Sim tests use the real `build_sim_app` with fixture content; mocks only at the client/OS boundary.
- **Property tests over commands:** random valid command sequences against invariants (HP within bounds, units in passable cells, entity count changes only through spawn and death journal events, hashes equal across twin and shuffled runs). Shrunk failures are saved as regression scenarios.
- **Golden replays:** stored command streams with expected hash streams under `tests/replays/`. The versioning and re-bless protocol arrive with C8.
- **Client:** kept thin. `MinimalPlugins` tests cover input → command mapping. A BRP integration test drives `fl.step`, `fl.sim_state` and `fl.issue_command` over HTTP (§15 C). Visuals are checked by hand and with offscreen screenshots; vision models are advisory only, never a gate.
- **Data gate:** `fl_cli data-check assets/base` runs in every gate.

### 12.5 Protected tests (B4)

- **Protected paths:** `**/tests/acceptance/**`, `**/tests/holdout/**`, `**/tests/replays/**`, `**/snapshots/**`, the root `scenarios/**` (test scenarios), `docs/design/mechanics/**` and perf baselines (`docs/perf/baselines/**`, `docs/perf/budgets.ron`). Content scenarios under `assets/` are not protected.
- **A PreToolUse hook** on `Edit|Write` denies these paths to implementer sessions (configured in the implementer role's agent definition).
- **A Bash hook** blocks `cargo insta accept`, `INSTA_FORCE_PASS`, `INSTA_UPDATE=always`, `--bless`, `cargo xtask bless` and `git checkout -- tests/` for implementer roles.
- **Accepting snapshots and baselines** happens only through `cargo xtask bless <paths> --reason "…"`, run by the lead (controller) session or the user. The reason goes into the commit. From M2, golden replays are re-blessed only through `fl_cli rebless --reason MCR-nnn` (ADR-0021). Re-blesses follow R146: under the user's delegation the lead blesses them with that MCR reason, and the plan's final report lists every re-bless for the user's ratification (C8, C10).
- **`xtask test-guard`** diffs test code against the merge base and flags removed or weakened `assert*` lines, new `#[ignore]` or `#[should_panic]`, changed tolerance or `ALPHA` constants, lower proptest case counts, deleted test functions, a falling `nextest list` count, new seed overrides, and mock or fake types in sim tests. The reviewer must accept each finding explicitly.
- **Escape hatch:** an implementer that believes a protected test is wrong writes `BLOCKED: test X contradicts spec Y because …` and stops. The lead or the user decides.

### 12.6 Performance gates (B5; ADR-0010)

- **Tier 1, the gate: deterministic work counters.** The sim counts algorithmic work per tick into the `Transient` `SimCounters` resource: path nodes expanded, LOS cells visited, spatial-query candidates, entities sorted, entities spawned and despawned, and allocations (through a counting allocator in test binaries). The sim is deterministic, so the counts are exact. Tests assert budgets on the scale scenario (in `docs/perf/budgets.ron`, protected) and snapshot the totals. An accidental O(n²) can't hide in noise.
- **Tier 2, trends: instruction counts.** `gungraun` needs Linux/Valgrind and CodSpeed needs a remote, so with B8 skipped this tier has no home yet (an open question for the user). Until it does, tiers 1 and 3 carry the load.
- **Tier 3, milestones only: wall clock.** `criterion` and `fl_cli bench` on the dev machine under a fixed power plan, baseline and candidate interleaved, median of 5, recorded in `docs/perf/M<n>.json`. The G7 budget (≤ 15 ms at 3,000 units, entity mix) is checked here. A regression above 20% against the previous milestone must be justified or fixed before the milestone closes.
- **This replaces the old 20% wall-clock gate per change.** The per-system profiler (§15 D) stays as a diagnostic, and its budgets are warnings.
- Benches run headless, or with the client window in continuous-update mode (§15 C).

### 12.7 Contract snapshots (B6)

- **Public API:** `cargo-public-api` diffs `fl_math`, `fl_protocol`, `fl_view` and the `fl_sim` extension API (`SimBuilder`, phases, registries, `SimFeature`, the runner protocol), stored as insta snapshots. It needs rustdoc JSON from a pinned, dated nightly toolchain; nothing else uses nightly.
- **Wire and save formats:** `serde-reflection` snapshots the formats of `PlayerCommand`, core command payloads, `MatchHeader`, the snapshot types and every registered save codec. A format change without a version bump fails the gate.
- Contract crates mark public enums and structs `#[non_exhaustive]` where growth is expected.

## 13. Delivery process

### 13.1 Plans, tasks and review

- Plan with `superpowers:writing-plans`, then execute with subagents (`superpowers:subagent-driven-development`), as the user asked. Tasks reference spec sections and ADRs rather than restating them.
- **Parallelization**
  - M0 is sequential while it defines the contracts: math, protocol, view, content, sim skeleton.
  - After that, independent tracks run in parallel subagents, each owning paths declared in `OWNERS.toml`: movement/pathfinding, combat, client camera/selection, debug tools, the asset pipeline.
  - Feature crates (M4 and later) are naturally parallel.
  - At most two agents build at the same time (§13.3); reviewers are read-only or test-only.
- **Each task:** TDD where the logic is testable, `gate task` green with its output pasted, a review for spec compliance, code quality and the determinism checklist, then a commit. From M2, review is risk-tiered (C10).

### 13.2 ADRs, deprecation and ownership (B7; ADR-0011)

- **ADRs** live in `docs/adr/NNNN-title.md` in MADR 4.0 format, indexed in `docs/adr/README.md`. ADRs 0001–0013 record revision 2.
- **Contract paths change only through an ADR.** They are `fl_math`, `fl_protocol`, `fl_view`, the `fl_sim` extension API (builder, phases, registries, feature traits, runner protocol, save format), the `fl_content` loader phases and registry, `xtask`, the agent config in `.claude/`, `deny.toml`, `OWNERS.toml`, the generated `clippy.toml` template and this spec. The contract snapshots (§12.7) make such changes visible in review.
- **Deprecation policy:**
  - Rust contract items: `#[deprecated(since = "M<n>", note = "…")]`, removed one milestone later.
  - Content keys: `#[serde(alias)]` plus a `data-check` warning listing deprecated keys, then a content migration (C6) when removed.
  - Saves: migrations are kept forever, or until an explicit save-break ADR.
  - Replays: guaranteed only for matching `sim_version`, features, mods and rules hash (§5.5).
  - The policy applies from the end of M0. Until then, contracts are still being written and change freely, through ADRs.
- **`OWNERS.toml`** maps paths to tracks, with a `contract` track for the paths above. `cargo xtask check-scope --track <name>` fails a branch that touches paths outside its track.

### 13.3 Agent development environment (B9, B10; ADR-0013)

- **Build loop (Windows):**
  - A per-worktree short build dir in the repo `.cargo/config.toml` (R19): `build.build-dir = "C:/t/fl/{workspace-path-hash}"`. The repo config sets no shared `target-dir`. A `WorktreeRemove` hook deletes the worktree's build dir.
  - `rust-lld` as the linker for `x86_64-pc-windows-msvc`, in the repo config.
  - Less debug info: `debug = "line-tables-only"` for workspace crates, `debug = false` for dependencies; `opt-level = 3` for dependencies.
  - sccache for dependencies, with `SCCACHE_BASEDIRS` so worktrees share compiled dependencies.
  - **At most two concurrent builds** across agents.
  - No `cargo check` hook after every edit; the rust-analyzer LSP plugin gives per-edit diagnostics.
- **Claude config in the repo:**
  - `CLAUDE.md` under 200 lines: build, test and CLI commands; the toolchain pin; the determinism and dependency rules; "use the `bevy-api` skill"; where specs, ADRs and plans live.
  - Path-scoped rules in `.claude/rules/` (`sim.md`, `client.md`, `content.md`, `shaders.md`).
  - Hooks in `.claude/settings.json`: the Stop/SubagentStop gate (§12.1), the protected-path and Bash guards (§12.5), and an ask-first guard on `cargo update`, `cargo add`, `rustup default` and `git push --force`.
  - **Role subagents** in `.claude/agents/`: `sim-engineer`, `client-engineer`, `test-author` (active from M2, C3), `reviewer` (read-only, with the determinism checklist) and `perf-analyst` (never edits code). Each names its tools, skills, hooks and MCP servers.
  - A **project skill** `bevy-api` that resolves Bevy APIs in this order: the local Bevy 0.20 sources, examples and migration guides in `~/.cargo/registry`; then `docs/research/bevy020-api-reference.md`; then a compile probe in `probes/`. Never from memory.
- **Test output:** a nextest `agent` profile (`.config/nextest.toml`) with failure-only output, JUnit, fail-fast after 5, a slow-test timeout, and a serial `gpu` test group.
- **Live game access:** `bevy_brp_mcp` 0.23, registered in the project `.mcp.json`. Each agent that runs the game uses its own BRP port (`--remote-port`, `BRP_EXTRAS_PORT`). Agents drive the game through `fl.*` methods, not mouse clicks (§15 C).
- **User-applied system settings** (agents don't change them): Dev Drive or Defender exclusions for `CARGO_HOME`, the repos and `C:/t/fl`; a fixed power plan for milestone benches.

### 13.4 CI: local only (B8 skipped)

- By user decision there is no GitHub remote and no hosted CI. `cargo xtask gate pr` runs before a branch merges into `main`, and `gate nightly` runs on the dev machine.
- Consequences: determinism is verified on Windows x64 only, and instruction-count trends have no runner. Both are open questions for the user; the remote returns as a Group D item with a trigger (§9.3).

### 13.5 Early probes in M0

- **`dynamic_linking`:** does `bevy/dynamic_linking` in `fl_game` combined with sim crates that depend only on `bevy_ecs`/`bevy_app` cause duplicate-crate link errors? Probe before relying on it; if it fails, enable dynamic linking for client-only dev runs or not at all.
- **`bevy_remote`:** custom method registration on the client app, a method that round-trips to the runner thread, and an offscreen-camera screenshot through `Screenshot::image`.

## 14. Left to the builder

- `I32F32` vs `I48F16` (M0 chose `I32F32`), and the trig approach.
- The exact cell size (4 m suggested), the testbed map size (start at 1×1 km; the 2×2 km scale map exists for benchmarks), and the HPA* cluster size.
- Parallel-compute mechanics (rayon vs Bevy's task pool) once the multi-threaded executor is enabled, within the pure-phase/ordered-apply rule (§5.4). The sim's parallel systems share Bevy's global task pools with the client; measure contention first.
- The snapshot record layout and view-channel encoding (§5.3), and the channel crate for the runner (std `mpsc` or `crossbeam`).
- The internals of target-acquisition scoring.
- The input crate: `leafwing-input-manager` or `bevy_enhanced_input` vs plain Bevy input.
- Which of the CC0/CC-BY assets in `docs/research/assets.md` §7 to start with.
- Crate versions, checked against crates.io at implementation time.

---

## 15. Observability (user requirement, added 2026-10-23)

The user wrote: "I also want to make sure that we have proper debugging and logging tools available, an ai-agent must also be able to easily trace and debug the game in debug mode. we also need to be able to trace performance of every system or mechanic in the game".

The foundation goes into M0. Retrofitting it later costs more. Every later milestone adds trace events, profiling spans and work counters for its own mechanics, as part of its definition of done (§9).

### A. Structured logging

- Use `tracing` everywhere. The client goes through Bevy's `LogPlugin`; headless binaries install an equivalent subscriber.
- Log targets follow crate and module names (`fl_sim::combat`, `fl_feat_artillery`, …).
- Per-target filters come from `RUST_LOG` or from `debug.ron`.
- Sim log records always carry `tick`. Where relevant they also carry `sim_id` and `player`.
- Debug builds also write JSON lines to `logs/<run-id>/log.jsonl`, so an agent can grep or parse them.

### B. Sim event journal: the "what happened and why" record

- Every tick, the sim appends structured records to a journal resource (`Transient`). Records include:
  - each command applied or rejected, with the reason
  - each order or state transition
  - each path request and its result
  - each shot, with every roll: dispersion point, hit or miss, armor side, piercing and armor rolls, cover roll, damage
  - each splash, suppression, death, track break, vision change, and so on
- Features register their own record types, the same way they register commands. Record kinds are a stable-string registry (§5.4). Nothing is centrally enumerated.
- Journal-writing systems may run unordered, so `step_tick` sorts each tick's entries by `(phase, kind, sim_id, emission order)` before output. The journal is then identical across shuffled runs.
- In debug mode the journal goes to `logs/<run-id>/journal.jsonl`. The client combat log reads from it.
- The journal is observe-only. It is kept out of the state hash and never read by sim logic. In release builds it is disabled.

### C. Agent debugging interface

An AI agent must be able to drive, inspect and reproduce issues with no human present.

- **Headless CLI** (`fl_cli`). Every command takes `--json`, so its output is machine-readable.
  - `run-scenario <scenario> --ticks N [--seed S] [--trace] [--dump-state-every K]`
  - `inspect <replay|scenario|save> --tick T [--entity SIMID]` shows entity components and world state at a tick.
  - `journal <run> --filter …` (tick range, entity, kind, limit), so agents never read a multi-MB file whole.
  - `replay-verify`, `replay dump --json`
  - `save` / `load`
  - `bisect-desync` finds the first tick at which two runs' hashes diverge.
- **Live debug mode** uses the Bevy Remote Protocol (`bevy_remote`: JSON-RPC over localhost HTTP). It sits behind a `debug_remote` Cargo feature and a CLI flag, and is never in release builds.
  - Built-in `world.*` methods act only on the **client** World. They never reach the sim, and `world.mutate_*` must never be wired to sim state.
  - The sim is reachable **only through custom `fl.*` methods**: `fl.sim_state`, `fl.entity` (by SimId), `fl.hash`, `fl.issue_command`, `fl.pause`, `fl.resume`, `fl.step` (N ticks), `fl.set_speed`, `fl.spawn` (issues `debug.spawn`), `fl.journal_since` (tick), `fl.perf`, `fl.screenshot`, `fl.toggle_overlay`, `fl.load_scenario`, `fl.save_scenario`, `fl.save`, `fl.load`.
  - Methods that change the sim go through the command stream (`fl.issue_command`, `fl.spawn`) or the driver (`pause`, `resume`, `step`, `set_speed`, `load`). Reads go through `SimQuery`.
  - The methods are also published as agent tools with descriptions through `bevy_brp_extras`, so `bevy_brp_mcp` can list and call them.
  - **`fl.screenshot`** renders from a dedicated **offscreen camera** into an `Image` target and captures it with `Screenshot::image`. Window captures come back black when the window is hidden behind another app.
  - **Agent and bench launch mode** sets the window's unfocused update mode to continuous; an unfocused window otherwise throttles and perf readings are wrong.
  - Agents wait for readiness by polling a probe (`fl.sim_state` tick ≥ T), never with fixed sleeps.
- **Documentation.** `docs/debugging.md` is written for both humans and agents. It covers:
  - how to launch in debug mode, and how to pick a BRP port
  - every CLI command, and every BRP method with example JSON
  - where logs and journals are written
  - a "reproduce a bug" recipe: seed + scenario + replay → bisect → inspect
- **Verification.** An integration test starts the client app with the remote plugin, then calls `fl.step`, `fl.sim_state` and `fl.issue_command` over HTTP.

### D. Performance tracing for every system and mechanic

- **Built-in sim profiler.** Always available in debug and profiling builds, and cheap enough to leave on.
  - Records wall time per system, per phase and per feature, every tick.
  - Keeps rolling p50, p95 and max values.
- **Work counters** (`SimCounters`): entities processed, path requests and nodes, LOS checks and cells, query candidates, shots, spawns. They are deterministic, so they are also the perf **gate** (§12.6).
- **Attribution.**
  - Every system is attributed to its owning feature automatically, from the module path of its system name (`fl_feat_infantry::…` → `infantry`, `fl_sim::movement::…` → `core.movement`, `fl_feat_infantry_client::…` → `infantry (client)`). Time therefore rolls up per feature ("infantry: 1.2 ms, artillery: 0.4 ms"), and features can't forget to tag.
  - Per-system timing comes from `bevy_ecs`'s `trace` spans, aggregated by a tracing layer that handles both the runner thread and the main thread. Per-phase timing comes from marker systems.
- **Budgets.** Time budgets per feature and per phase live in `debug.ron`; going over logs a warning naming the system. Counter budgets live in `docs/perf/budgets.ron` and fail the gate.
- **Determinism.** Timing data never flows back into the sim.
- **Where the data shows up:**
  - live in an egui debug panel, as a table plus a sparkline per feature and system
  - over BRP via `fl.perf`
  - from `fl_cli bench --profile --json`, which writes a per-system breakdown and counter totals to `docs/perf/<milestone>.json`
- **Deep profiling.** Cargo features pass through to Bevy's `trace`, `trace_tracy` (Tracy) and `trace_chrome` (Chrome trace JSON, which agents can read). Sim systems get named spans automatically. Hot inner loops (pathfinding, LOS, target scoring) get manual spans. The `perf-analyst` agent diffs `tracy-csvexport` output instead of reading a GUI.
- **Client.** Bevy frame-time and entity-count diagnostics, a render stats overlay, and an FPS and frame-time graph. The client can be profiled with a recorded snapshot stream in place of the sim, so the two budgets are measured separately.

### Hard rules still apply

- Determinism (§5.4) and the crate dependency rules (§5.1) are unchanged.
- Observability code in the sim depends only on `tracing`, `serde_json` and the `bevy_ecs`/`bevy_app` crates.
- The headless `fl_observe` crate adds `tracing-subscriber`.
- The client adds `bevy_remote` (and `bevy_brp_extras`), behind the `debug_remote` feature.
