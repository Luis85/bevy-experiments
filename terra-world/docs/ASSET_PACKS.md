# Validated actor asset packs

Terra's optional presentation importer loads a declared set of **self-contained
GLB 2.0 models** over the existing authoritative actors. The demo installs an
original five-species woodland pack by default: deer, boar, hare, fox and ibex.
Each model includes `idle`, `walk`, `run`, `feed` and `drink` clips. The character
creator and vehicle presentation remain in place because this pack binds only
wildlife species.

An optional original Comet transport example now binds the existing compact
vehicle model specifically. The direct `crates/terra-bevy/assets/transport-demo.pack.json` and
the editable composed recipe include the Comet plus all five original wildlife
models, without changing the default pack. See the
[transport handoff](ASSET_AUTHORING.md#preview-the-original-comet-transport-slice)
for native use, original-content attribution and remaining production limits.

An optional original Fieldhand human now reads the same saved body, skin, hair
and outfit choices. The composed `character-demo.pack.json` adds its walking,
running and known-manual-driver seated presentation to a new Comet driver-cabin
variant and unchanged wildlife sources. The original Comet example is retained. See [the human handoff](ASSET_AUTHORING.md#preview-the-original-customized-human-and-driver-handoff)
for its material roles, fallback, occupancy boundary and reproducible recipe.

This is a bounded production handoff, with a deliberately supported format
profile. It is not an unrestricted glTF renderer, a full animation state-machine
authoring tool, a humanoid retargeter or a complete commercial art library.

## Author and validate a pack

1. Model in metres, with feet on local Y=0, +Y up and local -Z forward. Apply
   object transforms in the authoring tool. A binding may correct uniform scale,
   yaw and ground offset; it cannot change collision dimensions or authority.
2. Export **one GLB, one scene**, using uniquely named nodes and local translation,
   quaternion rotation and scale. Include all nodes in that scene. Remove cameras
   and lights. Triangulate meshes and bake animation to linear or step keys.
3. Keep textures embedded as PNG images. Export standard glTF PBR materials.
   External files, remote URLs, data URIs and glTF extensions are rejected rather
   than fetched or silently ignored.
4. Export named clips. An animated binding supplies names for idle, walk, run,
   feed and drink; different states may intentionally reuse one clip. Set the
   physical ground speeds represented by the walk and run clips.
5. Use the [authoring recipe workflow](ASSET_AUTHORING.md) to inspect exports and
   build a new content-identified pack. It computes each GLB's SHA-256, preserves
   explicit binding policies and provenance, and validates before writing. SDK
   integrations may still produce the runtime manifest below directly; changed
   model bytes require new matching digests.
6. Run the validator from the SDK, then inspect it in the demo. A malformed pack
   fails preflight with its model ID and reason. A later renderer/dependency
   failure keeps the procedural actor available.

The bundled authoring script uses only Python's standard library:

```sh
python3 crates/terra-bevy/assets/wildlife-pack/generate.py
cargo run -p terra-bevy --no-default-features --bin terra-assets -- validate crates/terra-bevy/assets/wildlife-pack/pack.json
```

It regenerates all five original GLBs and their exact manifest hashes. The
script, meshes and animation are licensed MIT OR Apache-2.0. No downloaded
animal models or third-party art are incorporated.

`terra-assets validate` runs without a graphics device and prints a JSON
acceptance report containing the pack identity and every model budget. Any
rejection exits nonzero, so it can gate an art repository's CI. Play a supplied
pack with `cargo run -p terra-demo -- --play --asset-pack path/to/pack.json`.
Use `--procedural-models` to inspect the unchanged procedural fallback.

`terra-assets inspect MODEL.glb` lists accepted node/clip names and complete model
budgets. `terra-assets build RECIPE.json NEW_DIRECTORY` snapshots local exports,
checks the same importer and writes a new pack with content-named GLBs and an
authoring report. Existing destinations and symlink source/output paths are
rejected. See [Asset authoring](ASSET_AUTHORING.md) for a complete included example,
error recovery and the limits of headless acceptance.

## Manifest example

`sha256` is the lowercase 64-digit digest of the complete file; the example
placeholder must be replaced with the actual digest.

```json
{
  "schema_version": 1,
  "name": "My wildlife",
  "author": "My studio",
  "license": "My asset license",
  "models": [
    { "id": "deer", "file": "deer.glb", "sha256": "ACTUAL_SHA256", "scene": 0 }
  ],
  "bindings": [
    {
      "target": { "type": "species", "value": "deer" },
      "model": "deer",
      "scale": 1.0,
      "ground_offset_m": 0.0,
      "yaw_offset_degrees": 0.0,
      "animation": {
        "idle": "idle", "walk": "walk", "run": "run",
        "feed": "feed", "drink": "drink",
        "walk_speed_m_s": 1.4, "run_speed_m_s": 4.4,
        "blend_seconds": 0.22
      }
    }
  ]
}
```

A binding target is `species` with `deer`, `boar`, `hare`, `fox` or `ibex`; or
`kind` with `human`, `animal`, `creature`, `vehicle`, `ship`, `helicopter`,
`balloon` or `airplane`; or `vehicle_model` with `compact`, `pickup`, `taxi`,
`bus`, `box_truck`, `semi`, `tow_truck`, `ambulance` or `fire_engine`.
Species overrides take precedence over generic animal appearance. Vehicle-model
overrides take precedence over the generic vehicle binding only on road-vehicle
actors with that actual catalog model; owner-private state is unnecessary when
the public vehicle appearance is available. Unknown models are rejected. A
missing specific binding can use the generic kind, then ordinary procedural
fallback. `animation: null` installs a static model. Every target appears at
most once in a pack. Sharing a model between targets is supported: each binding
retains its own clip choices, gait speeds, blend duration and presentation
transform.

The new `vehicle_model` target and optional `animation.vehicle_motion` require
Terra 0.15 or later. Older readers reject these unsupported inputs. Schema 1 and
canonical serialization of all existing kind/species packs remain unchanged;
an absent optional wheel policy contributes no field and changes no identity.
The [vehicle-wheel contract](ASSET_AUTHORING.md#declare-actual-vehicle-wheel-motion)
defines its bounded nodes, radius, hierarchy, axes and conflict rules. Optional
`animation.human` and binding `driver_seat` require the 0.16 reader; absent fields
also preserve old pack identities. Their exact contract is in the human handoff
section of that guide.

## SDK integration

```rust,no_run
use bevy::prelude::*;
use terra_bevy::{
    asset_pack::ValidatedAssetPack,
    render::{AssetPackPlugin, TerraRenderPlugin},
};

fn build() -> Result<App, String> {
    let pack = ValidatedAssetPack::read("art/wildlife/pack.json")?;
    println!("Validated {}: {}", pack.name(), pack.identity());
    let mut app = App::new();
    // Asset sources must be registered before Bevy constructs AssetPlugin.
    app.add_plugins(AssetPackPlugin::new(pack));
    app.add_plugins(DefaultPlugins);
    // Add the world/game plugins and TerraRenderPlugin as usual.
    Ok(app)
}
```

`ValidatedAssetPack::from_bytes` supports embedded assets, with the same importer
and budgets. `render::bundled_wildlife_pack()` demonstrates it in the reference
game. One `AssetPackPlugin` installs one complete pack per app. Compose bindings
in one manifest before importing. A game's direct `ActorSceneOverrides` API
remains available for custom render pipelines.

The plugin stores a private snapshot of validated bytes in an in-memory Bevy
asset source, named with the pack digest. Models are addressed by their own
content digest. No file watcher is attached and subsequent edits to disk cannot
change those installed bytes. The pack digest also includes binding policy and
metadata, so a changed animation policy produces a different presentation pack
identity even when its GLB bytes are unchanged.

## Runtime behavior and recovery

The existing procedural visual remains until the model **and its dependencies**
are loaded, a scene instance is ready, and its named animation hierarchy matches
the validated model. Missing files and models without a reachable drawable mesh
fail pack preflight; native `--asset-pack` exits before opening a window. Later
renderer, dependency or hierarchy failures retain the procedural visual. Custom
scene overrides also retain fallback for meshless scenes. Removing an override
restores that same actor's fallback.

Imported visible descendants inherit the actor's visibility. First-person
possession and disabled actor layers therefore hide the entire imported
exterior. The pack cannot move the geographic root or turn an unowned actor into
an owned one. It never changes character customization, saved world authority,
terrain identity, inventory, collision support or multiplayer admission.

Locomotion is selected from actual authoritative horizontal velocity. A fleeing
but blocked animal does not run in place. Walk/run playback is scaled to the
binding's reference speed, with bounded playback rates. At rest, ecology selects
feeding or drinking; other behaviors use idle. Rendering blends local transforms
between clips and maintains rest values for unanimated properties. Skin-joint
node transforms can use this same path; morph-target animation and root-motion
authority are outside this profile.

A declared vehicle wheel rig overrides its preview rotation channels using
signed actual horizontal velocity, a binding-scaled radius and bounded steering.
It retains rolling phase through stops and reverse, independent of gait playback
floors/caps. Manual steering may change at zero speed; other known vehicles use
admitted yaw and the catalog wheelbase. Both clip and rig state freeze during
pause. Model geometry and rig state never write position, ownership, fuel,
vehicle dynamics, passenger occupancy or saved character/paint data.

The native Garage explains that a selected imported/override vehicle has a fixed
livery and disables its paid paint action. Dispatch rechecks the current resolved
appearance and displayed vehicle so stale clicks cannot charge for an invisible
change. This is a local presentation rule: procedural vehicles still support
paint, and the authoritative paint command and stored paint remain available to
other clients or appearances that can display them.

Clip phase and blend weight use the full virtual-time delta, including slow
render frames. Sampling remains constant work rather than replaying each missed
frame; a frame longer than 100 ms does not make the animation lag physical
movement. All applied rotation keys are normalized, including STEP keys and
shorter channels holding their final value.

Pausing virtual game time or authoritative simulation freezes both clip time
and transition weight. Clip time and transition weight do not advance while paused. An actual
authoritative occupancy change or a restored seated avatar selects its correct
discrete human posture immediately; it does not simulate a boarding movement. The calendar's time multiplier is independent of physical movement and
does not accelerate the walking animation. Network clients animate their
replicated actor state; opening the network menu does not pause the server.

`AssetPackDiagnostics` exposes pack name/identity, model count, ready/loading/
rejected instance counts, failed model loads, procedural fallback instances and
a plain recovery error for developer diagnostics. Fallback counts include an
actor whose model failed before a supplied scene could be instantiated.
These counters describe this renderer's instances, not the whole world.

## Aggregate scene admission and procedural degradation

Validated packs load models on demand. Installing a binding keeps its immutable
source bytes and validated metadata; it does not decode/upload every model.
`render::ActorSceneBudget` follows the current `RenderQualitySettings` unless a
game supplies explicit `ActorSceneLimits`:

| Reservation | Performance | Balanced | High |
| --- | ---: | ---: | ---: |
| Imported actor instances | 64 | 128 | 256 |
| Renderer vertices / triangles across instances | 400,000 / 200,000 | 800,000 / 400,000 | 1,600,000 / 800,000 |
| Instantiated mesh primitives | 2,048 | 4,096 | 8,192 |
| Animated nodes / maximum active clip channels | 4,096 / 1,024 | 8,192 / 2,048 | 16,384 / 4,096 |
| Texture-slot pixels across retained models | 8,388,608 | 16,777,216 | 33,554,432 |
| Estimated presentation payload | 128 MiB | 256 MiB | 512 MiB |
| Concurrent model loads | 1 | 2 | 2 |
| Entry distance from focus | 650 m | 1,200 m | 2,000 m |

These are deterministic SDK defaults, not measured frame-rate or hardware
certification thresholds. Hidden actors and actors outside the distance policy
keep procedural presentation. Eligibility uses root visibility and focus
distance, not a changing frustum result. Among eligible bodies, the active body
has first priority, then owned bodies, then other nearby bodies. Distances use
100 m bands with 25 m band hysteresis. An incumbent wins within an otherwise
equal priority/band, with body identity as the final stable tie-breaker. Existing
admission gets 150 m of distance exit hysteresis. Camera jitter therefore does
not continually recreate actors. Visibility, ownership, authority and collision
remain on the original body.

Limits reserve instantiated geometry and per-instance animation work. Channels
are the largest clip selected by that binding's policy plus any declared
rolling/steering rig nodes; shared animation data
does not incorrectly make per-actor pose updates free. Human bindings include
the seated clip in this maximum and reserve three private tint materials per
instance. A known seated driver uses the vehicle position for distance admission,
while the authoritative parked avatar position remains unchanged. A shared model's textures
and decoded model payload are charged once while retained. Texture slots count
separately even when several refer to one PNG. Indexed geometry without normals
uses the expanded vertex count that Bevy's flat-normal generation produces.
Primitive counts bound scene mesh multiplicity; they are not measured draw calls
across shadow passes, views and material batches.

Outgoing instances remain charged until the following update observes their
teardown. Outgoing model reservations remain charged while a load is unfinished
and until all supported glTF/PBR subasset labels leave the asset server, followed
by two quiet polls. A new model cannot spend that reservation early. External
handles that keep a dependency alive keep its reservation alive too. Model-load
failure preserves the procedural mesh and is not retried every frame. Restart
after fixing immutable input content. Increasing a budget makes deferred actors
eligible in the same deterministic order; lowering it initiates degradation and
reports outstanding retirement instead of pretending old resources vanished.
Installed immutable source snapshots and validated animation metadata remain for
the pack's lifetime. If that estimated floor alone exceeds a lowered byte limit,
draining cannot reach the new limit by evicting scenes. All new imported scenes
stay deferred until the game raises the limit or restarts with a smaller pack;
the diagnostics identify this condition explicitly.

`AssetPackDiagnostics.scene_budget` and native telemetry expose effective limits,
selected/reserved totals, eligible/deferred/hidden/distant counts, retained,
loading and retiring models, a lifetime estimated-byte peak, and draining status.
The immutable estimate includes each validated GLB/animation payload plus the
unique in-memory source snapshots, including unused installed models. A loaded
model reserves two encoded-byte copies, decoded scalar storage, 96 bytes per
renderer vertex, 12 per triangle, 24 per texture-slot pixel, 1 KiB per model node
and 4 KiB per material including the default. Each instance adds 1 KiB per node,
primitive and two roots, plus three animation transform arrays and entity IDs.
These explicit allowances include loading/retirement overlap; they **do not
measure total CPU/GPU memory**, allocator overhead, driver queues, actual GPU
reclamation or frame time. Hardware profiling remains necessary.

Direct `ActorSceneAppearance::new(handle)` overrides retain their game-owned
loading path. Without immutable validated metadata their geometry/memory is
reported as unmetered; use an asset pack for these aggregate guarantees. A game
can override complete limits without changing any saved world state:

```rust
use terra_bevy::render::{ActorSceneBudget, ActorSceneLimits, RenderQuality};
let mut limits = ActorSceneLimits::for_quality(RenderQuality::Balanced);
limits.instances = 96;
app.insert_resource(ActorSceneBudget { limits: Some(limits) });
```

## Fixed import budgets

Manifest size and model/binding counts apply to the pack. Model-resource budgets
apply per GLB, with narrower quantities such as joints per skin stated explicitly.
The aggregate GLB byte limit is 64 MiB, plus the separately capped 64 KiB manifest.

| Resource | Limit |
| --- | ---: |
| Manifest | 64 KiB |
| Models / bindings | 16 / 22 |
| Each GLB / aggregate GLB bytes | 16 MiB / 64 MiB |
| Nodes / maximum hierarchy depth | 256 / 32 |
| Meshes / materials / primitives | 128 / 64 / 256 |
| Declared mesh vertices / triangles per GLB | 100,000 / 100,000 |
| Instantiated scene vertices / triangles per GLB | 100,000 / 100,000 |
| Skins / joints per skin | 16 / 128 |
| Texture images / texture slots | 16 / 32 |
| PNG dimension / total decoded pixels | 4,096 / 16,777,216 |
| PNG decoder allocation / output buffer ceilings | 128 MiB each |
| Animation clips / total channels | 32 / 512 |
| Animation input + output keys | 120,000 |
| Runtime channel input + output keys, counting reused samplers | 120,000 |
| Clip duration | 600 seconds |
| Accessors / buffer views | 2,048 / 2,048 |
| Aggregate decoded accessor scalars, counting aliases | 4,000,000 |
| Composed hierarchy scale / local animation envelope | 1,000× / 10 km |

Animation accounting includes **every sampler in every clip**, even when it is
not referenced by a binding or channel. Model accounting includes unused meshes
and accessors too. An exporter cannot evade the import budget by leaving an
oversized clip or mesh unused. Accessor extents, alignment, indices, finite
binary floats, graph acyclicity, single parenting and complete scene coverage
are checked before renderer allocation. Aggregate accessor work is admitted
before any binary scans, so thousands of aliases cannot repeatedly decode a
large shared range. Core material colors, roughness, metallic factor, occlusion
and alpha cutoff must be within 0..1; normal-map scale is bounded to -16..16.
Every scene must contain a drawable mesh instance. Ancestor transform bounds
include rest poses and all animated keys. Animated scales retain the sign of
their rest pose across every clip so interpolation cannot cross a singular
zero-scale transform.

The validator reports declared mesh geometry as `vertices`/`triangles`, and the
sum across reachable mesh nodes as `scene_vertices`/`scene_triangles`. Reusing a
mesh at several scene nodes counts once per node in the latter budget; a compact
file cannot multiply declared geometry past the per-model scene ceiling. The
report additionally exposes `renderer_vertices`/`scene_renderer_vertices`,
including flat-normal expansion, `primitives`/`scene_primitives`, `materials`
and `texture_slot_pixels`. Aggregate runtime admission uses these renderer costs
as described above; the original per-model import ceilings still apply.

Every embedded PNG is actually decoded during preflight, including unused
images. Dimension and aggregate pixel admission precede decoding; limits are
installed before the decoder processes compressed metadata or pixels. Truncated
or corrupt image data therefore fails the headless validator rather than first
appearing as a renderer failure. Images are decoded one at a time and discarded
after validation; original compressed GLB bytes remain the immutable source.
The allocation ceilings above do not include the rest of the pack, renderer
meshes, mipmaps or GPU resources.

Skins use at most four influences per vertex: unsigned byte/short `JOINTS_0`
indices and FLOAT `WEIGHTS_0`. Joint indices must fit the referenced skin;
weights must be finite, nonnegative and sum to one. Zero-weight vertices and
additional influence sets are rejected. Bake/export normalized float weights
before importing assets using another representation.

Unsupported exports receive explicit errors: sparse accessors, cubic spline
curves, morph targets, matrix-only nodes, extra scenes, unnamed/duplicate nodes,
external resources and extensions. Bake or simplify the export, or deliberately
extend and test the importer for your game's required profile.
Extension payloads are rejected even if an exporter omits `extensionsUsed` or
`extensionsRequired`; opaque `extras` metadata remains permitted.

## Verification entry points

```sh
cargo test -p terra-bevy --no-default-features asset_pack
cargo test -p terra-bevy --no-default-features --test transport_assets
cargo test -p terra-bevy --lib render::animation
cargo test -p terra-bevy --lib render::scenes
```

The first group exercises real bundled GLB bytes, hash integrity, immutable
copies, independent shared-model binding policies, missing clips, path traversal,
invalid graph structure, out-of-bounds accessors and unused-animation budgets.
Runtime tests cover actual scene-ready fallback replacement, inherited hidden
visibility, removal/recovery, movement selection and pause-safe crossfades.
Native rendering and play evidence belong to the release validation report.
