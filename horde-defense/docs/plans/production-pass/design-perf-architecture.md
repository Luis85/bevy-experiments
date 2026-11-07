# perf-architecture: merged spec (base: Design B; judges 3/3 for B; grafts from A)

## 0. Summary
The order is: measure honestly, then restructure, then optimise. Every gate reads `docs/PERF_BUDGET.json`.

- The design adds no Game save field and moves neither GOLDEN nor GAMEPLAY_TRACE.
- Persisted additions: `Settings.vsync` and `Settings.low_latency`. They are serde-defaulted in settings.json, which sits outside SAVE_VERSION.
- The one core rule-adjacent change is widening `ProgressionSnapshot::validate`'s enemy_cap bound from 224 to `STAGED_ENEMY_CAP_MAX = 400`, so the stress scene can draw 400 enemies for real.
  - Authored caps stay ≤ 224.
  - This needs save-determinism sign-off before the v7 freeze. A fallback is defined in §5.

### Binding numbers (wave 0, HEAD 7bd5de8, dev, VSync on, RTX 1000 Ada, Vulkan)
| scene | size | Interface mean (v2 ui::draw) | gate |
|---|---|---|---|
| progressed-peak (17/30, 225 enemies) | 1440x900@1.0 | 1.6128 ms | ≤ 1.129 ms (0.70x) |
| progressed-peak | 1050x700@1.35 | 1.4719 ms | ≤ 1.030 ms |
| peak (52 enemies) | normal / small | 1.1904 / 1.0752 ms | not above wave 0 |

Other wave-0 values: p99 17.5-17.7 ms, 0 frames over 50 ms. Kills per stage attempt are 101-284 (smoke.txt).

The Interface gate always compares against these wave-0 numbers. It never switches to a re-baseline. If the v3 re-baseline on unchanged code differs from wave 0 by more than 5%, stop and investigate instrumentation overhead, and write the reason in the plan. The goalpost does not move.

## 1. Frame report v3 and probe (diagnostics)

### PerfProbe replaces the global atomics
`#[derive(Resource, Clone)] pub struct PerfProbe(Arc<PerfCounters>)` replaces the statics `WORK_NS` and `MEASURE_WORK` at diagnostics.rs:18-19.
- `PerfCounters` holds atomics for work phases, frame spans (main begin, egui pass, tessellate, extract, render schedule, present wait), count samples and a render-sample sequence number.
- The same Arc is inserted into the main World and into RenderApp.

### One install entry point
- `pub fn install(app: &mut App, config: &LaunchConfig)` replaces `enable_work_timing` at main.rs:87 and is called after DefaultPlugins.
- It adds `PerfPlugin` only when `config.perf().is_some()` and the `review` feature is on.
- `PerfPlugin` inserts PerfProbe, adds the span systems, the extract wrapper, the egui probes, the GPU span and `RenderDiagnosticsPlugin`.
- Without a perf config, install does nothing: no resource, no plugin, no systems.

### Callers
- `work_timer(probe: Option<&PerfProbe>, phase) -> Option<WorkTimer>`.
- `ui::draw` and the runner systems take `Option<Res<PerfProbe>>`. `RuntimeCtx` gains `perf: Option<&PerfProbe>`, and tests pass `None`.

### Span marks
- **main_cpu_ms**: a First-schedule begin mark to the end of Last. `capture_performance` moves from Update (review_tools.rs:14-21) to the end of Last, which fixes the one-frame-late Interface attribution.
- **egui_pass_ms**: in PostUpdate, from an exclusive system in `EguiPostUpdateSet::EndPass` directly before bevy_egui's `run_egui_context_pass_loop_system`, to `.after(EguiPostUpdateSet::EndPass)`. This covers every multipass draw plus end_pass, and not the PostUpdate systems unordered against `EndPass`. (Up to W1-61 the span opened `.before(EguiPostUpdateSet::EndPass)` and held those systems; review commit `7abb0ab`, 2026-11-07.)
- **egui_tessellate_ms**: around the `ProcessOutput` set (bevy_egui output.rs:51).
- **interface_total_ms** = egui_pass + egui_tessellate.
- **extract_ms**: a wrapper installed with `SubApp::take_extract`/`set_extract` on RenderApp in `Plugin::finish` (bevy_app-0.20.1 sub_app.rs:174, :207). It times extraction only, never the pipelined channel wait. A missing RenderApp is tolerated, as in MinimalPlugins tests.
- **render_schedule_ms**: a begin mark `.before(RenderSystems::ExtractCommands)` and an end mark `.after(RenderSystems::PostCleanup)`.
- **present_wait_ms**: `.after(create_surfaces).before(prepare_windows)` to `.after(prepare_windows)` in PrepareViews (view/window/mod.rs:239-258), where `get_current_texture` blocks. Surface (re)configuration in `create_surfaces` stays render-world CPU. (Up to W1-61 the span was not ordered after `create_surfaces`; review commit `7abb0ab`, 2026-11-07.)
- **render_world_cpu_ms** = extract + render_schedule − present_wait.
- **cpu_frame_ms** = main_cpu(i) + render_world_cpu(latest completed render sample, i−1 under pipelining), summed per sample. It is never a sum of means. The scope string documents this.
- **critical_path_ms** = max(main, render_world) when pipelined, otherwise the sum. Informational only.

### FrameReport v3 (diagnostics/frame_report.rs)
- `version: 3`. Every v2 key keeps its name and meaning. `work_mean_ms` and `work_max_ms` stay `[Simulation, Persistence, Interface(ui::draw)]`.
- New fields:
  - `phases`: the spans above, each `Stats {mean, p50, p95, p99, max}`.
  - `interface_breakdown_mean_ms {battle_frame_build, battlefield_paint, backdrop}`. These are sub-spans inside ui::draw, perf launches only.
  - `egui {shapes, clipped_primitives, mesh_vertices, mesh_indices, textured_quads, passes_per_frame}` as `CountStats {mean, max}`.
    - shapes come from `EguiFullOutput.0.shapes.len()` between EndPass and ProcessOutput.
    - The rest come from `EguiRenderOutput.paint_jobs`.
    - textured_quads = Σ indices/6 over meshes whose texture is not `TextureId::default()`.
  - `scene_load {visible_enemies, corpses, settled_corpses_drawn, collapsing_corpses_drawn, particles, fx_views}` as CountStats.
  - `present {requested_mode, source: settings|launch-override, pipelined_rendering, desired_maximum_frame_latency, low_latency}`.
  - `build {profile_label (LASTLIGHT_PERF_PROFILE, else "unlabelled"), features (cfg!(feature="review"), cfg!(feature="dev")), debug_assertions}`.
  - `gpu` (see below).
  - `longest_frames[].phases`.
- The report is assembled by a pure `fn build_report(&ReportInput) -> FrameReport`.
- `fn cpu_frames(main: &[f64], render: &[f64]) -> Vec<f64>`, `fn stats(&[f64]) -> Stats` and `fn egui_counts(&[ClippedPrimitive]) -> EguiCounts` are pure functions.

### GPU time (diagnostics/gpu_span.rs, review-gated)
- The file header names the pins: bevy =0.20.0 (lock 0.20.1) and bevy_egui =0.43.1.
- RenderDiagnosticsPlugin, plus two Core2d systems around `bevy_egui::render::egui_pass`:
  - `perf_egui_gpu_begin`: `.after(Core2dSystems::MainPass).before(egui_pass)`;
  - `perf_egui_gpu_end`: `.after(egui_pass).before(upscaling)`.
  - Both call `begin_time_span`/`end_time_span("lastlight/egui")` on the RenderContext diagnostic recorder.
