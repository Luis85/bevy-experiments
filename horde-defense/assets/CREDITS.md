# Asset credits

## Pixel survival presentation — 2 November 2026

The first pixel pass made five original OpenAI ImageGen outputs for Lastlight:
the twenty-pose infected gait sheet (`art/pixel/walkers-v1.png`), the
survivors/bosses/portraits/corpse atlas (`art/pixel/actors-v1.png`), the
sixteen-prop/environment atlas (`art/pixel/props-v1.png`), the four-biome ground
strip (`art/pixel/terrain-v1.png`) and a checkpoint menu scene, since replaced by
`art/pixel/keyart-v2.png` and archived. The user's reference screenshots supplied visual
direction; their pixels and characters are not redistributed as game assets.
Exact generation prompts (including the actor alpha correction) are embedded
in the PNGs and recorded in `art/pixel/PROVENANCE.json`. Final generated pixels
are preserved; runtime uses nearest sampling, measured UVs, alpha-derived flash
geometry and foot/muzzle pivots. The historical painted assets are archived,
not shipped (see "Archived (not shipped)" below). Original generated assets
follow the repository MIT license.

`fonts/PixelifySans.ttf` is the unmodified Pixelify Sans variable font, Copyright
2021 The Pixelify Sans Project Authors
([upstream](https://github.com/eifetx/Pixelify-Sans)), under SIL OFL 1.1.
The complete upstream notice is in `fonts/PixelifySans-OFL.txt`; the font and
notice were sourced from the [official Google Fonts directory](https://github.com/google/fonts/tree/main/ofl/pixelifysans).

`fonts/AtkinsonHyperlegible-Regular.ttf` is the unmodified Atkinson Hyperlegible
Regular font, Copyright 2020 Braille Institute of America, Inc., under SIL OFL 1.1.
The complete upstream notice is in `fonts/AtkinsonHyperlegible-OFL.txt`; the font
and notice were sourced from the [official Google Fonts directory](https://github.com/google/fonts/tree/main/ofl/atkinsonhyperlegible).

## Original game art and sound

Lastlight's tactical overlays, particles and fallback drawings are original
procedural/vector artwork authored for this experiment. No proprietary game art
or franchise reference images were used.

The shipped audio is six WAV beds plus 72 survival cues. The six beds are
original synthesized ambience generated with Python's standard library: the
base `audio/ambience.wav` by `scripts/generate_audio.py`, and the cyclic wind,
distant urban machinery, fictional bird-like nature calls, rain and cold-air
loops (`audio/ambient_*.wav`, the 2 November 2026 atmosphere pass) by
`scripts/generate_atmosphere_audio.py`. These are synthesis profiles, not field
recordings or claims about real species, countries or weapon models.
`audio/PROVENANCE.json` records their generators, fixed seeds, exact file
hashes, PCM dimensions, levels and loop-boundary checks; the original ambience
clip remains byte-identical. Both generators default to `--check`, which
re-renders into a temporary folder and compares, writing nothing. The 72
survival cues are described below.
No recordings, sound libraries, proprietary game assets or third-party samples
were used. These original assets follow the repository's MIT license.

## Archived (not shipped)

Production-pass decision 10 moved the assets the game no longer loads to
`archive/` at the workspace root, beside `assets/`, with mirrored paths;
nothing was deleted, and `archive/README.md` explains how to restore a file.
None of them ship in a playtest build. They remain original Lastlight assets
under the repository's MIT license:

- the six painted sheets generated with OpenAI ImageGen on 1 November 2026
  (character and prop atlases, boss sheet, biome panoramas, checkpoint
  illustration and ground material) and the five four-pose walk strips of the
  2 November animation pass, in `archive/art/` and `archive/art/animation/`;
- `archive/art/pixel/keyart-v1.png`, the original pixel menu scene and edit
  source of `art/pixel/keyart-v2.png`;
- the twelve earlier synthesized one-shot WAVs in `archive/audio/`: shot,
  reload, melee, damage, boss, incoming radio ticks, the melee-readiness chime
  and the structure-collapse sound from `scripts/generate_audio.py`, and the
  NPC-shot, turret-shot, trap-trigger and armor-impact cues from
  `scripts/generate_atmosphere_audio.py`.

Their exact prompts, seeds, SHA-256 hashes and original paths are in
`archive/art/PROVENANCE.json` and `archive/audio/PROVENANCE.json`.

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
The six Python-synthesized beds and their notices remain intact; the twelve
earlier one-shot WAVs are archived (see above). See
`docs/AUDIO_SURVIVAL.md` for the mix, export checks and audition reel.

The survivor-selection pass adds `art/pixel/keyart-v2.png`, an original ImageGen
edit removing the baked foreground hero from the original pixel menu scene,
now `archive/art/pixel/keyart-v1.png`. The new scene leaves the protagonist
area empty for the selected runtime survivor. The old scene is archived, not
shipped. Exact edit prompt and SHA-256 appear in pixel provenance;
animated light, mist, scenery and character layers are presentation code.

The 3 November menu-pose pass adds `art/pixel/menu-survivors-v1.png`, two
original Rook/June full-body low-ready pistol poses generated with ImageGen.
The exact prompt and source SHA-256 are retained in pixel provenance and PNG
metadata. Cohesive arms, hands and weapon remain together in the menu artwork;
the battlefield keeps its calibrated aiming rig. Original pixels/alpha are
retained and shared upload uses correct premultiplied alpha. MIT license.

## Modular survivors — 3 November 2026

`art/pixel/canonical-parts-v1.png` reconstructs only clean torso, sleeve, hand
and wearable layers from the canonical menu-survivor reference using built-in
ImageGen. Exact original head/hair and lower-body pixels are reused directly
from `art/pixel/menu-survivors-v1.png` by runtime UV masks; the generated upper-clothing
kit is a reconstruction. `art/pixel/canonical-parts-source.json` records both prompts,
reference hashes, source bounds, calibration notes and the original/embedded
file hashes. The final edit prompt is also embedded in the PNG. Raw RGBA is
preserved. The earlier survivor-component kits were removed with the retired
rig. MIT license.

`art/pixel/player-underlayers-v1.png` adds two coordinated charcoal underlayer
garment bundles with independent torso, sleeve and cuff pieces for unequipped
or alternative outfits. It was generated from the original survivor artwork as
a style/proportion reference, then edited to remove baked torso sleeve lobes.
Both exact prompts, original reference and raw-output hashes are retained in
`art/pixel/player-underlayers-source.json`; the final prompt is embedded in the PNG and
indexed in pixel provenance. Metadata embedding preserves the generated RGBA.
This asset supplies separate alternative clothing; it does not replace the
original source pixels in the accepted default survivor bind. MIT license.

`art/pixel/player-equipment-v1.png` contains nine independent carried accessories
and melee tools for the authored medic, radio, satchel, rig, crowbar, axe,
machete, hammer and saber styles. OpenAI image_gen used the original survivor
artwork as the visual reference. The exact prompt is embedded in the PNG;
`art/pixel/player-equipment-source.json` and pixel provenance retain its source, raw and
final hashes. Generated RGBA is preserved. MIT license.

`art/pixel/player-hidden-torsos-v1.png` supplies coherent olive/slate cloth
material for exposed unknown torso regions beneath the original posed limbs
and removable carriage. It is disabled in the exact original reference bind;
known visible original source pixels remain preserved. OpenAI image_gen used
the original still as its sole visual reference. The exact prompt is embedded;
`art/pixel/player-hidden-torsos-source.json` and pixel provenance retain reference, raw
and final hashes. Prompt embedding preserves generated RGBA. MIT license.

The source-owned player rig uses disjoint original-pixel masks from
`art/pixel/menu-survivors-v1.png` for its exact reference appearance. The earlier generated
body kits are retained historical assets, with wearable pieces used only by
explicit alternative gear recipes. Underlayers and hidden backing reconstruct
separately identified clothing or unseen regions; they do not replace known
original identity pixels in the reference bind. See `docs/PLAYER_MODEL_AUTHORING.md`
for source ownership and repair provenance.

## Asset index

Every shipped PNG and WAV, one row each, with its generator, its provenance
manifest and the first twelve hex digits of its SHA-256. The block is rendered
from the manifests by the ignored test `asset_audit::regenerate_credits_index`
and checked by `asset_audit::credits_index_matches_provenance`.

<!-- asset-index:begin -->
<!-- Generated from the provenance manifests by asset_audit::regenerate_credits_index; do not edit by hand. -->
| File | Generator | Provenance | SHA-256 |
|---|---|---|---|
| `art/pixel/actors-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `e1d50274d53e` |
| `art/pixel/canonical-parts-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `bf97c4d1b5d8` |
| `art/pixel/defender-rig-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `238a12b923da` |
| `art/pixel/keyart-v2.png` | OpenAI image_gen edit of original keyart-v1.png | `art/pixel/PROVENANCE.json` | `c0c6dcc7714f` |
| `art/pixel/menu-survivors-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `4feb21b05c1a` |
| `art/pixel/player-equipment-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `9f70b4d22468` |
| `art/pixel/player-hidden-torsos-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `c2f337f8ae97` |
| `art/pixel/player-underlayers-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `23489f322863` |
| `art/pixel/props-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `7d425c8722ab` |
| `art/pixel/terrain-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `1b45ffa9be15` |
| `art/pixel/walkers-v1.png` | OpenAI image_gen | `art/pixel/PROVENANCE.json` | `0dfe6a9436e1` |
| `audio/ambience.wav` | scripts/generate_audio.py, seed 12 | `audio/PROVENANCE.json` | `fcc36d85b248` |
| `audio/ambient_cold.wav` | scripts/generate_atmosphere_audio.py, seed 305 | `audio/PROVENANCE.json` | `7fe9d9c4809e` |
| `audio/ambient_nature.wav` | scripts/generate_atmosphere_audio.py, seed 303 | `audio/PROVENANCE.json` | `1758dc9eef88` |
| `audio/ambient_rain.wav` | scripts/generate_atmosphere_audio.py, seed 304 | `audio/PROVENANCE.json` | `ee010bb9a09c` |
| `audio/ambient_urban.wav` | scripts/generate_atmosphere_audio.py, seed 302 | `audio/PROVENANCE.json` | `aacf0a4cdcef` |
| `audio/ambient_wind.wav` | scripts/generate_atmosphere_audio.py, seed 301 | `audio/PROVENANCE.json` | `4ca0a9dcd14e` |
| `audio/survival_armor_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `3c0a9700c999` |
| `audio/survival_armor_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `2a3c305bdb9e` |
| `audio/survival_armor_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `97fac8efd41a` |
| `audio/survival_boss_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `49cfb8566f75` |
| `audio/survival_boss_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `fcd627f610cc` |
| `audio/survival_boss_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `d70f413b2f4a` |
| `audio/survival_damage_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `f47a50ae8f62` |
| `audio/survival_damage_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `aaa6cb81ef21` |
| `audio/survival_damage_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `4b5280733129` |
| `audio/survival_explosion_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `30d6de46a426` |
| `audio/survival_explosion_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `b21e68624134` |
| `audio/survival_explosion_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `cda62439fcbe` |
| `audio/survival_fire_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `38867a97959a` |
| `audio/survival_fire_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `5362e0de57a4` |
| `audio/survival_fire_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `004ae1c35baf` |
| `audio/survival_impact_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `eddc65b1885c` |
| `audio/survival_impact_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `9e64143b4ba6` |
| `audio/survival_impact_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `49d182e01bbb` |
| `audio/survival_incoming_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `0f1c58695e0e` |
| `audio/survival_incoming_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `d6c648af8989` |
| `audio/survival_incoming_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `b4b37ff8eb3a` |
| `audio/survival_machinegun_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `497e506df8ab` |
| `audio/survival_machinegun_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `c738d9bf9ab8` |
| `audio/survival_machinegun_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `fc23dd4b5928` |
| `audio/survival_marksman_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `f45d052fa2d3` |
| `audio/survival_marksman_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `0d4876f14071` |
| `audio/survival_marksman_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `a2ce705bb4d4` |
| `audio/survival_melee_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `9c52ceb12092` |
| `audio/survival_melee_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `3f8e76ae2f8a` |
| `audio/survival_melee_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `5a745df3b97e` |
| `audio/survival_molotov_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `3fa00daa4ca9` |
| `audio/survival_molotov_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `e5c8d56114db` |
| `audio/survival_molotov_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `aed80e9ad883` |
| `audio/survival_npc_shot_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `4ae7e1846b09` |
| `audio/survival_npc_shot_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `0cb4c362c620` |
| `audio/survival_npc_shot_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `103fac7c7b85` |
| `audio/survival_pistol_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `fd65323eae54` |
| `audio/survival_pistol_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `5d56c08765f0` |
| `audio/survival_pistol_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `021e03be5e40` |
| `audio/survival_ready_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `6be9660a1bce` |
| `audio/survival_ready_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `651fce587135` |
| `audio/survival_ready_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `5204663675dc` |
| `audio/survival_reload_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `d5fa421c7366` |
| `audio/survival_reload_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `25b72c48436d` |
| `audio/survival_reload_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `86bc2c3d9e2f` |
| `audio/survival_retreat_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `b3ea32ab4b7a` |
| `audio/survival_retreat_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `d93f37f0cfb7` |
| `audio/survival_retreat_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `678807e384c7` |
| `audio/survival_rifle_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `ed78d01486b6` |
| `audio/survival_rifle_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `20935284aa48` |
| `audio/survival_rifle_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `4b45f7eee28b` |
| `audio/survival_shotgun_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `bf9a677f3e65` |
| `audio/survival_shotgun_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `96ec106f929c` |
| `audio/survival_shotgun_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `7ee55d6cef3f` |
| `audio/survival_smg_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `01be1f24091c` |
| `audio/survival_smg_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `ea9792cc9ce5` |
| `audio/survival_smg_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `26d09af3d46b` |
| `audio/survival_structure_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `b315a80c95e3` |
| `audio/survival_structure_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `7501c465eba6` |
| `audio/survival_structure_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `9b315a548701` |
| `audio/survival_throw_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `9bc3c3aa6bab` |
| `audio/survival_throw_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `5c9339813ea0` |
| `audio/survival_throw_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `3392246330b7` |
| `audio/survival_trap_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `ce7b74bd92fb` |
| `audio/survival_trap_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `d76e14b0dc71` |
| `audio/survival_trap_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `4d57948f0c4d` |
| `audio/survival_turret_shot_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `05452664ee50` |
| `audio/survival_turret_shot_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `59a3c0719f28` |
| `audio/survival_turret_shot_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `b101a5e03f83` |
| `audio/survival_wave_0.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `611c3d78d0f7` |
| `audio/survival_wave_1.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `1ea4380b5df2` |
| `audio/survival_wave_2.wav` | crates/horde_game/src/audio.rs (cfg(test) offline authoring module) | `audio/SURVIVAL_PROVENANCE.json` | `af87148c7975` |
<!-- asset-index:end -->
