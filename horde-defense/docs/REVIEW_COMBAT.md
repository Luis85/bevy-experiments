# Combat, rules, storage and peak-load review

Review scope: the original GDD, all 34 acceptance criteria, and the later country,
recruitment, parallel-deployment and outbreak requirements in `EXPANSION.md`.
This report describes executable core behavior. Human comprehension, art quality
and renderer frame time need their separate native playtest evidence.

## Material findings and implemented corrections

| Finding | Correction | Regression evidence |
|---|---|---|
| A packed arrival lane could push already-spawned enemies rightward. | New arrivals begin behind the existing tail; collision correction never increases an existing actor's x. | `crowded_lane_arrivals_never_push_existing_hostiles_rightward` |
| NPCs could target invisible queued actors outside the arena. | `Enemy::in_view()` gates NPC targets, personal collision eligibility and the geometric test bot. Arrival cues still announce entries. | `npc_and_personal_attacks_cannot_target_unseen_entry_queue_positions` |
| Boss exposure used the attempt's global modulo clock. | Exposure uses time since that boss's arrival and the authored `BossTiming`. | `boss_exposure_is_relative_to_arrival_and_uses_authored_timing` |
| Caller announcements and reinforcements occurred together. | Two lane-specific warning events precede the arrivals; chosen lanes and the pending timer survive resume. | `caller_announces_specific_lanes_before_reinforcements_and_resume_preserves_them` |
| A Siege Brute's recovery had no distinct following windup. | The authored recovery expires before the next visible, timed attack windup begins. | `siege_brute_recovers_then_telegraphs_before_another_attack` |
| Boss defeat could be followed by more piercing hits or turret actions and extra ordinary payouts. | The first terminal boss outcome stops subsequent actors/followups. Remaining living ordinary hostiles receive no defeat rewards. | `a_boss_defeat_stops_piercing_followups_and_later_turrets` |
| Personal armor absorption still inflicted hidden minimum chip damage. | Fully absorbed ranged, melee and structure impacts deal zero and emit Armor feedback; finite-aim validation prevents invalid tracers. | `fully_absorbed_personal_hits_and_invalid_aim_do_not_create_hidden_damage` |
| NPC tracer feedback did not distinguish misses, defenses, protection and injury. | Each resolver result produces one classified impact event, tagged with source/target IDs and lane. Cosmetic effects do not reroll it. | `npc_impacts_distinguish_misses_defenses_protection_and_injury` |
| A turret's authored skill 15 was added to a second hardcoded base, producing 29. | Offensive structures now use explicit `npc_skill`, respect injured/stunned Dodge, and spend Ready after a critical fumble. | `npc_turret_uses_its_authored_skill_and_a_critical_failure_requires_ready` |
| Walls were treated as biological targets, consuming HT/knockdown/mortality rolls. | `resolve_object_attack` shares attack/damage/protection math without biological consequences or hidden extra dice. | `object_resolution_has_no_biological_injury_checks_or_hidden_random_draws` |
| ST training increased HP but not swung damage. | Supported human ST 1–20 uses arithmetic reference swing progression, preserving original ST-10 equipment modifiers; low FP reduces effective melee ST. Firearm damage remains separate. | Rules worked cases and `npc_strength_improves_hp_and_reference_swing_without_changing_bullets` |
| Risk preference was previously inert; declining an impossible shot offered no defensive option. | Low-odds policy selects legal Aim, waiting or All-Out Defense. The defensive maneuver grants +2 Dodge until the next maneuver and cannot also Aim/Attack. | Risk, legal-turn and `cautious_contact_defense_is_a_whole_maneuver_with_a_real_dodge_bonus` tests |
| Destroyed or salvaged blockers left stale enemy references until a later tick. | Removing a blocker clears its engagement relationships in the same transaction, permitting an immediate consistent save. | `destroyed_blockers_release_references_before_the_same_tick_can_be_saved` |
| A saved plan could silently lose unaffordable placements on deployment. | Preflight rejects it with the actual supply allowance; failed application releases its reservation and grants no attempt or reward. | `an_unaffordable_saved_plan_rejects_deployment_without_reserving_items_or_spending` |
| Corrupt numeric feedback or physiology could enter a save generation. | Writer and decoder validate finite state, footprints, budgets, actor identities, boss lifecycle, blocker references, reward IDs and biological invariants. | Invalid numeric/worker generation and backup recovery tests |
| A writer could acknowledge a generation larger than the reader's 64 MiB limit. | The complete encoded envelope is size-checked against the shared inclusive reader limit before any filesystem mutation. Both committed generations and an existing temporary file survive rejection unchanged. | `writer_and_reader_share_an_inclusive_generation_size_boundary`, `oversized_commit_preserves_both_readable_generations_and_any_temporary_file` |
| Every worker scanned the full shared stash every tick, and history copied the stash repeatedly. | Worker snapshots retain actual deployed gear; append cursors transfer only new loot. Settled reports retain at most the deployed four items plus the protected global four. | `parallel_loot_cursors_and_reports_do_not_duplicate_an_unused_shared_stash` |

