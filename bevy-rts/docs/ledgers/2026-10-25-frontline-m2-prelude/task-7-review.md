# Task 7 review: content tree pipeline and per-path origins (C6 part 1)

Commit a531307 on `m2p/t07-tree` (C:\fl\t07, base 1274ec3). I read `review-t7.diff` in four passes (loader files, object/rules, tree.rs, tests and API). Read-only: nothing was built or run.

### Spec Compliance

- ✅ Spec compliant, with deviations the implementer declared:
  - Every brief interface exists as specified: `Span`, `ContentTree`, `Node`, `parse_ron`, `to_ron`, `TreeDeserializer`, `from_tree`, `path_key`, `PathSeg`, `Origin` and `ContentDb::origins`.
  - The extras are additive: `TreeError`, `Span: Copy + Eq`, `PathSeg: Ord + Display`.
  - `load_dirs` keeps its signature. Its phases are discover → `parse_ron` → `object_doc` → overrides → `resolve_partial` on `Head<docid>` → typed deserialize once per writing document → link → hash (mods.rs:97-98, 535).
  - All 8 brief tests are present, plus `ambiguous_f32_float_is_rejected`.
  - Every Files-list file has a hunk.
- **Extra (declared):**
  - `rules.rs`, `kinds.rs` and `registry.rs` were edited outside the list. `KindParseFn` now takes `(&Node, &Path)`, and `parse_rules` reads a tree.
  - `fl_content/Cargo.toml` gained the dev-dependencies `fl_catalog` and `fl_sim` (Cargo.toml:26-27), which also changes `Cargo.lock`.
  - Both are needed to meet the brief: its test list requires the game registry inside `fl_content/tests/tree.rs`. See rulings (1) and (2).
- **R148 proof (named risk: does it cover every mod?):**
  - Check: I listed every `mod.ron` in the worktree.
  - The 4 valid game mods are hashed: base and fl_sim's `core_mod`, `core_mod_extra` and `core_mod_factions`. So are the 8 fl_content fixture loads: `mod_a`, `mod_a+mod_b` and the six `hash_*` mods.
  - The 2 game mods that are meant to fail are checked by first failing file: fl_sim's `absurd_speed` and fl_cli's `bad_mod`.
  - The 13 `bad_*` fixtures in fl_content are covered by the unchanged `load.rs` tests.
  - Coverage is complete (tests/tree.rs:318).
  - The base value `0x0f29a5a56c793aa7` matches the value committed before this task in docs/debugging.md:157. `core_mod` is cross-checked by the golden replays, which pass unchanged.
- ⚠️ **Cannot verify from the diff:**
  - The fixture hashes other than base (`mod_a`, `hash_*`, `core_mod_extra`, `core_mod_factions`) were recorded with a scratch test that was then deleted. Only the implementer's report attests to them. To confirm, the controller can run `cargo nextest run -p fl_content --test tree -E 'test(tree_load_matches)'` on 1274ec3 with only `tests/tree.rs` patched to the old API. That is optional: base and the goldens already agree.
  - `VisualDef` is the one kind outside the rules hash (builtin.rs:310), so the hash proof does not cover its typed equality. It rests on test-client passing and on the `visual:` origin keys in `origins_cover_every_leaf`.

### Strengths

- **Float handling is careful and fully specified.**
  - ron reads `f32` directly. A literal whose direct f32 parse disagrees with its f64 parse narrowed to f32 is rejected (tree.rs:665-686), and so is the integer `-0`. That makes `Float(f64)` exact for both widths.
  - When the shortest form is ambiguous, `float_text` falls back to the exact decimal expansion. That expansion always passes the f32 check, so the printer always round-trips.
  - `floats_parse_like_ron` compares Fx, f32 and f64 bit-for-bit against `ron::from_str`, including `-0.0`.
- **The inheritance refactor is minimal and does not duplicate logic.**
  - `resolve_partial` became generic over `Head<C>` (inherit.rs). The loader passes the id of the document that wrote each component.
  - Each component is deserialized once, from its writing document, and descendants share the `Arc`.
  - A failed component makes the object unresolved, so its children and referrers do not cascade errors.
- **Error locations survive erased-serde.** A shared `ErrorSite` records the innermost failing path and span (tree.rs:908-922, 1516). As a result, `t.x/components.Hull.hp` is reported at 4:22 and `Test.gravity` with its line.
- **Origins are deterministic.**
  - They are a `BTreeMap` with written-order traversal, and override documents replace whole before recording.
  - Rules origins come from the winning section only, and inherited leaves stay under the template's key, per ADR-0020.
  - `origins_cover_every_leaf` independently recomputes the key set for every base file and asserts it equals `origins()` exactly, with one owner per key.
- **No float reaches a hash.** Origins never enter `rules_hash`, and the typed values (Fx) are produced as before.
- **The API change is purely additive:** 129 lines added, 0 removed, against the blessed `fl_content.txt`.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
None.

#### Minor (Nice to Have)

1. **tree.rs:1-1571 is a new 1571-line file with three jobs:** parser (~180-690), printer (~690-810) and serde deserializer (~810-1571). Task 10 modifies tree.rs again, for migrations and patches. A pure-move split into `tree/{mod,parse,print,de}.rs` keeps the API and the snapshot unchanged. See ruling (4).
2. **tests/tree.rs:189-205: the inline proptest policy is not an exact copy of Task 6's `proptest_policy::config`.** It has no `FL_PROPTEST_RANDOM=1` → `RngSeed::Random` branch, so nightly would keep the fixed seed. It is also the source of the 2 test-guard `NewSeedOverride` findings (tests/tree.rs:15, :201). See ruling (3).
3. **Undocumented behaviour change: mods.rs:98 → 535.** `check_object_ids` drops documents that an `override` supersedes before `resolve_docs`, so their components are never type-checked. Their structural `object_doc` errors are still reported (mods.rs:97).
   - Before this task, `parse_object` type-checked every file.
   - This is consistent with ADR-0020's override-at-patch-phase. A mod's own `data-check` still catches the error.
   - Fix: add it to the report's behaviour-change list.
