# Simulation and persistence

The authoritative model lives in horde_core::sim without Bevy types/filesystem IO. phase logic lives in sim/{director,movement,structures,npc,manual,combat,lifecycle}; sim/parallel owns worker coordination and sim/persistence validates/commits generations. Each unpaused Game::tick advances 1/60 simulated second. Attempt.lane_count, width_yards and copied balance freeze at deployment: Normal begins3/12 and grows to 17/30, while legacy missing geometry preserves11/30 and old multipliers. The defender is(0.5,lane_count/2), contact x=1.5; presentation cannot change those coordinates.

Personal shots intersect enemy circles along the actual aim ray, sorted by distance; the first circle receives the hit unless the owned penetration skill enables the second. There is no attack roll in personal mode. Melee selects only the front contact enemy in the cursor's lane. A legal strike cancels reload and commits thirty ticks of exclusive recovery; the seventy-two-tick strike cooldown and stamina are separate restrictions.

NPCs select legal actions on one-second boundaries. Defender acts before hostiles; hostiles retain stable spawn order. A newly arrived hostile must finish its authored windup before its next legal attack opportunity. Aim holds a target unless an actual contact emergency warrants interruption. Active defense or injury interrupts Aim. Readying, magazine reload, stun recovery and posture consume turns. Switching between the slung rifle and melee weapon consumes a Ready turn; the held-weapon state is saved. Hostiles use natural attacks and cannot drop a claw. All NPC-mode attacks, including offensive structures, call the same rules resolver once; visual tracers consume its result. Wounded hostile movement is halved at low HP and separately at low FP.

The **permitted risky actions** preference gates ranged shot selection at the next legal decision. With risk disabled, the NPC declines an effective attack-roll skill below 8 (before defense and protection), takes up to three legal Aim turns when that can reach the threshold, or spends Do Nothing waiting for a closer or safer shot. With risk enabled, otherwise legal skill-3-through-7 attacks are permitted; an effective skill below 3 remains illegal and requires useful Aim or waiting. The threshold is an original tactical policy, not a modifier to the GURPS dice procedure. Cautious aiming can override the optional preference to avoid Aim. Policy changes never inspect future rolls, consume random draws merely to choose an action, or reset a pending Ready/reload timer.

Enemies never block each other (user decision, 2026-11-06, W1-69). There is no lane queue and no overtaking: bodies in one lane overlap. Each enemy walks at its own effective speed until it reaches the defense line (x = 1.5) or the face of a live non-trap structure in its lane, and every enemy that gets there makes contact and attacks; structures are barricades, not traffic. Movement still runs lane by lane in x then stable-ID order, so event order stays deterministic. `EnemyPhase::Queued` is never assigned; saves that hold it load and resolve to Advance or contact on the next tick. The saved `pass_offset` (bounded and validated as before) only eases back to 0 at the old two-yards-per-second rate. Enemy.y() drives hits/range/AoE/corpses/loot. Traps cover the full lane, and no actor changes assigned lane or bypasses a blocker. Presentation draws a crowd on one spot apart with a static per-id stagger (at most 0.15 yd behind the feet and 0.12 lane across, never from an RNG) and paints each lane far to near, x descending, a stack on one spot by its drawn feet (y + stagger, farther first), then by id; collapsing and settled bodies lie at the same staggered spot. Shared construction_bounds/build_cell helpers derive five columns from actual width. See [OVERTAKING_AND_CRITICAL.md](OVERTAKING_AND_CRITICAL.md).

The director integrates the authored increasing arrival envelope, skips ordinary arrivals while at capacity, and accumulates no deferred spawn debt. The boss has its reserved slot and arrives at the authored time regardless of ordinary capacity. `BossTiming` authors warnings, exposure and recovery: defaults expose protection for four seconds each twelve-second cycle, starting seven seconds after that boss arrives. The Caller announces specific reinforcement lanes two seconds before entry. Siege Brute strikes deal extra structural damage in both numerical modes, then recover for four seconds before a fresh attack windup. Defeating a boss stops later actors and piercing followups, then removes remaining ordinary hostiles without rewards.

