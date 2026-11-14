---
type: View
id: equipment-auto-sell
title: "Auto-sell rule"
parent: equipment
status: Implemented
source: "crates/horde_game/src/ui/armory/auto_sell.rs::panel, body, panel_model, threshold_chip"
---

# Auto-sell rule

Set the rule settlement uses to sell new drops, and preview it on the stash.

- **Entry, exit and actions:** A collapsed "Auto-sell" section under "Manage equipment" on Equipment; the Results loot block's off line and the auto-sell callout point here. It holds "Auto-sell new drops", five threshold chips (Poor only, Common and below (default), Uncommon and below, Rare and below, Epic and below) with quality markers, one checkbox per slot, "Keep upgrades for the defender who ran it" and "Also on expeditions and operations". Every change dispatches `Command::SetAutoSell` at once. "Would sell N now · +C" previews the stash (what is kept and why on hover), "Last run would have sold N · +C" shows while that run's drops are still owned, and "Apply rule to stash now…" opens the stash-apply confirmation.
- **State boundary:** The rule is `Campaign.auto_sell` and the previews are core's `auto_sell_preview`; the panel holds no copy. A threshold that sells Rare or Epic finds says so in amber, and the raise-the-threshold hint appears when recent drops all sit above the threshold. The apply button is disabled, with its reason on hover, while a defender is deployed or nothing matches.
- **Responsiveness and accessibility:** Chips and checkboxes wrap; quality has a name and shape as well as colour. No toast and no battle text ever mention a sale.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
