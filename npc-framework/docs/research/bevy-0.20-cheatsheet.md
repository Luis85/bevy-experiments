# Bevy 0.20 cheat-sheet (verified 2026-10-23)

Sources: crates.io, docs.rs bevy 0.20, bevyengine/bevy examples at v0.20.0, https://bevy.org/news/bevy-0-20/, https://bevy.org/learn/migration-guides/0-19-to-0-20/

## Version facts
- bevy 0.20.0 published 2026-10-08 (latest 0.20.x). Previous: 0.19.1.
- **MSRV `rust-version = "1.97.1"`, edition 2024.** Local rustc 1.96.1 is too old → `rustup update`.
- Default features: `["2d","3d","ui","audio"]`.

## App, schedules, fixed rate, sets, states
```rust
App::new()
  .add_plugins(DefaultPlugins)
  .insert_resource(Time::<Fixed>::from_hz(20.0)) // default 64 Hz
  .configure_sets(Update, (MySystems::A, MySystems::B).chain())
  .add_systems(Startup, setup)
  .add_systems(FixedUpdate, sim_tick)
  .init_state::<AppState>()
  .run();
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)] enum MySystems { A, B }
```
- New in 0.20: `chain_weak()`, `before_weak()`, `after_weak()`; several built-in schedules order sets weakly.
- `NextState::set_if_neq` → `set_if_different`.
- `ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, .. }` for strict schedules. 0.20 adds `ScheduleBuildSettings::shuffle_seed` (feature `debug`) to shake out missing ordering.
- Custom schedules: `#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)] struct SimTick;` then `app.init_schedule(SimTick)` / `world.run_schedule(SimTick)`.

## Camera, lights, ground
```rust
use bevy::{light::CascadeShadowConfigBuilder, image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor}, math::Affine2, prelude::*};
app.insert_resource(GlobalAmbientLight { color: Color::WHITE, brightness: 2000., ..default() });
commands.spawn((Camera3d::default(), Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y)));
commands.spawn((
  DirectionalLight { illuminance: light_consts::lux::OVERCAST_DAY, shadow_maps_enabled: true, ..default() },
  CascadeShadowConfigBuilder { first_cascade_far_bound: 4.0, maximum_distance: 10.0, ..default() }.build(),
  Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 1.0, -PI / 4.)),
));
let tex = asset_server.load_builder()
  .with_settings(|s: &mut _| { *s = ImageLoaderSettings {
      sampler: ImageSampler::Descriptor(ImageSamplerDescriptor {
          address_mode_u: ImageAddressMode::Repeat, address_mode_v: ImageAddressMode::Repeat, ..default() }),
      ..default() } })
  .load("textures/grass.jpg");
commands.spawn((
  Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
  MeshMaterial3d(materials.add(StandardMaterial { base_color_texture: Some(tex), uv_transform: Affine2::from_scale(Vec2::splat(25.)), ..default() })),
));
```
- Light field is `shadow_maps_enabled` (renamed in 0.19). `load_with_settings` → `load_builder()` (0.19).
- Plain `commands.spawn` still works alongside the new BSN `bsn_list!{}` form.

## glTF scene + animation
`SceneRoot` → **`WorldAssetRoot`**, `SceneInstanceReady` → **`WorldInstanceReady`** (0.19; module `bevy::world_serialization`).
```rust
use bevy::{prelude::*, world_serialization::WorldInstanceReady};
commands.spawn(WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(PATH))))
        .observe(play_when_ready);
fn play_when_ready(ready: On<WorldInstanceReady>, mut commands: Commands, children: Query<&Children>,
                   mut players: Query<&mut AnimationPlayer>) {
  for child in children.iter_descendants(ready.entity) {
    if let Ok(mut player) = players.get_mut(child) {
      let mut transitions = AnimationTransitions::new();
      transitions.play(&mut player, idx, Duration::ZERO).repeat();
      commands.entity(child).insert((AnimationGraphHandle(graph.clone()), transitions));
    }
  }
}
// Multiple clips by name (load Handle<Gltf>, wait until loaded):
let (graph, nodes) = AnimationGraph::from_clips([gltf.named_animations["Idle"].clone(), gltf.named_animations["Walking_A"].clone()]);
transitions.play(&mut player, nodes[1], Duration::from_millis(250)).repeat(); // cross-fade
```
- Per-entity observer + `iter_descendants` (animated_mesh.rs) is the multi-NPC pattern; don't use `Single<&mut AnimationPlayer>`.
- glTF materials load as `GltfMaterial`; `#Material0/std` label for a `StandardMaterial`.
- `GltfAssetLabel::Animation(i)` also works by index.

