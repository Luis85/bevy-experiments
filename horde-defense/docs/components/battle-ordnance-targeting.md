---
type: View
id: battle-ordnance-targeting
title: "Ordnance area targeting"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/battle.rs::ordnance_cursor, gather_combat_input"
---

# Ordnance area targeting

Preview a ground cell and authored area radius for grenade and Molotov throws.

- **Entry, exit and actions:** Select an unlocked throwable in the action strip, aim at a ground cell and press Fire once; select the gun to leave targeting. Input while a popup or construction owns the pointer cannot issue an accidental throw.
- **State boundary:** The attempt owns issued/spent/remaining charges, cooldowns, snapshotted projectile parameters and fire zones. Core content and fixed ticks resolve enemy-only damage and burning; cursor rings and flight arcs are presentation.
- **Responsiveness and accessibility:** A cell label and radius ring span the actual world geometry. Charges, lock reasons and cooldown text supplement ready/unavailable color; reduced-effects rendering preserves tactical cues.

**Current increment contract:** Implemented targeting uses a Fire press edge at a one-yard ground cell inside the frozen width and lane count. Holding fire and the firing latch cannot repeat throws. Charges, cooldowns, projectile flight and Molotov fire zones are saved authoritative state; rendered arcs/radius/fire are presentation.

Parent IDs express view containment, not an exclusive navigation route. Implementation status records the existing surface; it does not claim native GUI validation.