- Report shape: `gpu {supported, reason, egui_pass_ms: Option<Stats>, passes: {name: mean_ms}}`.
- `fn gpu_timing_support(features, backend) -> Result<(), &'static str>` returns one of: "no TIMESTAMP_QUERY", "no TIMESTAMP_QUERY_INSIDE_ENCODERS", "backend without timestamps".
- Negative spans, or spans longer than the frame interval, give "unsupported: inconsistent timestamps".
- GPU numbers are informational and never gated.

## 2. Render plan: VSync, Low latency, pipelined rendering (new `crates/horde_game/src/render_plan.rs`, un-gated)
- `pub fn present_mode_for(vsync, low_latency) -> PresentMode`:
  - low_latency gives Mailbox. Bevy falls back Mailbox → Immediate → Fifo itself (view/window/mod.rs:490-494).
  - vsync gives Fifo, today's default.
  - neither gives AutoNoVsync.
- `pub const PIPELINED_DEFAULT: bool = true` until step W1.14 decides it.
- `pub fn pipelined_for(low_latency, perf_override: Option<bool>) -> bool` = `perf_override.unwrap_or(!low_latency && PIPELINED_DEFAULT)`.
- `pub struct RenderPlan { present_mode, pipelined, source: PlanSource { Settings, LaunchOverride } }`.
- `pub fn render_plan(settings: &Settings, perf: Option<&PerfOptions>) -> RenderPlan`. A perf override wins.
- `pub fn default_plugins(config, asset_directory, &RenderPlan) -> PluginGroupBuilder`:
  - moves the group at main.rs:100-130 unchanged;
  - sets `Window.present_mode`;
  - calls `.disable::<PipelinedRenderingPlugin>()` when `!plan.pipelined`.
- main.rs, before App::new:
  `let startup_settings = Settings::load(&ProfilePaths::from_config(&config).settings()); let plan = render_plan(&startup_settings, config.perf());`
  - This is a file read through existing APIs, not an environment read. `FrontendParts::load` still loads the authoritative Settings later.
  - Review and perf launches use a temp profile, so they always start from defaults plus overrides.
- `apply_present_mode`: an Update system with `run_if(resource_changed::<Settings>)`. It writes `Window.present_mode` when the value differs and no perf override is active. Present mode therefore applies live; a pipelined change applies on restart.
- Settings gains `vsync: bool` (default true) and `low_latency: bool` (default false). They are covered by the struct-level `#[serde(default)]` at settings.rs:43. bindings stays `[String; 8]`, unchanged.
- PerfOptions (launch.rs:50-58) gains, from env vars in REVIEW_VARIABLES (launch.rs:175-192) and HELP:
  - `present_mode: Option<PresentModeOverride {Fifo, Mailbox, Immediate, AutoNoVsync}>` from `LASTLIGHT_PERF_PRESENT_MODE`;
  - `pipelined: Option<bool>` from `LASTLIGHT_PERF_PIPELINED` = 0|1;
  - `profile_label: Option<String>` from `LASTLIGHT_PERF_PROFILE`.
  - An invalid value is a typed LaunchConfig error, as for LASTLIGHT_PERF_NPC_SPEED.
- The UI widgets "VSync" and "Low latency (applies fully after restart)" are owned by **ux-flow**. perf owns the fields and the mapping.

## 3. One BattleFrame per egui pass (new `ui/battle/frame.rs`)
- `pub(super) struct BattleFrame { screen: BattleScreen, hud: HudView, field: BattleViewModel }` with `BattleFrame::build(ctx, f: &UiCtx) -> Option<Self>`.
  - It samples SceneCache from ctx temp data exactly as art/battlefield/mod.rs:34-41 does.
  - It calls `BattleViewModel::build` with `f.battle.interpolation.scene(None, a)` and reduced.
- `draw_screen` (ui.rs:101) builds it once when `f.screen == Screen::Battle`, then passes `Option<&BattleFrame>` explicitly to:
  - `controls`, for the Pause branch, which reads `frame.screen.paused` instead of rebuilding at ui/battle.rs:68;
  - `battle`, replacing :217-219 and keeping go(Results) on None;
  - `publish_controls` and `gather_combat_input`, replacing :105.
- New `art::battlefield(p, r, view: &BattleViewModel, a: &Attempt, aim, options)`. `a` is kept only for the defender rig.
  - `art::battlefield_for_attempt` keeps the per-panel build for the operations observer (operations.rs:286), once per visible worker.
- Counters: `#[cfg(test)] thread_local! VIEW_BUILDS: Cell<ViewBuilds {screen, field, hud}>`, incremented in BattleScreen::build, BattleViewModel::build_with and hud::view_model. Production cost is zero.
- `ui/review/workshop.rs:10` changes `f.launch.scenario.clone()` to `as_deref()`.
- This is exact because Game is immutable during a pass: dispatch only enqueues (ui/context.rs:247-253).

## 4. Battle camera: uniform zoom-to-fit (user requirement)

### fit_field
`pub fn fit_field(maximum: Rect, lanes: u8, width: f32) -> Rect` lives in `art/projection.rs` and is re-exported as `art::fit_field`. It replaces `clearing_rect` (ui/battle.rs:475-489) for both the main battle and the worker observer (ui/operations.rs:277).

```
const REFERENCE_LANES: f32 = 11.0; const REFERENCE_WIDTH_YARDS: f32 = 30.0;
let l = f32::from(lanes.max(1)); let w = width.max(1.0);
let ref_l = l.max(REFERENCE_LANES);                  // today's lane = maxH / max(lanes, 11)
let zoom = (REFERENCE_WIDTH_YARDS / w).min(ref_l / l); // uniform; fewer lanes or narrower => more zoom
let size = vec2(maximum.width() * (w / 30.0) * zoom, maximum.height() * (l / ref_l) * zoom);
// snap the limiting axis exactly to maximum, centre the other; no 160/86 minimums, no clamp
```

### What it preserves
- Today's own per-geometry lane:yard proportion is kept: lane/yard == (maxH/max(11,l))/(maxW/30).
- The full lanes and full width are always visible, and the field fills the limiting axis exactly.
- The leftover is forest from the existing peripheral_strips.
- 11/30 and 17/30 are identical to today: z = 1, rect == maximum. The progressed-peak gate scene keeps its geometry.

### Numbers
Normal window: maximum 1392x692. Small window: 729.8x310.5.

| geometry | normal | small | notes |
|---|---|---|---|
| 3/12 (z 2.5) | 1392x471.8 | 729.8x211.7 | lane 157.3 pt, yard 116 pt; today 556.8x188.7 |
| 7/16 peak (z 1.571) | 1166.4x692 | 611.5x310.5 | |
| 7/20 (z 1.5) | 1392x660.5 | 729.8x296.4 | |
| 9/24 (z 1.222) | 1361x692 | | |
| 11/30, 17/30 | maximum | maximum | identical to today |

Sprite scale: a 3-lane figure is 0.78 × 157.3 = 122.7 pt. That is a downscale at OS scale 1.0, and about 245 px against 235-267 px walk cells at OS scale 2.0 (≤ 1.04x). A walking boss is about 1.27x at OS scale 2.0.

