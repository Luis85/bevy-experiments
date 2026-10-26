### Spec Compliance

- ✅ Spec compliant. All brief files and interfaces are present, and the six named tests plus extras exist.
- Orientation matches ADR-0014: `index = cy*cells_x + cx` with y north, vertex z = (cy+0.5)*cell, and world z = sim y (`sim_to_world`, view.rs:86). Height scaling `lo + (hi-lo)*raw/65535` (terrain.rs:329) matches the plan's `TerrainGrid::height_at` (plan l.624). Winding is up-facing and tested.
- Layers are read from `MapDef.layers` in content only; `fl_client` has no PNG or sim dependency.
- ⚠️ Not verified from the diff: that `sim_to_world` and `MapView` are rebuilt on a scenario `LoadRequest` reload, so the terrain follows the new map. Check the loader path.

### Strengths
- Mesh, unit placement and picking all go through one `height_at`, so they agree with each other.
- `ray_ground` terminates (4096-step cap, bounded bisection), returns None for up/level rays and for rays starting underground, and returns the terrain y.
- Good tests: mesh winding, an oblique ray, and a real fake-runner unit test.
- `fl_game` gets a builder test.

### Issues

Critical: none.

Important: none.

Minor:
- terrain.rs:349-353: `height_at` clamps, so a ray that misses the map still "hits" the edge-extended heightfield. `ray_ground` returns a point outside the map (and the `far >= end` branch at :458 can too), contrary to the doc's "None when it misses". This matters for the spawn panel and orders when the cursor is off-map. Fix: return None if the hit x/z is outside `[0, cells*cell_m]`, or let the consumer reject it. No test covers a miss.
- terrain.rs:417 vs :362-364: the mesh uses a triangle split along the c-b diagonal, while `height_at` and picking are bilinear. On non-planar quads the surface differs by up to roughly a quarter of the cell twist. Units and the cursor can therefore sit a few cm to dm off the drawn surface. This is invisible at demo scale, and it matches the brief's interface.
- Half-cell border (2 m at 4 m cells): the mesh spans only the cell centres, so the outer 2 m of the map is undrawn, and picking still reports hits there. It does not matter for the demo. The 1 km map is large and the strip is a sliver, and the clear colour shows through.
- Camera focus and `ground_depth_range` still assume y=0. On the 12 m hill the pan/zoom focus and the cascade near range are off by up to 12 m. This is acceptable for the demo and is a known follow-up.
- map_view.rs: `TerrainView` is a snapshot taken at build time. `debug.set_terrain` (M1) changes sim heights and types, but the mesh and unit heights stay stale. Types-only edits will show the wrong colour, and height edits the wrong unit y. Acceptable if `set_terrain` is a debug path only, but note it.
- selection.rs:603: the ring is lifted by `Vec3::Y * RING_LIFT` but stays flat, so on slopes it can partly sink into the terrain. The change is sound otherwise (the old `with_y` would have put rings at y=0.05 under hill units). The debug grid change (map_view.rs:216-227) is also sound: it is sampled every 4 m and lifted 0.1 m. It allocates points every frame while the grid is visible, which is fine.
- Process: the report admits no RED run (implementation before tests). The tests are still meaningful. The temp-dir scratch mod in tests/terrain.rs leaves a directory behind if `load_dirs` panics before the cleanup. The oblique-ray test only asserts that the hit is on the surface, not the expected point.

### Assessment

**Task quality:** Approved

**Reasoning:** Orientation, height scaling, unit placement and picking agree with ADR-0014 and the plan's sim-side `height_at`. The march/bisect is bounded and robust. The remaining items are minor: off-map picking should return None, and the camera, mesh border and stale-view limits are acceptable for the demo.
