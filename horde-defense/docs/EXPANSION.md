# Country campaign and strategic progression

These later user requirements supersede the original GDD's single-expedition scope boundary. The GDD remains the baseline for battlefield combat, rewards and persistence except where the dated user overrides below supersede it.

Players choose their first infected country on the world map. Every country is an authored record: population determines its level count, with configurable bounds and overrides. Difficulty, boosts, buffs, debuffs, neighbors, biome and regional setting are data. Completing all local levels liberates the country and opens its neighbors. Authored sea links connect islands. Five encounter templates rotate across local defenses; regional procedural scenery and battlefield palettes supply the setting.

Default milestones unlock recruitment after one liberated country, parallel deployments after two, and country operations, garrisons and plague events after four. These thresholds are configurable. Recruits originate in liberated countries and have individual training, equipment and histories. Each simultaneous battlefield contains one defender. Unique equipment cannot serve two active deployments or guards.

Country operations chain actual local battles until liberation, defeat, recall or an invalid loadout. They make no purchases or training decisions. Defeat retains earned rewards and applies the campaign recovery rule.

Plague outbreaks threaten liberated countries. Map/header alerts allow a response through a successful defense or guards posted in advance. Guards use explicit one-second combat procedures and equipped rules profiles. Unanswered outbreaks reopen the final stage; earned travel routes persist. The default response window is seven simulated minutes, allowing a fresh defense to reach and defeat its boss. Tactical pause and closing the application freeze strategic time.

See [WORLD.md](WORLD.md), [STRATEGY.md](STRATEGY.md), [SIMULATION.md](SIMULATION.md) and [VALIDATION.md](VALIDATION.md) for configuration, implementation and evidence.


## User overrides — 2 November 2026

These later requirements supersede conflicting original GDD assumptions about
first-attempt planning, tactical construction pause, death framing, limited
skills and expiring corpse presentation. The GDD remains historical design
context; it is not silently rewritten.

- Choosing the first country immediately deploys live personal combat. Later
  preparation is optional. Retreat retains rewards, leads to rest/training and
  Skills, and permits live retry. The first battle should teach recovery
  naturally; roughly 3–5 early runs is the pacing target, with country-specific
  balance evidence pending.
- Opening construction, placing and salvaging do not pause combat, including
  observed expeditions. Explicit pause and Settings still freeze simulation
  clocks. Layouts are saved separately per country during live combat.
  Deployment pays supply for the ordered legal affordable prefix, reports
  omissions and preserves the full saved plan.
- Every fallen hostile body persists through that attempt and save/resume.
  Defeated actors remain terminal, nonblocking and paid only once; settled
  bodies render in batches. Retry creates a fresh battlefield.
- Skills are an authored 32-node DAG in four branches. Firearms have six classes;
  personal use requires class training and an owned weapon. Innate pistol
  proficiency grants no extra item; pistol damage caps prevent a full-health
  one-trigger kill. Grenades and Molotovs target ground cells with authoritative
  area damage and burning, finite attempt charges and press-edge input.
- Ordinary enemy archetypes combine reusable body, movement, attack, defense,
  reward and visual components. Structure-damage multipliers and contact armor
  fractions follow their components rather than legacy kind tags. First boss
  kills unlock ordinary boss-derived variants in future eligible waves,
  including worker expeditions; those ordinary kills do not clear encounters.

Implementation and authoring contracts are in [SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md),
[ENEMY_COMPOSITION.md](ENEMY_COMPOSITION.md) and the
[View hierarchy](VIEW_HIERARCHY.md). [AUDIO_SURVIVAL.md](AUDIO_SURVIVAL.md)
records the new survival effects library. Compilation, documentation structure
checks and deterministic simulations do not establish a human playthrough,
GUI accessibility, listening quality or every country's progression balance;
record actual evidence and pending limits in [VALIDATION.md](VALIDATION.md).


## Additional user overrides — 2 November 2026

