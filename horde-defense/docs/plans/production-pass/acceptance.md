# Acceptance matrix: brief, user restatements and rulings, mapped to proof and wave

"Release" means the ignored `pacing_targets` run with `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored`, after the debug builds. Every row is also listed in docs/ACCEPTANCE.md by qa (W7-08).

## Goal 1: architecture and performance

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G1.1 | Frame report v3: render-world CPU, egui shape, vertex and quad counts, GPU time where supported | `v3_keeps_every_v2_key_and_adds_phases`, `egui_counts_split_textured_quads`, `gpu_timing_support_reports_missing_timestamp_features`, `cpu_frame_quantiles_are_per_sample_sums` (W1-33) | 1 |
| G1.2 | Normal play records nothing | `normal_play_installs_no_perf_probe`, `review_launch_without_report_installs_no_probe` (gate-inversion mutation in the commit message) | 1 |
| G1.3 | `review` default-feature note; the shipped build gets only a smoke timing | `report_records_build_profile_and_features`; `shipped_smoke_seconds` recorded (W1-61, W7-09) | 1, 7 |
| G1.4 | BattleScreen and BattleViewModel built once per egui pass | `battle_views_build_once_per_egui_pass` ({1,1,1}; 3 at wave 0), `npc_observation_builds_battle_screen_once_per_pass`, `discarded_pass_builds_once_per_pass` (W1-42) | 1 |
| G1.5 | Batched meshes, with per-category caps in PERF_BUDGET.json and a test | `perf_budget_json_matches_presentation_caps`, `battlefield_primitives_are_independent_of_enemy_count` (≤ 24 primitives since R22, 48 before; 400 vs 52 differ by ≤ 2), `particles_paint_as_one_untextured_mesh`, `corpse_draw_respects_caps_with_5000_records`; live in every perf run since W2-47 (R18): `measure_performance.ps1` gates the frame report's `egui.battlefield_primitives.max` and `egui.battlefield_vertices.max` per scene, worst repeat (`battlefield_counts_take_only_the_battlefield_clip`, `measure_performance_gates_the_battlefield_primitives_and_vertices`, `-SelfTest` fixtures `over-battlefield-primitives`, `over-battlefield-vertices` and `missing-battlefield-counts`) | 1, 2 |
| G1.6 | measure_performance: peak, progressed-peak, stress; `-Profile dev\|playtest`; `-VsyncOff`; CARGO_TARGET_DIR; budget exit code; the display held on and a run that overlaps a Kernel-Power 506/507 event invalid (R27); a quiet window before the measurements and a run with a CPU load sample of 90% or more invalid (R30) | `measure_performance.ps1 -SelfTest` gives 0/1/2 on the fixtures; the R27 cases of `scripts/tests/run_script_tests.ps1` (an overlapping run is invalid and rerun, a run without an event stays valid, the hold is released when a run throws); its R30 cases (a quiet start, waiting out a foreign build and a loaded CPU, a timeout exiting 2, a run invalid on a 90% sample and rerun); its R34 case (a valid gated over beside invalid groups still exits 2 but prints `VALID OVER: ...` and is listed in summary.json `valid_overs`; alone it exits 1, within budget the list is empty); `perf_scenes_agree_across_script_budget_and_review_matrix`; `repo_checks::powershell_scripts_are_51_safe` (W1-36) | 1 |
| G1.7 | Split battle_view, ui/battle, art and upgrade_graph; ARCHITECTURE.md is an ownership table | 4 pure-move commits (`git diff -M --color-moved`); `check_views.py check_architecture` (16 missing at wave 0; 143 modules each have a row at the W1 close) (W1-38..41, W1-62) | 1 |
| G1.8 | Save clone with 1,000+ items and 3 workers; remedy if p99 > 1 ms | `save_clone` example JSON (dev, playtest, release) in VALIDATION.md; conditional `consecutive_snapshots_share_unchanged_component` (W1-50) | 1 |
| G1.9 | -VsyncOff progressed-peak and stress: CPU (main + render-world) p99 ≤ 10 ms, 0 frames > 50 ms | PERF_BUDGET `cpu_frame_p99_ms_max 10.0` / `frames_over_50ms_max 0`; measure_performance exits 0 with a median of 3 (W1-61). Re-gated in W2-47, W5-11, W7-09. W2-47 (2026-11-10): dev and playtest `-Scene all -VsyncOff -Repeat 3` exit 0 on the wave-2 stress fixture, stress p99 dev 8.46 / 9.65 ms, playtest 7.68 / 7.17 ms (`PERF_BUDGET.json acceptance.w2_47`, `perf_budget_records_the_w2_47_regate`). Dev progressed-peak is not claimed clean at W2-47: the first dev invocation had a valid over there (12.7565 ms) and the pass was rerun (plan §2.5 "A valid over in a pass masked by invalid runs"; W5-11 re-gates it). R34 (2026-11-10): the over stands and may not be re-measured away; the wave-2 close perf acceptance at HEAD is a new measurement, and measure_performance names every valid gated over (`VALID OVER: ...`, summary.json `valid_overs`) whatever the exit code. | 1, 2, 5, 7 |
| G1.10 | Interface mean at 225 enemies ≥ 30% below wave 0 (same profile and scene) | progressed-peak interface_draw ≤ 1.129 ms (normal) and ≤ 1.030 ms (small) = 0.70 × 1.6128 / 1.4719 rounded to the microsecond (1.12896 / 1.03033; R19); `perf_budget_baseline_is_wave0` (W1-61, W5-09/11). The progressed-peak interface_total guard (`interface_total_not_above_baseline_v3`) is re-gated from W2-47 on by R31 (2026-11-09): an interleaved A/B against the wave-1-close executable (ab03da9) in one sitting, 5 pairs per size, B's median interface_total ≤ 1.05 × A's per size (`scripts/ab_interface_total.ps1`, with the R30 quiet window and load validity; its R31 cases in `scripts/tests/run_script_tests.ps1`: pass, fail and invalid-pair fixtures, and the same three end to end). `baseline.v3` is still re-measured and recorded (R29), but the A/B decides the gate. W2-47 (2026-11-10): the A/B passes at B / A 0.962 (normal) and 0.963 (small); interface_draw 1.026 / 0.992 ms | 1, 2, 5 |
| G1.11 | Pipelined-rendering decision recorded with numbers | `pipelined_default_matches_recorded_decision` (≥ 12 playtest runs) (W1-61) | 1 |
| G1.12 | Low latency: Mailbox, falling back to Immediate, with pipelined off | `present_mode_matrix`, `low_latency_disables_pipelined_unless_overridden`; the report shows `requested_mode Mailbox`, `pipelined_rendering false` | 1 |
| G1.13 | Ceiling of 225 enemies; the stress scene runs 400 | `production_validation_rejects_enemy_cap_225` (mutation: raise the cap); `stress_fixture_is_not_saveable`; `stress_fixture_reaches_400_enemies_within_1800_ticks`; `stress_screen_never_requests_a_save...`; `practice_flag_with_stress_screen_is_rejected` | 1 |
| G1.14 | Integrated-GPU run logged as pending | `perf_budget_integrated_gpu_is_pending_or_has_report`; ACCEPTANCE.md human-check row | 1, 7 |
| G1.15 | Settings vsync and window mode apply live; settings are read once at startup | `apply_display_settings_updates_window_live_but_not_pipelined`, `startup_settings_are_read_once_and_reused` | 1 |

