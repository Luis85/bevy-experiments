---
type: View
id: battle-ammo-reticle
title: "Battlefield ammunition and reload reticle"
parent: battle
status: Implemented
source: "crates/horde_game/src/ui/reticle.rs::paint, resources; crates/horde_game/src/cursor.rs::sync_native_cursor"
---

# Battlefield ammunition and reload reticle

Aim while reading remaining ammunition/charges and actual reload or cooldown progress.

- **Entry, exit and actions:** Hover an unobscured focused personal or observed arena. The reticle follows aim; leaving the field, entering UI/popups/details or losing focus restores the native pointer. The readout itself submits no shot, reload or throw.
- **State boundary:** Resources read Attempt ammo/magazine, frozen reload total/remaining ticks and selected ordnance charges/cooldowns. CursorOptions visibility synchronizes after egui output; the egui None hint alone is not the native visibility mechanism.
- **Responsiveness and accessibility:** Written counts supplement the proportional ring, which has at most 24 pips (12 reduced). Reload/cooldown has a green progress arc. Empty state is explicit; NPC observation reads the actual survivor attempt. Construction retains the crosshair without the resource ring.

The frozen arena projection owns cursor/world mapping. This source-backed View does not claim an OS-window interaction check.

The crosshair stays at true world aim. A separately clamped resource badge/text uses the bounded effects region so edge counts remain readable, including the small three-row clearing; its position never changes aiming. Diagnostic preview can draw an in-bounds reticle without OS hover/focus and is not production pointer evidence.
