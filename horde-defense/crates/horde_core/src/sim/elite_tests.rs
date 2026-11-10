//! W2-33 (design enemy-balance §5 "Elites", ruling E7, acceptance G6.3 and
//! G6.5): the keyed ELITE and AFFIX spawn rolls, their rate by rank and
//! preset, and their isolation from the combat RNG. W2-35 (design
//! loot-economy §3-§4, ruling E8): the elite forced drop.

use super::*;
use crate::balance::{BalancePreset, BalanceSettings};
use crate::campaign::Campaign;
use crate::enemy_catalog::ResolvedEnemy;

/// A game with an unpaused attempt in `mode` at threat `rank` on Normal,
/// whose director only spawns when a test hands it credit and whose
/// defender cannot fall.
fn game_at(mode: Mode, rank: u32) -> Game {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.balance_settings = BalanceSettings::legacy();
    g.campaign.attempts = 1;
    g.deploy(0, mode).unwrap();
    g.set_paused(false);
    let normal = on(BalancePreset::Normal);
    let a = g.attempt.as_mut().unwrap();
    assert!(a.enemies.is_empty(), "nothing has spawned at deploy");
    a.progression = normal
        .progression_for(mode, rank, a.lane_count, None)
        .unwrap();
    a.max_hp = 1_000_000;
    a.hp = a.max_hp;
    a.spawn_credit = -1.0e9;
    g
}

/// A campaign on `preset`.
fn on(preset: BalancePreset) -> Campaign {
    let mut c = Campaign::default();
    c.set_balance_settings(BalanceSettings::for_preset(preset))
        .unwrap();
    c
}

/// One spawned hostile: id, archetype, lane and its elite affix.
type Spawned = (u64, String, u8, Option<String>);

/// Runs `calls` director passes with one group of credit each, clearing
/// the field before each pass so the cap never binds. Returns every spawn
/// and the combat RNG state afterwards.
fn director_spawns(g: &mut Game, calls: usize) -> (Vec<Spawned>, u64) {
    let mut a = g.attempt.take().unwrap();
    let mut spawned = Vec::new();
    for _ in 0..calls {
        a.enemies.clear();
        a.spawn_credit = 0.999_999;
        g.spawn_director(&mut a);
        for e in &a.enemies {
            let r = e.resolved.as_ref().expect("a wave spawn is resolved");
            spawned.push((
                e.id,
                r.archetype_id.clone(),
                e.lane,
                r.elite.as_ref().map(|m| m.affix_id.clone()),
            ));
        }
    }
    let state = a.rng.state;
    g.attempt = Some(a);
    (spawned, state)
}

#[test]
fn elites_consume_no_combat_rng() {
    for mode in [Mode::Personal, Mode::Npc] {
        let run = |rate: u32| {
            let mut g = game_at(mode, 13);
            g.attempt.as_mut().unwrap().progression.elite_rate_bp = rate;
            director_spawns(&mut g, 400)
        };
        let (none, none_state) = run(0);
        let (all, all_state) = run(10_000);
        assert!(none.len() >= 400, "{mode:?}: {} spawns", none.len());
        assert_eq!(none_state, all_state, "{mode:?}: the combat RNG moved");
        // The same hostiles in the same lanes; only the elite marks differ.
        let plain = |v: &[Spawned]| -> Vec<(u64, String, u8)> {
            v.iter().map(|(i, id, l, _)| (*i, id.clone(), *l)).collect()
        };
        assert_eq!(plain(&none), plain(&all), "{mode:?}");
        assert!(none.iter().all(|s| s.3.is_none()), "{mode:?}: rate 0");
        let catalog = crate::enemy_catalog::enemy_catalog();
        for (id, archetype, _, affix) in &all {
            assert_eq!(
                affix.is_some(),
                catalog.elite_eligible(archetype),
                "{mode:?}: enemy {id} ({archetype}) at rate 10000"
            );
        }
        let affixes: BTreeSet<&str> = all.iter().filter_map(|s| s.3.as_deref()).collect();
        assert_eq!(
            affixes.into_iter().collect::<Vec<_>>(),
            ["frenzied", "hardened", "shielded", "vicious", "volatile"],
            "{mode:?}: every affix is rolled"
        );
    }
}

