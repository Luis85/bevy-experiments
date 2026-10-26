# Bevy 0.20 API Reference for an Isometric RTS (verified 2026-10-23)

Source of truth: the `bevy 0.20.0` crate sources in `~/.cargo/registry`, plus the official migration guides (0.18→0.19, 0.19→0.20) and the 0.20 release post on bevy.org (bevyengine.org now redirects to bevy.org).

Probe projects (all paths are under `C:\Users\LUISME~1\AppData\Local\Temp\claude\C--Projects-the-factory--claude-worktrees-infallible-heisenberg-546176\b7db3890-88e3-4980-be96-2fec1adbea3a\scratchpad\bevy020-probe\`):
- `probe\` is bevy-only. It passes `cargo check` with zero errors and zero deprecation warnings. **A full `cargo build` was run, and the app ran for 8 s** on Windows 11 with an RTX 1000 Ada GPU (Vulkan).
- `probe3p\` holds the third-party crates. It passes `cargo check`.

**Legend.** VERIFIED (check) means the code compiles against 0.20.0. VERIFIED (run) means the behavior was also seen in the running app. UNVERIFIED means it was taken from docs or source and not exercised.

---

## 1. Toolchain status

| Item | Value |
|---|---|
| Installed | `cargo 1.96.1`, `rustc 1.96.1` (stable-x86_64-pc-windows-msvc, 2026-06-26) |
| bevy 0.20.0 `rust-version` (crates.io) | **1.97.1**. 0.20.0-rc.* required 1.96.0; 0.19.x required 1.95.0 |
| Verdict | **The installed toolchain is too old by the declared MSRV.** Cargo refuses to resolve or build it without `--ignore-rust-version`. With that flag, check, build and run all worked on 1.96.1, but that is unsupported. The `wesl*` and `wgsl-parse` crates also declare 1.97.1. |
| Action | Run `rustup update stable`. 1.99.1 is available per `rustup check`. Then pin the version in `rust-toolchain.toml`. |
| Windows gotcha | Bevy build scripts exceed MAX_PATH (260) when the target dir is deep, and link.exe fails with `LNK1104`. The probe uses `.cargo/config.toml` with `[build] target-dir = "C:/Users/LuisMendez/AppData/Local/Temp/b020t"`. That directory holds 9.6 GB and is safe to delete. Keep project and target paths short. |

Build cost: bevy dependency tree check took 2m24s cold; full debug build took 5m10s on a 16-core Core Ultra 9.

---

## 2. Topics with verified snippets

All imports come from `use bevy::prelude::*;` unless shown. Full files are in `probe\src\*.rs`.

### 2.1 App, DefaultPlugins, Plugin, PluginGroup — VERIFIED (run)
```rust
use bevy::app::PluginGroupBuilder;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "rts".into(), ..default() }),
            ..default()
        }))
        .add_plugins(MeshPickingPlugin)          // NOT in DefaultPlugins
        .add_plugins(RtsPlugins)
        .run();
}

pub struct SimPlugin { pub fixed_hz: f64 }
impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(self.fixed_hz));
    }
}

pub struct RtsPlugins;
impl PluginGroup for RtsPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(SimPlugin { fixed_hz: 20.0 })
            .add(HudPlugin)
    }
}
// Override or disable a member:
// app.add_plugins(RtsPlugins.set(SimPlugin { fixed_hz: 30.0 }).disable::<HudPlugin>());
```
- `Plugin` hooks: `build`, `ready`, `finish`, `cleanup`, `name`, `is_unique`.
- `DefaultPlugins` in 0.20 includes `StatesPlugin`, `GizmoPlugin`, `GltfPlugin`, `UiWidgetsPlugins`, `DefaultPickingPlugins` (pointer input, picking, interaction), `InputFocusPlugin` and `InputDispatchPlugin`.
- It also includes both `WorldSerializationPlugin` (the old scenes) and `ScenePlugin` (the new BSN scenes).
- `MeshPickingPlugin` is **not** included. Add it yourself.
- To quit: `MessageWriter<AppExit>` and `.write(AppExit::Success)`. `AppExit` is a `Message`.

### 2.2 Isometric orthographic camera, pan, zoom, rotate — VERIFIED (run, screenshot confirmed the iso view)
```rust
use bevy::camera::ScalingMode;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::window::PrimaryWindow;

#[derive(Component)]
pub struct RtsCamera { pub focus: Vec3, pub yaw: f32, pub pitch: f32, pub distance: f32, pub view_height: f32 }
// defaults: yaw 45°, pitch 35.264° (true iso; use 30° for 2:1 dimetric), distance 100, view_height 30

fn rig_transform(r: &RtsCamera) -> Transform {
    let rot = Quat::from_euler(EulerRot::YXZ, r.yaw, -r.pitch, 0.0);
    Transform::from_translation(r.focus + rot * Vec3::new(0.0, 0.0, r.distance))
        .looking_at(r.focus, Vec3::Y)
}

fn spawn_camera(mut commands: Commands) {
    let rig = RtsCamera::default();
    commands.spawn((
        Camera3d::default(),                 // #[require(Camera, Projection)]; Projection defaults to perspective
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: rig.view_height },
            near: 0.1, far: 1000.0,
            ..OrthographicProjection::default_3d()
        }),
        rig_transform(&rig),
        rig,
    ));
}

