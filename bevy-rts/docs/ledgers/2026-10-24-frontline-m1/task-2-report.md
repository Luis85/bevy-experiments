# Task 2 report: contract additions (payloads, debug queries, PlayerView, AI seam)

**Status:** DONE_WITH_CONCERNS. The concerns are about process only: the bless is pending, the brief's tracks don't match its Files list, and a controller commit sits in the gate range.

**Commit:** `c855f94 feat(contract): core.rotate/core.follow/debug.set_terrain payloads, SimQuery::Debug registry, PlayerView and AiController seam, fl_cli query`. It has the trailer `Refs: ADR-0007, ADR-0015, ADR-0016`. It has no `Blessed:` trailer, because the lead hasn't blessed yet.

**Base:** 15b8aba (`.gate-base`). The branch also holds the controller's docs commit 7eeb217, which isn't mine.

## What I implemented

**fl_protocol** (`payloads.rs`, `lib.rs`)
- `RotateCmd { toward: FxVec2 }`, named `"core.rotate"`.
- `FollowCmd { leader: SimId }`, named `"core.follow"`.
- `DebugSetTerrainCmd { min_cell: (u32, u32), max_cell: (u32, u32), terrain: String }`, named `"debug.set_terrain"`, with `DEBUG_ONLY = true` and `NEEDS_UNITS = false`.
- All three use `deny_unknown_fields` and are re-exported at the crate root.
- `FORMAT_VERSION` is unchanged. The format guard lets added types through, as the ADR-0007 M1 refinement says.

**fl_view**
- `runner.rs` adds `SimQuery::Debug { name, args: serde_json::Value }` and `SimReply::Debug(Value)`. Both enums were already `#[non_exhaustive]`.
- New `ai.rs`:
  - `PlayerView { tick, player, team, own, visible }`.
  - `PlayerView::from_snapshot`:
    - `own` holds the records with `owner == player`.
    - `visible` holds the records with `owner != player` whose `visible_to` bit for `team` is set.
    - Both keep the snapshot's sim_id order.
    - Team 8 or higher sees nothing, because the shift is a checked shift.
  - `AiController: Send + Sync + 'static`, with `id()` and `think()`.
  - `NullAi` has id `"null"` and returns no drafts.
  - `ai_controller(id)` returns `NullAi` for `"null"` and `None` for anything else.
- `VIEW_FORMAT_VERSION` is unchanged, and so is the view snapshot, because `SimQuery` and `SimReply` aren't traced.

**fl_sim**
- New `debug_query.rs`:
  - `pub type DebugQueryFn = fn(&World, &Value) -> Result<Value, String>`.
  - `#[derive(Resource)] pub struct DebugQueries`, which wraps a frozen `StableRegistry<DebugQueryFn>` and has `names()`.
  - `pub fn debug_query(world, name, args) -> Result<Value, String>`. An unknown name gives `Err("unknown debug query '<name>'")`; otherwise it returns the query's own result.
  - The built-in `core.debug_queries` answers `{"queries": [sorted names]}`.
- `builder.rs`:
  - `SimBuilder::register_debug_query(name, f) -> &mut Self` records the query under the current feature id.
  - A duplicate name panics through `StableRegistry` with `'<name>' registered by 'a' and 'b' in registry 'debug_queries'`.
  - The built-in query is registered in `SimBuilder::new()` with registrant `core`.
  - `finish()` classifies `DebugQueries` as External `"core.debug_queries"`, freezes it and inserts it. Like the other cases the brief names, it is not exported to the string tables, so no dense id leaves the sim.
- `runner/mod.rs`: `SimRunner::query(SimQuery::Debug { .. })` answers `SimReply::Debug(v)` or `SimReply::Error(msg)`.
- `lib.rs`: declares the module and re-exports `DebugQueries`, `DebugQueryFn` and `debug_query`.