/// W2-48 (design-save-determinism §5 "RNG keying", plan EB14): the keyed
/// ELITE and AFFIX rolls neither draw nor read the combat RNG, at any rate.
/// From one game, every rate from 0 to 10000 spawns the same hostiles in the
/// same lanes with the combat RNG in the same state after every director
/// pass, and leaves the strategy RNG and the seed alone. With the combat RNG
/// reseeded (other hostiles, other lanes) every hostile's verdict and affix
/// are still the ones its keyed hashes give.
#[test]
fn elite_roll_consumes_no_combat_rng() {
    let catalog = crate::enemy_catalog::enemy_catalog();
    for mode in [Mode::Personal, Mode::Npc] {
        let run = |rate: u32, reseed: Option<u64>| {
            let mut g = game_at(mode, 13);
            let a = g.attempt.as_mut().unwrap();
            a.progression.elite_rate_bp = rate;
            if let Some(seed) = reseed {
                a.rng = Rng::new(seed);
            }
            let strategy = g.strategy.rng.state;
            let (seed, attempt) = (g.seed, a.id);
            let mut spawned = Vec::new();
            let mut states = Vec::new();
            for _ in 0..300 {
                let (pass, state) = director_spawns(&mut g, 1);
                spawned.extend(pass);
                states.push(state);
            }
            assert_eq!(g.strategy.rng.state, strategy, "{mode:?} {rate}");
            assert_eq!(g.seed, seed, "{mode:?} {rate}");
            (spawned, states, seed, attempt)
        };
        let plain = |v: &[Spawned]| -> Vec<(u64, String, u8)> {
            v.iter().map(|(i, id, l, _)| (*i, id.clone(), *l)).collect()
        };
        let (base, base_states, _, _) = run(0, None);
        assert!(base.len() >= 300, "{mode:?}: {} spawns", base.len());
        for rate in [1, 200, 850, 1_275, 5_000, 9_999, 10_000] {
            let (spawned, states, _, _) = run(rate, None);
            let moved = (0..states.len()).find(|&n| states[n] != base_states[n]);
            assert_eq!(moved, None, "{mode:?} {rate}: the combat RNG moved at pass");
            assert_eq!(plain(&spawned), plain(&base), "{mode:?} {rate}");
            let (other, _, seed, attempt) = run(rate, Some(0x0dd5_eed5));
            assert_ne!(plain(&other), plain(&base), "{mode:?}: the reseed shows");
            for (id, archetype, _, affix) in other {
                let elite = catalog.elite_eligible(&archetype)
                    && director::rolls_elite(seed, attempt, id, rate);
                let expected = elite.then(|| {
                    let key = crate::hash::keyed(seed, attempt, id, crate::hash::tags::AFFIX);
                    let base = catalog.resolve(&archetype).unwrap();
                    let profile = catalog.elite_profile(&base, key).unwrap();
                    profile.elite.unwrap().affix_id
                });
                assert_eq!(affix, expected, "{mode:?} {rate}: enemy {id} ({archetype})");
            }
        }
    }
}

