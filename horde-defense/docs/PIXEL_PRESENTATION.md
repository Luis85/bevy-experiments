# Pixel survival presentation

The user's reference screenshots establish a full-screen atmospheric battlefield,
compact corner HUD, crisp pixel art and visual upgrade choices. Lastlight keeps
its survivor/plague setting and the authoritative deployment-frozen lane/path geometry.

## Battlefield and interaction

Continuous biome ground fills the window. Pixel trees, wrecks, fences and other
outpost dressing stay outside the combat rectangle. Normal aiming has no
spreadsheet grid or permanent tactical sidebar. Health, portrait and ammunition
occupy one corner; boss/time another. Management, inspection and NPC decision
details disclose on demand.

Choosing the first country immediately starts personal combat. There is no
briefing stop or precombat pause. Later preparation remains available. Retreat
leads to rest/training and retained rewards; the debrief's failed-personal-run
primary action opens Skills, and retry starts live. The intended early loop is
roughly 3–5 runs; final country-specific pacing evidence belongs in
[VALIDATION.md](VALIDATION.md).

Opening construction, placing and salvaging keep combat running, including
observed expeditions. Explicit Pause and Settings remain separate actions.
Construction reveals the current-row-count, five-column left-half grid, legality
preview and supply controls. **Save country plan** records surviving structures
during combat. Deployment pays supply for the ordered affordable prefix,
reports omissions and preserves the full saved plan. Countries keep independent
plans; clearing a plan leaves deployed structures intact.

The personal attack selector chooses the equipped gun, Grenade or Molotov.
Changing firearm instances is an Equipment task requiring ownership and class
training. Ground targeting discloses a separate one-yard-cell grid inside the actual frozen width/rows,
selected cell and radius. A Fire press edge throws once; hold-to-fire and the
firing latch cannot repeat throws. Flight arcs, throw poses, blast/fire visuals
and sound consume authoritative ordnance state rather than resolving damage.

## Skills and visual assets

The skill catalog contains 169 nodes in one global connected tree, including personal body, weapons/abilities, fortifications and feature access. A pan/zoom node canvas and selected-detail purchase
panel show actual prerequisite edges, prices, effects and learned state.
Fortifications show real blueprint milestones and sequential ranks. Wider
windows place details beside the graph; compact layouts place them above it.
Node selection is presentation state; purchases remain explicit commands.

The ordinary loader retains seven scene/effect atlases. The player_model manifest separately declares original model, garment, hidden-repair and equipment sources, cached at startup. All use managed straight-RGBA conversion once, NEAREST and actual RenderDevice texture limits. Retained historical component sheets are not active production bodies. Pixelify Sans and the original low-ready menu scene retain their established identity.

Original Rook/June still pixels are assigned to real semantic parts and reconstructed exactly in the accepted reference bind. Unknown surfaces revealed by articulation require explicitly identified repairs; alternate garments are authored alternatives. Their source-faithful pose/contact/native-motion approval is separate from still-image parity. Battle, equipped preview, displayed muzzle and cached throw release share the player_model facade. None can move the authoritative defender, resolve injury or grant equipment. See [CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md) and [PLAYER_MODEL_VALIDATION.md](PLAYER_MODEL_VALIDATION.md).

## Impacts, bodies and settings

Positive enemy injury applies a bounded six-fixed-tick movement halt (~0.10s)
and silhouette flash. Misses and fully absorbed/defended attacks do not stagger.
Hit stops refresh without accumulating; NPC decisions and attack clocks keep
their cadence. Pause/save/resume preserves the remaining counter. Reduced
effects uses a quiet impact tint.

Every fallen hostile leaves a persistent per-attempt corpse record. Bodies do
not expire after a timer or fade away; save/resume preserves their identities,
positions and resolved appearance. Collapse animation settles into batched
corpse rendering, keeping accumulated bodies visible without one independent
draw operation per body. Defeated hostiles remain nonacting/nonblocking and
cannot pay rewards again. Retry creates a fresh attempt and battlefield.

Settings names its actual return destination and offers a durable route to the
main menu from an active game. Save failure stays visible and cannot claim
successful navigation. Readability controls include scale, outlines, damage
numbers, contextual guidance, blood and reduced effects. Important sounds have
visible equivalents; fixed-camera aiming never shakes.

[Survival audio](AUDIO_SURVIVAL.md) describes the 69 original WAV variations,
bounded mixer and [audition reel](audio/survival-audition.wav). Audio variations
consume no combat randomness. An audition or file inspection does not establish
how the live mix sounds on a particular device.

## Short opening, balance and analysis

Normal starts with three lanes/twelve yards and 1.25× authored movement. Every three character levels adds two lanes/two yards, capped at 17/30. The physical clearing grows instead of stretching a few rows across the viewport; active and old eleven-lane/thirty-yard saves stay fixed. Peripheral dressing defines actual edges and retains full-sprite clearance. A separate bounded effects region does not expand gameplay inputs.

> **Superseded 4 November 2026 (user requirement, camera):** the physical clearing no longer stays at the normal yard scale. The field uses uniform zoom-to-fit: the clearing's lane:yard proportion is kept and it zooms until one axis fills the arena, so a 3-lane opening fills the viewport width. Already-filling fields (17x30, 11x20, eleven or more lanes) are pixel-identical; props are capped at the reference lane size; construction overlays the field without rescaling it; overlay effects get 60 pt of headroom above the arena. See docs/plans/2026-11-05-production-pass.md (CAM-1 to CAM-5).

Gameplay presets/custom drafts commit only for the next run. Actual DPS uses five simulation seconds, average/peak and source totals. Read-only History shows saved all-outcome records, frozen detail and last-forty trends while fields are paused. Its plot window never truncates stored history. [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md) defines those contracts and compatibility.

## Authoring and evidence

[SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md) and
[ENEMY_COMPOSITION.md](ENEMY_COMPOSITION.md) describe validated startup JSON
overrides, stable IDs and save snapshots. [VIEW_HIERARCHY.md](VIEW_HIERARCHY.md)
indexes the individual View documents and their parent properties.

Native screenshots, motion, interaction, regression gates and packaged artifact
evidence are recorded in [VALIDATION.md](VALIDATION.md) after execution.
Historical validation records describe their tested revision. Static references,
source inspection, compilation and documentation checks do not establish a
current human playthrough, listening result, accessibility assessment or
country-specific balance proof.

Every main round now returns to the shared Journey upgrades home. [JOURNEYS.md](JOURNEYS.md) defines safe reset/continue/leave, unified payment/gates and NPC separation; [CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md) defines actual equipment snapshots, sixteen body parts and shared socket/IK/muzzle geometry. These source contracts do not establish a measured NPC lag fix.
