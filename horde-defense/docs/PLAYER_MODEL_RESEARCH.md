# Art-preserving modular player models: research and implementation recommendation

Research checkpoint: 3 November 2026. This is a proposal and source audit, not an implementation, visual acceptance, performance result or release declaration. No production code, assets, dependencies or saves were changed for this research.

## Decision

Build a small, data-driven cutout model system using **the original artwork as its bind-pose source**, and retain the existing egui battlefield renderer. The first acceptance gate is exact reassembly of Rook and June from independently addressable source parts. Animation and equipment variants follow that gate.

The rejected approach repeatedly treated plausible bone lengths and regenerated clothing as evidence of visual parity. Those properties do not establish that the assembled character matches the artwork. A bind reconstruction must include the whole reference figure, its partially transparent edges, its depicted starting equipment and its original proportions. A whole-image background under otherwise incomplete parts must not be allowed to pass the reconstruction test.

Use separate concepts for bones, image attachments, equipment slots and draw order. This follows established cutout practice without importing another engine or animation runtime. Godot's official cutout tutorial demonstrates parent-relative parts, authored pivots, a stored rest pose and depth ordering independent of transform parentage; it also describes mixing cutout parts with cel variants for difficult extremities. Its page warns that some engine-specific details are outdated, so it supplies workflow principles rather than Bevy API instructions. [Godot cutout animation](https://docs.godotengine.org/en/stable/tutorials/animation/cutout_animation.html)

## What “1:1” must mean

There are two distinct claims:

1. **Reference bind parity:** with the declared reference appearance, bind transforms, source camera/proportions and original low-ready pistol pose, part assembly reconstructs the source figure exactly. Every source pixel with alpha greater than zero belongs to exactly one visible-source attachment. There are zero missing pixels, duplicate owners or altered visible RGBA values.
2. **Animated art fidelity:** aiming, recoil, reload, melee and throwing move those attachments while retaining their original pixels, proportions, recognizable silhouette and clothing language. A changed pose necessarily changes the resulting image. “Pixel-identical in every pose” is not a valid promise against a single still image. Newly revealed cloth is separately authored reconstruction, because the flattened still contains no evidence of what was hidden there.

The first gate is machine-checkable. The second requires representative native images and motion review against the reference; IK endpoint equality is only one supporting constraint.

The canonical source is `assets/art/pixel/menu-survivors-v1.png`, 1774×887, SHA-256 `4feb21b05c1ad5d281d4f0134fdfe546356ca71a204dba04b7c81c48bd356a3a`. The art-decomposition specialist measured the current crops and reported that almost all nontransparent pixels are partially transparent, commonly alpha 253. This measurement makes overlap handling essential, not cosmetic. Source ownership must not use the current alpha-128 threshold as its definition of visible artwork.

For example, compositing two copies of alpha 253 produces approximately alpha 255, rather than preserving the original 253. Drawing an opaque repair patch behind such a pixel also changes the result. At bind, hidden fills must therefore be disabled or clipped out wherever the reference's visible ownership covers the source. Arbitrary color filtering must not discard legitimate fabric or outline pixels.

Fully transparent RGB is irrelevant to visual parity. For nonzero alpha, retain source RGBA. The native comparison should compare the direct reference and the assembled parts through the **same upload, premultiplication, background and sampling pipeline**; do not compare a straight-alpha source PNG numerically to a screenshot in another alpha/color convention.

Use one parent/source-frame transform for the complete bind assembly. Independent per-part rounding, custom sizing or nonuniform scaling can create seams despite correct UV masks. If pixel snapping is needed, snap the common root in physical screen coordinates; do not independently snap every joint. Nearest sampling preserves the chosen texels, but arbitrary rotation changes their screen coverage and can shimmer. A low-resolution model render target is an optional later tradeoff if native motion evidence demands it, not a prerequisite or an excuse to defer bind parity.

## Pinned project and renderer facts

