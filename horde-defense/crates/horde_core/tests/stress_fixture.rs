//! The review/test-only 400-enemy stress fixture (W1-35, ruling W-11). It is
//! built through the staging seam and legal commands, validated at the
//! authored cap, and only then raised to `STRESS_ENEMY_CAP` through
//! `Attempt::stage_review_enemy_cap`, which leaves it unsaveable by
//! construction. The production cap stays `GAMEPLAY_ENEMY_CAP`.
use std::collections::BTreeSet;

use horde_core::{
    content::Mode,
    enemy_catalog::enemy_catalog,
    progression::GAMEPLAY_ENEMY_CAP,
    sim::{
        Game, Input, ParallelBattle, SAVE_VERSION, STRESS_ENEMY_CAP, SaveError, save_clone_fixture,
        stress_authored_fixture, stress_render_fixture,
    },
    weapons::WeaponClass,
    world::world,
};

fn drawn(g: &Game) -> usize {
    g.attempt().map_or(0, |a| a.enemies.len())
}

#[test]
fn stress_fixture_is_not_saveable() {
    let g = stress_render_fixture().expect("stress fixture");
    match g.to_save_json() {
        Err(SaveError::Invalid(reason)) => assert!(
            reason.contains("enemy cap") && reason.contains(&STRESS_ENEMY_CAP.to_string()),
            "{reason}"
        ),
        other => panic!("a staged stress game must not save: {other:?}"),
    }
    // Nor does its payload load.
    let payload = serde_json::to_value(&g).unwrap();
    assert!(Game::from_payload(payload, SAVE_VERSION).is_err());
    // Nothing else makes it unsaveable: back at the authored cap and crowd and
    // without the review-only fire zones the same game saves.
    let base = stress_authored_fixture().unwrap();
    let base = base.attempt().unwrap();
    let mut authored = g.clone();
    let a = authored.staging().attempt().unwrap();
    a.progression.enemy_cap = base.progression.enemy_cap;
    a.enemies.truncate(base.enemies.len());
    a.ordnance.fire_zones.clear();
    authored.to_save_json().unwrap();
}

#[test]
fn stress_fixture_reaches_400_enemies_within_1800_ticks() {
    let mut g = stress_render_fixture().expect("stress fixture");
    assert_eq!(STRESS_ENEMY_CAP, 399);
    assert_eq!(g.attempt().unwrap().enemy_cap(), STRESS_ENEMY_CAP);
    assert_eq!(drawn(&g), 400, "the first frame draws 400");
    let workers: Vec<_> = g.battles().iter().map(worker_tick).collect();
    assert!(workers.iter().all(Option::is_some) && workers.len() == 3);
    g.set_paused(false);
    let mut low = drawn(&g);
    // The director refills the raised cap: after kills the field draws 400
    // again, and it never sinks to the production ceiling.
    let mut refilled_at = None;
    for tick in 1..=1800 {
        g.tick(Input::default());
        let a = g
            .attempt()
            .unwrap_or_else(|| panic!("the stress battle ended at tick {tick}"));
        assert!(drawn(&g) <= STRESS_ENEMY_CAP + 1);
        low = low.min(drawn(&g));
        if tick == 300 {
            // The expeditions simulate beside the main field.
            let now: Vec<_> = g.battles().iter().map(worker_tick).collect();
            let expected: Vec<_> = workers.iter().map(|t| t.map(|t| t + 300)).collect();
            assert_eq!(now, expected);
        }
        if refilled_at.is_none() && a.kills > 0 && drawn(&g) == STRESS_ENEMY_CAP + 1 {
            refilled_at = Some(tick);
        }
    }
    assert!(
        refilled_at.is_some(),
        "no refill to 400 after a kill in 1800 ticks (low {low})"
    );
    assert!(low > GAMEPLAY_ENEMY_CAP + 1, "low {low}");
    assert!(g.attempt().unwrap().kills > 0);
}

fn worker_tick(battle: &ParallelBattle) -> Option<u64> {
    battle.game().attempt().map(|a| a.tick)
}

