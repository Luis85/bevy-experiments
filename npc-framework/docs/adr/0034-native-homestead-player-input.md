# 0034 — Native player input for shared homestead actions

Date: 2026-11-04. Status: accepted.

Superseded in part by [ADR 0044](0044-island-crates-split.md): the Home & farm panel is removed.

## Context

The larger Living island already has recorded, needs-driven homestead and production actions, but a human needs CLI/MCP calls to use them. Native controls must use the same ownership, reach, life, materials, reservation and economic rules. The existing RTS UI alone does not execute a directly controlled visitor's queued movement or offer homestead/gathering controls.

## Decision

The content view provides a collapsible Home & farm panel, with an in-process native input bridge anchored to `VillageState.played`. Human input explicitly records the existing non-forcing `AttachController`/`DetachController` commands. A foreign controller is refused. Native control pauses host scheduler nudges and restores its own prior policy only if a later writer has not changed it.

Native input is trusted application input, as are existing RTS orders. It calls the public in-process `api::act` resolver with its actual native controller; it does not forge a registration session, credential or remote authorization. Remote transports keep their existing registration checks. Observations and target positions come from actor-scoped registered providers, including remembered/owned land and protected unknown foreign state.

Walk-before-work records a normal Move/execute-orders action, keeps a view-only continuation, and resolves the requested shared action anew after successful arrival. Replacement orders, stop, blocked movement, control loss, life/actor replacement or world epoch changes invalidate that continuation. Existing right-click orders for the native-controlled hero are executed through the same recorded resolver. Presentation and navigation bookkeeping never enter simulation hashes or saved worlds.

The UI reads declared recipes, actual pack contents and public market opportunities. It creates no materials, seeds, money or private settlement. Explicit host resume controls change `SimControl` only; the view never steps the simulation itself. Pending `HomesteadAct` inputs join the existing owned-actor activation cleanup, while already-growing saved crops remain intact.

## Consequences

The human can build and tend a small homestead through native widgets while sharing the existing AI action handlers. Native ownership is reselected after host resume. The initial crop remains hardy grain; browser output stays read-only. Widget/bridge regressions, registered-action checks, native screenshots and the final workspace gate verify the increment without provider usage or protected-oracle changes.