## Goal 2: post-run flow, camp, navigation

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G2.1 | Results and Upgrades are two screens, each with exactly one amber primary | `primary_is_amber_and_registered`, `at_most_one_screen_primary_matrix`, `upgrade_opens_upgrades_screen_with_back_to_battle_and_back_to_camp` | 3 |
| G2.2 | Results has exactly three actions (Upgrade, continue_label, Leave journey) | `results_action_bar_has_exactly_three_buttons`, `liberation_results_action_bar_still_three_buttons`, `results_primary_table` | 3 |
| G2.3 | Camp home: roster with status, readiness and power (`defender_power` tested); offers, expeditions, alerts; "Go to briefing"; Operations and Mara are drill-downs | `camp_default_tab_is_home`, `roster_shows_core_power_and_status_labels`; core `fresh_starter_power_is_100...`, `defender_power_drops_as_threat_rises`, `defender_power_is_pure` (W2-21) | 2, 3 |
| G2.4 | No `{:?}` labels | `source_scan::no_debug_format_in_player_text`, `labels_are_human_over_all`, `readiness_summary_has_no_debug_names`, `recruit_status_labels_cover_all_variants` | 1, 2, 3 |
| G2.5 | Escape: topmost overlay, then Back through the chain to Menu with a Quit confirmation; construction per decision 18; full Screen x Tab x overlay matrix | `escape::tests::matrix_table`, `escape::tests::properties`, `escape_on_menu_opens_quit_confirmation`, `escape_in_construction_closes_without_pausing`, `pause_bound_to_escape_still_pauses_after_router_consumes` | 3 |
| G2.6 | One "Save" label and a "Saved" toast; Retry / Open folder modal; unapplied-draft warning; scale applies on release | `exactly_one_save_label`, `manual_save_shows_saved_toast_on_results`, `save_failure_opens_retry_modal`, `open_save_folder_uses_profile_directory`, `dirty_draft_escape_opens_warning`, `scale_applies_only_on_release` | 3 |
| G2.7 | Boss banner ≥ 1.5 s; low-HP edge warning | `boss_banner_visible_at_1_2s_and_gone_after_2s_wall_at_speed_4`, `boss_arrived_banner_lasts_1_5s`, `low_hp_edge_below_30_percent`, `low_hp_hysteresis_exits_above_35` | 3 |
| G2.8 | "Retreat & train" needs a hold or confirmation | `single_click_retreat_does_not_withdraw`, `held_retreat_withdraws_after_0_8s_and_opens_results` | 3 |
| G2.9 | HUD ≤ 2 rows at 1050x700@1.35 | `battle_bar_rows_measure_todays_bar` (characterisation, 3), then `battle_bar_stays_within_two_rows_at_small_size`; qa sidecar `hud_rows <= 2` | 3 |
| G2.10 | Construction mode no longer rescales the arena (overlay; R1) | `construction_toggle_keeps_arena_rect` (fails on old code by 252 pt), `construction_panel_never_covers_a_build_cell`, `construction_panel_keeps_plan_buttons`, `construction_panel_swallows_field_hover`, `occluded_spawn_cues_mark_lanes_hidden_by_the_panel`, `construction_frame_draws_incoming_chevron` | 1 |
| G2.11 | Onboarding: one-time "New: X - find it in <tab>"; stage count and Mara timing at country choice; expedition notices; core SelectCountry | `unlock_callout_shows_once_and_survives_restart`, `none_is_seeded_with_every_unlocked_callout_at_profile_load`, `country_dossier_names_defense_count_and_mara_timing`, `one_toast_per_unacknowledged_outcome_and_ack_dispatched_same_frame`, `select_country_never_creates_an_attempt` | 2, 3 |
| G2.12 | Headless full loop: new game, Rook, country, defeat, Results, Upgrade, buy, Back to battle, victory, Next, liberation, Recruit, Back to camp, the recruit defends; each step asserts Screen, Tab and one primary | `ui/tests/loop_tests.rs::full_loop_new_game_to_recruit_defends` at Harness::large and ::small (W3-28) | 3 |