## Requirement coverage

| GDD acceptance criteria | Core evidence and boundary |
|---|---|
| AC01–AC05 | Eleven immutable lanes, constant defender anchor, all-lane approach/contact targeting, symmetric one-yard close engagement, source-tagged contact and windup events. Visual plausibility of the shared line still requires human feedback. |
| AC06–AC11 | Immediate banked rewards, active identity ledger, bounded spawn capacity without debt, timer-triggered boss, atomic clear, retry and withdrawal. Results are read-only reports. |
| AC12–AC16 | Geometric personal hits; one NPC rules procedure; authored profiles for enemies, bosses, equipment and offensive structures; consciousness separate from death; visible-state policies without future-RNG access. |
| AC17–AC18 | Creation and advancement accounting, individual NPC ledgers and global equipment reservations. Idle-recruit sale reconciliation and transaction validation are covered by campaign/strategy tests. |
| AC19–AC21 | Same-lane blockers, left-half footprints, supply preflight, full pre-combat refund and health-scaled half salvage after combat. Removing a blocker resumes travel without rerouting. |
| AC22–AC24 | Global tactical pause, whole fixed ticks, saved RNG/actions/settlement cursors, checksum and complete-generation backup recovery. Native input scheduling and render FPS are application-layer checks. |
| AC25–AC29 | Protected starter set, renewable reserve ammunition, bounded repeats, stage-labelled probability functions, fixed eleven-lane geometry and the three-seed five-stage progression proof in both modes using earned currency and ordinary market purchases. Every customized country/modifier combination is not exhaustively balance-tested. |
| AC30–AC34 | Terminal unconscious hostiles release slots and pay once; temporary stun/posture/Ready recovery remains legal; melee has exclusive recovery; loot has no gameplay item cap; post-clear actors cannot issue extra payouts. |
| Country expansion | Active country/local stage/modifiers persist; repeated stages cannot clear a different frontier; independent workers retain real battles and RNG; whole-country orders stop on defeat/recall/invalid deployment; shared rewards settle monotonically. |

## Separation of responsibilities

`sim.rs` contains the serializable battle model and `tick` orchestration; its
phases live in `sim/{director,movement,structures,npc,manual,combat,lifecycle}.rs`. It performs no filesystem IO. `sim/parallel.rs` owns
multi-expedition dispatch, equipment reservations and shared reward settlement.
`sim/persistence.rs` is the storage adapter: versioned envelopes, atomic commit,
generation recovery and full-state validation. Child modules share private model
access; this extraction did not widen field visibility or change `Game::save`,
`load`, deployment or tick signatures.

`Game::decode_save_generation` is a pure single-generation decoding port. The
application's recovery service can inspect and preserve unreadable originals
without duplicating envelope validation or accidentally invoking backup fallback.
Rules resolution remains pure in `rules.rs`; Bevy, egui, audio and particles are
consumers of events, not additional authorities over hit results.