#[test]
fn elite_fraction_at_rank_13_is_8_5_pm_0_5_over_20k() {
    let mut g = game_at(Mode::Personal, 13);
    assert_eq!(g.attempt.as_ref().unwrap().progression.elite_rate_bp, 850);
    let catalog = crate::enemy_catalog::enemy_catalog();
    let mut eligible = 0u32;
    let mut elites = 0u32;
    while eligible < 20_000 {
        let (spawned, _) = director_spawns(&mut g, 200);
        for (_, archetype, _, affix) in spawned {
            if catalog.elite_eligible(&archetype) {
                eligible += 1;
                elites += u32::from(affix.is_some());
            }
        }
    }
    let percent = 100.0 * f64::from(elites) / f64::from(eligible);
    assert!(
        (8.0..=9.0).contains(&percent),
        "{elites} elites in {eligible} eligible spawns: {percent:.2}%"
    );
    let a = g.attempt.as_ref().unwrap();
    assert_eq!(a.telemetry.elites_spawned, elites, "telemetry counts them");
}

#[test]
fn hard_elite_rate_1_5x_normal_easy_0_5x() {
    let rate = |preset: BalancePreset, rank: u32| {
        let c = on(preset);
        c.progression_for(Mode::Personal, rank, c.lane_count(), None)
            .unwrap()
            .elite_rate_bp
    };
    let shipped = [13, 23].map(|rank| {
        [
            BalancePreset::Easy,
            BalancePreset::Normal,
            BalancePreset::Hard,
        ]
        .map(|preset| rate(preset, rank))
    });
    assert_eq!(shipped, [[425, 850, 1275], [750, 1500, 2250]]);
    for rank in 0..=60 {
        let normal = rate(BalancePreset::Normal, rank);
        assert_eq!(
            normal,
            crate::progression::elite_bp(crate::progression::catalog(), rank)
        );
        assert_eq!(rate(BalancePreset::Hard, rank), normal * 3 / 2, "{rank}");
        assert_eq!(rate(BalancePreset::Easy, rank), normal / 2, "{rank}");
    }
    // A Custom edit from Hard keeps Hard's rate (ruling E3).
    let mut custom = on(BalancePreset::Hard);
    let mut settings = BalanceSettings::for_preset(BalancePreset::Hard);
    settings.start_lanes = 5;
    custom.set_balance_settings(settings).unwrap();
    assert_eq!(
        custom
            .progression_for(Mode::Npc, 13, custom.lane_count(), None)
            .unwrap()
            .elite_rate_bp,
        1275
    );
}

#[test]
fn no_elites_below_rank_three() {
    for preset in [
        BalancePreset::Easy,
        BalancePreset::Normal,
        BalancePreset::Hard,
    ] {
        let c = on(preset);
        for mode in [Mode::Personal, Mode::Npc] {
            let rate = |rank| {
                c.progression_for(mode, rank, c.lane_count(), None)
                    .unwrap()
                    .elite_rate_bp
            };
            assert_eq!([0, 1, 2].map(rate), [0, 0, 0], "{preset:?} {mode:?}");
            assert!(rate(3) > 0, "{preset:?} {mode:?}: rank 3 rolls elites");
        }
    }
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = game_at(mode, 2);
        let (spawned, _) = director_spawns(&mut g, 2_000);
        assert!(spawned.len() >= 2_000);
        assert!(spawned.iter().all(|s| s.3.is_none()), "{mode:?}");
        assert_eq!(g.attempt.as_ref().unwrap().telemetry.elites_spawned, 0);
    }
}