These later requirements extend the same pixel-survival identity and supersede any assumption that five encounter templates form a repeating difficulty ceiling.

- Choose the personal survivor before the first country. Rook and June provide two visual identities with the same starting statistics and pistol. The selected personal identity persists; Mara remains the separate NPC companion.
- The main menu is a living checkpoint scene with the selected survivor rendered over background art. Cosmetic motion must preserve readable controls and become static with reduced effects; it cannot advance authoritative combat or strategic time.
- Peripheral trees must fit completely within the visible battlefield-edge scenery. Fit or omit a tree rather than cutting its crown/trunk or covering tactical space.
- Newly secured defenses raise bounded campaign threat. Arrival groups, enemy pressure and rewards continue across the rotating five encounter templates. Retries, defeat, withdrawal and XP alone must not raise threat; old encounters retain their deployment snapshot.
- Equipment has real saved instance levels with applicable derived combat values and prices, rather than a cosmetic level label. New shop/drop levels follow progression; owned items retain their acquired levels. Training and acquisition remain distinct, and fresh personal loadouts cannot appropriate NPC-issued starter gear by learning its class.

The current source and authoring contracts define implementation and compatibility. Exact test, native capture, interaction, performance and listening evidence remains in [VALIDATION.md](VALIDATION.md); these requested behaviors are not themselves validation results.


## Latest user overrides — quality, movement and victory arenas — 2 November 2026

These explicit later decisions supersede fixed-eleven geometry, unique-clear-only threat and a total difficulty cap at fifty in earlier plans/records.

- Six WoW-inspired named qualities—Poor, Common, Uncommon, Rare, Epic and Legendary—are separate from item level and unlock tier. Their real stat/value ratios and deterministic receipt contract are in [ITEM_QUALITY.md](ITEM_QUALITY.md).
- Earned loot appears at the actual death position with a quality outline and rises/fades in real UI time. It is already banked; presentation never grants or requires collecting a second item.
- The menu survivor returns to the original source-image cabin anchor (0.400, 0.807), at 21% source-image height. Fringes are corrected in the managed premultiplied upload path; original PNG pixels/hashes remain unchanged.
- Faster enemies may overtake slower moving peers within the same lane. Structures/contact still block and traps cover the full lane. Saved displacement and Enemy.y() are authoritative for aim, range, corpses and loot.
- Keep the pinned GURPS injury rules. Conscious zero/negative-HP NPC enemies may fight and must be visibly labeled critical with signed inspection. Actual terminal actors stop/reap/pay in the resolving fixed tick; zero HP alone does not grant NPC death rewards.
- Character MAX is 50, with up to 120 once-only completion XP per successful run in either mode. NPC kill XP remains separate. New deployment lanes unlock from 11 to 13/15/17 at levels 11/21/31; active geometry and legacy eleven-lane saves remain frozen.
- Every committed victory, including replay, outbreak and parallel/worker victories, raises future threat exactly once. Defeat/withdrawal do not. The old fifty rank is a bounded-growth knee, while health/damage continue to an explicit numerical support guard. Independent caps still bound lanes, groups, population and item level.

[PROGRESSION_SCALING.md](PROGRESSION_SCALING.md) and [OVERTAKING_AND_CRITICAL.md](OVERTAKING_AND_CRITICAL.md) are the current source contracts. Earlier dated requirements and validation remain historical; they are not the latest geometry, progression or performance guarantee.


## Menu pose, cursor and impact presentation — 3 November 2026

The user's additional presentation requests preserve all quality, victory-threat, frozen-lane, overtaking and GURPS contracts above.

- Replace the assembled menu arms/gun with a new cohesive Rook/June low-ready-pistol atlas. Keep the original cabin anchor/21% source height, planted boots and restrained breathing. The calibrated battle aiming rig stays unchanged.
- The focused unobscured battlefield has one custom crosshair with ammunition/reload/charge feedback. Native pointer visibility restores for ordinary menus, controls, popups, leaving the window and focus loss.
- Make resolved hit numbers larger and outlined, with a short impact punch, settle and upward fade, distinct weak-point/armor/incoming/status feedback and a reduced-effects path. Bounded real-time presentation caches must not change damage, timing, rewards or randomness.

