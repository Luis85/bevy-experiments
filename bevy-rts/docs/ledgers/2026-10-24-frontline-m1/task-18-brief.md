### Task 18: Overlays — paths, passability, cells; BRP `fl.debug_query`

**Track:** client · **Refs:** ADR-0015, spec §11 · **Lane 2** (after Tasks 7 and 8 are merged)

**Files:**
- Create: `crates/fl_client/src/overlays.rs`, `crates/fl_client/tests/overlays.rs`
- Modify: `crates/fl_client/src/{lib,debug/remote}.rs`, `crates/fl_client/tests/remote.rs`

**Interfaces:**
- Consumes: `SimQuery::Debug`, `SimReply::Debug` (Task 2); the JSON of `core.paths`, `core.passability`, `core.clusters` (Tasks 5, 7, 8); `OverlayRegistry`, `PendingReplies`, `send_query`, `Selection`, `CursorGround`.
- Produces:
```rust
#[derive(Resource, Default, Debug)] pub struct OverlayState { pub paths: bool, pub passability: bool, pub cells: bool }
#[derive(Clone, Debug, PartialEq)] pub struct BlockedCells { pub cells: UVec2, pub version: u64, pub runs: Vec<(u32, u32)> }
#[derive(Clone, Debug, PartialEq)] pub struct ClusterView { pub cluster_cells: u32, pub clusters: UVec2, pub entrances: Vec<UVec2> }
pub fn decode_paths(v: &serde_json::Value) -> Result<BTreeMap<SimId, Vec<Vec2>>, String>;   // raw Fx → metres
pub fn decode_passability(v: &serde_json::Value) -> Result<BlockedCells, String>;
pub fn decode_clusters(v: &serde_json::Value) -> Result<ClusterView, String>;
pub fn grid_lines(centre: Vec2, radius_m: f32, step_m: f32) -> Vec<(Vec2, Vec2)>;   // lines on multiples of step_m within the square centre ± radius
pub struct OverlaysPlugin;
    // overlays "paths" (F5), "passability" (F6), "cells" (F7) in OverlayRegistry; each active overlay polls its query every 15 frames;
    // paths: the selected units; passability and cells: {"unit": the first selected unit}, else {"class":"Tracked","size":1};
    // gizmos: path polylines from each unit's Transform through its corners; blocked cells within 120 m of the camera focus;
    //   cell lines within 60 m of the cursor, cluster borders in view, entrance markers
// BRP (debug_remote): fl.debug_query { "name": .., "args": .. } → the reply JSON, or a JSON-RPC error with the query's message
```

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn decode_paths_json()                     // {"3": {"corners": [[raw(10), raw(20)]], ..}} → {#3: [(10, 20)]}
#[test] fn decode_passability_runs()               // {"cells":[4,2],"version":7,"blocked":[[0,2],[6,1]]} → runs [(0,2),(6,1)]
#[test] fn decode_clusters_json()
#[test] fn paths_overlay_polls_selected_units()    // overlay on, selection {#1,#2}: the fake runner gets SimQuery::Debug { name "core.paths", args {"units":[1,2]} } within 15 frames
#[test] fn passability_follows_selected_unit()     // args {"unit": 1}
#[test] fn overlay_toggles_through_registry()      // OverlayRegistry toggle "cells" → OverlayState.cells true
#[test] fn grid_lines_count()                      // centre (50,50), radius 10, step 4 → 6 vertical + 6 horizontal lines (multiples of 4 in [40,60]:
                                                   //   40, 44, 48, 52, 56, 60)
// crates/fl_client/tests/remote.rs
#[test] fn brp_debug_query_round_trip()            // fl.debug_query core.debug_queries → JSON "queries"
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test overlays`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): path, passability and cell/cluster overlays from debug queries; fl.debug_query` with trailer `Refs: ADR-0015`.

---

## Phase III — hot reload, scale and determinism

