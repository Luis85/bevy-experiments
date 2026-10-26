# Actor Testbed assets: measured (2026-10-24)

Fetched by `scripts/fetch_assets.sh`; sources and licences in `assets/CREDITS.md` (all CC0); research in
`docs/research/testbed-assets.md`. Numbers below come from the glTF JSON: node TRS applied to every
POSITION accessor's min/max (skinned meshes in bind pose). Units are raw model units, +Y up.

## Scale convention

- **KayKit is 1:1** with the characters: Knight 2.315 tall (2.467 with helmet), Rogue_Hooded 2.251,
  Skeletons 2.166 (Warrior 2.590 with helmet). Feet at y = 0.
- "×KK" below = factor to bring a model to KayKit 1:1. Then apply one view factor **V** to everything:
  - `V = 0.75`: grid-friendly (a 4-unit Dungeon wall = exactly 3 one-metre cells; Knight = 1.74 m).
  - `V = 0.779`: Knight = 1.80 m exactly, but walls become 3.12 m (off-grid).
  - Sim (mm) = units × ×KK × V × 1000. E.g. wall width 4 × 1 × 0.75 = 3000 mm.

## Table

| Asset | Pack | Key nodes | Extent x × y × z | Base y | ×KK | External textures |
|---|---|---|---|---|---|---|
| `testbed/characters/Knight.glb` | Adventurers | 76 clips; embedded `1H_Sword`, `2H_Sword`, `1H_Sword_Offhand`, `*_Shield`, `Knight_Helmet`, `Knight_Cape` | 2.32 tall (bare head; 2.47 helmet) | 0 | 1 | none (embedded) |
| `testbed/characters/Skeleton_Minion.glb` | Skeletons | 95 clips, same rig; no weapons | 1.94 × 2.17 × 0.91 | 0 | 1 | none |
| `testbed/characters/Skeleton_Warrior.glb` | Skeletons | 95 clips; `Skeleton_Warrior_Helmet` | 1.94 × 2.59 × 1.46 | 0 | 1 | none |
| `testbed/props/Skeleton_Axe.gltf` | Skeletons | `Skeleton_Axe` (origin at grip) | 0.99 × 1.25 × 0.28 | -0.25 | 1 | `skeleton_texture.png` + `.bin` (same dir) |
| `testbed/dungeon/wall.gltf.glb` | Dungeon R. | `wall`; centred in x/z | 4 × 4 × 1 | 0 | 1 | none (all dungeon files embed the atlas) |
| `testbed/dungeon/wall_half.gltf.glb` | Dungeon R. | x 0..2 (origin at one end) | 2 × 4 × 1 | 0 | 1 | none |
| `testbed/dungeon/wall_corner.gltf.glb` | Dungeon R. | L: x -2..0.5, z -0.5..2 | 2.5 × 4 × 2.5 | 0 | 1 | none |
| `testbed/dungeon/wall_endcap.gltf.glb` | Dungeon R. | x 0..1.067 | 1.07 × 4 × 1 | 0 | 1 | none |
| `testbed/dungeon/wall_doorway.glb` | Dungeon R. | `wall_doorway` > `wall_doorway_door` | 4 × 4 × 1 | 0 | 1 | none |
| `testbed/dungeon/floor_tile_large.gltf.glb` | Dungeon R. | top at y 0.05 | 4 × 0.15 × 4 | -0.10 | 1 | none |
| `testbed/dungeon/chest.glb` | Dungeon R. | `chest` > `chest_lid` | 1.70 × 1.30 × 1.45 | 0 | 1 | none |
| `testbed/dungeon/key.gltf.glb` | Dungeon R. | upright in XY, origin at ring | 0.90 × 0.40 × 0.14 | -0.20 | 1 | none |
| `testbed/dungeon/plate_food_A.gltf.glb` | Dungeon R. | food item | 1.03 × 0.79 × 0.97 | 0 | 1 (×0.5 in hand) | none |
| `testbed/dungeon/box_large.gltf.glb` | Dungeon R. | crate | 1.5 × 1.5 × 1.5 | 0 | 1 | none |
| `testbed/dungeon/barrel_small.gltf.glb` | Dungeon R. | barrel | 1.0 × 1.02 × 1.0 | 0 | 1 | none |
| `testbed/dungeon/coin_stack_small.gltf.glb` | Dungeon R. | **ingot stand-in** | 0.96 × 0.48 × 0.90 | 0 | 0.6 | none |
| `hex/tree_single_A.gltf` / `_B` | Hex | tree | 0.57 × 1.20 × 0.55 / 0.69 × 1.21 × 0.72 | -0.10 | 4 | `hexagons_medieval.png` |
| `hex/tree_single_A_cut.gltf` / `_B_cut` | Hex | stump (top 0.228 / 0.137) | 0.17 × 0.33 × 0.16 / 0.18 × 0.24 × 0.19 | -0.10 | 4 | `hexagons_medieval.png` |
| `hex/resource_stone.gltf` | Hex | ore | 0.42 × 0.28 × 0.36 | 0 | 4 | `hexagons_medieval.png` |
| `hex/resource_lumber.gltf`, `hex/sack.gltf` | Hex | logs / sack | 0.69 × 0.21 × 0.33 / 0.11 × 0.07 × 0.16 | 0 | 4 | `hexagons_medieval.png` |
| `furniture/book_single.gltf` | Furniture | upright, centred on origin | 0.26 × 0.50 × 0.37 | -0.25 | 1 | `furniturebits_texture.png` |
| `testbed/survival/workbench.glb` | Kenney Survival | children `paper`, `hammer` | 0.33 × 0.29 × 0.30 | 0 | 4.5 | `Textures/colormap.png` |
| `testbed/survival/workbench-anvil.glb` | Kenney Survival | child `hammer` | 0.32 × 0.34 × 0.30 | 0 | 4.5 | `Textures/colormap.png` |
| `testbed/survival/campfire-pit.glb` | Kenney Survival | child `wood`; **forge stand-in** | 0.28 × 0.11 × 0.27 | 0 | 4.5 | `Textures/colormap.png` |
| `testbed/survival/tool-pickaxe.glb` | Kenney Survival | upright, base = handle end | 0.18 × 0.24 × 0.04 | 0 | 4.7 | `Textures/colormap.png` |
| `testbed/survival/tool-hammer.glb` | Kenney Survival | upright, base = handle end | 0.09 × 0.15 × 0.04 | 0 | 4.7 | `Textures/colormap.png` |
| `testbed/survival/tree-log.glb` | Kenney Survival | log along z | 0.25 × 0.28 × 1.0 | -0.11 | 2 carried / 4.5 pile | `Textures/colormap.png` |
| `testbed/survival/resource-planks.glb` | Kenney Survival | planks | 0.37 × 0.09 × 0.63 | 0 | 4.5 | `Textures/colormap.png` |
| `testbed/survival/resource-wood.glb` | Kenney Survival | small wood bundle | 0.21 × 0.06 × 0.09 | 0 | 4.5 | `Textures/colormap.png` |
| `testbed/survival/bedroll-packed.glb` | Kenney Survival | **scroll stand-in** (rolled cylinder along x) | 0.26 × 0.09 × 0.09 | 0 | 1.5 | `Textures/colormap.png` |
| `testbed/castle/siege-ram.glb` | Kenney Castle | `siege-ram` > `ram`, 6 × `wheel` | 2.09 × 1.18 × 1.00 | 0 | 2.5 | `Textures/colormap.png` |
| `testbed/castle/siege-catapult.glb` | Kenney Castle | `siege-catapult` > `catapult`, 4 × `wheel` | 1.74 × 1.33 × 1.21 | **-0.149** | 2.5 | `Textures/colormap.png` |
| `testbed/town/cart.glb` | Kenney Town 2.0 | `cart` > 2 × `Group` (wheels) | 0.89 × 0.54 × 1.34 | 0 | 3 | `Textures/colormap.png` |

