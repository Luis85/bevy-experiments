### Task 12: `fl_client` camera, ground and lighting

**Files:** `crates/fl_client/src/camera.rs` and `crates/fl_client/src/map_view.rs`, and no others.

**Interfaces — Produces:**
```rust
// map_view.rs
#[derive(Resource, Clone, Debug)] pub struct MapView { pub size_m: Vec2, pub ground_color: Color }
// MapViewPlugin: on Startup (if MapView exists) spawn ground plane centred at (w/2, 0, h/2), a DirectionalLight
// (shadow_maps_enabled, cascades tuned for ≤ 2 km), GlobalAmbientLight; F3 toggles a 100 m gizmo grid.
// camera.rs
#[derive(Component, Clone, Debug)] pub struct RtsCamera { pub focus: Vec3, pub yaw: f32, pub pitch: f32, pub distance: f32, pub view_height: f32 }
//   defaults: yaw 45°, pitch 35.264°, distance 400, view_height 120
#[derive(Resource, Clone, Debug)] pub struct CameraSettings { pub pan_speed: f32 /* view-heights per s, 1.0 */, pub edge_margin_px: f32 /* 8 */,
    pub zoom_min: f32 /* 15 */, pub zoom_max: f32 /* 600 */, pub zoom_step: f32 /* 0.1 */, pub rotate_speed: f32 /* 90°/s in rad */ }
pub fn rig_transform(r: &RtsCamera) -> Transform;                       // as in api-reference §2.2
pub fn pan_input(keys: &ButtonInput<KeyCode>) -> Vec2;                   // WASD + arrows, (x right, y forward)
pub fn edge_pan_input(cursor: Option<Vec2>, window: Vec2, margin: f32) -> Vec2;   // zero if window has zero size or cursor None
pub fn pan_delta(input: Vec2, yaw: f32, view_height: f32, speed: f32, dt: f32) -> Vec3;
pub fn clamp_focus(focus: Vec3, size_m: Vec2) -> Vec3;
pub fn zoom(view_height: f32, scroll: f32, s: &CameraSettings) -> f32;
pub fn snap_yaw(yaw: f32) -> f32;                                        // nearest 45°
// CameraPlugin: spawns Camera3d + orthographic projection (ScalingMode::FixedVertical) + RtsCamera focused on map centre;
// Update systems: pan (keys + edge), zoom (AccumulatedMouseScroll), rotate (Q/E free, R snap), clamp, apply_rig.
```
- [ ] **Step 1: Write the failing tests.** Unit tests in `camera.rs`:
  - `pan_forward_moves_along_yaw`: yaw 0 and input (0,1) give delta.z < 0 and |delta.x| < 1e-5.
  - `edge_pan_left_edge`: cursor (2, 300) in an 800×600 window gives x = −1.
  - `edge_pan_zero_size_window`: gives `Vec2::ZERO`.
  - `zoom_is_clamped`: 10 000 wheel steps each way stay within [15, 600].
  - `clamp_focus_inside_map`, `snap_yaw_nearest_45`.
  - `rig_looks_at_focus`: `rig_transform(..).forward()` points from the translation to the focus, within 1e-4.

  One app test:
  - `wasd_moves_focus`: `MinimalPlugins` plus a manually inserted `ButtonInput<KeyCode>` resource. Press `KeyW`, set `TimeUpdateStrategy::ManualDuration(100 ms)`, and run 3 updates; the focus changes.
  - The systems must not require a window, so use `Option<Single<&Window, With<PrimaryWindow>>>`.
- [ ] **Step 2: Run** `cargo test -p fl_client camera`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_client camera`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_client): isometric RTS camera, ground and lighting`.