### Construction does not rescale
- `battle_arena(canvas, construction)` becomes `field_area(canvas)` in `ui/battle/camera.rs`. It always uses the 24 pt right inset; today construction switches it to 276 pt at ui/battle.rs:494.
- The construction panel stays the existing overlay, unchanged: x = canvas.right() − 296, width 280, top r.top() + 8, height max(r.height() − 16, 180).
- The geometry moves unchanged into `construction_panel_rect(canvas, field)`.
- Invariant: `panel.left() ≥ projection.world(construction_bounds right edge = width_yards × 0.5, 0).x + 8` for every geometry at both sizes. Panel left is 1144 (normal) and 481.8 (small), against field centres at ≤ 720 and ≤ 388.9.
- Collapse, alpha and styling belong to ux-flow in wave 3.

### Top-row FX headroom
- `effects_clip = field_area(canvas)` extended upward by `FX_TOP_HEADROOM = 48.0` pt into the free centre of the 124 pt top band.
- It is written to `f.battle.effects_rect`, which ui/damage.rs:175 and ui/loot.rs:139 already read. HUD corners are drawn later and stay on top.
- Reason: with fit, height-limited fields such as 7/16 peak lose the top forest margin, and lane-0 drop fx would clip.
- The observer's effects_clip stays its allocated box.

## 5. Stress scene (core fixture plus review scene)

### Core cap ceiling
- In horde_core progression.rs: `pub const STAGED_ENEMY_CAP_MAX: usize = 400`, used only in `ProgressionSnapshot::validate` (progression.rs:238, today `> 224`).
- ProgressionCatalog bounds stay ≤ 224 (progression.rs:88, :93, :95), so play cannot reach 400.
- Persistence already follows `a.enemy_cap()` (persistence.rs:392-393).
- All saves from v1 to v7 that loaded before still load, because this only widens the bound.
- **Requires save-determinism sign-off before the v7 freeze.**

### Fallback if save-determinism rejects
1. Keep 224.
2. The stress main field holds 225 drawn (224 + boss).
3. 400-drawn render headroom is proven headlessly: an unvalidated 400-enemy attempt in a test asserts `prims(400) − prims(52) ≤ 2` and vertices within budget.
4. Simulated load reaches ≥ 400 through the workers.

### stress_render_fixture
`pub fn stress_render_fixture() -> Result<Game, String>` in `sim/fixtures.rs`, re-exported from sim.rs. It builds on `maximum_progression_render_fixture` (fixtures.rs:77) through `Game::staging()` and legal commands, and ends with `validate_game(&g, 0)`.
- Main field: 17 lanes, 30 yards, on the first (country, stage) whose `stage_theme(s).lighting == "night"` (checkpoint theme, world.rs:171-184).
- `progression.enemy_cap = 399`, giving 399 hostiles + 1 Brute boss = 400.
- `wave_tier` at its current maximum (4). It follows enemy-balance's new bound in wave 2.
- Npc loadout with `belt_fed_machine_gun` (class MachineGun).
- The base fixture's 2 turrets, trap and walls.
- 3 staged Molotov fire zones in `a.ordnance.fire_zones` at the ordnance.json maximum duration.
- 1,500 settled CorpseRecords staged by a private `stage_corpses` helper inside fixtures.rs, with consistent ids, reward_ledger and ascending ticks (persistence.rs:534-545). sim/combat.rs is **not** refactored.
- 1,200 stash items (`stage_items`) and 150 RunRecords (`stage_history`).
- 3 unsettled worker battles via `deploy_survivor` (sim/parallel.rs:66), after staging 3 roster recruits and the parallel unlock (max_parallel 4). Worker 1 is at its own cap (224 + 1); workers 2 and 3 have ≥ 60 enemies each.
- There are no warm ticks in the fixture. The review scene runs live under LASTLIGHT_REVIEW_MOTION, so events, particles and fire become active, and the director refills to the cap.
- `pub fn save_clone_fixture(items: usize) -> Result<Game, String>` = the stress fixture with the stash extended to `items`.
- Review matrix: screen `stress` beside peak and progressed-peak (ui/review/matrix.rs:53-65). Status text: "Synthetic stress fixture · 400 drawn + 3 expeditions · perf headroom, not a gameplay claim".
- The fixture is not part of any digest.

## 6. Batched battlefield (layered egui meshes)
- `art/battlefield/batch.rs`: `MeshBuilder { quad, quad_uv, fan(center, radii, segments), ring(center, radii, width, segments), bar }` with reserved capacity, and `FieldMeshes`.

### Packed battle atlas
- `visual_assets::loading` packs the existing actors-v1 (1774x887) and walkers-v1 (1122x1402) sheets plus a 4x4 opaque white block into one NEAREST managed texture, `lastlight-battle-atlas`:
  - actors at (0,0);
  - walkers at (1776,0);
  - white at (0,889);
  - size 2898x1402, about 16 MB RGBA.
- It is rebuilt whenever either sheet reloads, beside `publish_flash_masks` (loading.rs:136-150).
- `ArtTextures.battle_atlas: Option<BattleAtlas { texture, actors_origin, walkers_origin, white_uv, size }>`.
- If the packed size exceeds the device's max texture side (loading.rs:285-291), fall back to two actor meshes (walkers, then actors) and log once.
- This is repacked existing pixels, not a new raster, so decision 9 and provenance are unchanged.

### Pure layout and shadows
- `actor_quad(textures, kind, center, size, tint, pose) -> Option<ActorQuad { rect, uv, pivot }>` is extracted from `actor_animated` (visual_assets.rs:311-369). Both the batched and legacy paths call it, so the geometry is identical.
- Hit-flash runs (visual_assets/flash.rs, untextured mask runs) are emitted into the actors mesh with `white_uv`. Per-enemy feet order is exact, flash included.
- Procedural soft shadow (graft from A):
  - `visual_assets::soft_shadow_image() -> ColorImage`, 128x64 premultiplied black;
  - its alpha reproduces the three rings at spreads 1.35/1.12/0.9 with alphas 10/15/23 (art.rs:582-590): centre 48/255, quad covering 1.35x the size;
  - loaded once via `ctx.load_texture("lastlight-soft-shadow", .., TextureOptions::LINEAR)` into `ArtTextures.shadow: Option<TextureId>`;
  - `shadow_batch(..) -> bool` emits one quad per enemy plus the defender rig;
  - the fallback is `art::soft_shadow`.

### Layer order per battlefield
1. Settled corpse mesh (atlas, cached; §7).
2. Collapsing corpse mesh (atlas).
3. Underlay, untextured: contact lane bands, windup ground rings.
4. Shadow mesh (soft-shadow texture).
5. Actors mesh (atlas), per enemy in BVM feet-then-id order (battle_view.rs:355-356): sprite quad, hit-flash runs, elite tint.
6. Overlay, untextured: health and windup bars, stun and outline rings, elite rim (enemy_visuals v2).
7. Text: boss label, CRIT.
8. One particle mesh.

The fallback `figure()` vector path stays per enemy and is used only when `enemy_art_rejected`.

