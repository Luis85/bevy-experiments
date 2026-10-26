# Changelog

## 0.2.0 — Living world and native game pass

- Added an optional, reusable living-world plugin and persistent domain model for owned bodies, possession, RTS orders, needs, shops, money, production, NPC trade, traffic and courier cargo.
- Added first-person, third-person and RTS cameras/controls, crew selection, an owned rover and helicopter, contextual market/industry interfaces and visible interaction feedback.
- Increased the default native neighborhood to 49 chunks with 65×65 heightfields; added smooth indexed terrain, textured materials, richer map semantics, bounded feature detail tiers and render telemetry.
- Added georeferenced 8/16-bit PNG heightmap import and an explicitly synthetic demonstration dataset.
- Integrated new commands with authoritative multiplayer, existing inventories, building/farming/factions, source-world identity and durable saves.
- Hardened source-aware movement/placement, atomic admission, spawn residency, disconnect/resume, mixed-rate replica completeness and bounded payloads.
- Fixed traffic turn deadlocks, parked-vehicle queue stalls, distant source-road selection, road identity and snapshot truncation defects found by independent review and actual-map tests.
- Updated onboarding, integration, data, graphics, gameplay and multiplayer documentation; retained prior validation evidence separately.

The procedural base revision is now `terra-procedural-v2`. Local saves pin their base identity and reject incompatible old geometry. The living state is additive in domain serialization, but source/catalog changes still require deliberate host migration.