Knight's full bbox (2.69 × 2.47 × 2.58) is inflated by the embedded weapons; hide the unused weapon nodes.
Skeleton_Axe goes on bone `handslot.r` (same rig as Adventurers).

## Texture layout

- Dungeon `.glb` / `.gltf.glb` and all character `.glb`: atlas embedded, no external files.
- KayKit `.gltf`: `.bin` + shared atlas in the same folder (`hex/hexagons_medieval.png`,
  `furniture/furniturebits_texture.png`, `testbed/props/skeleton_texture.png`). Stumps, stone and book were
  added to the existing `hex/` and `furniture/` folders so the atlases are not duplicated.
- Kenney GLBs: the JSON image uri is exactly `Textures/colormap.png` (relative). Each kit has its **own**
  colormap (different sizes/palettes), so each kit folder has its own `Textures/colormap.png`:
  `testbed/survival/Textures/`, `testbed/castle/Textures/`, `testbed/town/Textures/`. Do not merge them.

## Pivots (measured node transforms)

- **Door**: `wall_doorway_door`, child of `wall_doorway`, t = (-0.82, 0, 0), no rotation. Leaf spans
  x -1.0..1.0 (2.0 wide), y 0..2.75, z ±0.39 in model space; its origin sits 0.18 inside the left edge,
  i.e. on the hinge line. Rotate the node about local Y to open.
- **Chest lid**: `chest_lid`, child of `chest`, t = (0, 0.5, -0.565) = back edge (hinge). The lid
  extends toward +z, so open by rotating about local X by a **negative** angle (about -100°).
- **Cart wheels** (`Group` × 2, t = (0.447, 0, -0.039) and (-0.325, 0, -0.039)): origin at a corner of
  the wheel bbox. Wheel centre = node origin + (-0.061, 0.234, 0.234); radius 0.234; axle along X. Insert
  a pivot entity at that offset (the report's "(0, 0.235, 0.235)" is confirmed; x is half the tyre width).
- **Ram wheels** (`wheel` × 6, e.g. t = (0.673, 0, -0.5)): origin at a rim corner. Centre = node origin +
  (-0.2015, 0.2015, 0.0665) for all six; radius 0.2015; axle along Z. Needs re-pivoting.
- **Ram beam**: `ram` t = (-0.02, 0.454, 0), spans x -1.06..1.02; animate by translating along X.
- **Catapult wheels** (`wheel` × 4): centred on the hub (t = (±…, 0.052, ±0.538), radius 0.2015, axle
  Z), spin directly. They reach y = -0.149, so lift the whole model by 0.149 to stand on the ground.
  Arm `catapult` t = (0.419, 0.177, 0), origin at the arm's +x end (the axle): rotate about Z.

## Gaps and stand-ins

- **Ingot**: no CC0 ingot; `coin_stack_small` (Dungeon) or a tinted procedural cuboid.
- **Scroll**: no CC0 scroll; `bedroll-packed` (a rolled cylinder, ×1.5 ≈ 0.39 long) or `book_single`.
- **Forge**: `campfire-pit` (with `wood` child) next to `workbench-anvil`.
- Kenney models are flatter-shaded than KayKit; the mix is accepted for the testbed.
