# Animation and atmosphere review — phase 2

This pass adds combat motion to the existing painted 2D battlefield. Its inputs
are the current authoritative attempt, its tick, actor identities and resolved
events. Animation does not choose actions, roll dice, move gameplay anchors,
change collision geometry or grant rewards.

## Architecture boundaries

| Module | Responsibility |
|---|---|
| `animation.rs` | Read-only pose/effect sampling and bounded, temporary presentation history. |
| `visual_assets.rs` | Asset handles, padded frame metadata, UV selection, common-scale foot alignment and textured mesh deformation. |
| `art.rs` | Painter order, shadows, actor/structure meshes, tactical cues and cosmetic particles. |
| `atmosphere_audio.rs` | Pure scene-to-audio selection and the bounded ambient mixer; shares the visual weather classifier. |
| `horde_core` | Combat, action timing, positions, damage, random state, economy and persistence remain authoritative. |

`RenderOptions` and the battlefield call signature remain unchanged. The art
surface stores `SceneCache` in egui temporary data. Histories are scoped by the
canvas/window identity, attempt ID, country, level and mode. This prevents an
observed expedition's local identities from borrowing another field's corpse
appearance. A rewind, actor-counter rollback or previously defeated ID becoming
alive clears the incompatible cosmetic history.

The cache is absent from saves and replays. Rendering receives immutable model
references; production animation code neither reads nor consumes core RNG.
Independent cosmetic variation comes from stable IDs and resolved event keys.
Particle trajectories do not change merely because an unrelated event is added
to the event list.

## Five painted walk strips

All five ordinary enemy families have four generated poses under
`assets/art/animation/`. The original actor atlas remains the fallback while a
strip is unavailable. Bosses and defenders retain their original painted poses
with procedural action/reaction transforms.

| Strip | Native canvas | Shared feet baseline | Common body-height reference | Padding |
|---|---:|---:|---:|---:|
| `shambler-walk-v1.png` | 2172 × 724 | 667 px | 593 px | 6 px |
| `runner-walk-v1.png` | 2172 × 724 | 652 px | 558 px | 6 px |
| `brute-walk-v1.png` | 2172 × 724 | 672 px | 615 px | 6 px |
| `armored-walk-v1.png` | 2138 × 736 | 665 px | 634 px | 6 px |
| `demolisher-walk-v1.png` | 2172 × 724 | 670 px | 623 px | 2 px |

Uniform quarter-width cuts would clip some extended boots and hands. The loader
uses measured, non-overlapping per-frame rectangles instead. Demolisher's first
two figures have a particularly narrow gap, so their padding stays at two pixels
to avoid sampling the neighboring hammer.

Frame rectangles use a common pixel scale and foot baseline for each family.
Their horizontal placement uses measured waist/hip anchors rather than the
changing bounding-box center. Extended limbs therefore do not shrink the whole
character or pull its torso sideways. Raised passing feet remain raised; a
passing frame is not enlarged or lowered to force its last opaque pixel onto
the ground. The armored strip uses its own native dimensions throughout.

The metadata was measured through read-only alpha inspection and visual review
of the source sheets. PNG pixels were not cropped, rescaled or repainted by the
implementation. Generation prompts, ownership and packaging records belong in
the art provenance and credits alongside the untouched source files.

## Motion and feedback

- **Approach:** per-family gait cadence, phase offsets between actors, small
  body sway and actual four-pose selection. Movement-speed modifiers and NPC
  low-HP/FP movement reductions affect cadence. Queued, stunned and prone
  actors do not continue a normal walking cycle.
- **Contact:** authoritative windup progress drives anticipation; resolved
  attacks drive short lunges, followed by recovery. Bending and reaction meshes
  pivot around planted feet. Contact strips, windup meters and the model's
  existing source-lane cues remain in place.
- **Impacts:** resolved hits drive brief squash/lean reactions, dust and armor
  sparks. The renderer preserves the distinction between misses, defenses,
  absorption, weak impacts and kills. Optional coin labels use a `+…c` form.
