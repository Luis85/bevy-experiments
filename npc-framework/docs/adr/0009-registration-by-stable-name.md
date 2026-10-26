# ADR 0009: Content registers by stable name; ids are frozen sorted by name

- Status: Accepted (implemented in baseline hardening P3)
- Date: 2026-10-23
- Source: baseline-hardening plan Task P3, `docs/research/framework-best-practices.md` §0.2 and §1, ADR 0007

## Context

Before P3, `AiRegistry::add_action/add_cond` returned the push index as the id, trees stored those ids,
`hash.rs` listed every hashed component by hand (content included), `SimCommand` had villager variants,
and needs were a closed `NeedKind` enum. Registering one more action earlier, or adding plugins in another
order, silently changed every id, the state hash and every recorded replay; a second game could not add
its own components, commands or needs without editing core.

## Decision

- **Everything is registered by a stable name** (`NpcAppExt`): actions, conditions, scorers and trees
  (`npc_ai::AiRegistry`), needs (`NeedDef`, `npc_needs`), hashed sim components (`SimHash::NAME`) and
  resources (`SimHashResource::NAME`), content commands (`ContentCommand::NAME`), kind namers and
  `SimId` leave hooks. Registration may happen in any order, before or after `NpcPlugins`.
- **Ids are frozen once**, in the kernel's `Plugin::finish` (`npc_kernel::ensure_frozen`, idempotent; also
  called lazily by `run_ticks`, the driver and content spawners): each kind is sorted by name, ids are the
  index (`u16`, checked conversion with a clear panic beyond `u16::MAX`), duplicates panic. Registering
  after the freeze panics.
- **Trees reference names.** `TreeBuilder::leaf(node, "ActionName", param)` / `guard(node, "cond", ..)`;
  `NameRef<Id>` keeps the name (serialized) and the resolved id (not serialized). An unknown name panics at
  freeze time listing the known names of that kind. Leaf params are typed (`ParamValue { None, Int,
  Pos, Name }`). `TreeDef::content_hash` hashes names, never ids.
- **State hash = registry.** `state_hash` writes `SimClock.tick`, the entity count, then per entity (ascending
  `SimId`) the id and, for every registered component in name order, a presence tag plus its `SimHash`
  encoding; then every registered resource in name order. Diffs (`components_json`) and the generic
  inspector use the same registry, keyed by the same names.
- **Content hash.** At freeze the kernel hashes the sorted `(category, name)` manifest of everything
  registered plus each tree's content hash (`ContentHash`). Replays (format version 2) carry it;
  `replay::verify` refuses a replay whose content hash differs from the running build. Version 1 replays
  still load (unchanged header and commands) but verify reports them as needing a re-record, because their
  checkpoints use the pre-registry hash encoding.
- **Content commands.** `SimCommand` core variants are `Despawn`, `SetNeed { need: String }` (handled by
  `npc_needs` through `SetNeedHandler`) and `Order`; any other `"type"` is `Content { kind, payload }`,
  dispatched by name to a registered `ContentCommand`. The payload keeps the typed struct's field order
  (`JsonObject`) and is canonicalized through the typed command on submit/apply, so the wire format,
  journals and replays are byte-identical to the pre-split ones (`{"type":"SetFood","stockpile":3,"food":0}`).
- **Presentation order is data, not registration order.** Where JSON field order or journal order of
  simultaneous events matters (needs), the def carries an explicit `order`; ids stay name-sorted.
- **BRP compatibility.** The reflected sim types keep their pre-split type paths
  (`#[type_path = "npc_sim::ids"]` etc.), so `world.*` queries such as `npc_sim::ids::SimId` keep working.
  `Needs` changed shape (one `Need` per registered def) and is reflected under its new path.

## Consequences

- Plugin order and registration order never change ids, hashes or replays (pinned by
  `registration_order_independent`; `ids_are_sorted_by_name_whatever_the_registration_order` in `npc_ai`).
- The hash encoding changed once (P3): the replay corpus and the run golden's `final_hash` were
  regenerated after proving that journals, inspect snapshots and run summaries are byte-identical before
  and after (evidence: `docs/_notes/p3-rebaseline-evidence.md`).
- Adding or renaming any registered name changes the content hash, and so invalidates recorded replays;
  that is deliberate (a replay of different content is not the same run). Say so in the change.
- Behaviour content can be moved to data files later (RON defs) without breaking saves or replays,
  because trees, journals and replays already speak names.
