# Campaign, equipment and strategic review

This pass reviewed the supplied GDD, the later country/strategy expansion and all 34 acceptance criteria against the actual game state and procedures. It concentrates on campaign and authoring integrity; battlefield ordering, rules procedures, rendering and human comprehension are separate review tracks.

## Material findings and changes

| Finding | Resulting behavior | Regression evidence |
|---|---|---|
| An optional item sold while assigned to an idle recruit left a dangling loadout and a campaign that could fail to resume. | Game-level previews name affected survivors and presets. A deliberate sale restores the recruit's real protected starter item; no item is duplicated. Batch sale preserves every survivor's equipped gear. | `idle_recruit_sales_warn_and_restore_the_original_owned_starter_set` plus game-level sale tests. |
| Runtime content could omit protected starter definitions, reorder index-addressed profiles, overflow spawn weights, or contain unsupported numerical values. | Validation rejects unsafe content before play, with profile-specific reasons. The five encounter templates and canonical enemy/structure tables remain explicit. | `authoring_rejects_missing_starters_reordered_profiles_and_overflow_weights`. |
| A custom map could be empty or disconnected, leaving arbitrary starting countries in permanent dead ends. | A validated catalogue is nonempty and connected through explicit symmetric land/sea routes. Centers, theme references and bounded local stage counts are checked. | `custom_catalogues_reject_empty_disconnected_and_invalid_theme_data` and existing complete-graph reachability tests. |
| Local stage presentation exposed only the repeating encounter index, biome and a broad regional string. | Ten configurable location records supply stable title, setting, weather, lighting, props and variation tags. Per-country sequences may override the default rotation. Combat balance is unchanged. | `local_dressing_is_stable_varied_and_uses_configured_tags`. |
| Camp had no structured next-improvement or complete mode-aware comparison model. | A deterministic recommendation references actual coins, owned equipment, unspent points and unlocks. Comparisons preview the resulting full loadout, with a personal Shambler breakpoint and explicitly staged NPC damage explanation. | `recommendations_are_attainable_and_comparison_reports_mode_specific_breakpoints`. |
| Every recruit copied the same starting attributes and policy; redistribution had no separate creation record. | Four original 76-point backgrounds have distinct attributes, skills and policies. Each creation budget is validated. Redistribution restores that individual's baseline; older Mara records retain the original default exactly. | Equal-budget/respec, individual-background and legacy-deserialization tests. |
| Guards recorded objective progress but could wait until a paid-training interaction to claim it. | Eligible idle/guard objectives are claimed on strategic updates, including pending awards when capacity opens. Deployed survivors retain battle-owned ledgers until settlement. | Existing finite-objective and allowance tests plus individual training regressions. |
| Saved campaign metadata did not fully enforce development caps, blueprint ranks, profile ordering, layouts or preset slot references. | Invalid ledgers fail before simulation. Legitimately incomplete presets remain incomplete rather than duplicating sold items. Guard injury, ammunition, reservations and alert identities are also validated. | Malformed development, strategic-state and transaction conservation tests. |
| Repeated linear item lookup made large-armory comparison unnecessarily costly. | Owned items keep stable increasing creation IDs and use binary lookup. There is still no gameplay stash capacity limit. | Existing 1,000-item retention and identity tests. |

The NPC Strength preview and actual profile now use the verified Lite swing-table delta. A worked regression checks ST 10 to 11: HP and melee force improve while bullet damage remains identical. Guardian garrisons use their own melee skill and weapon without fabricating ranged-objective hits.

## Acceptance criteria cross-check

