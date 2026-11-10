# Living world: graphics, assets and experience research

Research date: 2026-11-09. The existing 3D island, survival rules and dark/gold
Barlow UI remain the product direction. This is a native Bevy 0.20 game, not a
web redesign. Recommendations below are design conclusions, not claims that
every researched feature shipped.

## Baseline evidence

Read assets/CREDITS.md, assets.rs, anim.rs, model_preview.rs, visuals.rs,
village_world.rs, colony_scene.rs, camera.rs, island_camera.rs and homestead views.
Inspected the prior increment's actual arrival and continued-save Explore PNGs
under target/playtest-player-flow-a08fee70a8 and e64b4a6163 in the original checkout.

The baseline includes 28 GLBs, 20 glTFs, seven PNGs, one JPG and one font; no audio.
Only Hooded Rogue and Knight provide human bodies. Four-model arrays couple asset
loading, animation and preview. Every workshop except the well uses the same blue
house. The fixed 64 by 64 terrain creates 4,096 cuboid entities, with an obvious
checker pattern. Broad rectangular roads and sparse scenery flatten the sense of
place. Water is a static plane. Day/night lights switch abruptly; seasonal ground
colours, lanterns and small smoke spheres are the main atmosphere. The HUD is
consistent, but Explore emphasises four six-metre movement buttons over geography.

## Primary sources and decisions

1. [KayKit Medieval Hexagon source](https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0).
   CC0 source includes actual blacksmith, lumbermill, market, mine, tavern,
   watermill, well, windmill, second home and tower silhouettes, plus hills and
   shoreline plants. The current fetch script bundled only one house and grain.
   Expand this matching art family first, measuring footprint, scale and pivot.
