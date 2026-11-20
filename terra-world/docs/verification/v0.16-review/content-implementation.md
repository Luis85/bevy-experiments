# Content and character increment — implementation review

Status: implementation and independent source review complete. The release
owner is running integration and native acceptance against the frozen candidate;
those receipts are recorded separately. No native acceptance is claimed from
these code observations or the geometric measurements below.

## Scope selected from G09

The accepted 0.15 assets covered five wildlife species and one compact vehicle,
with an exact importer/authoring path but no customized human handoff. This
increment adds **one original Fieldhand articulated human**, four named clips,
three per-instance material roles and a specific manual-driver seat declaration.
It exercises existing customization, public profile data, bounded admission,
procedural fallback, camera visibility, actual motion and immutable publication.
It preserves the existing default woodland pack and every original wildlife and
Comet source file. The new composed pack selects the human, a separate original
Comet driver-cabin variant and the five original wildlife models.

## Original asset and authoring

| Property | Current generated source |
|---|---|
| GLB | `crates/terra-bevy/assets/character-pack/fieldhand.glb` |
| Bytes | 162,140 |
| SHA-256 | `b5d62261ee73a9a987b754b322ea95ce9d84941ca82e03c87de00568aa759b7e` |
| Named nodes / meshes / materials | 39 / 25 / 7 |
| Declared vertices / triangles | 5,268 / 1,756 |
| Clips / channels | idle 2, walk 10, run 10, seated 12 |
| Reproduction | Python standard library; five generated outputs; `--check` compares exact bytes and returns a JSON receipt |
| Attribution | Original code-authored geometry and animation, MIT OR Apache-2.0; no downloaded art or motion capture |

| Driver-cabin property | Current generated source |
|---|---|
| GLB | `crates/terra-bevy/assets/character-pack/comet-cabin.glb` |
| Bytes | 224,424 |
| SHA-256 | `07f58f28cc002288213bec5a03db3e318fb07ec8f6708aa713fd807332353d17` |
| Named nodes / declared meshes / materials | 96 / 83 / 10 |
| Declared / admitted-scene triangles | 2,440 / 2,348 |
| Clips / channels | parked 4, rolling 5 |

The generator reuses the unchanged original Comet GLB writer. Importing that
writer disables bytecode output, so `--check` also avoids writing a cache into
the retained asset directory. The human's root remains fixed; only its bounded
local hierarchy animates. The seated clip puts the hip at local origin. The
composed direct pack declares the existing compact-model seat at
`[-0.42, 0.46, -0.24]` metres in Comet model coordinates.

## Implemented runtime contract

- Optional `animation.human` declares a seated clip and three distinct named
  materials for skin, hair and outfit. Only a human-kind binding accepts it.
- Every role must be a uniquely named, used, opaque white material without
  base-color texture, nonzero emissive factor, emissive texture or vertex colors.
  All model nodes must already belong to the single admitted scene. Positive scale rules apply to the human hierarchy
  and all animation clips, including unselected clips.
- The ready-scene observer validates named nodes and exact material-role
  primitive counts before replacing fallback. It creates precisely three tint
  material handles for that instance; source/shared materials remain untouched.
- Profile changes reuse those same handles. The saved three body-style indices
  and eight skin, hair and outfit indices retain their meanings. Both procedural
  and imported world actors use the same sRGB palette and resulting linear RGBA
  values. Preview lighting/shading can differ; there is no pixel-equivalence claim.
- Scene admission includes the seated clip's maximum channel count and 12 KiB
  of additional estimated material allowance per human instance. Existing model,
  node, primitive, geometry, texture and retirement limits remain in force.
- `driver_seat` is allowed only on a specific vehicle-model binding, with finite
  model-space coordinates bounded to 10 metres each and 30 metres after uniform
  binding scale. Binding yaw/scale/ground offset transform the declared point.
- The reference adapter can seat only an account it actually knows: possessed,
  active registered road vehicle, matching owner, and that account's human
  player avatar. Public appearance/profile information alone cannot establish
  remote occupancy. Owner-private cargo, fuel or rental state is unnecessary.
- The rider visual follows the vehicle's interpolated, terrain-aligned visual;
  no authoritative geographic root moves. Admission follows that vehicle's
  position instead of the parked avatar when evaluating distance.
- Rider visibility waits for the selected cabin-capable car scene to be ready.
  A procedural/deferred/failed car remains available with its rider hidden,
  because its closed placeholder geometry does not provide a usable cabin.
  Inside the ready cabin, a seated procedural human preserves customization
  during human loading or failure. Without a relevant seat contract, a known
  manual driver's avatar is hidden instead of remaining visibly parked. First
  person and hidden actor layers hide rider and exterior.
