---
type: View
id: player-model-parts
title: "Original semantic part ownership"
parent: player-model-review
status: Implemented
source: "crates/horde_game/src/ui/review/workshop.rs::model_parts, paint_model_parts"
---

# Original semantic part ownership

Inspect each actual source-owned part separately, including faint residue, source masks, draw order and slot ownership. There is no whole-character fallback underneath these parts.

Removing or replacing a slot must remove its actual selected material; newly hidden/revealed surfaces stay explicit.

This is a source-backed developer task. Implemented records source presence, not a final native, interactive or performance verdict.
