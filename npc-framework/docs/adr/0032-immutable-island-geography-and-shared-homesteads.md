# ADR 0032: Immutable island geography and shared homesteads

- Status: Accepted
- Date: 2026-11-02
- Superseded in part by [ADR 0044](0044-island-crates-split.md): `living_homestead` and compact geography are removed.

The Living Village needs clearer economic routes and room for future growth. The player also needs a small material-backed home/garden loop using the same public actions as other actors.

Add an immutable `island_geography` construction profile to world presets and raw scenarios. Missing values are `compact` and omitted when serialized, preserving older saved preset copies and fixed-world descriptors. New Living presets explicitly select `expanded`. A new additive `living_homestead` preset makes the feature discoverable in existing world folders without overwriting user files. Expanded geography is valid only for the needs-driven island.

One integer geography definition supplies walkable cells, shoreline blocking, roads, static district labels, six clear plots and resource/pasture positions. The expanded ellipse is 30×25 metres, compared with 24×20; both fit the original 64-cell grid. Reuse the original shoreline identities and append additional segments after existing residents/items/animals. Rendering and export consume the same definition. No view writes geography or simulation state.

Expanded worlds register saved/hashed homestead plots, crops, cottages and construction work through the existing facade. Append plots after all incumbent spawning. Record external mutations as `HomesteadAct`, exposed through the existing registered `act` tool. No generic capability names an island role, plot or recipe, and the sixteen MCP tool names do not change.

An actor may claim one housing site and one garden. A living owner's property is protected. A deceased owner's physical estate can be reclaimed. Construction claims exact unreserved material lots and the plot, takes simulation time, rechecks ownership/reach/materials/clearance, and spends inputs only on success. Failure, stop, exhaustion or death releases claims. A real cottage blocks its footprint and changes the owner's home to the reachable door, without moving the actor.

Three housing beds or six garden beds grow hardy grain. Planting spends a real seed; watering spends real clean water and starts a deterministic growth deadline. A successful mature harvest grants physical grain and clears the bed atomically; a full inventory leaves it intact. The existing mill/bakery, consumption and finite-cash market complete the player loop. Claiming land grants no materials, seeds or money. NPCs do not acquire farms automatically.

Owned estate knowledge can persist remotely because the actor knows its own work and fixed layout. Learned foreign plot geometry can remain in memory, but unseen foreign ownership/crop changes are withheld. Observer exports intentionally describe the full operator world; their native/browser projections remain read-only. Closed attributed claim/build/crop effects feed AI continuation without passing private text.

The scope is a small farm/cottage loop. It introduces neither unrestricted town placement nor immigration, romances, livestock or an autonomous farm-management system. Those need separate design and resource/simulation budgets.