### Particles and FxView
- `fx::ambient` and `fx::particles` (fx.rs:19-117) emit one untextured mesh: lines as 4-vertex quads, dots as 8-segment fans, no feathering, up to MAX_PARTICLES 128.
- `fx_lifetime(kind) -> f32`, a single table in art/battlefield/fx.rs used by the BVM filter and by the painters:
  - BossWarning 2.0 s, which covers ux-flow's ≥ 1.5 s banner;
  - Explosion 0.5 s;
  - others 0.45-1.0 s.
- `BattleViewModel.fx` keeps only events with `age ≤ fx_lifetime(kind)` (battle_view.rs:394-407).
- Particles move into the BVM by value instead of being cloned (battle_view.rs:408).

## 7. Corpses (presentation only; saved CorpseRecords untouched)
- `SceneAnimation::sample` builds a DeathPose only for collapsing bodies: age < 18 ticks, and none in reduced mode.
  - Corpses are append-only with ascending ticks, so settled bodies are a prefix.
  - Cap: `CORPSES_COLLAPSING_DRAWN = 64`, the newest.
- `BattleViewModel.settled: SettledCorpses { first_id, count }`. Only collapsing CorpseViews are materialised.
- `art/battlefield/corpses.rs` owns an `Arc<Mesh>` cache in egui temp data.
  - Key: `CorpseCacheKey { layer: LayerId, first_id, rect_bits: [u32; 4], texture: TextureId, blood: bool }`.
  - When `count` grows, only the new bodies are appended, through the pure `visual_assets::settled_corpse_quad`.
  - It rebuilds on any key change, or when count shrinks.
  - It is painted as `Shape::Mesh(Arc)`.
- Safety cap: `CORPSES_SETTLED_DRAWN = 2048`, the newest. That is more than 7x the measured maximum per stage.
- This is the single documented cross-frame cache in the battlefield. It is module-owned, keyed and tested, not hidden.

## 8. docs/PERF_BUDGET.json and `perf_budget.rs`
`perf_budget.rs` is un-gated and declared in main.rs. It holds `pub const`s and a `CAPS: &[(&str, usize)]` table built from:
- `animation::MAX_PARTICLES` (128);
- `animation::EVENT_BUDGET` (64, made pub(crate));
- `animation::MAX_SCENES` (8);
- `ui::damage::LIMIT` (24) and `ui::loot::LIMIT` (12), made pub(crate);
- `CORPSES_SETTLED_DRAWN`, `CORPSES_COLLAPSING_DRAWN`, `BATTLEFIELD_PRIMITIVES_MAX`, `ENEMIES_CEILING` and `ENEMIES_STRESS`.

The JSON:
```
{"version":1,
 "hardware":{"gpu":"NVIDIA RTX 1000 Ada Laptop","cpu":"Intel Core Ultra 9 185H","backend":"Vulkan","integrated_gpu":{"status":"pending","adapter":"Intel(R) Arc(TM) Graphics"}},
 "sizes":{"normal":{"size":"1440x900","scale":1.0},"small":{"size":"1050x700","scale":1.35}},
 "definitions":{"interface_draw_ms":"ui::draw wall time (v2 WorkPhase::Interface)","interface_total_ms":"egui pass loop + tessellation","cpu_frame_ms":"main First..Last + (extract + Render schedule - present wait) of latest completed render sample; per-sample sum; VsyncOff runs only"},
 "profiles":{"dev":{"dir":"debug","features":["review"]},"playtest":{"dir":"playtest","features":["review"]},"shipped":{"features":[],"measured":"--smoke wall time only"}},
 "run":{"warmup_frames":180,"frames":{"peak":600,"progressed-peak":600,"stress":1800},"repeat_acceptance":3},
 "baseline":{"wave0":{"commit":"7bd5de8","profile":"dev","vsync":true,"frame_report":2,"interface_mean_ms":{"peak":{"normal":1.1904,"small":1.0752},"progressed-peak":{"normal":1.6128,"small":1.4719}}},"v3":null},
 "scenes":{
  "peak":{"frames_over_50ms_max":0,"interface_draw_mean_ms_max":{"normal":1.1904,"small":1.0752}},
  "progressed-peak":{"frames_over_50ms_max":0,"interface_draw_mean_ms_max":{"normal":1.129,"small":1.030},"interface_total_not_above_baseline_v3":true,"cpu_frame_p99_ms_max":10.0},
  "stress":{"frames_over_50ms_max":0,"cpu_frame_p99_ms_max":10.0}},
 "caps":{"enemies_ceiling":225,"enemies_stress":400,"corpses_settled_drawn":2048,"corpses_collapsing_drawn":64,"particles":128,"particle_events":64,"scenes":8,"damage_popups":24,"loot_flyups":12,"fx_lifetime_max_seconds":2.0},
 "egui":{"battlefield_primitives_max":48,"primitive_growth_52_to_400_max":2,"battlefield_vertices_max":{"progressed-peak":40000,"stress":72000}},
 "save_clone":{"items_min":1000,"workers":3,"p99_ms_max":1.0,"profiles":["dev","playtest"]},
 "decisions":{"pipelined_rendering":null}}
```
- Rust tests read it with `include_str!("../../../docs/PERF_BUDGET.json")`. The key sets must be equal in both directions, and the values equal.
- The egui caps are hard ceilings. They are tightened to measured × 1.25 at W1.14.
- vfx-lighting appends decals 64, lights 24, explosion_frames 8 and flames_per_zone 21 (9 reduced) in wave 5. The bidirectional test forces the matching Rust constants.

## 9. scripts: `scripts/lib/Native.ps1` and the `measure_performance.ps1` rewrite (PS 5.1)

### Native.ps1 (dot-sourced)
- `Resolve-TargetDir $workspace`:
  - CARGO_TARGET_DIR empty: `Join-Path $workspace target`;
  - otherwise, if not `[IO.Path]::IsPathRooted`, join it to the workspace;
  - then the one-argument `[IO.Path]::GetFullPath`.
- `Invoke-Native -File -Arguments -LogStem -TimeoutSeconds [-Gui]`:
  - `Start-Process -PassThru` with `-RedirectStandardOutput`/`-RedirectStandardError` to `<stem>.out.txt`/`<stem>.err.txt` (amended by R24, plan §2.4: `[Diagnostics.Process]::Start` with a `ProcessStartInfo`, UseShellExecute off and both streams redirected to those logs, is the accepted form; see S14);
  - `-NoNewWindow` for cargo, `-WindowStyle Normal` for the game (never both);
  - a splat hashtable that omits `-ArgumentList` when it is empty;
  - `$null = $p.Handle` right after start;
  - `WaitForExit(ms)`, then `Stop-Process` and throw on timeout;
  - returns ExitCode.
- `Write-Utf8` wraps `Out-File -Encoding utf8`, and JSON is written with `ConvertTo-Json -Depth 8`.

### measure_performance.ps1 parameters
- `-Scene peak|progressed-peak|stress|all` (default all).
- `-Profile dev|playtest`: dev runs `build --locked -p horde-defense` into debug; playtest adds `--profile playtest`, with default features.
- `-VsyncOff` sets LASTLIGHT_PERF_PRESENT_MODE=autonovsync.
- `-Pipelined default|on|off` sets LASTLIGHT_PERF_PIPELINED.
- `-Frames` (0 means the budget per-scene default), `-Warmup` (180), `-Repeat` (1; 3 for acceptance).
- `-Adapter <name>` sets WGPU_ADAPTER_NAME for an informational run.
- `-OutputDirectory` (default target/perf/<yyyyMMdd-HHmmss>-<profile>[-novsync]).
- `-NoBuild`, `-NoBudget`, `-SaveClone` (runs the core example in the same profile), `-SelfTest`.
- `-ShippedSmoke`:
  1. builds `--profile playtest --no-default-features` first;
  2. copies the exe to `<out>/shipped/`;
  3. times `--smoke` three times (median wall seconds);
  4. only then builds the review variant.