/// Pin (characterisation): the elite and affix rolls are stateless keyed
/// hashes on their own tags, so the W1-01 quality roll table is unchanged
/// whatever elite rolls run beside it, and the three keys of one hostile
/// never coincide.
#[test]
fn quality_rolls_unchanged_by_the_elite_and_affix_rolls() {
    /// `QUALITY_ROLL_TABLE_FNV` recorded by W1-01 (tests/save_migration.rs).
    const W1_01_QUALITY_ROLL_TABLE_FNV: u64 = 0x1f6b_c282_6f97_e583;
    let quality = crate::quality::catalog();
    let mut tuples = crate::rules::Rng::new(0x5155_414C_5441_424C);
    let mut table = String::new();
    let mut elites = 0;
    for n in 0..10_000u32 {
        let seed = tuples.next_u64();
        let attempt = tuples.next_u64() % 4096;
        let enemy = tuples.next_u64() % 4096;
        let rank = if n % 100 == 99 { 1000 + n % 7 } else { n % 41 };
        let elite = director::rolls_elite(seed, attempt, enemy, 1_500);
        elites += u32::from(elite);
        let keys = [
            crate::hash::tags::QUALITY,
            crate::hash::tags::ELITE,
            crate::hash::tags::AFFIX,
        ]
        .map(|tag| crate::hash::keyed(seed, attempt, enemy, tag));
        assert!(
            keys[0] != keys[1] && keys[1] != keys[2] && keys[0] != keys[2],
            "tuple {n}"
        );
        assert_eq!(elite, keys[1] % 10_000 < 1_500, "tuple {n}: the ELITE key");
        table.push_str(quality.roll(seed, attempt, enemy, rank).name());
        table.push('\n');
    }
    assert!((1_300..=1_700).contains(&elites), "{elites}");
    assert_eq!(
        crate::hash::fnv1a(table.as_bytes()),
        W1_01_QUALITY_ROLL_TABLE_FNV
    );
}

/// The one health guard of deploy and save validation (design
/// enemy-balance §5 "Health guard"): the boss with its health multiplier,
/// the largest record, and the largest elite only when the snapshot rolls
/// elites, times the encounter's health modifier.
#[test]
fn encounter_health_bound_counts_the_largest_elite_only_when_elites_roll() {
    let mut level = content().levels[0].clone();
    level.boss_health = 1;
    level.boss_npc_hp = 1;
    let modifiers = Modifiers::default();
    let snapshot = |rate: u32| crate::progression::ProgressionSnapshot {
        rank: 3,
        enemy_cap: 72,
        elite_rate_bp: rate,
        ..Default::default()
    };
    let bound = |mode, rate| encounter_health_bound(&level, mode, &snapshot(rate), &modifiers);
    // The juggernaut: 420 health and 30 NPC hit points; Hardened 1.5 x 1.6
    // gives 1008, and round(30 x 1.5) + 4 = 49.
    assert_eq!(bound(Mode::Personal, 0), 420.0);
    assert_eq!(bound(Mode::Personal, 1), 1008.0);
    assert_eq!(bound(Mode::Npc, 0), 30.0);
    assert_eq!(bound(Mode::Npc, 1), 49.0);
    let mut boss = snapshot(1);
    boss.boss_health_floor = 2_000;
    boss.boss_health_multiplier = 1.5;
    let mut doubled = modifiers;
    doubled.enemy_health = 2.0;
    assert_eq!(
        encounter_health_bound(&level, Mode::Personal, &boss, &doubled),
        6_000.0
    );
}

/// Elite kills are counted (feeding `RunRecord.elite_kills`), never past the
/// elites spawned, and an elite pays its baked reward.
#[test]
fn elite_kills_are_counted_and_pay_the_baked_reward() {
    let mut g = game_at(Mode::Personal, 13);
    let mut a = g.attempt.take().unwrap();
    a.progression.elite_rate_bp = 10_000;
    g.spawn_archetype_as(&mut a, "shambler", 5, director::Arrival::Wave)
        .unwrap();
    g.spawn_archetype(&mut a, "shambler", 6).unwrap();
    let profiles: Vec<_> = a
        .enemies
        .iter()
        .map(|e| e.resolved.clone().unwrap())
        .collect();
    assert!(profiles[0].elite.is_some() && profiles[1].elite.is_none());
    assert_eq!(profiles[0].reward, 4 * profiles[1].reward);
    assert_eq!(a.telemetry.elites_spawned, 1);
    for e in &mut a.enemies {
        e.hp = 0;
    }
    let coins = a.coins;
    g.reap(&mut a);
    assert!(a.enemies.is_empty());
    assert_eq!((a.telemetry.elites_killed, a.kills), (1, 2));
    let paid = |reward: u64| (reward as f32 * a.modifiers.reward).round().max(1.0) as u64;
    assert_eq!(
        a.coins - coins,
        paid(profiles[0].reward) + paid(profiles[1].reward)
    );
    // An elite whose spawn was never counted (a resumed attempt) counts no
    // kill.
    g.spawn_archetype_as(&mut a, "runner", 5, director::Arrival::Wave)
        .unwrap();
    a.telemetry.elites_spawned = 1;
    a.enemies[0].hp = 0;
    g.reap(&mut a);
    assert_eq!(a.telemetry.elites_killed, 1);
}

