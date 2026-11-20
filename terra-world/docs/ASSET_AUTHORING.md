# Asset authoring and handoff

The headless `terra-assets` tools turn local, self-contained GLBs into the exact
validated presentation packs used by the game. Artists keep an editable recipe;
the builder computes content hashes, validates every model and binding, and
publishes a new directory. This removes hand-editing hashes and copying model
filenames between a manifest and an export folder.

The tools support the [existing bounded GLB profile](ASSET_PACKS.md). They do not
convert `.blend`, `.fbx`, JSON `.gltf`, external textures or unsupported extensions.
They do not retarget humanoids, author animation, certify licenses or measure GPU
performance. The reference character creator remains procedural. The optional Fieldhand human
example reads the same saved choices and retains the procedural fallback.

## Try the complete included example

Run these commands from the project root. `wildlife-preview` must not exist; its
parent, the current directory, already exists.

```sh
cargo run -p terra-bevy --no-default-features --bin terra-assets -- inspect crates/terra-bevy/assets/wildlife-pack/deer.glb
cargo run -p terra-bevy --no-default-features --bin terra-assets -- build crates/terra-bevy/assets/wildlife-authoring.recipe.json wildlife-preview
cargo run -p terra-bevy --no-default-features --bin terra-assets -- validate wildlife-preview/pack.json
cargo run -p terra-demo -- --play --asset-pack wildlife-preview/pack.json
```

The [working recipe](../crates/terra-bevy/assets/wildlife-authoring.recipe.json)
uses the five original generated wildlife models. It preserves the original
files, their hashes, the shipped default pack identity, and each species'
animation policy. The newly built pack has its own identity because its declared
name and content-addressed model paths differ. This example adds an authoring
workflow; it is not new production human, vehicle or boarding art.

Use a fresh directory, such as `wildlife-preview-2`, for a later export. The
builder deliberately has no force/overwrite option. Select the resulting
`pack.json` explicitly when previewing; building does not change the game's
default pack or saved world.

## Preview the original Comet transport slice

The new transport example exercises the same workflow with a **Comet hatchback**:
an original exterior, transparent glazing, four empty cabin seats, a dashboard,
and separate rolling-wheel and steering pivots. It binds the existing `compact`
catalog model only. Other road-vehicle models, generic background vehicles and
the procedural character creator keep their existing presentation.

For a ready-to-load demo containing both the Comet and the five original
wildlife models:

```sh
python3 crates/terra-bevy/assets/transport-pack/generate.py --check
cargo run -p terra-bevy --no-default-features --bin terra-assets -- validate crates/terra-bevy/assets/transport-demo.pack.json
cargo run -p terra-demo -- --play --asset-pack crates/terra-bevy/assets/transport-demo.pack.json
```

Use Garage to rent or buy the **Comet hatchback** through the ordinary paid
commands; the initially owned pickup intentionally remains a pickup. Boarding,
driving, stopping, reversing and exiting retain the same gameplay rules. The
imported exterior hides in first person and the existing procedural cockpit
remains. The fixed teal/ivory example livery does not display saved paint changes.
Garage disables paid paint when the selected vehicle resolves to an imported or
game-owned fixed-livery scene, and explains why. It rechecks the active appearance
and displayed vehicle before submitting a queued paint click. Procedural vehicles
retain ordinary paid paint, and existing saved paint data remains unchanged.
Empty cabin seats are geometry, not a representation of passenger occupancy.

Artists can inspect and publish an independent copy through the composed
[transport recipe](../crates/terra-bevy/assets/transport-authoring.recipe.json):

```sh
cargo run -p terra-bevy --no-default-features --bin terra-assets -- inspect crates/terra-bevy/assets/transport-pack/comet.glb
cargo run -p terra-bevy --no-default-features --bin terra-assets -- build crates/terra-bevy/assets/transport-authoring.recipe.json transport-preview
cargo run -p terra-bevy --no-default-features --bin terra-assets -- validate transport-preview/pack.json
cargo run -p terra-demo -- --play --asset-pack transport-preview/pack.json
```