The source catalogue now has seven active managed textures, eight pixel provenance entries and nineteen retained repository PNGs. Original files are preserved; the new menu asset has its own prompt/hash provenance.


## Short opening, balance settings and saved analysis — 3 November 2026

These explicit user changes supersede the earlier fresh11/30 opening; legacy active fields stay frozen.

- Normal starts at 3 lanes/12 yards, with 1.25× authored movement, adding two lanes/two yards every three character levels, capped 17/30. The clearing grows physically rather than stretching three rows across the viewport.
  - **Superseded 4 November 2026 (user requirement, camera):** the field now uses uniform zoom-to-fit. It keeps the lane:yard proportion of the per-geometry clearing above and zooms until one axis fills the bounded arena, so fewer lanes or yards zoom in (3x12 is 2.5x). Every already-filling field (17x30, 11x20, all eleven-plus-lane fields) stays pixel-identical. Construction is an overlay panel and never rescales the arena. See docs/plans/2026-11-05-production-pass.md (CAM-1, CAM-2) and `art::camera::fit_field`.
- Validated Easy/Normal/Hard/Custom JSON presets and saved per-profile settings apply to the next run, including a chained worker's next deployment. Active attempts retain copied settings and dimensions.
- Actual DPS uses five simulation seconds, plus whole-run average/peak and damage-source totals. Pause,4× observation and checkpoint restoration preserve its time base.
- Every newly settled victory/defeat/withdrawal produces a saved context/gear/balance/geometry/reward/XP/damage/DPS record. Parent-owned worker outboxes append once. History is not silently truncated or retroactively invented.
- Read-only History provides filters, selected detail and recent trends while fields are paused; a 40-record chart window does not delete older saved records.

[BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md) defines actual fields/bounds. Current Normal proof has fourteen third-run clears and one fourth-run clear across the selected fifteen country/seed routes; earlier exactly-three-run/default-geometry records are historical, not human guarantees.

## Component survivors and incremental journeys — 3 November 2026

These latest user requirements supersede the separate personal body/skill/fortification purchase screens and the early management-first flow. The pixel-survival identity, fixed defender, earned equipment, retained rewards and later country strategy remain.

- The main menu offers New game. A new journey confirms a survivor, selects its infected starting country on the map and starts live defense. An existing campaign must be protected before a confirmed new game replaces the active profile.
- Every completed main round returns to one upgrade screen. The player spends retained earnings, equips owned gear and chooses to continue the journey or leave it. A failed round is a retreat, rest and training opportunity; a victory advances to the next local defense. A liberated country opens its eligible neighbors. Leaving is not a prestige reset and does not erase earned progression.
- One connected, data-driven skill tree contains personal development, weapon training, abilities, fortifications and access to later systems. Paths branch from a shared foundation. Path navigation changes the camera rather than splitting the tree into separate tabs. Weapon proficiency and buying/looting a weapon remain separate requirements.
- Start with the incremental battle/upgrade loop. Introduce companions, construction, recruitment and country protection through earned unlocks and the existing liberation milestones. Individual NPC rules advancement remains attached to that NPC; it must not silently modify the personal survivor or bypass the pinned GURPS accounting.
- Survivors use interchangeable body, clothing, carried equipment, arm/hand and weapon parts. Actual equipped instance metadata determines equipment appearance. A shared rig must keep hands on weapon grips, long-gun stocks near the shoulder and tracer origins at the displayed muzzle through aiming, recoil and reload. Animation remains presentation-only and cannot change the fixed defender position or an already resolved hit.

The implementation/authoring contracts and measured validation for this increment are recorded separately; this section records requested behavior, not a claim that visual inspection or human playtesting has passed.
