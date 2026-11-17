# Lastlight playtest

This build tests the complete survival, preparation, country and strategic loops. Its pixel art, feedback and controls are intended for hands-on assessment. It is a playtest build, with a documented supported rules subset and balance that still needs human evidence.

## Start

In a packaged Windows folder, use Play Lastlight.cmd (or its optional process-local DirectX12 launcher); from the workspace use cargo run --locked -p horde-defense. Choose New game. Existing profiles get a confirmation and complete archive before replacement; Keep current journey cancels. Select Rook/June, confirm, then select the infected starting country and press Start journey: the first Personal defense opens paused and starts with Begin defense. Every main outcome returns to Results, with three actions: Upgrade opens Journey upgrades, the continuation retries/advances and Leave saves without erasing earnings. See docs/JOURNEYS.md in the source repository.

Between rounds the loop is Results, then Upgrades, then Camp home. Results is the debrief, and its Upgrade opens Journey upgrades, the only route to the complete tree, which shares earned coins and actual requirements. There Back to battle continues the journey, and Back to camp opens Camp home, the camp's first page, whose camp row leads to Equipment, Character, the Briefing and the map. Main and worker outcomes are distinct; workers do not force the main screen away. The old management-first body/skills/fortification purchase tabs are superseded.

The pixel field retains compact corner status, direct combat actions, focused resource reticle and actual geometry. Journey upgrades shows all 177 nodes in one graph; Branch focus changes camera/selection only. Purchase requires the named learned parents, victories/liberations and Coins or explicitly eligible legacy CP. Fresh body ranks are bought here, not on Character. Settings keeps its durable named return.

On a damaging hit, look for a brief enemy flash and approximately 0.10-second movement halt. Compare it with a miss or fully absorbed attack. Pause during an impact and save/resume: clocks and the remaining halt must stay coherent. Reduced effects substitutes a quiet impact tint. This is a movement reaction, not an extra attack roll or automatic defense cancellation.

Use **Battle practice.cmd** to try fortifications immediately, or **Strategy practice.cmd** to try recruited survivors, parallel battles, guards and an outbreak. Practice has explicitly staged resources and milestones. Its saves are separate from the ordinary campaign; it is not evidence that those resources were earned or that a progression gate is balanced.

If the default graphics backend fails on your device, the optional **Play Lastlight (DirectX12).cmd** launcher sets DirectX 12 for that process only. The ordinary launcher keeps automatic backend selection. This fallback has native capture evidence, not verified foreground performance or an OS playthrough on every device.

## Loop

Walk one whole loop once per build, at 1440x900 and at 1050x700 with 135% interface scale. On every screen exactly one button is amber (the primary), and each Escape press does one thing (see Escape below).

1. On the main menu choose New game (amber on a fresh profile; Resume journey is amber once a journey exists). Pick Rook, press Continue as Rook, select the infected starting country and press Start journey. The first defense opens paused; Begin defense starts it.
2. Lose the defense, or hold Retreat & train. Results opens with exactly three buttons: Upgrade, the continuation (Try again) and Leave journey. Upgrade is amber after a loss whose recommended node you can buy now, otherwise the continuation is. The loot line and the Defender row sit above the bar.
3. Press Upgrade. Journey upgrades shows the whole tree with Back to battle (amber) and Back to camp. Buy one node and watch the coins drop.
4. Press Back to battle: the next defense deploys live. Win it, and Results offers Next defense as the continuation.
5. Play Next defense through the country's last defense. The liberation Results names the country and the fronts it opened; once Survivor network is bought it offers "Recruit from <Country>", otherwise it names the node you are missing.
6. Press "Recruit from <Country>". The Recruit window offers two archetypes, each with its origin trait; press "Recruit as <archetype>" (it shows the price), then Back to camp.
7. Camp home is the camp's first page: the next defense with its defender picker, the Defenders list, alerts, expeditions, offers and recent reports, with the world clock in its footer. Press Defend next on the new recruit. The amber button is Choose next country.
8. On the World map choose a front and press Plan defense; nothing deploys yet. The Briefing's amber button reads "Deploy <recruit>": press it and the recruit defends (see Takeover).

