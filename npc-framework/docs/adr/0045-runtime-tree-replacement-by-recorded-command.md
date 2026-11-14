# 0045: Runtime tree replacement by recorded command

Date: 2026-11-14. Status: accepted (island review fix pass, S3.2).

## Context

Editable behaviour trees are an owner requirement: the island's inspector
offers "Reload behaviour from file", and the island's `ReplaceBehavior`
command carries a whole behaviour document. The generic `npc_ai` registry
freezes once (ids sorted by name, references resolved) and had no way to
take new trees afterwards. The `Guard` type, `CondFn` and the way existing
trees are hashed must not change: village, testbed and RTS replays and
snapshots are protected.

## Decision

- `npc_ai::AiRegistry::replace_trees(trees, conds) -> Result<(), String>`
  is the only addition to the generic crate. It works only on a frozen
  registry, at an explicit host call. It adds the condition names the new
  trees need (existing names keep their functions), sorts conditions by
  name again and resolves every new tree; on any error nothing changes.
  Actions and scorers stay frozen: a tree naming an unknown one is refused.
  No existing function changes behaviour or signature; nothing in the
  village, testbed or RTS crates calls it; the crate names no content.
- The island calls it only from the recorded `ReplaceBehavior` command
  (applied in `NpcSet::Inputs`, a tick boundary). It registers the document's
  new conditions with parameters in free slots (`resident::param`), applies
  the kernel's tree transforms, replaces the trees, recomputes the content
  hash as a fresh build of the new document would have it (registered names
  without the conditions with parameters no tree uses any more, plus every
  tree's content hash), writes the document into the world's behaviour data
  and saved rules, and resets every resident's brain in `SimId` order
  (stopping the running leaf through the one teardown path).

## Why it is safe

- Replays: the document travels in the command, so a replay repeats the
  same replacement at the same tick, and every id afterwards is derived from
  names in a fixed order.
- Saves: the saved rules hold the new document, Continue builds the island
  from them (ADR 0042), and the recomputed content hash equals that build's.
- Existing worlds that never send the command are unaffected: their trees,
  ids and hashes are exactly as before.

## What stays frozen

Actions, scorers, needs, components, resources and commands; the kernel's
`ContentHash` algorithm; every tree's own content hash (names only).

## Replays

The kernel's replay recorder stamps the content hash the world had at its
first tick (`npc_kernel::registry::InitialContentHash`, set when the
registries freeze), not the hash when the recording ends. A replay that
contains a tree replacement therefore carries the hash its fresh build
starts with, and `replay verify` accepts it. For every replay without a
replacement the two hashes are equal, so existing replay files are
unchanged byte for byte (checked: the 8 protected replays verify, and a
re-recorded `village_seed1_3days` equals the committed file).

Proof: `a_recorded_replay_with_a_tree_replacement_verifies_and_rerecords`
(`content/island_content/tests/island_behaviour_data.rs`) records an island
replay with a `ReplaceBehavior` carrying an edited tree, verifies it, and
records it again byte for byte (written under `target/island-split/`).