`transport-preview` must be new. The recipe and direct demo reference the same
six unchanged source GLBs; the recipe-built content-named paths and declared
pack name produce a distinct pack identity. The separate
`transport-pack/pack.json` contains only the Comet binding, so selecting that
manifest leaves wildlife procedural. The original default woodland pack and
all its bytes and identities remain unchanged.

The model, materials, generator and animation are original project content under
MIT OR Apache-2.0. `generate.py` writes only the new transport files; `--check`
checks reproducibility without writes. The model uses named `parked` and
`rolling` clips for standalone export preview. Runtime wheel rolling uses the
explicit rig described below, so it follows actual forward/reverse motion even
below animal gait thresholds or at speeds above animation playback caps.

This standalone transport slice has empty seats. The next section adds one
customized human and a known manual driver's seated handoff. Broader production
human rigs, equipment attachments, physical boarding, dynamic livery, audio,
DCC presets, LODs and hardware performance still need their own work.

## Preview the original customized human and driver handoff

The optional **Fieldhand** example is an original articulated human with `idle`,
`walk`, `run` and `seated` clips. It uses the existing bounded node-transform
profile, without adding a retargeting or animation-graph format. The composed
pack contains this human, a new Comet driver-cabin variant and all five unchanged
wildlife models. The original Comet files remain byte-for-byte intact. The default wildlife-only pack remains unchanged.

```sh
python3 crates/terra-bevy/assets/character-pack/generate.py --check
cargo run -p terra-bevy --no-default-features --bin terra-assets -- inspect crates/terra-bevy/assets/character-pack/fieldhand.glb
cargo run -p terra-bevy --no-default-features --bin terra-assets -- validate crates/terra-bevy/assets/character-demo.pack.json
cargo run -p terra-bevy --no-default-features --bin terra-assets -- build crates/terra-bevy/assets/character-authoring.recipe.json character-preview
cargo run -p terra-demo -- --play --asset-pack character-preview/pack.json
```

Use a new output directory. The human GLB, new cabin GLB, standalone human pack,
composed direct pack and recipe are reproducible from the Python standard-library script; its
`--check` command returns a JSON receipt without changing source files. The
recipe-built manifest has a distinct normalized identity because its declared
name and content-named model paths differ from the direct manifest.

To preview the choices saved through native character creation, select that
managed profile explicitly; its location is described in [Game flow](GAME_FLOW.md):

```sh
cargo run -p terra-demo -- --world-profile /path/to/world.json --asset-pack character-preview/pack.json
```

All **three body styles** and **eight skin, hair and outfit choices** read the
existing `CharacterProfile`. The creation preview retains its current look;
there is no profile migration. Imported instances own exactly three tint
materials and reuse them when a profile changes. Shared source materials are
never recolored, so one person's choices cannot affect another person's model.
The tint uses the same sRGB palette and sRGB-to-linear conversion as the existing world/fallback meshes;
different lighting and preview shading do not imply pixel-identical images.

A human binding declares the optional policy inside `animation`:

```json
"human": {
  "seated": "seated",
  "skin_material": "character_skin",
  "hair_material": "character_hair",
  "outfit_material": "character_outfit"
}
```

This policy is allowed only for `kind: human`. Its three names must identify
three distinct, uniquely named materials used in the admitted scene. Each must
be opaque, have a white base-color factor, and have no base-color texture,
nonzero emissive factor, emissive texture or vertex colors on its primitives.
This ensures saved colors remain visible.
Hierarchy/rest and animation scales must stay positive; unused clips are
checked too. At runtime, material-role primitive counts and named node identity
must match the accepted model before fallback is hidden. Exactly three private
materials are charged per instance in addition to normal mesh/animation budgets.
The seated clip contributes to the maximum admitted channel count.

For a specific registered vehicle model, a binding may also declare a hip
position in that model's coordinates:

```json
"driver_seat": { "translation_m": [-0.42, 0.46, -0.24] }
```

Only a `vehicle_model` binding may declare a seat. Coordinates must be finite,
at most 10 metres in magnitude and at most 30 metres after binding scale.
Binding yaw, scale and ground offset transform the point. The human seated clip
puts its hip at the model origin; the driver faces the vehicle's forward axis.
The new cabin variant lowers the front cushions, reshapes their backs, opens
the original prototype's closed cabin shell, and connects a lower footwell to
its floor. This keeps ordinary character proportions and clears the imported cabin roof.
The procedural Compact remains a closed vehicle placeholder; its geometry is
not a fit-qualified driver cabin. The original Comet
remains an independent empty-cabin example. Declaring an anchor does not create occupancy or certify a new
vehicle's ergonomic fit: inspect the actual model and all three body styles.