| Criteria | Campaign review conclusion |
|---|---|
| AC01–05 | Fixed geometry, preserved lanes and contact timing are simulation/rendering responsibilities. No progression change moves the defender, switches enemy lanes or grants a free contact action. |
| AC06–10 | Rewards remain simulation-ledger transactions, banked when earned. Campaign helpers neither pay on results reopening nor recreate encounter state. First-clear milestones remain separate from repeat rewards. Mode-specific gate tests remain necessary after balance changes. |
| AC11–16 | Preparation and supported NPC profiles are retained. The new comparison explicitly distinguishes personal geometry, NPC basic damage and later resolution stages. Recommendations and recruit policies inspect current state only. Rules compliance remains limited to the declared subset and adapters. |
| AC17 | Creation and advancement are now separately represented and validated for each survivor. Historical awarded points survive redistribution and include unspent points in the allowance. |
| AC18 | Unique identity, real loadout reservation, sale warnings, protected fallback and preservation of all recruit equipment in bulk sales cover the previously missing idle-recruit path. |
| AC19–22 | Blueprint ownership/ranks and saved grid cells are checked; actual blocking, placement spending, salvage and complete tactical pause remain simulation responsibilities. New scene tags do not alter these rules. |
| AC23–24 | Campaign, world and strategy validation now reject more malformed state without arithmetic overflow. Save/resume and observation-speed equivalence still require the simulation's complete-state regression tests. No wall-clock reward or strategic pressure is introduced. |
| AC25–26 | Protected equipment remains genuinely owned and unsellable, including independent recruit sets. Guaranteed quartermaster purchases remain available before tier gates. Repeat/country operations perform no implicit shop or training transactions. |
| AC27–29 | NPC comparisons name the resolution stage rather than calling a skill probability an injury probability. Theme and recommendation APIs support clear UI explanations. Minimum-window readability and human gate viability cannot be established by these APIs alone. |
| AC30–32 | The campaign does not redefine unconsciousness, stun, knockdown or personal melee exclusivity. Guard injury legality reuses the same validated injury-state rules. |
| AC33–34 | Unlimited loot retention and strictly unique item IDs remain intact. Boss and completion payments stay owned by the authoritative combat transaction; camp suggestions and sale previews are read-only. |

## Playtest paths

1. Begin with a fresh campaign and choose a country. Finish or recall an ordinary fight; if it ends in defeat, verify retained rewards. Check that the suggested affordable improvement matches the wallet and selected mode. No intentional death is required.
2. Compare a found rifle in personal and NPC modes. Check that the personal body-hit breakpoint is labeled and NPC basic-damage numbers are not described as a final hit or injury chance.
3. Recruit two survivors after liberation. Inspect different creation backgrounds, equip optional loot, preview a sale, then sell it. Confirm the named recruit returns to their original protected item and a save/resume remains valid.
4. Start simultaneous deployments using different unique gear, then try to sell or assign that gear again. Confirm the rejection names the reservation and no coins or items change.
5. Assign a trained guard, observe a plague alert, pause and resume, and resolve it through a real defense or garrison. Review injury/fatigue and finite objective awards. Let another alert expire and confirm only its final stage reopens, without revoking earned travel routes.
6. Use a copied world configuration to select a custom stage-theme sequence. Verify stable scene tags after retry and resume. Try a missing theme or disconnected island and confirm the override fails explicitly before play.

## Verification and limits

The final coordinated integrated command `cargo test --locked -p horde_core --target-dir target/core-check` passed 111 tests: 70 unit cases and 41 simulation cases, with zero failures or ignored cases. It includes the original 75 core cases, the three-seed five-gate progression proof, and the complete-batch wallet-overflow regression. `cargo clippy --locked -p horde_core --all-targets --target-dir target/core-check -- -D warnings` also passed. New regressions are additive; no prior assertion or oracle was removed. Combined workspace totals and renderer evidence belong in the current validation report after frontend integration.

Names and regional dressing are original scenario fiction, not researched representations of every culture. The shipped creation profiles all cost 76 points, but their tactical balance is not assumed identical. Full-world completion, every equipment build and human understanding of all progression choices require actual playtests. Clearing/replaying versus intentionally delaying a boss also needs a measured coins-per-minute comparison: a spawn-rate ceiling alone does not model boss occlusion, targeting policy, survival risk or loot. No bonus was raised speculatively to force that conclusion. This review does not claim a graphical playthrough, audio listening test or measured minimum-spec frame target.