## Goal 3: defenders and campaign

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G3.1 | `available_defenders()` and `SetNextDefender`; picker on Results and Briefing | `available_defenders_lists_player_mara_recruits_with_blockers`, `picker_lists_ready_recruit_and_dispatches_set_next_defender`, `results_picker_persists_choice_across_save_load` | 2, 3 |
| G3.2 | Persisting the defender choice | `next_defender_persists_through_save_load_and_continue_deploys_it` | 2 |
| G3.3 | Rejecting a reserved or guarding recruit | `set_next_defender_typed_rejections_leave_state_unchanged`, `recruit_on_main_run_cannot_be_dispatched_trained_or_equipped`, `defending_recruit_rejects_every_ready_gated_mutation` | 1, 2 |
| G3.4 | `continue_journey` reserves the defender's loadout like deploy_survivor | `recruit_takeover_deploys_with_recruit_loadout_and_identity`, `staged_recruit_defending_main_run_validates`, `takeover_does_not_reserve_maras_idle_loadout`; on the v6 fixture `v6_fixture_recruit_can_take_over_the_next_defense` (W2-48) | 1, 2 |
| G3.5 | Idempotent `settle_parallel` | `settle_parallel_twice_is_a_no_op`, `random_tick_recall_interleavings_settle_each_battle_once` (200 cases), `repeated_settle_parallel_is_idempotent` | 2 |
| G3.6 | SelectCountry never creates an attempt; outbreaks get "Plan response" | `select_country_never_creates_an_attempt`, `outbreak_plan_targets_final_stage...`, `plan_response_opens_briefing_without_attempt` | 2, 3 |
| G3.7 | Recruit progression reaches 10 levels by the 6th liberation | `allowance_reaches_level_ten_by_sixth_liberation` (84), `allowance_never_below_legacy_and_monotonic` | 2 |
| G3.8 | Worker wins credit their recruit_id; service history; origin trait | `worker_victory_still_credits_the_worker_npc_copy`, `worker_victory_credits_recruit_and_threat_once` (W2-15), `service_record_is_derived_from_run_history_and_containments`, `origin_traits_cover_every_world_biome_without_fallback` | 1, 2 |
| G3.9 | Exactly one notice per expedition outcome; worker liberations get "X liberated · N new fronts" | `each_settled_expedition_yields_exactly_one_notice_across_save_reload`, `country_operation_notice_appears_once_after_final_stage_with_liberation_and_fronts`, `pruning_folds_unacknowledged_outcomes_and_conserves_the_count`, `v6_settled_battles_produce_no_notice_burst` | 2 |
| G3.10 | Strategy profile, 20 seeds: liberations 2/4/10 and coins per minute reported | `strategy_profile_reports_liberations_2_4_10_and_coins_per_minute` (release) and the plan table | 4 |
| G3.11 | Parallel frees 3-stage neighbours in ≤ 1/1.5 of the solo runs | `parallel_frees_three_stage_neighbours_within_two_thirds_of_solo_runs` (release) | 4 |
| G3.12 | < 5 outbreak reopens over 10 liberations | `fewer_than_five_outbreak_reopens_over_ten_liberations` (release) | 4 |
| G3.13 | Decision 4: the winning defender gets XP/CP; Personal first_clear XP unchanged | `takeover_victory_credits_recruit_not_mara`, `mara_finish_matches_legacy_attempt_counting` | 2 |
| G3.14 | Decision 5: liberation Results shows "Recruit from <Country>" or names the node | `liberation_results_offers_recruit_when_node_owned`, `liberation_results_names_missing_survivor_network`, `recruit_offer_names_the_missing_recruitment_node` | 2, 3 |
| G3.15 | Decision 15: two archetypes plus an origin trait | `recruit_offer_has_two_distinct_archetypes_and_recruit_as_applies_the_choice`, `recruiting_draws_no_strategy_rng` | 2 |
| G3.16 | R2: an absent `defender` key means the mode owner; GOLDEN does not move for it; takeover stamps the recruit's appearance | `golden_and_default_games_serialize_no_defender_key`, `npc_report_without_key_reads_mara_personal_reads_player`, `expedition_attempt_carries_recruit_seat_and_appearance`, `battlefield_draws_profile_appearance_not_forced_female`, `continue_journey_after_migrated_companion_report_redeploys_companion` (W2-48) | 1, 2, 3 |