// W2-35 (design loot-economy §3-§4, ruling E8): the elite forced drop.

/// A shambler profile and its elite twin.
fn shambler_pair() -> (ResolvedEnemy, ResolvedEnemy) {
    let catalog = crate::enemy_catalog::enemy_catalog();
    let plain = catalog.resolve("shambler").unwrap();
    let elite = catalog.elite_profile(&plain, 0).unwrap();
    assert!(plain.elite.is_none() && elite.elite.is_some());
    (plain, elite)
}

/// Places `profile` in lane 5 (a split child at x 7 when `split`), defeats
/// it, sets the kill count and the combat RNG, reaps it and returns its id.
fn kill(
    g: &mut Game,
    a: &mut Attempt,
    profile: &ResolvedEnemy,
    split: bool,
    kills: u32,
    rng: Rng,
) -> u64 {
    a.enemies.clear();
    if split {
        g.place_enemy(a, profile.clone(), 5, false, director::Arrival::Split(7.0));
    } else {
        g.spawn_pick(a, profile.clone(), 5);
    }
    let mode = a.mode;
    let e = a.enemies.last_mut().expect("placed");
    assert_eq!(e.split_child, split);
    if mode == Mode::Personal {
        e.hp = 0;
    } else {
        e.injury.unconscious = true;
    }
    let id = e.id;
    a.kills = kills;
    a.rng = rng;
    g.reap(a);
    assert!(a.enemies.is_empty(), "the hostile was reaped");
    id
}

/// The `skip`-th seeded combat RNG whose next `below(100)` hits (`< 3`) or
/// misses the 3% drop roll.
fn drop_rng(hit: bool, skip: usize) -> Rng {
    (1..)
        .map(Rng::new)
        .filter(|rng| (rng.clone().below(100) < 3) == hit)
        .nth(skip)
        .unwrap()
}

/// The combat RNG after the draws a kill should take: the 3% roll, then
/// one weighted pick, `below(total)` over a DropTable of total `total`, when
/// the normal drop fires (W2-09; the uniform pick drew `below(len)`).
fn after_draws(mut rng: Rng, total: Option<u64>) -> Rng {
    rng.below(100);
    if let Some(total) = total {
        rng.below(total);
    }
    rng
}

/// The definition index a loot event names.
fn definition_of(event: &crate::quality::LootEvent) -> usize {
    event.definition.index().expect("a catalogue item")
}

#[test]
fn elite_status_never_changes_combat_rng_draw_count() {
    let (plain, elite) = shambler_pair();
    for mode in [Mode::Personal, Mode::Npc] {
        let mut gp = game_at(mode, 13);
        let mut ge = game_at(mode, 13);
        let mut ap = gp.attempt.take().unwrap();
        let mut ae = ge.attempt.take().unwrap();
        assert!(
            ap.drop_tier.is_some(),
            "a wave-2 deploy is on the Some path"
        );
        let pool = crate::loot::DropTable::for_attempt(&ap).total();
        for kills in 0..42u32 {
            for hit in [false, true] {
                let rng = drop_rng(hit, kills as usize);
                let normal = hit || (kills + 1).is_multiple_of(20);
                let (plain_loot, elite_loot) = (ap.loot, ae.loot);
                kill(&mut gp, &mut ap, &plain, false, kills, rng);
                kill(&mut ge, &mut ae, &elite, false, kills, rng);
                let label = format!("{mode:?} kills {kills} hit {hit}");
                assert_eq!(ap.rng, after_draws(rng, normal.then_some(pool)), "{label}");
                assert_eq!(ae.rng, ap.rng, "{label}: the elite draws the same");
                assert_eq!(ap.loot - plain_loot, u32::from(normal), "{label}");
                assert_eq!(ae.loot - elite_loot, 1, "{label}: an elite always drops");
            }
        }
    }
}

