# Lastlight playtest

This build tests the complete survival, preparation, country and strategic loops. Its pixel art, feedback and controls are intended for hands-on assessment. It is a playtest build, with a documented supported rules subset and balance that still needs human evidence.

## Start

In a packaged Windows folder, use Play Lastlight.cmd (or its optional process-local DirectX12 launcher); from the workspace use cargo run --locked -p horde-defense. Choose New game. Existing profiles get a confirmation and complete archive before replacement; Keep current journey cancels. Select Rook/June, confirm, then select the infected starting country for live Personal combat. Every main outcome returns to Journey upgrades; Continue retries/advances and Leave saves without erasing earnings. See [JOURNEYS.md](JOURNEYS.md).

Journey upgrades is the main between-round destination. The complete tree shares earned coins and actual requirements; Equipment and Run analysis are direct, with map/stats and progressively learned advanced routes under More. Main and worker outcomes are distinct; workers do not force the main screen away. The old management-first body/skills/fortification purchase tabs are superseded.

The pixel field retains compact corner status, direct combat actions, focused resource reticle and actual geometry. Journey upgrades shows all 169 nodes in one graph; Branch focus changes camera/selection only. Purchase requires the named learned parents, victories/liberations and Coins or explicitly eligible legacy CP. Fresh body ranks are bought here, not on Character. Settings keeps its durable named return.

On a damaging hit, look for a brief enemy flash and approximately 0.10-second movement halt. Compare it with a miss or fully absorbed attack. Pause during an impact and save/resume: clocks and the remaining halt must stay coherent. Reduced effects substitutes a quiet impact tint. This is a movement reaction, not an extra attack roll or automatic defense cancellation.

Use **Battle practice.cmd** to try fortifications immediately, or **Strategy practice.cmd** to try recruited survivors, parallel battles, guards and an outbreak. Practice has explicitly staged resources and milestones. Its saves are separate from the ordinary campaign; it is not evidence that those resources were earned or that a progression gate is balanced.

If the default graphics backend fails on your device, the optional **Play Lastlight (DirectX12).cmd** launcher sets DirectX 12 for that process only. The ordinary launcher keeps automatic backend selection. This fallback has native capture evidence, not verified foreground performance or an OS playthrough on every device.

## Cursor and impact feedback

Move from a menu/control into a focused unobscured battlefield and back. Expect one custom field crosshair, with the standard pointer restored for UI, popups and focus loss. Read exact ammo or selected throwable charges at the cursor; fire and reload to compare pips/progress with the corner HUD. Construction hides the resource ring while preserving its placement/aim cursor.

With damage numbers enabled, compare an ordinary hit, weak point, absorbed armor, incoming loss, miss and defense. The larger outlined numbers punch, settle and rise for 0.9 real seconds; nearby resolved values can aggregate without extra injury. Repeat at 4× NPC observation and with reduced effects: counts stay readable and the reduced path has no punch or rise. These visuals never pay rewards or resolve another attack.

The menu uses the chosen survivor’s cohesive low-ready-pistol art, with planted boots and upper-body breathing at the original cabin anchor. Equipment and battlefield consume the frozen actual loadout through the shared source-owned player_model facade. Original bind parity is established separately from acceptance of new action/garment poses; use the current model validation guide for that review.

## First ten minutes

1. Choose and confirm a personal survivor, then choose a starting country and enter a personal defense. Move the mouse to aim and hold the primary button to fire. Reload with **R** or its button. The character stays at the left-center lane (lane 2 in the Normal three-lane opening).
2. Let one enemy reach an outer lane's line. Its source lane and windup identify the danger. Any contact lane can threaten the same survivor. Point at that lane and use the secondary button or the selected-contact melee button. Melee consumes stamina and commits recovery; no target means no stamina cost.
3. Watch the boss countdown. The timer announces the boss; only defeating it clears the level. Read the exposure and reinforcement cues. Pause at any point with **Escape**.
4. Retreat when overwhelmed, then inspect retained rewards and **Rest & train skills**. Use the shared graph to buy an eligible improvement, compare/equip owned gear and Continue. Training permits a weapon class; it does not grant the item. Starter pistol proficiency is innate and starter equipment stays protected. Record attempts and purchases: roughly 3–5 early runs is the pacing target, with final country-specific balance evidence reported separately.
5. After learning companion access and meeting its liberation milestone, try Mara as an alternative defender. Watch a few legal one-second actions and inspect the log. A missed attack, a successful defense and absorbed injury are different outcomes. Changing policy does not reset an action timer.