The independent workspace declares Bevy `=0.20.0`, `bevy_egui =0.43.1` and Rust 1.99.1. Its lockfile resolves the inspected Bevy sprite/image/transform/ECS packages to 0.20.1. Installed crate source is the authority for implementation APIs; current public examples are supporting concepts, not permission to upgrade.

| Inspected source | Relevant fact |
| --- | --- |
| `bevy_sprite-0.20.1/src/sprite.rs` | `Sprite` supports image handles, atlas regions, one-off rectangles, tint, flips and optional custom size; its required components include transform, visibility and anchor. |
| `bevy_ecs-0.20.1/src/hierarchy.rs` | `ChildOf` and `Children` provide the entity hierarchy relation. |
| `bevy_transform-0.20.1/src/plugins.rs` | Transform propagation updates descendant global transforms through named plugin systems. |
| `bevy_egui-0.43.1/src/render/mod.rs` | The pipeline uses premultiplied-alpha blending. |
| `bevy_egui-0.43.1/src/render/egui.wesl` | The shader samples the supplied texture without the former blanket RGB-by-alpha multiplication. |
| Project `visual_assets.rs` | Managed `ColorImage::from_rgba_unmultiplied` conversion is cached once and uses nearest sampling and the actual render-device texture budget. |
| Project `ui/battle.rs` and `art.rs` | Terrain, actors and atmosphere are painted inside the egui canvas in order. |

The installed source files are under Cargo's registry source root, currently `%USERPROFILE%/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`. They were read; no Cargo command or build was run for this document. Some attempted public pinned-version documentation URLs were inaccessible to the research tool, so they are not presented as verified API references.

Bevy's official sprite-sheet example uses an atlas and nearest image sampling for crisp sprites. This validates the basic rendering technique; it does not provide an art segmentation or modular character pipeline. [Bevy sprite-sheet example](https://bevy.org/examples/2d-rendering/sprite-sheet/)