- **Death:** the cache remembers a visible actor's appearance and last frame.
  A kill can collapse and fade that appearance for up to 60 simulation ticks.
  Source-less history, such as a first render after loading mid-effect, uses a
  generic ground/dust collapse instead of guessing an enemy family.
- **Defender:** the fixed anchor stays at `(0.5, 5.5)`. Recoil, body reaction,
  magazine/hand reload motion and a short melee weapon sweep are cosmetic
  responses to the actual action state and events.
- **Structures:** the turret's tripod stays fixed while its cropped receiver
  and barrel region turns and recoils. Shot events produce bounded casings and
  muzzle effects. Traps compress on a trigger, spent traps dim, damaged
  structures show wear, and destruction events produce debris.

Ground shadows follow the painted foot baselines. Terrain/weather is painted
before actors and tactical labels; transient impact effects are painted above
the subjects they explain. No camera shake or moving aim origin is introduced.

## Bounded presentation history

| Resource | Bound |
|---|---:|
| Encounter histories in the shared presentation cache | 8 |
| Remembered visible actor snapshots per history | 1,024 |
| Remembered corpses per history | 24 |
| Corpse lifetime | 60 simulation ticks |
| Sampled recent events for particle generation | 64 |
| Combined transient and environmental particles per sampled field | 128 |

Repeated egui layout passes at the same tick cannot duplicate a remembered
corpse. Effects can be omitted when their cosmetic budget is full; authoritative
events, damage, collection and rewards are unaffected. The normal renderer's
essential tactical cues continue to read the authoritative model and events.

## Weather, audio and pause

`WeatherKind::from_scene` is shared with the ambient audio module. Authored
clear/calm/none tags suppress precipitation, including in a tropical biome.
Rain/drizzle/storm, snow/frost/cold, ash, dust/sand/dry and mist/fog/haze select
their corresponding presentation materials. Arid fallback favors dust; cold
wind can produce snow drift; other fallback conditions use restrained mist.

Weather uses the authored country/local-stage tags. Its low-opacity directional
particles concentrate beyond the contact strip and are painted behind actors
and the later tactical indicators. It is
fictional scene atmosphere, with no unannounced visibility or accuracy penalty.
The six-channel ambient mixer selects compatible wind, rain, cold, urban and
nature layers and uses bounded 1.2-second presentation-time crossfades. Menu and
pause contexts duck ambience; muting applies independently of a crossfade.

Combat poses, weather positions, recoil and corpse age use `Attempt.tick`.
Pausing therefore freezes them without a hidden catch-up clock. Menu/UI motion
and ambient audio crossfades can use presentation time independently; they
cannot advance combat, recovery or earnings.

Reduced effects selects stable actor poses and removes nonessential gait,
leans, falling motion, muzzle flashes, particles and extra blood splatter.
Contact/windup progress, reload information, impact/defense labels and health
remain visible. Labels keep a fixed position rather than drifting upward.
Corpse removal can retain a brief opacity fade. The blood preference controls
extra splatter, while the painted creature artwork still contains its authored
injuries and worn clothing.

## Automated evidence

The integrated application checkpoint ran:

```powershell
cargo test --locked -p horde-defense --no-fail-fast
cargo clippy --locked -p horde-defense --all-targets -- -D warnings
```

That test run reported **62 passed, zero failed and zero ignored**, including
nine pure animation cases, the five-family padded-frame metadata regression,
six ambient-mixer tests and the existing input/runtime tests. The final owned
presentation source also passed Clippy with warnings denied. Later UI status and
startup review-hook additions belong to the parent task's final combined gate;
its final count is recorded separately in `VALIDATION.md`.

Animation tests cover repeated tick determinism, unchanged authoritative state,
pause freezing, reduced-motion cues, particle/history limits, remembered species
and corpse lifetime, planted attack/recoil sampling, a full defeat burst,
campaign/identity reuse, and stable casing trajectories after unrelated events.
UV tests check the real occupied source bounds, common scale, hip alignment,
foot baselines and the armored sheet's different dimensions. These tests catch
clipped extended boots and neighboring-frame bleed without editing a source PNG.

## Native evidence — pending parent capture

