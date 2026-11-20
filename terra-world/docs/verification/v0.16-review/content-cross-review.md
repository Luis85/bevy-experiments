# Independent human-content cross-review — 0.16

Date: 2026-10-29 (UTC). Reviewer: guidance increment subagent, independent of the
content implementation owner. Source review and exact exported-mesh evaluation
are complete. The findings below have coordinated source corrections and
regressions; no remaining source blocker was identified. Native screenshots,
runtime rendering and final Cargo execution are owned by the parent run.

## Findings and coordinated corrections

| ID | Priority | Evidence and consequence | Correction / status |
|---|---|---|---|
| C16-R1 | P1 | The original human at the declared 0.72 m hip anchor intersected the retained Comet roof; all three styles extended above it. Boots also fell below the floor. The original closed shell crossed the cabin at its top and underside. | The owner authored a separate original driver-cabin variant, preserving the accepted Comet source and standalone example. It opens the cabin, connects a lowered footwell, adjusts front seats and declares a 0.46 m hip anchor. Human knees/ankles now keep boots inside the footwell. Independent final GLB measurements pass all three styles. |
| C16-R2 | P2 | An intermediate arm correction placed the hands inside the dashboard, 155–234 mm from the steering rim. The previous coarse whole-body bound test could not detect this. | Shoulder/elbow posture and procedural arms were corrected. Final imported hand-to-rim vertex spacing is 5.9–9.2 mm, with no transverse dashboard intersections. |
| C16-R3 | P2 | An accepted nonzero human ground offset remained on the imported child while the avatar root was placed at the hip anchor, separating imported and procedural seated bodies. | Human scene readiness and subsequent updates suppress only the imported child's standing offset while the known driver is seated; standing/Release restores it. The shared actor root and saved position are unchanged. A nonzero-offset scene regression covers this transition. |
| C16-R4 | P2 | Customization-role admission rejected colored base factors/textures but allowed emission, which could wash out the saved palette despite the promised tint contract. | The three customizable roles now reject nonzero emissive factors and emissive textures in addition to base-color textures, vertex colors and nonopaque/precolored materials. Ordinary nonrole materials retain the existing supported PBR profile. |
| C16-R5 | P2 | A fresh on-foot scene used rest transforms until positive virtual time. A valid idle clip whose first pose differs from rest therefore opened in a rest/T-pose when paused. | SceneAnimator initializes the authored idle phase zero; the readiness observer installs the initial pose for animated scenes. The paused regression explicitly uses a human idle pose different from rest and checks that zero delta does not advance it. |
| C16-R6 | P1 | A seat declaration alone did not make the existing procedural Compact cabin fit-capable. Its roof underside is 1.46 m, and its closed chassis/body extend down to 0.32/0.455 m, whereas the new rider soles are about 0.20 m. Vehicle import failure/defer could expose feet below the fallback chassis and the tall profile through its roof. | Parent approved a narrow readiness boundary: keep the procedural car but hide the rider until its cabin-capable imported replacement is actually active and visible. Within that ready cabin, the procedural human remains visible through its own loading/failure. The pending/ready/lost/ready and view-transition regression covers this adapter boundary. The old procedural cabin was not silently treated as fit-qualified. |
| C16-R7 | P2 | The parent's initial integrated run exposed a palette mismatch: MeshBuilder converts the existing sRGB palette into linear vertex colors, while imported tint materials treated the raw triples as already linear. The earlier source review had accepted that incorrect color-space claim. | Content owner changed imported tint to Color::srgb and compared procedural vertex colors against the resulting linear RGBA. Independent follow-up traced both production paths and Bevy's material-uniform conversion; the correction and revised documentation agree. |

These findings were sent to the owner before editing their files. The owner
implemented the source fixes; this reviewer independently re-evaluated the
exported geometry. No shared display or full-suite Cargo run was started here.

## Exact geometry evidence

The first exported human (`d6c10a24430778d6577a8fc138e6b2eb307c72efb2ee3dc73ad3affd618fc5c3`)
had seated head/hair maximum heights of 1.6993 / 1.7188 / 1.7580 m for balanced,
broad and slender styles at the 0.72 m anchor. The retained roof slab spans
1.535–1.625 m. Hundreds of sampled head vertices fell inside or above that slab;
this was a measured asset-fit defect, not a screenshot interpretation.