Record every screen whose amber button, button count or Escape result differed from this list, and the step where the loop stopped.

## Cursor and impact feedback

Move from a menu/control into a focused unobscured battlefield and back. Expect the system crosshair over the field, with the standard pointer for UI, popups and focus loss; the crosshair must track the mouse without visible lag. Read exact ammo or selected throwable charges at the cursor; fire and reload to compare pips/progress with the corner HUD. Construction hides the resource ring while preserving its placement/aim cursor.

With damage numbers enabled, compare an ordinary hit, weak point, absorbed armor, incoming loss, miss and defense. The larger outlined numbers punch, settle and rise for 0.9 real seconds; nearby resolved values can aggregate without extra injury. Repeat at 4× NPC observation and with reduced effects: counts stay readable and the reduced path has no punch or rise. These visuals never pay rewards or resolve another attack.

The menu uses the chosen survivor’s cohesive low-ready-pistol art, with planted boots and upper-body breathing at the original cabin anchor. Equipment and battlefield consume the frozen actual loadout through the shared source-owned player_model facade. Original bind parity is established separately from acceptance of new action/garment poses; use the current model validation guide for that review.

## First ten minutes

1. Choose and confirm a personal survivor, then choose a starting country, press Start journey and Begin defense. Move the mouse to aim and hold the primary button to fire. Reload with **R**, or with **Reload** under Management › Field actions. The character stays at the left-center lane (lane 2 in the Normal three-lane opening).
2. Let one enemy reach an outer lane's line. Its source lane and windup identify the danger. Any contact lane can threaten the same survivor. Point at that lane and use the secondary button, or **Strike contact lane** under Management › Field actions. Melee consumes stamina and commits recovery; no target means no stamina cost.
3. Watch the boss countdown. The timer announces the boss; only defeating it clears the level. A "BOSS APPROACHING" banner shows at the top of the field for 2 s of real time (the same at 1×, 2× and 4×; a paused battle holds it), then "<BOSS> HAS ARRIVED" for 1.5 s. At 30% HP or less a red band frames the screen and the HP bar reads "LOW"; both stay until HP is back above 35%. Read the exposure and reinforcement cues. Pause at any point with **Escape** (the default Pause key; with construction open the first press only closes it, see Escape below).
4. Retreat when overwhelmed (hold **Retreat & train** for 0.8 s; one click only shows the hint). Results shows the retained rewards above **Upgrade**, **Try again** and **Leave journey**. Press Upgrade and buy an eligible improvement in the shared graph, then **Back to camp**: compare and equip owned gear on Equipment, and continue with **Go to briefing** and **Deploy personally** (or press **Back to battle** on Upgrades to continue at once). Training permits a weapon class; it does not grant the item. Starter pistol proficiency is innate and starter equipment stays protected. Record attempts and purchases: roughly 3–5 early runs is the pacing target, with final country-specific balance evidence reported separately.
5. After learning companion access and meeting its liberation milestone, try Mara as an alternative defender. Watch a few legal one-second actions and inspect the log. A missed attack, a successful defense and absorbed injury are different outcomes. Changing policy does not reset an action timer.

## Fortification practice

Open construction with **B** or its button. **Combat continues** while the grid
and supply panel are open, including in observed expeditions. **Escape** closes
construction without pausing; press Pause (Escape again, by default) if you need time. Place only in the
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
starts a fresh battlefield. See docs/SKILLS_AND_WEAPONS.md in the source repository
for authored effects and compatibility rules.

## Identity, scenery and continuing progression

On the menu and survivor picker, compare Rook and June over the checkpoint scene. The selected survivor is drawn at runtime rather than baked into the background. Confirm that the saved personal appearance returns in the menu, portrait and battlefield. Changing the unconfirmed preview or returning to the menu must not purchase gear or alter combat stats. Check breathing, distant infected, fog, lamp light and quiet parallax, then enable reduced effects and verify a static scene with usable controls. Existing campaigns should continue with their compatible identity.