**fl_cli** (new `query_cmd.rs`, plus a `Query` subcommand in `main.rs` and `lib.rs`)
- Usage: `fl_cli query <source> --tick T --name N [--args JSON] [--seed/--wander] [--mods ...]`.
- It uses `inspect`'s `Session::open` + `run_to`, so a replay goes through `fl_sim::replay_app` + `replay_inputs`.
- Exit codes and output:
  - A successful answer is printed with exit 0.
  - A query error is `CliError::Check`: exit 1, `error: <msg>` on stderr, and `{"error": msg}` with `--json`.
  - `--args` that isn't valid JSON is `CliError::Usage` (exit 2), checked before the sim is built.

## Deviations and notes

1. **`fl_cli query` takes the source as a positional argument, not `--scenario`.**
   - The brief's test text says `query --scenario base.testbed ...`.
   - Its interface line (`fl_cli query <replay|scenario|save> --tick T ...`) and "reuses `inspect`'s source handling" both describe the positional source that `inspect` and `save` take.
   - I followed the interface line, so the tests run `query base.testbed --tick 0 ...`. Adding `--scenario` would be a small clap change if it's wanted.
2. **fl_cli calls `fl_sim::debug_query(session.world(), ...)` rather than `SimRunner::query`.**
   - `Session`'s runner is private in `agent.rs`, and `agent.rs` isn't in my Files list.
   - `SimRunner::query` calls the same function, so both paths give the same answers and the same error strings.
   - The cost is one extra public fl_sim item (`debug_query`), which shows in the API diff. The alternative is a one-line `Session::query` in `agent.rs`.
3. **The brief's tracks don't match its Files list.**
   - The brief says `--track contract --track tools`.
   - Its Files list includes `crates/fl_sim/src/lib.rs`, the new `crates/fl_sim/src/debug_query.rs` and `crates/fl_sim/tests/debug_query.rs`. `OWNERS.toml` puts all three in **sim-core**; only builder, runner and the other named modules are contract.
   - I ran the gate with `--track sim-core` added, and the scope check is then clean for all my files.
4. **The controller's commit 7eeb217** (docs plan and spec nits) comes after `.gate-base` 15b8aba. It puts `docs/superpowers/plans/2026-10-24-frontline-m1.md` (track `docs`) into check-scope's range, and that is the only remaining check-scope finding. I didn't touch `.gate-base`.
5. **The ADRs and the brief agree.** ADR-0015's debug-query text and ADR-0016's types match the brief. R79 isn't exercised here: Task 2 adds no handlers, so nothing rejects with `core.invalid_target` yet.
6. **test-guard warning, for the reviewer to accept.** The brief requires a new `#[should_panic(expected = "registered by 'test' and 'other'")]` in `duplicate_debug_query_panics_naming_both`. In that test, feature `other` depends on `test`, so `test` registers first. Without the dependency, `sort_features` would order the tie by id and build `other` first, which reverses the names in the message.

## TDD evidence

**RED**, before implementation:

`cargo nextest run --profile agent -p fl_protocol -p fl_view -p fl_sim -p fl_cli -E 'test(/payload|player_view|ai_|debug_quer|query_/)'`
```
error[E0432]: unresolved imports `fl_protocol::DebugSetTerrainCmd`, `fl_protocol::FollowCmd`, `fl_protocol::RotateCmd`
error: could not compile `fl_protocol` (test "protocol") due to 1 previous error
```

With the payloads in place, `cargo build --tests --keep-going -p fl_protocol -p fl_view -p fl_sim -p fl_cli`:
```
error[E0432]: unresolved imports `fl_view::AiController`, `fl_view::NullAi`, `fl_view::PlayerView`, `fl_view::ai_controller`
error[E0425]: cannot find type `DebugQueryFn` in crate `fl_sim`
error[E0599]: no method named `register_debug_query` found for mutable reference `&mut SimBuilder` in the current scope
error[E0599]: no variant named `Debug` found for enum `SimQuery`
error[E0599]: no variant, associated function, or constant named `Debug` found for enum `SimReply` in the current scope
```