fn pan_camera(time: Res<Time>, keys: Res<ButtonInput<KeyCode>>,
              window: Single<&Window, With<PrimaryWindow>>, mut rig: Single<&mut RtsCamera>) {
    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) { input.y += 1.0; } /* ...A/S/D, plus edge-scroll via window.cursor_position() */
    if input == Vec2::ZERO { return; }
    let yaw = Quat::from_rotation_y(rig.yaw);
    let delta = (yaw * Vec3::X * input.x + yaw * Vec3::NEG_Z * input.y).normalize()
        * rig.view_height * time.delta_secs();
    rig.focus += delta;
}

fn zoom_camera(scroll: Res<AccumulatedMouseScroll>, mut rig: Single<&mut RtsCamera>) {
    if scroll.delta.y != 0.0 {
        rig.view_height = (rig.view_height * (1.0 - scroll.delta.y * 0.1)).clamp(8.0, 120.0);
    }
}

fn apply_rig(mut cam: Single<(&RtsCamera, &mut Transform, &mut Projection), Changed<RtsCamera>>) {
    let (rig, ref mut tf, ref mut proj) = *cam;
    **tf = rig_transform(rig);
    if let Projection::Orthographic(o) = &mut **proj {
        o.scaling_mode = ScalingMode::FixedVertical { viewport_height: rig.view_height };
    }
}
```
- `ScalingMode` has these variants: `WindowSize`, `Fixed{width,height}`, `AutoMin{..}`, `AutoMax{..}`, `FixedVertical{viewport_height}`.
- `OrthographicProjection` fields: `near`, `far`, `viewport_origin`, `scaling_mode`, `scale`, `area`.
- With ortho, zoom by changing `viewport_height` (or `scale`), not distance.
- The built-in `PanOrbitCamera` is new in 0.20, behind the `pan_orbit_camera` feature (`bevy::camera_controller::pan_orbit_camera::prelude::*`). It compiled in `probe3p`, but it is a CAD-style orbit controller. A custom rig like the one above fits an RTS better.
- `bevy_rts_camera` 0.15 targets bevy 0.20 (see §5).

### 2.3 Meshes, StandardMaterial, primitives, shadows — VERIFIED (run, shadows visible)
```rust
use bevy::light::CascadeShadowConfigBuilder;

app.insert_resource(GlobalAmbientLight { brightness: 200.0, ..default() }); // resource name is GlobalAmbientLight

