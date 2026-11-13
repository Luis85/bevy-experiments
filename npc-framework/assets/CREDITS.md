# Asset credits

The models and textures below are **CC0 1.0 (public domain)**. Re-download them with `scripts/fetch_assets.sh`.
The bundled Barlow fonts (Regular and SemiBold) use SIL Open Font License 1.1; its complete license is included alongside them.

| Folder | Source | Author | License |
|---|---|---|---|
| `fonts/Barlow-Regular.ttf` | [Barlow](https://github.com/jpt/barlow) | The Barlow Project Authors | [SIL OFL 1.1](fonts/OFL.txt) |
| `fonts/Barlow-SemiBold.ttf` | [Barlow](https://github.com/jpt/barlow) | The Barlow Project Authors | [SIL OFL 1.1](fonts/OFL.txt) |
| `characters/Rogue_Hooded.glb`, `props/` | [KayKit Character Pack: Adventurers 1.0](https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0) | Kay Lousberg (kaylousberg.com) | CC0 |
| `hex/` | [KayKit Medieval Hexagon Pack 1.0](https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0) | Kay Lousberg | CC0 |
| `furniture/` | [KayKit Furniture Bits 1.0](https://github.com/KayKit-Game-Assets/KayKit-Furniture-Bits-1.0) | Kay Lousberg | CC0 |
| `textures/grass_color.jpg` | [ambientCG Grass004](https://ambientcg.com/view?id=Grass004) | Lennart Demes (ambientCG) | CC0 |
| `testbed/characters/Knight.glb` | [KayKit Character Pack: Adventurers 1.0](https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0) | Kay Lousberg (kaylousberg.com) | CC0 |
| `testbed/characters/Skeleton_*.glb`, `testbed/props/` | [KayKit Character Pack: Skeletons 1.0](https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Skeletons-1.0) | Kay Lousberg (kaylousberg.com) | CC0 |
| `testbed/dungeon/` | [KayKit Dungeon Remastered 1.0](https://github.com/KayKit-Game-Assets/KayKit-Dungeon-Remastered-1.0) | Kay Lousberg (kaylousberg.com) | CC0 |
| `testbed/survival/` | [Kenney Survival Kit](https://kenney.nl/assets/survival-kit) | Kenney (kenney.nl) | CC0 |
| `testbed/castle/` | [Kenney Castle Kit](https://kenney.nl/assets/castle-kit) | Kenney (kenney.nl) | CC0 |
| `testbed/town/` | [Kenney Fantasy Town Kit 2.0](https://kenney.nl/assets/fantasy-town-kit) | Kenney (kenney.nl) | CC0 |

## Living world expansion

Curated model files are listed with exact sources, download URLs, byte counts and SHA-256
hashes in `living-world-manifest.json`. Reproduce/verify them with
`python scripts/fetch_living_assets.py`. No paid-tier files are included.

- `living/buildings/`, `living/nature/`: Kay Lousberg, KayKit Medieval Hexagon 1.0, CC0.
  Includes actual blacksmith, mill, market, mine, tavern, well, tower and second-house
  meshes. Original pack license is retained under `licenses/`.
- `living/characters/{Barbarian,Mage,Rogue}.glb`: Kay Lousberg, KayKit Adventurers 1.0,
  CC0; original pack license retained under `licenses/`.
- `living/characters/{VillageWoman,TravellerWoman,FarmerMan,TravellerMan}.glb`:
  Quaternius, Ultimate Modular Women/Men, CC0. Downloaded from the creator's own
  Poly Pizza uploads, each individually marked Public Domain (CC0):
  [dress](https://poly.pizza/m/nIItLV9nxS),
  [casual woman](https://poly.pizza/m/qJ2gsTUBHL),
  [farmer](https://poly.pizza/m/7pn3R6hPvE),
  [adventurer](https://poly.pizza/m/5EGWBMpuXq).
  Their imported meshes and embedded animations are unchanged.
  [CC0 1.0 terms](https://creativecommons.org/publicdomain/zero/1.0/).

The four civilian GLBs contain four or five skinned body/accessory meshes and 24
embedded animation clips. Their rig is distinct from KayKit; the view uses separate
clip aliases and normalized wrist sockets, without automatic retargeting.

### Ships, animals, nature and village props (2026-11-12)

All **CC0 1.0**. Sources, evidence pages, archive hashes and changes are recorded in
`licenses/Living-World-Ships-Animals-Nature-CC0-sources.txt`; per-file URLs, sizes and
SHA-256 hashes are in `living-world-manifest.json`. These files are not fetched by
`scripts/fetch_living_assets.py`; it keeps their manifest entries unchanged.

| Folder | Source | Author | License | Changes |
|---|---|---|---|---|
| `living/nature/{trees_*_large,trees_*_medium,trees_*_cut,rock_single_A,rock_single_C,waterlily_B,waterplant_C}.gltf`, `living/props/*.gltf` | [KayKit Medieval Hexagon Pack 1.0](https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0), commit `84fa4e91` | Kay Lousberg | [CC0](licenses/KayKit-Medieval-Hexagon-Pack-1.0.txt) | none |
| `living/ships/*.glb`, `living/ships/Textures/colormap.png` | [Pirate Kit 2.1](https://opengameart.org/content/pirate-kit) (`kenney_pirate-kit_2.1.zip`) | Kenney (kenney.nl) | [CC0](licenses/Kenney-Pirate-Kit-2.1.txt) | none |
| `living/nature/*.glb`, `living/props/{fence_simple,fence_gate,sign,crops_wheatStageA,crops_wheatStageB}.glb` | [Nature Kit 2.1](https://opengameart.org/content/nature-kit) (`Nature Kit (2.1).zip`) | Kenney (kenney.nl) | [CC0](licenses/Kenney-Nature-Kit-2.1.txt) | none |
| `living/ships/{Prop_Anchor,Environment_Dock_Pole,Prop_Fish_Mackerel,Prop_Bucket_Fishes}.gltf` | [Pirate Kit](https://quaternius.com/packs/piratekit.html) (creator's linked Google Drive) | Quaternius | CC0 (stated on the pack page) | none |
| `living/animals/{Deer,Stag}.gltf` | [Ultimate Animated Animal Pack](https://quaternius.com/packs/ultimateanimatedanimals.html) (creator's linked Google Drive) | Quaternius | [CC0](licenses/Quaternius-Ultimate-Animated-Animal-Pack.txt) | none |

Kenney's two kits were taken from Kenney's own OpenGameArt account, as for the audio.
Nothing from Kenney's Watercraft Kit is included (its boats are modern).

### Living-world audio (2026-11-13)

All **CC0 1.0**, Ogg Vorbis, used unchanged. Each comes from its author's own OpenGameArt
page, whose licence field lists CC0 only. Sources, evidence pages and download hashes are in
`licenses/Living-World-Audio-CC0-sources.txt`; per-file URLs, sizes and SHA-256 hashes are in
`living-world-manifest.json`. These files are not fetched by `scripts/fetch_living_assets.py`.

| Files | Source | Author | License | Changes |
|---|---|---|---|---|
| `living/audio/ui/{click,hover,open,close,confirm,error,place,alert}.ogg` | [Interface Sounds 1.0](https://opengameart.org/content/interface-sounds) | Kenney (kenney.nl) | [CC0](licenses/Kenney-Interface-Sounds-1.0.txt) | none |
| `living/audio/ui/{coin,coin_2,page}.ogg`, `living/audio/work/{chop_1,cloth_1,cloth_2,door_open,door_close,step_dirt_1,step_dirt_2}.ogg` | [50 RPG sound effects](https://opengameart.org/content/50-rpg-sound-effects) | Kenney (kenney.nl) | [CC0](licenses/Kenney-RPG-Audio.txt) | none |
| `living/audio/music/{jingle_morning,jingle_built}.ogg` | [85 Short music jingles](https://opengameart.org/content/85-short-music-jingles) | Kenney (kenney.nl) | [CC0](licenses/Kenney-Music-Jingles.txt) | none |
| `living/audio/work/{hammer_1,hammer_2,metal_1,metal_2}.ogg` | [100 CC0 metal and wood SFX](https://opengameart.org/content/100-cc0-metal-and-wood-sfx) | rubberduck | CC0 | none |
| `living/audio/work/{chop_2,chop_3,stone_1,stone_2,stone_3}.ogg`, `living/audio/ambience/thunder.ogg` | [100 CC0 SFX #2](https://opengameart.org/content/100-cc0-sfx-2) | rubberduck | CC0 | none |
| `living/audio/work/saw_loop.ogg` | [30 CC0 SFX loops](https://opengameart.org/content/30-cc0-sfx-loops) | rubberduck | CC0 | none |
| `living/audio/work/{splash_1,splash_2,splash_3}.ogg` | [40 CC0 water / splash / slime SFX](https://opengameart.org/content/40-cc0-water-splash-slime-sfx) | rubberduck | CC0 | none |
| `living/audio/work/{step_grass_1,step_grass_2,step_wood_1,step_wood_2,step_wood_3,dig_1,dig_2}.ogg` | [Different steps on wood, stone, leaves, gravel and mud](https://opengameart.org/content/different-steps-on-wood-stone-leaves-gravel-and-mud) | TinyWorlds | CC0 | none |
| `living/audio/work/{eat_1,eat_2}.ogg` | [7 Eating Crunches](https://opengameart.org/content/7-eating-crunches) | StarNinjas | CC0 | none |
| `living/audio/work/saw.ogg` | [Saw or Wood Impact](https://opengameart.org/content/saw-or-wood-impact) | StarNinjas | CC0 | none |
| `living/audio/work/tree_fall.ogg` | [tree chop fall thud](https://opengameart.org/content/tree-chop-fall-thud) | kheetor | CC0 | none |
| `living/audio/ambience/rain.ogg` | [Rain (loopable)](https://opengameart.org/content/rain-loopable) | Ylmir | CC0 | none |
| `living/audio/ambience/storm.ogg` | [Rain + Long Thunder](https://opengameart.org/content/rain-long-thunder) | WuxiaScrub | CC0 | none |
| `living/audio/ambience/birds.ogg` | [Ambient Bird Sounds](https://opengameart.org/content/ambient-bird-sounds) | isaiah658 | CC0 | none |
| `living/audio/ambience/birds_morning.ogg` | [AMB Morning Sounds (Perfect Loop)](https://opengameart.org/content/amb-morning-sounds-perfect-loop) | Kresiek The Furry | CC0 | none |
| `living/audio/ambience/wind.ogg` | [Wind](https://opengameart.org/content/wind) | IgnasD | CC0 | none |
| `living/audio/ambience/fire.ogg` | [Fire Crackling](https://opengameart.org/content/fire-crackling) | AntumDeluge | CC0 | none |
| `living/audio/music/day.ogg` | [Meadow Thoughts](https://opengameart.org/content/meadow-thoughts) | Écrivain | CC0 | none |
| `living/audio/music/night.ogg` | [Forest Whisper Theme](https://opengameart.org/content/forest-whisper-theme) | Cleyton Kauffman | CC0 | none |

## Island audio

All sound files under `audio/` are **CC0 1.0 (public domain)**. Each was downloaded on
2026-11-10 from its author's own OpenGameArt page, whose licence field lists CC0 only.
Per-file sources, changes and SHA-256 hashes are recorded in
`licenses/Island-Audio-CC0-sources.txt`; the two Kenney pack licence files are retained
under `licenses/`.

| File | Source | Author | License | Changes |
|---|---|---|---|---|
| `audio/kenney-rpg-audio/{chop,creak1,creak2,metalClick,metalPot2,metalPot3}.ogg` | [50 RPG sound effects](https://opengameart.org/content/50-rpg-sound-effects) (`RPGsounds_Kenney.zip`) | Kenney (kenney.nl) | CC0 | none |
| `audio/kenney-interface-sounds/click_001.ogg` | [Interface Sounds 1.0](https://opengameart.org/content/interface-sounds) (`kenney_interfaceSounds.zip`) | Kenney (kenney.nl) | CC0 | none |
| `audio/ambience/birds-isaiah658.ogg` | [Ambient Bird Sounds](https://opengameart.org/content/ambient-bird-sounds) | isaiah658 | CC0 | none |
| `audio/ambience/forest-tinyworlds.ogg` | [Forest Ambience](https://opengameart.org/content/forest-ambience) (`Forest_Ambience.mp3`) | TinyWorlds | CC0 | MP3 to Ogg Vorbis, +18 dB, 50 ms edge fades |
| `audio/ambience/crickets-wolfgang.ogg` | [Crickets Ambient Noise - loopable](https://opengameart.org/content/crickets-ambient-noise-loopable) (`crickets-oneloop.mp3`) | Wolfgang | CC0 | MP3 to Ogg Vorbis, 50 ms edge fades |
| `audio/ambience/waves-jasinski.ogg` | [Beach Ocean Waves](https://opengameart.org/content/beach-ocean-waves) (`jasinski-wave-prev.ogg`), from [Freesound 18363](https://freesound.org/people/jasinski/sounds/18363/) | jasinski (packaged by qubodup) | CC0 | re-encoded with 0.6 s edge fades |

Kenney's own site serves these packs through a signed, browser-only download link, so the
identical packs were taken from Kenney's OpenGameArt account instead. "Impact Sounds" and
"UI Audio" were not fetched; nothing of unclear licence is included.

## Interface icons

The interface pictograms under `ui/icons/` are **CC0 1.0 (public domain)**. The 92 SVG
sources were fetched on 2026-11-11 by `scripts/fetch_ui_icons.py` from the two pinned
commits below and are kept unchanged; the script checks that each licence text opens with
"CC0 1.0 Universal" before it keeps anything. Per-file URLs, sizes and SHA-256 hashes, the
atlas cell of each icon and the hashes of the two atlases are in `ui-icons-manifest.json`;
the licence texts are `licenses/Maki-8.0.0-CC0.txt` and `licenses/Temaki-5.13.0-CC0.txt`.

| Folder | Source | Author | License | Changes |
|---|---|---|---|---|
| `ui/icons/src/maki/` (41 files) | [Maki 8.0.0](https://github.com/mapbox/maki), commit `070ed0af1012da8dee83593bb0a33e4592ed68f5` | Mapbox and contributors | CC0 | none |
| `ui/icons/src/temaki/` (51 files) | [Temaki 5.13.0](https://github.com/rapideditor/temaki), commit `ff12eb30f7f91b4c0e17594f8c548be29aa03f4b` | Rapid editor contributors | CC0 | none |
| `ui/icons/icons-32.png`, `ui/icons/icons-64.png` | the 92 sources above | as above | CC0 | rasterised to white PNG atlases at 32 and 64 px |

Nine glyphs with no suitable pictogram (pause, menu, tick, coin, sun, cloud, fog, flatten,
smooth) are drawn in code and are not assets.
