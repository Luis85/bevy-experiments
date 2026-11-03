---
type: View
id: skill-details
title: "Selected upgrade requirements and payment"
parent: skills
status: Implemented
source: "crates/horde_game/src/ui/progression.rs::skill_detail"
---

# Selected upgrade requirements and payment

Read the selected node's actual effect, ownership, prerequisites and payment.

The action uses can_purchase_node/node_payment and remains disabled with the core reason when unavailable. Requirements include named owned parents and victory/liberation counts. Coins and eligible legacy CP are explicit distinct payments. The optional old-allocation refund remains subject to core preflight. Weapon training never grants/equips an item. The explanation scrolls while the purchase action stays at its head.

Parent IDs express containment, not exclusive entry routes. Implemented records source presence, not native interaction or performance validation.