Inspect woodland and cold battlefield edges at both supported window sizes and high interface scale. A tree should fit fully in its peripheral strip or be omitted; no cut crown/trunk should cover a lane, entrance or contact point.

After every victory, inspect threat, completion XP, next-run balance and dimensions. Normal begins with three lanes/12 yards with 1.25× movement, adding two lanes/two yards every three character levels, capped 17/30. Replay/NPC/worker success raises threat once; defeat/withdrawal do not. Character MAX 50 stops XP without stopping future health/damage growth. Current and legacy 11/30 fields keep saved geometry after settings or XP changes.

Compare two owned instances of one equipment definition at different levels. Inventory/details must show the actual instance level; complete-loadout comparisons, purchase prices, sale values and applicable combat statistics should agree. Buying or looting a higher-level item does not equip it automatically, and old items should not silently gain levels when threat rises. Record rank, item IDs/levels, country, defense, purchases and the actual result separately from fixture resources.

## Quality, critical enemies and overtaking

Compare Poor, Common, Uncommon, Rare, Epic and Legendary names/borders, the quality filter and Highest quality sort. Keep quality, item Lv and unlock tier distinct. Inspect actual comparison and sale values; an old item with no override must retain its original effective values. Shop offers have fixed base quality, and proficiency never grants an item. In Settings → Readability & effects try each Quality markers choice (Colour only, Text, Shape, Text and shape) and the Colour-blind quality palette: every quality must stay tellable apart by its name or its shape alone.

Watch an earned drop at the defeated actor’s position: its quality outline/icon rises and fades for 1.5 real seconds, even at 4× observation. The HUD/debrief receipt says it is already banked. Check the owned item once; flying icons must never require collection, create another item, pay again or change combat randomness.

The user chose to retain GURPS injury rules. In NPC mode a conscious zero/negative-HP hostile can still move/fight with existing injury penalties; inspect signed HP and the explicit CRIT · FIGHTING label. Do not treat zero alone as a kill. Actual dead/unconscious/mortally wounded terminals must stop and become a single paid corpse in the resolving tick. Compare 1×/2×/4× without changing outcomes.

Enemies never block each other (W1-69): faster bodies walk through slower ones without a side step, and every enemy that reaches the line or a wall attacks, so a crowd stacks there. A stack draws slightly fanned out, with one contact band per lane and one windup ring per stack, and every body's own bars. Walls still stop enemies, and traps still cover the whole lane strip.

## Auto-sell

Auto-sell is off in a new journey and in every older save. Before turning it on, finish a run: Results shows the loot line ("Loot N" and the count per quality) and the muted "Auto-sell is off · Equipment › Auto-sell".

1. Open Equipment → Manage equipment → Auto-sell. Turn on "Auto-sell new drops" and read the five threshold chips: Poor only, Common and below (default), Uncommon and below, Rare and below, Epic and below. Each chip shows its best quality's colour and shape. The two highest show an amber warning ("Also sells Rare finds." / "Also sells Rare and Epic finds."). Nothing Legendary is ever auto-sold.
2. Check the four slot boxes, "Keep upgrades for the defender who ran it" and "Also on expeditions and operations". Any change is stored at once, with no Apply button, and is used when the next run ends; nothing is sold mid-battle.
3. Play a run with the default rule. The battle screen and the expedition observer show no sale text: drops fly up and bank as before. Results then shows "Auto-sold N · +C"; hover it for the sold items (at most 12, then "and N more"). A drop that would upgrade the defender who fought stays in the stash. However many items sold, one coin sound plays shortly after Results opens, and the Results line is its visual twin. History's run detail shows the same "Auto-sold N · +C coins".
4. Lock or favourite a Poor or Common drop, equip another and put a third in a preset (shop stock and starters are never auto-sold). Back on the panel, "Would sell N now · +C" counts only the rest (its hover names what is kept and why). Press "Apply rule to stash now…": the dialog asks "Sell N items for C coins?" and names what is kept. Escape or "Keep items" closes it and sells nothing; "Sell N · +C" sells exactly that quote and shows "Sold N · +C". The locked, favourite, equipped and preset items must remain. While a defender is deployed, or when nothing matches, the button is disabled and its hover says why.
5. Later in the journey, when every recent drop is Uncommon or better, the panel may say "Your recent drops are Uncommon or better; raise the threshold to keep selling". Record whether the default threshold still feels useful, and how large the stash has grown (the default rule sells about 2% of all drops over a long journey).
6. With recruits on an expedition or operation, turn "Also on expeditions and operations" off and on between stages: drops from a stage settled while it was off are never sold later by the rule, only by Apply rule to stash now.

