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

The shipped audio is six WAV beds plus 275 survival cue files. The six beds are
original synthesized ambience generated with Python's standard library: the
base `audio/ambience.wav` by `scripts/generate_audio.py`, and the cyclic wind,
distant urban machinery, fictional bird-like nature calls, rain and cold-air
loops (`audio/ambient_*.wav`, the 2 November 2026 atmosphere pass) by
`scripts/generate_atmosphere_audio.py`. These are synthesis profiles, not field
recordings or claims about real species, countries or weapon models.
`audio/PROVENANCE.json` records their generators, fixed seeds, exact file
hashes, PCM dimensions, levels and loop-boundary checks; the original ambience
clip remains byte-identical. Both generators default to `--check`, which
re-renders into a temporary folder and compares, writing nothing. The 275
survival cue files are described below.
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
  `scripts/generate_atmosphere_audio.py`;
- the three retired Rust-synthesized NPC-shot variants
  (`archive/audio/survival_npc_shot_*.wav`): `Cue::NpcShot` was
  decided by the user on 4 November 2026 and retired on 6 November 2026
  (W1-58); NPC defenders now sound like their weapon class.

Their exact prompts, seeds, SHA-256 hashes and original paths are in
`archive/art/PROVENANCE.json`, `archive/audio/PROVENANCE.json` and
`archive/audio/SURVIVAL_PROVENANCE.json`.

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