## Fortification practice

Open construction with **B** or its button. **Combat continues** while the grid
and supply panel are open, including in observed expeditions. Use **Escape** or
an explicit Pause action separately if you need time. Place only in the
current-lane-count, five-column grid between the contact strip and midpoint. Check cost,
range and invalid-placement feedback. Try an occupied cell and the right half:
neither may spend supply. During combat, salvage returns at most half the
remaining-health value, rounded down. Walls and turrets block their lane; traps
can be crossed.

Use **Save country plan** during combat to remember the surviving structures for
that country. Retry and check that placement costs are paid from renewed supply.
A smaller budget applies only the ordered affordable prefix, reports omitted
placements and keeps the full stored plan. Save a different layout in another
country and return to confirm independent plans. Clearing a country's plan must
not remove already deployed structures.

## Firearms and ordnance practice

Personal guns require an owned, unreserved eligible item and class training. Mara’s NPC-issued carbine is labeled separately; fresh personal characters must buy or loot their own gun instead of unlocking that issued weapon through training. The six
classes are Pistol, SMG, Shotgun, Rifle, Marksman and Machine gun. Change the
actual firearm in Equipment; buying/training never silently equips it. A pistol
cannot kill a full-health target in one trigger pull, even with skill or
weak-point bonuses. Compare projectile cadence, shotgun spread, ammunition and
reload feedback using legal loadouts; keep NPC rules outcomes separate.

In the personal action strip choose the equipped gun, **Grenade** or **Molotov**.
The selector chooses attack mode; it does not swap inventory guns. Learn handling in the **Ordnance** branch of Survival skills
before using a locked throwable. Select a throwable, point at a ground cell and
press Fire once. Check the cell/radius preview, throw pose, flight arc and impact.
Grenades cause an area blast; Molotovs leave an authoritative burning area.
Holding Fire or toggle firing must not produce repeated throws. Read remaining
charges and cooldowns, then select the gun to return to ordinary aiming. Charges
renew on a new attempt, never by reopening the selector or saving/resuming.

The throwable grid spans the attempt’s frozen path width and row count in one-yard cells; it is distinct
from the five-column construction zone. Ordinary defeated enemies pay once and
leave bodies visible throughout the attempt. Wait, pause and save/resume to
check that old bodies remain without blocking lanes or paying again. Retry
starts a fresh battlefield. See [SKILLS_AND_WEAPONS.md](SKILLS_AND_WEAPONS.md)
for authored effects and compatibility rules.

## Identity, scenery and continuing progression

On the menu and survivor picker, compare Rook and June over the checkpoint scene. The selected survivor is drawn at runtime rather than baked into the background. Confirm that the saved personal appearance returns in the menu, portrait and battlefield. Changing the unconfirmed preview or returning to the menu must not purchase gear or alter combat stats. Check breathing, distant infected, fog, lamp light and quiet parallax, then enable reduced effects and verify a static scene with usable controls. Existing campaigns should continue with their compatible identity.

Inspect woodland and cold battlefield edges at both supported window sizes and high interface scale. A tree should fit fully in its peripheral strip or be omitted; no cut crown/trunk should cover a lane, entrance or contact point.

After every victory, inspect threat, completion XP, next-run balance and dimensions. Normal begins with three lanes/12 yards with 1.25× movement, adding two lanes/two yards every three character levels, capped 17/30. Replay/NPC/worker success raises threat once; defeat/withdrawal do not. Character MAX 50 stops XP without stopping future health/damage growth. Current and legacy 11/30 fields keep saved geometry after settings or XP changes.

Compare two owned instances of one equipment definition at different levels. Inventory/details must show the actual instance level; complete-loadout comparisons, purchase prices, sale values and applicable combat statistics should agree. Buying or looting a higher-level item does not equip it automatically, and old items should not silently gain levels when threat rises. Record rank, item IDs/levels, country, defense, purchases and the actual result separately from fixture resources.

## Quality, critical enemies and overtaking