commands.spawn((
    Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
    MeshMaterial3d(materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.45, 0.25), perceptual_roughness: 1.0, ..default()
    })),
));
commands.spawn((
    Mesh3d(meshes.add(Cuboid::new(2.0, 1.0, 3.0))),
    MeshMaterial3d(materials.add(Color::srgb(0.4, 0.4, 0.3))),   // From<Color> for StandardMaterial
    Transform::from_xyz(0.0, 0.5, 0.0),
));
commands.spawn((
    DirectionalLight {
        illuminance: light_consts::lux::OVERCAST_DAY,
        shadow_maps_enabled: true,          // renamed from `shadows_enabled` in 0.19
        ..default()
    },
    Transform::from_xyz(-40.0, 50.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    CascadeShadowConfigBuilder { num_cascades: 3, maximum_distance: 150.0,
                                 first_cascade_far_bound: 30.0, ..default() }.build(),
));
```
- `Mesh3d` requires `Transform`.
- `DirectionalLight` also has `contact_shadows_enabled`, `shadow_depth_bias`, `soft_shadow_size`, and `affects_lightmapped_mesh_diffuse`. Shadow-map resolution comes from the `DirectionalLightShadowMap` resource.
- Primitives live in the new `bevy_shape` crate (`bevy::shape`). They are still re-exported by `bevy::prelude`.

### 2.4 Required components, observers, Events vs Messages, Query — VERIFIED (run: required components and the `Add` observer fired)

**Event and Message are separate concepts.** The split happened in 0.17 and continues in 0.20.
- `Event` is push-based and runs observers immediately. Fire it with `commands.trigger(..)` or `world.trigger(..)` and handle it with `On<E>`.
- `EntityEvent` is an event with a target entity. It can propagate.
- `Message` is the buffered, pull-based kind (the old `EventReader`/`EventWriter`). Use `#[derive(Message)]`, `app.add_message::<M>()`, `MessageWriter::write`, and `MessageReader::read`.
- There is **no** `add_event`, `EventReader` or `EventWriter` in 0.20.
- Asset events are messages too: `MessageReader<AssetEvent<T>>`.

```rust
use bevy::ecs::query::QuerySingleError;

#[derive(Component, Default)]
#[require(Health, Team, Transform, Visibility)]
pub struct Unit;
#[derive(Component)]
#[require(Unit, Health(250.0), Team(1))]        // inline constructor expressions
pub struct HeavyTank;

#[derive(Message, Clone, Copy, Debug)]
pub struct UnitKilled { pub victim: Entity, pub killer: Option<Entity> }

#[derive(Event, Debug)]
pub struct Alert { pub text: String }

#[derive(EntityEvent, Debug)]                     // field `entity` is the target (or mark one #[event_target])
pub struct Damage { pub entity: Entity, pub amount: f32 }

app.add_message::<UnitKilled>()
   .add_observer(on_unit_added)
   .add_observer(on_damage);

fn on_unit_added(add: On<Add<Unit>>, q: Query<&Team>) {   // 0.20: On<Add<T>> (was On<Add, T>)
    let _ = q.get(add.entity);
}
fn on_damage(dmg: On<Damage>, mut q: Query<&mut Health>, mut killed: MessageWriter<UnitKilled>,
             mut commands: Commands) {
    let Ok(mut hp) = q.get_mut(dmg.entity) else { return };
    hp.0 -= dmg.amount;
    if hp.0 <= 0.0 {
        killed.write(UnitKilled { victim: dmg.entity, killer: None });
        commands.entity(dmg.entity).despawn();
    }
}
fn read_kills(mut reader: MessageReader<UnitKilled>, mut commands: Commands) {
    for m in reader.read() { commands.trigger(Alert { text: format!("{:?} destroyed", m.victim) }); }
}
// Entity-scoped observer and EntityEvent trigger:
commands.spawn(HeavyTank).observe(|d: On<Damage>| info!("hit {}", d.amount));
commands.entity(e).trigger(|entity| Damage { entity, amount: 10.0 });
commands.add_observer(|r: On<Remove<Health>>| debug!("{:?}", r.entity));

// Query API
fn sys(cams: Query<&GlobalTransform, With<Camera3d>>,
       _cam: Single<(&Camera, &GlobalTransform)>,          // system skipped if != 1 match
       sel: Option<Single<Entity, With<Selected>>>) {     // None if 0 or >1
    match cams.single() {
        Ok(_) => {}
        Err(QuerySingleError::NoEntities(n)) => warn!("{n}"),
        Err(QuerySingleError::MultipleEntities(n)) => warn!("{n}"),
    }
}
fn sys2(units: Query<(Entity, &Health)>, selected: Query<Entity, With<Selected>>,
        cams: Query<&Transform, With<Camera3d>>) -> Result {
    let _tf = cams.single()?;                                 // systems may return Result
    for item in units.iter_many(selected.iter()) { let Ok(_x) = item else { continue }; } // 0.20: yields Result
    for _x in units.iter_many(selected.iter()).matched() {}   // old behaviour
    Ok(())
}
```
- Lifecycle event patterns: `Add<B>`, `Insert<B>`, **`Discard<B>`** (renamed from `Replace` in 0.19), `Remove<B>`, `Despawn<B>`. The event payloads are `AddEvent { entity }` and so on, reached through Deref on `On`.
- On `On<E>`: `event()`, `observer()`, and, for propagating entity events, `original_event_target()` and `propagate(bool)`.
- `single()` and `single_mut()` return `Result<_, QuerySingleError>`. `get_single` is gone.
- Since 0.20, panics in systems, commands and observers are caught and routed to `FallbackErrorHandler` (renamed from `DefaultErrorHandler` in 0.19).

### 2.5 Picking and cursor-to-ground raycast — VERIFIED (check). The cursor ray runs each frame without error, but clicks were not exercised.
```rust
use bevy::window::PrimaryWindow;

app.add_observer(on_unit_click);                    // global: fires for any clicked entity

fn on_unit_click(click: On<PointerClick>, units: Query<(), With<Unit>>, mut commands: Commands) {
    if click.button != PointerButton::Primary || units.get(click.entity).is_err() { return; }
    commands.entity(click.entity).insert(Selected);
    let _world: Option<Vec3> = click.hit.position;
    let _screen: Vec2 = click.pointer.position;     // 0.20: was pointer_location.position
}
// Per-entity:
commands.spawn((Unit, Pickable::default()))
    .observe(|o: On<PointerOver>| info!("{:?}", o.entity))
    .observe(|mut d: On<PointerDrag>| { d.propagate(false); let _ = d.delta; });
commands.spawn(Pickable::IGNORE);

// Cursor -> ground plane (y = 0)
fn cursor_to_ground(window: Single<&Window, With<PrimaryWindow>>,
                    camera: Single<(&Camera, &GlobalTransform)>, mut out: ResMut<CursorGround>) {
    let (camera, cam_tf) = *camera;
    out.0 = window.cursor_position()
        .and_then(|c| camera.viewport_to_world(cam_tf, c).ok())   // Result<Ray3d, ViewportConversionError>
        .and_then(|ray| ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
                           .map(|d| ray.get_point(d)));
}
// On-demand mesh raycast:
fn raycast(mut ray_cast: MeshRayCast, /* ... */) {
    let hits = ray_cast.cast_ray(ray, &MeshRayCastSettings::default());
    if let Some((entity, hit)) = hits.first() { let _ = (entity, hit.point, hit.distance); }
}
```
- **The 0.20 pointer events are flat:** `PointerOver`, `PointerOut`, `PointerEnter`, `PointerLeave`, `PointerPress`, `PointerRelease`, `PointerClick`, `PointerMove`, `PointerDragStart`, `PointerDrag`, `PointerDragEnd`, `PointerDragEnter`, `PointerDragOver`, `PointerDragLeave`, `PointerDragDrop`, `PointerScroll` and `PointerCancel`.
- Each one has `entity` and `pointer: Pointer { id, target, position }`, and most have `hit: HitData`. `Pointer<Click>`-style generics are gone.
- These events also derive `Message`, so a `MessageReader<PointerClick>` is likely possible. That is UNVERIFIED.
- Mesh picking is opt-out by default. For opt-in, set `MeshPickingSettings::require_markers = true` and add `MeshPickingCamera` and `Pickable` to the entities.
- For an RTS, do box-select yourself. Project unit positions with `camera.world_to_viewport(..)` and test them against the drag rectangle. That API is not probed; its name is UNVERIFIED for 0.20.

### 2.6 States, SubStates, system sets, FixedUpdate — VERIFIED (run: Loading→InGame transition happened)
```rust
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState { #[default] Loading, MainMenu, InGame }

#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(AppState = AppState::InGame)]
pub enum GamePhase { #[default] Deployment, Battle, Paused }

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimSet { Orders, Movement, Combat }

app.init_state::<AppState>()
   .add_sub_state::<GamePhase>()
   .configure_sets(FixedUpdate,
       (SimSet::Orders, SimSet::Movement, SimSet::Combat).chain().run_if(in_state(GamePhase::Battle)))
   .add_systems(OnEnter(AppState::InGame), enter_game)
   .add_systems(FixedUpdate, (issue_orders.in_set(SimSet::Orders),
                              move_units.in_set(SimSet::Movement),
                              resolve_combat.in_set(SimSet::Combat)));

fn enter_game(mut commands: Commands) {
    commands.spawn((Name::new("battle-root"), DespawnOnExit(AppState::InGame)));
}
fn toggle(mut next: ResMut<NextState<GamePhase>>) { next.set_if_different(GamePhase::Paused); } // was set_if_neq
fn move_units(time: Res<Time>) { let _dt = time.delta_secs(); }      // Time<Fixed> inside FixedUpdate
fn resolve_combat(fixed: Res<Time<Fixed>>) { let _a = fixed.overstep_fraction(); }
```
- The old `StateScoped` is replaced by `DespawnOnExit(S)` and `DespawnOnEnter(S)`.
- Since 0.19, `NextState::set` re-runs transitions, including `DespawnOnExit`, even when the state is set to its current value. Use `set_if_different` to avoid that.
- `ComputedStates` (`add_computed_state`) also exists.
- 0.20 adds `chain_weak()`, `before_weak()` and `after_weak()`. Several built-in schedule sets now use weak ordering. Plain `.chain()` is still strict.

### 2.7 Assets: glTF and a custom RON loader — VERIFIED (run: the RON loaded; the glTF path resolved and reported a missing file as expected)
```rust
// glTF scene (0.19+: WorldAssetRoot replaces SceneRoot)
commands.spawn((
    WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset("models/panzer.glb"))),
    Transform::from_xyz(-10.0, 0.0, 0.0),
));

// Custom loader
use bevy::asset::{io::Reader, AssetLoader, LoadContext};
use serde::Deserialize;

#[derive(Asset, TypePath, Deserialize, Debug, Clone)]
pub struct UnitDef { pub name: String, pub hp: f32, pub speed: f32, pub armor: Armor, pub model: String }

#[derive(Default, TypePath)]                 // AssetLoader: TypePath is required
pub struct UnitDefLoader;

#[derive(Debug, thiserror::Error)]
pub enum UnitDefError {
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("ron: {0}")] Ron(#[from] ron::error::SpannedError),
}

impl AssetLoader for UnitDefLoader {
    type Asset = UnitDef;
    type Settings = ();
    type Error = UnitDefError;               // bound: Into<BevyError>
    async fn load(&self, reader: &mut dyn Reader, _s: &(), _cx: &mut LoadContext<'_>)
        -> Result<UnitDef, UnitDefError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(ron::de::from_bytes::<UnitDef>(&bytes)?)
    }
    fn extensions(&self) -> &[&str] { &["unit.ron"] }
}

app.init_asset::<UnitDef>().register_asset_loader(UnitDefLoader);
let h: Handle<UnitDef> = server.load("units/panzer_iv.unit.ron");
fn on_loaded(mut ev: MessageReader<AssetEvent<UnitDef>>, defs: Res<Assets<UnitDef>>) {
    for e in ev.read() {
        if let AssetEvent::LoadedWithDependencies { id } = e { let _ = defs.get(*id); }
    }
}
```
- **serde and RON:** bevy uses serde and ron 0.12 internally for `.meta` files, but **re-exports neither**. Add `serde = { features = ["derive"] }` and `ron = "0.12"` yourself; ron 0.12.2 resolved here, the same line bevy uses.
- The `serialize` cargo feature only adds serde impls to bevy types.
- Alternative: `bevy_common_assets` gives the one-line `RonAssetPlugin::<UnitDef>::new(&["unit.ron"])`. It is VERIFIED (check) in `probe3p`.
- Other renames in 0.19:
  - `Scene` → `WorldAsset`, `DynamicScene` → `DynamicWorld`, `SceneSpawner` → `WorldInstanceSpawner`, `bevy::scene` → `bevy::world_serialization`.
  - `bevy::scene` is now **BSN**, the new `bsn!` scene system.
  - glTF `"x.glb#Material0"` now loads `GltfMaterial`. Use `#Material0/std` for `StandardMaterial`.
  - Advanced loads go through `asset_server.load_builder()...`, and inside loaders `LoadContext::load_builder()`. UNVERIFIED beyond the migration guide.

### 2.8 bevy_ui HUD and gizmos — VERIFIED (run: the HUD rendered with text and button)
```rust
use bevy::picking::hover::Hovered;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button as UiButton};   // prelude `Button` is the DEPRECATED bevy_ui one

commands.spawn((
    Node { position_type: PositionType::Absolute, bottom: px(0), left: px(0),
           width: percent(100), height: px(120), padding: UiRect::all(px(8)),
           column_gap: px(8), align_items: AlignItems::Center, ..default() },
    BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.85)),
)).with_children(|hud| {
    hud.spawn((SelectionText, Text::new("No selection"),
               TextFont { font_size: FontSize::Px(18.0), ..default() }, TextColor(Color::WHITE)));
    hud.spawn((
        UiButton, Hovered::default(),
        Node { width: px(120), height: px(40), justify_content: JustifyContent::Center,
               align_items: AlignItems::Center, ..default() },
        BackgroundColor(Color::srgb(0.25, 0.25, 0.25)),
        children![(Text::new("Stop"), TextFont { font_size: FontSize::Px(16.0), ..default() })],
    )).observe(|_a: On<Activate>| info!("Stop order"));
});

fn style_buttons(mut q: Query<(&Hovered, Has<Pressed>, &mut BackgroundColor), With<UiButton>>) {
    for (h, pressed, mut bg) in &mut q {
        bg.0 = match (pressed, h.get()) { (true, _) => Color::srgb(0.1,0.5,0.1),
                                          (false, true) => Color::srgb(0.35,0.35,0.35),
                                          _ => Color::srgb(0.25,0.25,0.25) };
    }
}

// Gizmos (immediate mode, call every frame)
fn draw(mut gizmos: Gizmos, sel: Query<&GlobalTransform, With<Selected>>) {
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);   // XY-plane shapes -> ground
    for tf in &sel { gizmos.circle(Isometry3d::new(tf.translation().with_y(0.05), flat), 2.0, Color::srgb(0.2,1.0,0.2)); }
    gizmos.line(a, b, Color::WHITE);
    gizmos.rect(Isometry3d::new(p, flat), Vec2::splat(1.0), Color::srgb(1.0, 1.0, 0.0));
    // screen-space drag box: gizmos.rect_2d(Isometry2d, size, color) — needs a 2D camera to render
}
```
- **`bevy::ui::widget::Button` and `bevy::ui::Interaction` are deprecated in 0.20.** Both are still in the prelude, with `#[expect(deprecated)]`.
- Replace them with `bevy::ui_widgets::Button`, which is headless and emits `Activate` when released. Style it with `Hovered` (from `bevy_picking`; insert it explicitly) and `Pressed` (from `bevy_ui`). Disable it with `InteractionDisabled`.
- `TextFont.font_size` is a `FontSize` enum (`Px`, `Rem`, `Vw`, ...). The default changed to `FontSize::Rem(1.0)` in 0.20.
- `TextFont.font` is a `FontSource`; use `handle.into()`.
- `px()` and `percent()` are helper functions. `Val::Em` and `Val::Rem` are new.
- `BorderRadius` fields are `CornerRadius`.
- Gizmo methods exist with these signatures: `line`, `line_2d`, `ray`, `linestrip`, `arrow`, `rect(Isometry3d, Vec2, color)`, `rect_2d(Isometry2d, ..)`, `circle`, `sphere`, `grid`, `axes`, `cuboid`/`aabb`.

### 2.9 Navigation and pathfinding — VERIFIED (absent)
- No bevy 0.20 crate contains navmesh, A* or pathfinding code. A grep of all `bevy_*-0.20.0/src` came up empty. Use a third-party crate (§5).
- The `pathfinding` crate's `astar` is VERIFIED (check) in `probe3p`.

---

## 3. Breaking changes from 0.18 to 0.20 that matter here

**0.18 → 0.19**
- **Resources are components.** `#[derive(Resource)]` now implements `Component`, so you can't derive both on one type. Broad queries such as `Query<EntityMut>` conflict with resources; add `Without<IsResource>`. Non-send renames: `insert_non_send_resource` → `insert_non_send`.
- **Scenes:** `bevy_scene` was renamed to `bevy_world_serialization` (`SceneRoot` → `WorldAssetRoot`, and so on). The name `bevy_scene` now means BSN. glTF material labels changed: `#MaterialN` gives `GltfMaterial`; `/std` gives `StandardMaterial`.
- **Lights:** `shadows_enabled` → `shadow_maps_enabled` on Directional, Point and Spot lights. New `contact_shadows_enabled`.
- **Lifecycle:** `Replace` → `Discard` and `on_replace` → `on_discard`. Lifecycle triggers carry the old and new archetypes.
- **`Command` trait:** it now uses `type Out` (`impl Command for X { type Out = Result; ... }`).
- **Custom `SystemParam`:** validation moved into `get_param`, which returns a `Result`. `SystemState::get` also returns a `Result`.
- **Text moved to Parley:**
  - `TextFont { font: h.into(), font_size: FontSize::Px(..) }`.
  - `TextLayout::new_with_justify` → `TextLayout::justify`.
  - System font discovery needs the `system_font_discovery` feature.
- **Assets:**
  - Advanced loading uses `load_builder()`; `NestedLoader` → `NestedLoadBuilder`.
  - `Assets::get_mut` returns `AssetMut` and only fires `Modified` when the asset is actually mutated.
  - `Reader` must implement `seekable()`.
- **States:** `set()` re-triggers `DespawnOnExit`/`DespawnOnEnter` even when the state does not change.
- **Plugins:**
  - `UiWidgetsPlugins` and `InputDispatchPlugin` joined `DefaultPlugins`; remove any manual adds.
  - `InputFocus` fields are private (`get`/`set`/`clear`).
  - `WindowPlugin` exit systems moved to `Last`.
- **Features:** `audio` and `ui` are no longer implied by `3d`. The `default` feature set still includes 2d, 3d, ui and audio.
- **Renames:** `DefaultErrorHandler` → `FallbackErrorHandler`; `Hdr` moved to `bevy_camera`; `Atmosphere` is its own entity; occlusion culling is out of experimental (`bevy::render::occlusion_culling`).
- `rand` was bumped to 0.10, where `RngCore` → `Rng` and `Rng` → `RngExt`. This matters if you share rand with bevy.

**0.19 → 0.20**
- **Observers:** `On<Add, T>` → `On<Add<T>>` for all lifecycle events. `IntoObserverSystem` has three generics.
- **Picking:** `On<Pointer<Click>>` → `On<PointerClick>`, with fields `pointer.id` and `pointer.position`. There is a `PointerEvent` trait for generic handlers. Cursor code moved to `bevy_picking::cursor`.
- **Queries:** `iter_many` and the other many-entity iterators yield `Result`; use `.matched()` for the old behavior. `Some Entity::PLACEHOLDER` values became `Option<Entity>`.
- **Exclusive systems:** unified with regular systems, so `&mut World` can appear anywhere in the parameter list.
- **Weak ordering:** some built-in schedule sets are now ordered weakly. If you relied on implicit set ordering without a data conflict, add an explicit `.after()`.
- **States:** `NextState::set_if_neq` → `set_if_different`. The old name is deprecated.
- **UI:**
  - `Button` and `Interaction` are deprecated in favor of `ui_widgets::Button` plus `Activate`, `Hovered` and `Pressed`.
  - Default font size is `Rem(1)`; add `Val::Em`/`Rem`.
  - `BorderRadius` uses `CornerRadius`.
  - UI rendering is retained, which affects only custom UI render code.
- **Shaders:** WGSL with naga_oil became **WESL**. Rename `.wgsl` to `.wesl`; `#import` becomes `import ...;` and `#ifdef` becomes `@if`. GLSL was removed. This affects any custom shader (fog of war, terrain, selection outlines).
- **Rendering:**
  - `Tonemapping` moved to `bevy::render::view`; `Tonemapping::None` → `Linear`.
  - `ScreenSpaceTransmission` is opt-in.
  - `MeshAabb::compute_aabb` → `get_aabb`.
  - `MeshTag(x)` → `MeshTag::new(x)`.
- **Math:** `bevy_shape` and `bevy_curve` split out of `bevy_math`. They are still in the prelude; the `curve` feature is gone.
- **New and useful:**
  - `PanOrbitCamera` behind a feature flag.
  - Batched `despawn_all<F>`.
  - Caught panics routed to `FallbackErrorHandler`.
  - `#[component(summary_tick)]` for per-column change ticks.
  - `RenderDebugOverlay` in `DefaultPlugins` (disabled by default).
  - The BSN `Ready` event.

---

## 4. Not verified, or verified only partly

- **Interactive input was not exercised.** No clicks, key presses or scrolling were sent to the running app, so `PointerClick` observers, `Activate` on the button, `Hovered`/`Pressed` styling, edge pan, zoom and rotate compile but were not seen working.
- **glTF loading with a real model** was not exercised; the path resolved and logged `Path not found` because no asset exists. The `#Material0/std` label behavior comes from the migration guide only.
- **Deprecated widgets:** I did not check whether the deprecated `bevy_ui` `Button`/`Interaction` still work at runtime. Avoid them.
- **Migration-guide details I did not grep in source** came through a summarizing web fetch, so treat their fine details as UNVERIFIED. These include `load_builder()` chain names, `Command { type Out }`, WESL syntax, `Tonemapping` paths, `Val::Em`/`Rem`, `CornerRadius`, `chain_weak` and `despawn_all`.
- **Items grepped in source and compiled:** `On<Add<T>>`, `Discard`, `PointerClick` fields, `shadow_maps_enabled`, `WorldAssetRoot`, the `AssetLoader` signature, `set_if_different`, `iter_many(..).matched()`, `ui_widgets::Button`/`Activate`, `Hovered`/`Pressed`, `GlobalAmbientLight` and `QuerySingleError`.
- **`camera.world_to_viewport`** for box select was not probed.
- **`MessageReader<PointerClick>`:** the derive exists, but I did not check that picking writes these messages.
- **MSRV:** building on 1.96.1 with `--ignore-rust-version` worked but is unsupported. Upgrade the toolchain.

---

## 5. Third-party crates (crates.io, checked 2026-10-23)

"bevy dep" is the requirement declared by the latest version on crates.io. In `probe3p`, every crate marked "yes" resolved against one copy of bevy 0.20.0 with no duplicates in `cargo tree -d`, and the crate compiled (`cargo check`).

| Purpose | Crate | Latest | bevy dep | 0.20-ready | Notes |
|---|---|---|---|---|---|
| Physics | `avian3d` | 0.8.0 (2026-10-20) | `bevy ^0.20` | **yes** (used in probe) | ECS-native. `PhysicsPlugins::default()`, `RigidBody`, `Collider::cuboid` |
| Physics | `bevy_rapier3d` | 0.37.0 (2026-10-10) | `bevy ^0.20` | **yes** (compiled) | Alternative to avian3d |
| Spatial index | `bevy_spatial` | 0.11.0 (2025-05) | `bevy ^0.16` | **no** | Stale. Use avian's spatial queries, or `rstar` 0.13.0 / a custom grid |
| Debug UI | `bevy_egui` | 0.43.1 (2026-10-10) | `bevy_app/ecs/asset ^0.20.0` | **yes** (used) | `EguiPlugin::default()`; systems run in the `EguiPrimaryContextPass` schedule; `contexts.ctx_mut()?` |
| Inspector | `bevy-inspector-egui` | 0.38.0 | `bevy_app/ecs/asset ^0.20.0`, `bevy_egui ^0.43.0` | yes (per crates.io; not compiled) | |
| RON/JSON/etc. loader | `bevy_common_assets` | 0.18.0 (2026-10-09) | `bevy_app/asset ^0.20` | **yes** (used, feature `ron`) | `RonAssetPlugin::<T>::new(&["unit.ron"])` |
| Asset loading states | `bevy_asset_loader` | 0.28.1 | `^0.20.0` | **yes** (compiled) | Loading screens and asset collections |
| Flowfield pathing | `bevy_flowfield_tiles_plugin` | 0.16.0 (2026-10-22) | `bevy ^0.20` | **yes** (compiled) | Good fit for RTS group movement |
| Grid A*/Dijkstra | `pathfinding` | 4.16.0 | none (engine-agnostic) | **yes** (used `astar`) | |
| Navmesh (polyanya) | `vleue_navigator` | 0.16.0 (2026-08) | `bevy ^0.19.1` | **no** (0.19 only) | Core `polyanya` 0.17.1 has no bevy dep, so it can be used directly |
| Navmesh + steering | `bevy_landmass` | 0.12.1 (2026-09) | `^0.19.0` | **no** | Core `landmass` 0.9.3 has no bevy dep |
| Hierarchical grid pathfinding | `bevy_northstar` | 0.7.0 (2026-07) | `bevy ^0.19` | **no** | |
| Navmesh generation | `oxidized_navigation` | 0.12.0 (2024-12) | `bevy ^0.15` | **no** | Stale |
| Input mapping | `leafwing-input-manager` | 0.22.0 | `bevy ^0.20.0` | **yes** (compiled) | |
| RTS camera | `bevy_rts_camera` | 0.15.0 (2026-10-14) | `bevy ^0.20` | yes (per crates.io; not compiled) | |
| Outlines (selection) | `bevy_mod_outline` | 0.13.0 | `bevy ^0.19.0` | **no** | |

Verified third-party snippet (`probe3p\src\main.rs`, passes `cargo check`):
```rust
use avian3d::prelude::*;
use bevy::camera_controller::pan_orbit_camera::prelude::*;   // bevy feature "pan_orbit_camera"
use bevy_common_assets::ron::RonAssetPlugin;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};

App::new().add_plugins((DefaultPlugins, MeshPickingPlugin,
        RonAssetPlugin::<UnitDef>::new(&["unit.ron"]), EguiPlugin::default(),
        PhysicsPlugins::default(), DefaultPanOrbitCameraPlugins))
    .add_systems(EguiPrimaryContextPass, debug_ui);

commands.spawn((RigidBody::Static, Collider::cuboid(100.0, 0.1, 100.0)));
let path = pathfinding::prelude::astar(&(0i32, 0i32),
    |&(x, y)| [(x+1,y),(x-1,y),(x,y+1),(x,y-1)].map(|p| (p, 1u32)),
    |&(x, y)| ((5 - x).abs() + (5 - y).abs()) as u32, |&p| p == (5, 5));

fn debug_ui(mut contexts: EguiContexts) -> Result {
    egui::Window::new("Units").show(contexts.ctx_mut()?, |ui| { ui.label("..."); });
    Ok(())
}
```

## Probe: dynamic_linking

Result: **works** (headless). `bevy/dynamic_linking` links and runs with sim crates that depend only on `bevy_app`/`bevy_ecs` (`fl_sim`); no duplicate-crate, link or type-mismatch error (Windows, MSVC, rust-lld, Rust 1.99.1, Bevy 0.20.0).

`probes/dynlink` builds a sim `App` from the `core_mod` fixture and a `bevy` `App` with `MinimalPlugins`, then crosses the boundary three ways, asserting `SimClock` = `Tick(10)` after 10 steps:
(a) an `fl_sim` resource (`SimClock`) inserted into the `bevy` app and read by a `bevy::prelude` system;
(b) a `bevy::prelude` `Schedule` run against the sim app's `World`;
(c) the sim `World` moved into a resource of the `bevy` app and stepped with `run_schedule(SimTick)` from a `bevy` system (what the `fl_game` runner will do).

- `cargo run -p probe_dynlink` (static): `dynlink probe: OK sim_ticks=10 app_updates=10`
- `cargo run -p probe_dynlink --features dynamic`: same OK line (all three crossings pass)
- `cargo build -p fl_game --features dynamic`: Finished (17m38s cold)

Decision: dev runs may use `cargo run -p fl_game --features dynamic` (feature `dynamic = ["bevy/dynamic_linking"]` in `fl_game`). Not for release or CI. Not tested: a windowed/rendering run of `fl_game --features dynamic` (build and headless crossings only).

## Probe: bevy_remote

Result: **works** (Windows, NVIDIA RTX 1000 Ada, Vulkan, window hidden via `visible: false`). `cargo run -p probe_remote` prints `remote probe: OK ping=42 screenshot=11217 (cube px 8649 of 65536)`, exit 0. Run four times (continuous and `default` variants, two each). Not run: a visible window that is focused or unfocused. Labels: **tested** = exercised by the probe; **source-read, untested** = read from the 0.20 / bevy_brp_extras 0.23 sources only.

`probes/remote` also needs `serde_json` as a dependency (handler signature and results use `serde_json::Value`; neither `bevy` nor `bevy_remote` re-exports it). The `bevy` feature `bevy_remote` brings in the HTTP transport (`http` is a `bevy_remote` default feature).

**Plugins and port (names as compiled)**
- Tested: `bevy::remote::RemotePlugin::default().with_method_main("fl.ping", handler)` registers an instant method on the main app; `rpc.discover` lists it.
- Tested: `.with_watching_method_main("fl.watch+watch", handler)` registers a watching method. The name **must end in `+watch`** (as the built-ins do, e.g. `world.get_components+watch`): the HTTP layer streams as `text/event-stream` only when the request method contains `+watch`, and the method is looked up under that exact name. Registered as plain `fl.watch`, the request `fl.watch+watch` fails with -32601. A watching handler is `fn(In<Option<Value>>, ..) -> BrpResult<Option<Value>>`; `Ok(Some(v))` emits an event (first event observed: `data: {"jsonrpc":"2.0","id":1,"result":{"frames":4}}`), `Ok(None)` emits nothing.
- Instant handler type (tested): `fn(In<Option<serde_json::Value>>, <any system params>) -> bevy::remote::BrpResult`. Errors are `BrpError { code, message, data }`, codes in `bevy::remote::error_codes`.
- Source-read, untested: `_render` variants (`with_method_render`, `with_watching_method_render`) target the render sub-app.
- Tested: `bevy::remote::http::RemoteHttpPlugin::default().with_port(15799)`, used via `BrpExtrasPlugin::with_http_plugin(..)`. Source-read, untested: `.with_address`, `.with_headers`; a second server for the render sub-app on default render port 15703 and no setter for it.
- Wire format (tested): HTTP POST `/` with a JSON-RPC 2.0 body. The reply uses `Transfer-Encoding: chunked`, so a raw-socket client must de-chunk (the probe takes first `{` to last `}`).

**Blocking inside a method**
- Tested: an instant handler blocking on a crossbeam `recv_timeout(2 s)` for a worker reply works: `fl.ping {n: 41}` returned `42`.
- Tested: the handler ran on the thread that called `main()` (thread-id comparison). Source-read, untested: it runs in the exclusive `process_remote_requests` system in `RemoteLast`.
- Tested: it stalls the main world. A worker delay of 700 ms gave a call time of 705 to 857 ms and a gap of about 705 ms between main-world `Update` frames (one run showed 145 ms because frame times were irregular at that point). Block only as long as needed. Render-thread behaviour during the stall is not measured.
- Tested: a reply later than the timeout yields JSON-RPC error `-32603` (`timed out waiting on receive operation`), not a hang.
- Tested: the late reply stays in the channel. A `fl.ping {n: 10}` sent right after the timed-out call returned `2` (the stale reply to the earlier call), not `11`. A real bridge must tag replies with a request id and drop stale ones.

**bevy_brp_extras 0.23**
- `use bevy_brp_extras::{AgentTool, AppAgentToolExt, BrpExtrasPlugin};`
- Tested: `app.register_agent_tool(AgentTool::new("fl.ping", "fl.ping", "description"))` returns `&mut App`. Listing method `brp_extras/agent_tools` (no params) returned `{"version":1,"tools":[{"name":"fl.ping","method":"fl.ping","description":"..."}]}`.
- Tested: `BrpExtrasPlugin::with_http_plugin(RemoteHttpPlugin::default().with_port(15799))` added after a `RemotePlugin` carrying the `fl.*` methods keeps those methods.
- Source-read, untested: `.params_schema(schemars::Schema)` / `.result_schema(..)`; panic on an invalid or duplicate name (1 to 128 of ASCII letters, digits, `.`, `_`, `-`); the catalog request rejecting everything if a backing method is missing or is a watching method (so never register a `+watch` method as an agent tool); `BrpExtrasPlugin::with_port(p)` and `new()` (env `BRP_EXTRAS_PORT`, default 15702).

**Offscreen screenshot recipe (tested, window hidden; no focused or unfocused-visible run)**
```rust
let image = images.add(Image::new_target_texture(256, 256, TextureFormat::Rgba8UnormSrgb, None));
commands.spawn((Camera3d::default(), RenderTarget::Image(image.clone().into()), Transform::..));
// later, e.g. from a BRP method via Commands:
commands.spawn(Screenshot::image(image)).observe(save_to_disk("target/probe-remote.png"));
```
- `RenderTarget` is a component (`bevy::camera::RenderTarget`), not a field of `Camera`. `Screenshot` and `save_to_disk` are in `bevy::render::view::screenshot`; a second observer on `On<ScreenshotCaptured>` gets the `Image` (`.data`) for pixel checks.
- Result: 256x256 PNG of 11217 bytes. The probe asserts that some but not all pixels differ from the corner (clear-colour) pixel: 8649 of 65536, so the lit cube was rendered.
- A BRP method can spawn the `Screenshot` entity through `Commands`; it is applied right after the handler runs.

**Continuous update when unfocused**
- Source-read, untested for the unfocused case: `app.insert_resource(bevy::winit::WinitSettings::continuous())` sets `focused_mode` and `unfocused_mode` to `Continuous`; the default `WinitSettings::game()` uses `reactive_low_power` (60 Hz) when unfocused.
- Tested (hidden window, steady state after the screenshot, 4 s): idle update rate was 172 and 180 frames/s with `continuous()`, and 187 and 192 frames/s with the default settings. So a hidden window does not throttle under the default settings in this setup (offscreen camera present); the difference between the two modes for a visible but unfocused window remains unmeasured.
- Window hidden via `WindowPlugin { primary_window: Some(Window { visible: false, .. }), .. }`. The probe takes an optional first argument `default` to skip `WinitSettings::continuous()`.
