# 0037: Regional living world

Status: accepted for the 2026-11-09 iteration.

The larger coastal world is an opt-in content construction profile, `living_world`,
whose complete validated definition is frozen into a save. The original island
profile and its registered state remain compatible. The normal entry uses a new
`living-world.world.json` save; an explicit legacy save still loads its original
construction. This is not a migration or an in-place registry replacement.

`village::frontier` composes bounded content contexts:

- `world_spec` owns immutable dimensions, settlements, homes, sites and arrival.
- `settlements` owns membership and stable site/home identities.
- `terrain` owns deterministic construction and terrain queries; colony terrain
  remains the authoritative editable grid. Rendering projects it into chunks.
- `ecology` owns renewable biomass, depletion, recovery and habitat history.
- `labor` owns assignment transitions and their reasons/cooldowns. `Villager`
  remains the sole current role/workplace; identity, home and skills survive changes.
- `routes` supplies deterministic reachability/cost queries to physical sourcing.
- `activities` adapts recorded actions to shared approach, reservation, work and
  material-consumption semantics, calling ecology only on valid completion.

Regional production allocates work among qualified producers. Trade remains
physical; prices and the existing public fund remain shared world institutions,
not fictional independent village treasuries. No controller silently creates goods,
skills, cash or equipment when an occupation changes.

Presentation never writes simulation state. Character appearance is stable identity
data, independent of occupation. Terrain chunks, decorative assets, fog and lighting
are view concerns. Known-place navigation respects actor knowledge; inspector tools
remain explicit omniscient modes. No generic framework crate names this content.

Survival and permanent death remain in force. Renewables recover over simulated
time; player restoration consumes actual inputs. Legacy daily refill behavior stays
confined to legacy profiles. Authoring and regression tests must cover the new
profile independently without replacing replay or snapshot baselines.