These failures were expected, because the types and names didn't exist yet. The fl_cli tests compiled; they would have failed at run time on the unknown `query` subcommand.

**GREEN**, same nextest command:
```
Starting 19 tests across 28 binaries (275 tests skipped)
Summary [   3.075s] 19 tests run: 19 passed, 275 skipped
```

**Full suites of the touched crates:**

`FL_SHUFFLE_SEEDS=1 cargo nextest run --profile agent --no-fail-fast -p fl_protocol -p fl_view -p fl_sim -p fl_cli -p fl_test_utils -E 'not (test(/^gpu_|^brp_|^milestone_/))'`
```
Summary [  27.190s] 316 tests run: 315 passed, 1 failed, 1 skipped
FAIL fl_protocol::formats protocol_formats   <- unblessed snapshot (expected until bless)
```

`fl_view::formats view_formats` passes, because the view snapshot is unchanged.

## Gate

Command: `cargo xtask gate task --track contract --track tools --track sim-core` (sim-core per deviation 3).

An earlier run with only `contract tools` was killed by my 595 s shell timeout during data-check. That run also flagged the three sim-core files.

```
PASS fmt (2.8s)
PASS clippy-drift (3.8s)
PASS claude-md (0.0s)
FAIL check-scope (0.7s)
    1 of 23 changed files are outside track(s) contract, tools, sim-core:
      docs/superpowers/plans/2026-10-24-frontline-m1.md → docs
PASS protected (0.6s)
WARN test-guard (2.6s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_sim/tests/debug_query.rs:127 NewShouldPanic: #[should_panic(expected = "registered by 'test' and 'other'")]
PASS clippy (22.1s)
PASS arch-check (7.7s)
FAIL test-headless (153.0s)      <- 394 run: 393 passed, 1 failed = protocol_formats (awaiting bless)
SKIP test-client: no client package touched since 15b8aba (.gate-base)
FAIL api (123.7s)                <- the three .txt.new files awaiting bless
    api: 3 of 5 crates changed or new
      fl_protocol: changed
      fl_view: changed
      fl_sim: changed
PASS data-check (104.6s)
WARN gate task took 423s (budget 300s)
gate task: FAIL (check-scope, test-headless, api) in 423s
```

After the bless I expect:
- The step 2 command passes.
- `cargo xtask api --check` prints `api: 5 crates unchanged`.
- test-headless passes.

check-scope stays red only for the controller's 7eeb217 docs file. That clears if the controller moves `.gate-base` or adds `--track docs`.

## Protected artifacts for the lead to bless

Reason: `M1 Task 2: core.rotate, core.follow, debug.set_terrain payloads; debug queries; AI seam types`

- `crates/fl_protocol/tests/snapshots/formats__protocol_formats.snap.new`
- `xtask/snapshots/api/fl_protocol.txt.new`
- `xtask/snapshots/api/fl_view.txt.new`
- `xtask/snapshots/api/fl_sim.txt.new`

`formats__view_formats.snap` is unchanged, so there is no view `.new` file.

## Files changed

- Modified:
  - `crates/fl_protocol/src/{payloads,lib}.rs` and `crates/fl_protocol/tests/{protocol,formats}.rs`
  - `crates/fl_view/src/{runner,lib}.rs` and `crates/fl_view/tests/view.rs`
  - `crates/fl_sim/src/{builder,lib}.rs` and `crates/fl_sim/src/runner/mod.rs`
  - `crates/fl_cli/src/{main,lib}.rs` and `crates/fl_cli/tests/agent.rs`
- Created: `crates/fl_view/src/ai.rs`, `crates/fl_sim/src/debug_query.rs`, `crates/fl_sim/tests/debug_query.rs` and `crates/fl_cli/src/query_cmd.rs`.
- Left untracked for the bless: the four `.new` files.
- No `Cargo.toml` or `Cargo.lock` change.

## Self-review