Record the threshold, the counts and coins Results showed, and any item sold that you expected to keep (with its item ID, quality and where it was equipped). See docs/PROGRESSION.md, "Auto-sell", in the source repository for the exact rule.

## Gameplay settings, DPS and history

Open Settings → Gameplay · difficulty. Select Easy/Normal/Hard or edit Custom, inspect draft dimensions, then Apply to next run before saving/returning. Compare saved next-deployment settings against an active field: its copied parameters must not change. Use a separate profile for aggressive custom tuning.

Read DPS as resolved injury over five simulation seconds, with a zero-padded opening; hover for average/peak/source totals. Pause/save/resume and 4× observation must preserve the simulation clock, independently of frame rate.

Open History from the header on Camp, Results or Journey upgrades, or from the main menu. Filter mode, Victory/Defeat/Retreated and country; inspect frozen gear/balance and last 40 matching DPS/coin/threat trends. Older finalized runs are not invented. Reopening results or settling workers cannot append/pay twice. Back restores the prior pause.

## Journey safety and equipped parts

On an isolated profile, test New game cancellation before a confirmed reset. A confirmed reset must archive the complete old generation before replacing it; failures keep the old journey active. Difficulty/display/audio/controls remain. Leave and resume a settled journey without resetting its equipment, nodes, money or history.

After both a win and a retreat, Results shows exactly Upgrade, the continuation and Leave journey; open Upgrade and inspect the same Journey upgrades screen and whole tree. Branch focus must not hide the other paths. The continuation and Back to battle must use the legal next/retry stage without buying; country liberation returns to map choice.

Inspect actual equipment on Character: change an owned outfit/accessory/melee or trained firearm explicitly, then compare the shared preview and field layers. Hands should stay on grips, long-gun stock near the shoulder, muzzle/tracers aligned and boots planted through reload/recoil/mirroring. The preview must not spend, equip or alter combat/RNG. Old missing appearance snapshots must remain a valid fallback.

## Country and strategy practice

Country populations determine local defense counts, with data-driven bounds and overrides. Clear every local defense to liberate a country and open its neighbors. Explicit sea routes keep island fronts connected. Zoom, drag, search and inspect the map; garrison and deployment badges show where survivors are assigned.

Default milestones unlock recruitment after one liberated country, parallel expeditions after two, and garrisons, country operations and plague pressure after four. Each recruited survivor owns training and equipment references. A deployed survivor or guard cannot serve in another role simultaneously. A whole-country operation chains real battles and stops on defeat, recall or liberation. It never buys or trains on its own, and it sells only what your auto-sell rule sells while "Also on expeditions and operations" is on.

In strategic practice, resume the world clock and inspect the active outbreak. Respond with a successful defense or assign guards. An unanswered alert reopens the final stage; earned travel routes persist. The seven-minute response window runs in simulation time. Pause and closing the game freeze pressure and income.

## Takeover

A recruit can defend the main run in your place. Use Strategy practice.cmd, or a journey with Survivor network bought and a recruit in camp.

