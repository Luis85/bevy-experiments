# Asset credits

## Pixel survival presentation — 2 November 2026

The five active PNGs in `art/pixel/` are original OpenAI ImageGen outputs made
for Lastlight: the twenty-pose infected gait sheet, survivors/bosses/portraits/
corpse atlas, sixteen-prop/environment atlas, four-biome ground strip and
checkpoint menu scene. The user's reference screenshots supplied visual
direction; their pixels and characters are not redistributed as game assets.
Exact generation prompts (including the actor alpha correction) are embedded
in the PNGs and recorded in `art/pixel/PROVENANCE.json`. Final generated pixels
are preserved; runtime uses nearest sampling, measured UVs, alpha-derived flash
geometry and foot/muzzle pivots. Historical painted assets remain retained but
are not the active renderer catalogue. Original generated assets follow the
repository MIT license.

`fonts/PixelifySans.ttf` is the unmodified Pixelify Sans variable font, Copyright
2021 The Pixelify Sans Project Authors
([upstream](https://github.com/eifetx/Pixelify-Sans)), under SIL OFL 1.1.
The complete upstream notice is in `fonts/PixelifySans-OFL.txt`; the font and
notice were sourced from the [official Google Fonts directory](https://github.com/google/fonts/tree/main/ofl/pixelifysans).

## Original game art and sound

Lastlight's tactical overlays, particles and fallback drawings are original
procedural/vector artwork authored for this experiment. The character and prop
atlases, checkpoint illustration and ground material in `art/` were generated
specifically for Lastlight with OpenAI ImageGen on 1 November 2026. Exact prompts
are embedded in each PNG and recorded in `art/PROVENANCE.json`. No proprietary
game art or franchise reference images were used. Atlas UV rectangles are
presentation metadata; the generated pixels and alpha are retained.
The 2 November animation pass adds five original four-pose walk strips in
`art/animation/`, with exact embedded prompts and a separate
`art/animation/PROVENANCE.json`. Padded per-pose UVs preserve boots and silhouettes.
The eighteen WAV files in `audio/` are original synthesized sound effects and ambience,
generated with Python's standard library by `scripts/generate_audio.py` and
`scripts/generate_atmosphere_audio.py`. The original nine clips remain byte-identical.
The tactical cues include incoming radio ticks, a rising melee-readiness chime,
and a low mechanical structure-collapse sound, alongside shot, reload, melee,
damage, boss and ambient cues.
The 2 November 2026 atmosphere pass adds cyclic wind, distant urban machinery,
fictional bird-like nature calls, rain and cold air, plus distinct NPC-shot,
turret-shot, trap-trigger and armor-impact cues. These are synthesis profiles,
not field recordings or claims about real species, countries or weapon models.
`audio/PROVENANCE.json` records source generators, fixed seeds, exact file hashes,
PCM dimensions, levels and loop-boundary checks.
No recordings, sound libraries, proprietary game assets or third-party samples
were used. These original assets follow the repository's MIT license.

## Barlow Condensed

`fonts/BarlowCondensed-Bold.ttf` — Barlow Condensed, Copyright 2017 The Barlow
Project Authors. Distributed under the SIL Open Font License, Version 1.1;
the complete license and upstream copyright statement are retained in
`fonts/OFL.txt`. [Upstream font project](https://github.com/jpt/barlow),
[Google Fonts distribution](https://github.com/google/fonts/tree/main/ofl/barlowcondensed).

## Geographical boundaries

Made with Natural Earth. The country/territory boundaries and population source
fields in the bundled `crates/horde_core/src/world.json` derive from Natural
Earth's 1:110m and 1:50m administrative country datasets, which are
[public domain](https://www.naturalearthdata.com/about/terms-of-use/).
The file retains upstream URLs and SHA-256 fingerprints. The generator,
original game modifiers, regional setting text and campaign connections are
project-authored. See `docs/WORLD.md` for the data vintage and map conventions.

## Rules references

GURPS is a trademark of Steve Jackson Games. The GDD and rules documentation
reference GURPS Fourth Edition Lite (August 2004). No rulebook PDFs, rulebook
artwork, rulebook prose or proprietary equipment tables are bundled. See
`docs/RULES.md` for the implemented subset and distribution dependency.

## Survival rig and sound increment — 2 November 2026

`art/pixel/defender-rig-v1.png` adds two empty-hand survivor bodies, throw poses,
six separate firearms, grenade/bottle and blast/fire/smoke motifs. Original
OpenAI image_gen pixel art; exact prompt and SHA-256 are recorded in
`art/pixel/PROVENANCE.json` and embedded by the asset metadata tool. Generated
pixels and transparent alpha are retained. Runtime crops and source anchors
calibrate planted feet, aiming arms and actual weapon muzzles. No reference
game artwork was copied or bundled. Original assets use this repository's MIT license.

`audio/survival_*.wav` adds 72 original synthesized effects: 24 cue families
with three deterministic variants. They contain no recordings or third-party
samples. The pure Rust exporter is `crates/horde_game/src/audio.rs`;
`audio/SURVIVAL_PROVENANCE.json` records seeds, dimensions and SHA-256 hashes.
The earlier eighteen WAV assets and their notices remain intact. See
`docs/AUDIO_SURVIVAL.md` for the mix, export checks and audition reel.

The survivor-selection pass adds `art/pixel/keyart-v2.png`, an original ImageGen
edit removing the baked foreground hero from `keyart-v1.png`. The new scene
leaves the protagonist area empty for the selected runtime survivor. The old
scene is retained. Exact edit prompt and SHA-256 appear in pixel provenance;
animated light, mist, scenery and character layers are presentation code.

The 3 November menu-pose pass adds `art/pixel/menu-survivors-v1.png`, two
original Rook/June full-body low-ready pistol poses generated with ImageGen.
The exact prompt and source SHA-256 are retained in pixel provenance and PNG
metadata. Cohesive arms, hands and weapon remain together in the menu artwork;
the battlefield keeps its calibrated aiming rig. Original pixels/alpha are
retained and shared upload uses correct premultiplied alpha. MIT license.

## Modular survivors — 3 November 2026

`art/pixel/survivor-components-v2.png` is an original OpenAI image_gen atlas of
sixteen independent survivor heads, torsos, pelvises, legs, sleeves and hands.
The original generation `survivor-components-v1.png` remains as the edit source;
v2 removes a baked vest so equipped protection can be a separate swappable layer.
Exact generation/edit prompts and source hashes are in
`art/pixel/survivor-components-source.json` and `art/pixel/PROVENANCE.json`;
the exact prompt for each operation is also embedded in its PNG metadata.
Generated pixels and alpha are preserved. MIT license; no reference-game art.

`art/pixel/character-rig.json` authors assembly, attachment sockets and original
code-native pixel patterns for removable armor, carried equipment and melee gear.
Weapons use the original separate six-firearm artwork in `defender-rig-v1.png`.
Equipped instance snapshots select these layers; procedural aiming and hand
placement do not create equipment, resolve hits or change authoritative damage.

The native model review adds `art/pixel/survivor-components-v3.png`, one targeted
ImageGen edit replacing the four curved walking-leg cutouts with neutral standing
legs. v1/v2 remain retained source assets. The exact edit prompt and both original
and metadata-bearing SHA-256 are in `art/pixel/survivor-components-v3-source.json`;
the prompt is embedded in the PNG and indexed in pixel provenance. Generated
RGBA is retained without raster postprocessing. The runtime separately corrects
part layering, shoulder contours and fitted equipment masks. MIT license.

`art/pixel/canonical-parts-v1.png` reconstructs only clean torso, sleeve, hand
and wearable layers from the canonical menu-survivor reference using built-in
ImageGen. Exact original head/hair and lower-body pixels are reused directly
from `menu-survivors-v1.png` by runtime UV masks; the generated upper-clothing
kit is a reconstruction. `canonical-parts-source.json` records both prompts,
reference hashes, source bounds, calibration notes and the original/embedded
file hashes. The final edit prompt is also embedded in the PNG. Raw RGBA is
preserved; previous kits remain historical assets. MIT license.

`art/pixel/player-underlayers-v1.png` adds two coordinated charcoal underlayer
garment bundles with independent torso, sleeve and cuff pieces for unequipped
or alternative outfits. It was generated from the original survivor artwork as
a style/proportion reference, then edited to remove baked torso sleeve lobes.
Both exact prompts, original reference and raw-output hashes are retained in
`player-underlayers-source.json`; the final prompt is embedded in the PNG and
indexed in pixel provenance. Metadata embedding preserves the generated RGBA.
This asset supplies separate alternative clothing; it does not replace the
original source pixels in the accepted default survivor bind. MIT license.

`art/pixel/player-equipment-v1.png` contains nine independent carried accessories
and melee tools for the authored medic, radio, satchel, rig, crowbar, axe,
machete, hammer and saber styles. OpenAI image_gen used the original survivor
artwork as the visual reference. The exact prompt is embedded in the PNG;
`player-equipment-source.json` and pixel provenance retain its source, raw and
final hashes. Generated RGBA is preserved. MIT license.

`art/pixel/player-hidden-torsos-v1.png` supplies coherent olive/slate cloth
material for exposed unknown torso regions beneath the original posed limbs
and removable carriage. It is disabled in the exact original reference bind;
known visible original source pixels remain preserved. OpenAI image_gen used
the original still as its sole visual reference. The exact prompt is embedded;
`player-hidden-torsos-source.json` and pixel provenance retain reference, raw
and final hashes. Prompt embedding preserves generated RGBA. MIT license.

The source-owned player rig uses disjoint original-pixel masks from
`menu-survivors-v1.png` for its exact reference appearance. The earlier generated
body kits are retained historical assets, with wearable pieces used only by
explicit alternative gear recipes. Underlayers and hidden backing reconstruct
separately identified clothing or unseen regions; they do not replace known
original identity pixels in the reference bind. See `docs/PLAYER_MODEL_AUTHORING.md`
for source ownership and repair provenance.