The independent [reviewer script](content-cabin-review.py) decodes GLB JSON/BIN
accessors, composes the complete node hierarchy and animation TRS, applies each
saved body-style scale and the declared seat/binding transforms, then checks
the actual mesh. It does not import or call the content generator. It samples
the seated keyframes and their interval midpoints (49 samples); this sample's
seated tracks are constant. Transverse triangle tests cover the shell/footwell,
floor, roof, windscreen, side glazing and dashboard. Coplanar overlap and general
dynamic-interpolation certification are outside this review tool.

Run from the project root:

```sh
python3 docs/verification/v0.16-review/content-cabin-review.py
```

The [intermediate receipt](content-cabin-intermediate.json) retains the corrected
roof/floor but failed hand posture. The [final imported receipt](content-cabin-after.json)
records the subsequent correction:

| Profile | Roof clearance | Boot sole above floor | Closest hand/rim vertices | Shell, footwell, dashboard, roof, floor and glazing crossings |
|---|---:|---:|---:|---|
| Balanced | 95.74 mm | 25.88 mm | 7.07 mm | None detected |
| Broad | 76.15 mm | 20.80 mm | 5.92 mm | None detected |
| Slender | 36.98 mm | 10.64 mm | 9.16 mm | None detected |

Reviewed final source digests:

- Fieldhand: `b5d62261ee73a9a987b754b322ea95ce9d84941ca82e03c87de00568aa759b7e`.
- Driver cabin: `07f58f28cc002288213bec5a03db3e318fb07ec8f6708aa713fd807332353d17`.

These are flat-ground authored visual-envelope measurements. They do not change
collision authority or certify comfort, anatomy, contact animation, native
transparency/sorting, GPU performance, every third-party model or every camera.
The close hand spacing is a geometric sanity check, not articulated finger or
steering-wheel contact simulation.

### Procedural human inside the ready cabin

The procedural human has a separate mesh, not a reuse of the GLB. Its source
dimensions put the highest hair point at `0.46 + 0.983 × body_y_scale` and the
lowest boot sole at `0.46 − 0.249 × body_y_scale`. These calculated source bounds
are 1.443 / 1.46266 / 1.50198 m at the top and 0.211 / 0.20602 / 0.19606 m at the
bottom for balanced/broad/slender. They clear the **ready imported** cabin's
1.535 m roof underside and 0.18 m floor. The revised actual-Mesh regression checks
all three styles and all eight palette choices against those cabin bounds. Its
execution belongs to the integrated demo run; these source calculations are not
represented as an independently executed Rust mesh test.

The procedural hand centers are also placed at the wheel: balanced hands have
Y=1.05 m and Z=−0.47 m, compared with the rim center Y=1.045 m and Z=−0.47 m.
Saved body scale changes the pose coherently. The imported-cabin measurement
tool does not claim to inspect the procedural mesh's every triangle, and native
fallback viewing remains part of parent acceptance.

## Correctness, privacy and compatibility review

**Known occupancy.** `known_driver_vehicle` requires a human Player-role avatar,
that owner's actual account, matching avatar ID, manual possession and an active
owned Vehicle body with public transport appearance. A public vehicle owner or
character profile alone is insufficient. The avatar visual follows the actual
interpolated, terrain-aligned vehicle root; authority positions, private cargo,
rentals and fuel are not used to invent remote occupancy. The regression
explicitly removes private vehicle state while retaining public appearance, then
removes the account and verifies that occupancy is no longer inferred.

**Per-instance appearance and animation.** Every accepted human instance receives
exactly three private material handles only after all role counts/names and
source handles match the validated model. Shared GLB materials are not edited.
Palette updates reuse those handles and do not recolor another person. Animation
policy belongs to the binding; phase, blend and posture belong to the instance.
The scene test covers two distinct profiles, all eight palette updates, stable
handle counts, direct actor teardown, role mismatch and pack removal. Its scene
world is a controlled Bevy scene fixture, not a claim that a hardware GLB render
has already passed.