#[test]
fn stress_render_fixture_validates_at_authored_cap_before_staging() {
    let base = stress_authored_fixture().expect("authored stress fixture");
    base.to_save_json()
        .expect("the authored stage saves before the review seam");
    let a = base.attempt().expect("main field");
    assert!(a.enemy_cap() <= GAMEPLAY_ENEMY_CAP);
    assert_eq!(a.enemies.len(), a.enemy_cap() + 1);
    assert_eq!(a.enemies.iter().filter(|e| e.boss).count(), 1);
    assert_eq!((a.lane_count, a.width_yards), (17, 30.0));
    assert_eq!(a.mode, Mode::Npc);
    assert_eq!(a.profile.weapon.class, WeaponClass::MachineGun);
    let country = world()
        .country(a.country_id.as_deref().expect("country stage"))
        .unwrap();
    assert_eq!(country.stage_theme(a.stage).lighting, "night");
    // The last authored wave tier since W2-47 (wave 1 held it at 4).
    assert_eq!(a.progression.wave_tier, 11);
    assert!(a.structures.len() >= 4);
    assert!(a.ordnance.fire_zones.is_empty());
    assert_eq!(a.corpses.len(), 1500);
    assert_eq!(base.campaign().items.len(), 1200);
    assert_eq!(base.campaign().run_history.len(), 150);
    let workers: Vec<_> = base
        .battles()
        .iter()
        .map(|b| b.game().attempt().expect("unsettled expedition"))
        .collect();
    assert_eq!(workers.len(), 3);
    assert_eq!(workers[0].enemies.len(), workers[0].enemy_cap() + 1);
    assert!(workers[1..].iter().all(|w| w.enemies.len() >= 60));

    // The review seam raises the cap, tops the crowd up and adds the fire
    // zones on the main field; the campaign, expeditions, structures, corpses
    // and the authored crowd are the validated ones.
    let staged = stress_render_fixture().unwrap();
    let s = staged.attempt().unwrap();
    assert_eq!(s.enemy_cap(), STRESS_ENEMY_CAP);
    assert_eq!(s.ordnance.fire_zones.len(), 3);
    assert_eq!(s.enemies.len(), STRESS_ENEMY_CAP + 1);
    assert_eq!(json(staged.campaign()), json(base.campaign()));
    assert_eq!(json(&s.structures), json(&a.structures));
    assert_eq!(json(&s.corpses), json(&a.corpses));
    assert_eq!(json(&s.enemies[..a.enemies.len()]), json(&a.enemies));
    let digests = |g: &Game| -> Vec<u64> {
        g.battles()
            .iter()
            .map(|b| b.game().state_digest())
            .collect()
    };
    assert_eq!(digests(&staged), digests(&base));
}

/// W2-47: the stress crowd follows the wave-2 content. Both the authored
/// stage and the staged 400 draw every catalogue archetype (the W2-29 to
/// W2-31 records among them), at least one elite (W2-33), the last authored
/// wave tier, and a boss that carries phase state (W2-36), so the perf
/// re-gate measures the full wave-2 crowd. What that paints today: every
/// archetype's existing sprite with its tint (the new archetypes need no new
/// art); no elite tell, which W5-07 adds; and the phase ring only while a
/// boss holds, which this test does not require (the boss spawns with
/// `hold_until` 0).
#[test]
fn stress_render_fixture_includes_every_catalog_archetype_and_an_elite() {
    let catalog = enemy_catalog();
    let every: BTreeSet<&str> = catalog.archetype_ids().collect();
    for (name, g) in [
        ("authored", stress_authored_fixture().unwrap()),
        ("staged", stress_render_fixture().unwrap()),
    ] {
        let a = g.attempt().unwrap();
        let drawn: BTreeSet<&str> = a
            .enemies
            .iter()
            .filter_map(|e| e.resolved.as_ref())
            .map(|r| r.archetype_id.as_str())
            .collect();
        assert_eq!(drawn, every, "{name}: archetypes drawn");
        let elites = a
            .enemies
            .iter()
            .filter(|e| e.resolved.as_ref().is_some_and(|r| r.elite.is_some()))
            .count();
        assert!(elites > 0, "{name}: no elite among {}", a.enemies.len());
        assert!(a.progression.elite_rate_bp > 0, "{name}: elite rate");
        assert_eq!(
            a.progression.wave_tier,
            catalog.wave_count() - 1,
            "{name}: wave tier"
        );
        let boss = a.enemies.iter().find(|e| e.boss).expect("boss");
        assert!(boss.boss_state.is_some(), "{name}: the boss has no phases");
    }
}

/// Serialized form, for comparing parts of two games.
fn json<T: serde::Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

#[test]
fn save_clone_fixture_sizes_the_stash() {
    for items in [1000, 2500] {
        let g = save_clone_fixture(items).expect("save-clone fixture");
        assert_eq!(g.campaign().items.len(), items);
        assert_eq!(g.attempt().unwrap().enemy_cap(), STRESS_ENEMY_CAP);
    }
}
