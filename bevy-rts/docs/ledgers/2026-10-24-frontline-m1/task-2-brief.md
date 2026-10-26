### Task 2: Contract additions — payloads, debug queries, `PlayerView` and the AI seam types

**Track:** contract + tools · **Refs:** ADR-0007, ADR-0015, ADR-0016 · **Lane 1**

**Files:**
- Modify:
  - `crates/fl_protocol/src/{payloads,lib}.rs`, `crates/fl_protocol/tests/{protocol,formats}.rs`
  - `crates/fl_view/src/{runner,lib}.rs`, `crates/fl_view/tests/view.rs`
  - `crates/fl_sim/src/{builder,lib}.rs`, `crates/fl_sim/src/runner/mod.rs`
  - `crates/fl_cli/src/{main,lib}.rs`, `crates/fl_cli/tests/agent.rs`
- Create:
  - `crates/fl_view/src/ai.rs`
  - `crates/fl_sim/src/debug_query.rs`, `crates/fl_sim/tests/debug_query.rs`
  - `crates/fl_cli/src/query_cmd.rs`
- Blessed:
  - `crates/fl_protocol/tests/snapshots/formats__protocol_formats.snap`
  - `crates/fl_view/tests/snapshots/formats__view_formats.snap` (if it changes)
  - `xtask/snapshots/api/{fl_protocol,fl_view,fl_sim}.txt`

**Interfaces:**
- Consumes: `CommandPayload`, `EntityRecord`, `SimSnapshot`, `CommandDraft`, `StableRegistry`, `SimRunner::query`, the `inspect` command's source parsing in `fl_cli`.
- Produces:
```rust
// fl_protocol/src/payloads.rs (deny_unknown_fields like the others)
pub struct RotateCmd { pub toward: FxVec2 }        // "core.rotate": each unit turns its hull to face `toward`
pub struct FollowCmd { pub leader: SimId }         // "core.follow": follow `leader` at the unit's current offset
pub struct DebugSetTerrainCmd { pub min_cell: (u32, u32), pub max_cell: (u32, u32), pub terrain: String }
                                                   // "debug.set_terrain": cells min..=max (x, y) become terrain id `terrain`; DEBUG_ONLY, NEEDS_UNITS = false
// fl_view/src/runner.rs
SimQuery::Debug { name: String, args: serde_json::Value }
SimReply::Debug(serde_json::Value)
// fl_view/src/ai.rs
#[derive(Debug, Clone, PartialEq)] pub struct PlayerView { pub tick: Tick, pub player: PlayerId, pub team: TeamId,
    pub own: Vec<EntityRecord>, pub visible: Vec<EntityRecord> }
impl PlayerView { pub fn from_snapshot(snapshot: &SimSnapshot, player: PlayerId, team: TeamId) -> PlayerView; }
    // own: owner == player; visible: owner != player and visible_to bit `team` set; both in sim_id order
pub trait AiController: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn think(&mut self, view: &PlayerView, tables: &StringTables) -> Vec<CommandDraft>;
}
pub struct NullAi;                                 // id "null"; think returns no drafts
pub fn ai_controller(id: &str) -> Option<Box<dyn AiController>>;   // "null" → NullAi; anything else → None
// fl_sim/src/debug_query.rs
pub type DebugQueryFn = fn(&World, &serde_json::Value) -> Result<serde_json::Value, String>;
#[derive(Resource)] pub struct DebugQueries { .. }  // External "core.debug_queries"; names sorted, frozen in finish()
impl SimBuilder { pub fn register_debug_query(&mut self, name: &'static str, f: DebugQueryFn) -> &mut Self; }
    // panics on a duplicate name, naming both features (StableRegistry)
// built-in query "core.debug_queries" → {"queries": [every registered name, sorted]}
// SimRunner::query(SimQuery::Debug { name, args }) → SimReply::Debug(v), or SimReply::Error("unknown debug query '<name>'"),
//   or SimReply::Error(<the query's message>)
// fl_cli: `fl_cli query <replay|scenario|save> --tick T --name <query> [--args <json>] [--json]`
//   runs to tick T exactly as `inspect` does, then prints the reply (exit 0) or the error (exit 1)
```

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_protocol/tests/protocol.rs
#[test] fn payload_names()                       // extended: RotateCmd "core.rotate", FollowCmd "core.follow", DebugSetTerrainCmd "debug.set_terrain"
#[test] fn debug_set_terrain_is_debug_only_without_units()   // DEBUG_ONLY && !NEEDS_UNITS; RotateCmd and FollowCmd are neither DEBUG_ONLY nor unit-free
#[test] fn new_payloads_round_trip_postcard()    // RotateCmd { toward: (Fx::from_raw(-7), Fx::from_int(3)) }, FollowCmd { leader: SimId(9) }, DebugSetTerrainCmd { (1,2), (3,4), "base.water" }
// crates/fl_protocol/tests/formats.rs: protocol_formats also traces the three new payloads (snapshot .new)
// crates/fl_view/tests/view.rs
#[test] fn player_view_splits_own_and_visible()  // records #1 owner 0, #2 owner 1 visible_to 0b01, #3 owner 1 visible_to 0b10;
                                                 //   from_snapshot(P0, team 0) → own [#1], visible [#2]