## Goal 4: loot, items, auto-sell

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G4.1 | `Game::sale_eligibility` is used by SellItem, sell_extras, quotes and auto-sell, and excludes protected, locked, favourite, reserved, equipped, preset, recruit-equipped and npc_only items (R11) | `every_sell_path_agrees_with_sale_eligibility`, `strict_sell_rejects_equipped_preset_reserved_recruit_and_npc_only`, `sale_eligibility_is_pure`, `properties::sale_paths_never_sell_guarded_items` (500) | 1, 2 |
| G4.2 | A test shows the old filter selling a recruit-equipped item | `old_campaign_filter_sells_recruit_equipped_item` (W1-18), inverted to `strict_sell_rejects_recruit_equipped_item` (W1-20) | 1 |
| G4.3 | `AutoSellRule {..}` plus `Command::SetAutoSell` | `set_auto_sell_rejects_poor_threshold_with_auto_sell_rule_invalid`, `command_serde_round_trip_covers_appended_economy_variants`, `commands_round_trip_through_serde_for_replay` (all 9 appended variants, W2-48) | 1, 2 |
| G4.4 | Runs after reservation release (main) and after the worker remap; no RNG; checked wallet; each item sold at most once | `takeover_auto_sell_runs_after_release...`, `auto_sell_consumes_no_rng`, `wallet_overflow_keeps_items`, `worker_loot_sold_exactly_once_in_main_campaign`, `save_mid_expedition_preserves_pending_auto_sell_and_sells_once`, `queued_drop_reserved_by_another_deployment_keeps_the_save_valid`, `final_sweep_sells_a_queue_no_record_drained` | 2 |
| G4.5 | `tests/auto_sell.rs` plus a property test: no guarded item is sold; the quote equals the coins realised | `tests/auto_sell.rs` suite; `properties::sale_paths_never_sell_guarded_items` | 2 |
| G4.6 | Rule off: nothing moves | `rule_off_moves_nothing` | 2 |
| G4.7 | Parallel runs sell each item exactly once | `worker_loot_sold_exactly_once_in_main_campaign`, `chained_operation_gets_one_receipt_per_stage_with_count_le_loot` | 2 |
| G4.8 | Old saves load with the rule off | `v6_save_loads_with_auto_sell_off_and_empty_drop_tracking`, `v6_inflight_attempt_and_expedition_sell_nothing_after_enabling_rule` | 1, 2 |
| G4.9 | Bounded receipt; RunRecord count and coins; sale status only on Results | `receipt_validation_rejects_13_lines_and_bad_quality_sum`, `fifty_sales_one_receipt_12_lines_exact_totals`, `battle_and_observer_never_render_sale_text`, `flyup_never_shows_sale_status` (W5-08) | 1, 2, 3, 5 |
| G4.10 | UI: Auto-sell panel with colour and text markers and a "would have sold N" preview; Results loot block with Equip and "Auto-sold N · +C" | `equipment_autosell_panel_preview_counts_match_core`, `threshold_chips_show_text_and_shape_markers`, `results_shows_drops_by_quality_and_auto_sold_line`, `results_better_than_equipped_row_dispatches_equip_command`; captures `equipment-autosell`, `results-*` | 3 |
| G4.11 | "Apply rule to stash now" asks for confirmation | `apply_to_stash_requires_confirmation_and_escape_closes`, `apply_auto_sell_to_stash_rejects_a_stale_quote_without_mutation` | 1, 3 |
| G4.12 | 39 append-only items: every tier 1-4 has 1 Ranged per class and ≥ 3 per other slot; drops weighted by tier and trained class | `every_tier_1_to_4_has_one_ranged_per_class_and_three_per_other_slot`, `appended_ids_keep_v1_order`, `weighted_drops_favour_current_tier`, `trained_class_weighted` | 2 |
| G4.13 | The auto-sell profile stays within 1 run of baseline at tiers 1-3 | `auto_sell_profile_within_one_run_of_default_at_tiers_1_to_3` (release) | 4 |
| G4.14 | Decision 7: relative quality; existing items keep stats and value | `relative_roll_never_drops_a_tier4_below_epic`, `v6_items_keep_stats_and_value` | 1, 2 |
| G4.15 | R4: legacy attempts keep the old drop path | `V6_DROP_TRACE_FNV` pin, `legacy_attempt_never_pushes_drop_ids_and_is_never_auto_sold`; their frozen snapshots roll no elite and phase no boss: `legacy_attempt_snapshots_have_zero_elite_rate_and_no_boss_phases`, `v6_active_attempt_spawns_no_elites_for_3600_ticks`, `v6_legacy_boss_has_no_phases` (W2-48) | 1, 2 |

