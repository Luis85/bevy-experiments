# Player model validation

## Accepted original bind, 3 November 2026

The original default Rook/June bind and visible source ownership are accepted
for the source-model subsystem. The independent reviewer opened both native
source-versus-assembly captures, both semantic part sheets, and their manifests,
then reran the read-only exact PNG checker for each survivor. This approval
does not cover articulation, alternate clothes, alternate weapons, production
integration, or performance.

Evidence directory: `target/journey-review/source-model-native-1/`.

| Evidence | Rook | June |
| --- | ---: | ---: |
| Source ownership tile | `[0,0,887,887)` | `[887,0,1774,887)` |
| Native layout frame | `[282,42,654,848)` | `[1128,60,1465,848)` |
| Visible original tile pixels | 195,379 | 182,305 |
| Partial-alpha original tile pixels | 194,427 | 181,738 |
| Opaque original tile pixels | 952 | 567 |
| Actual selected parts | 22 | 23 |
| Actual cached rectangles | 4,110 | 3,945 |
| Actual cached vertices / indices | 16,440 / 24,660 | 15,780 / 23,670 |
| Missing / duplicate original visible pixels | 0 / 0 | 0 / 0 |
| Raw differing pixels / bytes | 0 / 0 | 0 / 0 |
| Actual mesh position / UV / index mismatches | 0 | 0 |
| Native differing pixels / bytes, black | 0 / 0 | 0 / 0 |
| Native differing pixels / bytes, white | 0 / 0 | 0 / 0 |
| Maximum native channel difference | 0 | 0 |
| Original opaque source texels independently matched per background | 952 | 567 |

The source PNG is unchanged, SHA256
`4feb21b05c1ad5d281d4f0134fdfe546356ca71a204dba04b7c81c48bd356a3a`.
Its decoded source RGBA FNV1a64 is `f6b2cb172a17bb3d`.

Rook's legacy layout right edge **652 explicitly extends to 654** to retain
the original faint pistol fringe. This preserves 34 additional nonzero-alpha
source pixels in the native crop. It is the first exact native fixture, not
a replacement/blessing of an earlier screenshot. Full ownership continues to
cover the original 887-square tile. June's layout frame is unchanged.

Both captures used egui pixels-per-point 2. Each source texel still occupies
exactly one physical screenshot pixel: the common model root scale is 0.5
logical points per source texel, with integer physical ROI edges. The native
comparison uses the same managed premultiplied texture, nearest sampling,
uniform transform and per-panel clip for both unsplit reference and actual
part assembly. No tolerance, SSIM, resampling or screenshot rewriting is used.

The native part sheets show independent original boots, trouser legs, pelvis,
face/hair/neck, near sleeves/cuff, visible far-sleeve fragments, hands, pistol,
pack/straps and June's webbing. There is no whole-character image underneath
the part packets. Faint residue remains separate and is nearly invisible in
the sheets. Far-arm fragments represent only the surfaces visible in the
still; they are not complete recovered occluded limbs.

Removing each slot is also tested on actual selected packets. The egui adapter
test examines emitted meshes after slot removal: selected nonempty cached
meshes only, original UVs, shared transformed positions and white vertex tint.
Empty logical attachments need no degenerate draw mesh. These safeguards keep
an exact whole image from concealing a fallback body or meaningless labels.

## Reproduce the bind proof

From the `horde-defense` workspace:

```powershell
pwsh -NoProfile -File scripts/capture_player_models.ps1 -BindOnly
```

The runner builds with `cargo build --locked -p horde-defense`, captures the
four isolated native fixtures at logical 1800×1000, runs both exact checkers,
and writes `summary.json` in a fresh evidence directory. `-NoBuild` and
`-Executable <path>` support an already verified binary. Environment values
are restored, including the parity manifest path; saves use an isolated root.
Read-only PNG analysis requires Python with Pillow.

To recheck existing captures without running the game:

```powershell
pwsh -NoProfile -File scripts/verify_player_models.ps1 `
  -Capture target/journey-review/source-model-native-1/model-parity-rook.png `
  -Metadata target/journey-review/source-model-native-1/model-parity-rook.json
pwsh -NoProfile -File scripts/verify_player_models.ps1 `
  -Capture target/journey-review/source-model-native-1/model-parity-june.png `
  -Metadata target/journey-review/source-model-native-1/model-parity-june.json
```

The checker independently verifies the original PNG hash, approved frame,
physical ROI size/bounds/nonoverlap, nonempty reference artwork, transparent
source/background alignment, exact opaque original RGBA samples, exact raw
bind audit, and every native comparison RGBA byte.
It rejects disabled Python assertions. Native semantic sheets still require
human review; the runner deliberately does not claim that it performed one.