Compare Poor, Common, Uncommon, Rare, Epic and Legendary names/borders, the quality filter and Highest quality sort. Keep quality, item Lv and unlock tier distinct. Inspect actual comparison and sale values; an old item with no override must retain its original effective values. Shop offers have fixed base quality, and proficiency never grants an item.

Watch an earned drop at the defeated actor’s position: its quality outline/icon rises and fades for 1.5 real seconds, even at 4× observation. The HUD/debrief receipt says it is already banked. Check the owned item once; flying icons must never require collection, create another item, pay again or change combat randomness.

The user chose to retain GURPS injury rules. In NPC mode a conscious zero/negative-HP hostile can still move/fight with existing injury penalties; inspect signed HP and the explicit CRIT · FIGHTING label. Do not treat zero alone as a kill. Actual dead/unconscious/mortally wounded terminals must stop and become a single paid corpse in the resolving tick. Compare 1×/2×/4× without changing outcomes.

Watch faster approaches overtake slower moving peers through a small within-lane side step. Aim, AoE, corpse and loot positions must follow the visible displacement. Walls/contact still stop and queue, and traps still cover the whole lane strip.

## Gameplay settings, DPS and history

Open Settings → Gameplay · difficulty. Select Easy/Normal/Hard or edit Custom, inspect draft dimensions, then Apply to next run before saving/returning. Compare saved next-deployment settings against an active field: its copied parameters must not change. Use a separate profile for aggressive custom tuning.

Read DPS as resolved injury over five simulation seconds, with a zero-padded opening; hover for average/peak/source totals. Pause/save/resume and 4× observation must preserve the simulation clock, independently of frame rate.

Open History from menu/camp or Run analysis from results. Filter mode, Victory/Defeat/Withdrawal and country; inspect frozen gear/balance and last 40 matching DPS/coin/threat trends. Older finalized runs are not invented. Reopening results or settling workers cannot append/pay twice. Back restores the prior pause.

## Journey safety and equipped parts

On an isolated profile, test New game cancellation before a confirmed reset. A confirmed reset must archive the complete old generation before replacing it; failures keep the old journey active. Difficulty/display/audio/controls remain. Leave and resume a settled journey without resetting its equipment, nodes, money or history.

After both a win and a retreat, inspect the same Journey upgrades screen and whole tree. Branch focus must not hide the other paths. Continue must use the legal next/retry stage without buying; country liberation returns to map choice.

Inspect actual equipment on Character: change an owned outfit/accessory/melee or trained firearm explicitly, then compare the shared preview and field layers. Hands should stay on grips, long-gun stock near the shoulder, muzzle/tracers aligned and boots planted through reload/recoil/mirroring. The preview must not spend, equip or alter combat/RNG. Old missing appearance snapshots must remain a valid fallback.

## Country and strategy practice

Country populations determine local defense counts, with data-driven bounds and overrides. Clear every local defense to liberate a country and open its neighbors. Explicit sea routes keep island fronts connected. Zoom, drag, search and inspect the map; garrison and deployment badges show where survivors are assigned.

Default milestones unlock recruitment after one liberated country, parallel expeditions after two, and garrisons, country operations and plague pressure after four. Each recruited survivor owns training and equipment references. A deployed survivor or guard cannot serve in another role simultaneously. A whole-country operation chains real battles and stops on defeat, recall or liberation; it never buys, sells or trains automatically.

In strategic practice, resume the world clock and inspect the active outbreak. Respond with a successful defense or assign guards. An unanswered alert reopens the final stage; earned travel routes persist. The seven-minute response window runs in simulation time. Pause and closing the game freeze pressure and income.

## Controls and recovery

Default keys: primary mouse fires the equipped gun (or a press throws the selected throwable), secondary mouse uses contact melee, **R** reloads, **1/2** use purchased personal abilities, **B** opens/closes live construction, **Escape** pauses, **F** toggles fire, and **F12** captures the native framebuffer. Settings remap essential actions and offer toggle fire, aim assistance, text scaling, outlines, reduced effects, optional blood and independent sound levels. Combat hints reflect the saved bindings.

Save and return to the menu, quit, and continue the same deployment. Reload progress, actions, positions, RNG and banked rewards should resume together. A save failure keeps the application open and reports the failure. Never remove a save to test recovery. Use separate practice or `LASTLIGHT_SAVE_DIR` instead.

## Atmosphere and feedback pass