- **Determinism.**
  - Queries take `&World`, so they are read-only, and sim logic never calls them.
  - `DebugQueries` is External, so it is never hashed or saved.
  - `debug_query_does_not_change_state` checks that the state hash is the same before and after 10 queries.
  - `PlayerView` lives in Bevy-free `fl_view`.
- **The tests check behaviour.**
  - A query receives its args (the echo test).
  - The listing is sorted and includes both the built-in query and the feature's query.
  - Error strings are checked exactly.
  - The CLI tests check the exit code and stderr.
  - The `PlayerView` tests compare records for equality and check that a team which sees nothing gets an empty `visible`.
- **Test output** is clean apart from the expected insta snapshot failure.

## Snapshot and API diffs

The lead can also diff the `.new` files directly.

### formats__protocol_formats.snap -> .new
```diff
3c3
< assertion_line: 60
---
> assertion_line: 63
11a12
> DebugSetTerrainCmd: Struct([Named { name: "min_cell", value: TupleArray { content: U32, size: 2 } }, Named { name: "max_cell", value: TupleArray { content: U32, size: 2 } }, Named { name: "terrain", value: Str }])
14a16
> FollowCmd: Struct([Named { name: "leader", value: TypeName("SimId") }])
18a21
> RotateCmd: Struct([Named { name: "toward", value: TupleArray { content: I64, size: 2 } }])
```

### fl_sim.txt -> .new
```diff
523a524,533
> pub struct fl_sim::DebugQueries
> impl fl_sim::DebugQueries
> pub fn fl_sim::DebugQueries::names(&self) -> impl core::iter::traits::iterator::Iterator<Item = &'static str> + '_
> impl bevy_ecs::component::Component for fl_sim::DebugQueries where Self: core::marker::Send + core::marker::Sync + 'static
> pub type fl_sim::DebugQueries::Mutability = bevy_ecs::component::Mutable
> pub const fl_sim::DebugQueries::STORAGE_TYPE: bevy_ecs::component::StorageType
> pub fn fl_sim::DebugQueries::clone_behavior() -> bevy_ecs::component::clone::ComponentCloneBehavior
> pub fn fl_sim::DebugQueries::register_required_components(bevy_ecs::component::info::ComponentId, &mut bevy_ecs::component::required::RequiredComponentsRegistrator<'_, '_>)
> pub fn fl_sim::DebugQueries::relationship_accessor() -> core::option::Option<bevy_ecs::relationship::ComponentRelationshipAccessor<Self>>
> impl bevy_ecs::resource::Resource for fl_sim::DebugQueries where Self: core::marker::Send + core::marker::Sync + 'static
1161a1172
> pub fn fl_sim::SimBuilder::register_debug_query(&mut self, &'static str, fl_sim::DebugQueryFn) -> &mut Self
1672a1684
> pub fn fl_sim::debug_query(&bevy_ecs::world::World, &str, &serde_json::value::Value) -> core::result::Result<serde_json::value::Value, alloc::string::String>
1695a1708
> pub type fl_sim::DebugQueryFn = fn(&bevy_ecs::world::World, &serde_json::value::Value) -> core::result::Result<serde_json::value::Value, alloc::string::String>
```