Without `-BindOnly`, the runner additionally captures
`model-matrix-rook`/`model-matrix-june` using the production texture and pose
pipeline. Each matrix must report all 34 actual source/selected-equipment,
missing-slot, firearm, outfit, directional, action and reduced-effect cases.
The JSON records the submitted parts/geometry and measured contact errors;
the PNGs still require visual review. These later matrices are not covered by
the original-bind approval above.

## Static confirmation after the bounded correction

The independent reviewer inspected all eight PNGs and JSON manifests in
`target/journey-review/source-model-confirmation/`, plus the six actual
battle/character/comparison PNGs in
`target/journey-review/source-model-game-confirmation/`. Both exact native
checkers were rerun successfully on the confirmation captures.

| Confirmation evidence | Rook | June |
| --- | ---: | ---: |
| Original source parts after wrist separation | 23 | 24 |
| Original cached rectangles | 4,142 | 3,978 |
| Actual vertices / indices | 16,568 / 24,852 | 15,912 / 23,868 |
| Raw missing / duplicate / differing pixels | 0 / 0 / 0 | 0 / 0 / 0 |
| Added reference packet matrices differing from identity | 0 | 0 |
| Native differing pixels / bytes on each background | 0 / 0 | 0 / 0 |
| Independently matched opaque source samples per background | 952 | 567 |
| Actual matrix cases / unsupported cases | 34 / 0 | 34 / 0 |

The source PNG/hash, alpha-class totals, layout frames and uniform source
mapping remain unchanged. The authorized semantic refinement separates the
original near-wrist bridge from the pointing palm: 1,492 Rook and 1,308 June
source pixels remain in `hand.near.wrist`. This changes ownership granularity,
not the reconstructed source image. Bind manifests now report the actual
`rook.original`/`june.original` skin ID and each emitted part's material.

The correction removes the stationary sleeve-like waist band and June's
repeated repair grid. The causal four-column strips confirm those artifacts
were hidden backing, not the original source owners. Corrected Pistol/up
keeps the gun and hands visible. Active melee uses the original far-fist
pixels as a closed near grasp with the original wrist/cuff retained; source
Bind and finished action recovery retain the original palm. Missing gear and
the three armor variants remain independently rendered selections. Heads,
proportions, separate planted feet and mirrored facing remain recognizable.

This is static acceptance of the original source and the corrected common
battle/preview presentation. A remaining visual limitation is the stylized
near-vertical rifle hold close to the outer shoulder/head and the fixed
profile head pitch; the comparison/matrix images do not establish fully
natural aiming anatomy at every elevation. Static evidence does not establish
animation timing, real-time performance, audio or complete interactive play.

## Appearance and motion acceptance requirements

The static/geometry contracts above are implemented and inspected; temporal
coverage and performance require their separate evidence. Keep
`resolve_model` as the unchanged source-exact reference. Use
`resolve_appearance(catalogue, model_id, &EquipmentVisual)` and
`resolve_pose(catalogue, &appearance, PlayerPoseSample, placement)` for articulation.
The verification must consume their actual returned packets/meshes and sockets.

| Gate | Required evidence |
| --- | --- |
| Reference regression | Both original binds retain all existing raw/native zero-difference checks; hidden fill contributes no default bind pixels. |
| Frozen appearance | Starter and real owned item styles resolve from the frozen visual snapshot; changes to current inventory do not change an already deployed model. No name-based grants or inferred missing gear. |
| Empty slots | Removing ranged/carriage/outfit removes its actual owned packets, without baked original gun/webbing remaining in another part. Defined base clothing is explicit; empty inventory is not a starter gear grant. |
| Skin bundles | Outfit replacement selects compatible torso, sleeves/cuffs and reveals together. Incompatible/unknown recipes return a defined error or explicit documented fallback; no mixed unreviewed body. |
| Six firearms | Pistol, SMG, shotgun, rifle, marksman and machine gun select real distinct authored weapon packets and matching hand/contact/socket data on both survivors. Metadata-only class labels do not count. |
| Segment lengths | Near/far upper- and forearm distances derive from original bind joints and retain those lengths under the common scale. No independent body/limb stretching to force contact. |
| True contacts | Compare actual named hand-contact anchors to the transformed gun grip/support. Original palm centers and contacts are distinct source points; do not force their centers together. |
| Shared origins | Drawn barrel endpoint, reported muzzle, muzzle flash, bullet trail and recoil use one resolved weapon transform. Throw VFX uses the resolved throwing hand. |
| Aiming | Native views at right, up-right, up, down-right/down and left. Inspect grip, both arms/hands, shoulder/cuff seams, body silhouette, head direction and planted distinct feet. |
| Actions | Native samples at start/middle/end of reload, throw, melee, brace and hit; firing/recoil plus release/recovery. Inspect both heroes and representative short/long guns. |
| Hidden surfaces | Every newly exposed shoulder/elbow/wrist/torso patch is an explicitly authored hidden fill/reveal. It must not silently replace already visible original pixels. |
| Paused/reduced | Identical immutable tick/action/aim inputs yield identical packets and sockets. Pausing freezes the model; reduced effects suppress incidental motion without altering equipment, legal attack origins or reference bind. |
| Cache/performance | Local source geometry stays cached across repeated resolution; no source image scans, I/O, RNG or game mutation during rendering. Measure actual native NPC-battle timing after production integration. |