### Per run
- Env: LASTLIGHT_PERF_ACTIVE_ONLY=1, LASTLIGHT_PERF_NPC_SPEED=1, LASTLIGHT_PERF_PROFILE=<Profile>, and a fresh LASTLIGHT_SAVE_DIR.
- Outputs: `<scene>-normal[-rN].json` and `<scene>-small[-rN].json`, plus summary.json and summary.md.
- A run is invalid unless all of these hold: version == 3, completed_sample_target, !paused, unfocused_sample_frames == 0, and the requested present mode and pipelined state are echoed back. An invalid run is rerun once automatically; if it is still invalid, exit 2.

### Budget compare
- Repeats are compared by median; frames_over_50ms by worst repeat.
- frames_over_50ms is checked always.
- cpu_frame p99 only with -VsyncOff.
- interface_draw and interface_total only when the profile and vsync match the baseline (dev, VSync on).

Exit codes: 0 pass, 1 over budget, 2 invalid evidence, 3 build or run failure. `-SelfTest` runs `scripts/fixtures/perf/{within-budget,over-budget,unfocused}.json` and must give 0, 1 and 2.

Only measure_performance adopts Native.ps1 in this remit. capture_review, capture_motion, trace_npc and package_playtest adopt it under qa-captures and the release wave.

## 10. Save clone
- `Game::save_snapshot(&self) -> Game` clones without the serde-skipped `event_outbox` and `settlement_issues` (sim.rs:735-743). It is used at runner/apply.rs:290-295, and its serialized output equals the game's.
- New example `crates/horde_core/examples/save_clone.rs`:
  - `pub fn measure(fixture: &Game, iterations) -> SaveCloneReport` over `save_clone_fixture(1000 | 2500 | 5000)`;
  - 50 warmup + 1,000 timed iterations;
  - reports mean, p50, p95, p99 and max, plus a per-component breakdown (campaign.items, run_history, attempt, battles), serde_json::to_vec time and bytes, and debug_assertions;
  - writes JSON to `--out`.
- Gate: p99 ≤ 1.0 ms at 1,000 items + 3 workers in **dev and playtest**. Release is informational.
- In-situ cross-check: `work_max_ms[1]` in the stress report, across ≥ 2 autosaves (600 ticks, runtime.rs:226-229).
- Remedy, only if the gate fails, aimed at the largest measured component:
  - items: `ItemDefId(Arc<str>)` with `#[serde(from="String", into="String")]` (content.rs:79), keeping Ord and Hash semantics;
  - run_history: `Arc<[RunRecord]>` copy-on-write, with serde `rc`.
- The JSON stays byte-identical in both remedies, guarded by save_v6.json and save_v7.json byte-identical round-trip tests (fixtures owned by save-determinism).

## 11. File splits (pure moves, one commit each, facades keep crate paths)
- **battle_view.rs** → battle_view/:
  - interpolation (:1-152);
  - field (:153-417, :735-826);
  - readouts (:418-575 minus arena_geometry);
  - screen (:576-703 + arena_geometry);
  - placement (:704-734);
  - tests (:827-1282).
- **ui/battle.rs** → ui/battle/:
  - input (:23-213);
  - camera (clearing_rect, battle_arena, attack_selector_rect, projection);
  - field (:214-472);
  - overlays (:499-678);
  - attack (:679-877);
  - details (:878-943);
  - construction (:944-1050);
  - tests (:1051-1200).
  - frame.rs is added later in the BattleFrame commit.
- **art.rs** → art/palette (:13-19), projection (:20-86), backdrop (:87-334), figures (:335-625), scenes (:626-865), tests (:866-1191).
- **ui/progression/upgrade_graph.rs** → upgrade_graph/model, wires, viewport, pictogram, navigation, tests. **Graph::show stays whole**; any method extraction is a later separate commit by progression-tree.
- Update the 9 docs/components `source:` lines:
  - battle.md → ui/battle/field.rs::battle;
  - battle-management.md → field.rs::battle; details.rs::field_details;
  - battle-npc-decision.md → field.rs::battle; inspector.rs::npc_decision;
  - battle-hud.md → overlays.rs::corner_hud; attack.rs::personal_controls;
  - battle-build.md → construction.rs::construction_controls, placement_cursor;
  - saved-layouts.md → construction.rs::construction_controls;
  - battle-ordnance-targeting.md → attack.rs::ordnance_cursor; input.rs::gather_combat_input;
  - battle-weapon-selector.md → attack.rs::attack_selector;
  - debrief-combat-log.md → details.rs::field_details.

## 12. ARCHITECTURE.md
- One table: `Module (file) | Owns | Does not own | Entry points / key types | Guard tests`.
- One row per module and submodule, including the 16 missing today: capture_sequence, defender_presentation, diagnostics, gear_appearance, logging, player_model, settings, balance, catalog, enemy_catalog, hash, quality, skill_catalog, stats, survivors, weapons.
- New rows: render_plan, perf_budget, battle_view/*, ui/battle/*, art/*, art/battlefield/batch and corpses, upgrade_graph/*, scripts/measure_performance.ps1 and lib/Native.ps1, sim/fixtures stress_render_fixture, examples/save_clone.
- Short sections follow: Mutation path, Scheduling (First/Update/PostUpdate egui sets/Last/Extract/Render with the probe marks), Persistence, Performance (link to PERF_BUDGET.json).
- Stale facts fixed: :25 envelope v7, :139 corpse draw policy (presentation caps, saved records uncapped), :168 zoom-to-fit, :172 frame latency 1 and the present plan, :188 FrameReport v3. VALIDATION.md:332 is updated to match.
- Gate: `check_architecture(workspace)` in scripts/check_views.py. Every .rs module file or directory at depth ≤ 2 under crates/*/src (excluding lib, main, tests) must appear backticked in a table row.

