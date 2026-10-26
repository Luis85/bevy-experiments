### Task 17: Terrain view — heightmap mesh, unit heights, ground picking

**Track:** client · **Refs:** spec §10 · **Lane 2** (after Task 4)

**Files:**
- Create: `crates/fl_client/src/terrain.rs`, `crates/fl_client/tests/terrain.rs`
- Modify:
  - `crates/fl_client/src/{map_view,view,lib}.rs`
  - `crates/fl_client/src/debug/spawn_panel.rs` (`update_cursor_ground` uses `ray_ground`)
  - `crates/fl_game/src/builder.rs` (`MapView.terrain` from the scenario's map)

**Interfaces:**
- Consumes: `MapDef.{terrain, layers}`, `TerrainDef` (Task 3); base layers (Task 4); `MapView`, `sim_to_world`, `CursorGround`.
- Produces:
```rust
#[derive(Clone, Debug, PartialEq)] pub struct TerrainView { pub cells: UVec2, pub cell_m: f32, pub heights_m: Vec<f32>, pub colors: Vec<[f32; 3]> }   // per cell, y north
impl TerrainView {
    pub fn from_content(map: &MapDef, terrains: &KindTable<TerrainDef>) -> Option<TerrainView>;   // None without layers
    pub fn height_at(&self, x_m: f32, y_m: f32) -> f32;   // bilinear between cell centres, clamped at the edges
}
pub struct MapView { pub size_m: Vec2, pub ground_color: Color, pub terrain: Option<TerrainView> }   // existing resource gains `terrain`
pub fn terrain_mesh(t: &TerrainView) -> Mesh;   // a vertex per cell centre (x = (cx + 0.5) × cell, z = (cy + 0.5) × cell, y = height), vertex colours, normals, 2 triangles per quad
pub fn ray_ground(ray: Ray3d, terrain: Option<&TerrainView>) -> Option<Vec3>;   // y = 0 plane without terrain; else march 1 m from the max-height plane, then bisect 8 times
// MapViewPlugin spawns the mesh in place of the flat plane when `terrain` is Some; unit visuals stand at height_at(x, y)
```

- [ ] **Step 1: Write the failing tests:**
```rust
#[test] fn terrain_view_from_base_testbed()     // cells (250, 250); the colour at cell (10, 124) == base.water's
#[test] fn height_at_bilinear_and_clamped()     // a 2×1 view with heights 0 and 2: the midpoint → 1.0; beyond the edge → the edge height
#[test] fn terrain_mesh_counts()                // a 4×3 view: 12 vertices, 36 indices
#[test] fn ray_ground_hits_hill()               // a straight-down ray at the testbed hill centre → y within 0.05 of height_at
#[test] fn ray_ground_flat_without_terrain()
#[test] fn units_stand_on_terrain()             // fake-runner record at the hill → Transform.translation.y within 0.05 of height_at
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client --test terrain`. Expected: FAIL.
- [ ] **Step 3: Implement.** Check `Mesh`, `PrimitiveTopology`, `Mesh::ATTRIBUTE_COLOR` and `Ray3d` with the `bevy-api` skill.
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_client -p fl_game`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_client): heightmap terrain mesh coloured by terrain type, units on the ground, heightfield cursor picking`.

