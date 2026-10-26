# Free asset sources (researched 2026-10-23)

All CC0 (LICENSE.txt checked in each repo). KayKit repos on GitHub are plain files (no LFS) — raw URLs download with curl, no login.

## Character — KayKit Character Pack: Adventurers 1.0
- Repo: https://github.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0
- GLB (one per character, animations reportedly embedded, ~3.6 MB):
  `https://raw.githubusercontent.com/KayKit-Game-Assets/KayKit-Character-Pack-Adventures-1.0/main/addons/kaykit_character_pack_adventures/Characters/gltf/<Name>.glb`
  `<Name>` ∈ Barbarian, Knight, Mage, Rogue, Rogue_Hooded. Villager pick: **Rogue_Hooded** (fallback Barbarian).
- Props in same repo (`.../Assets/gltf/`): `axe_1handed.gltf`, `mug_full.gltf` (+ .bin, png).
- Clip names (from a third-party import — **verify by dumping the GLB**): Idle, Unarmed_Idle, Walking_A/B/C, Running_A/B, 1H_Melee_Attack_Chop, 2H_Melee_Attack_Chop, Interact, PickUp, Use_Item, Lie_Down, Lie_Idle, Lie_StandUp, Sit_Floor_Down/Idle/StandUp, Sit_Chair_*, Cheer, Death_A/B, Hit_A/B …
- Hand bone for attaching tools: `handslot.r` (reported).

## Props — KayKit Medieval Hexagon Pack 1.0
- Repo: https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0
- Base: `https://raw.githubusercontent.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0/main/addons/kaykit_medieval_hexagon_pack/Assets/gltf/`
  Each model = `.gltf` + `.bin` + the folder's `hexagons_medieval.png` atlas.
- Zip: `https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0/archive/refs/heads/main.zip` (~65 MB repo; gltf subset ~12.7 MB)
- Wanted files:
  - House: `buildings/<color>/building_home_A_<color>.gltf` (color ∈ blue, green, red, yellow)
  - Field: `buildings/neutral/building_grain.gltf`, `building_dirt.gltf`
  - Trees: `decoration/nature/tree_single_A.gltf`, `tree_single_B.gltf`, `_cut` variants, `trees_A_small/medium/large`
  - Wood: `decoration/props/resource_lumber.gltf`
  - Stockpile: `decoration/props/sack.gltf`, `barrel.gltf`, `crate_A_big.gltf`, `crate_open.gltf`, `pallet.gltf`, `wheelbarrow.gltf`
  - Dressing: `buildings/neutral/fence_wood_straight.gltf`, `decoration/nature/rock_single_A..E.gltf`
  - Optional ground: `tiles/base/hex_grass.gltf`
- Scale is hex-tile scale — check character/house ratio after import.

## Bed — KayKit Furniture Bits 1.0
- Repo: https://github.com/KayKit-Game-Assets/KayKit-Furniture-Bits-1.0
- `addons/kaykit_furniture_bits/Assets/gltf/bed_single_A.gltf` (+ .bin + png)

## Ground texture
- ambientCG Grass004 (CC0): `https://ambientcg.com/get?file=Grass004_1K-JPG.zip` (~10 MB). Photo texture — tint/desaturate to match KayKit's flat style, or use a solid-colour plane.
- Poly Haven alt: `https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/aerial_grass_rock/aerial_grass_rock_diff_1k.jpg`

## Fallback
Kenney (CC0, direct zips; hash segment in URL changes on update — read from page): Survival Kit, Nature Kit, Mini Characters. Animation set unverified.
Quaternius packs need manual itch.io clicks.