The current managed-image path is appropriate and should be preserved. Upstream `bevy_egui` removed automatic user-texture premultiplication in its shader; the source and destination alpha conventions must agree. egui separately documents its straight-RGBA and premultiplied-RGBA constructors. [bevy_egui alpha change](https://github.com/vladbat00/bevy_egui/pull/465), [egui ColorImage](https://docs.rs/egui/latest/egui/struct.ColorImage.html)

## Backend options

| Option | Fit for this increment | Recommendation |
| --- | --- | --- |
| Ordered masked attachment meshes in the current egui canvas | Integrates with the existing arena projection, viewport clipping, previews, hit feedback and captures. Reuses the correct managed textures. | **Choose now.** Keep its model/pose data independent of egui so another backend can be added later. |
| One Bevy `Sprite` entity per attachment, parented through `ChildOf` | Native engine primitives can represent many rectangular parts and propagate transforms. Arbitrary source-owner masks still require suitable region textures or meshes. | Valid future backend. A player-only migration now needs camera/DPI/clip/draw-order integration: ordinary world sprites cannot simply appear between the current opaque egui ground and egui actors. |
| Bevy offscreen character rendering, then compositing the image in egui | Can retain engine sprites behind one UI image. Adds render targets, texture lifetime, resizing, sampling and another composition stage. | Defer until an actual need justifies that extra seam. |
| Spine runtime/editor integration | Established attachment, skin and constraint workflow; adds another authoring/runtime contract. | Borrow concepts, do not add a dependency in this pass. Its runtime has its own stated license terms, so it is not an incidental MIT package substitution. [Official runtime license](https://en.esotericsoftware.com/spine-runtimes-license) |
| Regenerate all body parts and calibrate them to plausible human ratios | Can create a coherent new character, but source identity and texture matching become subjective and repeatedly drift. | Reject for the user's exact-art requirement. Use repair material only for genuinely hidden pixels. |

## Relevant cutout concepts from primary sources

Spine's slot model explicitly separates attachment selection and draw order from bone parenting. That distinction solves cases where an arm must inherit the torso transform while some of its images sit above or below the weapon/body. Bone parentage must not be used as a substitute for painter order. [Spine slots](https://eu.esotericsoftware.com/spine-slots)

Spine skins provide alternate attachments for semantic placeholders and allow animation reuse across clothing sets. Adopt stable slots such as `torso`, `near_upper_arm`, `near_forearm`, `main_hand`, `back_carriage` and `weapon`; a skin resolves each slot to an attachment definition. It should not select artwork by substring matching an item's display name. [Spine skins](https://en.esotericsoftware.com/spine-skins)

Spine distinguishes image regions, textured meshes and conceptual point attachments. The analogous local system can use a source-region/mask attachment for pixels and named sockets for grips, muzzle, magazine and throw origin. A socket is not another visible sprite or an accuracy system. [Spine attachments](https://uk.esotericsoftware.com/spine-attachments)

Constraint order is meaningful. Apply authored pose tracks, then aim/grip constraints, then resolve derived sockets and paint. The same resolved pose must drive the displayed weapon and its muzzle/throw output. [Spine constraints](https://en.esotericsoftware.com/spine-constraints)

## Audit of the current attempt

These are research findings in the source inspected during the freeze, not declarations about later implementation changes:

- `character_rig.rs` now contains a generic generated-body route plus a newer canonical route. Their overlapping responsibilities and fallback-dependent previews make it hard to identify which model is being accepted. Introduce one public facade and a declared active model implementation; keep historical fixtures clearly separate.
- `character_rig/canonical.rs::source_sleeves` uses `rgba[3] >=128` and a color heuristic to decide cloth ownership. This drops original visible pixels and can alter edge/shading identity. Source art ownership needs authored semantic masks, with no hue classifier or alpha cutoff.
- `canonical.rs` still uses generated torso/far sleeves/hands for visible regions. The generated kit is a reconstruction, not a lossless decomposition. It cannot establish a pixel-exact original default pose.
- Existing lower-body coverage tests skip alpha below128 and count membership in source rectangles. They do not prove whole-image source coverage, actual native composition, default gear inclusion or a complete source bind pose.
- `Body::new` has a uniform source-height scale, which is useful. However, a separate actual **bind** pose is missing: the ordinary `Pose::new` immediately solves an aimed weapon/IK pose. A default-looking pose cannot be treated as the original source pose by implication.
- The artwork contains its original pistol and carriage. Default parity requires those visible source pixels to be assigned to the corresponding weapon/carriage slots. A different atlas pistol and an empty `EquipmentVisual` are not the same reference appearance.
- Exact wrist/palm contacts and shared muzzle math are useful existing invariants. Keep them, but add image ownership and visible joint-connection gates so a detached textured arm cannot pass on mathematics alone.
- `gear_appearance.rs` already provides a useful immutable compatibility seam: existing visual snapshots win; missing newer snapshots can resolve only from deployed frozen gear records. It must never inspect today's mutable inventory to redraw an older deployment.

## Proposed assembly contracts

Use string IDs in authored visual data, with typed runtime indices after validation. The existing gameplay survivor/equipment IDs remain authoritative; an application adapter maps them to visual model/skin IDs.

| Layer | Owns | Must not own |
| --- | --- | --- |
| Model catalogue | Source asset/hash/frame, bones and bind transforms, semantic slots, owner masks, default skins, source sockets, supported aim/action variants | Coins, item acquisition, health or save transactions |
| Art compiler/cache | Validate masks; build cached UV polygons or merged row runs; source dimensions and sampling metadata | Color-based guesses about which limb a pixel belongs to; per-frame image scans |
| Appearance resolver | Frozen equipped metadata to slot attachments, compatible garment groups and correct appearance variant | A second inventory, automatic purchases or unsupported fake starter gear |
| Pose resolver | Authored rest/action tracks, constrained aim/grips, bone world transforms, deterministic draw-order changes and derived sockets | Combat hit tests, RNG, rewards or frame-rate-dependent simulation |
| Egui backend | Ordered meshes from resolved pose, source texture IDs, clip and presentation scale | Re-solving IK independently for the muzzle or deciding equipment ownership |
| Validation harness | Source partition/reassembly, per-slot isolation, native direct-reference comparisons, action/equipment matrices | Silent snapshot blessing or hiding a whole reference figure behind incomplete parts |

Recommended facade: `resolve_model(catalogue, model_id, appearance, pose_input) -> ResolvedModelPose`; `draw_model(painter, textures, &pose)`; derive `pose.muzzle`, `pose.throw_origin`, `pose.hand_contacts` from that same result. `pose_input` carries a read-only action phase/aim/anchor/scale snapshot. It does not carry a mutable `Game` or a filesystem path.

Choose garment groups as well as individual slots. An outfit may replace torso and both sleeves together; independently choosing a different torso while retaining unrelated sleeve widths is how disconnected cuffs and inconsistent clothing arise. A ranged attachment declares grip/support/muzzle/magazine points and compatible hand/cel variants. Adding a new visual skin should be data/asset work within that contract, not another branch of hardcoded gender coordinates in the painter.

## Gates and implementation sequence

1. **Freeze reference:** retain the original PNG/hash and precise frame/groundline. Declare the exact starting appearance represented by each source figure.
2. **Partition visible source:** assign every nonzero-alpha source pixel to one semantic owner. Cache bounded row-run or polygon meshes. Check no uncovered/duplicate ownership; report mismatch counts and source coordinates. No color/alpha-threshold cleanup.
3. **Exact bind reassembly:** render all owner attachments through their bind transforms. At source resolution, visible RGBA must reconstruct exactly. Disabling a slot removes precisely its ownership; its content must not remain in an uncut fallback image.
4. **Native bind comparison:** direct original and assembled bind use identical anchor/height, texture conversion, nearest sampling, color, clip and background. Compare normal and compact supported windows, with an enlarged side-by-side and a difference artifact. Camera/pixel-grid discrepancies are fixed rather than excused by a loose similarity score.
5. **Hidden fill and swaps:** author unseen cloth/skin as explicit supplemental material, inactive or properly excluded at exact bind. Validate compatible garment groups, actual equipment metadata, default carriage, locked/reserved item rules and legacy records. One survivor's skin must not silently borrow a different character's source dimensions.
6. **Animate incrementally:** breathing first; then the useful right-facing aim arc; firearm recoil; reload; melee; throw; hit/brace. Use authored source-compatible pose/hand variants where a single cutout would otherwise stretch or reveal unknown perspective. Review both arms, hands, shoulder contact and feet in actual native images/motion.
7. **Integration/performance:** all six firearm classes and clothing/carriage variants; reduced motion; pause; native arena/preview/menu reuse; immutable save/RNG comparisons. Only then benchmark the final renderer with the actual NPC checkpoint and peak field. No image scans, catalogue parsing, mesh-topology rebuilding or atlas uploads per frame.

The use of cel attachments for difficult angles is supported by the official cutout workflow, but the chosen attachment artwork and angle boundaries remain authored project decisions. FK/IK is a pose tool, not a replacement for missing art or an aesthetic oracle.

## Open decisions for Root before implementation

- Confirm the reference appearance mapping: Rook pack, June tan chest rig and original pistol must correspond to the actual starting equipment or a declared reference-only skin.
- Approve the art specialist's semantic owner map and measured original joint/socket locations. The current broad overlapping rectangles are not adequate proof.
- Decide the supported pose/angle matrix and where a small authored hand/weapon-side variant is needed instead of deforming one piece.
- Approve the catalogue/facade boundaries and old-route retirement plan. Existing behavioral assertions should migrate to the active implementation, not survive solely as tests of an unused helper.
- Establish a measured mesh/run-count budget after the mask compiler is implemented. Do not reduce fidelity by throwing away low-alpha pixels to meet an unmeasured budget; merge equal ownership runs/polygons instead.

Research outcome: exact reference bind parity is feasible with the existing source pixels and current backend. The present canonical route is a useful partial source adapter, but it has not met that criterion. Animation, hidden-material artwork and modular wardrobe quality require their own subsequent evidence.
