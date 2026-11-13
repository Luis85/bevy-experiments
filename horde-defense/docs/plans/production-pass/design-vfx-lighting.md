# vfx-lighting: merged design spec

**Base:** Design A, which won 3 of 3 judges (8.4/7.2, 8.0/6.3, 8.0/6.0).

**Majority preferences adopted from B:**
- construction overlay with an INCOMING edge cue (3/3);
- weapon class taken from the gear snapshot (2 for B, 1 tie);
- render-pass split, so the grade, lights and vignette never touch cues (3/3);
- lightning suppressed near Windup and Contact as well;
- a shared SHA-256 test module with the full FIPS vector set.

**Grafts:** dawn-reference scene, capture sidecar JSON, cue scaling with zoom, colour-blind-safe reduced ordnance rings, scorch tail, stage-theme count guard, `flyup_never_shows_sale_status` (W5-08), `shot_fx_never_mutates_attempt` (W5-03), archetype differentiation test.

All line numbers were re-read at HEAD 7bd5de8. The audit's fx.rs:415-467 citations are stale: fx.rs is 371 lines. Shots are at :228-280, explosions at :139-177, hits and kills at :293-345.

## 0. Invariants this remit honours

- **No vfx-owned saved field.** No new Save, Attempt, Settings or content field. No world.json, world.rs or generate_world.py edit. No GOLDEN or GAMEPLAY_TRACE move.
- **Fields read but owned elsewhere:** `ResolvedEnemy.elite` (enemy-balance, wave 2) and a v6→v7 appearance back-fill (save-determinism, wave 1).
- **Determinism:** all presentation hashing is a presentation-local splitmix or fnv over (`theme.variation`, tick, event seq, corpse id). It never reads `a.rng`, the wall clock, or `Game::apply`.
- **Reduced effects** covers every new effect (section 9).
- **Construction** never rescales the arena. The arena signature has no construction input.
- **No screen shake.** No new egui widget IO.

## 1. Camera: uniform zoom-to-fit (user requirement), wave 3

### Today

`clearing_rect` (ui/battle.rs:475-489) gives lane = safe.h/11 and yard = safe.w/30 for every geometry up to 11x30. So today's lane:yard proportion is viewport-relative:

`k(ref) = (ref.h/11)/(ref.w/30)`

- 1440x900 (safe area 1392x692): k = 1.356.
- 1050x700@1.35 (safe area 729.8x310.5): k = 1.160.

### New module `crates/horde_game/src/art/camera.rs`

```rust
pub const LEGACY_LANES: f32 = 11.0;
pub const LEGACY_WIDTH_YARDS: f32 = 30.0;
pub const ASPECT_RANGE: (f32, f32) = (1.0, 1.6);
pub const MAX_MAGNIFICATION: f32 = 1.10;
#[derive(Clone, Copy)] pub struct FieldFitClamps { pub pixels_per_point: f32, pub crisp_lane_px: f32 }
#[derive(Clone, Copy, Debug, PartialEq)] pub struct FieldFit { pub rect: Rect, pub lane_to_yard: f32, pub legacy_lane: f32, pub texel_clamped: bool }
pub fn legacy_aspect(reference: Rect) -> f32; // ((ref.h/11)/(ref.w/30)).clamp(1.0, 1.6)
pub fn fit_field(reference: Rect, area: Rect, lanes: u8, width_yards: f32, clamps: FieldFitClamps) -> FieldFit;
```

### Algorithm

1. `k = legacy_aspect(reference)`.
2. `s = min(area.w/width, area.h/(lanes*k), clamps.crisp_lane_px*MAX_MAGNIFICATION/(ppp*k))`.
3. `lane = k*s`. The rect is (width*s, lanes*lane), centred in `area`.
4. `texel_clamped` is true when the third term wins.
5. `legacy_lane = reference.h/11`.

`ArenaProjection` (art.rs:24-77) is unchanged; only its rect changes.

### Properties

- The fit is uniform and keeps today's proportion.
- It fills the limiting axis, and fewer lanes or yards always zoom in.
- 11x30 and 11x20 (lanes-limited) are pixel-identical to today at both target sizes.

### Zoom table (lane height in pt)

| Geometry | 1440x900 | 1050x700@1.35 |
|---|---|---|
| 3x12 | 157.3 (field 1392x472) | 70.6 |
| 5x14 | 134.8 | 60.5 |
| 7x16 | 98.9 | 44.4 |
| 9x18 | 76.9 | 34.5 |
| 11x20 | 62.9 | 28.2 |
| 13x22 | 53.2 | 23.9 |
| 15x24 | 46.1 | 20.7 |
| 17x26 or 17x30 | 40.7 (field 901x692) | 18.3 (field 472x310.5) |

Today's 3x12 lane is 62.9 / 28.7 pt.

### Texel clamp

- **Why it is needed:** sheets load with `TextureOptions::NEAREST` (visual_assets/loading.rs:294) and are true high-res (median colour run 1 px).
- **`visual_assets::crisp_lane_px()`** = the minimum over enemy_visuals sprites of `walk.height / sprite.height`. Bosses use standing height / boss_height, and the survivor uses 806/0.87. Archetype scale is ignored.
  - Measured today: 296.7 px per lane (Brute 267/0.90).
- **Resulting lane cap:** 163.2 pt at ppp 2.0 and 120.9 pt at ppp 2.7.
  - The target laptop captures at 2880x1800 and 2100x1400, so ppp is 2.0 and 2.7.
- **Effect:** both target sizes fill exactly (3x12 at 1440x900 is a 1.06x Brute magnification). The clamp engages only for custom 3x8 at ppp 2.0 and for large HiDPI windows.

### Battle layout (ui/battle.rs)

`battle_arena(canvas, construction)` (:490-498) becomes `battle_safe_area(canvas) -> Rect`. `clearing_rect` (:473-489) is deleted.

New function:

```rust
pub(super) const HINT_BAND_PT: f32 = 92.0; // holds the existing 84-pt callout (ui/battle.rs:641-644) + 4 pt margins
pub(super) struct BattleLayout { pub safe: Rect, pub field: FieldFit, pub hint: Option<Rect>, pub effects_clip: Rect, pub construction_panel: Rect, pub banner_y: f32 }
pub(super) fn battle_layout(canvas: Rect, lanes: u8, width: f32, hint_band: bool, clamps: FieldFitClamps) -> BattleLayout;
```

Layout rules:

- **Hint band.**
  - `hint_band = first_deployment && personal`. It is constant for the whole attempt and does not depend on `show_guidance`, so "Hide hints" never moves the field.
  - Reference = the full safe area, and area = the safe area minus the top 92 pt.
  - At 3x12 the zoom cost is 0 at both sizes, because the field is width-limited.