The reference game uses the existing known account's **manually controlled
vehicle** state to move only the avatar's visual onto the vehicle's interpolated
and terrain-aligned pose. It never moves the saved avatar, creates a passenger,
reads another player's private cargo or invents remote occupancy. A public
vehicle appearance alone is insufficient to place a remote driver. The parked
duplicate is hidden; first person or a hidden actor layer hides the entire rider.
On exit/restore the standing/seated handoff is immediate rather than interpolating
a walking body through the car. Both on-foot idle and seated posture initialize at the authored first pose for
a paused save. An optional standing ground offset is suppressed while seated
and restored exactly on foot, so it cannot lift the hip away from the seat. Releasing control to RTS/autopilot clears this manual-driver presentation;
the existing authority does not model persistent physical occupancy for that
control mode, so the unchanged parked avatar becomes visible again.

The driver becomes visible only when the selected cabin-capable vehicle scene
is actually ready. While that car is procedural, deferred or failed, the car
remains visible and the rider stays hidden. Once the cabin is ready, a seated
procedural human retains all saved choices until its own model is ready, and
also survives a human-model failure. If the active pack has no applicable
human/vehicle seat contract, a known manual driver's avatar is hidden. It is not
left standing at the old boarding position. On-foot model failure keeps the
usual standing procedural fallback. Scene admission measures a known seated
driver's distance at the vehicle rather than at the parked authoritative avatar.

The SDK exposes `character_appearance_colors`, `character_body_scale`,
`known_driver_vehicle` and `driver_seat_presentation` for game-owned visual
adapters. The reference adapter lives in `terra-demo/src/remote.rs`; games that
render their own actors must integrate the seat transform/visibility handoff.
The renderer handles validated human tinting and node clips.

Both optional fields require the **0.16 reader**. Existing schema-1 packs omit
these fields exactly as before and keep their normalized identity. Hair geometry
is fixed; all existing hair-color choices remain available. Physical/timed
boarding, facial animation, equipment attachments, remote occupancy replication,
passenger crowds, audio, production LODs and actual target-GPU qualification
remain open. The model is an original, low-poly articulated example, not a claim
of complete production character art. See the
[original model notes](../crates/terra-bevy/assets/character-pack/README.md).

## Prepare the model

1. Author in metres, with **+Y up, local −Z forward, feet at Y=0**. Apply object
   transforms before export. Collision dimensions and authoritative movement
   remain game-owned; scaling an imported mesh does not change them.
2. Export **GLB 2.0 with one scene**, all nodes reachable from that scene and
   unique node names. Remove cameras, lights and unused scene roots. Use local
   translation, rotation and scale transforms instead of node matrices.
3. Triangulate meshes. Embed PNG textures and use supported core PBR materials.
   Disable exporter extensions such as compression or material extensions unless
   a separately extended importer explicitly supports them. Keep external/data
   URI resources out of the export.
4. Bake node-transform animation to **LINEAR or STEP** keys. Use nonempty,
   unambiguous names for clips. Morph-target animation and authority driven by
   root motion are outside the profile. The importer also budgets unused clips,
   samplers, accessors and geometry; hiding unused content does not make it free.
5. Run `inspect` before writing the binding. It lists the exact available clip
   and node names, duration/channel counts and the full model budget. A rejected
   model needs a corrected export before it can be published.