#[test] fn null_ai_issues_nothing()              // NullAi.think(any view) is empty; id() == "null"
#[test] fn ai_controller_by_id()                 // "null" → Some(id "null"); "nope" → None
// crates/fl_sim/tests/debug_query.rs
#[test] fn debug_queries_listed()                // SimRunner::query(Debug "core.debug_queries") → {"queries": [..]} containing "core.debug_queries", sorted
#[test] fn unknown_debug_query_is_error()        // name "nope" → SimReply::Error("unknown debug query 'nope'")
#[test] #[should_panic(expected = "registered by 'test' and 'other'")] fn duplicate_debug_query_panics_naming_both()
#[test] fn debug_query_does_not_change_state()   // state hash before and after 10 queries is equal
#[test] fn failing_query_reports_its_message()   // a test feature's query returning Err("bad args") → SimReply::Error("bad args")
// crates/fl_cli/tests/agent.rs
#[test] fn query_lists_debug_queries()           // `query --scenario base.testbed --tick 0 --name core.debug_queries --json` → exit 0, JSON "queries" contains "core.debug_queries"
#[test] fn query_unknown_name_fails()            // --name nope → exit 1, stderr contains "unknown debug query 'nope'"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_protocol -p fl_view -p fl_sim -p fl_cli -E 'test(/payload|player_view|ai_|debug_quer|query_/)'`. Expected: FAIL (types and names missing).
- [ ] **Step 3: Implement.** The `DebugQueries` registry is a `StableRegistry<DebugQueryFn>`. Like the other registries, it is not exported to the string tables (no dense id ever leaves the sim). `fl_cli query` reuses `inspect`'s source handling. For a replay, that's `replay_app`, then a `ReplayDriver` over `fl_sim::replay_inputs`, which remaps recorded command ids by name (R73/I1), as `fl_cli/src/agent.rs` does since the final M0 fix wave.
- [ ] **Step 4: Regenerate protected snapshots.** Run the tests to write `formats__protocol_formats.snap.new` (and the view one if it changes), and run `cargo xtask api` for the three `.txt.new` files. Paste their diffs in the report.
- [ ] **Step 5: Bless (lead)** the `.new` files with the reason `M1 Task 2: core.rotate, core.follow, debug.set_terrain payloads; debug queries; AI seam types`.
- [ ] **Step 6: Run** the step 2 command and `cargo xtask api --check`. Expected: PASS, and `api: 5 crates unchanged`.
- [ ] **Step 7: Gate** `cargo xtask gate task --track contract --track tools`. Expected: last line `gate task: PASS in <n>s`.
- [ ] **Step 8: Commit** `feat(contract): core.rotate/core.follow/debug.set_terrain payloads, SimQuery::Debug registry, PlayerView and AiController seam, fl_cli query` with trailers `Refs: ADR-0007, ADR-0015, ADR-0016` and `Blessed: …`.