### fl_view.txt -> .new
```diff
1a2,39
> pub mod fl_view::ai
> pub struct fl_view::ai::NullAi
> impl core::clone::Clone for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::clone(&self) -> Self
> impl core::cmp::Eq for fl_view::ai::NullAi
> impl core::cmp::PartialEq for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::eq(&self, &Self) -> bool
> impl core::default::Default for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::default() -> Self
> impl core::fmt::Debug for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::Copy for fl_view::ai::NullAi
> impl core::marker::StructuralPartialEq for fl_view::ai::NullAi
> impl fl_view::ai::AiController for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::id(&self) -> &'static str
> pub fn fl_view::ai::NullAi::think(&mut self, &fl_view::ai::PlayerView, &fl_protocol::tables::StringTables) -> alloc::vec::Vec<fl_view::driver::CommandDraft>
> pub struct fl_view::ai::PlayerView
> pub fl_view::ai::PlayerView::own: alloc::vec::Vec<fl_view::snapshot::EntityRecord>
> pub fl_view::ai::PlayerView::player: fl_protocol::ids::PlayerId
> pub fl_view::ai::PlayerView::team: fl_protocol::ids::TeamId
> pub fl_view::ai::PlayerView::tick: fl_protocol::ids::Tick
> pub fl_view::ai::PlayerView::visible: alloc::vec::Vec<fl_view::snapshot::EntityRecord>
> impl fl_view::ai::PlayerView
> pub fn fl_view::ai::PlayerView::from_snapshot(&fl_view::snapshot::SimSnapshot, fl_protocol::ids::PlayerId, fl_protocol::ids::TeamId) -> fl_view::ai::PlayerView
> impl core::clone::Clone for fl_view::ai::PlayerView
> pub fn fl_view::ai::PlayerView::clone(&self) -> Self
> impl core::cmp::PartialEq for fl_view::ai::PlayerView
> pub fn fl_view::ai::PlayerView::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_view::ai::PlayerView
> pub fn fl_view::ai::PlayerView::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_view::ai::PlayerView
> pub trait fl_view::ai::AiController: core::marker::Send + core::marker::Sync + 'static
> pub fn fl_view::ai::AiController::id(&self) -> &'static str
> pub fn fl_view::ai::AiController::think(&mut self, &fl_view::ai::PlayerView, &fl_protocol::tables::StringTables) -> alloc::vec::Vec<fl_view::driver::CommandDraft>
> impl fl_view::ai::AiController for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::id(&self) -> &'static str
> pub fn fl_view::ai::NullAi::think(&mut self, &fl_view::ai::PlayerView, &fl_protocol::tables::StringTables) -> alloc::vec::Vec<fl_view::driver::CommandDraft>
> pub fn fl_view::ai::ai_controller(&str) -> core::option::Option<alloc::boxed::Box<dyn fl_view::ai::AiController>>
97a136,138
> pub fl_view::runner::SimQuery::Debug
> pub fl_view::runner::SimQuery::Debug::args: serde_json::value::Value
> pub fl_view::runner::SimQuery::Debug::name: alloc::string::String
118a160
> pub fl_view::runner::SimReply::Debug(serde_json::value::Value)
292a335,337
> pub fl_view::SimQuery::Debug
> pub fl_view::SimQuery::Debug::args: serde_json::value::Value
> pub fl_view::SimQuery::Debug::name: alloc::string::String
313a359
> pub fl_view::SimReply::Debug(serde_json::value::Value)
411a458,487
> pub struct fl_view::NullAi
> impl core::clone::Clone for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::clone(&self) -> Self
> impl core::cmp::Eq for fl_view::ai::NullAi
> impl core::cmp::PartialEq for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::eq(&self, &Self) -> bool
> impl core::default::Default for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::default() -> Self
> impl core::fmt::Debug for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::Copy for fl_view::ai::NullAi
> impl core::marker::StructuralPartialEq for fl_view::ai::NullAi
> impl fl_view::ai::AiController for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::id(&self) -> &'static str
> pub fn fl_view::ai::NullAi::think(&mut self, &fl_view::ai::PlayerView, &fl_protocol::tables::StringTables) -> alloc::vec::Vec<fl_view::driver::CommandDraft>
> pub struct fl_view::PlayerView
> pub fl_view::PlayerView::own: alloc::vec::Vec<fl_view::snapshot::EntityRecord>
> pub fl_view::PlayerView::player: fl_protocol::ids::PlayerId
> pub fl_view::PlayerView::team: fl_protocol::ids::TeamId
> pub fl_view::PlayerView::tick: fl_protocol::ids::Tick
> pub fl_view::PlayerView::visible: alloc::vec::Vec<fl_view::snapshot::EntityRecord>
> impl fl_view::ai::PlayerView
> pub fn fl_view::ai::PlayerView::from_snapshot(&fl_view::snapshot::SimSnapshot, fl_protocol::ids::PlayerId, fl_protocol::ids::TeamId) -> fl_view::ai::PlayerView
> impl core::clone::Clone for fl_view::ai::PlayerView
> pub fn fl_view::ai::PlayerView::clone(&self) -> Self
> impl core::cmp::PartialEq for fl_view::ai::PlayerView
> pub fn fl_view::ai::PlayerView::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_view::ai::PlayerView
> pub fn fl_view::ai::PlayerView::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_view::ai::PlayerView
486a563,568
> pub trait fl_view::AiController: core::marker::Send + core::marker::Sync + 'static
> pub fn fl_view::AiController::id(&self) -> &'static str
> pub fn fl_view::AiController::think(&mut self, &fl_view::ai::PlayerView, &fl_protocol::tables::StringTables) -> alloc::vec::Vec<fl_view::driver::CommandDraft>
> impl fl_view::ai::AiController for fl_view::ai::NullAi
> pub fn fl_view::ai::NullAi::id(&self) -> &'static str
> pub fn fl_view::ai::NullAi::think(&mut self, &fl_view::ai::PlayerView, &fl_protocol::tables::StringTables) -> alloc::vec::Vec<fl_view::driver::CommandDraft>
494a577
> pub fn fl_view::ai_controller(&str) -> core::option::Option<alloc::boxed::Box<dyn fl_view::ai::AiController>>
```

