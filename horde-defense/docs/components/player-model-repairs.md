---
type: View
id: player-model-repairs
title: "Hidden surface repair inspection"
parent: player-model-review
status: Implemented
source: "crates/horde_game/src/ui/review/workshop.rs::model_repairs, paint_model_repairs"
---

# Hidden surface repair inspection

Disclose authored hidden fills and replacement material separately from the original still, including their activation and selected pose. Inspect source provenance, bounds and seams rather than claiming unknown surfaces were recovered.

Repairs must not contribute extra reference-bind pixels. New garments/actions have independent visual acceptance; original RGBA remains unchanged.

This is a source-backed developer task. Implemented records source presence, not a final native, interactive or performance verdict.
