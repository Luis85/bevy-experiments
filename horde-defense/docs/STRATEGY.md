# Late-campaign strategic systems

`StrategyState` is saved with `Game`. It owns recruited survivors, guard assignments, plague alerts, a seeded random generator, a strategic tick clock, bounded explanatory history and the maximum number of simultaneously liberated countries seen so far. Historical unlocks therefore survive a country being reopened by plague pressure.

## Configuration and identity

`world.json` contains `strategy_config`: recruitment after 1 liberated country, parallel operations after 2, grand strategy after 4, up to 4 parallel deployments, 12 recruits, and 3 guards per country. A recruit costs 100 shared coins. These defaults are intentionally configurable rather than hard-coded progression gates in the UI.

Recruitment requires a currently liberated country and records that country as the survivor's origin. Each recruit has a persistent ID and name, a full independent `Npc` record, character-point awards/investments, loadout references, objectives, tactical policy and deployment history. Recruitment grants a genuinely new protected starter set with four new globally unique armory IDs. It never creates a second reference to an already deployed rifle as a substitute for ownership.

An idle survivor may equip owned items, train, deploy, or join a garrison. A survivor has exactly one status: Ready, Deployed or Guarding. Taking a deployment or guard role reserves that survivor's actual loadout in `Campaign::reserved_items`. Reservation rejects another concurrent use, and protects those items from sale and reassignment. Return or recall releases the reservation. The simulation layer owns the actual isolated parallel battle states and synchronizes only completed reward deltas into the shared campaign.

## Plague pressure

Strategic time starts after the grand-strategy milestone and advances only when the caller advances the unpaused simulation. Closing the application creates no outbreaks or rewards. A default check occurs every 36,000 simulation ticks (ten minutes). A seeded candidate is chosen from currently liberated countries; the default chance is 35%, reduced by 30 percentage points per assigned guard. Each active alert has its own ID, start, deadline, country, remaining infected groups and authoritative enemy injury state.

The default response window is 25,200 ticks (seven minutes), long enough to complete a fresh defense including its boss. An actual successful country deployment may contain that country's active alert. A garrison fights its alert through the procedure below. An alert that reaches its deadline reopens only the country's final stage. Existing travel unlocks persist. Neither alerts nor containment award free combat coins. The alert outcome is recorded as Active, Contained or Overrun, so resolving a completed alert again cannot pay or reopen it again.

## Guard combat adapter

Guards remain persistent survivors with their real equipment and training. The strategic resolver is an explicit **scenario adapter** for a garrison skirmish, separate from the deployment-frozen encounter renderer. It uses one-second action opportunities, the same 3d6 attack/active-defense/protection/injury module, the guard's Guns skill, rifle damage and ammunition, outfit/accessory protection, HT checks, Dodge, stun, posture, readiness and unconsciousness/lethal outcomes. The current infected group uses an authored close-engagement scenario profile: 9 HP, HT 10, attack 9, Dodge 7, 1d crushing damage and no armor. A seeded alert contains three to seven groups. These values are original scenario content.

Ammunition and multi-turn reloads are real state. A critical result requiring readiness consumes a separate Ready turn and cannot refill the magazine. Shock expires at the next action boundary, even if the survivor spends that turn recovering from stun or changing posture. Defeating a group advances the alert once; defeating the last contains it. Incapacitated guards release their role, the alert remains actionable, and the campaign recovery provision restores the survivor between assignments.

Ordinary post-battle exertion costs one FP after a garrison fight lasting more than ten simulated seconds. It does not charge FP every shot or every ten seconds. Guard FP and injury persist until recalled; recall applies the baseline campaign recovery provision. Multiple guards receive separate actions, while the infected group receives only one action per country each second. Reloading, readiness, stun recovery and posture changes do not prevent that hostile action.

## Verification boundary

Strategy tests exercise independent equipment identities, guard/deployment exclusivity, reservation release, deterministic strategic save/serialization continuation, manual containment idempotency and outbreak expiry without free currency. The larger simulation suite owns parallel reward merging, global pause, save/resume and battle lifecycle evidence. Test helper campaigns construct liberated states directly; they are not evidence that all preceding country stages have been beaten by a human player.


## Readiness, sales and individual advancement

`StrategyState::readiness(campaign, id)` derives an individual survivor's role, prepared weapon/skill/protection profile and current guard HP/FP. It does not invent an expedition success percentage. Parallel battle injuries remain owned by that battle's live attempt. New recruits have equal 76-point creation budgets with different attributes, skills and initial policies; see PROGRESSION.md. Free redistribution restores the selected recruit's own creation baseline.

Single-item sale previews report affected survivor names and presets. Selling optional equipment assigned to an idle recruit restores that survivor's original, genuinely owned protected starter set. It grants no duplicate item. Batch selling excludes every survivor's equipped items, as well as protected, locked, favorite, reserved and preset-referenced equipment. Active deployment or garrison reservations remain unsellable. Game-level sale operations reconcile roster references before a save can commit.

Guard objective progress now claims finite earned character points at the next strategic update, including pending awards when a milestone opens capacity. Deployed survivors keep their battle-owned award ledger until settlement. Paid training never substitutes for an already earned objective. Strategic save validation checks individual creation/advancement, unique guard equipment, actual magazine bounds, valid injury states, active alert identities and countries, deadlines, and independently bounded histories.


Guardian policy now uses its trained melee weapon in the close-engagement garrison scenario; other guards may use melee when a magazine is empty and fatigue permits it. This resolves through the actual melee skill, weapon damage type and ST-adjusted swing profile. It does not consume firearm ammunition and does not count toward the ranged-hit objective. Reloads already in progress still consume their legal action opportunities. Low FP applies the same supported strength reduction to muscle-powered damage, while firearm damage stays unchanged.