### fl_protocol.txt -> .new
```diff
56a57,60
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::DebugSetTerrainCmd
> pub const fl_protocol::payloads::DebugSetTerrainCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::DebugSetTerrainCmd::NAME: &'static str
> pub const fl_protocol::payloads::DebugSetTerrainCmd::NEEDS_UNITS: bool
68a73,76
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::FollowCmd
> pub const fl_protocol::payloads::FollowCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::FollowCmd::NAME: &'static str
> pub const fl_protocol::payloads::FollowCmd::NEEDS_UNITS: bool
72a81,84
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::RotateCmd
> pub const fl_protocol::payloads::RotateCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::RotateCmd::NAME: &'static str
> pub const fl_protocol::payloads::RotateCmd::NEEDS_UNITS: bool
367a380,399
> pub struct fl_protocol::payloads::DebugSetTerrainCmd
> pub fl_protocol::payloads::DebugSetTerrainCmd::max_cell: (u32, u32)
> pub fl_protocol::payloads::DebugSetTerrainCmd::min_cell: (u32, u32)
> pub fl_protocol::payloads::DebugSetTerrainCmd::terrain: alloc::string::String
> impl core::clone::Clone for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::clone(&self) -> Self
> impl core::cmp::Eq for fl_protocol::payloads::DebugSetTerrainCmd
> impl core::cmp::PartialEq for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_protocol::payloads::DebugSetTerrainCmd
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::DebugSetTerrainCmd
> pub const fl_protocol::payloads::DebugSetTerrainCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::DebugSetTerrainCmd::NAME: &'static str
> pub const fl_protocol::payloads::DebugSetTerrainCmd::NEEDS_UNITS: bool
> impl serde_core::ser::Serialize for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
424a457,474
> pub struct fl_protocol::payloads::FollowCmd
> pub fl_protocol::payloads::FollowCmd::leader: fl_protocol::ids::SimId
> impl core::clone::Clone for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::clone(&self) -> Self
> impl core::cmp::Eq for fl_protocol::payloads::FollowCmd
> impl core::cmp::PartialEq for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_protocol::payloads::FollowCmd
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::FollowCmd
> pub const fl_protocol::payloads::FollowCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::FollowCmd::NAME: &'static str
> pub const fl_protocol::payloads::FollowCmd::NEEDS_UNITS: bool
> impl serde_core::ser::Serialize for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
442a493,510
> pub struct fl_protocol::payloads::RotateCmd
> pub fl_protocol::payloads::RotateCmd::toward: fl_math::vec::FxVec2
> impl core::clone::Clone for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::clone(&self) -> Self
> impl core::cmp::Eq for fl_protocol::payloads::RotateCmd
> impl core::cmp::PartialEq for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_protocol::payloads::RotateCmd
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::RotateCmd
> pub const fl_protocol::payloads::RotateCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::RotateCmd::NAME: &'static str
> pub const fl_protocol::payloads::RotateCmd::NEEDS_UNITS: bool
> impl serde_core::ser::Serialize for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
631a700,719
> pub struct fl_protocol::DebugSetTerrainCmd
> pub fl_protocol::DebugSetTerrainCmd::max_cell: (u32, u32)
> pub fl_protocol::DebugSetTerrainCmd::min_cell: (u32, u32)
> pub fl_protocol::DebugSetTerrainCmd::terrain: alloc::string::String
> impl core::clone::Clone for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::clone(&self) -> Self
> impl core::cmp::Eq for fl_protocol::payloads::DebugSetTerrainCmd
> impl core::cmp::PartialEq for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_protocol::payloads::DebugSetTerrainCmd
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::DebugSetTerrainCmd
> pub const fl_protocol::payloads::DebugSetTerrainCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::DebugSetTerrainCmd::NAME: &'static str
> pub const fl_protocol::payloads::DebugSetTerrainCmd::NEEDS_UNITS: bool
> impl serde_core::ser::Serialize for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_protocol::payloads::DebugSetTerrainCmd
> pub fn fl_protocol::payloads::DebugSetTerrainCmd::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
710a799,816
> pub struct fl_protocol::FollowCmd
> pub fl_protocol::FollowCmd::leader: fl_protocol::ids::SimId
> impl core::clone::Clone for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::clone(&self) -> Self
> impl core::cmp::Eq for fl_protocol::payloads::FollowCmd
> impl core::cmp::PartialEq for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_protocol::payloads::FollowCmd
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::FollowCmd
> pub const fl_protocol::payloads::FollowCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::FollowCmd::NAME: &'static str
> pub const fl_protocol::payloads::FollowCmd::NEEDS_UNITS: bool
> impl serde_core::ser::Serialize for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_protocol::payloads::FollowCmd
> pub fn fl_protocol::payloads::FollowCmd::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
797a904,921
> pub struct fl_protocol::RotateCmd
> pub fl_protocol::RotateCmd::toward: fl_math::vec::FxVec2
> impl core::clone::Clone for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::clone(&self) -> Self
> impl core::cmp::Eq for fl_protocol::payloads::RotateCmd
> impl core::cmp::PartialEq for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_protocol::payloads::RotateCmd
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::RotateCmd
> pub const fl_protocol::payloads::RotateCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::RotateCmd::NAME: &'static str
> pub const fl_protocol::payloads::RotateCmd::NEEDS_UNITS: bool
> impl serde_core::ser::Serialize for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_protocol::payloads::RotateCmd
> pub fn fl_protocol::payloads::RotateCmd::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
947a1072,1075
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::DebugSetTerrainCmd
> pub const fl_protocol::payloads::DebugSetTerrainCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::DebugSetTerrainCmd::NAME: &'static str
> pub const fl_protocol::payloads::DebugSetTerrainCmd::NEEDS_UNITS: bool
959a1088,1091
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::FollowCmd
> pub const fl_protocol::payloads::FollowCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::FollowCmd::NAME: &'static str
> pub const fl_protocol::payloads::FollowCmd::NEEDS_UNITS: bool
963a1096,1099
> impl fl_protocol::command::CommandPayload for fl_protocol::payloads::RotateCmd
> pub const fl_protocol::payloads::RotateCmd::DEBUG_ONLY: bool
> pub const fl_protocol::payloads::RotateCmd::NAME: &'static str
> pub const fl_protocol::payloads::RotateCmd::NEEDS_UNITS: bool
```
