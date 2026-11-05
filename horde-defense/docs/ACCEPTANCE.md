# Acceptance evidence ledger

This maps the supplied GDD's 34 criteria to implementation and regression evidence. A test name is a verification target, not a claim that a GUI playtest occurred. Exact executed commands/results belong in the final validation report. Rendering, player comprehension, broad balance and minimum-spec performance require interactive evidence in addition to compilation.

| ID | Implementation / automated evidence | Remaining assessment |
|---|---|---|
| AC01 | `sim::LANES=11`, fixed `DEFENDER=[0.5,5.5]`; all-lane targeting and lane-preservation tests | Inspect all eleven lanes in the native window at supported sizes/scales. |
| AC02 | Same-lane sorted movement and queues; `enemies_keep_lanes_and_contact_has_a_windup` | Visual clarity of queues and death effects. |
| AC03 | Geometric personal rays from the fixed origin; `starter_can_hit_all_eleven_contact_and_approach_lanes` covers every lane at x=1.5,14,29 | Aim feel and silhouette readability. |
| AC04 | Same contact predicate and one-yard NPC adapter; `melee_contact_eligibility_and_exclusive_recovery` | Human understanding of shared contact across outer lanes. |
| AC05 | Contact records entry tick and starts windup; `enemies_keep_lanes_and_contact_has_a_windup` | Visible attack-source cues must be checked in play. |
| AC06 | Defeated enemies are removed before further actions; per-attempt reward-ID ledger; `kills_pay_once_and_results_never_pay_again` | None beyond regression execution. |
| AC07 | Authored growing envelope, live cap, skipped overflow without debt; `spawn_capacity_skips_without_a_deferred_burst` and content envelope test | Representative peak performance remains unmeasured. |
| AC08 | Timer spawns boss; clear follows terminal boss defeat; `boss_timer_is_not_victory_and_boss_transaction_is_atomic` | Boss announcement/exposure legibility in play. |
| AC09 | Boss exposure changes protection for supported ordinary attacks; every boss has an NPC profile | Seeded campaign clearability and actual player counterplay remain balance checks. |
| AC10 | Banked campaign state survives result; fresh deployment rebuilds encounter; reward and replay tests | Manual retry flow should be played through. |
| AC11 | Result/camp routes exist for victory, defeat and recall | Native UI journey validation. |
| AC12 | Personal geometric hit path applies damage directly, with no NPC attack roll | Test and UI inspection. |
| AC13 | Pure `rules::resolve_attack` returns authoritative results; NPC cosmetic events consume those results; `npc_manual_inputs_do_not_fire_or_reset_action_timers` | Combat-log readability. |
| AC14 | Required dual profiles in validated content; explicit NPC boss/armor/damage-type data; rules reference cases | Supported subset and documented scenario adapters are the claimed scope, not unrestricted GURPS compliance. |
| AC15 | InjuryState tracks consciousness/death independently of HP; `wounded_npc_at_zero_is_not_automatically_dead` | None beyond reference cases and execution. |
| AC16 | Policies inspect current enemies, condition and announced cues; stable spawn-ID ties; RNG consumed only by resolution/director | Policy quality and commitment behavior remain playtest topics. |
| AC17 | Awarded/unspent/investment ledger conserved and validated; `training_cannot_reset_allowance`; pending-award tests | None beyond execution. |
| AC18 | Persistent item IDs, references in presets, main/external reservations; reservation/preset tests | Sale-warning and reservation-error UI readability. |
| AC19 | Same-lane walls/turrets block; destroyed or salvaged blockers release movement; `construction_bounds_invalid_spend_salvage_and_blocking` | Visual blocker interaction. |
| AC20 | Eleven-by-five validated cells, authored centers/footprints confined to x=3..15 | Native construction grid inspection. |
| AC21 | Placement validates before spending; salvage is bounded; invalid-layout and construction tests | Preview text/range/footprint clarity. |
| AC22 | Paused simulation returns before clocks/actions; `pause_freezes_every_authoritative_clock`; global pause includes parallel and strategy state | Mouse pause/build flow inspection. |
| AC23 | Atomic generation includes campaign, RNG, actions, active rewards, parallel and strategy state; serialization continuation and native `--smoke` round trip | Interrupted filesystem-write and recovery-path testing should be reported separately. |
| AC24 | Fixed 60 Hz schedule and one-second NPC opportunities; render speed feeds whole ticks | Equivalent-input frame/observation-speed regression plus actual render responsiveness. |
| AC25 | Four protected starters, unlimited retained armory; `starter_set_survives_every_sale`; all-lane starter shot tests | Winning an opening encounter with starter-only equipment remains a balance evidence requirement. |
| AC26 | Bounded repeats on an already-cleared NPC encounter; explicit stop conditions; no shop/training/sale in repeat step | Whole-repeat journey and country-specific routing need regression evidence. |
| AC27 | Probability labels distinguish attack-roll success from defense/injury in NPC detail | Human-readable labeling in native UI. |
| AC28 | Arena uses its own fitted logical rectangle, separate from management panels | Inspect 1050×700 and 1440×900 at supported text scales; no compile-only claim. |
| AC29 | Guaranteed purchasable gear for each slot/tier; `optional_market_covers_each_slot_before_every_gate`; bounded NPC points and authored five-template campaign | Market availability alone does **not** establish solvability. Measure seeded clears and attainable pre-gate purchases in both modes. |
| AC30 | Terminal unconscious hostiles are removed and pay once; `unconscious_hostile_and_boss_release_slot_and_pay_once` | None beyond execution. |
| AC31 | Stun recovery, posture and Ready consume legal NPC actions; terminal consciousness/death ends attempt | Reference cases and simulation regression evidence. |
| AC32 | Personal melee cancels reload and sets separate committed recovery/cooldown; exclusive-recovery and reload-cancel tests | Commitment feedback in play. |
| AC33 | Shared armory has no gameplay stash cap; `all_loot_is_retained_without_capacity_limit` adds 1,000 items | Presentation/scroll usability under a large stash. |
| AC34 | Boss reward plus completion bonus enter a single ledger-gated defeat transaction; first-clear flags idempotent; atomic boss transaction test | Reopening native results must remain presentation-only. |