1. Finish a run. Results shows a "Defender" row: you, Mara (once Meet Mara is bought) and each recruit; more than three appear in a drop-down. The Briefing and Camp home's Next defense card have the same picker, and Camp home's Defenders list has "Defend next" on each row with its HP, level and power line. A recruit on guard duty or an expedition is disabled, and its hover names why ("Guarding <country>", "Away on an expedition").
2. Choose a recruit. The Briefing's main button reads "Deploy <name>"; press it. Nothing about the choice deploys on its own, and during a live defense the picker is hidden.
3. In battle, the HUD names the recruit (its first name in capitals) with its own portrait, and the field draws its figure, not Mara's. Open Tactics and change the policy or the recall threshold: the change acts at the recruit's next action and is kept on the recruit, not on Mara.
4. While it defends, Camp home shows "Active defense · <country> · Defense N · <name>" and the recruit as Defending. In Operations, sending, guarding, training or equipping it is refused with "Already defending the front", and nothing changes.
5. Win once, lose once and retreat once with the recruit. Results and History name it. A win still gives you the completion XP, and the recruit's victories and advancement grow while Mara's stay as they were. After every outcome the recruit is Ready again with its items free, and the continuation sends the same recruit unless you choose another.
6. Choose a recruit for the next defense, then send it on an expedition or a guard duty. The log says "<name> left the next defense; you will defend next." and the picker shows you selected.
7. Save and return to the menu during a takeover, then resume: the same recruit defends, with the same items and tactics.
8. With an outbreak active, press "Plan response" on Camp home or in Operations. The country's Briefing opens on its final stage and nothing deploys until you press the main button.
9. When an expedition ends, one toast reports it (for example "<recruit> liberated <country> · 2 new fronts"), and Camp home's "Recent reports" keeps the line. Resume the save: the toast does not repeat. During a live, unpaused defense the toast waits until you pause or leave the field.

Record who defended each run, the outcome, whether the names and the figure matched the recruit, every refusal text you saw, and any toast that was missing or repeated. See docs/STRATEGY.md, "Recruit takeover", in the source repository for the exact rules.

## Escape

Escape does one thing per press, topmost first. Check each row at 1440x900 and at 1050x700 with 135% interface scale, in normal and reduced effects:

| Where you are | One Escape press |
|---|---|
| A rebind prompt in Settings | Cancels it; the old key stays |
| An open window: the quit or return-to-menu question, a save error, unapplied difficulty changes, Recruit, Apply rule to stash | Closes it; unapplied difficulty changes keeps you editing, and Apply rule to stash sells nothing |
| An open dropdown or menu, or a text field you are typing in | Closes it, or leaves the field |
| Operation details, the Operations observer window, an Equipment sale confirmation | Closes it |
| Main menu | Asks "Quit Last Light?" |
| Survivor selection, or the world map before Start journey | Main menu |
| Briefing, Equipment, Character, World map, Mara or Operations | Camp home |
| Camp home | Results, when the last defense has settled and the journey is running; otherwise asks "Return to the main menu?" |
| Results | Asks "Return to the main menu?" |
| Journey upgrades | Results, when the last defense has settled and the journey is running; otherwise Camp home |
| Battle, holding Retreat & train | Cancels the hold; nothing else happens |
| Battle, construction open | Closes construction; the battle keeps running |
| Battle, otherwise | Pauses or resumes while Pause is bound to Escape (the default); with Pause on another key, Escape does nothing in battle |
| Run history | Back to where you opened it |
| Settings | Saves and returns; with an unapplied difficulty change it asks first: Apply and return, Discard and return, or Keep editing |

While "Saving…" shows, Escape goes nowhere. Record any press that did something else, paused a battle you did not mean to pause, or closed two things at once.

## Controls and recovery

Default keys: primary mouse fires the equipped gun (or a press throws the selected throwable), secondary mouse uses contact melee, **R** reloads, **1/2** use purchased personal abilities, **B** opens/closes live construction, **Escape** pauses the battle (see Escape above), **F** toggles fire, **Q** cycles the unlocked attacks (gun, grenade, Molotov), **P** pauses or resumes the world clock on Camp home, and **F12** captures the native framebuffer. Settings remap essential actions and offer toggle fire, aim assistance, text scaling, outlines, reduced effects, optional blood and independent sound levels. Combat hints reflect the saved bindings.

