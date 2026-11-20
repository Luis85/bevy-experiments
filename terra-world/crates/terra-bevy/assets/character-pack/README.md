# Original Fieldhand character

This original articulated human is a developer handoff example under
**MIT OR Apache-2.0**, authored from code without downloaded models, textures,
fonts or motion capture. The script reuses the project's original Comet GLB
writer, then creates its own geometry, materials, hierarchy and four clips. It
also generates the separate driver-cabin variant without modifying the original
Comet files. `--check` compares all five generated outputs.

```sh
python3 crates/terra-bevy/assets/character-pack/generate.py --check
cargo run -p terra-bevy --no-default-features --bin terra-assets -- inspect crates/terra-bevy/assets/character-pack/fieldhand.glb
cargo run -p terra-bevy --no-default-features --bin terra-assets -- validate crates/terra-bevy/assets/character-demo.pack.json
cargo run -p terra-demo -- --play --asset-pack crates/terra-bevy/assets/character-demo.pack.json
```

The native character creator and procedural fallback retain all three body
styles and eight skin, hair and outfit choices. The imported model reads the
same saved profile and uses the same sRGB palette and conversion to linear scene colors. It owns three tint
materials per instance; it never edits shared source materials or saved values.
Body scale is presentation only and does not change authority/collision sizes.
To inspect a saved character from native creation, use
`--world-profile /path/to/world.json --asset-pack crates/terra-bevy/assets/character-demo.pack.json`;
[Game flow](../../../../docs/GAME_FLOW.md) explains managed profile locations.

The standalone `pack.json` replaces humans only. The composed
`../character-demo.pack.json` includes a new `comet-cabin.glb` variant and five
unchanged wildlife GLBs. The original Comet GLB and its manifests are retained
unchanged. The cabin variant opens the original closed shell above/below the
footwell, lowers its floor/front cushions and reshapes the front seat backs;
ordinary character proportions are preserved. Its compact-vehicle binding explicitly declares a driver hip
anchor; the new `../character-authoring.recipe.json` publishes the same seven
source models through the normal immutable authoring workflow.

## Supported handoff

- `idle`, `walk` and `run` are ordinary bounded node-transform clips driven by
  actual admitted horizontal movement. A blocked person stands still.
- `seated` places the hip at local origin. The reference game attaches a **known
  manually controlling player's** seated visual to the compact vehicle's
  interpolated, terrain-aligned visual. The original geographic avatar remains authoritative.
- The existing Enter/Exit commands still own the handoff. There is no new
  boarding animation, delay, vehicle ownership, passenger count or collision.
- A driver becomes visible only once the selected cabin-capable car model is
  ready. A procedural/deferred/failed car stays available with its rider hidden.
  Inside a ready cabin, a seated procedural human preserves all saved choices
  during human loading or failure. With no applicable seat declaration the
  known driver is hidden, not left standing at the old boarding position. Other players' public vehicle/profile
  fields do not prove occupancy; unknown remote occupancy is not inferred.
- First person and hidden actor layers hide rider and exterior. Exit or restore
  selects the appropriate standing/seated pose without interpolating a walking
  body through the car. Releasing manual control to RTS/autopilot returns the
  standing avatar under the existing authority. Pause freezes clip phase and
  blend; an actual occupancy change is a discrete pose handoff even while paused.

`animation.human` and `driver_seat` are optional schema-1 additions requiring the
**0.16 reader**. Earlier kind/species and Comet packs retain their normalized
identities because absent fields serialize identically. A field-level contract,
limits and authoring commands are in [Asset authoring](../../../../docs/ASSET_AUTHORING.md).

This is a low-poly articulated node model, not a humanoid retargeter or skinning
workstation. Hair geometry is fixed; the existing hair color options are all
retained. Facial animation, equipment attachments, physical/timed boarding,
replicated remote occupancy, passenger crowds, audio, commercial fleet content,
LOD authoring and measured target-GPU qualification remain open.
