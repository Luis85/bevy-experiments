# Task 12 report

Implemented camera.rs (RtsCamera, CameraSettings, pure fns, pan/zoom/rotate/clamp/apply_rig systems, Option<Single<Window>> so no window needed) and map_view.rs (MapView, ground plane at (w/2,0,h/2), DirectionalLight with 4 cascades up to 2000 m, GlobalAmbientLight, F3 100 m gizmo grid).

Tests: `cargo test -p fl_client camera` -> 8 passed, 0 failed (all brief tests incl. wasd_moves_focus). `cargo fmt` applied.

NOT verified: full gate (`clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, `check_deps.sh`) was started but had not finished after ~hours due to heavy contention on shared target dir C:/t/fl (two toolchains rebuilding bevy). Controller should run the gate. map_view.rs has no tests (only compiled via the camera test build).

Commit: 99e42d6. Files: crates/fl_client/src/camera.rs, map_view.rs.

## Fix round 1

Changes (camera.rs, map_view.rs only):
1. Near-plane clipping: chose a zoom-dependent rig distance over a larger ortho near plane (the near plane is fixed at 0.1 and depth precision/shadow cascades benefit from a tight range). Visible ground depth spans `distance -/+ view_height / (2 tan pitch)`. New `ground_depth_range(rig)` and `fit_distance(vh, pitch, base) = max(base, vh/(2 tan pitch) + 50)`; `zoom_camera` and `spawn_camera` apply it. At zoom_max 600: distance 474, depth 50..898 (near 0.1, far 2000). `CAMERA_NEAR/FAR` consts shared.
2. Cascades: `map_view::cascade_config(rig)` sets minimum_distance to the nearest visible ground depth, maximum to the farthest, first cascade ends at 15% of the span; `tune_cascades` (PostUpdate) re-derives it when `RtsCamera` changes.
3. Scroll: `scroll_lines(delta, unit)` normalises Pixel by /100; `zoom()` clamps the per-call factor to [0.1, 10] so height never reaches <= 0 before the clamp.
4. `wasd_moves_focus` now asserts direction (-1,0,-1)/sqrt2 at the default 45 deg yaw and a displacement bound.

Tests: new `max_zoom_ground_is_inside_clip_range` (also asserts the stock 400 distance would clip), `every_zoom_level_is_inside_clip_range`, `pixel_scroll_is_normalised_and_zoom_stays_positive`, `map_view::cascades_start_at_visible_ground`; strengthened `wasd_moves_focus`. `cargo test -p fl_client`: 12 passed. (Tests and implementation were written in one edit before the first compile, so no separate red run was observed.)

Command: `cargo xtask gate task --track client --base 51254b7`
```
PASS fmt (3.2s)
PASS clippy-drift (2.2s)
PASS check-scope (0.3s)
PASS protected (0.3s)
PASS test-guard (0.5s)
PASS clippy (24.7s)
PASS arch-check (7.6s)
WARN test-headless (58.7s): no test matched `(((package(=fl_sim) | package(=fl_test_utils)) & test(/smoke_/))) & not (test(/gpu_/) | test(/brp_/) | test(/milestone_/))`
PASS test-client (25.7s)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 124s
```