2. [KayKit Forest Nature](https://kaylousberg.itch.io/kaykit-forest).
   Free CC0 tier offers 100-plus tree, rock, bush and grass models. The much larger
   headline includes paid recolours. Candidate for future foliage expansion;
   catalogue counts must not be presented as distinct shipped geometry.
3. [KayKit Adventurers source](https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0).
   Exact additional legacy models include Barbarian, Mage and unhooded Rogue.
   Embedded animations make these inexpensive silhouette additions, but civilians
   need a wider body/clothing roster than adventure archetypes alone.
4. [KayKit Character Animations](https://kaylousberg.itch.io/kaykit-character-animations).
   CC0 social/tool/movement categories include waving, sitting, lying, fishing,
   hammering and digging. Rig_Medium and Rig_Large are separate. Inspect actual
   clips and map semantic activities per rig; do not make animation authoritative.
5. [Quaternius Modular Women](https://quaternius.com/packs/ultimatemodularwomen.html)
   and [Modular Men](https://quaternius.com/packs/ultimatemodularcharacters.html).
   These specific older packs show CC0, interchangeable body pieces and 24
   animations. Their creator's Google Drive currently returns quota-exceeded
   download pages. Independently verified creator uploads on Poly Pizza supply
   selected CC0 GLBs; per-model source/rights records live in the asset manifest.
   Some other models in the same Poly Pizza collection carry CC-BY: never assume
   the entire collection has the same license.
6. [Quaternius Universal Base Characters](https://quaternius.com/packs/universalbasecharacters.html).
   Multiple proportions and hairstyles offer a longer-term customization route;
   roughly 13k triangles per body and separate outfit/retarget work increase scope.
   Not required for the curated current roster.
7. [Bevy atmospheric fog](https://bevy.org/examples/3d-rendering/atmospheric-fog/).
   DistanceFog and coloured visibility falloff can create depth without hiding
   nearby interactions. Confirmed against installed 0.20 source, not just the
   moving website. Suitable first atmosphere step.
8. [Bevy example catalogue](https://github.com/bevyengine/bevy/blob/main/examples/README.md).
   Installed 0.20 examples demonstrate shared-handle automatic instancing, SSAO,
   atmosphere, bloom and animated water/SSR. SSAO example uses Hdr, Msaa::Off and
   TAA; SSR uses deferred rendering. Add expensive effects only after profiling.
9. [A Short Hike creator postmortem](https://www.gdcvault.com/play/1026613/).
   The accessible session abstract discusses visual style, level design and
   writing within a compact open world. The video was not transcribed. Our design
   inference is to measure meaningful discovery density, not just map size.
10. [Xbox accessibility guideline 115](https://learn.microsoft.com/en-us/xbox/accessibility/xbox-accessibility-guidelines/115).
    Explain errors where they occur and allow review/correction of destructive
    operations. Terraforming should preview affected cells and explain rejection;
    undo deserves an explicit simulation contract rather than a view-only hack.

## Direction and ownership

Three settlements should be identifiable without labels: harbour market/pier and
pale shore; woodland timber hamlet/watermill and dense canopy; exposed upland
mine/forge with stout buildings and grey rock. Region silhouettes, street layout,
landmarks and productive purpose do most of the work; roof colour supports them.
Paths connect useful discoveries and shelter. World travel should support a loop:
prepare supplies, follow a visible landmark, discover residents/resources, perform
real work or exchange, return with a new production possibility. No fabricated
quest economy or invisible reward grants.

Appearance is stable resident identity independent of job. At least six genuinely
different body/clothing silhouettes, including women and men, precede colour
variation. Animation maps semantic action to imported clips and tolerates missing
clips. Tools follow equipment; work effects follow real work. Rendered identity
and task cues make job changes visible without replacing a resident's face.

Simulation owns terrain, regions, settlements, resource lifecycles and recorded
consequences. Renderer owns chunk meshes, shared palettes, dressing and atmosphere;
host owns quality/audio preferences. Terrain chunks project authoritative cells,
rebuilding only chunks whose cell content changes. Cosmetic movement never affects
navigation or simulation hashes. Terrain bounds drive camera limits and overview.
Fog must prevent new decorations/labels from revealing hidden identities or stock.

Priorities: first geometry and coherent region composition; then smooth daylight,
distance haze, warm lamps, moving water and restrained wind; then state-driven
smoke, seasonal foliage and ambience. Profile before enabling SSR/volumetric fog.
A discovered-region map should replace directional movement buttons as Explore's
primary experience, with a compact destination hint during ordinary play.

## Asset audit and validation

`scripts/fetch_living_assets.py` records exact URLs, authors, licenses, byte counts
and SHA-256 values. It rejects changed bytes when a manifest exists and checks
downloaded glTF syntax before promotion. KayKit source revisions are pinned and
original license files retained. Four selected civilian GLBs were inspected:
each has 24 embedded CharacterArmature clips and four/five skinned pieces;
KayKit has its own 76-clip naming/hand-slot convention. Civilian wrist hierarchy
has scale100, requiring a normalized accessory socket. No retargeting is assumed.

Acceptance must include native views of all regions, varied residents, actual
working machinery, depleted/restored vegetation, daylight/dusk/night, the map and
editor, at 1280x720 and 1024x768. Measure frame-time median/p95, entity/material
counts and sim tick time on the actual machine. A 60fps 1080p target is a target,
not a measured claim. Verify appearance stability across job changes/save-load,
missing-clip fallback, terrain grounding and headless/rendered hash equivalence.
The final verification report records what actually shipped and what was tested.

### Imported payload measurements

The curated importer verified 59 files, 18,550,130 bytes, with 31 distinct model
hashes. These comprise ten building silhouettes, fourteen nature models and seven
new character models; buffers, textures and licenses account for the other files.
Measured triangles/clips from the actual GLB accessors and animation tables:

| Model | Triangles | Embedded clips |
|---|---:|---:|
| Barbarian | 6,543 | 76 |
| Mage | 5,683 | 76 |
| Unhooded Rogue | 6,377 | 76 |
| Village Woman | 6,108 | 24 |
| Traveller Woman | 6,424 | 24 |
| Farmer Man | 5,476 | 24 |
| Traveller Man, including backpack | 10,198 | 24 |

The three KayKit counts include accessory meshes which the presentation hides
when unequipped. The live civilian work pose uses the pack's Interact clip, not a
promised bespoke farming/fishing animation. No animation retargeter or paid asset
purchase was needed. Native playtest remains necessary to confirm motion/grounding.

## Native verification and bounded visual polish (2026-11-10)

The completed first matrix is `target/playtest-living-world-269746f22b/summary.json`.
Its 54 PNGs exposed missing home visuals, excessive overview distance, faceted
water highlights, obstructive cloud placement, the arrival/well overlap and
below-the-fold travel controls. The home loss was a lifetime-filter bug, not
missing simulation housing. A separate read-only rendering subagent also found
terrain-height, fog-coverage, saved-cottage and seasonal compatibility gaps; these
received focused fixes and regression coverage before the visual confirmation.

The confirmation is `target/playtest-living-world-f902beb92f/summary.json`:

- `ok: true`, 54 native PNGs at 1280x720 and 1024x768, no native error log entries.
- Binary SHA-256 `f9d3ba2b091022ce7dc48151f011a59eeb5535a17ebb77a66222b6647bde79a9`.
- 43 loaded animation players; 42 resident profiles; all 42 authored home visual
  roots remain after Continue and after a second New game.
- Actual character travel to Alderwood, Stone Ridge and Harbour through enabled
  chart widgets; no teleportation or material grants. Arrival is `(500,-43000)`.
- Save/title/Continue retains one native window and character. A recorded regional
  terrain brush changes and saves revision exactly once. Spectator stepping reaches
  tick 17713 while preserving the unarrived visitor.
- Selected civilian woman/man are captured at an actual 12m camera distance.
  The reviewed images show their bodies, clothing/materials and grounded feet;
  captions sit above the selected body. Houses, daylight/night, regional scenery,
  larger overview framing, continuous sea and all three compact travel controls
  were visually inspected after the polish.

This is BRP/widget and recorded-command QA, not physical keyboard/mouse or
screen-reader testing. The eight-second CPU capture ran alongside repository
verification and is not an independent GPU/FPS benchmark. The earliest cold title
frame briefly lacks terrain, which is present in the following title/New captures;
asset/pipeline warm-up is a plausible explanation, not an instrumented diagnosis.
The confirmation also identified a legacy-only navigation-gizmo guard in the
regional editor; that observation was returned to the root coordinator without
changing code underneath its running full verification gate.