## 13. Decisions
| question | chosen | rejected (reason) |
|---|---|---|
| Camera | B: uniform zoom of today's per-geometry rect, `z=min(30/w, max(11,l)/l)` | A's constant 11-lane proportion (17/30 shrinks to 900.7x692 and changes the gate scene); anisotropic fill (not uniform); vfx clamp [0.75,1.6] (does not fill: 80.5% at 3/12) |
| Observer | the same fit_field on its box | a separate clearing (two camera models); observer clamps (no requirement) |
| Construction | the existing overlay unchanged + build-half invariant (A) | B's collapse toggle and alpha (ux-flow wave 3); reserved column (wastes ~20%); bottom dock (conflicts with the HUD) |
| Top-row FX | FX_TOP_HEADROOM 48 pt (B) | headroom inside the fit (shrinks 11/30 and 17/30 by 3-4%); accept clipping (peak 7/16 lane-0 drops clip) |
| Stress | 400 drawn via STAGED_ENEMY_CAP_MAX (B, 2 of 3 judges) + 3 workers (A) + fixtures-local corpse staging (A) | unvalidated fixture (autosave errors); synthetic views (no sim cost); refactoring combat.rs record_corpse (touches the traced sim path). A's 225-drawn path is kept as the fallback |
| Normal play records nothing | PerfProbe resource absent (B) + recorded mutation check (A) | static atomics + lock (flaky across test threads) |
| Render-world | extract wrapper + Render schedule − present wait (B) | ExtractCommands..PostCleanup only (misses extract); gap timing (counts the channel wait) |
| Interface gate | v2 draw ≤1.129/≤1.030 against wave 0 + interface_total ≤ baseline_v3 (A anti-shift guard) | B's extra −30% on the total (beyond the brief); A's switch to 0.70×v3 on drift (moves the goalpost) |
| Once per pass | BattleFrame {screen, hud, field} (B) | temp-data cache (hidden); a Bevy-system resource (ordering, SceneCache in ctx) |
| Enemy batching | packed atlas, one actors mesh, exact feet order incl. flashes (B) | per-texture runs (flash reordering; primitives grow with mixed-sheet wave-2 art); Mesh2d (projection/clip/observer rewrite) |
| Shadows | procedural 128x64 LINEAR texture quad (A) | B's 12-segment fan (changes the look, builds vertices inside the gated draw); ellipses (cost) |
| Corpses | collapsing-only sampling + cached settled Arc<Mesh>, cap 2048 (B) | A's newest-1024 cap (still O(n) per pass, hides 476 bodies in stress); render-to-texture (rebakes per zoom) |
| Budget source | Rust consts + JSON with a bidirectional test (B) | JSON-only (drift); script-only (untestable) |
| Script tests | text hygiene test (A) + -SelfTest exit codes (B) + one auto-rerun (A) | Python driver (brief says PowerShell) |
| Profile label | LASTLIGHT_PERF_PROFILE from the script (A) | build.rs commit stamp (release-wave scope) |
| Display module | render_plan.rs with LASTLIGHT_PERF_* names, Settings::load before App::new | partial preferences parser (duplicate loader); App restart (unsupported) |
| Pipelined default | B protocol (median of 3, both profiles) + A's VSync-on FPS check | no measurement |
| Save clone | dev+playtest gate (A), breakdown-chosen remedy and save_snapshot (B) | release-only gate; Arc COW up front; diff snapshots |
| Splits | strict pure moves, Graph::show whole (A) | method extraction inside a move commit |
| ARCHITECTURE gate | check_views check_architecture depth ≤2 (B) | main.rs `mod` test only (misses core) |

## 14. Steps
Wave 1 is ordered so that every done_when can be achieved at its own step. Steps are numbered W1.1 to W1.16.

1. **W1.1** PERF_BUDGET.json + perf_budget.rs.
2. **W1.2** Frame report v3 + PerfProbe + GPU span.
3. **W1.3** render_plan and settings.
4. **W1.4** Stress fixture + cap ceiling + review scene.
5. **W1.5** Native.ps1 + script rewrite.
6. **W1.6** v3 re-baseline on the unchanged renderer.
7. **W1.7** Pure-move splits + sidecars.
8. **W1.8** BattleFrame.
9. **W1.9** Camera fit + construction + observer + FX headroom.
10. **W1.10** Batched battlefield + shadow + particles + FxView.
11. **W1.11** Corpses.
12. **W1.12** Save clone + save_snapshot (+ conditional remedy).
13. **W1.13** Tighten the egui caps.
14. **W1.14** Acceptance + pipelined decision + shipped smoke.
15. **W1.15** ARCHITECTURE.md + check_architecture.
16. **W1.16** Announce the new paths to ux-flow, vfx-lighting, progression-tree and qa-captures.

Later waves: wave 2 makes the stress fixture follow archetypes, elites and boss phases, then re-gates; wave 5 binds the fx caps and fx-stress; wave 7 runs the full matrix at release candidate.

## 15. Risks
- **Cost shifting.** Batching builds vertices in draw. The anti-shift guard plus interface_breakdown catch it. If a gate is missed, the next reductions in order are:
  1. cache world_backdrop dressing per (canvas, arena, theme);
  2. precompute walk_frame_layout max widths;
  3. avoid the EnemyVisual clone per corpse;
  4. replace the BTreeMap lookups in SceneAnimation.
  Stop and ask before any Mesh2d migration.
- **Cap widening.** It needs save-determinism sign-off. The fallback is defined in §5.
- **GPU span.** It depends on doc-hidden begin/end APIs. It is isolated in gpu_span.rs and degrades to "unsupported".
- **Atlas.** It adds about 16 MB of VRAM and must rebuild on every sheet reload. Devices whose max side is too small use the two-mesh fallback.
- **Visual changes for qa-captures to review**: shadows under all bodies, bars above all sprites, particles without feathering, zoom on ≤11-lane fields, and the FX headroom. Review at both sizes, normal and reduced, including max-lanes 17/30 and a 3/12 opening.
- **Zoom-to-fit and wave-3 UI.** On height-limited fields the field touches the top band, so ux-flow must re-place the pause banner, field hint and attack selector in wave 3.
- **Construction overlay.** It covers the spawn side of a full-width field while combat continues. ux-flow may restyle it, but the no-rescale test must keep passing.
- **PS 5.1 pitfalls**: empty ArgumentList, null ExitCode without `.Handle`, `-NoNewWindow` versus `-WindowStyle`, the BOM from Out-File utf8 (Python readers use utf-8-sig), and focus loss (exit 2, rerun).
- **One CARGO_TARGET_DIR.** The shipped and review playtest builds overwrite one exe, so the script copies the shipped exe out first. The orchestrator must serialise perf runs behind the cargo builds.
- **Environment noise**: background throttling and isolated 0.5 s frames (VALIDATION.md:312-321). Use mains power, the High Performance power plan, a focused window and Repeat 3.
- **Settings field names** must be agreed with ux-flow before the wave-1 schema freeze: vsync, low_latency, window_mode (ux-flow).

## 16. Numbers to measure and record
- v3 re-baseline per scene, size and profile, VSync on and off: interface_draw, interface_total, egui_pass, tessellate, main, extract, render_schedule, present_wait, cpu_frame p99, egui primitives, vertices and textured_quads, GPU egui ms.
- After the changes: the same set, and the Interface gate result at both sizes.
- Pipelined A/B table.
- Save clone p99 in dev, playtest and release at 1,000, 2,500 and 5,000 items, with the per-component breakdown.
- Stress Persistence max.
- Shipped smoke median seconds.
- Battlefield primitives at 52, 225 and 400 enemies.
- settled_corpses_drawn in stress.
- Integrated GPU: pending.

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U6. Camera

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Uniform zoom-to-fit that keeps the lane:yard proportion; fewer lanes means more zoom; construction never rescales the arena. Detail in R1 and the camera-layout cluster.
- **Supersedes in this spec:** Own fit_field formula and placement.

### Orchestrator rulings

#### R1. Camera formula, owner and construction overlay

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Base = today's per-geometry clearing_rect for (lane_count, width_yards) including its clamps; uniform zoom z = min(arena.w/base.w, arena.h/base.h) (&gt;= 1), centred; no texel or magnification clamp; 17x30 and every other already-filling field stays pixel-identical; the worker observer window uses the same fit function. Owner vfx-lighting, implemented in WAVE 1 because later captures depend on it. Construction is an overlay panel drawn over the field (no arena rescale), with an INCOMING occlusion chevron when the panel covers spawns; Save/Clear plan stay in the panel.
- **Supersedes in this spec:** z = min(30/w, max(l,11)/l) without the 160/86 clamps; W1.9 ownership of art::fit_field; construction_panel_rect anchored to the field.

