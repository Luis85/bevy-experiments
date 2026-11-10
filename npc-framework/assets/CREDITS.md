# Asset credits

The models and textures below are **CC0 1.0 (public domain)**. Re-download them with `scripts/fetch_assets.sh`.
The bundled Barlow speech font uses SIL Open Font License 1.1; its complete license is included alongside it.

| Folder | Source | Author | License |
|---|---|---|---|
| `fonts/Barlow-Regular.ttf` | [Barlow](https://github.com/jpt/barlow) | The Barlow Project Authors | [SIL OFL 1.1](fonts/OFL.txt) |
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