Mouse-only play: the battle bar no longer has Reload, Strike or Toggle firing buttons (design-ux-flow risk 6). Their keys still work and the legend still names them. With the mouse alone, open **Management** on the battle bar, then use **Field actions**: **Reload**, **Strike contact lane** and, with the gun selected, **Toggle firing** (or **Stop firing**), each with its key beside it. A click does what the key does and closes the menu, so the field gets the mouse back. Firing (hold the primary button) and melee (the secondary button) need no menu; Pause and the Gun, Grenade and Molotov selector stay on screen as buttons. Construction and an observed battle have no field actions. Play one defense without the keyboard and record any action you could not reach, or any click that also fired or struck on the field.

Save and return to the menu, quit, and continue the same deployment. Reload progress, actions, positions, RNG and banked rewards should resume together. A save failure keeps the application open and opens a "Save failed" window with Retry, Open folder, Copy path and Dismiss; an unreadable save or settings file opens the same kind of window at startup. Manual check: Open folder shows the save folder in the file manager (if it does not, Copy path still gives the folder). Never remove a save to test recovery. Use separate practice or `LASTLIGHT_SAVE_DIR` instead.

## Logs and crash reports

Each run writes `%LOCALAPPDATA%\LastlightHordeDefense\logs\lastlight-<unix-timestamp>.log` (or `<LASTLIGHT_SAVE_DIR>\logs\`); the newest 10 are kept. If the game crashes it also writes `crash-<unix-timestamp>.txt` in that folder with the version, OS, panic message, backtrace and the last 200 log lines. Send the newest crash file and log with your feedback. Set `RUST_LOG` to change verbosity; the default is `warn,horde_core=info,horde_defense=info,wgpu=error,naga=warn`.

## Atmosphere and feedback pass

Keep your ordinary campaign intact and use practice or a separate `LASTLIGHT_SAVE_DIR` for this comparison. Inspect a temperate daytime location, an arid approach, a cold region and a rainy authored stage. Scene tags choose the environmental layers; clear weather must not acquire rain just because its biome is tropical. Bird-like nature calls are absent at night. Changing region or opening an observed expedition should crossfade the environment rather than restart it abruptly.

1. Set combat effects to zero and leave Atmosphere audible. Listen for low wind, distant urban detail or daytime nature, with rain/cold air only where the scene supports it. Watch for an obvious repeat boundary over several loops.
2. Set Atmosphere to zero and restore combat effects. Compare the six legal firearm classes, watch Mara fire, and trigger a turret, trap, grenade blast and Molotov fire. Check that their timbres communicate the source without implying another hit roll or injury. An absorbed hit should differ from health damage.
3. Pause during a scene transition, open settings, and change the audio sliders. Authoritative combat and strategic clocks remain frozen. Cosmetic ambience may continue more quietly; its fade cannot generate resources or advance recovery. Reduced-effects settings should not silently override your audio levels.
4. Mute Master while a cue is playing, then unmute. Check for complete silence, no delayed burst from a clip that was still loading, and a gentle ambient return. At normal levels, wind and rain should remain behind danger cues and visible enemy windups.
5. Observe one parallel expedition, then close its observation window. Ordinary fire from every off-screen survivor should not become a wall of sound. Boss/structure alerts still need to be recognizable when relevant to the campaign.

The audio guide (AUDIO_SURVIVAL.md in the source repository) documents the 275 survival files, the buses, the mix table and the loudness gate. This package ships no audition reel; listen in the game.

Record the country, local stage, weather/lighting tags, defender, the five audio slider values, the Mono audio setting and whether you used speakers or headphones. File/header checks do not establish listening quality; report masking, repetition or a missing source cue as a playtest result.

## Listening pass

Status: **PENDING** (no human listening pass has been logged for the production-pass mix). The mix is measured offline at -23 ±2 LUFS integrated with true peak at most 0.95 FS at the default sliders; that number does not say how it sounds on your device, which is what this pass checks.

Play one ordinary battle of a few minutes at the default sliders (Master 0.8, Combat effects 0.8, Atmosphere 0.8, Music & stingers 0.7, Interface sounds 0.7), then repeat it with Mono audio on. Listen for:

1. Each firearm class, the turret and the reloads: distinct, never one repeated sample, and no click or clip on a dense volley.
2. Enemy windups and contacts under a crowd: still audible over gunfire, and always matched by the windup ring or aim line on screen.
3. A boss arrival and phase: the atmosphere dips under the cue and comes back; the phase label and ring show for the whole hold.
4. Victory, defeat or withdrawal: one stinger, not cut off, at a level that fits the battle.
5. The bed against gunfire: wind, rain or city stays behind the guns and warnings and does not sound too forward.
6. Lane pan: a far-right cue sits a little to the right, and nothing drops out with Mono audio on.
7. Results and the armory: one loot reveal, one coin sound per sale batch, no sound on hover.

Log one entry per session:

| Field | Value |
|---|---|
| Date and build stamp (main menu or `--version`) | |
| Device: speakers or headphones, and their model | |
| Output: OS volume and any system effects (spatial sound, loudness equalisation) | |
| Sliders: Master / Combat effects / Atmosphere / Music & stingers / Interface sounds | |
| Mono audio: on or off | |
| Reduced effects: on or off | |
| Country, stage, weather and defender | |
| Findings: masking, repetition, harshness, level, pan | |

## Click-to-photon

Status: **PENDING** (no click-to-photon run has been logged for the production-pass build). This measures how long a mouse click takes to reach the screen.

1. Use a 60 Hz display. Note the GPU, its driver, the refresh rate, whether hardware-accelerated GPU scheduling (HAGS) is on, and the build stamp on the main menu.
2. Start Battle practice.cmd, unpause, aim at mid-field and fire 20 single shots, at least 1 s apart.
3. Film the mouse button and the muzzle area together with a phone at 240 fps or more (one frame is 4.17 ms at 240 fps).
4. For each click, count the frames from the button bottoming out to the first changed muzzle-flash or tracer pixel: ms = frames x 1000 / fps.
5. Repeat steps 2-4 in three rows, restarting the game after each Settings change: VSync on with Low latency off (the default), Low latency on, and VSync off with Low latency off. In each row also note whether the drawn reticle trails the system cursor during fast flicks.

Targets: a median of at most 50 ms in the default row and at most 34 ms with Low latency on. Any single click over 67 ms is a finding: report it with its row and frame count.

| Field | VSync on, Low latency off (default) | Low latency on | VSync off |
|---|---|---|---|
| Date and build stamp | | | |
| GPU, driver, refresh rate, HAGS | | | |
| Camera fps | | | |
| Frames per click (20 values) | | | |
| Median ms | | | |
| Max ms | | | |
| Reticle trails the cursor? | | | |

## Reporting a bug with a recording

Packaged playtest builds do not include recording. For a reproducible bug, use a developer build (`cargo run --locked -p horde-defense --features dev`, optionally with `LASTLIGHT_SAVE_DIR` pointing at a copy of the save):

1. Start recording with **F7** (or launch with `--record bug.jsonl`) shortly before the problem; the file is `<save dir>/recordings/rec-<timestamp>.jsonl`.
2. Reproduce the problem, then press **F7** again to stop. Press **F6** to dump the game state if the screen shows something wrong, and **F12** for a screenshot.
3. Attach the recording, the dump, the newest log and crash file (see Logs and crash reports) and say what you expected.
4. A developer replays it with `horde-defense --replay bug.jsonl`, which prints the final `state_digest`; the same recording must give the same digest on the same build.

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

Personal body/skill/fortification purchases now share docs/JOURNEYS.md; actual modular appearance is described in docs/CHARACTER_COMPONENTS.md (both in the source repository). Human feel, NPC stutter and device performance require their own current evidence; no visual/compilation result alone closes them.

For an isolated durable-save check, request menu/return/quit during a save and during confirmed New game. Preferences/model mutations must stay gated for the critical transition, old autosave success must not dismiss a newer pending request, and no stale Quit may exit a newly reset profile. A failed save remains visible. Native IO now uses a bounded single-writer snapshot service; visual smoothness and NPC timing still require the current measured evidence.