`audio/survival_*.wav` adds 275 original synthesized effects in 63 banks: the
23 v1 cue families with three or five deterministic variants (the 24th,
NpcShot, is retired and archived), and the gun-handling, enemy-voice,
boss and interface families added on 13 November 2026. They contain no
recordings, no third-party samples, no model-generated audio and no music.
The pure Rust exporter is `crates/horde_audio/examples/export_audio.rs`; it
checks by default and writes only with `--write`. Each file is mastered offline
to a loudness target under a 0.80 true-peak ceiling.
`audio/SURVIVAL_PROVENANCE.json` (schema 2) records each file's seed,
dimensions, SHA-256, peak, true peak, loudness and limiter gain reduction; the
v1 manifest it replaced is kept verbatim as
`archive/audio/SURVIVAL_PROVENANCE.v1.json`.
The six Python-synthesized beds and their notices remain intact; the twelve
earlier one-shot WAVs are archived (see above). No runtime limiter or
music track is shipped; the mix is calibrated to -23 LUFS at the default
sliders. The v1 audition reel (pre-production-pass, survival-audition.wav and .csv in
the source repository's docs/audio folder) is an original export of the v1
library; it is not shipped. See
`docs/AUDIO_SURVIVAL.md` in the source repository for the buses, the mix, the
loudness gate and the export checks.

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
| `audio/survival_armor_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 346010666078 | `audio/SURVIVAL_PROVENANCE.json` | `ff0d3d8e3ef4` |
| `audio/survival_armor_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 348257488585 | `audio/SURVIVAL_PROVENANCE.json` | `4f40bd21da66` |
| `audio/survival_armor_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 350504311092 | `audio/SURVIVAL_PROVENANCE.json` | `0c3c38bcbe2d` |
| `audio/survival_armor_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 352751133599 | `audio/SURVIVAL_PROVENANCE.json` | `8c91b0e1c808` |
| `audio/survival_armor_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 354997956106 | `audio/SURVIVAL_PROVENANCE.json` | `b143bec22d97` |
| `audio/survival_boss_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 613382544411 | `audio/SURVIVAL_PROVENANCE.json` | `72621fcfc413` |
| `audio/survival_boss_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 615629366918 | `audio/SURVIVAL_PROVENANCE.json` | `5773b7a20a37` |
| `audio/survival_boss_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 617876189425 | `audio/SURVIVAL_PROVENANCE.json` | `1ff7dc804979` |
| `audio/survival_boss_breacher_arrival_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2026633901314 | `audio/SURVIVAL_PROVENANCE.json` | `7eb33912a53f` |
| `audio/survival_boss_breacher_arrival_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2028880723821 | `audio/SURVIVAL_PROVENANCE.json` | `8afe769c85bb` |
| `audio/survival_boss_breacher_arrival_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2031127546328 | `audio/SURVIVAL_PROVENANCE.json` | `9371a30db7fb` |
| `audio/survival_boss_breacher_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2103025866552 | `audio/SURVIVAL_PROVENANCE.json` | `c5b5456316fb` |
| `audio/survival_boss_breacher_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2105272689059 | `audio/SURVIVAL_PROVENANCE.json` | `8a0f89860889` |
| `audio/survival_boss_breacher_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2107519511566 | `audio/SURVIVAL_PROVENANCE.json` | `dc0828dabd26` |
| `audio/survival_boss_breacher_phase_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2064829883933 | `audio/SURVIVAL_PROVENANCE.json` | `cb6431ba5f81` |
| `audio/survival_boss_breacher_phase_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2067076706440 | `audio/SURVIVAL_PROVENANCE.json` | `1a77b0e853e2` |
| `audio/survival_boss_breacher_phase_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2069323528947 | `audio/SURVIVAL_PROVENANCE.json` | `1c4d2d4edda0` |
| `audio/survival_boss_caller_arrival_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2141221849171 | `audio/SURVIVAL_PROVENANCE.json` | `7fcf623f5414` |
| `audio/survival_boss_caller_arrival_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2143468671678 | `audio/SURVIVAL_PROVENANCE.json` | `7eae8dd5e021` |
| `audio/survival_boss_caller_arrival_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2145715494185 | `audio/SURVIVAL_PROVENANCE.json` | `d813f8a107ef` |
| `audio/survival_boss_caller_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2217613814409 | `audio/SURVIVAL_PROVENANCE.json` | `bebb070c44c6` |
| `audio/survival_boss_caller_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2219860636916 | `audio/SURVIVAL_PROVENANCE.json` | `90b3d59aba27` |
| `audio/survival_boss_caller_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2222107459423 | `audio/SURVIVAL_PROVENANCE.json` | `fe2b392f40e3` |
| `audio/survival_boss_caller_phase_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2179417831790 | `audio/SURVIVAL_PROVENANCE.json` | `9aa3f4b4d43d` |
| `audio/survival_boss_caller_phase_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2181664654297 | `audio/SURVIVAL_PROVENANCE.json` | `3bda83facdfe` |
| `audio/survival_boss_caller_phase_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2183911476804 | `audio/SURVIVAL_PROVENANCE.json` | `bd602f1edda1` |
| `audio/survival_boss_siege_brute_arrival_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2255809797028 | `audio/SURVIVAL_PROVENANCE.json` | `3a410174b1fa` |
| `audio/survival_boss_siege_brute_arrival_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2258056619535 | `audio/SURVIVAL_PROVENANCE.json` | `048954f9bfa6` |
| `audio/survival_boss_siege_brute_arrival_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2260303442042 | `audio/SURVIVAL_PROVENANCE.json` | `d4e8c1807842` |
| `audio/survival_boss_siege_brute_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2332201762266 | `audio/SURVIVAL_PROVENANCE.json` | `58b30eb01f3e` |
| `audio/survival_boss_siege_brute_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2334448584773 | `audio/SURVIVAL_PROVENANCE.json` | `df03b38242e6` |
| `audio/survival_boss_siege_brute_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2336695407280 | `audio/SURVIVAL_PROVENANCE.json` | `42eec1067274` |
| `audio/survival_boss_siege_brute_phase_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2294005779647 | `audio/SURVIVAL_PROVENANCE.json` | `63043b01fb33` |
| `audio/survival_boss_siege_brute_phase_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2296252602154 | `audio/SURVIVAL_PROVENANCE.json` | `fdb1895a328b` |
| `audio/survival_boss_siege_brute_phase_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2298499424661 | `audio/SURVIVAL_PROVENANCE.json` | `63c0b7d1e38e` |
| `audio/survival_damage_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 689774509649 | `audio/SURVIVAL_PROVENANCE.json` | `5e322feb346d` |
| `audio/survival_damage_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 692021332156 | `audio/SURVIVAL_PROVENANCE.json` | `e3d5493755b7` |
| `audio/survival_damage_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 694268154663 | `audio/SURVIVAL_PROVENANCE.json` | `6b044e7d5742` |
| `audio/survival_damage_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 696514977170 | `audio/SURVIVAL_PROVENANCE.json` | `14a5d906b386` |
| `audio/survival_damage_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 698761799677 | `audio/SURVIVAL_PROVENANCE.json` | `962775acd8ae` |
| `audio/survival_enemy_armored_contact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1491890144648 | `audio/SURVIVAL_PROVENANCE.json` | `b718b3e475e5` |
| `audio/survival_enemy_armored_contact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1494136967155 | `audio/SURVIVAL_PROVENANCE.json` | `14589e15c4a5` |
| `audio/survival_enemy_armored_contact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1496383789662 | `audio/SURVIVAL_PROVENANCE.json` | `33f4d3337de0` |
| `audio/survival_enemy_armored_contact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1498630612169 | `audio/SURVIVAL_PROVENANCE.json` | `9e904841a9a1` |
| `audio/survival_enemy_armored_contact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1500877434676 | `audio/SURVIVAL_PROVENANCE.json` | `26d8570c0e42` |
| `audio/survival_enemy_armored_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1530086127267 | `audio/SURVIVAL_PROVENANCE.json` | `3e50f052eb41` |
| `audio/survival_enemy_armored_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1532332949774 | `audio/SURVIVAL_PROVENANCE.json` | `05c8285197ff` |
| `audio/survival_enemy_armored_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1534579772281 | `audio/SURVIVAL_PROVENANCE.json` | `d5424d645567` |
| `audio/survival_enemy_armored_death_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1536826594788 | `audio/SURVIVAL_PROVENANCE.json` | `91472f1d3a7a` |
| `audio/survival_enemy_armored_death_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1539073417295 | `audio/SURVIVAL_PROVENANCE.json` | `a2ec0d398d8a` |
| `audio/survival_enemy_armored_windup_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1453694162029 | `audio/SURVIVAL_PROVENANCE.json` | `91a3cf2feb2e` |
| `audio/survival_enemy_armored_windup_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1455940984536 | `audio/SURVIVAL_PROVENANCE.json` | `aeba8408f5cf` |
| `audio/survival_enemy_armored_windup_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1458187807043 | `audio/SURVIVAL_PROVENANCE.json` | `4785ea4ebc09` |
| `audio/survival_enemy_armored_windup_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1460434629550 | `audio/SURVIVAL_PROVENANCE.json` | `a062a9064655` |
| `audio/survival_enemy_armored_windup_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1462681452057 | `audio/SURVIVAL_PROVENANCE.json` | `73eb02151c30` |
| `audio/survival_enemy_brute_contact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1377302196791 | `audio/SURVIVAL_PROVENANCE.json` | `5f71a0740a11` |
| `audio/survival_enemy_brute_contact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1379549019298 | `audio/SURVIVAL_PROVENANCE.json` | `c44f897c64bc` |
| `audio/survival_enemy_brute_contact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1381795841805 | `audio/SURVIVAL_PROVENANCE.json` | `e0efae7bcbc5` |
| `audio/survival_enemy_brute_contact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1384042664312 | `audio/SURVIVAL_PROVENANCE.json` | `6a745d52fa1e` |
| `audio/survival_enemy_brute_contact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1386289486819 | `audio/SURVIVAL_PROVENANCE.json` | `73a5ba6f47c7` |
| `audio/survival_enemy_brute_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1415498179410 | `audio/SURVIVAL_PROVENANCE.json` | `dc581b255049` |
| `audio/survival_enemy_brute_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1417745001917 | `audio/SURVIVAL_PROVENANCE.json` | `b300e1efdda2` |
| `audio/survival_enemy_brute_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1419991824424 | `audio/SURVIVAL_PROVENANCE.json` | `85cc70152c02` |
| `audio/survival_enemy_brute_death_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1422238646931 | `audio/SURVIVAL_PROVENANCE.json` | `2a931c3f9be4` |
| `audio/survival_enemy_brute_death_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1424485469438 | `audio/SURVIVAL_PROVENANCE.json` | `cee97fdd753b` |
| `audio/survival_enemy_brute_windup_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1339106214172 | `audio/SURVIVAL_PROVENANCE.json` | `e20829f317c8` |
| `audio/survival_enemy_brute_windup_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1341353036679 | `audio/SURVIVAL_PROVENANCE.json` | `054fc4e43e12` |
| `audio/survival_enemy_brute_windup_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1343599859186 | `audio/SURVIVAL_PROVENANCE.json` | `432cdac8b670` |
| `audio/survival_enemy_brute_windup_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1345846681693 | `audio/SURVIVAL_PROVENANCE.json` | `721e0cdd6198` |
| `audio/survival_enemy_brute_windup_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1348093504200 | `audio/SURVIVAL_PROVENANCE.json` | `23800f88ac43` |
| `audio/survival_enemy_demolisher_contact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1606478092505 | `audio/SURVIVAL_PROVENANCE.json` | `e70f7fafd617` |
| `audio/survival_enemy_demolisher_contact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1608724915012 | `audio/SURVIVAL_PROVENANCE.json` | `d4056eb51c53` |
| `audio/survival_enemy_demolisher_contact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1610971737519 | `audio/SURVIVAL_PROVENANCE.json` | `f8f50ded6957` |
| `audio/survival_enemy_demolisher_contact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1613218560026 | `audio/SURVIVAL_PROVENANCE.json` | `d7fdc5ff6eef` |
| `audio/survival_enemy_demolisher_contact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1615465382533 | `audio/SURVIVAL_PROVENANCE.json` | `56e36609f1dd` |
| `audio/survival_enemy_demolisher_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1644674075124 | `audio/SURVIVAL_PROVENANCE.json` | `aef758f21011` |
| `audio/survival_enemy_demolisher_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1646920897631 | `audio/SURVIVAL_PROVENANCE.json` | `f2b20eae0739` |
| `audio/survival_enemy_demolisher_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1649167720138 | `audio/SURVIVAL_PROVENANCE.json` | `467004c43e24` |
| `audio/survival_enemy_demolisher_death_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1651414542645 | `audio/SURVIVAL_PROVENANCE.json` | `04c8c01819db` |
| `audio/survival_enemy_demolisher_death_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1653661365152 | `audio/SURVIVAL_PROVENANCE.json` | `77ee28fbc3ec` |
| `audio/survival_enemy_demolisher_windup_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1568282109886 | `audio/SURVIVAL_PROVENANCE.json` | `b6cc3228b1fe` |
| `audio/survival_enemy_demolisher_windup_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1570528932393 | `audio/SURVIVAL_PROVENANCE.json` | `eee9eb2148f3` |
| `audio/survival_enemy_demolisher_windup_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1572775754900 | `audio/SURVIVAL_PROVENANCE.json` | `7c5363b9e4f6` |
| `audio/survival_enemy_demolisher_windup_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1575022577407 | `audio/SURVIVAL_PROVENANCE.json` | `a6ab974f62bf` |
| `audio/survival_enemy_demolisher_windup_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1577269399914 | `audio/SURVIVAL_PROVENANCE.json` | `7c8673581003` |
| `audio/survival_enemy_exploder_contact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1950241936076 | `audio/SURVIVAL_PROVENANCE.json` | `3aba23c38c7a` |
| `audio/survival_enemy_exploder_contact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1952488758583 | `audio/SURVIVAL_PROVENANCE.json` | `8df94ff24d6b` |
| `audio/survival_enemy_exploder_contact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1954735581090 | `audio/SURVIVAL_PROVENANCE.json` | `8b96000fcf93` |
| `audio/survival_enemy_exploder_contact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1956982403597 | `audio/SURVIVAL_PROVENANCE.json` | `eb12ca41f9e3` |
| `audio/survival_enemy_exploder_contact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1959229226104 | `audio/SURVIVAL_PROVENANCE.json` | `e480c262ff3c` |
| `audio/survival_enemy_exploder_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1988437918695 | `audio/SURVIVAL_PROVENANCE.json` | `0724db3ed5f3` |
| `audio/survival_enemy_exploder_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1990684741202 | `audio/SURVIVAL_PROVENANCE.json` | `edd03518cc94` |
| `audio/survival_enemy_exploder_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1992931563709 | `audio/SURVIVAL_PROVENANCE.json` | `bf90ef4f5d14` |
| `audio/survival_enemy_exploder_death_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1995178386216 | `audio/SURVIVAL_PROVENANCE.json` | `e154e4c13edc` |
| `audio/survival_enemy_exploder_death_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1997425208723 | `audio/SURVIVAL_PROVENANCE.json` | `b7e5573fcfb3` |
| `audio/survival_enemy_exploder_windup_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1912045953457 | `audio/SURVIVAL_PROVENANCE.json` | `0db9ca1d7bc1` |
| `audio/survival_enemy_exploder_windup_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1914292775964 | `audio/SURVIVAL_PROVENANCE.json` | `5e0af96e49e1` |
| `audio/survival_enemy_exploder_windup_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1916539598471 | `audio/SURVIVAL_PROVENANCE.json` | `ab7624590d6e` |
| `audio/survival_enemy_exploder_windup_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1918786420978 | `audio/SURVIVAL_PROVENANCE.json` | `1e6590127f13` |
| `audio/survival_enemy_exploder_windup_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1921033243485 | `audio/SURVIVAL_PROVENANCE.json` | `bcb5575d9477` |
| `audio/survival_enemy_runner_contact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1262714248934 | `audio/SURVIVAL_PROVENANCE.json` | `96eb48937c75` |
| `audio/survival_enemy_runner_contact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1264961071441 | `audio/SURVIVAL_PROVENANCE.json` | `d558740d471b` |
| `audio/survival_enemy_runner_contact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1267207893948 | `audio/SURVIVAL_PROVENANCE.json` | `063401ad151f` |
| `audio/survival_enemy_runner_contact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1269454716455 | `audio/SURVIVAL_PROVENANCE.json` | `73d70819e57c` |
| `audio/survival_enemy_runner_contact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1271701538962 | `audio/SURVIVAL_PROVENANCE.json` | `23a72ee83478` |
| `audio/survival_enemy_runner_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1300910231553 | `audio/SURVIVAL_PROVENANCE.json` | `146fbaef4126` |
| `audio/survival_enemy_runner_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1303157054060 | `audio/SURVIVAL_PROVENANCE.json` | `3d90d47aa9b6` |
| `audio/survival_enemy_runner_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1305403876567 | `audio/SURVIVAL_PROVENANCE.json` | `bdec8d672740` |
| `audio/survival_enemy_runner_death_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1307650699074 | `audio/SURVIVAL_PROVENANCE.json` | `6579b9693811` |
| `audio/survival_enemy_runner_death_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1309897521581 | `audio/SURVIVAL_PROVENANCE.json` | `d3191af9d825` |
| `audio/survival_enemy_runner_windup_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1224518266315 | `audio/SURVIVAL_PROVENANCE.json` | `ee26db0a4e4e` |
| `audio/survival_enemy_runner_windup_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1226765088822 | `audio/SURVIVAL_PROVENANCE.json` | `3d69792bf335` |
| `audio/survival_enemy_runner_windup_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1229011911329 | `audio/SURVIVAL_PROVENANCE.json` | `ca30f97b6f0c` |
| `audio/survival_enemy_runner_windup_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1231258733836 | `audio/SURVIVAL_PROVENANCE.json` | `cf31722a00d9` |
| `audio/survival_enemy_runner_windup_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1233505556343 | `audio/SURVIVAL_PROVENANCE.json` | `1cae0453d01c` |
| `audio/survival_enemy_shambler_contact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1148126301077 | `audio/SURVIVAL_PROVENANCE.json` | `8c89d63dbc70` |
| `audio/survival_enemy_shambler_contact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1150373123584 | `audio/SURVIVAL_PROVENANCE.json` | `7048e34666b6` |
| `audio/survival_enemy_shambler_contact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1152619946091 | `audio/SURVIVAL_PROVENANCE.json` | `f203d6b24b28` |
| `audio/survival_enemy_shambler_contact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1154866768598 | `audio/SURVIVAL_PROVENANCE.json` | `926045b654e6` |
| `audio/survival_enemy_shambler_contact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1157113591105 | `audio/SURVIVAL_PROVENANCE.json` | `b77e4751a441` |
| `audio/survival_enemy_shambler_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1186322283696 | `audio/SURVIVAL_PROVENANCE.json` | `14ee08695feb` |
| `audio/survival_enemy_shambler_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1188569106203 | `audio/SURVIVAL_PROVENANCE.json` | `d0fb711f3004` |
| `audio/survival_enemy_shambler_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1190815928710 | `audio/SURVIVAL_PROVENANCE.json` | `887ab69735f0` |
| `audio/survival_enemy_shambler_death_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1193062751217 | `audio/SURVIVAL_PROVENANCE.json` | `7fa35db53f50` |
| `audio/survival_enemy_shambler_death_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1195309573724 | `audio/SURVIVAL_PROVENANCE.json` | `dd5388eff515` |
| `audio/survival_enemy_shambler_windup_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1109930318458 | `audio/SURVIVAL_PROVENANCE.json` | `04de4c10c7e9` |
| `audio/survival_enemy_shambler_windup_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1112177140965 | `audio/SURVIVAL_PROVENANCE.json` | `7ea98f63b350` |
| `audio/survival_enemy_shambler_windup_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1114423963472 | `audio/SURVIVAL_PROVENANCE.json` | `e56da3906e4e` |
| `audio/survival_enemy_shambler_windup_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1116670785979 | `audio/SURVIVAL_PROVENANCE.json` | `c44c15f27d85` |
| `audio/survival_enemy_shambler_windup_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1118917608486 | `audio/SURVIVAL_PROVENANCE.json` | `be5f779a51d4` |
| `audio/survival_enemy_spitter_contact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1835653988219 | `audio/SURVIVAL_PROVENANCE.json` | `e0415fda3ba6` |
| `audio/survival_enemy_spitter_contact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1837900810726 | `audio/SURVIVAL_PROVENANCE.json` | `4167a968bff8` |
| `audio/survival_enemy_spitter_contact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1840147633233 | `audio/SURVIVAL_PROVENANCE.json` | `4a8c98450a99` |
| `audio/survival_enemy_spitter_contact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1842394455740 | `audio/SURVIVAL_PROVENANCE.json` | `273c4530d483` |
| `audio/survival_enemy_spitter_contact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1844641278247 | `audio/SURVIVAL_PROVENANCE.json` | `1bf6729b0a33` |
| `audio/survival_enemy_spitter_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1873849970838 | `audio/SURVIVAL_PROVENANCE.json` | `51eb36025b7a` |
| `audio/survival_enemy_spitter_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1876096793345 | `audio/SURVIVAL_PROVENANCE.json` | `a0c565eb7121` |
| `audio/survival_enemy_spitter_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1878343615852 | `audio/SURVIVAL_PROVENANCE.json` | `0f921c154183` |
| `audio/survival_enemy_spitter_death_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1880590438359 | `audio/SURVIVAL_PROVENANCE.json` | `a52e4540d075` |
| `audio/survival_enemy_spitter_death_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1882837260866 | `audio/SURVIVAL_PROVENANCE.json` | `d0df5b7b590a` |
| `audio/survival_enemy_spitter_windup_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1797458005600 | `audio/SURVIVAL_PROVENANCE.json` | `6ff8950ea187` |
| `audio/survival_enemy_spitter_windup_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1799704828107 | `audio/SURVIVAL_PROVENANCE.json` | `96d1eff4786d` |
| `audio/survival_enemy_spitter_windup_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1801951650614 | `audio/SURVIVAL_PROVENANCE.json` | `ab249edbbfdb` |
| `audio/survival_enemy_spitter_windup_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1804198473121 | `audio/SURVIVAL_PROVENANCE.json` | `20e62182b3e6` |
| `audio/survival_enemy_spitter_windup_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1806445295628 | `audio/SURVIVAL_PROVENANCE.json` | `312b0a4e0169` |
| `audio/survival_enemy_swarm_contact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1721066040362 | `audio/SURVIVAL_PROVENANCE.json` | `e59a6e85c336` |
| `audio/survival_enemy_swarm_contact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1723312862869 | `audio/SURVIVAL_PROVENANCE.json` | `11d23dc698be` |
| `audio/survival_enemy_swarm_contact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1725559685376 | `audio/SURVIVAL_PROVENANCE.json` | `b45b0d76dc64` |
| `audio/survival_enemy_swarm_contact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1727806507883 | `audio/SURVIVAL_PROVENANCE.json` | `5622db57e19d` |
| `audio/survival_enemy_swarm_contact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1730053330390 | `audio/SURVIVAL_PROVENANCE.json` | `de6194bd98bf` |
| `audio/survival_enemy_swarm_death_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1759262022981 | `audio/SURVIVAL_PROVENANCE.json` | `ed752baf8e8d` |
| `audio/survival_enemy_swarm_death_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1761508845488 | `audio/SURVIVAL_PROVENANCE.json` | `f97af5d60ff3` |
| `audio/survival_enemy_swarm_death_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1763755667995 | `audio/SURVIVAL_PROVENANCE.json` | `a83cd72d0420` |
| `audio/survival_enemy_swarm_death_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1766002490502 | `audio/SURVIVAL_PROVENANCE.json` | `9b3393de8bcb` |
| `audio/survival_enemy_swarm_death_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1768249313009 | `audio/SURVIVAL_PROVENANCE.json` | `12c7c32291c2` |
| `audio/survival_enemy_swarm_windup_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1682870057743 | `audio/SURVIVAL_PROVENANCE.json` | `f0945631a4ae` |
| `audio/survival_enemy_swarm_windup_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1685116880250 | `audio/SURVIVAL_PROVENANCE.json` | `f99f77ad59d3` |
| `audio/survival_enemy_swarm_windup_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1687363702757 | `audio/SURVIVAL_PROVENANCE.json` | `cbc039c4a1cf` |
| `audio/survival_enemy_swarm_windup_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1689610525264 | `audio/SURVIVAL_PROVENANCE.json` | `478f3e6eb433` |
| `audio/survival_enemy_swarm_windup_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1691857347771 | `audio/SURVIVAL_PROVENANCE.json` | `b5788ea2b28d` |
| `audio/survival_explosion_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 460598613935 | `audio/SURVIVAL_PROVENANCE.json` | `f38db06a8c78` |
| `audio/survival_explosion_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 462845436442 | `audio/SURVIVAL_PROVENANCE.json` | `7558d8846f94` |
| `audio/survival_explosion_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 465092258949 | `audio/SURVIVAL_PROVENANCE.json` | `eed7957d4a84` |
| `audio/survival_explosion_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 467339081456 | `audio/SURVIVAL_PROVENANCE.json` | `41e92f19a48b` |
| `audio/survival_explosion_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 469585903963 | `audio/SURVIVAL_PROVENANCE.json` | `61c07625f111` |
| `audio/survival_fire_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 536990579173 | `audio/SURVIVAL_PROVENANCE.json` | `936191a52f86` |
| `audio/survival_fire_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 539237401680 | `audio/SURVIVAL_PROVENANCE.json` | `8e2338d8e332` |
| `audio/survival_fire_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 541484224187 | `audio/SURVIVAL_PROVENANCE.json` | `b00f51b05913` |
| `audio/survival_gun_bolt_cycle_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1071734335839 | `audio/SURVIVAL_PROVENANCE.json` | `9ef20215d97c` |
| `audio/survival_gun_bolt_cycle_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1073981158346 | `audio/SURVIVAL_PROVENANCE.json` | `86a505c0bb4d` |
| `audio/survival_gun_bolt_cycle_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1076227980853 | `audio/SURVIVAL_PROVENANCE.json` | `f578d4336f0b` |
| `audio/survival_gun_dryfire_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 918950405363 | `audio/SURVIVAL_PROVENANCE.json` | `46530f955f28` |
| `audio/survival_gun_dryfire_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 921197227870 | `audio/SURVIVAL_PROVENANCE.json` | `668052ba6aa0` |
| `audio/survival_gun_dryfire_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 923444050377 | `audio/SURVIVAL_PROVENANCE.json` | `98a15a521ffd` |
| `audio/survival_gun_dryfire_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 925690872884 | `audio/SURVIVAL_PROVENANCE.json` | `651e972a9340` |
| `audio/survival_gun_dryfire_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 927937695391 | `audio/SURVIVAL_PROVENANCE.json` | `a73b98ab35c9` |
| `audio/survival_gun_reload_in_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 995342370601 | `audio/SURVIVAL_PROVENANCE.json` | `603488ae72fa` |
| `audio/survival_gun_reload_in_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 997589193108 | `audio/SURVIVAL_PROVENANCE.json` | `825a36906614` |
| `audio/survival_gun_reload_in_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 999836015615 | `audio/SURVIVAL_PROVENANCE.json` | `722753141877` |
| `audio/survival_gun_reload_out_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 957146387982 | `audio/SURVIVAL_PROVENANCE.json` | `2e5f99eb38f6` |
| `audio/survival_gun_reload_out_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 959393210489 | `audio/SURVIVAL_PROVENANCE.json` | `8ce404b2e3ba` |
| `audio/survival_gun_reload_out_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 961640032996 | `audio/SURVIVAL_PROVENANCE.json` | `82b39f635fc5` |
| `audio/survival_gun_shell_insert_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 1033538353220 | `audio/SURVIVAL_PROVENANCE.json` | `ad0e89a1f340` |
| `audio/survival_gun_shell_insert_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 1035785175727 | `audio/SURVIVAL_PROVENANCE.json` | `843ed298f908` |
| `audio/survival_gun_shell_insert_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 1038031998234 | `audio/SURVIVAL_PROVENANCE.json` | `6ce7f2716d1b` |
| `audio/survival_gun_shell_insert_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 1040278820741 | `audio/SURVIVAL_PROVENANCE.json` | `16ed588b60ca` |
| `audio/survival_gun_shell_insert_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 1042525643248 | `audio/SURVIVAL_PROVENANCE.json` | `411515ce225a` |
| `audio/survival_impact_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 307814683459 | `audio/SURVIVAL_PROVENANCE.json` | `27ce95edc037` |
| `audio/survival_impact_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 310061505966 | `audio/SURVIVAL_PROVENANCE.json` | `6d4b14e5a588` |
| `audio/survival_impact_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 312308328473 | `audio/SURVIVAL_PROVENANCE.json` | `602b0861b2a0` |
| `audio/survival_impact_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 314555150980 | `audio/SURVIVAL_PROVENANCE.json` | `26ac7793b62d` |
| `audio/survival_impact_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 316801973487 | `audio/SURVIVAL_PROVENANCE.json` | `12f27fbd72bd` |
| `audio/survival_incoming_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 880754422744 | `audio/SURVIVAL_PROVENANCE.json` | `84fc46f97077` |
| `audio/survival_incoming_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 883001245251 | `audio/SURVIVAL_PROVENANCE.json` | `c94cb6bba45c` |
| `audio/survival_incoming_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 885248067758 | `audio/SURVIVAL_PROVENANCE.json` | `9bc6845d56ab` |
| `audio/survival_machinegun_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 193226735602 | `audio/SURVIVAL_PROVENANCE.json` | `68de9618eff9` |
| `audio/survival_machinegun_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 195473558109 | `audio/SURVIVAL_PROVENANCE.json` | `90fb5517d78d` |
| `audio/survival_machinegun_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 197720380616 | `audio/SURVIVAL_PROVENANCE.json` | `2ca6c95490b4` |
| `audio/survival_machinegun_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 199967203123 | `audio/SURVIVAL_PROVENANCE.json` | `5f89e7662e02` |
| `audio/survival_machinegun_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 202214025630 | `audio/SURVIVAL_PROVENANCE.json` | `f0a7c2d99960` |
| `audio/survival_marksman_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 155030752983 | `audio/SURVIVAL_PROVENANCE.json` | `0aa32d002669` |
| `audio/survival_marksman_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 157277575490 | `audio/SURVIVAL_PROVENANCE.json` | `ea9ca8f25006` |
| `audio/survival_marksman_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 159524397997 | `audio/SURVIVAL_PROVENANCE.json` | `3e0987e56f62` |
| `audio/survival_marksman_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 161771220504 | `audio/SURVIVAL_PROVENANCE.json` | `6f5a963d88c2` |
| `audio/survival_marksman_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 164018043011 | `audio/SURVIVAL_PROVENANCE.json` | `909befc5a279` |
| `audio/survival_melee_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 727970492268 | `audio/SURVIVAL_PROVENANCE.json` | `fb965326471f` |
| `audio/survival_melee_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 730217314775 | `audio/SURVIVAL_PROVENANCE.json` | `037a1cacd557` |
| `audio/survival_melee_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 732464137282 | `audio/SURVIVAL_PROVENANCE.json` | `a90c2ec80a02` |
| `audio/survival_melee_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 734710959789 | `audio/SURVIVAL_PROVENANCE.json` | `0954c9b1d1ee` |
| `audio/survival_melee_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 736957782296 | `audio/SURVIVAL_PROVENANCE.json` | `7b9143ca75d9` |
| `audio/survival_molotov_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 498794596554 | `audio/SURVIVAL_PROVENANCE.json` | `5b46188da5e8` |
| `audio/survival_molotov_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 501041419061 | `audio/SURVIVAL_PROVENANCE.json` | `17ecf89901d4` |
| `audio/survival_molotov_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 503288241568 | `audio/SURVIVAL_PROVENANCE.json` | `bf59fc42fafb` |
| `audio/survival_molotov_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 505535064075 | `audio/SURVIVAL_PROVENANCE.json` | `58a3b023fe00` |
| `audio/survival_molotov_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 507781886582 | `audio/SURVIVAL_PROVENANCE.json` | `42cf44a1638d` |
| `audio/survival_pistol_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2246822507 | `audio/SURVIVAL_PROVENANCE.json` | `727e37730c6d` |
| `audio/survival_pistol_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 4493645014 | `audio/SURVIVAL_PROVENANCE.json` | `a6dcd527db5d` |
| `audio/survival_pistol_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 6740467521 | `audio/SURVIVAL_PROVENANCE.json` | `e8a70c1ad72d` |
| `audio/survival_pistol_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 8987290028 | `audio/SURVIVAL_PROVENANCE.json` | `bb560b3000dc` |
| `audio/survival_pistol_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 11234112535 | `audio/SURVIVAL_PROVENANCE.json` | `cbe79c79082f` |
| `audio/survival_ready_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 766166474887 | `audio/SURVIVAL_PROVENANCE.json` | `88bd3233806e` |
| `audio/survival_ready_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 768413297394 | `audio/SURVIVAL_PROVENANCE.json` | `6311591e511d` |
| `audio/survival_ready_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 770660119901 | `audio/SURVIVAL_PROVENANCE.json` | `a3e5cf2c191e` |
| `audio/survival_ready_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 772906942408 | `audio/SURVIVAL_PROVENANCE.json` | `12da3b817d28` |
| `audio/survival_ready_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 775153764915 | `audio/SURVIVAL_PROVENANCE.json` | `b10f5a32fba3` |
| `audio/survival_reload_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 384206648697 | `audio/SURVIVAL_PROVENANCE.json` | `842f56dbcc95` |
| `audio/survival_reload_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 386453471204 | `audio/SURVIVAL_PROVENANCE.json` | `d3d84604143e` |
| `audio/survival_reload_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 388700293711 | `audio/SURVIVAL_PROVENANCE.json` | `89203f34a2c5` |
| `audio/survival_reload_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 390947116218 | `audio/SURVIVAL_PROVENANCE.json` | `48dd7b9bbb45` |
| `audio/survival_reload_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 393193938725 | `audio/SURVIVAL_PROVENANCE.json` | `dc5739c1512f` |
| `audio/survival_retreat_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 575186561792 | `audio/SURVIVAL_PROVENANCE.json` | `74855bbfb825` |
| `audio/survival_retreat_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 577433384299 | `audio/SURVIVAL_PROVENANCE.json` | `436499eb41f7` |
| `audio/survival_retreat_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 579680206806 | `audio/SURVIVAL_PROVENANCE.json` | `be88d71289b5` |
| `audio/survival_rifle_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 116834770364 | `audio/SURVIVAL_PROVENANCE.json` | `5a5f2252de38` |
| `audio/survival_rifle_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 119081592871 | `audio/SURVIVAL_PROVENANCE.json` | `1d6abf1d9f00` |
| `audio/survival_rifle_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 121328415378 | `audio/SURVIVAL_PROVENANCE.json` | `260624f784ad` |
| `audio/survival_rifle_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 123575237885 | `audio/SURVIVAL_PROVENANCE.json` | `c567344a1eb2` |
| `audio/survival_rifle_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 125822060392 | `audio/SURVIVAL_PROVENANCE.json` | `6e6874f6593c` |
| `audio/survival_shotgun_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 78638787745 | `audio/SURVIVAL_PROVENANCE.json` | `28045ca0396c` |
| `audio/survival_shotgun_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 80885610252 | `audio/SURVIVAL_PROVENANCE.json` | `479a79d18c22` |
| `audio/survival_shotgun_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 83132432759 | `audio/SURVIVAL_PROVENANCE.json` | `a39117876e75` |
| `audio/survival_shotgun_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 85379255266 | `audio/SURVIVAL_PROVENANCE.json` | `9032885b8411` |
| `audio/survival_shotgun_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 87626077773 | `audio/SURVIVAL_PROVENANCE.json` | `68c5a5fb3273` |
| `audio/survival_smg_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 40442805126 | `audio/SURVIVAL_PROVENANCE.json` | `eb2c4d8e9200` |
| `audio/survival_smg_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 42689627633 | `audio/SURVIVAL_PROVENANCE.json` | `eb222769f364` |
| `audio/survival_smg_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 44936450140 | `audio/SURVIVAL_PROVENANCE.json` | `31b652930468` |
| `audio/survival_smg_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 47183272647 | `audio/SURVIVAL_PROVENANCE.json` | `929379d22d11` |
| `audio/survival_smg_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 49430095154 | `audio/SURVIVAL_PROVENANCE.json` | `43fd0d7f81c8` |
| `audio/survival_structure_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 804362457506 | `audio/SURVIVAL_PROVENANCE.json` | `fe0750d7f52d` |
| `audio/survival_structure_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 806609280013 | `audio/SURVIVAL_PROVENANCE.json` | `184f262c6cdd` |
| `audio/survival_structure_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 808856102520 | `audio/SURVIVAL_PROVENANCE.json` | `bf957f1b7cf5` |
| `audio/survival_throw_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 422402631316 | `audio/SURVIVAL_PROVENANCE.json` | `7533d689e41d` |
| `audio/survival_throw_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 424649453823 | `audio/SURVIVAL_PROVENANCE.json` | `ebaf107f2eb0` |
| `audio/survival_throw_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 426896276330 | `audio/SURVIVAL_PROVENANCE.json` | `1c5bcc1061f9` |
| `audio/survival_throw_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 429143098837 | `audio/SURVIVAL_PROVENANCE.json` | `1d12fcdfd104` |
| `audio/survival_throw_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 431389921344 | `audio/SURVIVAL_PROVENANCE.json` | `e43173104264` |
| `audio/survival_trap_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 842558440125 | `audio/SURVIVAL_PROVENANCE.json` | `dfce3be0bc7c` |
| `audio/survival_trap_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 844805262632 | `audio/SURVIVAL_PROVENANCE.json` | `501659761079` |
| `audio/survival_trap_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 847052085139 | `audio/SURVIVAL_PROVENANCE.json` | `866ea678f386` |
| `audio/survival_trap_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 849298907646 | `audio/SURVIVAL_PROVENANCE.json` | `f40f1251f2b8` |
| `audio/survival_trap_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 851545730153 | `audio/SURVIVAL_PROVENANCE.json` | `d0fa80be1ddd` |
| `audio/survival_turret_shot_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 269618700840 | `audio/SURVIVAL_PROVENANCE.json` | `2c21ce0988c2` |
| `audio/survival_turret_shot_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 271865523347 | `audio/SURVIVAL_PROVENANCE.json` | `8248747a047d` |
| `audio/survival_turret_shot_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 274112345854 | `audio/SURVIVAL_PROVENANCE.json` | `9184cd00f38b` |
| `audio/survival_turret_shot_3.wav` | crates/horde_audio/examples/export_audio.rs, seed 276359168361 | `audio/SURVIVAL_PROVENANCE.json` | `29223276dbb7` |
| `audio/survival_turret_shot_4.wav` | crates/horde_audio/examples/export_audio.rs, seed 278605990868 | `audio/SURVIVAL_PROVENANCE.json` | `b12f22897963` |
| `audio/survival_ui_coin_batch_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2408593727504 | `audio/SURVIVAL_PROVENANCE.json` | `b46c9bd76aed` |
| `audio/survival_ui_coin_batch_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2410840550011 | `audio/SURVIVAL_PROVENANCE.json` | `b6958706d646` |
| `audio/survival_ui_coin_batch_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2413087372518 | `audio/SURVIVAL_PROVENANCE.json` | `adbc84849bbe` |
| `audio/survival_ui_loot_reveal_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 2370397744885 | `audio/SURVIVAL_PROVENANCE.json` | `521a38a3faba` |
| `audio/survival_ui_loot_reveal_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 2372644567392 | `audio/SURVIVAL_PROVENANCE.json` | `59c404f852f6` |
| `audio/survival_ui_loot_reveal_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 2374891389899 | `audio/SURVIVAL_PROVENANCE.json` | `06416291c679` |
| `audio/survival_wave_0.wav` | crates/horde_audio/examples/export_audio.rs, seed 651578527030 | `audio/SURVIVAL_PROVENANCE.json` | `c4e09e06ff09` |
| `audio/survival_wave_1.wav` | crates/horde_audio/examples/export_audio.rs, seed 653825349537 | `audio/SURVIVAL_PROVENANCE.json` | `6e51b2bc0ec9` |
| `audio/survival_wave_2.wav` | crates/horde_audio/examples/export_audio.rs, seed 656072172044 | `audio/SURVIVAL_PROVENANCE.json` | `f298a438c7f9` |
<!-- asset-index:end -->