4. **tree.rs:268 vs error.rs:22: a parse error prints its position twice.** It shows as `x.ron:1:5: char literals are not supported at line 1, column 5`. The brief's test text asks for "line 1, column 5", so this is cosmetic. Typed errors do not repeat the position, which is inconsistent.
5. **kinds.rs:130: a content kind's typed error path has no `<id>/` prefix.** Object errors read `<id>/components.…` and rules errors read `<Section>.…`, so kind errors are less specific. The id can be read from the tree's `id` field before deserializing. This is still better than before, when the path was empty.
6. **tree.rs:113/123: `path_key` writes map keys as written, without escaping `.`.** Keys are therefore not injective in principle. No base map key contains a `.` (checked by grep), so nothing breaks today. Task 10's patch-path parser must define how a dotted key is addressed. Note it for Task 10's dispatch.
7. **tree.rs:908-922: `ErrorSite` keeps the first error recorded.** If a deserializer error were swallowed and retried (`untagged`/`flatten`-style), a later real error would report a stale site.
   - No content type uses those: the only grep hit is hash.rs:7, which already forbids `flatten`.
   - So this is latent. A short doc line on `TreeDeserializer` would cover it.
8. **Process items for the controller:**
   - The RED evidence was reconstructed after GREEN (by temporarily restoring the base sources). This is acceptable but not true TDD order.
   - The shared-scratchpad `gate.log` clash (report concern 7) may have overwritten Task 5's gate log. Check that Task 5's evidence is intact.
   - `milestone_m1_random_orders_branch_long` timed out once. It is outside the gate and the report calls it unrelated, but it was not rerun.
   - Before merging, rebase onto 1eac3cc as the ledger requires.

### Recommended rulings

1. **Keep the `fl_content` dev-dependencies on `fl_catalog` and `fl_sim`.**
   - ADR-0020's Confirmation places `tree_load_matches_typed_load` and `origins_cover_every_leaf` in `fl_content/tests/tree.rs`.
   - Task 8's `base_ledger_is_complete`, in `fl_content/tests/provenance.rs`, needs the same game registry, so moving the tests now would only move the problem.
   - arch-check allows dev edges between non-client crates (global-constraints:132, the `fl_sim`→`fl_test_utils` precedent), and the gate's arch-check passed.
   - The cost was one workspace-wide gate run for the `Cargo.lock` change.
   - Condition: only integration tests (`tests/`) may use them, never `#[cfg(test)]` in `src`, because `src` would see a second copy of `fl_content`. Add that to the Cargo.toml comment, and record the ruling so it also covers Task 8.
2. **Accept the edits to `rules.rs`, `kinds.rs` and `registry.rs`.** They are the minimum needed to deserialize kinds and rules from trees, they stay inside the contract track, and Task 10 lists `registry.rs` anyway. Amend the Task 7 Files list after the fact.
3. **Accept the inline proptest policy and both `NewSeedOverride` findings until Task 6 lands.**
   - Task 6's sweep ("every `crates/*/tests/*.rs` that runs a proptest") must replace it with `#[path = "../../fl_test_utils/src/proptest_policy.rs"] mod proptest_policy;` and `config(256, 1024)`. Name `fl_content/tests/tree.rs` explicitly in Task 6's dispatch.
   - Do not allowlist these findings: the code is temporary.
4. **Split `tree.rs` now, in a follow-up commit on t07 before merge.**
   - Split it into `tree/{mod.rs (types, path_key, for_each_leaf), parse.rs, print.rs, de.rs}`.
   - It is a pure move: the API snapshot stays unchanged, and checking the diff-stat is enough review.
   - This avoids Task 10 growing a 1.5k-line file. If the controller wants no extra round, make the split Task 10's first step instead.
5. **Accept the stricter RON subset.** It rejects f32/f64-ambiguous floats, the integer `-0`, overflowing floats, `0x…`/`_`/suffixes, char and raw strings, extra escapes and attributes.
   - No existing content is affected: every mod hashes identically.
   - It is documented in the tree.rs module docs and the `parse_ron` doc.
   - Each rejection names the file, line and column and gives a remedy ("write it with fewer digits", "write 0, or -0.0 for a float"), so the data-check message is adequate.
   - Add one line to the modding docs when Task 21 writes the content-format notes. Optionally drop the duplicated " at line L, column C" suffix (Minor 4) if the brief's test is reworded to check `line_col`.

### Bless recommendation

**Yes: bless `xtask/snapshots/api/fl_content.txt.new`** as `M2P Task 7: content tree and origins`.
- It is purely additive (129 lines added, 0 removed).
- It matches the brief's interface plus `TreeError`.
- No private helper leaks: `ErrorSite`, `from_tree_at`, `erased_from_tree` and `for_each_leaf` are all `pub(crate)`.

### Assessment

**Task quality:** Approved

**Reasoning:** The tree pipeline gives byte-identical rules hashes for every valid mod. Base is confirmed against a hash committed earlier, and the golden replays are unchanged. Its parser, printer and deserializer are carefully matched to ron's number semantics, and the origins are deterministic and tested against an independent key computation. The remaining items are structure (file size), a temporary policy copy, and small error-message polish.