## Goal 5: progression, tree, skills

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G5.1 | Lower Steady hands and Hardened; raise Trap mastery 1 and Quick throw | `steady_hands_..._at_most_40_percent_per_shotgun_trigger`, `hardened_leaves_runner_contact_damaging`, `trap_mastery_1_adds_a_trap_charge`, `quick_throw_adds_throws_per_deployment` | 4 |
| G5.2 | Milestones at ranks 5/10/15/20 and the level-up coin grant (decision 14) | `level_grant_is_paid_once_at_root_finish`, `milestone_paid_once_per_rank_across_save_reload`, `worker_victory_level_up_pays_the_grant_once_in_the_parent...`, `v6_save_migrates_level_grant_paid_to_level_and_claims_reached_milestones`; on the v6 fixture `worker_level_up_does_not_pay_the_level_grant_twice` and `migrated_threat_jump_pays_no_milestone` (W2-48); the shipped values: `shipped_catalogue_authors_the_design_grant_and_milestones` (W2-54) | 1, 2 |
| G5.3 | Up to 8 new append-only nodes (user: yes, ≤ 8) | `skill_catalog_is_append_only_with_eight_weapon_mastery_nodes` (len 177, FNV of the first 169), `counters_name_catalog_archetypes` | 2 |
| G5.4 | Remove the disabled IQ/HP/FP rows | `npc_trainable_rows_exclude_iq_hp_fp`, `training_lists_have_no_iq_hp_fp_text` | 2, 3 |
| G5.5 | Mara training and recruit cost scale with threat; the UI reads core prices (R10) | `recruit_and_training_prices_scale_with_threat`, `camp_price_labels_follow_core_price_at_rank_10`, `ui_has_no_literal_camp_prices_or_mara_mapping` | 2, 3 |
| G5.6 | Pacing profiles: equip-drops, auto-sell, takeover, farm, Easy/Hard, 150-run; per-archetype telemetry; `--csv` | `pacing_registry_ids_are_unique...`, `equip_drops_profile_wears_dropped_upgrades`, `auto_sell_profile_records_auto_sold_coins`, `takeover_profile_records_first_takeover`, `combat_telemetry_counts_spawns_kills_and_damage_taken_by_archetype`, `pacing_csv_has_header...` | 2 |
| G5.7 | First victory on run 2-4 (expert-deu, Normal, 20 seeds, median) | `goal5_first_victory_run_2_to_4` (release) | 4 |
| G5.8 | Tiers 1-3 affordable at about runs 4/8/12 | `goal5_tiers_affordable...` (release); `tier_costs_sit_in_design_bands` | 4 |
| G5.9 | The tree lasts past the 2nd liberation | `goal5_tree_outlasts_second_liberation` (release; median(TreeExhausted) > median(Liberation(2)) and ≥ 1 node bought after it) | 4 |
| G5.10 | Unlock order Fortifications, then Mara, then Operations (OperationsOpen = `operations_available()`) | `goal5_unlock_order_fortifications_mara_operations` (release); synthetic `unlock_order_rejects_*` | 2, 4 |
| G5.11 | No dominated node; no node under 100 coins worth > 2x its branch's value per coin | `no_valued_node_is_dominated_at_its_reference_states`, `cheap_nodes_stay_within_twice_branch_value_per_coin` | 4 |

