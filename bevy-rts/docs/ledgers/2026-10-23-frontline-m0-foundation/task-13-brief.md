### Task 13: `fl_client` sim host, sync, interpolation, placeholders

**Files:** `crates/fl_client/src/view.rs` and `crates/fl_client/src/placeholder.rs`, and no others.

**Interfaces — Consumes:**
- fl_sim: `SimIndex`, `Pose`, `Owner`, `UnitType`, `SimClock`, `Players`, `CommandRegistry`, `submit`, `SimTick`.
- fl_content: `VisualDef`, `Placeholder`, `Faction`.

**Produces:**
```rust
// view.rs
#[derive(Resource)] pub struct SimHost { world: World, outbox: Vec<PlayerCommand>, pub paused: bool, pub step_requests: u32, last_tick: std::time::Duration }
impl SimHost {
    pub fn new(world: World) -> Self;                   // from build_sim_app(..) after setup: std::mem::take(app.world_mut())
    pub fn world(&self) -> &World;
    pub fn world_mut(&mut self) -> &mut World;          // debug/test use only; gameplay goes through issue()
    pub fn tick(&self) -> Tick;
    pub fn issue<P: CommandPayload>(&mut self, player: PlayerId, units: Vec<SimId>, payload: &P, queue: bool);  // stamped at the next tick to run
    pub fn last_tick_duration(&self) -> std::time::Duration;   // measured with bevy::platform::time::Instant (allowed in client)
    pub fn step(&mut self);                             // submit outbox, run SimTick once, measure, then drain fl_sim's Journal into `journal`
    pub fn journal(&self) -> &std::collections::VecDeque<JournalEntry>;  // last 10 000 entries (ring), feeds the combat log / BRP / file sink
}
#[derive(Resource, Clone)] pub struct ClientContent(pub Arc<ContentDb>);
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)] pub struct LocalPlayer(pub PlayerId);
#[derive(Component, Clone, Copy, Debug)] pub struct SimLink(pub SimId);
#[derive(Component, Clone, Copy, Debug)] pub struct UnitVisual { pub unit: UnitTypeId, pub player: PlayerId }
#[derive(Component, Clone, Copy, Debug)] pub struct ViewPose { pub prev: Vec3, pub curr: Vec3, pub prev_yaw: f32, pub curr_yaw: f32 }
#[derive(Resource, Default)] pub struct SimLinks(pub BTreeMap<SimId, Entity>);
pub fn sim_to_world(pos: FxVec2, height: f32) -> Vec3;      // Vec3::new(x, height, y)
pub fn heading_to_yaw(h: Angle) -> f32;                      // -h.to_radians_f32()
pub fn lerp_yaw(a: f32, b: f32, t: f32) -> f32;              // shortest arc
// SimViewPlugin: Time::<Fixed>::from_hz(SIM_TICK_HZ as f64); FixedUpdate: (step_sim, sync_views).chain()
//   step_sim: unless paused, host.step(); when paused, run step_requests ticks then zero it.
//   sync_views: merge SimIndex with SimLinks — spawn (SimLink, UnitVisual, ViewPose, Transform, Visibility) for new ids,
//   despawn (recursive) for vanished ids, shift curr→prev and copy new curr for all others.
// Update: interpolate_views sets Transform from ViewPose with Time<Fixed>::overstep_fraction().
// placeholder.rs
// PlaceholderPlugin: Startup builds PlaceholderAssets { meshes per UnitTypeId (from VisualDef, or a Box from Hull footprint), materials per PlayerId (faction colour) };
// on Added<UnitVisual> attach child meshes: Tank = hull cuboid + turret cuboid + barrel cylinder (along local +X), Truck = cab + bed,
// Infantry = capsule, Box = cuboid. Meshes/materials are shared handles so Bevy batches them.
```
- [ ] **Step 1: Write the failing tests.** They use `MinimalPlugins` and `SimViewPlugin` only, with no render.
  - Pure functions:
    - `sim_to_world_axes`.
    - `heading_90_faces_plus_z`: rotating +X by `heading_to_yaw(90°)` gives +Z within 1e-5.
    - `lerp_yaw_wraps`: from 170° to −170° at t = 0.5 gives ±180°.
  - App tests, with a SimHost built from `base.testbed` and `TimeUpdateStrategy::ManualDuration(50 ms)`:
    - `visuals_match_sim_units`: 3 updates give 10 `SimLink` entities.
    - `debug_spawn_adds_visual`: issuing `DebugSpawnCmd` gives 11.
    - `despawn_removes_visual`: despawning a SimId directly in the host world leaves 9 after the next fixed step, with no panic in interpolation.
    - `paused_host_does_not_tick`: when paused, the tick stays the same; `step_requests = 2` advances it by 2.
  - `interpolation_midpoint`: ViewPose with prev (0,0,0) and curr (10,0,0) at overstep 0.5 gives x = 5.
- [ ] **Step 2: Run** `cargo test -p fl_client view`. Expected: FAIL.
- [ ] **Step 3: Implement.** If `World` cannot live in a `Resource` on 0.20, fall back to `insert_non_send` and keep the same `SimHost` API.
- [ ] **Step 4: Run** `cargo test -p fl_client view`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_client): sim host, sim→client sync, interpolation, placeholder visuals`.

---

## Phase C — parallel tracks (worktrees, after their Phase B inputs are merged)

