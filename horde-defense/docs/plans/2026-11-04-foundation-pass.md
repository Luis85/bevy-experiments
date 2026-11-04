# Foundation pass: loop, campaign, skills, battle, shell

Branch `claude/horde-foundation-pass`, cut from local `main` at 613e66b (after the tree and flow overhaul). This pass is not a redesign. It makes the game hang together from the first launch to the mid-campaign, and it investigates the reported mouse sluggishness.

## Evidence

- **Native captures, before:** `target/foundation-before/` holds 20 scenes at 1440×900 @1.0 and at 1050×700 @1.35, made with `LASTLIGHT_REVIEW_*` and an isolated `LASTLIGHT_SAVE_DIR`. Selected pairs are copied to `docs/screenshots/foundation/`.
- **Pacing:** `cargo run --locked --release -p horde_core --example pacing -- 60 --runs` drives `horde_core::sim::simulate_pacing`. It is a scripted fresh profile (Rook, a starting country, the tree's recommended node, then gear) through 60 main rounds of the real simulation, headless, as `--smoke` does.
  - Three profiles:
    - an expert bot that re-aims every tick, melees, heals and braces;
    - a novice bot that re-aims every 0.5 s and only shoots and melees;
    - a novice that buys gear before skills in a small country.
  - "Battle min" counts simulated combat only, not time in camp.
- **Frame evidence:** `LASTLIGHT_PERF_REPORT` reports for the `battle` and `progressed-peak` motion scenes at both sizes, in `target/foundation-perf-before/`.
- **Read-only audits** of the flow, the skill catalogue, the battle HUD and the input/present path. Their findings are folded in below, with file references.

## Loop map (fresh profile → mid-campaign)

| # | Screen / state | Driven by | Next |
|---|---|---|---|
| 1 | Main menu: New game / Resume journey | `ui/menu.rs`; `runner::begin_new_journey` archives an existing profile first | 2 |
| 2 | Survivor select (Rook / June, cosmetic) | `Command::ChooseSurvivor` | 3 |
| 3 | World map: choose an infected starting country | `Command::ChooseStart` → `choose_start_country` + `deploy_country_stage(0)` | 4 |
| 4 | **First defense**, live Personal combat | `Game::tick`; `Report` on finish | 5 |
| 5 | Defeat or retreat → Results = Journey upgrades (one tree) | `Command::Withdraw` / `finish` → `Screen::Results` | 6 |
| 6 | Learn nodes, change equipment | `Command::BuySkillNode`, Equipment tab | 7 |
| 7 | Try again → same stage, live | `Game::continue_journey` (Deployed) | 4 |
| 8 | **First victory** → "Next defense" | `continue_journey` advances `completed(country)` | 4 |
| 9 | Victories 1/2/3 open the trap/wall/turret blueprints; learning Field engineering opens **Fortifications** | `minimum_victories`, `FeatureUnlock(Construction)` | 4 |
| 10 | Final stage won → **liberation** → "Choose next country" | `continue_journey` → `CountryLiberated`; neighbours unlock | 11 |
| 11 | World map: enter a neighbour | `Command::EnterCountry` → Briefing | 4 |
| 12 | 1 liberation: **Mara** (Meet Mara) and Recruitment become learnable. 2: Parallel defenses. 4: Country operations | `minimum_liberated` + `FeatureUnlock` | strategy layer |

## Findings

Severity: **H** high, **M** medium, **L** low. Captures named `x.png` / `x-small.png` are in `target/foundation-before/`.

### G: game loop

- **G1 (H): the first victory takes 5–6 runs.** On Germany, all three pacing bots lose stage 1 four or five times before the first win: the expert at run 5, the novice at run 6. Every defeat comes at or after the boss arrives at 180 s; the stage-1 boss has 2,400 HP. A new player meets a wall of identical defeats on Defense 1.
- **G2 (H): after the first victory the loop never pushes back.** The bots win every one of the next 54–55 runs, in every country and stage. Player power from the tree and gear outgrows the +1.5% enemy HP per threat rank.
- **G3 (H): the reason for a loss is hidden or misworded.** `Report.tip` is computed (`sim/lifecycle.rs:383`) but never rendered. The cause shows as a 12 pt truncated line (`results.png`). A personal defeat reads "Retreated from the overrun line in lane N", and the lane falls back to "6" on a 3-lane field (`sim.rs:1128`).
- **G4 (M): one outcome has three names.** Results says "Retreated" (`ui/results.rs:55`), History says "Withdrawal" (`history-small.png`), and expedition reports print `{:?}`.
- **G5 (M): a victory doesn't say what it opened.** Victories 1/2/3 open the blueprints, and liberations open Mara, Recruitment and Operations. Nothing on Results names a newly learnable node (`journey-victory.png`).
- **G6 (M): Results duplicates routes.** "Run history" duplicates the header's History, and More → "Plan or replay" duplicates Briefing. "Saving…" disappears without a confirmation (`ui/results.rs:114`).

### C: campaign and world

- **C1 (H): liberation is silent.** "Choose next country" returns to the map with "Journey continued." The freed country stays selected and no new front is named (`journey-liberated.png`, `ui/notices.rs:253`). The header shows a cryptic "1 freed".
- **C2 (H): "Plan outbreak response" starts a live defense,** because core `EnterCountry` deploys at once (`ui/operations.rs:46`, `command.rs:224`).
- **C3 (M): the Build key and Briefing give wrong text for fresh profiles.** They say "first boss clear opens trap blueprints" (`ui/battle.rs:78`, `ui/briefing.rs:220`). Fresh profiles actually need Field engineering plus the 25-coin Trap blueprint after 1 victory.
- **C4 (M): Operations shows a single-gate message with broken grammar** ("Liberate {} country", `ui/operations.rs:114`) and doesn't name the missing tree node. Finished expeditions are never announced.
- **C5 (L): the world-map footer advertises G/D shortcuts that don't exist** (`worldmap.rs:405`).
- **C6 (measured): the first liberation takes a whole country.** Germany has 9 stages, so liberation comes at run 13–14 (52–54 battle min). Luxembourg has 3 stages: run 6 (21 min). Nothing at country choice says that a large country delays Mara.

### S: skills and progression content

- **S1 (H): the recommender walks into traps.**
  - Rule 1 hard-codes Reload drill, even when Steady hands or Field fitness (20 coins, much stronger) are better.
  - Rule 2 takes the cheapest buyable node in catalogue order. So on run 2 the pacing bot buys all five body rank-1 nodes (+5 HP, +5 melee or 2.5% reload each), when Field fitness alone gives +35 HP.
  - It also recommends weapon classes with no owned gun, Field engineering before any blueprint is learnable, and Reinforced frames with nothing to reinforce.
- **S2 (H): early tiers are far too cheap.**
  - Depth 1 costs 495, depth 2 another 850, depth 3 another 795.
  - So tiers 1/2/3/4 are affordable after 2/4/5/6 runs, against a target of 3–5 runs per tier.
  - The whole 18,125-coin tree is owned by run 13–14. After that nothing is left to buy except the four companion nodes.
- **S3 (M): useless and dominant nodes.**
  - Traps: the body rank-1 nodes next to their dedicated equivalents; Trap mastery 1 (a base trap already kills every common enemy); Quick throw (it shortens 1.5–2 s cooldowns).
  - Dominant: Steady hands (+67% pistol damage for 20) and Hardened (+3 flat armor turns a Shambler hit from 4 to 1).
- **S4 (M): icons.**
  - The four Companions and world nodes use `companion`, which has no mapping, so **they draw a gun**. The root's `pistol` also falls back.
  - Blueprints and masteries all draw the wrench, so the trap, wall and turret pictograms are never used (`skills-small.png`).
- **S5 (L): readability.**
  - Brace says "reduced damage", but it halves damage.
  - Grenade and Molotov give no numbers.
  - Reload drill says "20% faster" but means ×0.80 time.
  - Quick throw says "shared cooldown", but cooldowns are per kind.

### B: battle

- **B1 (H): the first-defense hints sit only inside the Management menu** (`ui/battle.rs:433-452`). The field itself says only "Hold fire to shoot" (`intro.png`).
- **B2 (H): the first deployment starts live** (`command.rs:206`). The "Begin defense" state exists but is never reached on a first run.
- **B3 (H): default key bindings are never shown.** The legend is painted only when a binding differs from its default (`ui/battle.rs:332`).
- **B4 (M): there is no on-field pause indicator.** Escape freezes the field silently, and Escape also closes construction *and* pauses.
- **B5 (M): boss arrival and low HP are easy to miss.** The boss banner is 14 pt for under 1 s, and there is no low-HP warning.
- **B6 (M): "Retreat & train" ends the run on one click,** right next to Management.
- **B7 (M, measured): difficulty is front-loaded.** Stage 1 is the only stage the bots ever lose (G1, G2).

### U: user flow and shell

- **U1 (H): Escape in camp toggles the world clock even when the clock control is hidden.** With a paused defense it jumps into the battle and resumes it (`ui/camp.rs:298`). Everywhere else Escape means Back.

  | Screen | Escape today |
  |---|---|
  | Menu | nothing |
  | New-journey modal | nothing |
  | Survivor select | nothing |
  | Camp | toggles the clock |
  | Battle | pauses |
  | Results | nothing |
  | History | back |
  | Settings | saves and goes back |
- **U2 (M): History's "Clear filters" is a `small_button`,** 12 pt shorter than the dropdowns beside it (`ui/history.rs:78`, `history-small.png`).
- **U3 (M): the Results outcome line wraps** at 1050 @1.35 with long country names (`ui/results.rs:80`).
- **U4 (M): Escape is mishandled in Settings and the new-journey modal.** Escape while rebinding binds Escape instead of cancelling. The new-journey modal ignores Escape.
- **U5 (M): "Leave journey" can leave "Saving…" stuck** if the runtime rejects it during a durable transition (`ui/results.rs:175`).
- **U6 (L): one thing, several names.** Saving is "Save campaign", "Save journey" and "Save deployment". "Survivor" means both Rook/June and Mara. Debug `{:?}` names leak into Operations.

### P: performance and mouse (user report: "a bit sluggish")

- **P1 (H): the arena hides the OS cursor and paints its own reticle,** which trails the pointer by the whole present path.
  - The causes are Fifo vsync with `desired_maximum_frame_latency` 2 (Bevy defaults, `main.rs:113`) plus pipelined rendering, where the render world runs one frame behind.
  - Together that is about 3–4 frames (50–67 ms at 60 Hz) of visible lag. A hardware cursor has about 0–1 frame.
  - Menus and the tree have the same latency, without the hidden cursor.
- **P2 (measured): throughput is not the problem.**
  - Frame reports show 60.0 FPS in every case: p99 is 17.2–17.8 ms, except 24.6 ms at peak/small, and at most 1 frame is over 33 ms.
  - Interface work averages 0.9–1.7 ms, with a maximum of 9.8 ms. Simulation averages 0.03–0.16 ms. These are dev-build figures, at 14 and 225 enemies.

## Pacing table (before)

Each cell is runs / battle minutes until the milestone first holds.
- "Tier N affordable" means the coins earned so far would buy every node of prerequisite depth ≤ N.
- "Tier N first node" (in the example output) is the run before which the bot first owns a depth-N node.

Seeds are fixed, so the table is reproducible.

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory | 5 / 18 | 6 / 21 | 4 / 14 |
| First liberation | 13 / 52 | 14 / 54 | 6 / 21 |
| Tier 1 affordable (495) | 2 / 6 | 2 / 6 | 2 / 6 |
| Tier 2 affordable (1345) | 4 / 14 | 4 / 13 | 4 / 14 |
| Tier 3 affordable (2140) | 5 / 18 | 5 / 17 | 5 / 17 |
| Tier 4 affordable (2800) | 6 / 21 | 6 / 21 | 5 / 17 |
| Fortifications | 5 / 18 | 5 / 17 | 7 / 24 |
| Mara | 13 / 52 | 14 / 54 | 8 / 29 |
| Parallel defenses | 16 / 64 | 17 / 67 | 11 / 41 |
| Country operations | 22 / 89 | 23 / 92 | 19 / 75 |
| Defeats after the first victory (60 runs) | 0 | 0 | 0 |

Targets for this pass:
- the first victory by run 2–4;
- 3–5 runs per early tier, so tiers 1–3 become affordable at about runs 4, 8 and 12;
- the tree does not run out before the first liberations;
- the feature unlocks stay in order: Fortifications, then Mara, then Operations.

**Limitation.** The bots aim at the nearest visible enemy; the novice only re-aims twice a second. The game's aim assist narrows the gap between them, so real players will be slower. The run counts are a floor, not a human playtest.

## Chosen fixes, in waves

Each wave ends with captures at both sizes and the full verify. Rules live in `horde_core` as tested functions; `horde_game` only renders, and changes go through `Game::apply`.

**Wave 1: input latency and battle clarity**
- **P1:**
  - Over the arena, show the native **crosshair** cursor instead of hiding it. The painted reticle keeps only its ammo/reload readout and the ordnance and placement rings.
  - Set `desired_maximum_frame_latency` to 1.
  - Drop pipelined rendering. Frame work is at most about 10 ms, so serial main and render work still fit in 16.7 ms. *(Measured, not shipped; see the Outcome.)*
  - Re-measure the frame reports.
- **B1, B3:** show the current first-defense hint as an on-field callout with "Hide hints", and always paint the key legend.
- **B2:** in core, a profile's first deployment starts paused, and "Begin defense" teaches the controls.
- **B4:** add an on-field "Paused" banner.
- **G3, G4, U3:**
  - Add core `Outcome::label()` and use it on every screen.
  - The defeat cause becomes "Overrun in lane N", without inventing a lane.
  - Results shows the cause and `Report.tip` wrapped, not truncated, and the outcome line truncates.

**Wave 2: loop, campaign and shell**
- **G5, C1:**
  - Add core `Campaign::newly_learnable_nodes()` (unowned nodes whose milestone gate the current counts have just met) and `WorldProgress::newly_enterable(country)`.
  - Results lists "Now learnable: …".
  - On liberation, Results names the freed country and the fronts it opens, and the map preselects the first front.
  - The header says "1 liberated".
- **C2:** rename "Plan outbreak response" to "Defend now", which is what it does.
- **C3, C4:** a core construction blocker and an operations unlock blocker name the exact missing step.
- **C5:** remove the phantom G/D hint.
- **G6:** drop "Run history" from Results (the header has History) and show the save status. *(The save status was reverted; see the Outcome.)*
- **U1:** Escape in camp toggles the clock only when the clock is shown, and never jumps into a battle.
- **U2:** make "Clear filters" full size.
- **U4:** Escape cancels a rebind, and the new-journey modal closes on Escape.
- **U5:** disable "Leave journey" while a durable transition is pending.

**Wave 3: skills and pacing**
- **S1:** core `recommended_node` ranks buyable nodes by whether they are useful now, and takes the cheapest within the most useful class.
  - Useful now: an effect that applies to the mode and can act. Body ranks come next.
  - Never recommended: a weapon class without an owned gun; construction with no learnable blueprint; structure ranks or frames with no blueprint.
  - The hard-coded Reload drill is removed.
- **S4:**
  - Map `pistol`.
  - Add a vector `Companion` pictogram for the four world nodes.
  - Blueprints and masteries use their trap, wall and turret pictograms.
  - Only `icon` fields change.
- **S5:** text-only description fixes.
- **S2, G1, G2, B7:** change prices and the stage-1 boss only. Every step is re-measured with the pacing table. Ids, gates and positions stay unchanged.

## Rejected options

- **`AutoNoVsync` as the default present mode.** It cuts latency further, but it tears, burns laptop power and needs a frame cap. Frame latency 1 plus a hardware cursor removes the visible trail.
- **Removing the reticle.** The ammo and reload readout at the pointer is useful. Only the cross moves to the OS cursor.
- **Halving coin income instead of repricing the tree.** That would also slow gear, which already paces with item level. The tree is what runs out.
- **Removing or merging body-rank nodes.** Ids never change and saves own them. Ranking them below the dedicated nodes fixes the trap without touching ownership.
- **Auto-pausing construction, or a tutorial overlay.** The first is a combat-rule change outside this pass. The second is a wall of text; one-line hints on the field teach in context.
- **A new `SelectCountry` command for outbreaks.** That is a new command surface. The honest label removes the surprise now; the command is a follow-up.

## Outcome

**Process deviation.** The waves were implemented in order in this worktree. However, the code for all three was in place before the first full gate ran, so the full verify, the after-captures and the pacing table cover the finished branch, not each wave separately. A fixture-only subagent updated the tests that depend on prices; it changed no rules or content.

### Findings resolved

| Finding | Result |
|---|---|
| P1 | Over the arena the **system crosshair cursor** is shown instead of being hidden, so the aim point has no presentation lag. The painted readout (ammo and reload ring, ordnance and placement rings) stays beside it, and the diagnostic capture still paints the cross. `desired_maximum_frame_latency` is 1, which saves one queued frame. The `CustomCursor` plumbing is removed. **Pipelined rendering is kept** (see Performance below). |
| B1, B3 | The current first-defense lesson is a one-line callout over the field. It names the player's own keys and has a "Hide hints" button. The key legend is always painted. |
| B2, B4 | Core: `ChooseStart` deploys the journey's first defense **paused**. A READY/PAUSED banner names a frozen field. |
| G3, G4, U3 | Core `Outcome::label` (Victory / Defeat / Retreated) is used on Results, History and expedition reports. The defeat cause reads "Overrun at the line in lane N", with no lane when none was recorded. Results shows the cause and `Report.tip` wrapped. The outcome line truncates and shows the full text on hover. |
| G5, C1 | Core `Campaign::milestone_opened_nodes` and `WorldProgress::fronts_opened_by`. A victory names what it opened: "Germany liberated. New fronts: …" and "Now open in the tree: Trap blueprint, Meet Mara, …" (rank ladders excluded, at most four names). After a liberation, the map opens with "<Country> liberated · N new fronts" and the easiest front preselected. The header says "N liberated". |
| C2 | "Plan outbreak response" is now "Defend now" with hover text, because it deploys. |
| C3, C4 | Core `Campaign::construction_blocker` names the next missing build step. The Build key and the Briefing now use it instead of the wrong "first boss clear" text. The Operations gate text is grammatical. |
| C5 | The phantom "G: GARRISON · D: DEPLOYMENT" map hint is removed. |
| G6 | "Run history" is removed from Results; the header has History. Showing a save confirmation on Results duplicated the tree's learned line, so it was reverted (see follow-ups). |
| U1 | Escape in camp drives the clock only where the clock is shown, and never resumes a waiting main defense. |
| U2 | "Clear filters" is as tall as the dropdowns and is inactive when no filter is set. |
| U4 | Escape cancels a key rebind. The new-journey prompt closes on Escape. |
| U5 | "Leave journey" is disabled while a save or reset is pending. |
| S1 | Core `node_helps_now` / `best_buyable_node`. The recommendation is the cheapest affordable node that acts now, with body ranks after those. Weapon classes need an owned gun; construction needs a learnable blueprint; structure upgrades need a blueprint. The hard-coded Reload drill is gone. |
| S2, G1, G2, B7 | Prices scale with depth: ×2 at depth 1, ×2.5 at depth 2, ×3 below that. Blueprints, feature unlocks and the root are unchanged. The tree goes from 18,125 to 52,180 coins. The stage-1 boss goes from 2,400 to 1,100 HP. Numbers are below. |
| S4 | A vector `Companion` pictogram for the four world nodes. Blueprints and masteries draw the trap, wall and turret pictograms, and `pistol` is mapped. A test pins both. |
| S5 | Text fixes:<br>• Brace: halves damage.<br>• Reload drill and Rapid cycling.<br>• Quick throw: cooldowns are per kind.<br>• Grenade and Molotov: damage and duration.<br>• Hardened.<br>• Field engineering: when the trap blueprint opens.<br>• Steady hands.<br>• The root node. |
| Review scenes | Added `skills-search`: the search "reload" plus the goal path to Combat reload. The existing `intro`, `journey-victory` and `journey-liberated` scenes now show the first defense, the first victory and a liberation with the new lines. |

**Balance and digests.** Ids, gates and positions are unchanged. The golden determinism digests and gameplay traces did **not** move: those runs buy nothing and last 30 s, ending before any boss. `--smoke` still clears five stages in both modes. Personal stage 1 still falls on attempt 3, with 488/610 coins instead of 515/616, because of the lower boss HP and the repricing.

### Pacing table (after)

`cargo run --locked --release -p horde_core --example pacing -- 60 --runs`; each cell is runs / battle minutes. The pacing bot now also builds the structures it owns, as smoke's NPC route does. This is a measurement change: a bot that buys fortification ranks but never builds understated its own power.

| Milestone | expert DEU before → after | novice DEU before → after | novice LUX before → after |
|---|---|---|---|
| First victory | 5 / 18 → **4 / 13** | 6 / 21 → **3 / 9** | 4 / 14 → **2 / 6** |
| First liberation | 13 / 52 → 14 / 60 | 14 / 54 → 14 / 59 | 6 / 21 → 5 / 18 |
| Tier 1 affordable | 2 → 3 | 2 → 3 | 2 → 3 |
| Tier 2 affordable | 4 → 5 | 4 → 5 | 4 → 5 |
| Tier 3 affordable | 5 → 7 | 5 → 6 | 5 → 6 |
| Tier 4 affordable | 6 → 7 | 6 → 7 | 5 → 7 |
| Fortifications | 5 → 4 | 5 → 3 | 7 → 3 |
| Mara | 13 → 14 | 14 → 14 | 8 → 5 |
| Parallel defenses | 16 → 17 | 17 → 17 | 11 → 8 |
| Country operations | 22 → 23 | 23 → 23 | 19 → 18 |
| Defeats after the first victory | 0 → 2 (stages 5, 6) | 0 → 3 (stages 2, 4, 4) | 0 → 1 (stage 2) |
| Last run with something left to learn | 23 (core tree done by 13) → 24 | 23 (by 14) → 24 | 19 → 26 |

**Reading the table.**
- The first victory now lands in the 2–4 target in every profile.
- The mid-campaign pushes back with single defeats instead of a 55-run win streak.
- The tree lasts about twice as long.
- The unlock order is unchanged.
- Early tiers now take about 2 runs each instead of 1–2. That is still under the 3–5 target. In an intermediate measurement, pushing prices further turned stage 5 into five straight defeats. The rest is left to a human playtest, since the bots aim almost perfectly.

### Performance and mouse (after)

Frame reports (300 samples, dev build, same machine):
- Every case holds a **60 FPS median** with p95 ≤ 18.3 ms.
- Interface work averages 0.9–1.7 ms, with a maximum of 7.6 ms. That is unchanged; the crosshair only removes work.
- Some runs showed a single 0.5 s frame. A/B runs reproduced it at the same rate with Bevy's default frame latency and with pipelined rendering on or off. It is environmental, not caused by this change, but it pulls those runs' mean FPS down to 54.4.
- Disabling pipelined rendering, which would save one more frame of latency, was measured but **not shipped**. The frame reports don't time the render world, so the risk on slower GPUs can't be bounded.
- The crosshair itself can't be measured from frame reports: it replaces a painted cross that trailed by 3–4 frames with the OS cursor. Feeling it needs a human at the mouse.

### Verification (final HEAD)
- `cargo fmt --all -- --check` passes.
- `cargo clippy -D warnings` passes for the workspace (all targets), with `--no-default-features`, and with `--features dev`.
- `cargo test --locked --workspace`: 644 passed, 0 failed, 1 ignored (the offline audio exporter).
- `cargo test --locked -p horde-defense --features dev`: 343 passed, 0 failed, 1 ignored.
- `cargo run --locked -p horde-defense -- --smoke` passes.
- Native captures: `target/foundation-before/` (40) and `target/foundation-after/` (42, including `skills-search`). Selected pairs are in `docs/screenshots/foundation/`.

### Follow-ups
- B4–B6:
  - a larger boss-arrival banner;
  - a low-HP warning;
  - a confirmation for "Retreat & train";
  - Escape should close construction without pausing.
- HUD:
  - at 1050×700 the bottom bar can wrap to three rows once Heal, Brace and Build are unlocked;
  - construction mode rescales the arena.
- C2: a core `SelectCountry` command, so an outbreak can be planned instead of only defended at once.
- C4:
  - announce finished expeditions;
  - Operations should name the missing tree node.
- C6: the starting-country choice should say that a large country (Germany: 9 defenses) delays Mara.
- G6: a save confirmation on Results that doesn't duplicate the tree's learned line.
- U4:
  - Settings drops an unapplied difficulty draft without warning;
  - the scale slider applies while dragging.
- U6:
  - one "Save" label everywhere;
  - name Mara in her actions instead of "Survivor".
- S3:
  - Trap mastery and Quick throw stay weak;
  - Steady hands and Hardened stay strong;
  - rebalancing their effects is a content pass.
- Pacing: early tiers take about 2 runs each against the 3–5 target. Revisit after a human playtest, because the bots overstate player skill.
- Perf: time the render world in frame reports before reconsidering disabling pipelined rendering.