The importer cannot prove that a model's size, axis convention, foot placement
or animation *meaning* is correct. Inspect these visually in the native game.
The [runtime budget table](ASSET_PACKS.md#fixed-import-budgets) is the source of
truth for numeric limits; the authoring tools do not relax them.

## Write a recipe

Keep a recipe beside or above its source exports. `source` paths resolve from
the recipe's directory, independent of the command's working directory. Paths
must be relative `.glb` paths without `..`, URLs, backslashes, control characters
or symlink components. Copy externally authored exports into this tree first.

```json
{
  "schema_version": 1,
  "name": "Studio woodland",
  "author": "Your studio",
  "license": "Your asset license",
  "models": [
    {
      "id": "deer",
      "source": "exports/deer.glb",
      "scene": 0,
      "provenance": "Original model and animation by your studio; state the source and permitted license here."
    }
  ],
  "bindings": [
    {
      "target": { "type": "species", "value": "deer" },
      "model": "deer",
      "scale": 1.0,
      "ground_offset_m": 0.0,
      "yaw_offset_degrees": 0.0,
      "animation": {
        "idle": "idle",
        "walk": "walk",
        "run": "run",
        "feed": "feed",
        "drink": "drink",
        "walk_speed_m_s": 1.4,
        "run_speed_m_s": 4.4,
        "blend_seconds": 0.22
      }
    }
  ]
}
```

Replace the example names and speeds with values from the actual export. The
builder never guesses clip semantics, real ground speeds or license terms.
Different states may intentionally reference the same named clip; a static
binding uses `"animation": null`. Multiple targets may share a model, and each
binding retains its own clips, speeds, blend duration and presentation transform.
Each target can occur only once. Targets and transforms follow the
[runtime manifest contract](ASSET_PACKS.md#manifest-example).

### Declare actual vehicle-wheel motion

Terra 0.15 adds the `vehicle_model` target and an optional `vehicle_motion`
object inside the animation policy. These additions require a 0.15-or-newer
reader. Existing schema-1 kind/species packs retain exactly their prior meaning
and normalized identity; absent wheel policies are omitted during serialization.

```json
{
  "target": { "type": "vehicle_model", "value": "compact" },
  "model": "comet",
  "scale": 1.0,
  "ground_offset_m": 0.0,
  "yaw_offset_degrees": 0.0,
  "animation": {
    "idle": "parked", "walk": "rolling", "run": "rolling",
    "feed": "parked", "drink": "parked",
    "walk_speed_m_s": 2.0, "run_speed_m_s": 10.0,
    "blend_seconds": 0.18,
    "vehicle_motion": {
      "wheel_radius_m": 0.31,
      "wheels": ["front_left_wheel", "front_right_wheel", "rear_left_wheel", "rear_right_wheel"],
      "steering": ["front_left_steer", "front_right_steer"]
    }
  }
}
```

Use 2–8 distinct rolling pivots and up to four distinct steering pivots. Every
steering pivot must be a non-root ancestor of at least one rolling pivot; wheels
cannot contain other wheels or have nested steering pivots. Apply unit scales
through each wheel's ancestor chain before export. Radius is in model metres;
the binding's uniform scale also scales the radius used at runtime. Bake the
wheel axis to local **+X** and its steering parent's axis to local **+Y**.
The renderer composes rotations after each pivot's rest rotation.

The importer rejects missing/duplicate nodes, invalid radii, incompatible target
kinds, ancestor scale, and unsupported animation on the rig's ancestor chain.
Wheel and steering pivots may have preview rotation channels, which the runtime
explicitly replaces with actual rolling/steering. Translation/scale channels on
these pivots or animation on other rig ancestors are rejected, including in
unused clips. Other named model animation remains available.

Rolling integrates signed admitted velocity and preserves phase while stopped,
reversing or switching clips. It does not animate from throttle intent. Manual
steering can turn the front wheels at zero speed; autonomous/public presentation
derives steering from admitted heading change and the known catalog wheelbase.
Without a known model it can roll but does not invent a wheelbase. Pausing time
or authority persistence freezes rolling, steering, clip phase and blending.
The wheel rig reserves its additional per-instance work in scene admission.

`provenance` is a required bounded plain-text statement for each source. It is
retained in the authoring report, separate from the runtime manifest. It records
what the author declared; it is not license verification or publisher signing.
Supply any required license texts, credits and notices with the final game.

## What a build produces

| Output | Purpose |
| --- | --- |
| `pack.json` | Existing runtime schema, complete model digests and explicit binding policies. |
| `models/<sha256>.glb` | Exact accepted model bytes, named by their complete SHA-256. Identical bytes are written once even when several model IDs use them. |
| `authoring-report.json` | Recipe digest, resulting pack identity, source-relative provenance, exact nodes/clips and measured import budgets. |

Identical recipe and model bytes produce identical output bytes and identities.
Identical model bytes are deduplicated in the published directory, but the
aggregate import budget conservatively counts each declared model ID. Reusing
the same bytes under several IDs does not remove their admission cost.
The report's recipe digest includes the original recipe bytes; formatting an
otherwise equivalent recipe can change that report digest. A changed binding
policy changes runtime pack identity even when geometry bytes stay the same.
Provenance is an authoring-report statement, not an added runtime-manifest field.

All source bytes are snapshotted and the complete pack is run through
`ValidatedAssetPack::from_bytes` before an output directory is created. Changes
to an export after preparation cannot replace those accepted bytes during
publication. Runtime validation checks all original budgets and animation,
hierarchy and content-identity rules.

Publication requires a new destination with an existing, non-symlink parent.
Absolute paths are supported; explicit `..` traversal is rejected. Files use
exclusive creation, and `pack.json` is written last. On an I/O failure the tool
attempts to remove only its own new files and directories; if cleanup fails, the
error identifies the incomplete destination for inspection. This local workflow
does not promise atomic directory publication through a power failure or defend
against another process maliciously replacing path components concurrently.

## Diagnose an unsuccessful export

| Message or observation | Next action |
| --- | --- |
| Missing animation named by a binding | Run `inspect` on that source GLB; use an exact listed name or export the missing clip. Do not infer intent from clip order. |
| Unsupported extension or external resource | Export the core self-contained GLB profile with embedded PNGs and supported materials. |
| Hierarchy, parent or reachable-node rejection | Export one complete scene with unique names, no cycles, no multiple parents and no orphan nodes. |
| Node, geometry, decoded scalar, animation or texture budget rejection | Simplify or remove unused source data; merely leaving it unbound is insufficient. |
| Invalid speed/blend/transform | Set finite values within the documented ranges. Walk speed must be positive and run speed greater than walk speed. |
| Relative path or symlink rejection | Copy the GLB below the recipe's real directory and reference it using a plain relative path. Choose a real output parent. |
| Destination already exists | Select a new output directory. Existing packs and source exports are never updated. |
| Validation passes but orientation/feet/scale look wrong | Correct units/axes in the authoring tool, or use the deliberate binding presentation correction. Rebuild and preview again. |
| Native renderer retains a procedural fallback | Inspect runtime asset diagnostics for dependency/hierarchy/loading failure. Headless acceptance does not establish renderer readiness. |

Success writes a JSON report to stdout and exits zero. A rejection exits nonzero,
writes a plain diagnostic to stderr and emits no acceptance JSON. In an art CI
job, treat the exit status as authoritative; an old output folder is not evidence
that a new build succeeded.

## SDK use and acceptance

```rust,no_run
use terra_bevy::asset_pack::authoring::{PreparedAssetPack, inspect_model};

fn prepare() -> Result<(), String> {
    let model = inspect_model("art/exports/deer.glb")?;
    println!("Clips: {:?}", model.clips);
    let pack = PreparedAssetPack::read("art/recipe.json")?;
    println!("Accepted identity: {}", pack.report().identity);
    pack.write_new("art/published-001")?;
    Ok(())
}
```

The focused suite exercises the actual CLI and importer, including deterministic
publication, immutable preparation, exact clip/budget inspection, working-directory
independence, distinct shared-model policies, existing-output preservation,
invalid recipes, path boundaries, source-size limits, unused-animation budgets,
hierarchy rejection and the unchanged shipped wildlife identity:

```sh
cargo test -p terra-bevy --no-default-features --test asset_authoring --test asset_cli --test transport_assets
```

A completed asset handoff also requires a native pass through loading, idle,
walking/running, feed/drink, pause/resume, visibility/fallback and saved-world
reopening. For transport, include forward/reverse motion, stopped/paused wheels,
model-specific selection, and first-person/procedural fallback. The human handoff
adds independent saved palettes/body styles, driver-seat placement, paused
initialization, Exit and Release/RTS transitions, and missing/failed-model fallback.
Hardware frame time, actual GPU residency, a production human/vehicle catalog,
equipment attachments, physical boarding, remote/passenger occupancy and audio
remain separate acceptance work.