- Enter/Exit and save restore select the appropriate local posture immediately,
  avoiding interpolation of a walking body through the car. A paused restored
  driver initializes in the seated posture; on-foot idle also samples its authored
  first frame. A standing ground correction is suppressed while seated and restored
  on foot/Release. With unchanged occupancy, pausing
  freezes clip phase and blend. Releasing control to RTS/autopilot restores the
  existing standing avatar: current authority does not model persistent physical
  occupancy across that control mode.

## Independent-review corrections

The reviewer reproduced roof penetration, feet below the original shell and
hands reaching into the dashboard. The original empty-cabin Comet was therefore
retained as-is and a separate `comet-cabin.glb` variant was generated. Its floor
and front cushions are lower, front seat backs are reshaped, the shell is open
through the cabin/footwell, and a connected lower pan preserves an enclosed
underside. The human keeps its standing anatomy and profile scales; bent knees,
flat ankles and a closer elbow pose provide the seated handoff. Independent
actual-GLB geometry measurements and native inspection remain distinct evidence.

The independent [geometry receipt](content-cabin-after.json) samples 49 seated
keyframes and interval midpoints for each profile on flat ground. It finds no
transverse intersections with the open shell, footwell, floor, roof,
windscreen, side glazing or dashboard. Minimum margins and mesh-vertex distances
are below; these describe the authored pose, not dynamic hand contact or IK.

| Body style | Head-to-roof clearance | Sole-to-floor clearance | Closest hand-to-wheel-rim vertex distance |
|---|---:|---:|---:|
| Balanced | 95.7 mm | 25.9 mm | 7.07 mm |
| Broad | 76.2 mm | 20.8 mm | 5.92 mm |
| Slender | 37.0 mm | 10.6 mm | 9.16 mm |

The [independent reviewer](content-cabin-review.py) reads the generated GLB
directly and does not import the authoring generator. The procedural car's closed
chassis is deliberately excluded from this cabin claim: the rider remains hidden
until the selected cabin-capable replacement is ready. Procedural human fallback
is admitted only inside that ready cabin.

The review also corrected color-space consistency: procedural mesh construction
already converts the sRGB palette to linear vertex colors. Imported tint materials
now use that same sRGB interpretation; tests compare the exact resulting linear
RGBA values. Treating the palette as already linear would brighten a replacement.

The review also found and corrected three pipeline edges: standing ground offset
could lift a seated hip; emissive materials could wash out saved color choices;
and a new scene becoming ready while paused could retain an exported rest pose
instead of its authored idle. Regressions cover each. Profile lookup now borrows
saved profiles and uses allocation-free background values, reuses tint handles,
and avoids marking unchanged human transforms as changed while paused.

## Compatibility

Domain save, regional schema and multiplayer wire contracts are unchanged.
Both new fields require a **0.16 reader**, while absent optional fields serialize
exactly as before. Old pack normalized identities are covered by existing
identity regressions. Existing `CharacterProfile` values are read, not migrated.
The direct and recipe-built composed packs are deliberately different identities
because names and content-addressed paths differ. These are content-integrity
identities, not publisher signing.

## Verification and review targets

The code regressions cover material-role admission, duplicate/unused roles,
wrong target, absent seated clip, reflected hierarchy and unused animation,
seat bounds/identity, seated-channel/material budgets, actual speed and pause,
independent instances, exact linear colors, all palettes, handle reuse, actual
asset release after despawn/removal, malformed runtime material hierarchy,
public-appearance privacy boundary and native adapter fallback/first-person/
Exit/Release/RTS/paused restore. The report will be reconciled to the final gate
results; development run logs are not final-executable native acceptance.

The native review should inspect an on-foot customized person, real walking and
running, the paid Comet/manual driver handoff, first person, exit, RTS release,
pause/resume and save/reopen. It should verify the visible seat fit for the
available body styles and retain current build/hash receipts. A model-validity
check alone does not establish a good-looking native result.

## Remaining boundaries

This is an articulated node model, not a full human retargeting/skinning tool.
Hair geometry is fixed while the existing eight hair colors are retained.
Physical or timed boarding, facial animation, equipment attachments, replicated
remote occupancy, passenger crowds, audio, a complete production fleet and human
catalog, DCC presets, production LODs and target-GPU qualification remain open.
The chosen seat fits the new imported cabin variant. The procedural car is an
explicit visibility boundary, not a claimed usable cabin. Other models need
their own deliberate anchor, ready replacement and body-style inspection.