`CombatTelemetry` stores bounded aggregates: spawn/skip/peak counts, classified
attack outcomes, per-lane damage, maneuver counts, actor/structure contributions
and boss timestamps. No frame-by-frame permanent history is added. Events cap
at 512 and logs at 80 per field; active enemies use the authored ceiling with a
reserved boss slot. The shared stash remains uncapped by gameplay.
The version-1 storage format separately limits each encoded generation to
64 MiB. Oversized checkpoints fail explicitly without discarding or selling
inventory and without replacing a readable prior checkpoint.

## Verification and measured CPU cost

Executed inside this workspace:

```powershell
cargo test --locked -p horde_core --target-dir target/core-check
cargo clippy --locked -p horde_core --all-targets --target-dir target/core-check -- -D warnings
$env:LASTLIGHT_BENCH_CPU=(Get-CimInstance Win32_Processor).Name
cargo run --locked -p horde_core --example peak_benchmark --target-dir target/core-check -- --ticks 12000 --output target/peak-benchmark.json
```

The core gate passed **113 tests** (72 unit tests and 41 integration tests), with
zero failures or ignored tests. Every previous assertion remains, including all
five campaign gates in both modes for seeds 1, 2026 and 0x1A571167. Clippy passed
with warnings denied. The final combined workspace gate also includes the
application's newer command, input and recovery tests.

CPU measurement: **Intel(R) Core(TM) Ultra 9 185H**, dev profile with core
optimization level 1 and dependencies at level 2. Each case measured 12,000
complete fixed steps after 600 warmup steps. Maintaining peak population is
included; initial fixture creation and reset cloning are excluded. Encounters
restart after terminal outcomes, so this measures cost rather than survival.

| Case | Peak enemies / structures | Mean | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|---:|
| Personal peak | 73 / 10 | 6.17 µs | 7.9 µs | 10.1 µs | 41.6 µs |
| NPC peak | 73 / 10 | 6.17 µs | 9.3 µs | 25.9 µs | 93.4 µs |
| Four parallel peaks | 292 / 40 | 23.63 µs | 27.8 µs | 83.0 µs | 237.1 µs |

Observed aggregate event/log peaks were 94/10, 75/54 and 300/227 respectively;
the benchmark asserts each individual field's caps throughout. The full machine
report is generated at `target/peak-benchmark.json`. These values exclude the
renderer, UI, audio and OS presentation and establish **no GUI FPS claim or
minimum-spec promise**.

`peak_render_fixture()` supplies a separate validated, paused 73-actor scenario
with genuinely affordable mixed defenses for native screenshot and frame-time
review. It does not write a user save or silently grant resources in normal play.

## Remaining playtest limits

- The highest-priority human test remains explaining why an outer-lane contact
  damages the defender and how to answer it. Automated geometry cannot establish
  that players understand this abstraction.
- The seeded progression proof covers the five neutral encounter templates.
  Country dressing, modifiers, archetypes and engineering builds need broader
  measured human clear-rate and purchase-pacing data; there is no claim of
  exhaustive balance across all configurable countries.
- Clear/replay versus deliberate boss-delay income, including potential loot
  sale value, remains a pacing measurement. A spawn-rate ceiling alone does not
  prove real farming efficiency: a contact boss can block personal firing rays,
  and NPC/structure policies may target it automatically. Completion rewards
  were not raised on the strength of that analytic ceiling alone.
- The supported Lite scope is documented in `RULES.md`. Shared melee skill,
  stationary engagement, renewable ammunition, fixed actor ordering, authored
  equipment modifiers and object durability are explicit adapters. This is not
  a complete tabletop implementation or a resolved publisher distribution
  license.
- Cross-release content migration remains a release task. Current generation
  recovery and additive field defaults preserve valid current saves; arbitrary
  manual edits to item identities or incompatible authored data are rejected.