## Country and strategic extensions

The later user requests extend the GDD. World validation covers country IDs, population-derived stage counts, geometry, symmetric neighbors, authored sea connections, overrides, and modifier bounds. Country clears use local stage identities: replaying an already completed stage cannot advance another stage. Liberating a country unlocks neighbors. Plague may reopen only the final stage while preserving earned travel routes.

Strategic tests cover independent recruited equipment, exclusive survivor roles and equipment reservations, seeded continuation, containment idempotency, and failure reopening a country without free coins. Guard combat is a documented scenario adapter using the shared rules resolver; each guard and each active hostile group receive one action at their own legal one-second boundary. Parallel battle ownership and live reward merging belong to the simulation suite. No offline rewards or pressure are implemented.

## Pending human checks

These have no automated substitute and do not block a wave. The performance gates themselves live in `docs/PERF_BUDGET.json`.

| Check | Protocol | Status |
|---|---|---|
| Integrated-GPU run | `measure_performance.ps1 -Adapter "Intel(R) Arc(TM) Graphics" -VsyncOff` on progressed-peak and stress at both sizes (informational, not a gate). Then set `hardware.integrated_gpu` in `docs/PERF_BUDGET.json` to `status` `measured` with the report path in `report`. | pending human check |

## Explicit limits

The game does not claim implementation of all GURPS rules or supplements. The supported reference is Fourth Edition Lite August 2004 with original fixed-lane, shared-contact, renewable-reserve, campaign-recovery and strategic-garrison adapters. Unified melee skill remains an explicit adapter; supported muscle-powered profiles now scale from the NPC's actual Strength through the verified swing procedure. Independent HP/FP buying and informational IQ training are disabled rather than sold without useful supported effects. A digital-game distribution license is not established by the reference PDF's availability.

There is no named minimum-spec performance result in the evidence yet. Global campaign balance across all country/modifier combinations, independent accessibility usability, and long-session strategic pressure require further measurement. This ledger deliberately separates code coverage from those human and hardware acceptance checks.