#[test]
fn elite_only_drop_uses_keyed_pick_and_draws_no_rng() {
    let (_, elite) = shambler_pair();
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = game_at(mode, 13);
        let mut a = g.attempt.take().unwrap();
        let table = crate::loot::DropTable::for_attempt(&a);
        let mut picked = BTreeSet::new();
        for n in 0..40usize {
            let rng = drop_rng(false, n);
            let loot = a.loot;
            let id = kill(&mut g, &mut a, &elite, false, 0, rng);
            assert_eq!(a.loot, loot + 1, "{mode:?} {n}: the elite term alone drops");
            assert_eq!(
                a.rng,
                after_draws(rng, None),
                "{mode:?} {n}: only the 3% roll"
            );
            let key = crate::hash::keyed(g.seed, a.id, id, crate::hash::tags::ELITE_PICK);
            let event = a.loot_events.last().unwrap();
            let definition = table.pick(key % table.total());
            assert_eq!(definition_of(event), definition, "{mode:?} {n}");
            assert_eq!(a.drop_ids.last(), Some(&event.item_id), "{mode:?} {n}");
            picked.insert(definition);
        }
        assert!(
            picked.len() > 5,
            "{mode:?}: the keyed pick spreads: {picked:?}"
        );
        // The legacy path (drop_tier None, ruling E11) knows no elites.
        a.drop_tier = None;
        let rng = drop_rng(false, 0);
        let loot = a.loot;
        kill(&mut g, &mut a, &elite, false, 0, rng);
        assert_eq!(a.loot, loot, "{mode:?}: no elite drop before v7");
        assert_eq!(a.rng, after_draws(rng, None), "{mode:?}");
    }
}

/// Characterisation pin (W2-29 landed the split-child rule; ruling E8): a
/// split child, elite or not, takes and discards the 3% roll and never
/// drops, even on a hit or on the every-20th kill.
#[test]
fn split_child_kill_takes_the_drop_draw_and_never_drops() {
    let (plain, elite) = shambler_pair();
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = game_at(mode, 13);
        let mut a = g.attempt.take().unwrap();
        for profile in [&plain, &elite] {
            for (kills, hit) in [(0, false), (0, true), (19, false), (39, true)] {
                let rng = drop_rng(hit, 3);
                let loot = a.loot;
                kill(&mut g, &mut a, profile, true, kills, rng);
                let label = format!(
                    "{mode:?} elite {} kills {kills} hit {hit}",
                    profile.elite.is_some()
                );
                assert_eq!(a.kills, kills + 1, "{label}");
                assert_eq!(a.loot, loot, "{label}: a split child never drops");
                assert_eq!(a.rng, after_draws(rng, None), "{label}: only the 3% roll");
            }
        }
    }
}