Kill identities are recorded in the active attempt ledger. A single boss transaction banks boss coins, completion coins, guaranteed equipment and the victory. The results screen only reads its report. Ordinary kill, loot and XP transactions bank immediately; withdrawal cannot lose them or grant a completion payout. Personal XP and NPC objectives have different recipients.

## Geographical campaigns

A deployment records the selected country ID and local stage, then rotates through the five authored encounter templates using `stage % 5`. Its saved effective modifiers combine the country's authored boosts/debuffs with its effective difficulty (the start country is clamped to 0.95-1.05; WORLD.md): enemy health and rewards multiply by difficulty; enemy speed multiplies by its square root. Enemy health includes bosses in both combat modes. Build supply and the declared personal-damage and NPC-skill modifiers also apply. Completing an earlier local stage cannot advance a different frontier stage. A country clears only when every population-derived local stage is complete.

## Concurrent deployments

`Game::deploy_survivor` launches one actual NPC battle. Each battle stores an independent boxed worker game, explicit seed, current action and active reward ledger. Worker games cannot dispatch workers, so the serialized tree is bounded to one level. The configured parallel limit defaults to four; at most thirty-two recent battle records remain visible.

The root owns the shared economy. Every simulation tick it transfers only the increase after each worker's last synchronized wallet cursor. New loot is transferred by monotonic identity and append cursors and receives new globally unique item identities. Workers retain deployed gear instead of cloning the unused stash; settled histories compact to that loadout plus the protected starter set. Actual loadout items are reserved globally until recall or completion. Each recruited NPC's advancement and history return to that same NPC. Workers do not buy, sell or train automatically.

`Game::deploy_country_operation` is a later, separately gated order. It chains actual local-stage battles until the country is freed, the survivor loses, the player recalls them, or deployment validation fails. The normal between-attempt health and equipment recovery applies; no coins are spent. A single-stage parallel battle remains available for individual encounters and outbreak responses.

Call `Game::tick` even in camp to advance workers and the strategic clock. `Game::set_paused` freezes the entire simulation, including parallel battles, recovery and outbreaks. Construction remains live; explicit pause and Settings are separate. Observation speed means more whole fixed ticks; no actor receives an extra action per simulated second.

## Balance, DPS and history

Validated BalanceSettings is copied at deployment; changing the next-run selection cannot alter active main/worker profiles, rows or width. Shared core helpers own spawn/view bounds, building and ordnance.

Attempt.live_dps uses positive post-protection resolved injury in the last 300 fixed ticks divided by 5; the opening is zero-padded. average_dps uses elapsed simulation time; peak_dps retains the rolling maximum. DamageTotals separates defender/structures/ordnance, including logged overkill. Misses, absorption, input and graphics add no damage. Pause/save/1×/2×/4× preserve the same clock.

RunRecord history is untruncated and parent-owned. Main outcomes append once; saved worker outboxes drain once into global records, including victory/defeat/withdrawal. Records describe already-banked transactions and never pay them again. Older finalized results are not fabricated. The 64 MiB generation ceiling remains explicit. See [BALANCE_AND_RUN_STATS.md](BALANCE_AND_RUN_STATS.md).

## Saves

`Game::save` serializes the entire campaign, current battle, workers, RNGs, action timers, strategic state and settlement cursors into one versioned generation with a checksum. It flushes a temporary file, retains the previous valid generation as `.bak`, then renames the temporary file into place. `Game::load` checks content compatibility, development ledgers, battlefield bounds, worker depth, roster and equipment reservations; a corrupt main generation falls back to the complete backup. Validation also runs before writing; an inconsistent worker or equipment reservation cannot replace the previous valid generation. It never mixes a newer wallet with an older battle.

