# ADR 0028: Shared island actions and NPC life trees

- Status: Accepted
- Date: 2026-11-01
- Superseded in part by [ADR 0044](0044-island-crates-split.md): the Rust `island_life` tree and the older tiers are removed.

Island residents previously had no `Brain`: a sorted Effects loop chose survival and profession actions directly. That duplicated policy and made NPC-only free home recovery unavailable to player clients.

Register an island-only `island_life` behavior tree using the existing generic runtime. Leaf starts stage a small registered `LifeDecision`; sorted Effects applies its physical action after vitality, resource renewal and work completion. Its pending/cooldown data is hashed and saved so snapshot resume cannot repeat an action. The generic external-control branch owns takeover and release; no second NPC dispatcher runs beside it. Critical actionable food/water needs may abort owned work, while normal needs allow a bounded task to finish. Empty shops do not block their own producers from restarting supply.

NPC and client consumption, purchases, gathering, voluntary rest and recovery use the same `VillageAct::check` and `perform` paths. Movement and crafting retain their existing shared framework implementations, navigation and station reservations. Add `rest_home` (within 800 mm of one's own home, 300 ticks at 400 energy/tick), `rest_ground` (300 ticks at 40 energy/tick), and `recover_diary`. Automatic involuntary exhaustion, mortality, resource renewal and community redistribution remain environment rules, not privileged input actions. Home denotes an actor's established resting position; this increment adds no property or housing market.

Explicitly register the six stable island `craft:<recipe>` aliases against the facade's public crafting handler during island construction. Headless hosts freeze content without running Bevy's later plugin-finish phase; advertising aliases alone would otherwise leave clients with unknown actions. Named aliases carry the canonical `craft` capability in both world and actor policy checks. Autonomous manufacturing consults the same registered policy gates.

Manufacturers procure a finite buffer of up to eight ingredient units per supplier visit, falling back to a smaller valid quantity when capacity or funds require it. The original accelerated-year survival test exposed bakery travel overhead with four-unit buffers: food ran out despite solvent buyers. Procurement remains an ordinary paid, capacity-checked trade; no resource or currency is injected to satisfy survival checks.

Action parity does not equal identical starting assets or economic roles. Visitors start without coins or a resident profession and receive no community-fund stipend. Residents retain their finite shops and initial stock. A visitor can gather, manufacture and actively sell the same goods; passive visitor shopkeeping is outside this increment. Private host notebooks remain authored only by agents/players, distinct from physical diaries.

Observation adds known resource duration and currently sensed/reached stock/renewal time plus known workstation recipe inputs and outputs. Stale memory never exposes live remote stock. The observer projection may publish public current decision labels, production recipes and resource timings; these are read-only derived facts, not generated inner thoughts or a simulation input.

Known public workplaces additionally describe their static role, typical products and accepted goods, including non-manufacturing well/inn sites. This supports searching learned locations when a trader has moved out of perception. It never reveals the resident's live location, stock, money or availability. A reference client's current affordance set replaces its previous set; personal remembered places are search hints, not live actor tracking.

The new names intentionally change island content hashes. Existing strict island saves from earlier builds are not silently migrated; use a new island for this increment. Saves made by this build must resume equivalently. Other scenarios register none of these additions, and their protected replay/golden corpus remains unchanged.