#### R8. Single owners

- **Owner / wave:** progression-tree, wave 1-4.
- **Ruling:** Pacing harness v2 = progression-tree (wave 2; CD and EB add profiles on its API); per-archetype telemetry = enemy-balance; SHA-256 / provenance / orphan helpers and scripts/lib/Native.ps1 = qa-captures (wave 1; audio, vfx and perf reuse them); measure_performance.ps1 = perf-architecture on top of Native.ps1; storm tagging = vfx presentation table only (no world.json change); regional checkpoints are derived (no saved field), presentation lines by CD plus EB's Regional boss rank (HP and reward x1.2) in wave-4 balance, no theme-rule change; save_v7.json is cut at the wave-1 close and frozen, later cases use inline JSON or new fixtures; stress scene raises the staged enemy cap only behind a review/test-only seam (no production save accepts &gt; cap+1), owner perf.
- **Supersedes in this spec:** Own Native.ps1 helpers; STAGED_ENEMY_CAP_MAX = 400 in production validation.

### Cluster: camera-layout

#### C-1. Camera owner, wave and file placement

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** vfx-lighting implements camera and battle layout in wave 1 in four commits: C1 crates/horde_game/src/art/camera.rs (pure Rect math, re-exported as art::{clearing_size, fit_field, FieldFit}); C2 ui/battle/layout.rs BattleLayout plus main-battle wiring; C3 observer window; C4 INCOMING chevron. perf's ui/battle split (W1-39) lands before C2. Presentation-only: no saved field, no digest move, no Command change. perf measures the commit before C2 and C2. qa verifies by captures and does not write camera tests. C2 deletes clearing_rect and battle_arena (grep guard).
- **Supersedes in this spec:** W1.9 art::fit_field in art/projection.rs, ui/battle/camera.rs field_area, construction_panel_rect.

#### C-2. Camera formula

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** art/camera.rs: REFERENCE_LANES 11.0, REFERENCE_WIDTH_YARDS 30.0, MIN_CLEARING vec2(160, 86); FieldFit{rect, zoom}; clearing_size(reference, lanes, width) is today's clearing_rect size moved verbatim from ui/battle.rs:475-489; fit_field(reference, area, lanes, width): zx = area.w/base.x, zy = area.h/base.y, the smaller wins and the limiting axis snaps exactly to the area; centred. No texel clamp. Every field with &gt;= 11 lanes is bit-identical to today. Pinned zooms: arena 1392x692: 3x12 1392x471.8 z2.5, 5x14 1392x674.0 z2.143, 7x16 1166.6x692 z1.571, 9x18 1020.8x692 z1.222; arena 729.8x310.5: 3x12 729.8x215.0, 5x14 729.8x302.5, 7x16 611.6x310.5, 9x18 535.2x310.5. EXPANSION.md:99 and PIXEL_PRESENTATION.md:77 get a dated supersession note. Tests: fresh_three_lane_battle_fills_the_arena_width_at_small_size (fail-first), fit_field_zoom_table_at_both_target_sizes, arena_fits_full_lanes_and_width_on_the_limiting_axis, fit_field_keeps_every_filling_geometry_bit_identical (pin with two mutation proofs).
- **Supersedes in this spec:** z = min(30/w, max(l,11)/l) with no clamps (729.8x211.7).

#### C-3. Single BattleLayout API

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** One pure struct in ui/battle/layout.rs built once per battle frame: BattleLayout{canvas, arena, field, hint_band, hint_callout, effects_clip, hud_left, hud_right, selector, receipt_slot, construction_panel, pause_banner_center, bar, legend_anchor} with BattleLayout::new(canvas, lanes, width_yards, hint_band). Constants TOP_BAND 124, ARENA_SIDE_INSET 24, ARENA_BOTTOM_INSET 84, HINT_BAND 92, HINT_CALLOUT_TOP 4, HINT_CALLOUT_HEIGHT 84, FX_TOP_HEADROOM 60, PANEL_MAX_WIDTH 280, PANEL_RIGHT_MARGIN 16, PANEL_TOP_GAP 8, PANEL_MIN_HEIGHT 180, PANEL_FILL_ALPHA 235, CHEVRON_GAP 10. arena = today's battle_arena(canvas,false); selector and receipt_slot anchor to arena.top (never move with zoom); bar and legend unchanged. f.battle.arena_rect = layout.field.rect, effects_rect = layout.effects_clip. Later owners may append fields, not redefine them.
- **Supersedes in this spec:** field_area(canvas) / construction_panel_rect(canvas, field).

#### C-5. Effects clip

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Main battle effects_clip = arena extended upward by FX_TOP_HEADROOM = 60 pt (top at canvas.top+64), x at the arena edges, bottom at arena.bottom. Used only by overlay passes (damage, loot, reticle). The battlefield sprite painter stays clipped to the arena. Observer clip = its allocated box.
- **Supersedes in this spec:** FX_TOP_HEADROOM = 48.

#### C-6. Construction overlay

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** The construction panel overlays the field; the field rect never depends on construction. width = min(280, canvas.w*0.5-32), left = canvas.right-16-width, top = arena.top+8, height = max(arena.h-16, 180): bit-identical to today at 17x30; never covers a build cell. Fill black alpha 235, scroll area; construction_controls including "Save country plan"/"Clear country plan" stay inside. Field hover is ignored inside the panel. Tests: construction_toggle_keeps_arena_rect (fail-first), construction_panel_never_covers_a_build_cell, construction_panel_keeps_plan_buttons, construction_panel_swallows_field_hover.
- **Supersedes in this spec:** construction_panel_rect anchored to the field; the 276-pt construction inset of battle_arena (deleted).

### Cluster: settings-audio-tooling

#### S1. Settings v2 struct

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Keep the 13 keys and bindings: [String; 8]. Add #[serde(default)] version: u32 (absent = 0), extra_bindings: ExtraBindings{world_clock "P", cycle_attack "Q"} (struct), window_mode: WindowModeSetting{Windowed, BorderlessFullscreen}, vsync true, low_latency false, music f32 ("Music & stingers"), ui f32 ("Interface sounds"), mono_audio false, quality_markers: QualityMarkers{ColorOnly, Text, Shape, TextAndShape}, palette: Palette{Standard, ColorBlind}, seen_callouts: Option&lt;BTreeSet&lt;String&gt;&gt;. Enums are append-only and safe under per-key salvage. SETTINGS_VERSION 1 from wave 1, 2 from wave 6. Defaults in settings::defaults (audio owns values; wave-1 0.6/0.7/0.15/0.7/0.7). Lands before perf's render_plan.
- **Supersedes in this spec:** W1.3 adding vsync / low_latency itself.

#### S3. Startup settings read and display apply

- **Owner / wave:** perf-architecture, wave 1.
- **Ruling:** The profile is fixed per process. main.rs reads settings once before App::new, builds RenderPlan (present_mode_for: low_latency -&gt; Mailbox, vsync -&gt; Fifo, else AutoNoVsync; pipelined = override or !low_latency && PIPELINED_DEFAULT), and passes the SettingsLoad into FrontendParts::load; load_profile no longer reads settings.json. One system render_plan::apply_display_settings (run_if resource_changed::&lt;Settings&gt;) writes Window.mode and present_mode live; pipelined never changes live; the UI shows "(applies after restart)" exactly when pipelined_for(low_latency, None) != plan.pipelined. UX owns only the widgets.
- **Supersedes in this spec:** "FrontendParts::load still loads the authoritative Settings later" (double read).