#[test]
fn elite_drop_quality_is_at_least_uncommon_and_boss_drop_has_no_floor() {
    use crate::quality::Quality;
    let (_, elite) = shambler_pair();
    let mut g = game_at(Mode::Personal, 13);
    let mut a = g.attempt.take().unwrap();
    let rank = a.progression.rank;
    let mut elite_qualities = BTreeSet::new();
    for n in 0..300usize {
        // An elite-only drop and an elite that also fires the normal drop
        // (the every-20th kill) both carry the floor.
        let kills = if n % 2 == 0 { 0 } else { 19 };
        let loot = a.loot;
        let id = kill(&mut g, &mut a, &elite, false, kills, drop_rng(false, n));
        assert_eq!(a.loot, loot + 1, "{n}: an elite always drops");
        let event = a.loot_events.last().unwrap();
        let base = content().items[definition_of(event)].base_quality;
        assert!(
            event.quality >= Quality::Uncommon,
            "{n}: {:?}",
            event.quality
        );
        assert_eq!(
            event.quality,
            crate::loot::drop_quality(base, g.seed, a.id, id, rank, true),
            "{n}"
        );
        elite_qualities.insert(event.quality);
    }
    assert!(elite_qualities.len() >= 2, "{elite_qualities:?}");
    let mut boss_qualities = BTreeSet::new();
    for n in 0..300u32 {
        a.enemies.clear();
        g.spawn_enemy(&mut a, EnemyKind::Brute, 5, true);
        let e = a.enemies.last_mut().unwrap();
        assert!(e.boss);
        e.hp = 0;
        let id = e.id;
        let loot = a.loot;
        g.reap(&mut a);
        assert_eq!(a.loot, loot + 1, "{n}: a boss always drops");
        let event = a.loot_events.last().unwrap();
        let base = content().items[definition_of(event)].base_quality;
        assert_eq!(
            event.quality,
            crate::loot::drop_quality(base, g.seed, a.id, id, rank, false),
            "{n}"
        );
        boss_qualities.insert(event.quality);
    }
    assert!(
        boss_qualities.iter().any(|q| *q < Quality::Uncommon),
        "a boss drop has no floor: {boss_qualities:?}"
    );
}

/// Acceptance G6.3: every elite a director wave fields drops one item of
/// Uncommon or better when it falls, whatever its archetype and affix. Every
/// boss is defeated first, so the boss-gated pack variants join too.
#[test]
fn every_elite_kill_drops_uncommon_or_better() {
    let catalog = crate::enemy_catalog::enemy_catalog();
    let eligible: Vec<&str> = catalog
        .archetype_ids()
        .filter(|id| catalog.elite_eligible(id))
        .collect();
    let gates: Vec<_> = eligible
        .iter()
        .filter_map(|id| catalog.resolve(id).unwrap().unlocked_by)
        .collect();
    assert!(!gates.is_empty(), "some eligible archetype is boss-gated");
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = game_at(mode, 23);
        g.campaign.defeated_bosses.extend(&gates);
        let mut a = g.attempt.take().unwrap();
        a.progression.elite_rate_bp = 10_000;
        let mut affixes = BTreeSet::new();
        let mut kills = 0;
        for &archetype in &eligible {
            for n in 0..8usize {
                a.enemies.clear();
                g.spawn_archetype_as(&mut a, archetype, (n % 5) as u8, director::Arrival::Wave)
                    .unwrap_or_else(|e| panic!("{mode:?} {archetype}: {e}"));
                a.enemies.truncate(1);
                let e = &mut a.enemies[0];
                let affix = e.resolved.as_ref().and_then(|r| r.elite.as_ref());
                affixes.insert(affix.expect("rate 10000 makes it elite").affix_id.clone());
                if mode == Mode::Personal {
                    e.hp = 0;
                } else {
                    e.injury.unconscious = true;
                }
                a.kills = 0;
                a.rng = drop_rng(false, n);
                let loot = a.loot;
                g.reap(&mut a);
                assert_eq!(a.loot, loot + 1, "{mode:?} {archetype} {n}");
                let quality = a.loot_events.last().unwrap().quality;
                assert!(
                    quality >= crate::quality::Quality::Uncommon,
                    "{mode:?} {archetype} {n}: {quality:?}"
                );
                kills += 1;
            }
        }
        assert_eq!(
            kills,
            8 * eligible.len(),
            "{mode:?}: every eligible archetype fell"
        );
        assert_eq!(affixes.len(), 5, "{mode:?}: every affix fell: {affixes:?}");
    }
}