Keep your ordinary campaign intact and use practice or a separate `LASTLIGHT_SAVE_DIR` for this comparison. Inspect a temperate daytime location, an arid approach, a cold region and a rainy authored stage. Scene tags choose the environmental layers; clear weather must not acquire rain just because its biome is tropical. Bird-like nature calls are absent at night. Changing region or opening an observed expedition should crossfade the environment rather than restart it abruptly.

1. Set combat effects to zero and leave Atmosphere audible. Listen for low wind, distant urban detail or daytime nature, with rain/cold air only where the scene supports it. Watch for an obvious repeat boundary over several loops.
2. Set Atmosphere to zero and restore combat effects. Compare the six legal firearm classes, watch Mara fire, and trigger a turret, trap, grenade blast and Molotov fire. Check that their timbres communicate the source without implying another hit roll or injury. An absorbed hit should differ from health damage.
3. Pause during a scene transition, open settings, and change both buses. Authoritative combat and strategic clocks remain frozen. Cosmetic ambience may continue more quietly; its fade cannot generate resources or advance recovery. Reduced-effects settings should not silently override your audio levels.
4. Mute Master while a cue is playing, then unmute. Check for complete silence, no delayed burst from a clip that was still loading, and a gentle ambient return. At normal levels, wind and rain should remain behind danger cues and visible enemy windups.
5. Observe one parallel expedition, then close its observation window. Ordinary fire from every off-screen survivor should not become a wall of sound. Boss/structure alerts still need to be recognizable when relevant to the campaign.

For an offline comparison, [listen to the survival audition reel](audio/survival-audition.wav) and follow its [cue timeline](audio/survival-audition.csv). The [audio guide](AUDIO_SURVIVAL.md) documents the 72 original WAV variations and mixer limits. This reel bypasses the in-game mix and is not a substitute for device listening during combat.

Record the country, local stage, weather/lighting tags, defender, master/effects/atmosphere values and whether you used speakers or headphones. File/header checks do not establish listening quality; report masking, repetition or a missing source cue as a playtest result.

## Feedback to record

- Which personal identity, country, threat, frozen lane count, defender, equipment IDs/levels/qualities and policy were used?
- Was the cause of every health loss clear, particularly an outer-lane breach?
- Did aiming and hit feedback agree with the visible silhouettes?
- Did retreat clearly lead to rest/training, retained rewards, a useful purchase and retry?
- Which purchases and how many runs were needed for the first clear in this country?
- Did class training, personal weapon ownership, NPC-issued gear and the attack selector behave distinctly?
- Did every successful replay/NPC/worker run increase future threat once, while defeat/withdrawal did not?
- Did the selected menu survivor persist, and were peripheral trees fully visible?
- Did throwable cell/radius previews agree with blast and burning outcomes?
- Did the country plan survive a smaller supply budget and a switch to another country?
- Did old bodies remain visible through the attempt and save/resume?
- Could you explain why Mara aimed, attacked, reloaded, defended or waited?
- Did bosses provide enough visible time for both manual and NPC responses?
- Did a fortification or a different layout change the attempt meaningfully?
- Could you identify each survivor's actual duty country and respond to an outbreak?
- Did save/resume preserve an unfinished action and avoid duplicate rewards?
- At what window size/UI scale did text, targeting or controls become difficult?
- Did audio identify a warning, weapon/defense source or armor absorption without hiding more urgent cues?
- Did weather, actor animation or particles obscure an arrival, contact position or cursor target?

Include an F12 screenshot and the relevant result/decision log when reporting a problem. Do not include unrelated personal files. Native and core performance reports in the review documentation distinguish measured frame intervals from simulation throughput and from minimum-spec claims.

Personal body/skill/fortification purchases now share [JOURNEYS.md](JOURNEYS.md); actual modular appearance is described in [CHARACTER_COMPONENTS.md](CHARACTER_COMPONENTS.md). Human feel, NPC stutter and device performance require their own current evidence; no visual/compilation result alone closes them.

For an isolated durable-save check, request menu/return/quit during a save and during confirmed New game. Preferences/model mutations must stay gated for the critical transition, old autosave success must not dismiss a newer pending request, and no stale Quit may exit a newly reset profile. A failed save remains visible. Native IO now uses a bounded single-writer snapshot service; visual smoothness and NPC timing still require the current measured evidence.
