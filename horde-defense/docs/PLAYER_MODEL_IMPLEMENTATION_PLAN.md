# Source-faithful player model implementation

Approved implementation direction, 3 November 2026. This plan follows
[research](PLAYER_MODEL_RESEARCH.md), the [art audit](PLAYER_MODEL_ART_DECOMPOSITION.md)
and [architecture proposal](PLAYER_MODEL_ARCHITECTURE_PROPOSAL.md). It records work
and acceptance gates, not completion or visual approval.

## Non-negotiable source contract

The original `menu-survivors-v1.png` is the default visual authority. Preserve its
RGBA bytes. Rook and June's original pistol, hands, sleeves, garments, carriage,
head, hair, trousers and boots must all participate in the modular default.
Every source texel with alpha greater than zero has exactly one semantic visible
owner. Alpha-zero RGB remains available for a complete raw-byte round trip.
There is no alpha threshold, cloth-color classifier, per-part stretch, generated
visible replacement, duplicated crop, or whole-character fallback behind parts.

All bind points use original full-image coordinates. At bind every part maps its
source coordinates identically; only one shared source-to-display transform is
applied. Bone lengths are derived from the source bind joints rather than
declared a second time. Draw order is independent of bone parenting.

Generated repair material is explicitly `hidden_fill`. It contributes nothing
to the exact default bind. Alternate skins and poses require their own review;
the single still cannot establish the appearance of originally occluded cloth.

## Decisions

- Keep the installed Bevy/egui versions and existing canvas backend. Introduce
  a backend-neutral model subsystem; migrate no simulation or camera system.
- Use validated string model, part, bone, slot and skin IDs. Rook/June map to
  those IDs at the current survivor adapter boundary; adding a visual recipe
  must not require another hardcoded gender-specific painter.
- Default `Work jacket`, `Field webbing` and the owned starter pistol select
  the source-original costume, Rook pack/June webbing and original pistol.
  Current inventory, rewards and transactions remain in the core.
- Keep `gear_appearance` as an immutable compatibility adapter from frozen gear
  records when the newer visual snapshot is absent. Do not invent missing gear.
- Resolve one immutable pose containing part transforms and muzzle/hand/throw
  sockets. The painter, previews and shot effects consume that result.
- Cache source masks and local geometry at load/revision boundaries. Rendering
  updates transforms/attachment selection, without rescanning PNGs each frame.

## Dedicated ownership

| Stream | Owned deliverables | First gate |
| --- | --- | --- |
| Art decomposition | Original-source semantic mask metadata, source joints, original gun/hand fragments, default material mapping and hidden-region ledger | Complete audited ownership, no baked limbs left in another slot |
| Rig and assembly | Validated catalogue/types, compiled mask cache, bind/pose graph, equipment attachment selection, egui backend and compatibility facade | Actual modular bind packets with identity source mapping |
| Independent verification | Raw reconstruction, source isolation, alpha-compositing/native parity, diagnostic comparison and pose/equipment fixtures | Zero missing/duplicate/altered source pixels and zero native bind difference |
| Parent integration | Shared contract coordination, existing game/preview seams, terrain work, final checks and packaging | No simulation/save/economy regression |

The rig owner publishes the concrete schema and narrow verification interface
before other streams bind to it. The art owner authors source boundaries; the
verification owner must not implement a second approximate renderer or approve
its own uninspected screenshots. Source edits remain disjoint between streams.

## Stages

1. Freeze schema, original source hash and coordinate conventions. Support
   explicit authored integer regions/runs; compilation creates one disjoint
   owner map. Reject uncovered anatomical pixels, duplicate IDs, invalid
   transforms, unknown references, cycles and over-budget geometry.
2. Author and independently inspect semantic parts for both survivors. A
   residue owner may retain faint isolated source artifacts; it cannot hide
   unsegmented arms, hands, weapons or garments. Render solo/ownership views.
3. Reconstruct default bind solely from those actual parts. Compare all source
   channels and alpha classes. Required result: zero differing pixels/bytes,
   zero missing or duplicate visible ownership, zero hidden-fill contribution.
4. Render unsplit reference and assembled bind using identical premultiplied
   upload, nearest sampling, transform, clip and dark/light backgrounds at
   source resolution. Require zero native difference; inspect supported
   scaled viewports separately. Do not silently bless a tolerance.
5. Add real equipment skins as compatible bundles (torso/sleeves/cuffs together
   where required), carriage and all six firearm classes with their sockets.
   Verify that removing/replacing a slot removes its actual source owners.
6. Add restrained source-compatible breathing, aim, recoil, reload, melee,
   throw, brace and hit poses. Review source/posed comparisons and motion,
   including both arms, hands, feet, seams and the displayed muzzle.
7. Integrate the accepted facade into battle and character/equipment previews;
   retire conflicting active generic-body routes. Run locked checks and real
   checkpoint/performance comparisons before producing the new package.

Stages one through four precede animation tuning. A green IK test is supporting
evidence and never substitutes for the source/native parity gates. Existing
behavioral assertions remain; obsolete renderer tests migrate to equivalent
constraints on the active implementation instead of testing unused replicas.

## Accepted source bind gate (2026-11-03)

Stages 1–4 passed eight focused Rust tests and the native checks in
`target/journey-review/source-model-native-1/summary.json`. Both survivors have
zero differing pixels or RGBA bytes on black and white. Full source-tile CPU
audits preserve all 377,684 nontransparent pixels exactly once, including faint
fringes and transparent RGB payload. The actual cached mesh positions, UVs and
indices are audited, and slot removal is checked through the egui adapter.

Parent and independent verification inspected the original comparisons and
individual part sheets. Rook has 22 parts / 4,110 rectangles; June has 23 parts /
3,945 rectangles. This approval covers the source bind and visible semantic
ownership. Originally occluded surfaces are unknown; animations and alternate
equipment still require their own checks. See `PLAYER_MODEL_VALIDATION.md`.

Stages 5–6 now add separate attachment/hold data without changing the accepted
original masks or default reference skins. True grip contacts remain distinct
from anatomical wrists and palms. Generated or sampled repairs are explicitly
identified and contribute nothing to the reference bind.
