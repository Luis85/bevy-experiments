---
status: "accepted"
date: 2026-10-25
decision-makers: "controller (accepts under the user's delegation, R85), lead"
consulted: "M2 prelude planner (planner-m2p-report.md; spec-gap resolutions accepted as R148); research: architecture-best-practices.md N8, N9, N11, N12, C10, C22, C23; baseline-recommendations.md C5"
informed: "M2 prelude implementers (Tasks 12, 17, 20)"
---

# Core vocabulary: conditions, stats and modifiers, tags, verbs and command buttons

## Context and Problem Statement

Spec §9.2 C5 asks for a shared vocabulary before any M2 mechanic: OpenRA-style conditions with `requires`
expressions, a modifier and stat spec with a deterministic stacking order and a cached effective stat, tag bitsets with
class expressions, effect and trigger verb registries, and command-panel buttons that carry `requires`. Without it,
every combat, morale, experience or formation feature would read its own fields and multiply its own factors, and two
features changing one value would compose in whichever order their systems happened to run. Stacking order and
fixed-point rounding are part of the hash stream, so they must be fixed once, for every later layer, before the first
feature depends on them. The registries follow ADR-0004, the classes ADR-0006 and the schedule ADR-0005.

## Decision Drivers

* Features compose without depending on each other's code (two-crate rule, ADR-0001; OpenRA's traits).
* One deterministic stacking order in `Fx`, with bounds that rule out overflow in every intermediate value (spec
  §5.4, R104).
* Content names conditions, tags and verbs; an unknown name is a content error with file and path, never a runtime
  surprise (spec §6.1, strict parsing).
* Hashed and saved state carries names, never dense ids (ADR-0004), and cached values never change outcomes
  (ADR-0015, R84).
* Cheap per-unit checks at scale: class expressions and `requires` are evaluated by thousands of units per tick.

## Considered Options

* A registered vocabulary
* Per-feature fields and multipliers
* A general expression or scripting layer

## Decision Outcome

Chosen option: "A registered vocabulary", because it is the only option that fixes one stacking order for every
feature, keeps every name checkable at content link time and keeps evaluation in `Fx` without an interpreter.

- **Conditions.**
  - Features register condition names with `SimBuilder::register_condition(name)` (and, for content,
    `DefRegistry::register_condition`), a stable-string table `"conditions"` (ADR-0004); `condition_id(world, name)`
    gives the dense `ConditionId` once the registry is frozen.
  - A unit's `Conditions` component (Hashed `"core.conditions"` v1) holds sorted `(ConditionId, count)` pairs, hashed
    and saved by name. `grant_condition` inserts the component on the first grant and returns a `ConditionToken`;
    `revoke_condition(token)` decrements the count, removes the pair at zero and the component when it is empty. A
    token names its unit (`SimId`) and its condition; it is neither `Copy` nor `Clone`, and `revoke_condition`
    consumes it, so one grant is revoked at most once. A grant that outlives the tick keeps its token in the granting
    feature's Hashed state, whose codec saves the condition by name (remapped through `"conditions"` on load). A
    revoke whose pair is already at zero is a feature bug: a debug assertion, and a no-op in release builds. Journal
    `core.condition_changed`.
  - A `requires` expression (`&&`, `||`, `!`, parentheses, condition names; precedence `!` over `&&` over `||`) is a
    def field `Requires(String)`, compiled at content link time (`LinkCtx::requires`) into a `ConditionExpr` (postfix
    bytecode over condition ids). An unknown name is a content error at the field path ("unknown condition 'x'"), and
    a malformed expression names its column. `requires_met(world, entity, expr)` evaluates it.