The source sheets have been inspected. Native motion, visual timing, audio
device output and render frame time still require the parent's running-window
capture/listening pass. Paused screenshots are useful for clarity and density;
motion verification needs an unpaused encounter and actual fixed ticks between
frames. A wall-clock-only delay cannot advance these combat animations.

The native pass should inspect walk cycles, contact anticipation/reactions,
normal death fading, defender reload/melee feedback, turret/trap/break effects,
pause and reduced effects, and readability of every lane at supported sizes.
Renderer performance measurements should include warmed, loaded art and state
their actual machine and workload. Earlier CPU-only simulation measurements are
not measurements of this animated renderer.

## Known scope limits

The parent integration subsequently passed the final 185-test workspace gate and strict Clippy. Native views, normal/reduced/tropical sequences and active-frame measurements are recorded in [VALIDATION.md](VALIDATION.md), with exact scopes and reports. These results supersede the earlier pending native checklist above. Native frame sequences are deterministic visual exports, separate from real-time performance samples and human interaction/listening.

This remains a 2D painted-strip renderer with mesh deformation and short,
bounded cosmetic effects. It has four gait poses per ordinary family, not a 3D
skeleton, motion capture, or a complete set of directional weapon poses. Boss
and defender actions use the existing painted silhouettes plus procedural
motion. An immediate terminal results transition can supersede a remaining
battlefield fade.

Presentation history is deliberately temporary, so an evicted or freshly loaded
scene can lose the precise previous corpse appearance. It never reconstructs
that history by replaying combat or rerolling rewards. Animation is shared by
fictional enemy families; it makes no claim of researched national movement.
Authored regional scene tags and biomes guide atmosphere rather than asserting
that a generated pose is culturally or historically specific.

## Final native-review corrections

The independent review of `motion-peak-final/frame-0016.png`, `battle-small`,
`npc-small` and `peak-small` identified two presentation issues. The turret's
painted bore was not horizontal, its assumed attachment was offset from the
spindle, and the former 0.7-radian turn limit visibly disagreed with steep shot
traces. Dark ordinary actors also competed with high-frequency ground detail.

The correction calibrates the unchanged prop source at spindle `(1114,250)`,
breech `(1190,145)` and muzzle `(1344,123)`. The head reflects consistently for
leftward aiming and rotates without an artificial angle limit. Its bore offset
from the spindle is accounted for, so the transformed muzzle ray reaches the
requested screen target. Axial recoil leaves the attachment and separately
painted tripod fixed. Turret traces start at that same transformed muzzle;
the resolved target remains committed for the eight-tick visible tracer window
in both normal and reduced effects. These remain cosmetic coordinates; model
shot origins, targets, timings and damage are unchanged.

The ground now blends the untouched texture beneath an 81.6% opaque matte in
the existing biome palette. This limits source micro-contrast to 18.4%, removes
the extra procedural cracks and scattered ground specks, and reduces alternate
lane darkening. Continuous restrained light/dark seams mark all twelve edges
of the eleven lanes from the contact line through the right approach. Actor
sizes, source PNG pixels and encounter geometry remain unchanged.

Four added regressions check mirrored bore/target alignment across all four
quadrants and near vertical headings at two display aspect ratios and three
recoil amounts; unchanged painted tripod vertices/UVs; resolved-target
commitment during a tracer with unchanged authoritative events; and all lane
edges plus the full-ground contrast veil at small arena size. Standalone
Rustfmt parsed and formatted the three touched presentation files. Cargo and
fresh native captures for this final correction batch are delegated to the
parent's combined gate; the earlier native evidence above predates this batch.

Final parent confirmation: the full workspace gate passed **185 tests** and strict Clippy. Fresh `screenshots-polished/` and three `motion-*-polished/` sets were generated after this correction. The independent reviewer marked HUD clipping, turret heading and small-target/lane readability **resolved** and returned a scoped **ship** verdict. Active dense rendering again averaged approximately 60 FPS at both tested sizes. The immutable portable package passed all 60 manifest entries, copied campaign smoke and native capture; a later actual Windows firing/reload/pause/save/relaunch/resume/F12 check also succeeded. Exact evidence and human-testing limits are in [VALIDATION.md](VALIDATION.md).