Contact example: Rook's existing `palm.near` source point is `(540,430)`,
while his pistol `grip` is `(566,433)`. A contact socket or explicit attachment
offset must preserve that authored difference at bind. Making those two
centers coincide would destroy the accepted still-art reconstruction.

Motion changes geometry, so arbitrary animated frames cannot be pixel-identical
to one still. Their acceptance is source-faithful appearance, correct assembly,
contacts and animation, supported by actual native static/motion evidence.
Compile success or an IK endpoint test alone is insufficient.

## Native motion confirmation and final check scope

The independent reviewer inspected native first/middle/recovery PNGs, player
frame progression, raw manifests, and decoded representative MP4 frames under
`target/journey-review/source-model-motion/`. Every scene has all 24 native
PNGs. Native frames advance 60→152 in steps of four; the encoded previews are
H264, 24 frames at nominal 15 fps, 1.6 seconds. These deterministic exports
are presentation evidence, not wall-clock performance measurements.

| Scene | Simulation ticks | Precisely observed coverage |
| --- | --- | --- |
| `npc` | 2338→2430 | Autonomous aimed hold, actual barrel flash/trail and shooting progression; retained hands/feet and existing battlefield remains. |
| `battle-female` | 2338→2430 | June's quiet held stance and world/turret combat; this interval does not establish manual player firing. |
| `reticle-reload` | 2412→2504 | Live reload countdown/reticle and return to held weapon without a low-ready arm reset. |
| `ordnance` | 2383→2475 | Actual explosion/burn aftermath and battlefield remains; prepare-time releases precede this export window. |
| `gesture-throws` | 2338→2430 | Rook's actual grenade release at tick 2343 and Molotov release at tick 2375, visible hand/projectiles and gun stowing, followed by coherent held recovery. |
| `gesture-melee` | 2338→2430 | June's actual contact melee at tick 2343 against target 13, visible closed grasp/axe and target hit flash, followed by held recovery; recorded commitment decreases 27→0. |

The two gesture fixtures require scene, motion, sequence and non-practice
opt-ins together. They enqueue normal controls after the 60-frame warmup:
grenade at native frame 64, Molotov 96, or contact melee 64. Fixed runtime/core
processing creates the observed events and consumes actual charges. The melee
setup legally salvages defenses and stages one already-spawned living enemy
at contact during isolated initialization; later frames do not overwrite
health, cooldowns, events or outcomes. Unit observers retain real event facts
before normal VFX expiry; production event lifetimes are unchanged.

Both gesture manifests contain 24 frames and the expected post-warmup legal
events. Their native recovery samples show no missing limb, detached fist or
low-ready arm pop in the covered sequence. Raw corpse counts never decrease
over these windows: 11→12 for throws and 11→11 for melee. This is a bounded
observed retention check, not an exhaustive proof for an entire campaign.
The earlier static acceptance and stylized vertical-rifle/head-pitch limitation
remain unchanged. No additional visual/data corrections were made during this
confirmation.

The final coordinated workspace gate is **432 passed, 0 failed, 1 ignored**
(231 app and 201 core tests), with formatting and strict Clippy passing;
the native build also passes. Gate log:
`target/journey-review/playtest-verified-workspace-gate.log`.

Separate focused native NPC checkpoint measurements report 1× 59.8917 fps
(1,657 focused frames, p99 18.5217 ms) and 4× 59.8085 fps
(315 focused frames, p99 19.3262 ms). Maximum persistence work is 0.197/0.2554 ms.
The 4× sample has one frame above 50 ms and none above 100 ms; maximum measured
UI work is 8.48 ms. These are local focused-window/OS/VSync observations, not
a portable FPS guarantee. Both speeds complete the same 1,772 simulation ticks;
their entire decoded game states also exactly match each other and the prior
worker baseline (`source-model-worker-comparison.json`). Fixed-step video
exports are not included in those performance claims.