## ECS basics
```rust
#[derive(Component)] #[require(Transform)] struct Npc;
#[derive(Component)] #[component(storage = "SparseSet")] struct Stunned;
#[derive(Event)] struct Alarm { pos: Vec2 }
#[derive(EntityEvent)] struct Explode { entity: Entity }
app.add_observer(|e: On<Alarm>| {});
app.add_observer(|add: On<Add<Npc>>| { let _ = add.entity; });   // 0.20 form
#[derive(Message)] struct DealDamage { amount: i32 }               // buffered, formerly Event
app.add_message::<DealDamage>();
fn w(mut w: MessageWriter<DealDamage>) { w.write(DealDamage { amount: 10 }); }
fn r(mut r: MessageReader<DealDamage>) { for m in r.read() {} }
```
- Lifecycle: `Add`, `Insert`, `Discard` (was Replace), `Remove`.
- Resources are components (0.19): a type can't derive both; `Query<EntityMut>` conflicts with `Res<T>` unless filtered.
- Deterministic iteration: `query.iter().sort::<&SimId>()` (also `sort_by`, `sort_by_key`).

## Determinism
- FixedUpdate runs 0..n times per frame. Interpolate: `fixed_time.overstep_fraction()` (examples/movement/physics_in_fixed_timestep.rs).
- `bevy_transform_interpolation = "0.6"` supports 0.20 (optional).
- rand 0.10: `Rng`→`RngExt`, `RngCore`→`Rng`. Bevy uses `chacha20 = { version = "0.10", default-features = false, features = ["rng"] }`. `bevy_rand 0.16` supports 0.20.
- Query order not guaranteed; never rely on `Entity` order or `par_iter` reductions.

## Gizmos / debug text
```rust
fn draw(mut gizmos: Gizmos) {
  gizmos.line(a, b, WHITE); gizmos.arrow(a, b, YELLOW); gizmos.sphere(p, 1.0, PURPLE);
  gizmos.circle(Quat::from_rotation_arc(Vec3::Z, Vec3::Y), 3., BLACK); // Into<Isometry3d>
}
commands.spawn((Text::new("debug"), TextFont { font_size: FontSize::Px(16.), ..default() },
  Node { position_type: PositionType::Absolute, top: px(12), left: px(12), ..default() }));
```
- `TextFont` uses `FontSize::Px(..)` (default `FontSize::Rem(1.)`). Unverified: `Text` inner `String` access via `.0`.

## Windows build
```toml
[profile.dev]
opt-level = 1
[profile.dev.package."*"]
opt-level = 3
```
- `cargo run --features bevy/dynamic_linking` for fast iteration (requires the opt-level profile above on Windows).
- Optional LLD: `.cargo/config.toml` → `[target.x86_64-pc-windows-msvc] linker = "rust-lld.exe"`.

## Breaking changes that bite (vs 0.14–0.16)
- 0.17: `Event` split → `Message`/`MessageWriter`/`MessageReader`; observers take `On<E>`, `event.entity`; `StateScoped` → `DespawnOnExit`; `*Set` → `*Systems`; `Camera{hdr}` → `Hdr`; crates `bevy::light/camera/mesh/image`; Timer `finished()` → `is_finished()`.
- 0.19: `SceneRoot` → `WorldAssetRoot`; `shadows_enabled` → `shadow_maps_enabled`; `Replace` → `Discard`; resources as components; `TextFont`/`FontSize`, Parley text; `ui` not implied by `3d`; `load_builder()`; rand 0.10.
- 0.20: `On<Add, A>` → `On<Add<A>>`; `On<Pointer<Press>>` → `On<PointerPress>`; `Tonemapping::None` is true passthrough; `BorderRadius` → `CornerRadius::circular(px(..))`; `ui::Interaction`/`Button` deprecated for `ui_widgets`; system panics go through `FallbackErrorHandler`.