Every save format version (the envelope is currently v7; see [SAVE_FORMAT.md](SAVE_FORMAT.md)) has a technical **64 MiB limit per complete encoded save generation**, including its envelope. Writer, file reader and single-generation decoder share this exact inclusive limit. The writer checks the encoded size before creating directories or touching temporary, primary or backup files; an oversized checkpoint returns an error and retains both previous readable generations. This is a storage-format limit, separate from the armory's lack of a gameplay item-capacity limit. Loot is never silently dropped or sold to fit a checkpoint. Supporting larger generations requires a future storage-format change; the current game cannot acknowledge an oversized save as successful.

## Save format 7

`SAVE_VERSION` is 7 (the user's decision before the production pass, W1-14) and `RULESET_VERSION` is 4 (W4-08). The envelope, every v7 key and each migration sub-step are specified in [SAVE_FORMAT.md](SAVE_FORMAT.md); this section is the summary.

**Migration chain.** An older generation runs the raw-JSON chain in `sim/migrations.rs` before typed decoding; a v7 payload decodes directly, and version 0 or anything above 7 is refused (`decode_rejects_version_8`). Each step freezes its source version's meaning and never reads the current catalogues:

- v1→v2: item indices become IDs and per-kind arrays become name-keyed maps.
- v2→v3: status counters move into `statuses`, unlock flags become IDs, and the runner gains `target_urgency`.
- v3→v4: quality-indexed loot arrays become maps.
- v4→v5: ordnance per-kind arrays become `KindMap<OrdnanceKind, u32>`.
- v5→v6: level-positional clears, claims, frontier and level references become stable level IDs, using the frozen v5 level order.
- v6→v7: path-based, idempotent `or_insert` back-fills in a fixed order: the expedition games' `defender` seats (`{"Recruit": id}` from each `battles[*].recruit_id`; the root attempt and report are never touched, so a v6 root reads as its mode owner), the Npc `appearance` Female that every v6 Npc deploy stamped, the reports' `drop_ids` and `loot_by_quality`, the `level_grant_paid` ledger (no retroactive level-up grant) and the `milestones_claimed` ranks a v6 campaign had already reached (no retroactive milestone). The auto-sell rule, attempt drop tracking, receipts and pending sales keep their defaults, off and empty; no outcome notice is raised for a v6 expedition that had already settled.

Running a step twice gives the same value, and every migrated fixture resaves as v7 and resumes byte-identically (`migrated_v1_fixture_resaves_as_current_version_and_resumes_byte_identically` through `migrated_v6_fixture_resaves_as_current_version_and_resumes_byte_identically`). Debug recordings with a header below 7 replay through the same migration under the current rules (the sale rules of ruleset 2 and later included), so a pre-v7 recording may diverge; replay still reaches its end line.

**Fixtures.** `crates/horde_core/tests/fixtures/` holds one real generation per version: `save_v1.json`, `save_v2.json`, `save_v3.json`, `save_v4.json` and `save_v5.json`, each recorded on the build that wrote it; `save_v6.json`, cut on untouched v6 code (W1-01, parent `3d492cd`) before any v7 schema edit; and `save_v7.json`, the schema-freeze file (W1-63): a `Game::save` generation of envelope 7 and ruleset 2 in which every `V7_SAVED_KEYS` key sits at a non-default value. Its checksum and byte length are pinned, and it is never re-cut; later cases use inline JSON or new fixture files.

**The freeze rule.** Since the W1-63 freeze a new saved field must be serde-defaulted, skipped at exactly its neutral value through the `schema.rs` predicates (`is_default`, `is_false`, `is_zero_f32`, `is_one_f32`) and must load `save_v7.json` unchanged; otherwise the work stops and the user is asked. The one pre-approved exception was `SkillModifiers.heavy_target_damage_multiplier` (ruling R5). `V7_SAVED_KEYS` (`tests/support/mod.rs`) registers every v7 key, and `tests/schema_v7.rs` holds it equal to the types (`v7_saved_keys_registry_matches_types`), round-trips the v6 payload unchanged through the v7 types (`v6_payload_round_trips_unchanged_through_v7_types`) and keeps a default game and the golden games free of v7 keys (`default_and_golden_games_serialize_no_v7_key`), so an unused v7 feature never moves a digest.

**Settings are not saves.** `settings.json` has its own `SETTINGS_VERSION` (2 since A6-16, the calibrated mix): a file below it has each untouched wave-1 volume default migrated once, each unreadable key takes its default alone, the unreadable original is preserved before defaults, and the eight core bindings stay a `[String; 8]`. Settings never enter the state digest.

## Determinism digest and golden tests

`Game::state_digest()` returns the FNV-1a hash (the same function as the save checksum) of the canonical serialized save payload. It covers every authoritative field including RNG state, so equal digests mean identical state. It is the determinism and replay digest: tests and tooling compare one `u64` instead of whole games. Save-format changes alone move it (the serialized shape is part of the hash), so a format change also needs a migration and a checked-in legacy fixture.

`tests/properties.rs` adds dependency-free seeded-loop properties (exact-once rewards, saturating coins, save/load/save byte equality, idempotent settlement, per-battle strictly increasing event sequence numbers). Shared test helpers live in `tests/support/mod.rs`.

## Determinism pins

Every pin below is a constant in `crates/horde_core/tests/determinism.rs` or `crates/horde_core/tests/save_migration.rs`. A failure means gameplay outcomes, RNG consumption, the serialized shape or a frozen record changed.

| Pin | Test | What it hashes | Who may move it |
|---|---|---|---|
| `GOLDEN` | `seeded_headless_runs_reach_the_pinned_state_digests` | The state digest after 1800 ticks of the headless bot: seeds 1, 0x1A57_1167 and 0xDEAD_BEEF_2026, each in Personal and Npc mode (six runs). | Only a save-determinism re-bless, through `PROVISIONAL` (below). |
| `GAMEPLAY_TRACE` | `seeded_headless_runs_keep_their_pinned_gameplay_traces` | Per tick of the same six runs: tick, hp, coins, enemies and events, no serialized format. It justifies a format-only `GOLDEN` move. Recorded under `TRACE_RULESET`, which follows `RULESET_VERSION` (`gameplay_trace_is_recorded_under_the_current_ruleset`). | As `GOLDEN`. |
| `PROGRESSED_GOLDEN`, `PROGRESSED_TRACE` | `progressed_runs_reach_the_pinned_state_digests_and_traces`, with `progressed_golden_runs_exercise_elites_boss_phases_and_new_archetypes` | Four runs the six never reach (W4-09): seeds 1 and 0xDEAD_BEEF_2026 as `HardCapstone` and `RecruitTakeover`, threat rank 12, 3600 ticks, each spawning an elite, entering a boss phase and killing a wave-2 archetype. | As `GOLDEN`, through `PROGRESSED_PROVISIONAL`. |
| `V1_CONTINUATION_FNV`, `V2_CONTINUATION`, `V3_CONTINUATION_FNV`, `V4_CONTINUATION_FNV`, `V5_CONTINUATION_FNV`, `V6_CONTINUATION_FNV` | `v1_fixture_continues_exactly_as_the_v2_build_did`, `v2_fixture_continues_exactly_as_the_v2_build_did` and `v3_` to `v6_fixture_continues_exactly_as_the_v<N>_build_did` | Each migrated legacy fixture continued on today's code, recorded on the build that wrote the fixture (v1 on the v2 build). The v6 lines add each expedition and leave out campaign coins, items and the strategy RNG. | Never. Live legacy attempts keep their frozen snapshot fields, and the catalogue records they tick are frozen (`FROZEN_CATALOGUE_FNV`). |
| `V6_DROP_TRACE_FNV` | `v6_drop_trace_is_pinned` | The first drops after the v6 fixture resumes: definition, quality, level and the attempt RNG after each. An attempt deployed before v7 keeps the legacy drop path byte-identical (ruling E11). | Never. |
| `QUALITY_ROLL_TABLE_FNV` | `quality_roll_table_is_pinned` | 10,000 keyed quality rolls, recorded on the v6 build before the mixer moved to `crate::hash` (W1-09). | Never. |
| `V6_STASH_TABLE_FNV` | `v6_stash_item_stats_and_values_are_pinned` | Every v6 stash item's effective stats and sale value. | Never. |
| `FROZEN_CATALOGUE_FNV` | `legacy_catalogue_records_that_tick_saved_encounters_are_frozen` | The catalogue records a saved v1-v6 encounter ticks: the 11 frozen archetypes, the legacy kind defaults, wave tiers 0-4 and the five content levels (W2-53). A move means a frozen record was edited: fix the edit. | Never. |
| `V7_FIXTURE_CHECKSUM`, `V7_FIXTURE_BYTES` | `v7_fixture_is_a_real_v7_generation`, with `v7_fixture_loads_validates_and_resaves_byte_identically` | The identity of `save_v7.json`: its envelope checksum and byte length. | Never: the file is frozen. |
| `V7_CONTINUATION_FNV` | `v7_fixture_continues_exactly_as_the_wave4_close_did` | `save_v7.json` continued for 600 idle ticks, sampled every 50, in the v6 line format (W4-09). | As `GOLDEN`; waves 5-7 expect no move. |

**The provisional-pin protocol (ruling P3).** A commit that moves `GOLDEN` or `GAMEPLAY_TRACE` never edits them: it sets `PROVISIONAL` to the new digests (printed by `cargo test -p horde_core --test determinism -- --nocapture`) and appends a `PROVISIONAL_CAUSES` row (its tag, its class and the reason), and `PROGRESSED_PROVISIONAL` and `PROGRESSED_PROVISIONAL_CAUSES` do the same for the progressed pair (`provisional_digests_name_their_causes`). The class comes from the determinism anatomy (`support::classify`), in precedence order: G (gameplay: the outcome or the trace moved, or core state with nothing else to explain it), T (only telemetry and receipt subtrees), C (campaign items) and F (the serialized key paths). One save-determinism re-bless per wave folds the provisional digests into the pins with a written reason per digest, and moves `RULESET_VERSION` and `TRACE_RULESET` when the rules changed. The re-blesses so far:

- W1-69 (the user's decision of 6 November 2026 that enemies overlap): `GOLDEN` 1/6 and `GAMEPLAY_TRACE` 3/6, class G.
- W2-49: six `GOLDEN` moves folded (net F: telemetry, drop tracking, the drop pick and the level-grant ledger, with unchanged items, outcomes and traces), `GAMEPLAY_TRACE` 0/6, ruleset 3.
- W4-08, the single re-bless for waves 3 and 4: nothing provisional to fold, `GOLDEN` and `GAMEPLAY_TRACE` 0/6, ruleset 4. W4-09 then added the progressed pair and `V7_CONTINUATION_FNV`.

Waves 5 to 7 are presentation, audio and docs, and expect `GOLDEN` 0/6, `PROGRESSED_GOLDEN` and `PROGRESSED_TRACE` 0/4 and `GAMEPLAY_TRACE` 0/6. Under ruling R36 a digest move after W4-08 stays provisional and is re-blessed once in the closing phase, which also records the final zero-move run. The V1-V6 continuation pins never move.

## Keyed rolls and RNG streams

The simulation has two saved RNG streams, both `rules::Rng` and both consumed only by core: `Attempt.rng`, one per attempt (seeded from the game seed and the attempt id), for combat, spawns and drops; and `StrategyState.rng` for strategic pressure. Their states are in the save and in the digest.

Decisions the production pass added must not change how often those streams draw, so they use a stateless keyed roll instead: `hash::keyed(seed, a, b, tag)` mixes the game seed, two stable ids and a purpose tag through the frozen SplitMix `hash::mix`. The tags in `hash::tags` are big-endian ASCII, append-only and never reused (`keyed_tags_are_distinct_frozen_ascii`):

| Tag | Decides | Keyed by |
|---|---|---|
| `QUALITY` | A drop's quality (`quality.rs`) | attempt, enemy |
| `ELITE` | Whether a spawned enemy is elite, at the snapshot's elite rate (`sim/director.rs`) | attempt, enemy |
| `AFFIX` | An elite's affix (`sim/director.rs`) | attempt, enemy |
| `ELITE_PICK` | The definition of an elite-only drop (`sim/combat.rs`) | attempt, enemy |
| `RECRUIT_OFFER` | The second archetype of a recruit offer (`strategy.rs`) | next recruit id, country |
| `MILESTONE` | The rank milestone's item (`campaign/progression.rs`) | rank |

So elite status never changes a kill's combat draw count (`elite_status_never_changes_combat_rng_draw_count`), recruiting draws no strategy RNG (`recruiting_draws_no_strategy_rng`) and auto-sell draws nothing (`auto_sell_consumes_no_rng`).

**Presentation never reads core RNG.** `presentation_never_reads_core_rng` (`crates/horde_game/src/architecture_tests.rs`) scans `horde_game/src`, `debug_tools/` excepted, for `.rng`, `Rng::`, `hash::keyed`, `keyed::`, the quality rolls (`catalog().roll(`, `.roll_relative(`, `drop_quality(`) and `.tick(` outside `runner/` and the three review recipe files (ruling R13), with a self-test against planted strings. Audio and VFX variation is keyed by `SimEventRecord.seq` or a frontend counter.

**Deferred on purpose.** Per-purpose RNG sub-streams for the existing draws and fixed-point coordinates would change every seeded outcome (and every pin above), and cross-machine lockstep multiplayer is not a goal, so f32 simulation and the two saved streams stay; keyed rolls are stateless and add no stream. Digests are pinned for the supported build toolchain, not promised across platforms or compiler versions.

## Checks and limits

`cargo test --locked -p horde_core` covers contact windup, actual frozen geometry and legacy eleven-lane aiming, exclusive melee recovery, reload cancellation, structure placement/blocking/refunds, bounded spawning, exact-once rewards, boss completion, terminal unconsciousness, fixed inputs, country-stage replays, parallel reward and save accounting, global pause, and strategic country operations. `cargo run --locked -p horde_core --example campaign_smoke` runs full encounters using ordinary aim inputs and NPC decisions, purchasing upgrades only from actually earned coins. The campaign regression repeats all five gates in both modes for seeds 1, 2026 and 0x1A571167, without equipping dropped loot. The autonomous proof uses the accessible Boss Focus policy, affordable market upgrades and ordinary point training; the first failed attempt may fund the next one.

The deterministic bot is a combat/economy test, not evidence of human aiming enjoyment, interface comprehension or minimum-spec performance. Authored game adapters, the supported rules subset and excluded supplements are described in `RULES.md`. Infinite reserve ammunition, lane geometry, actor ordering, monster profiles and campaign recovery are explicit scenario rules.

## Native storage adaptation

The core storage adapter still validates/writes one coherent immutable generation; native Runtime now runs that operation through the single-worker service in runner/persistence.rs. Main-frame enqueue/poll does not wait for IO. Critical navigation/reset requests gate later mutations and raw settings input until matching durable acknowledgment, with epochs preventing stale profile completion. This execution-policy change does not move combat, RNG, NPC decisions or reward accounting off the authoritative fixed-step owner. [JOURNEYS.md](JOURNEYS.md) specifies that seam.