- **Stats and modifiers.**
  - Stats are registered with `SimBuilder::register_stat(name, StatDef { base, min, max })`, table `"stats"`;
    `base` reads the unit's base value (`None`: the unit has no such stat). Bounds, asserted at registration:
    `-1,000,000 ≤ min ≤ max ≤ 1,000,000`.
  - A `Modifier` is `(stat, op ∈ {Add, AddPct, Mul, Override}, value: Fx, source: ModSource { key, unit })`, where
    `key` is a stable name and `unit` an optional `SimId`. A unit's modifiers live in its Hashed `"core.modifiers"`
    v1 (`Modifiers`), kept sorted in the total order below. `add_modifier` checks the bounds below and returns `Err`
    otherwise, adding nothing; a caller that cannot return the error (an `EffectVerb::apply`) journals it
    (`core.modifier_changed` with `"rejected"` and the reason) and never panics. `remove_modifiers_from(key, unit)`
    removes a source's modifiers, and the component is removed when its list empties, as `Conditions` is, so a
    once-modified unit hashes like its unmodified twin. Journal `core.modifier_changed`.
  - **Evaluation** (`evaluate(base, def, mods)`, pure), where "clamp" is to the stat's `[min, max]`:
    1. `v = override.unwrap_or(base)`, clamp. Overrides apply in bucket order, so the last one in the total order
       wins.
    2. `v += Σ add`, clamp.
    3. `v *= max(0, 1 + Σ add_pct)`, clamp. The factor is clamped at 0 before multiplying, so stacked slows (two
       −0.6) stop at 0 and never flip the sign.
    4. `v *= mul` for each `Mul`, one factor at a time, clamping after each.

    **A total order within each bucket**, independent of insertion history: `(source key by its stable-name bytes,
    source SimId with None first, op kind, value raw)`. `evaluate` sorts its input by it, so neither the order of
    `add_modifier` calls nor how `remove_modifiers_from` compacts the list can change a result; two entries equal
    in all four fields are identical in effect. Sums are exact in `Fx`; the order matters for the rounding of each
    `Mul` product, which rounds as `Fx`'s own multiplication does.
  - **`Override` replaces the base, it does not force the final value**: `Override 0` plus any `Add` still moves the
    stat. A rule that must pin a value to zero (a track break stopping a tank) uses `Mul 0`, which wins over every
    earlier bucket.
  - **Value bounds** (R104, R149): `Mul` in `[0, 100]`, `AddPct` in `[-1, 100]`, `Add` in `[-max, max]`, `Override`
    in `[min, max]`, and at most `MAX_MODIFIERS_PER_STAT` (16) modifiers on one stat of one unit. So no intermediate
    overflows `Fx` (I32F32, about ±2.1 × 10⁹): after step 1 `|v| ≤ 10⁶`; step 2 adds at most 16 × 10⁶; step 3
    multiplies a clamped `|v| ≤ 10⁶` by a factor in `[0, 1 + 16 × 100 = 1,601]` (at most 1.6 × 10⁹); each step-4
    product is at most 10⁶ × 100.
  - **Cache** (R149, R151). Effective values are cached per unit in the Derived `EffectiveStats`
    (`"core.effective_stats"`, rebuilt by core's one rebuild, `rebuild_core`, R74). Each cached entry stores the base
    value it was computed from next to the effective value.
    - Every modifier change marks the unit in the `"core.stats_dirty"` set. `recompute_stats` refills the cache for
      the marked units in `SimId` order at the start of `ThinkSet::Orders` and in `ResolveSet::Stats` (span
      `recompute_stats`, counter `stats.recomputed`). The set is emptied only by a recompute, never by
      `Input::Housekeeping` (`Class::TransientKept`), so a change made after `ResolveSet::Stats` waits for the next
      tick's recompute. The set is an optimisation only.
    - A read never observes a stale value (the R84 rule). `effective_stat` evaluates on the spot when the unit is
      marked, and otherwise re-reads the base through `StatDef.base` and evaluates on the spot when it differs from
      the cached base. A feature that writes a base component directly (engine damage lowering `Mobility.speed`)
      therefore cannot leave the cache stale, marked or not. A unit without modifiers answers the clamped base.
    - In `ThinkSet::Orders`' core slot, `recompute_stats` runs first, then `run_activities` (ADR-0018). The order is
      outcome-neutral, since reads never see a stale value, but it is fixed.
  - **Scopes.** Only unit scope exists in the prelude. Global, player, team and aura scopes come with their first
    user (M5 morale or M9 experience), applied before unit modifiers inside the same buckets; that user revisits the
    count bound. Per-weapon stats (keyed by weapon slot) are the first extension M2 needs; ADR-0018's "M2
    direct-fire gaps" section records how they fit.
  - Core registers `"move.speed"` (base `Mobility.speed`, `[0, 100]` metres per tick); movement reads it through
    `effective_stat`. The debug query `core.stats` breaks a unit's stats down into base, effective and modifiers.
- **Tags.** Content tags (an object's `tags:`) form the sorted table `"tags"`, at most 128 ("129 tags exceed the
  limit of 128" is a content error). A unit's tags are a `TagSet(u128)`, held in the Derived `Tags` (`"core.tags"`,
  set from content at spawn). A class expression (`vehicle & !truck & (tank | gun)`) is terms joined by `&`: `tag`,
  `!tag` and at most one `(a | b | …)` group, compiled to `ClassExpr { all, none, any }` masks; it matches a set
  when `all ⊆ set`, `none ∩ set = ∅` and `any` is empty or meets the set. An unknown tag or a second any-group is a
  content error.
- **Verbs.** Effect, trigger-condition and trigger-action verbs are registered by name
  (`DefRegistry::register_verb(family, name)`, `SimBuilder::register_effect`; tables `"effects"`,
  `"trigger_conditions"`, `"trigger_actions"`) and written in content as `[Name(args), …]` (`VerbList`). An unknown
  verb is a content error listing the family's names, and the canonical rules hash writes a verb as its name, then its
  fields. Trigger verbs have registries only, until map triggers exist.
- **Effects and their emission window** (R151).
  - An effect requested before the core slot of `ResolveSet::Effects` is an `EffectRequest` message
    (`"core.msg.effect"`). The core applier, `apply_effects`, runs in that slot and applies them in `(source SimId,
    emission order)` order (counter `effects.applied`).
  - An effect requested after that slot (the feature slot of `Effects`, `ResolveSet::Stats`, a `LifecycleSet::Death`
    explosion or ammo cook-off, or an effect's own `apply` chaining another) goes through `queue_effect` into the
    Hashed `EffectQueue` resource (`"core.effect_queue"`), never into a message that would have to cross a tick
    (R72). Its codec saves each entry as the effect's name (remapped through `"effects"`), its canonical argument
    bytes, the source `SimId` and the target. The next tick's `apply_effects` applies the queue first, in the order
    entries were queued, then that tick's messages. A chain therefore advances one link per tick and cannot loop
    within a tick. The resource exists only while it holds entries (the first `queue_effect` inserts it, the drain
    removes it), so a match without late effects hashes exactly as before: absent state writes nothing.
  - **Queue limits** (R153). An entry whose target unit despawned before it is applied is dropped, with a journal
    record `core.effect_dropped { effect, source, reason: "target_gone" }` (a `Point` target never drops; the source
    may be gone, as a death explosion's is). The queue holds at most `MAX_EFFECT_QUEUE` (4,096) entries: an entry
    queued past it is dropped, deterministically in queue order (the newest go), journaled with reason
    `"queue_full"`, counted in `effects.dropped`, and debug-asserted, since a full queue means a chain that
    re-queues itself without end.
  - In debug builds `step_tick` asserts that no `EffectRequest` message was written after `apply_effects` ran,
    naming the writer's system when it can, so a late emitter fails its first test instead of losing effects.
  - **Per-shot data is not an effect.** `EffectRequest` carries only a verb's content arguments, a source and a
    target. Impact point, angle, shell and penetration travel in the combat feature's own messages (ADR-0019), whose
    readers then request effects or add modifiers.
- **Command buttons.** A `ClientFeature` declares `CommandButton`s (`buttons()`), each with an id, icon, hotkey slot,
  command, enabling def component and an optional `requires`. The client compiles every `requires` when a match
  starts (a bad expression or two buttons in one slot is a startup error) and evaluates it against the
  `"core.conditions"` view channel (`fl_view::ConditionsView`): a button is hidden when no selected unit's type has
  its enabling def, enabled when some such unit meets `requires`, and disabled otherwise. M6 draws the panel.

### Consequences

* Good, because a feature changes another feature's numbers only through modifiers and conditions, in one documented
  order, without depending on its code.
* Good, because every name in content is checked at link time and stored and hashed by name, so content renumbering
  never breaks a save or replay.
* Good, because the cache is outcome-neutral: a load rebuilds it and a dirty read evaluates on the spot, so it stays
  out of the hash and the save.
* Bad, because the stacking order is fixed for good: a mechanic that needs another order (diminishing returns, say)
  needs a new op and a new ADR.
* Bad, because 128 tags and 16 modifiers per stat are hard limits; widening either changes formats and bounds.
* Bad, because an effect chain advances one link per tick through the `EffectQueue`, so a cook-off chain of three
  effects takes three ticks.
* Neutral, because trigger verbs and the wider scopes are registries and rules without a user yet.

### Confirmation

* Content half (Task 12, `fl_content/tests/vocabulary.rs`): `requires_compiles_and_evaluates`,
  `precedence_and_parentheses`, `unknown_condition_in_requires_is_error`, `malformed_expression_names_column`,
  `class_expressions_match`, `unknown_tag_is_error`, `second_any_group_is_error`, `tag_overflow_is_error`,
  `base_unit_tags`, `verb_list_deserializes_registered_verbs`, `unknown_verb_is_error`,
  `vocabulary_tables_are_sorted`, `base_rules_hash_unchanged`.
* Sim half (Task 17, `fl_sim/tests/vocab.rs`): `grants_count_and_revoke`, `requires_reads_conditions`,
  `no_modifiers_is_exactly_base`, `stacking_order_is_fixed`, `override_replaces_base`,
  `extreme_modifiers_stay_in_bounds`, `out_of_bounds_modifier_is_rejected`, `effective_stats_rebuild_after_load`,
  `move_speed_modifier_slows_unit`, `unit_tags_from_content`, `effects_apply_in_source_order`,
  `conditions_channel_published`, `stats_query_breaks_down`, `vocabulary_is_hash_neutral`; for the R151 rules,
  `add_pct_factor_never_negative`, `bucket_order_ignores_insertion_order`, `direct_base_write_is_never_stale`,
  `seventeenth_modifier_is_rejected`, `modifiers_component_removed_when_empty`, `late_effect_is_queued_for_next_tick`
  `late_effect_message_is_debug_asserted`, and for R153 `queued_effect_on_despawned_target_is_dropped` and
  `effect_queue_is_bounded`.
* Client half (Task 20, `fl_client/tests/buttons.rs`): `button_hidden_without_enabling_def`,
  `requires_disables_button`, `any_selected_unit_enables`, `slot_clash_is_startup_error`,
  `bad_requires_is_startup_error`, `core_buttons_registered`.
* Gate rows: `api` (the `fl_content`, `fl_protocol`, `fl_sim` and `fl_view` snapshots), the system-order and
  `formats__save_codecs` snapshots (`protected`), and Task 17's `golden_replays_verify` passing without a re-bless
  (`change-class` N).

## Pros and Cons of the Options

### A registered vocabulary

* Good, because the order, bounds and rounding are written once and tested against exact values.
* Good, because names are validated at link time and the runtime checks are bit operations and small sorted lists.
* Bad, because every new kind of value needs a registration and a stat definition.

### Per-feature fields and multipliers

* Good, because each feature stays simple at first and needs no shared API.
* Bad, because two features scaling one value compose in system order, which a schedule change silently alters.
* Bad, because mods cannot add a condition or a modifier without code (OpenRA's lesson, research N8).

### A general expression or scripting layer

* Good, because modders can write any formula.
* Bad, because an interpreter in the sim needs its own determinism, `Fx` arithmetic and sandboxing, and its scripts
  are opaque to the link pass and to the bounds that rule out overflow.
* Bad, because it adds a dependency and a second language for agents and modders to get right.

## More Information

* Spec §5.4 (registries, classes), §6.1 (def components), §9.2 C5, §10 (command panel).
* Research: `architecture-best-practices.md` N8 (conditions), N9 (modifiers; HOI4, 0 A.D. `ModifiersManager`),
  N11 (verbs), N12 (tags).
* Related: ADR-0004 (registries), ADR-0005 (hook sub-sets), ADR-0006 (classes, R72, R74), ADR-0015 (outcome-neutral
  caches, R84), ADR-0018 (order targets used by effects; the M2 direct-fire gaps), ADR-0019 (messages for effects).
* Rulings: R149 (count bound, `TransientKept` dirty set, `min` and `Override` bounds), R151 (factor clamp, total order,
  base check on read, late effects), R153 (queue limits).
* Revisit: when the first non-unit scope arrives (the count bound and the scope order), or when 128 tags run out.