#### S14. Native.ps1: one PowerShell 5.1 helper

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One dot-sourced scripts/lib/Native.ps1 (ASCII, LF, no BOM, #requires -Version 5.1), qa names only: Resolve-HordeTarget, Get-HordeExecutable, ConvertTo-NativeArgument, Invoke-Native (Start-Process with redirected streams, or, as amended by R24 (plan §2.4, user 2026-11-07), `[Diagnostics.Process]::Start` with a `ProcessStartInfo`, UseShellExecute off and stdout and stderr redirected to the logs; never `>`; taskkill /T /F on timeout, returns ExitCode/Id/Seconds/Out/Err), Invoke-Cargo, Resolve-Tool, Reset-/Restore-LastlightEnv, Write-Utf8NoBom, Get-FileSha256. Timeouts cargo 1800 s, capture 300 s, smoke 900 s, --version 30 s, git/rustc 30 s. Tested by scripts/tests/run_script_tests.ps1.
- **Supersedes in this spec:** §9 Native.ps1 (Resolve-TargetDir, Invoke-Native returning ExitCode only, -Gui, Write-Utf8, Stop-Process).

#### S15. Script porting order and the 5.1 lint

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** qa step 1 Native.ps1 plus tests; qa step 2 ports capture_review.ps1 and capture_motion.ps1 only; perf W1-36 is the single owner of measure_performance.ps1 (full rewrite with -Profile, -VsyncOff, -Pipelined, stress, budget compare, exit codes, -SelfTest). repo_checks::powershell_scripts_are_51_safe keeps an explicit PENDING list (measure_performance removed by W1-36; package/verify by W7-02); capture_player_models, trace_npc and verify_player_models stay on a PS 7 allowlist.
- **Supersedes in this spec:** "Only measure_performance adopts Native.ps1" as a parallel helper.

### Cluster: enemies-world-pacing

#### W-11. 400-enemy stress seam

- **Owner / wave:** perf-architecture, wave 1.
- **Ruling:** GAMEPLAY_ENEMY_CAP: usize = 224 replaces the literal at progression.rs:238; production validation does not widen. horde_core feature review = []; horde_game review = ["horde_core/review"]. Behind cfg(any(test, feature = "test-support", feature = "review")): STRESS_ENEMY_CAP = 399, sim/fixtures.rs stress_render_fixture() (17 lanes, 30 yd, night stage, MG Npc, structures, 3 fire zones, 1,500 corpses, 1,200 stash items, 150 run records, 3 workers) validated at the authored cap, then Attempt::stage_review_enemy_cap(399): unsaveable by construction. Review screen "stress" rejects LASTLIGHT_PRACTICE and never autosaves. Wave 1 uses tier 4; perf updates it in W2-47.
- **Supersedes in this spec:** §5 core cap ceiling and validate_game at 400.

#### W-12. CPU p99 10 ms gate and integrated-GPU pending log

- **Owner / wave:** perf-architecture, wave 1.
- **Ruling:** docs/PERF_BUDGET.json is the single gate source: scenes.progressed-peak and stress carry cpu_frame_p99_ms_max 10.0 and frames_over_50ms_max 0, evaluated only on -VsyncOff runs (median of 3 for p99, worst repeat for &gt; 50 ms); 12.0 ms is only the pipelined-decision threshold. perf_budget.rs mirrors CPU_FRAME_P99_MS_MAX / FRAMES_OVER_50MS_MAX with a bidirectional test. hardware.integrated_gpu = {status "pending", adapter "Intel(R) Arc(TM) Graphics", report null} plus the ACCEPTANCE.md human-check row.
- **Supersedes in this spec:** Any reading of 12.0 ms as acceptance.

### Wave-1 close rulings (2026-11-07)

Orchestrator rulings R12-R23, recorded verbatim in plan §2.4. The ones that touch this remit:

#### R14. Per-commit capture reviews

- **Owner / wave:** qa-captures and perf-architecture, wave-1 close.
- **Ruling:** Per-commit capture reviews named in a done_when (e.g. W1-48 batching) are satisfied by the wave-close pair review (target/production-after/wave1 vs target/production-before, 356 captures, CF ledger rows). Note this on the W1-48 row in waves.md.
- **Supersedes in this spec:** A separate per-commit pair review for W1-48 (and any other wave-1 done_when that names one). The evidence is plan §10.3 and the CF ledger.

#### R18. What the W2-47 re-gate also owns

- **Owner / wave:** perf-architecture, wave 2 (W2-47).
- **Ruling:** W2-47 (stress fixture rebuild and re-gate) also owns: the sustained 15-20 FPS VSync-on stress episode, the 1050x700@1.35 VSync-on p99 interval of 19.1-19.7 ms, script-level battlefield primitive/vertex gates in measure_performance.ps1, and any egui_probe/present_wait span-ordering item not fixed in wave 1. Add these to the W2-47 row.
- **Supersedes in this spec:** Leaving these items without an owning commit after wave 1. W2-50 still attributes the gate-3 episode before W2-47 re-gates.

#### R19. Peak gate, the 30% rounding and the frame window

- **Owner / wave:** perf-architecture, wave-1 close.
- **Ruling:** Peak interface_draw stays a not-above-wave-0 gate (fix plan §9.1 which calls it informational). 1.129 / 1.030 are the rounded 30% values (fix the 'floor' wording in the plan and acceptance.md). The 600/180 vs 300/90 window difference is accepted as documented.
- **Supersedes in this spec:** Any "informational" reading of the peak interface_draw gate, and "floor" for 1.129 / 1.030 (0.70 x 1.6128 = 1.12896 and 0.70 x 1.4719 = 1.03033, rounded).

#### R22. egui gates only tighten

- **Owner / wave:** perf-architecture, wave-1 close (W2-51 applied early).
- **Ruling:** Gates only tighten. BATTLEFIELD_PRIMITIVES_MAX goes 48 -> 24 (re-derived min(ceiling, ceil(max x 1.25)) from the post-W1-69 measure of 19). The stress vertex gate stays 61098 (no loosening; measured 49502). progressed-peak stays at its 40000 ceiling. live_battlefield_runs_stay_within_the_egui_gates samples every 2nd tick of the first 420 (or the exact maxima ticks). Update PERF_BUDGET.json evidence.egui_measured to the post-W1-69 maxima.
- **Supersedes in this spec:** W2-51's default plan of re-deriving every gate from the 4d599a5 maxima: a re-derived gate above the live one is not applied, so the stress vertex gate keeps 61098.

#### R23. Integrated-GPU human check

- **Owner / wave:** perf-architecture, waves 1-7.
- **Ruling:** Pending human checks stay pending; fix the integrated-GPU check so it names how to select the adapter this machine actually has, or marks it not possible on this machine.
- **Supersedes in this spec:** Nothing in the protocol. The check stays pending and names `-Adapter "Intel(R) Arc(TM) Pro Graphics"`, the adapter W1-68 ran on this machine.

### Rulings owned by this remit

S3, W-11, W-12, R18, R19, R22, R23