## Goal 6: enemies, scaling, difficulty

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G6.1 | Two-axis threat; floors past rank 4; `wave_tier` bound lifted; 8-12 composition tiers | `snapshot_wave_tiers_at_10_20_40_are_7_10_11`, `catalogue_has_twelve_tiers...`, `progression_snapshot_accepts_wave_tier_up_to_max_wave_tier` / `_rejects_above_max`, `every_floor_tier_exists_in_enemy_catalog` | 1, 2 |
| G6.2 | 6-8 archetypes with serde defaults, bounds and an NPC resolution | `resolved_enemy_validate_rejects_*`, `legacy_resolved_enemy_and_snapshot_serialize_byte_identically`, `npc_resolution_for_each_new_archetype`, the per-archetype NPC tests in W2-29/30 | 1, 2 |
| G6.3 | Elite affix table and elite fields; elite_rate 0 below rank 3, 2% at 3, 15% at 23; elites drop Uncommon+ | `elite_bp_pins`, `elite_fraction_at_rank_13_is_8_5_pm_0_5_over_20k`, `every_elite_kill_drops_uncommon_or_better`, `elite_status_never_changes_combat_rng_draw_count`, `elite_roll_consumes_no_combat_rng` and `rank_zero_deploy_spawns_no_elites_and_has_no_new_archetypes` (W2-48) | 2 |
| G6.4 | Boss armor and damage scale with threat; one phase per BossKind; capstone boss | `rank_20_boss_armor_exceeds_rank_0_both_modes`, `boss_damage_multiplier_follows_rank`, `boss_crossing_50_percent_enters_phase_once...`, `capstone_has_one_extra_transition...` | 2, 4 |
| G6.5 | Hard raises the elite rate, ramp and boss phases; the preset applies to the next deployment | `hard_elite_rate_1_5x_normal_easy_0_5x`, `front_ramp_orders_hard_over_normal_over_easy`, `hard_has_two_above_rank_4`, `custom_edit_from_hard_keeps_hard_extras` | 1, 2 |
| G6.6 | Short-range spitter, never attacking from the far half | `spitter_never_winds_up_or_damages_from_the_far_half` (widths 8/12/20/30) | 2 |
| G6.7 | Country difficulty about 0.85-1.6, via the generator, never hand-edited | `world_difficulty_matches_band_formula`; `generate_world.py --check` exits 0; `--self-test` | 2 |
| G6.8 | Regional checkpoint every 6 stages (max 3), never saved per stage | `boss_rank_derivation_usa_24`, `regional_checkpoint_distribution_is_219_12_4_7`, `checkpoints_add_no_saved_field` (W2 review: a USA save deployed on stages 1-23 adds only the attempt-scoped `boss_rank` off Sector and the Capstone's `boss_phases`), `stage_boss_rank_reads_the_front_position_purely` | 2 |
| G6.9 | Victory to threat guard (test, not re-implemented) | `tests/threat_guard.rs` plus a proptest (saturating_add mutation recorded) | 2 |
| G6.10 | `tests/pacing_targets.rs` is #[ignore] and documented in AGENTS.md | file present; AGENTS.md section (W2-03, W4-11) | 2, 4 |
| G6.11 | Defeat rate 10-25% in every 20-run window after the first victory (Normal, expert-deu, 20 seeds) | `goal6_defeat_rate_windows` (release) | 4 |
| G6.12 | No 15-win streak after run 10 | `goal6_no_15_streak_after_run_10` (release) | 4 |
| G6.13 | Final stage ≥ 1.3x the first in EHP x arrival | `front_ramp_final_over_first_normal_is_at_least_1_35` (core, equal rank) | 2, 4 |
| G6.14 | Wave tiers at ranks 10/20/40 differ | `snapshot_wave_tiers_at_10_20_40_are_7_10_11` | 2 |
| G6.15 | LUX: Easy ≤ 6 runs; Hard ≤ 1.5x Normal | `goal6_lux_easy_at_most_6_runs`, `goal6_lux_hard_at_most_1_5x_normal` (release) (W2 review, 6f20cf2: a censored Hard median, whose median seed never liberates in 20 runs, fails as unmeasured instead of counting as 21, since 21 only bounds it from below; a censored Normal median against a measured Hard one passes; design-enemy-balance W-4 as-built note) | 4 |
| G6.16 | Every archetype spawns and is killed | `goal6_archetype_coverage` (release, telemetry) | 4 |
| G6.17 | Boss time-to-kill 20-60 s | `goal6_boss_ttk_median_20_60_s` (release) | 4 |
| G6.18 | Clamped starts get their first victory on run 2-4 | `goal6_clamped_starts_first_victory_runs_2_to_4` | 4 |

## Goal 7: graphics

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G7.1 | weapon_fx for 6 classes plus turret are distinct; capture and logged human review | `every_class_and_turret_profile_distinct_on_two_axes...`; `weapon-classes` capture; human review logged (pending allowed) | 5 |
| G7.2 | Night luminance ≥ 30% below dawn; cue contrast ≥ 3:1 | `night_grade_at_most_0_70_of_dawn...`; `measure_field_luminance.py` ratio ≤ 0.70; `every_cue_primitive_has_an_ink_backing_with_contrast_at_least_3` | 5 |
| G7.3 | Lightning: ≤ 1 pulse per 4 s wall time at any speed, ΔL < 0.2, never masks cues; reduced mode uses a static storm grade | `sky_flash_starts_at_least_4000_ms_apart_at_observation_speed_4x`, `sky_flash_peak_delta_below_0_2...`, `sky_flash_paints_under_cue_pass`, `reduced_effects_storm_stage_draws_static_storm_grade_and_no_flash` | 5 |
| G7.4 | A MOTION sequence shows no flashes in reduced mode | `capture_motion -Scene storm -ReducedEffects` (all flash fields 0); `reduced_96_frame_fx_stress_sequence_has_no_additive_vertex...` | 5 |
| G7.5 | fx-stress within budget | measure_performance fx-stress exits 0; `fx_stress_shape_counts_within_perf_budget` | 5 |
| G7.6 | Elite tells via enemy_visuals v2, with v1 still loading; quality glint | `v2_with_default_elite_tell_parses`, `v1_fixture_loads_and_gets_builtin_elite_tell` (W1-52), `elite_enemy_paints_ring_and_marker...`, `rare_or_better_flyup_paints_one_glint_common_none` | 1, 5 |
| G7.7 | Provenance test over `assets/art/**` and `assets/player_models/**` | `asset_audit::shipped_art_has_provenance_with_matching_sha256`, `shipped_art_is_referenced_at_runtime`, `player_model_png_sha256_matches_file` | 1 |
| G7.8 | Grenade and Molotov differ; multi-frame explosions; damage-scaled hits; decal cap; blood opt-in | `grenade_and_molotov_explosions_paint_distinct_shape_sets`, `explosion_has_at_least_4_distinct_frames...`, `hit_spark_length_monotonic...`, `blood_decals_zero_when_off_newest_64...` | 5 |
| G7.9 | Camera uniform zoom-to-fit (R1); 17x30 and other filling fields stay pixel-identical; the observer uses the same fit | `fit_field_zoom_table_at_both_target_sizes`, `arena_fits_full_lanes_and_width_on_the_limiting_axis`, `fit_field_keeps_every_filling_geometry_bit_identical`, `fresh_three_lane_battle_fills_the_arena_width_at_small_size`, `observer_uses_fit_field`; max-lanes sidecar equals the old arena rect | 1 |

## Goal 8: audio (user: new gun/enemy/UI audio YES; no new music files; tracks Deferred)

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G8.1 | Coverage over SimEvent, catalog archetype ids and BossKind | `sim_event_mapping_is_exhaustive` (no wildcard), `every_catalog_archetype_has_explicit_voice_row_with_existing_files`, `every_boss_kind_has_arrival_phase_death_banks...` | 6 |
| G8.2 | No repeat in 1,000 triggers; ≥ 5 variants for frequent cues | `round_robin_never_repeats_in_1000_triggers`, `frequent_cues_have_at_least_five_variants` | 6 |
| G8.3 | 50 sales produce 1 cue (R9: one batch of 50 gives 1 cue) | `fifty_auto_sold_items_one_coin_batch_on_results_none_on_rerender`, `sell_extras_of_50_plays_one_coin_batch`, `one_stash_apply_of_50_items_returns_one_sold_outcome` | 1, 6 |
| G8.4 | No orphan WAVs; every sha256 matches provenance | `asset_audit::every_shipped_wav_is_loaded_and_matches_provenance` (75 after W1-58), `archived_files_keep_matching_provenance`, `survival_generator_sha256_matches_lf_normalised_sources` | 1, 6 |
| G8.5 | **User override:** a 60 s mix at default sliders reads -23 ±2 LUFS integrated with true peak ≤ 0.95 FS (replaces the brief's -16..-14) | `reference_mixed_60s_meets_loudness_gate` (both themes), `dense_battle_60s_true_peak_at_most_0_95`; BEFORE/AFTER table in the plan; the gate is recorded in ACCEPTANCE.md, AUDIO_SURVIVAL.md and the plan (W7-03) | 6, 7 |
| G8.6 | R3: offline per-clip true-peak ceiling allowed; runtime limiter only if needed | `every_rust_family_masters...tp...`; the plan records whether the runtime limiter was needed | 6 |
| G8.7 | MusicPlugin = director + Music bus + stingers from existing banks; tracks Deferred; slider "Music & stingers" | `music_director_matrix`, `every_stinger_resolves_to_existing_bank`, `music_volume_scales_stingers_only`, `settings_show_music_and_stingers_and_interface_sounds_sliders` | 3, 6 |
| G8.8 | Buses, ducking, voice stealing, lane pan (mono-compatible); gain tables out of audio.rs | `each_bus_slider_changes_only_its_voices`, `boss_arrival_ducks_atmosphere_6db...`, `steal_prefers_oldest...`, `pan_is_mono_compatible...`, `horde_game_audio_sources_define_no_gain_tables` | 6 |
| G8.9 | Gun audio v2 (dry fire, staged reloads); NPCs use their own class | `npc_defender_shot_uses_weapon_class_cue` (W1-58), `manual_reload_plays_out_in_finish_once`, `dry_trigger_auto_reload_off_plays_dry_fire_once_per_350ms` | 1, 6 |
| G8.10 | Decision 17: timbre offset per quality tier, no hover sound, loot-reveal sound | `takeover_recruit_rare_shotgun_gets_rare_tone`, `no_widget_hover_writes_a_sound_request` (mutation proof), `loot_reveal_follows_quality_speed...` | 6 |
| G8.11 | Every audio warning has a visual twin | `every_warning_row_names_a_visual_twin`, `warning_cue_table_has_a_visual_twin`, `every_windup_event_has_a_windup_view_in_the_same_frame`, `boss_phase_hold_shows_phase_label_and_ring_for_90_ticks` | 2, 6 |
| G8.12 | Listening pass logged as pending | PLAYTEST_GUIDE "Listening pass: PENDING" with fields; ACCEPTANCE human-check row | 7 |

## Goal 9: release

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G9.1 | Version and commit stamp in the manifest and on the menu | `stamp_formats_dev_and_commit_builds`, menu fit test at 1050x700@1.35, manifest v2 (W7-01/02) | 7 |
| G9.2 | Curated docs only | zip contains docs = {PLAYTEST_GUIDE.md} ≤ 65,536 B; no `archive/`, `docs/audio` or source-only files | 7 |
| G9.3 | Packaged `--smoke` and `verify_playtest.ps1` pass under 5.1 | `powershell -File scripts/verify_playtest.ps1` PASS; smoke `^PASS:` | 7 |

## Goal 10: polish

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| G10.1 | Final full-loop capture review at both sizes, normal and reduced; every finding has an owner and is closed by an after-capture or a user-approved deferral | `check_capture_findings.py --final` exits 0; `docs/screenshots/production/` with captures-before/after.json (W7-11) | 7 |

## Decisions, isolation rules and user restatements

| AC | Criterion | Proof | Wave |
|---|---|---|---|
| D8 | SAVE_VERSION 7 (user yes); v1-v6 load; `save_v6.json` / `save_v7.json` | `decode_rejects_version_8`; the v1..v7 load tests; `migrated_v6_fixture_resaves_as_current_version_and_resumes_byte_identically`; `v7_fixture_loads_validates_and_resaves_byte_identically`; `v7_fixture_holds_every_defender_key_at_non_default` | 1 |
| D10 | Archive (user list): 12 WAVs, 6 root sheets, 5 strips, pixel/keyart-v1, survival_npc_shot_0..2 | `asset_audit::archived_files_keep_matching_provenance` (24, then 27), `archive_paths_mirror_archived_from`, `credits_never_lists_an_archived_path_as_shipped`; the keyart pin is retargeted | 1 |
| D9 | Provenance (generator, seed, sha256) and an `assets/CREDITS.md` line per file | `credits_index_matches_provenance`, `credits_index_lists_every_shipped_asset_exactly_once` | 1, 6 |
| D18 | Escape: Menu gives Quit; construction closes; Pause default kept; Camp home owns the clock; extra bindings, markers, palette, window mode, vsync, Music and UI volumes | `escape::tests::matrix_table`, `clock_controls_only_on_home`, `old_13_key_settings_file_loads_with_new_defaults`, `one_bad_key_falls_back_alone`, `rebind_conflict_spans_core_and_extra_tables` | 1, 3 |
| ISO-1 | Root files untouched (R6) | Review step: `git diff --name-only main... \| grep -v ^horde-defense/` is empty; the `pwsh -NoProfile -File ../scripts/dev.ps1 verify -Project horde-defense` line (README.md:166 and VALIDATION.md:147 at the wave-1 review; README.md:165 at 3d492cd) is byte-identical to 3d492cd, once per file and unannotated, pinned by `repo_checks::root_helper_line_is_unannotated` | every wave |
| ISO-2 | PowerShell 5.1 only; Start-Process for native executables, or (R24) `Invoke-Native`'s `[Diagnostics.Process]::Start` with a `ProcessStartInfo`, UseShellExecute off and stdout and stderr redirected to the logs; never `>`; taskkill /T /F on timeout | `run_script_tests.ps1` (its late-exit case guards against a return to `Start-Process -PassThru`, R24), `powershell_scripts_are_51_safe`, `docs_invoke_scripts_with_windows_powershell`, `pending_list_only_names_existing_unported_scripts` | 1, 7 |
| ISO-3 | Ids never change; content is append-only | `appended_ids_keep_v1_order`, `skill_catalog_is_append_only...`, `waves_0_to_4_byte_identical`, `legacy_catalogue_records_that_tick_saved_encounters_are_frozen` (W2-53), `stage_theme_ids_are_pinned_to_the_10_in_order` | 1, 2, 5 |
| ISO-4 | Old settings files load; `bindings: [String;8]` unchanged | `old_13_key_settings_file_loads_with_new_defaults` (`settings_v0.json`), `absent_version_key_reads_zero_not_current` | 1 |
| ISO-5 | Rules change only through `Game::apply` with typed errors; no widget IO | `defender_blockers_have_plain_display`, `strategy_bot_acts_only_through_commands`, `production_fixtures_stage_features_through_commands`, `open_save_folder_uses_profile_directory` (runner only) | 1-3 |
| ISO-6 | `--smoke` and GOLDEN keep passing; every move has a reason and a deliberate re-bless | `determinism.rs` with the provisional ledger; re-bless commits W1-69, W2-49 and W4-08 with per-digest reason tables; W1: GOLDEN 1/6, TRACE 3/6 from the W1-69 re-bless (user decision 2026-11-06); W2: GOLDEN 6/6 (net F; causes W2-05 T, W2-06 F, W2-08 C, W2-52 F, W2-09 G, W2-54 F), TRACE 0/6, from the W2-49 re-bless, RULESET 3; 0 moves recorded for W3, W5, W6, W7 | all |
| ISO-7 | Balance changes have pacing and smoke before and after | the plan tables per W2/W4 commit; `target/production-after-wave{2,4}/` | 2, 4 |
| ISO-8 | Presentation never touches core RNG | `presentation_never_reads_core_rng` (self-test on a planted string; forbids `hash::keyed`), `sky_flash_never_reads_core_rng`, `shot_fx_never_mutates_attempt` (W5-03), `horde_audio_never_reads_core_rng` | 1, 5, 6 |
| ISO-9 | No screen shake | `field_rect_never_offsets_on_damage_or_explosion` (W5-06) | 5 |
| ISO-10 | Reduced effects covers every new effect | reduced MOTION gates (Δmean L < 0.01, block ΔL < 0.1); the per-feature `reduced_*` tests (toast, banner, low-HP, retreat hold, chevron static, shots, lights, explosions, decals, elite pulse, fly-ups, lightning) | 3, 5 |
| ISO-11 | Full verify every wave | fmt; clippy -D warnings ×3; `cargo test --locked --workspace`, `-p horde-defense --no-default-features` and `--features dev`; `--smoke`; `check_views.py`; release `pacing_targets` | all |
| R7 | One GOLDEN/TRACE re-bless per wave in a dedicated commit with reasons | W1-69 (user decision), W2-49 and W4-08; PROVISIONAL None at every wave close (critic check); None again since W2-49 | 1, 2, 4 |
| R8 | Stress cap only behind a review/test seam | `production_validation_rejects_enemy_cap_225`; a `--no-default-features` build compiles without the seam | 1 |
| HUMAN | Human checks: weapon-classes and night/storm review; 3x12 at 2.5x zoom feel; sheet upscale on 3-lane sprites; click-to-photon; listening pass; integrated-GPU run; stepped damage pop (CF-1-04, W1-64); whole-pixel text placement (CF-1-05, W1-64) | Logged as "pending human check" in ACCEPTANCE.md and the plan if not done; they do not block | 1, 5, 6, 7 |
