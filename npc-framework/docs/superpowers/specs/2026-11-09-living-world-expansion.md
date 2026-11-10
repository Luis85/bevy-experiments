# Living-world expansion

Implement a connected 256×256 metre coastal world with three authored settlements:
Harbour, Alderwood and Stone Ridge. Forty-two residents have individual homes,
stable appearances, professions and real inventories. Coast, woodland, meadow,
wetland and upland terrain supply distinct silhouettes and activities.

The player can explore, fish, gather, build, farm, trade and restore depleted land.
NPCs respond to blocked work and vacancies through qualified, capacity-limited
assignment changes at safe work boundaries. Resource depletion and delayed recovery
must alter available work and survive save/load. The existing colony inspector and
terraforming editor continue to use recorded commands and the same world.

Implementation ownership: regional construction/ecology (world agent), resident
identity/labor/production (resident agent), asset provenance/rendering/animation
(fidelity agent), host lifecycle/player activities/UI/integration (orchestrator).
Research briefs in `docs/research/2026-11-09-*` connect primary sources to concrete
decisions and distinguish implemented scope from future possibilities.

Acceptance evidence: focused headless regressions for assignment, resource recovery,
recorded actions and persistence; autonomous regional economy run with finite stock;
native title/player/inspector/editor captures and readable small-window controls;
the full `cargo xtask verify` verdict. Follow the first integrated implementation
with a subagent review, then a bounded improvement pass and final verification.

Retain 3D and survival rules. Preserve legacy saves by using a separate default
save for the new construction, with no silent conversion. Do not upgrade engine or
dependencies, weaken protected tests, or replace authoritative state with visual
approximations. A bigger island is not evidence of an unbounded streamed world;
the implementation and documentation must describe the actual finite world.