- **Receipt and attack selector** (:307-310, :679-684) anchor to `safe.top` instead of `r.top`.
- **Pause banner** (:662-678) goes in the bottom letterbox when it is at least 36 pt. Otherwise it sits at `field.bottom-18` (today's position).
- **Effects clip** = canvas y in [top+12, bottom-76], so top-lane fly-ups are no longer cut.
- **Construction panel** (:330-347) is an overlay anchored to the safe area:
  - x = canvas.right-296, top = safe.top+8, height = safe.h-16, width = min(280, canvas.w/2-32);
  - fill alpha 215 (was 235).
- **Why the panel never covers a build cell:**
  - `construction_bounds[1] = w/2` (sim.rs:482-487), and the field is centred, so the last build column sits at the canvas centre: 388.9 pt at 1050x700 and 720 pt at 1440x900.
  - The panel's left edge is at 481.8 / 1144 pt, so it is at least 93 pt clear for every geometry.
- **INCOMING edge cue (graft B):**
  - `occluded_spawn_cue(layout, view) -> Vec<(lane, Pos2)>` places an AMBER chevron with INK backing at `panel.left-10`, for every lane whose spawn-side enemies or Incoming events lie at x ≥ panel.left.
  - It is drawn in the cue pass via `RenderOptions.occluder: Option<Rect>`.

### Forest dressing

- `art::world_backdrop` (art.rs:110-160) gains `unit: f32`, set to `min(field_lane, legacy_lane)`. Props never outgrow today's size, and 11x30 is identical.
- Ground tiling stays screen-space (ground_material.rs:20-22, unchanged).

### Observer window (ui/operations.rs:276-298)

- It uses the same `fit_field`, with its own allocated rect as both reference and area. That keeps today's look at 11x30 while zooming like the main battle.
- Effects clip = the allocated rect.
- `theme: a.level` (:285, :297) is replaced by the shared `battle_view::scene_look(a)`, extracted from BattleScreen::build (battle_view.rs:606-620).
- It renders through the same `art::battlefield`, so grade, lightning, vignette order and cues are identical.

### Docs and old tests

- docs/EXPANSION.md:99 and docs/PIXEL_PRESENTATION.md:77 get a dated 2026-11-04 supersession note: the user now requires uniform zoom-to-fit.
- Old tests to replace:
  - ui/battle.rs:1088-1104 (deleted);
  - ui/battle.rs:1105-1146 (ported to `fit_field`);
  - ui/battle.rs:1179-1199 (replaced);
  - ui/tests/battle_tests.rs:668-689 (`width < 400` becomes "zoomed to fill").

## 2. Render-pass contract (majority B), wave 5

`art::battlefield` (art/battlefield/mod.rs:32-87) builds `BattleViewModel` once and paints five passes.

1. **World pass:**
   - `terrain::paint`, `fx::ambient`;
   - `ordnance::fire_zones` (4-frame flames);
   - `decals::blood`, newest 64 corpses, blood on only (new; before corpses);
   - `structures::paint`, `enemies::corpses`;
   - `enemies::bodies` (sprite, archetype tint, elite tint pulse);
   - defender;
   - `ordnance::thrown`;
   - `fx::world_events`: explosion keyframes, scorch tail, muzzle flashes, tracers, pellet streaks, hit sparks, kill bursts;
   - `fx::particles` (weather).
2. **Grade:** `lighting::paint_grade(p_canvas, canvas, grade, sky)` paints one over-rect across the whole canvas, forest backdrop included. It uses a canvas-clipped painter. `sky` is the lightning pulse, added as a second rect only when > 0.
3. **Lights:** `light::paint_pools`, one additive mesh.
4. **Vignette:** `art::edge_shade` moves here from `art::world_atmosphere` (called at ui/battle.rs:264, after the battlefield). `world_atmosphere` is deleted, and its callers stop calling it.
5. **Construction grid** (`terrain::construction_grid`, when construction is on).
6. **Cue pass:** `cues::paint(p, arena, &CueBatch)`, which paints:
   - windup (enemies.rs:126-140) and contact (:104-117);
   - HP bars (:192-204) and labels (:181-215);
   - elite ground ring and marker;
   - INCOMING / BOSS / REINFORCEMENTS (fx.rs:195-226) and contact damage (fx.rs:281-293);
   - survivor hit (mod.rs:184);
   - the occluded-spawn edge cue and coin numbers.

`enemies::paint` is split into `bodies` and a `CueBatch` collector. Cue geometry is emitted as at most 3 meshes: INK halo, fill, and marker. Label text stays one galley per labelled enemy, as today. This must be built inside perf-architecture's wave-1 batched enemy path; coordinate on enemies.rs.

### Cue helpers (`crates/horde_game/src/art/cue.rs`)

- Functions: `cue_line`, `cue_ellipse`, `cue_bar`, `cue_label`, `cue_marker`.
- Each paints an INK (17,23,24) alpha-230 backing first: stroke +2 pt, bars expanded 1 pt, labels padded 2 pt at alpha 190.
- Cues scale with zoom (graft B):
  - bar height = clamp(0.05·sy, 3, 6) pt;
  - cue font = clamp(0.16·sy, 10, 14) pt.
- Contrast against INK (WCAG 1.4.11 adjacent colour): RED 6.12:1, AMBER 9.83:1, GREEN 9.0:1, TEXT 14.3:1. Today RED on arid p90 is 2.13:1 on the swatch and 2.77:1 in the capture.

## 3. Lighting grade, wave 5

New module `crates/horde_game/src/art/lighting.rs`:

```rust
pub enum Lighting { Neutral, Dawn, Sunset, Overcast, Dusk, Storm, Night }
#[derive(Clone, Copy)] pub struct Grade { pub tint: [u8; 3], pub alpha: f32, pub light_gain: f32 }
pub fn grade(l: Lighting) -> Grade;
pub fn lighting_for(theme: &StageTheme) -> Lighting; // is_storm first, then theme.lighting string, else Neutral
pub const STORM_THEME_IDS: &[&str] = &["depot"];
pub fn is_storm(theme: &StageTheme) -> bool { theme.weather.eq_ignore_ascii_case("storm") || STORM_THEME_IDS.contains(&theme.id.as_str()) }
pub fn paint_grade(p: &Painter, canvas: Rect, g: Grade, sky: f32);
```

Grade table. The grade is a full-canvas over-rect, so actors, survivor and structures are graded too; A's `actor_multiply` is dropped.

| Lighting | Tint | Alpha | light_gain |
|---|---|---|---|
| Neutral | none | 0.00 | 0.55 |
| Dawn | (255,196,150) | 0.05 | 0.55 |
| Sunset | (236,142,84) | 0.10 | 0.65 |
| Overcast | (96,106,116) | 0.12 | 0.70 |
| Dusk | (54,48,92) | 0.20 | 0.85 |
| Storm | (22,30,44) | 0.26 | 0.90 |
| Night | (8,14,34) | 0.34 | 1.00 |

Modelled night/dawn mean luminance on the ground composite (soil + swatch at alpha 192/255):
- gamma-space blend: 0.44-0.46;
- linear-space blend: about 0.35-0.50.

Both are at or under the 0.70 gate. The blend space in bevy_egui is unverified, so the analytic test must pass under both models, and the capture is the authority.

**RenderOptions** (mod.rs:20-29):
- `theme: usize` is replaced by `scene: SceneLook { theme_index: usize, lighting: Lighting, storm: bool, seed: u64 }`, where `seed = StageTheme.variation`;
- plus `canvas: Rect`, `occluder: Option<Rect>` and `paused: bool`.

### Storm tagging

- A presentation table: no content change and no theme reshuffle.
- **Measured** by replicating world.rs:174-181, where theme = (stage + 5·((fnv1a("ID:stage")>>32)%2)) % 10:
  - `depot` only: 131 of 1026 stages (12.8%) in 120 countries;
  - A's {depot, railway}: 287 of 1026 (28.0%), every one of the 242 countries.
- A's claim of 195/242 was wrong.
- Only depot (light_rain) is chosen, so storms stay rare. Railway (drizzle) keeps its Overcast grade and rain particles.
- A future content weather "storm" is honoured automatically.

## 4. Lightning (decision 16), wave 5

```rust
pub const SKY_SLOT_TICKS: u64 = 480;   // 8 s at 60 Hz (sim.rs:45)
pub const SKY_FIRE_PERCENT: u64 = 60;
pub const SKY_OFFSET_RANGE: u64 = 240;
pub const SKY_RISE_TICKS: u64 = 2;  pub const SKY_DECAY_TICKS: u64 = 14;
pub const SKY_PEAK_ALPHA: f32 = 0.14; pub const SKY_RGB: [u8; 3] = [200, 214, 235];
pub const SKY_QUIET_BOSS_TICKS: u64 = 60; pub const SKY_QUIET_CUE_TICKS: u64 = 45;
pub fn sky_flash(seed: u64, tick: u64, recent: &[FxView], paused: bool, reduced: bool, storm: bool) -> f32; // 0..=1 intensity
```

**Schedule.**
- `slot = tick/480` and `h = splitmix(seed ^ slot ^ SKY_TAG)`.
- The slot fires when `h % 100 < 60`, starting at offset `(h >> 8) % 240`.
- Each pulse rises linearly over 2 ticks and decays linearly over 14.
- Starts are therefore at least 240 ticks (4 s) apart by construction. The mean rate is about 1 per 13 s.

**Returns 0 when:**
- the stage is not storm, the game is paused, or reduced effects is on (reduced shows the static Storm grade only);
- within 60 ticks after a BossWarning or Boss event;
- within 45 ticks after a Windup, Contact or defender Damage event (`FxView` needs `tick`).

**Paint:** an `SKY_RGB` over-rect at alpha `0.14·I` in the grade pass, under lights, vignette and cues, so it cannot mask a cue.

**As built (W5-02; rulings RL and W-8 supersede the tick schedule above):**
- `art::lighting::sky_flash(seed, wall_ms, quiet: SkyQuiet, paused, reduced, storm) -> f32` on wall-clock milliseconds: `SKY_SLOT_MS` 8000, `SKY_FIRE_PERCENT` 60, `SKY_OFFSET_RANGE_MS` 4000, `SKY_RISE_MS` 33, `SKY_DECAY_MS` 233, `SKY_QUIET_BOSS_MS` 1000, `SKY_QUIET_CUE_MS` 750. `sky_pulse_start(seed, wall_ms)` is the slot's scheduled start; `h = splitmix(seed ^ slot ^ SKY_TAG)` with `seed = StageTheme::variation`.
- `SkyClock { wall_ms, speed }` travels in `RenderOptions.sky`. `BattleUi.sky_wall_ms` is advanced by `battle_view::interpolation::advance_sky_clock` from `Time<Real>`, frozen while the game is globally paused or no attempt is running. A fixed-step export or automated screenshot (`sequence_path` / `capture_path`) reads the active attempt's tick at 60 Hz instead, so its frames are deterministic.
- Quiet windows read `Attempt.events` (`sky_quiet(a, speed)`): an event `n` ticks old is `n * 1000 / (60 * speed)` ms old at the chosen observation speed. `sky_pulse(look, clock, a, reduced)` is what `render` passes to `scene_grade`.

**Modelled:** worst mean ΔL is 0.091 (linear blend, storm-graded arid) and 0.03-0.05 in gamma space. Both are below the 0.2 gate.

## 5. Weapon FX, wave 5

New module `crates/horde_game/src/art/battlefield/weapon_fx.rs`:

```rust
pub struct WeaponFx { pub flash_len: f32, pub flash_width: f32, pub flash_points: u8, pub flash_life_ticks: u8, pub flash_rgb: [u8;3],
  pub tracer_width: f32, pub tracer_life_ticks: u8, pub tracer_rgb: [u8;3], pub tracer_core_rgb: Option<[u8;3]>, pub tracer_core_width: f32,
  pub tracer_every: u8, pub dim_tracer_width: f32, pub dim_tracer_rgb: [u8;3], pub pellet_streak: Option<f32>, pub odd_volley_scale: f32,
  pub light_radius: f32, pub light_intensity: f32, pub weapon_skin: &'static str }
pub enum FxSource { Weapon(WeaponClass), Turret }
pub fn weapon_fx(source: FxSource) -> &'static WeaponFx;
pub const TURRET_FX: WeaponFx;
pub fn shooter_class(a: &Attempt) -> WeaponClass; // gear_snapshot Ranged GearRecord.weapon_class (stats.rs:134-141; set at sim/lifecycle.rs:143) else a.profile.weapon.class
```

Profile values. Flash length and width are in lane heights; lives are in 60 Hz ticks.

| Class | Flash len / width | Points | Life | Flash RGB | Tracer |
|---|---|---|---|---|---|
| Pistol | 0.18 / 0.30 | 4 | 4t | (248,207,117) | 1.0 pt, 5t, AMBER |
| SMG | 0.15 / 0.24 | 5 | 3t | (250,226,150), x0.8 on odd ordinals | 0.8 pt, 5t, (246,222,140) |
| Shotgun | 0.34 / 0.60 | 6 | 5t | (255,196,110) | one flash per volley; per-pellet streak 0.9 pt at 35% length, 5t, (236,170,96) |
| Rifle | 0.26 / 0.30 | 4 | 4t | (as today) | 1.4 pt, 7t, AMBER |
| Marksman | 0.34 / 0.18 | 4 | 6t | (255,236,200) | 2.0 pt, 12t, (255,214,150), core (255,252,240) 0.8 pt |
| MachineGun | 0.24 / 0.28 | 6 | 3t | (255,186,96) | bright 1.6 pt (255,120,60) every 3rd volley; others 0.7 pt (220,170,110) at 50% alpha |
| Turret | 0.14 / 0.20 | 4 | 3t | (200,230,240) | 1.1 pt, 6t, (180,220,235) |

- Widths scale by (lane/62.9) clamped to [1, 1.6].
- Light radius (in lanes) / intensity: pistol 0.9/0.35, SMG 0.7/0.25, shotgun 1.2/0.5, rifle 1.0/0.4, marksman 1.3/0.55, MG 1.0/0.4, turret 0.6/0.2.
- `weapon_skin` names the existing `rook/june.weapon.<class>` attachment skin, which covers the silhouette criterion.

### Volley grouping

`FxView` (battle_view.rs:262-271) gains:
- `tick: u64`, `seq: u64`;
- `volley: Option<Volley { ordinal: u32, first: bool }>`;
- `source: Option<FxSource>`;
- `ordnance: Option<OrdnanceKind>`.

`BattleViewModel::build` (:394-407) computes the volley:
- Walk `a.events` newest-first. Group defender Shot events (actor_id None) by tick; turret shots by (tick, structure id).
- `ordinal = a.shots - (groups newer than this one)`. `a.shots` increments once per trigger at manual.rs:124 and npc.rs:267, so ordinals stay stable as the 512-event log rolls.
- `first` = the lowest seq in the group.

Usage rules:
- Flash, light and casings draw only when `first`. Casings (animation.rs:541-582) drop from 18 to 2 per shotgun trigger.
- MG bright tracer when `ordinal % 3 == 0`.
- `Event.seq % 3` was rejected because seq is the global event cursor (sim.rs:286, :421-423).

`FxOptions` gains `source`. Traps stay skipped (fx.rs:228-234).

**Reduced:** no flash, no light, and one static 1-pt line in `tracer_rgb`.

**As built (W5-03):**
- `art/battlefield/weapon_fx.rs` holds the table above as `PISTOL_FX` .. `MACHINE_GUN_FX` and `TURRET_FX`; `weapon_fx(FxSource)` picks one and `paint_shot(painter, &WeaponFx, Shot { from, to, lane, age_ticks, volley, reduced })` is the only Shot painter (`fx::events` calls it; the W5-09 gallery uses the same call).
- `FxView` gained `tick`, `seq`, `volley: Option<Volley { ordinal, first }>` and `source: Option<FxSource>`. The source travels on the event (`None` for a trap), so `FxOptions` did not need a `source` field. `FxView.ordnance` is left to W5-05, its only reader.
- `battle_view::first_of_volley(a, index)` (no earlier Shot of the same tick and actor in the log) is shared by the view model and the casing sampler, so it does not depend on `Event.seq` (zero in hand-built logs). A turret volley has ordinal 0.
- Flash polygons: 4 corners a kite, 5 a blunt wedge, 6 a long spike. The flash shrinks from full to half length over its life; alpha runs 230 to 0. Tracers fade linearly over `tracer_life_ticks`; the dim machine-gun tracer is at half of that alpha.
- Reduced: one static 1-pt line per volley (a nine-pellet shell draws one line, to its first pellet's end point), full alpha for the tracer's life.
- `light_radius` and `light_intensity` are read by the W5-04 light pools (section 6).

## 6. Light, explosions, fire, hits, decals, wave 5

**Light pools** (`art/battlefield/light.rs`):
- `struct LightPool { at: Pos2, radius: Vec2, rgb: [u8;3], intensity: f32 }`, collected newest-first.
- Priority: explosion > fire zone > muzzle (volley.first, age ≤ flash_life+4t) > turret.
- Capped at `LIGHTS_MAX = 24`.
- Painted as one Mesh of 16-segment fans: centre `Color32::from_rgb_additive(rgb·intensity·grade.light_gain)`, rim TRANSPARENT.
- Explosions: radius 1.2x blast, intensity 0.8, life 0.35 s. Fire: radius 1.0x zone, intensity 0.35 ± 0.05 flicker by (tick/6)%2.
- Reduced: none.

**As built (W5-04):**
- `art/battlefield/light.rs`: `collect(arena, view, muzzle, textures, reduced) -> Vec<LightPool>` and `paint_pools(p, &pools, grade.light_gain)`, called from `render` as pass 3: after `paint_grade`, before `edge_shade` and the cue pass. The mesh is clipped to the field (the arena painter), not the canvas.
- Order: explosions, fire zones (highest id first), survivor volleys, turret shots; `view.fx` newest first inside each class; then `truncate(LIGHTS_MAX)`.
- Explosion: `EXPLOSION_LIGHT_RGB` (255,190,110), radii 1.2 x blast in yards and lanes, intensity 0.8 falling linearly to 0 over 0.35 s (no hard cut-off).
- Fire: `FIRE_LIGHT_RGB` (255,150,70), the zone's radii, 0.40 or 0.30 by `(tick/6) % 2`, times the zone's last-45-tick fade. It reads the sim tick, so it holds still while paused.
- Muzzle and turret: the profile's `flash_rgb`, a circle of `light_radius` lane heights at the drawn muzzle (`fx::shot_origin`, shared with the tracer), `light_intensity` while the flash shows and then a linear fade over `MUZZLE_LIGHT_TAIL_TICKS` = 4. Only a volley's first event lights a pool; a trap lights none.
- Centre colour: `Color32::from_rgb_additive(rgb * min(intensity * light_gain, 1))`; the 16 rim vertices are `TRANSPARENT`.
- Not in this row: a `SceneDraw` light counter and the PERF_BUDGET cap entry (W5-09 to W5-11 read `light::LIGHTS_MAX`).

**Molotov detection.** `FxView.ordnance` = Molotov when `a.events` has a Burn with the same actor_id and tick (sim/manual.rs:352-399), or a fire zone with that id exists. Otherwise it is Grenade. No saved field.

**Explosions** (new `art/battlefield/explosion.rs`, moved out of fx.rs:139-177). Six procedural frames over 0.5 s on rig cells 12 and 13; no new sheets. The rig sprite is magnified at most 1.5x, and the vector ring carries the size.
- Grenade: white core → shock ring → cell 12 plus 8 debris lines → cell 12 at 1.3x alpha .7 plus 5 grey puffs → rising smoke (frames 4-5).
- Molotov: glass glints and orange bloom (no ring) → 5 cell-13 tongues → cell 12 tinted (255,170,90) plus tongues → settling → dark smoke.
- Scorch tail (graft): at most 8 concurrent, 3 s fade, derived from Explosion events still in `a.events`.
- Reduced (graft B): grenade = solid static ring, Molotov = dashed static ring. These stay distinct without colour.

**Fire zones** (ordnance.rs:10-73):
- 4-frame cycle `((tick/6 + idx) % 4)` by mirror flip, y-scale [0.85, 1.0, 1.1, 0.95] and tint (255,220,160) or (255,180,110).
- Frozen when paused.
- Reduced: 9 static frame-0 flames.

**As built (W5-05):**
- `explosion_ordnance(a, event)` in `battle_view/field.rs` is the Molotov detection above; `FxView.ordnance` carries it for Explosion events (`None` for every other kind). `BattleViewModel.scorch` (`ScorchView { center, radius, age }`) is the newest `SCORCH_MAX` = 8 Explosion events no older than `SCORCH_SECONDS` = 3 s, read from `a.events`; `fx_lifetime(Explosion)` stays 0.5 s.
- `art/battlefield/explosion.rs`: `frame(age)` gives the frame (0..6) and the progress inside it; `prims(&Blast, reduced)` is the pure part list (`Part` names what a part is, `Geom` its shape) and `paint` draws the newest `EXPLOSIONS_MAX` = 8 bursts in three passes for all of them together (ground parts, rig icons, thrown and rising parts), called just before `fx::events`. `scorch` paints the marks on the ground before the fire zones.
- Grenade parts by frame: Core; Core and ShockRing; ShockRing, Fireball and 8 Debris; ShockRing, Fireball at 1.3x alpha 178 and 5 Puffs; ShockRing and 5 Smoke; 5 Smoke. Molotov: Bloom and 6 Glints; Bloom and 5 Tongues; Fireball tinted (255,170,90) and Tongues; Tongues; Tongues and 3 dark Smoke banks; a dim Bloom and Smoke. A Molotov never has a ring.
- The rig icon is clamped by `clamp_sprite` to `SPRITE_MAX_MAGNIFICATION` = 1.5 screen pixels per texel (it reads the painter's pixels per point); without the rig sheet the fireball is a disc and a tongue a box.
- Reduced: `Part::SolidRing` (grenade) or 12 `Part::Dash` lines (Molotov) at the blast radius, alpha 150, the same for the whole 0.5 s; the scorch mark holds alpha 70 for its 3 s.
- Fire zones: `ordnance::flame_frame(tick, index, reduced)` is frame `(tick/6 + index) % 4`: mirror on frames 1 and 3, the y-scale table above, tint (255,220,160) on frames 0-1 and (255,180,110) on 2-3; reduced holds frame 0. `FLAMES_PER_ZONE` = 21, `FLAMES_PER_ZONE_REDUCED` = 9; flames are drawn on the newest `FIRE_ZONES_DRAWN_MAX` = 8 zones (every zone keeps its ground and radius ring).
- Not in this row: the PERF_BUDGET fx entries and the fx-stress scene (W5-09 to W5-11 read the constants above).

**Hits** (fx.rs:293-345):
- Spark length = sy·clamp(0.05 + 0.04·log2(1 + dmg/10), 0.06, 0.25).
- Stroke width = clamp(sy·0.024, 1, 3).
- Count 4/6/8 by damage tier; at most the newest 64 impact events.

**Kill burst:** 6 sparks at 0.16·sy, x1.8 when the target_id corpse is a boss.

**Blood decals** (`art/battlefield/decals.rs`):
- Newest 64 `a.corpses` (at, tick, id), 3-5 hash-shaped blobs (78,30,26) alpha 150.
- One untextured mesh, drawn before corpse_batch (visual_assets.rs:442).
- Only when `settings.blood`; static in reduced.

## 7. Elite tells: enemy_visuals.json v2

### Wave 1 loader (visual_assets/enemy_visuals.rs)

- `File.schema_version` accepts 1 or 2 (the reject is today at :110-115).
- New field `#[serde(default)] elite_tells: Option<EliteTellsEntry { default: EliteTell, affixes: Vec<AffixTell> }>`.
- `EliteTell { outline_rgb: [u8;3], marker: TellMarker, ring_px: u8 (1..=4), glow: f32 (0..=0.4), pulse_hz: f32 (0..=2.0) }`.
- `TellMarker { Chevron, Crown, Pips1, Pips2, Pips3, Shield, Spark }`.
- deny_unknown_fields, and at most 16 affixes, with no duplicates.
- A v1 file gets the built-in default tell: AMBER ring, Chevron, glow 0.15, 1.2 Hz.
- Rejected: schema ≥ 3, v1 with `elite_tells`, an unknown marker, out-of-range values, and (once wave 2 lands the table) an affix id missing from core's elite table.
- API: `EnemyVisualCatalog::elite_tell(affix: Option<&str>) -> &EliteTell`; an unknown affix gets the default tell.
- The bundled file goes to v2 with the default tell only. A byte copy of today's v1 file goes to `visual_assets/fixtures/enemy_visuals_v1.json`.
- `crisp_lane_px()` lives here as well.

### Wave 5 render

- `EnemyView` (battle_view.rs:200-221) gains `elite: Option<&'static EliteTell>`, set from `ResolvedEnemy.elite.affix_id` in `enemy_view` (:735).
- World pass: the tint lerps toward `outline_rgb` by `glow·(0.5+0.5·sin(2π·tick/60·hz))`. The pulse is at most 2 Hz on sim ticks, so it freezes on pause.
- Cue pass: a ground ring (`cue_ellipse`, ring_px) plus the shape marker above the HP bar (`cue_marker`). The shape is colour-blind safe and Okabe-Ito colours are secondary.
- Reduced: static glow·0.5, no pulse.
- Affix tells are appended to enemy_visuals.json in wave 5.

## 8. Drops: quality glint (ui/loot.rs:153-230)

- `draw_for` gains `reduced: bool`. Callers: ui/loot.rs:134-151 and ui/operations.rs.
- Rare and better get one additive diagonal band sweep across the 25x25 icon between ages 0.15 and 0.55 s.
- Every tier gets a corner marker, using ux-flow's shared `quality_marker(Quality)` shape set (Uncommon dot, Rare diamond, Epic triangle, Legendary star).
- Reduced: no glint, no rise or drift. The fly-up holds at -12 pt and only fades. This fixes loot.rs:178-183.
- No auto-sell or sale status is ever shown on the battlefield.

## 9. Reduced-effects coverage (every new effect)

- No muzzle flash, light pool, sky flash, explosion keyframes, embers, glint, elite pulse, fly-up rise or scorch motion.
- Tracers become static 1-pt class-coloured lines.
- Grenade shows a solid ring and Molotov a dashed ring. Fire shows 9 static flames. Decals are static.
- Storm stages show the static Storm grade.

## 10. Defender appearance: Npc no longer means Mara (wave 3)

- New `defender_presentation::appearance(a: &Attempt) -> Appearance` returns `a.profile.appearance`.
- It replaces the Mode::Npc overrides at art/battlefield/mod.rs:104-108 and ordnance.rs:90-94.
- It relies on:
  - core already freezing Female for Mara (campaign/loadout.rs:222-227);
  - save-determinism's v6→v7 step writing "Female" into every Npc attempt (active and `battles[]`) that lacks the key, since the serde default is Male (loadout.rs:97-98);
  - campaign-defenders freezing the recruit's appearance into `DefenderProfile.appearance` at takeover deploy.
- The Npc rule branches stay as they are: animation.rs:284/377 and defender_presentation.rs:100/122.
- The HUD name and portrait (hud/view.rs:85-91) go to ux-flow.

## 11. Provenance (wave 1, lands atomically with the decision-10 archive move)

- **`crates/horde_game/src/test_support/sha256.rs`** (`#[cfg(test)]`, ~80 lines, FIPS 180-4). It is shared with the audio provenance test, so there is no new crate and no Cargo.lock change. Test vectors: '', 'abc', the 448-bit message and 1,000,000 'a'.
- **`crates/horde_game/src/visual_assets/provenance_tests.rs`** walks `assets/art/**` and `assets/player_models/**`. Every file must be one of:
  1. a PNG listed in its directory's PROVENANCE.json with generator and a matching sha256;
  2. an allowlisted data file with a reason (exactly: enemy_visuals.json, 4 pixel/*-source.json, 5 player_models/*.json);
  3. a PROVENANCE.json.
- **Further rules:**
  - every listed file exists;
  - every listed PNG is referenced by `loading.rs` PATHS (:22-30, made `pub(crate)`) or by a player-model source, or carries `retained_reason`;
  - player-model `png_sha256` equals the file hash.
- **Archive move:** the 6 root sheets (actors, props, checkpoint-keyart, ground, biome-panoramas and bosses -v1.png, 16,013,365 bytes) move to `archive/art/`, together with `archive/art/PROVENANCE.json` (sha256 added). The `assets/CREDITS.md` line is updated. `archived_art_hashes_match` covers them.
- **Retained files:** keyart-v1 gets `retained_reason: "edit source of keyart-v2; visual_assets/tests.rs:363"`. The 5 animation strips (7.75 MB) get `retained_reason: "pending archive approval"`; the user decides whether to archive them.

## 12. Measurement method

- **Unit level** (art/lighting.rs tests):
  - WCAG 2.x relative luminance (sRGB decode; 0.2126/0.7152/0.0722) and contrast (L1+.05)/(L2+.05);
  - computed over the decoded terrain-v1.png swatch means and p90, composited as ground_material.rs does (soil + swatch at alpha 192/255);
  - measured composite mean/p90: temperate 0.028/0.033, boreal 0.053/0.062, arid 0.077/0.097, tropical 0.037/0.049;
  - every gate must pass under both gamma-space and linear-space blending.
- **Capture level:** new `scripts/measure_field_luminance.py` (stdlib zlib PNG decode, run with `python -I`).
  - It reads each capture plus its sidecar `<scene>.json`, which the review capture writes (graft): `{canvas_px, arena_rect_px, ppp, lighting, storm, reduced}`.
  - It measures over the arena inset by 4%.
- **Night gate:** `night-checkpoint` vs `dawn-reference` mean L ≤ 0.70.
  - Same country DZA (5 stages): stage 4 is checkpoint (clear/night) and stage 1 is suburb (mist/dawn).
  - Same biome, same forced lanes and width, paused at tick 0, no enemies.
  - Model and capture must agree within ±0.03 (graft B).
  - Measured coverage: 31 of 1026 stages are checkpoint, and 22 countries have both a checkpoint and a dawn stage.
- **Cue gate:** at least 3:1 against the INK halo. The raw ratio against graded p90 is also reported.
- **Reduced MOTION gate** (fx-stress): consecutive-frame |Δmean L| < 0.01 and no 32x32 block ΔL > 0.1.
- **Storm MOTION gate** (normal): frame ΔL < 0.2, with peaks at least 240 frames apart.

## 13. Budgets (docs/PERF_BUDGET.json fx block; schema owned by perf-architecture)

- lights 24, decals 64, scorch 8, explosions_concurrent 8, tracers 96, hit_events 64;
- flames_per_zone 21 (reduced 9), fire_zones_drawn 8, elite_tells 48, particles 128.

fx-stress interface mean must stay at or under the perf targets of 1.13 ms (1440x900) and 1.03 ms (1050x700@1.35) at 225 enemies.

## 14. Review scenes (ui/review/matrix.rs, fixtures.rs)

- **`weapon-classes`:** an 8-panel gallery (6 classes, 9-pellet double barrel, turret). Each panel is its own isolated Game, paused at shot tick + 1, painted through the real `paint_shot` path.
- **`storm`:** at a pulse peak, via a `first_pulse_after(seed, tick)` helper.
- **`night-checkpoint`** and **`dawn-reference`.**
- **`fx-stress`:** 17 lanes, 4 fire zones, a grenade and a Molotov, MG and turret, a boss, night, blood on, 30% elites.
- **`elite-tells`.**
- **Wave-3 captures:** `battle`, `construction`, `max-lanes`, `peak` and `operations`.
- **Rules:**
  - Every scene is captured at both sizes, normal and `LASTLIGHT_REVIEW_REDUCED`, with MOTION for storm, weapon-classes and fx-stress.
  - New fixtures must not go through the fixtures.rs:138 path that panics today (owned by qa-captures).
  - The night-checkpoint scene means the checkpoint stage theme, not the decision-13 regional checkpoint.

## 15. Ordered steps (failing-first)

- **Wave 1:**
  - (1a) sha256 module, provenance tests and the archive move;
  - (1b) enemy_visuals v2 loader with v1 fixture and `crisp_lane_px`.
- **Wave 3:**
  - (3a) pure `fit_field`;
  - (3b) `battle_layout`, wired with construction overlay, edge cue, hint band, anchors, effects clip, docs and test rewrites;
  - (3c) observer, `scene_look` and dressing unit;
  - (3d) appearance accessor.
- **Wave 5:**
  - (5a) pass split, grade, cue halos and scaling, vignette move, storm table and guard tests;
  - (5b) lightning;
  - (5c) weapon_fx, volleys and casings;
  - (5d) light pools;
  - (5e) explosions, fire, Molotov and scorch;
  - (5f) hits and decals;
  - (5g) elite tells;
  - (5h) glint and reduced fly-ups;
  - (5i) review scenes, sidecar, measurement script and budget.

The full list with tests is in `steps`.

## 16. Risks

- **Fit-zoom supersedes earlier recorded requirements** (EXPANSION.md:99, PIXEL_PRESENTATION.md:77). The docs and old tests must change in the same commit as 3b.
- **On-screen speed at 3x12 rises 2.5x** (116 vs 46.4 pt per yard). This needs a human feel check; the pacing bots cannot see it.
- **17-lane fields at 1050x700** stay at 18.3-pt lanes and narrow to 472 pt (65% of width). Only HUD inset changes (ux-flow) could help.
- **Window-relative k** means resizing changes the proportion (1.356 vs 1.160), exactly as today.
- **The texel clamp depends on `ctx.pixels_per_point()`.** Capture pairs must come from the same machine.
- **The cue-pass split touches enemies.rs, fx.rs and mod.rs** at the seams perf-architecture batches in wave 1. Land 5a on top of that batching and reuse its BattleViewModel-built-once counting test.
- **Blend space is unverified.** The capture is the gate (±0.03 against the model).
- **The construction overlay hides 20-37%** of the 3x12 spawn side while combat continues. The edge cue mitigates this, and a docked strip is the fallback if ux-flow objects.
- **Cross-remit dependencies:**
  - the v6→v7 appearance back-fill (save-determinism) and the takeover appearance freeze (campaign-defenders);
  - `ResolvedEnemy.elite.affix_id` (name to be unified by save-determinism; critic gap 20);
  - archetype visual tint and scale (enemy-balance).
- **generate_world.py drops stage_themes on regeneration** (critic gap 0, owned by enemy-balance). vfx no longer depends on world.json theme edits, but `stage_theme_count_is_pinned_to_10` and `storm_stage_coverage_is_pinned` will catch drift.
- **The provenance test fails until the archive move lands in the same commit.**

## 17. Numbers to measure

- **Camera:** the zoom table at both sizes; `crisp_lane_px` = 296.7 ± 0.5.
- **Construction:** before/after rescale, today -18.1% / -34.6% yard, target 0%.
- **Luminance:**
  - night/dawn ratio: model ≈ 0.45, gate ≤ 0.70;
  - cue contrast vs INK, and raw vs graded p90;
  - sky-flash ΔL (≤ 0.091 model, gate < 0.2) and minimum pulse spacing (≥ 240 ticks);
  - reduced MOTION Δmean L < 0.01 and block ΔL < 0.1.
- **Storm coverage:** 131/1026 stages in 120 countries.
- **fx-stress:** interface mean ms at 225 enemies against 1.13 / 1.03, p99, frames > 50 ms (must stay 0), and the per-frame counters against the budget.
- **Wave-0 safe-area mean L at 1440x900:** battle 0.026, arid 0.062, tropical 0.033, peak 0.030. At 1050x700: 0.027, 0.063, 0.034, 0.031.

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U2. Audio assets

- **Owner / wave:** audio, wave 6.
- **Ruling:** New gun, enemy and UI audio: YES (Rust offline exporter, 44.1 kHz). No new music files. No new ImageGen sheets.
- **Supersedes in this spec:** Any plan that would need a new raster sheet: visuals stay procedural, vector or tinted variants of existing sheets.

#### U6. Camera

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Uniform zoom-to-fit that keeps the lane:yard proportion; fewer lanes means more zoom; construction never rescales the arena. Detail in R1 and the camera-layout cluster.
- **Supersedes in this spec:** Constant-k aspect, magnification clamp and wave-3 placement.

#### U7. Archive list

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** Archive to horde-defense/archive/: the 12 unreferenced WAVs, the 6 unreferenced root sheets in assets/art/, the 5 unreferenced animation strips in assets/art/animation/, pixel/keyart-v1.png, and survival_npc_shot_0..2.wav (Cue::NpcShot stays as a retired row). Nothing is deleted.
- **Supersedes in this spec:** "retained_reason: pending archive approval" for the 5 strips and keyart-v1; the needs_user item is answered yes.

### Orchestrator rulings

#### R1. Camera formula, owner and construction overlay

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Base = today's per-geometry clearing_rect for (lane_count, width_yards) including its clamps; uniform zoom z = min(arena.w/base.w, arena.h/base.h) (&gt;= 1), centred; no texel or magnification clamp; 17x30 and every other already-filling field stays pixel-identical; the worker observer window uses the same fit function. Owner vfx-lighting, implemented in WAVE 1 because later captures depend on it. Construction is an overlay panel drawn over the field (no arena rescale), with an INCOMING occlusion chevron when the panel covers spawns; Save/Clear plan stay in the panel.
- **Supersedes in this spec:** legacy_aspect / ASPECT_RANGE / MAX_MAGNIFICATION / FieldFitClamps / texel clamp; wave-3 placement of camera steps 3a-3c; panel alpha 215.

#### R2. Defender identity

- **Owner / wave:** campaign-defenders, wave 1-2.
- **Ruling:** Serde key "defender" on Attempt and Report; an absent value means the mode owner (Player for Personal, Companion/Mara for Npc), so existing GOLDEN digests do not move for this reason. Mara stays selectable from the Companion node; recruits from the Recruitment node. One status model (RecruitStatus::Defending appended) and one npc source of truth (the roster entry, justified for determinism in D6). Takeover deploy stamps the recruit's appearance identity, not Mara's Female.
- **Supersedes in this spec:** The forced-Female art branch (D17).

#### R8. Single owners

- **Owner / wave:** progression-tree, wave 1-4.
- **Ruling:** Pacing harness v2 = progression-tree (wave 2; CD and EB add profiles on its API); per-archetype telemetry = enemy-balance; SHA-256 / provenance / orphan helpers and scripts/lib/Native.ps1 = qa-captures (wave 1; audio, vfx and perf reuse them); measure_performance.ps1 = perf-architecture on top of Native.ps1; storm tagging = vfx presentation table only (no world.json change); regional checkpoints are derived (no saved field), presentation lines by CD plus EB's Regional boss rank (HP and reward x1.2) in wave-4 balance, no theme-rule change; save_v7.json is cut at the wave-1 close and frozen, later cases use inline JSON or new fixtures; stress scene raises the staged enemy cap only behind a review/test-only seam (no production save accepts &gt; cap+1), owner perf.
- **Supersedes in this spec:** test_support/sha256.rs and provenance_tests.rs walker.

#### RL. Lightning pacing and reduced storm grade

- **Owner / wave:** vfx-lighting, wave 5.
- **Ruling:** Lightning pulses use wall-clock pacing (&lt;= 1 per 4 s wall time at any observation speed). Storm stages in non-storm themes get a static storm grade in reduced mode.
- **Supersedes in this spec:** Tick-based 480-tick slot schedule; "reduced -&gt; sky_flash 0" without a grade override.

### Cluster: camera-layout

#### C-1. Camera owner, wave and file placement

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** vfx-lighting implements camera and battle layout in wave 1 in four commits: C1 crates/horde_game/src/art/camera.rs (pure Rect math, re-exported as art::{clearing_size, fit_field, FieldFit}); C2 ui/battle/layout.rs BattleLayout plus main-battle wiring; C3 observer window; C4 INCOMING chevron. perf's ui/battle split (W1-39) lands before C2. Presentation-only: no saved field, no digest move, no Command change. perf measures the commit before C2 and C2. qa verifies by captures and does not write camera tests. C2 deletes clearing_rect and battle_arena (grep guard).
- **Supersedes in this spec:** Steps 3a-3c in wave 3 (scene_look for the observer stays later, wave 5 W5-01).

#### C-2. Camera formula

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** art/camera.rs: REFERENCE_LANES 11.0, REFERENCE_WIDTH_YARDS 30.0, MIN_CLEARING vec2(160, 86); FieldFit{rect, zoom}; clearing_size(reference, lanes, width) is today's clearing_rect size moved verbatim from ui/battle.rs:475-489; fit_field(reference, area, lanes, width): zx = area.w/base.x, zy = area.h/base.y, the smaller wins and the limiting axis snaps exactly to the area; centred. No texel clamp. Every field with &gt;= 11 lanes is bit-identical to today. Pinned zooms: arena 1392x692: 3x12 1392x471.8 z2.5, 5x14 1392x674.0 z2.143, 7x16 1166.6x692 z1.571, 9x18 1020.8x692 z1.222; arena 729.8x310.5: 3x12 729.8x215.0, 5x14 729.8x302.5, 7x16 611.6x310.5, 9x18 535.2x310.5. EXPANSION.md:99 and PIXEL_PRESENTATION.md:77 get a dated supersession note. Tests: fresh_three_lane_battle_fills_the_arena_width_at_small_size (fail-first), fit_field_zoom_table_at_both_target_sizes, arena_fits_full_lanes_and_width_on_the_limiting_axis, fit_field_keeps_every_filling_geometry_bit_identical (pin with two mutation proofs).
- **Supersedes in this spec:** legacy_aspect, ASPECT_RANGE, MAX_MAGNIFICATION 1.10, FieldFitClamps, texel_clamped, constant-k (17x30 shrinking to 900x692).

#### C-3. Single BattleLayout API

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** One pure struct in ui/battle/layout.rs built once per battle frame: BattleLayout{canvas, arena, field, hint_band, hint_callout, effects_clip, hud_left, hud_right, selector, receipt_slot, construction_panel, pause_banner_center, bar, legend_anchor} with BattleLayout::new(canvas, lanes, width_yards, hint_band). Constants TOP_BAND 124, ARENA_SIDE_INSET 24, ARENA_BOTTOM_INSET 84, HINT_BAND 92, HINT_CALLOUT_TOP 4, HINT_CALLOUT_HEIGHT 84, FX_TOP_HEADROOM 60, PANEL_MAX_WIDTH 280, PANEL_RIGHT_MARGIN 16, PANEL_TOP_GAP 8, PANEL_MIN_HEIGHT 180, PANEL_FILL_ALPHA 235, CHEVRON_GAP 10. arena = today's battle_arena(canvas,false); selector and receipt_slot anchor to arena.top (never move with zoom); bar and legend unchanged. f.battle.arena_rect = layout.field.rect, effects_rect = layout.effects_clip. Later owners may append fields, not redefine them.
- **Supersedes in this spec:** battle_layout(canvas, lanes, width, hint_band, clamps) and the banner_y letterbox rule.

#### C-4. Hint band

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** hint_band = view.first_deployment && view.personal(), for the whole attempt, independent of show_guidance. Field area = arena minus a 92-pt top band, honoured only when the fit zoom stays &gt;= 1; otherwise fall back to the full arena (filling fields stay bit-identical). The callout is drawn at today's rect only when show_guidance && first_deployment && personal. Npc first deployments get no band.
- **Supersedes in this spec:** Band without the zoom &gt;= 1 fallback.

#### C-5. Effects clip

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Main battle effects_clip = arena extended upward by FX_TOP_HEADROOM = 60 pt (top at canvas.top+64), x at the arena edges, bottom at arena.bottom. Used only by overlay passes (damage, loot, reticle). The battlefield sprite painter stays clipped to the arena. Observer clip = its allocated box.
- **Supersedes in this spec:** Effects clip = canvas y in [top+12, bottom-76].

#### C-6. Construction overlay

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** The construction panel overlays the field; the field rect never depends on construction. width = min(280, canvas.w*0.5-32), left = canvas.right-16-width, top = arena.top+8, height = max(arena.h-16, 180): bit-identical to today at 17x30; never covers a build cell. Fill black alpha 235, scroll area; construction_controls including "Save country plan"/"Clear country plan" stay inside. Field hover is ignored inside the panel. Tests: construction_toggle_keeps_arena_rect (fail-first), construction_panel_never_covers_a_build_cell, construction_panel_keeps_plan_buttons, construction_panel_swallows_field_hover.
- **Supersedes in this spec:** Panel alpha 215.

#### C-7. INCOMING occlusion chevron

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** Pure occluded_spawn_cues(field, panel, enemies) -&gt; Vec&lt;(u8, Pos2)&gt; in ui/battle/layout.rs from live enemies (bosses included), one cue per lane whose enemy x projects at or right of panel.left. Drawn after the panel only while construction is on: static AMBER right-pointing chevron 12x16 on INK, "INCOMING" on the first cue. RenderOptions gains no occluder field. BattleUi.occluded_lanes (presentation-only) records the lanes.
- **Supersedes in this spec:** RenderOptions.occluder drawn in the cue pass and the "Incoming events" input.

#### C-8. Observer window fit

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** ui/operations.rs:276-278: effects_clip = r; r = art::fit_field(r, r, lanes, width).rect. No hint band or headroom. BattleUi.observer_arena_rect is written each drawn frame and cleared on close. scene_look stays later (W5-01).
- **Supersedes in this spec:** Observer keeps clearing_rect.

#### C-9. Forest dressing under zoom

- **Owner / wave:** vfx-lighting, wave 1.
- **Ruling:** art::world_backdrop gains a trailing max_unit; unit = lane_height.min(max_unit).max(1.0); main battle passes arena.h/11, the observer box.h/11. Lanes &gt;= 11 stay bit-identical.
- **Supersedes in this spec:** "unit = min(field_lane, legacy_lane)" without an API.

#### C-12. Camera capture review and human check

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** qa captures battle, construction, max-lanes (17x30), peak/progressed-peak and operations-observer at both sizes, normal and reduced, after C4, and reviews every flagged pair with the written reason "uniform zoom-to-fit, construction overlay, effects headroom". 17x30 and 11x20 pairs must show no arena move. ReviewFrameFacts arena_rect/effects_rect come from f.battle; observer facts from observer_arena_rect. Human checks "3x12 at 2.5x zoom feel" and "sheet upscale on 3-lane sprites" are logged pending if not done.
- **Supersedes in this spec:** Wave-3 captures for the camera (later lighting captures stay in wave 5).

### Cluster: defenders

#### D17. Battlefield appearance reads the stamped profile

- **Owner / wave:** vfx-lighting, wave 3.
- **Ruling:** vfx deletes the Npc =&gt; Female branches at art/battlefield/mod.rs:104-108 and ordnance.rs:90-94 and reads a.profile.appearance (Attempt::defender_appearance()). The observer draws the worker attempt's appearance and takes names from Game::battle_defender_card(battle_id). Main-run Mara stays pixel-identical (W3-17).
- **Supersedes in this spec:** The forced-Female presentation mapping.

### Cluster: settings-audio-tooling

#### S7. SHA-256: one implementation

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One std-only FIPS 180-4 SHA-256 at crates/horde_core/src/sha256.rs, #[cfg(any(test, feature = "test-support"))] pub mod sha256 with digest() and hex(). No new crate, dependency or Cargo.lock change. PowerShell uses Get-FileHash via Native.ps1 Get-FileSha256; Python keeps hashlib.
- **Supersedes in this spec:** test_support/sha256.rs.

#### S8. Provenance and orphan audit helpers

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One #[cfg(test)] mod asset_audit at crates/horde_game/src/asset_audit.rs (qa): workspace_root, memoised file_sha256, shipped_files, audio_manifests (both shipped manifests), art_manifests, archive_manifest. Runtime sets: visual_assets::loading::PATHS (pub(crate)), enemy_visuals sheets, player_models PNGs, #[cfg(test)] audio::runtime_wav_paths() (Cue::ALL minus retired x variants plus beds). vfx's provenance walker folds in; later owners extend only the runtime-set functions in the commit that adds files. Tests include every_shipped_wav_is_loaded_and_matches_provenance (78 then 75), shipped_art_has_provenance_with_matching_sha256, shipped_art_is_referenced_at_runtime, archived_files_keep_matching_provenance, player_model_png_sha256_matches_file, every_manifest_entry_exists.
- **Supersedes in this spec:** visual_assets/provenance_tests.rs as a separate walker.

#### S9. Archive layout

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One qa commit with git mv mirrored under horde-defense/archive/: 6 root sheets to archive/art/, 5 strips to archive/art/animation/, pixel/keyart-v1.png to archive/art/pixel/; one archive/art/PROVENANCE.json (12 entries keyed relative to archive/art, original record plus sha256, bytes, archived_from, archived, reason). The keyart pin (visual_assets/tests.rs:363) retargets to the archive path. 12 legacy WAVs to archive/audio/ with archive/audio/PROVENANCE.json (12 records plus 8 preserved_originals). archive/README.md: "Not shipped; retained by production-pass decision 10; restore with git mv". archive/ is outside assets/, so packaging needs no change.
- **Supersedes in this spec:** "retained_reason: pending archive approval".

### Cluster: enemies-world-pacing

#### W-8. Storm tagging, reduced storm grade and wall-clock lightning

- **Owner / wave:** vfx-lighting, wave 5.
- **Ruling:** art::lighting::STORM_THEME_IDS = ["depot"]; is_storm(theme) = weather == "storm" (case-insensitive) or id in the table; no StageThemeDefinition.storm, no world.json or theme-order change. Reduced effects on any storm stage: sky_flash 0 and the static Lighting::Storm grade. sky_flash takes wall_ms from an unscaled real-time accumulator (paused frames excluded): SKY_SLOT_MS 8000, SKY_OFFSET_RANGE_MS 4000, SKY_RISE_MS 33, SKY_DECAY_MS 233, SKY_QUIET_BOSS_MS 1000, SKY_QUIET_CUE_MS 750; pulse starts &gt;= 4000 ms apart at 1x/2x/4x; schedule hash splitmix(variation ^ slot ^ SKY_TAG), never core RNG.
- **Supersedes in this spec:** §4 tick-based SKY_SLOT_TICKS 480 schedule and reduced "sky_flash 0" without a grade override.

#### W-9. Regional checkpoints and Capstone

- **Owner / wave:** enemy-balance, wave 2, 4.
- **Ruling:** One derivation: FrontPosition{stage, count}::boss_rank() (Capstone if stage+1 == count, else Regional if (stage+1) % 6 == 0, else Sector); Country::regional_checkpoints() from it (&lt;= 3 for 24 stages). Nothing saved per stage; only attempt-scoped snapshot.boss_rank and Report/RunRecord.boss_rank. No theme change, no checkpoint coin grant. Wave 2: FrontPosition, stamping, Capstone +1 transition. Wave 4: Regional HP x1.2, reward x1.2; Capstone HP x1.4, armor x1.25, NPC armor +2, reward x1.4, damage x1.10. CD owns presentation lines in wave 4 reading Game::stage_boss_rank.
- **Supersedes in this spec:** No forced night theme on Regional stages.

#### W-13. Visual twins for EnemyWindup and BossPhase

- **Owner / wave:** vfx-lighting, wave 2.
- **Ruling:** The existing windup ellipse and bar already draws for every Windup enemy, defender-targeted included (critic claim incorrect). Added: EnemyView.aim_target dashed aim line for ranged windups (W2-28); BattleView.boss_phase: Option&lt;BossPhaseView{transition, name, hold_left_ticks}&gt; with label "PHASE n - NAME" and a ring for the whole 90-tick hold, static in reduced mode (W2-37). They precede the audio cues; the UX boss banner is not the twin.
- **Supersedes in this spec:** Nothing beyond the ruling itself; this remit implements it.

### Wave-1 close rulings (2026-11-07)

Orchestrator rulings R12-R23, recorded verbatim in plan §2.4. The ones that touch this remit:

#### R17. Capture rows CF-1-01, CF-1-02, CF-1-03 and CF-1-07

- **Owner / wave:** vfx-lighting, wave 3 (W3-29, W3-30, W3-31).
- **Ruling:** CF-1-01, CF-1-02, CF-1-03 and CF-1-07 get new wave-3 ids appended in waves.md (owner vfx-lighting, before W3-27, beside W3-14 for BattleLayout), each with its CF fail-first test; update the CF ledger rows with those ids.
- **Supersedes in this spec:** CF-1-03's qa-captures owner: W3-30 is a vfx-lighting commit. The ledger rows in plan §10 name the ids and fail-first tests.

### Rulings owned by this remit

U6, R1, RL, C-1, C-2, C-3, C-4, C-5, C-6, C-7, C-8, C-9, D17, W-8, W-13, R17
