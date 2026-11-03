---
type: View
id: battle-weapon-selector
title: "Battle attack selector"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle.rs::attack_selector"
---

# Battle attack selector

Choose the equipped firearm or an unlocked grenade/Molotov action during personal combat.

- **Entry, exit and actions:** A bounded three-button row above the arena shows Gun, Grenade and Molotov. The gun hover names its equipped class; throwable hints expose charges or their Ordnance-branch unlock requirement. Select a throwable to enter ground targeting; select the gun to return to ordinary aiming. Selection clears pending fire and the firing latch.
- **State boundary:** AttackSelection is scene state; the attempt profile and ordnance state supply class, unlocks, charges and cooldowns. Selecting a mode does not equip a different item or issue a throw.
- **Responsiveness and accessibility:** Short button labels and a single truncated readiness/cooldown helper stay above the combat lanes. Full hover text and disabled reasons preserve the explanation.

**Current increment contract:** This is an attack selector, not an inventory hot-swap. It selects the equipped gun or unlocked Grenade/Molotov actions; firearm ownership, training and reservations are validated through equipment/deployment.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