**Color conversion.** `geometry::MeshBuilder` writes colors through
`Color::srgba(...).to_linear().to_f32_array()`. Native creation preview uses
`Color::srgb` for the same palette. Bevy 0.20's StandardMaterial uniform path
converts its Color through `LinearRgba::from`; therefore imported materials must
also interpret the returned triples as sRGB. The corrected `Color::srgb` path
converts once and matches the procedural linear vertex values. Treating the raw
triples as linear brightened the imported replacement. Different mesh lighting,
shading and exposure can still produce different pixels; color-space equivalence
does not promise pixel identity.

**Limits and lifetime.** The seated clip participates in maximum channel
admission, and three private materials are charged to each instance's byte
estimate. Source model/texture budgets remain shared and bounded. Invalid role
scenes return before any partial private-material allocation. Existing model
retirement retains reservations until source/subasset handles leave the asset
server; tests also exercise private material release. Profile lookup now borrows
saved values and uses an allocation-free unnamed background profile. These are
explicit CPU-side accounting estimates, not measured GPU residency.

**Import contract.** Human customization is restricted to Human-kind bindings,
requires a named seated clip, prohibits a simultaneous vehicle rig, and requires
positive rest/animation scales, including unselected clips. Role names are
bounded, distinct, unique and actually used in the admitted scene. Driver-seat
metadata is permitted only for exact vehicle-model bindings and has bounded
finite coordinates including the binding scale. Existing bounded importer
checks continue to include unused clips, meshes and decoded data. Semantic hip
placement and ergonomic fit remain an author responsibility; schema acceptance
does not prove anatomy.

**Saved state and older packs.** No new occupancy, quest, profile, inventory or
economy representation is added to saves. Existing body/skin/hair/outfit choices
are read as presentation. Optional human/seat fields are omitted when absent,
preserving old normalized pack identities; packs using them require the 0.16
reader. The separate original Comet/wildlife sources remain retained. The new
cabin is an explicit new content-identified model, not a silent replacement of
the accepted source.

**Failure and transitions.** Named hierarchy/material mismatches retain the
procedural actor. Visibility is inherited from the actor so first person and
hidden actor layers hide imported descendants. Enter/Exit, paused restore and
Release/RTS select a discrete pose/root handoff; there is no walking interpolation
through the vehicle. Release returns to the existing parked authoritative
avatar because persistent physical occupancy under RTS is not modeled.

The native adapter runs after the chained render/scene synchronization systems.
It requires the vehicle's `ActorSceneActive` marker and visible root before
leaving a seated rider visible. Removing readiness for failure/defer restores
the car fallback and hides the rider before visibility propagation. Readiness
return exposes the procedural human at the correct transformed hip anchor while
its own replacement loads. Hidden actor layers and first-person rules continue
to hide both; no absent vehicle or absent seat contract fabricates occupancy.
The adapter regression deliberately toggles readiness to test this boundary;
the Bevy scene-spawner regressions separately verify how that marker is produced
and removed. Neither is misrepresented as native hardware-rendered failure
injection.

## Author handoff and remaining acceptance

The authoring guide distinguishes the optional sample from production art,
describes the material and hip-anchor contracts, and identifies what the game
adapter must integrate. It now includes a `--world-profile ... --asset-pack ...`
example so an author can inspect customization chosen through native creation,
identifies the new cabin variant in the character composition, and keeps the
unchanged standalone Comet claim scoped correctly. The fallback-readiness and
color-conversion boundaries are explicit.

The parent integrated pass must execute the current source tests and inspect
native ready/loading/failure presentation, first person, hidden layers,
Exit/Release and paused reopening. This report does not claim those screenshots
have been observed. It also leaves broader DCC presets, humanoid retargeting,
attachments, facial/boarding animation, remote/passenger occupancy, audio,
production LODs, target-device measurement and uncoached player research open.

Reviewed implementation: [pack admission](../../../crates/terra-bevy/src/asset_pack.rs),
[scene readiness](../../../crates/terra-bevy/src/render_scenes.rs),
[human appearance](../../../crates/terra-bevy/src/render_character.rs),
[animation](../../../crates/terra-bevy/src/render_animation.rs),
[scene budgets](../../../crates/terra-bevy/src/render_budget.rs),
[native adapter](../../../crates/terra-demo/src/remote.rs),
[procedural meshes](../../../crates/terra-demo/src/vehicle_models.rs),
[sample generator](../../../crates/terra-bevy/assets/character-pack/generate.py),
[asset guide](../../ASSET_AUTHORING.md) and [pack contract](../../ASSET_PACKS.md).
