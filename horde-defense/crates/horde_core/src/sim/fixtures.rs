//! Deterministic render/review fixtures. Un-gated: the shipped practice scenes
//! (peak, progressed-peak, max-lanes) build their games with them.

use super::persistence::validate_game;
use super::*;
/// Deterministic, isolated review scenario. It uses legal purchases, real
/// footprints and the authored population ceiling; it never touches disk.
pub fn peak_render_fixture() -> Result<Game, String> {
    let mut g = Game::default();
    g.campaign.legacy_features = true;
    g.campaign.choose_start_country("USA")?;
    for level in 0..4 {
        g.campaign.first_clear(level, Mode::Personal);
        g.campaign.finish_attempt();
        g.campaign.record_country_stage_clear(level)?;
    }
    g.campaign.reward(50_000, 0, Mode::Personal);
    g.campaign
        .purchase_skill(crate::campaign::Skill::Engineering)?;
    for kind in StructureKind::ALL {
        g.campaign.purchase_blueprint(kind)?;
    }
    for slot in crate::content::Slot::ALL {
        let definition = content()
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.slot == slot && item.purchasable() && item.tier <= g.campaign.unlocked()
            })
            .max_by_key(|(_, item)| item.tier)
            .map(|(n, _)| n)
            .ok_or("Missing fixture equipment")?;
        let id = g.campaign.buy_item(definition)?;
        g.campaign.equip(Mode::Npc, id)?;
    }
    g.deploy_country_stage(4, Mode::Npc)?;
    let center = g.attempt.as_ref().expect("deployed").lane_count / 2;
    let secondary = (center + 4).min(g.attempt.as_ref().unwrap().lane_count - 1);
    g.place(StructureKind::Turret, center, 1)?;
    g.place(StructureKind::Turret, secondary, 2)?;
    g.place(StructureKind::Trap, 2, 3)?;
    for lane in 0..g.attempt.as_ref().expect("deployed").lane_count {
        if g.attempt.as_ref().unwrap().supply >= content().structure(StructureKind::Wall).supply {
            g.place(StructureKind::Wall, lane, 0)?;
        }
    }
    let mut a = g.attempt.take().expect("fixture deployment");
    a.tick = u64::from(seconds(content().levels[a.level].boss_time));
    let cap = a.enemy_cap();
    for n in 0..cap {
        let lane = (n % usize::from(a.lane_count)) as u8;
        g.spawn_enemy(&mut a, EnemyKind::ALL[n % 5], lane, false);
        let front = a
            .structures
            .iter()
            .filter(|structure| structure.lane == lane && structure.kind != StructureKind::Trap)
            .map(|structure| structure.x() + structure.blocking_radius() + 0.1)
            .fold(2.5_f32, f32::max);
        let rows = cap.div_ceil(usize::from(a.lane_count)).max(2);
        let gap = ((a.width_yards - 0.4 - front) / (rows - 1) as f32).max(0.65);
        a.enemies.last_mut().expect("spawn below cap").x =
            front + (n / usize::from(a.lane_count)) as f32 * gap;
    }
    let center = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, center, true);
    a.enemies.last_mut().expect("fixture boss").x = a.width_yards - 0.6;
    a.boss_spawned = true;
    a.telemetry.boss_arrival_tick = Some(a.tick);
    a.events.clear();
    g.attempt = Some(a);
    g.set_paused(true);
    validate_game(&g, 0)?;
    Ok(g)
}

/// Synthetic maximum-rank crowd for rendering/performance review. Progress and
/// funds are staged deliberately; this is never an earned campaign proof.
pub fn maximum_progression_render_fixture() -> Result<Game, String> {
    let mut g = peak_render_fixture()?;
    g.attempt = None;
    g.campaign.reserved = None;
    let mut remaining = crate::progression::catalog().max_rank as usize;
    for country in crate::world::world()
        .countries
        .iter()
        .filter(|c| c.id != "USA")
    {
        if remaining == 0 {
            break;
        }
        let count = remaining.min(country.level_count());
        remaining -= count;
        g.campaign
            .world
            .country_levels
            .insert(country.id.clone(), count);
        g.campaign
            .world
            .clear_high_water
            .insert(country.id.clone(), count);
        if !g.campaign.world.unlocked.contains(&country.id) {
            g.campaign.world.unlocked.push(country.id.clone());
        }
        if count == country.level_count() && !g.campaign.world.cleared.contains(&country.id) {
            g.campaign.world.cleared.push(country.id.clone());
        }
    }
    g.campaign.player.level = g.campaign.player_level_cap();
    g.campaign.player.xp = 0;
    g.campaign.player.body = KindMap::default();
    g.campaign.player.points = g.campaign.player.level - 1;
    g.campaign.first_clear(4, Mode::Npc);
    g.campaign.victories = u64::from(crate::progression::catalog().max_rank);
    for slot in crate::content::Slot::ALL {
        let definition = content()
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.slot == slot && i.purchasable() && i.tier <= g.campaign.unlocked())
            .max_by_key(|(_, i)| i.tier)
            .map(|(n, _)| n)
            .ok_or("Missing maximum fixture equipment")?;
        let id = g.campaign.buy_item(definition)?;
        g.campaign.equip(Mode::Npc, id)?;
    }
    g.deploy_country_stage(4, Mode::Npc)?;
    let center = g.attempt.as_ref().expect("deployed").lane_count / 2;
    let secondary = (center + 4).min(g.attempt.as_ref().unwrap().lane_count - 1);
    g.place(StructureKind::Turret, center, 1)?;
    g.place(StructureKind::Turret, secondary, 2)?;
    g.place(StructureKind::Trap, 2, 3)?;
    for lane in 0..g.attempt.as_ref().expect("deployed").lane_count {
        if g.attempt
            .as_ref()
            .is_some_and(|a| a.supply >= content().structure(StructureKind::Wall).supply)
        {
            g.place(StructureKind::Wall, lane, 0)?;
        }
    }
    let mut a = g.attempt.take().expect("fixture deployed");
    a.tick = u64::from(seconds(a.boss_time()));
    for n in 0..a.enemy_cap() {
        let lane = (n % usize::from(a.lane_count)) as u8;
        g.spawn_enemy(&mut a, EnemyKind::ALL[n % 5], lane, false);
        a.enemies.last_mut().expect("within cap").x = 5.0
            + (n / usize::from(a.lane_count)) as f32
                * (22.0 / (a.enemy_cap().div_ceil(usize::from(a.lane_count)).max(2) - 1) as f32);
    }
    let center = a.lane_count / 2;
    g.spawn_enemy(&mut a, EnemyKind::Brute, center, true);
    a.enemies.last_mut().expect("boss").x = 28.8;
    a.enemies.last_mut().expect("fixture boss").x = a.width_yards - 0.6;
    a.boss_spawned = true;
    a.telemetry.boss_arrival_tick = Some(a.tick);
    a.events.clear();
    g.attempt = Some(a);
    g.set_paused(true);
    validate_game(&g, 0)?;
    Ok(g)
}

/// Synthetic-scene staging seam: direct mutable access to a [`Game`] for
/// building review fixtures and the shipped practice scenes (deliberately
/// staged funds, cleared stages, placed crowds, rewound attempts).
///
/// This is not a command because staging is not play: it bypasses every rule
/// [`Game::apply`] enforces, on purpose, to reach states no legal command
/// sequence reaches quickly. Frontends must use it only while constructing a
/// synthetic scene, never on a live player campaign. It is un-gated because
/// practice scenes ship in every build. Each method consumes the handle so a
/// call such as `game.staging().campaign().coins = 10` borrows `game` directly.
pub struct Staging<'a> {
    game: &'a mut Game,
}
impl Game {
    /// Opens the [`Staging`] seam. See its documentation before using it.
    pub fn staging(&mut self) -> Staging<'_> {
        Staging { game: self }
    }
}
impl<'a> Staging<'a> {
    pub fn campaign(self) -> &'a mut Campaign {
        &mut self.game.campaign
    }
    pub fn attempt(self) -> Option<&'a mut Attempt> {
        self.game.attempt.as_mut()
    }
    /// The attempt slot itself, to take an attempt out, edit it and put it back.
    pub fn attempt_slot(self) -> &'a mut Option<Attempt> {
        &mut self.game.attempt
    }
    pub fn strategy(self) -> &'a mut crate::strategy::StrategyState {
        &mut self.game.strategy
    }
    /// Strategy and campaign together, for strategy calls that take the campaign.
    pub fn strategy_and_campaign(
        self,
    ) -> (&'a mut crate::strategy::StrategyState, &'a mut Campaign) {
        (&mut self.game.strategy, &mut self.game.campaign)
    }
    pub fn last_result(self) -> &'a mut Option<Report> {
        &mut self.game.last_result
    }
    pub fn battles(self) -> &'a mut Vec<ParallelBattle> {
        &mut self.game.battles
    }
    /// Test-support: hands the live depth-0 Npc defense Mara holds to the
    /// Ready roster recruit `recruit_id`, the saved shape of a recruit
    /// takeover: the recruit becomes Defending, its loadout joins
    /// `reserved_items`, `reserved_recruit` names it, and the attempt takes
    /// the recruit seat and identity appearance. The frozen combat profile
    /// stays the one the Npc deploy built; the takeover deploy
    /// (`Game::deploy_country_stage_as`) builds it from the recruit. Fails without a change when there is no such
    /// attempt or the recruit is unknown, not Ready, or holds reserved items.
    #[cfg(any(test, feature = "test-support"))]
    pub fn stage_takeover(self, recruit_id: u64) -> Result<(), String> {
        let game = self.game;
        let held_by_mara = game
            .attempt
            .as_ref()
            .is_some_and(|a| a.mode == Mode::Npc && a.seat == DefenderSeat::ModeOwner);
        if game.worker || !held_by_mara {
            return Err("A takeover needs Mara's live main-run defense".into());
        }
        let loadout = game
            .strategy
            .roster
            .iter()
            .find(|r| r.id == recruit_id)
            .ok_or("Unknown survivor")?
            .npc
            .loadout;
        game.strategy.reserve_defender(recruit_id)?;
        if let Err(error) = game.campaign.reserve_external(loadout) {
            game.strategy
                .release_defender(recruit_id)
                .expect("just reserved");
            return Err(error);
        }
        game.campaign.reserved_recruit = Some(recruit_id);
        let attempt = game.attempt.as_mut().expect("checked above");
        super::parallel::stamp_recruit_defender(attempt, recruit_id);
        Ok(())
    }
}

#[cfg(any(test, feature = "test-support", feature = "review"))]
pub use stress::{
    STRESS_ENEMY_CAP, save_clone_fixture, stress_authored_fixture, stress_render_fixture,
};

/// The review/test-only stress scene (ruling W-11). Absent from production
/// builds: neither the fixture nor [`Attempt::stage_review_enemy_cap`] exists
/// without `review` or `test-support`.
#[cfg(any(test, feature = "test-support", feature = "review"))]
mod stress {
    use super::*;

    /// Most regular enemies the review-only stress scene keeps alive; with its
    /// boss that is 400 drawn. Production play and saves stay at
    /// [`crate::progression::GAMEPLAY_ENEMY_CAP`]: only
    /// [`Attempt::stage_review_enemy_cap`] reaches this, and the result never saves.
    pub const STRESS_ENEMY_CAP: usize = 399;
    /// Stash size, settled corpses and run records of the stress fixture.
    const STASH_ITEMS: usize = 1200;
    const CORPSES: usize = 1500;
    const RUN_RECORDS: usize = 150;
    /// Wave 1 holds the composition tier at 4; W2-47 follows enemy-balance's bound.
    const WAVE_TIER: usize = 4;
    /// Expeditions fielded beside the main battle, and the hostiles carried by
    /// the two that are not at their own cap.
    const EXPEDITIONS: usize = 3;
    const EXPEDITION_CROWD: usize = 60;

    /// Synthetic 400-enemy stress fixture for performance headroom (never a
    /// gameplay or balance claim): [`stress_authored_fixture`] with the main
    /// field raised to [`STRESS_ENEMY_CAP`], its crowd topped up to 399
    /// hostiles plus the boss, and three Molotov fire zones at their authored
    /// burn duration. All of it is review-only staging: the raised cap fails
    /// production validation, so the game is unsaveable by construction, and
    /// an Npc battle neither ticks nor saves ordnance, so the zones are drawn
    /// load only. No ticks are run; the live review's director refills the
    /// field to the raised cap as the defenders kill.
    pub fn stress_render_fixture() -> Result<Game, String> {
        stage_review(fixture_with_stash(STASH_ITEMS)?)
    }

    /// The stress fixture before the review seam, validated as a save would
    /// be: the maximum-progression crowd on the first night stage of its
    /// country (17 lanes, 30 yd) at the authored population cap plus the boss,
    /// an Npc with the belt-fed machine gun behind the base fixture's turrets,
    /// trap and walls, 1,500 settled corpses, a 1,200-item stash, 150 run
    /// records and three unsettled expeditions (the first at its own cap plus
    /// boss, the others with 60 hostiles each).
    pub fn stress_authored_fixture() -> Result<Game, String> {
        fixture_with_stash(STASH_ITEMS)
    }

    /// [`stress_render_fixture`] with its stash sized to exactly `items`, for
    /// the save-clone benchmark. Errs when `items` is below what the fixture
    /// already owns (its equipment and recruit starters).
    pub fn save_clone_fixture(items: usize) -> Result<Game, String> {
        stage_review(fixture_with_stash(items)?)
    }

    fn stage_review(mut g: Game) -> Result<Game, String> {
        let mut a = g.attempt.take().ok_or("Stress fixture without a field")?;
        a.stage_review_enemy_cap(STRESS_ENEMY_CAP)?;
        // Top the crowd up to the raised cap between the authored rows, so the
        // first frame already draws 400.
        let lanes = usize::from(a.lane_count);
        let authored = a.enemies.iter().filter(|e| !e.boss).count();
        let gap = (a.width_yards - 8.0) / (authored.div_ceil(lanes).max(2) - 1) as f32;
        for n in 0..STRESS_ENEMY_CAP - authored {
            let lane = ((n + lanes / 2) % lanes) as u8;
            g.spawn_enemy(
                &mut a,
                EnemyKind::ALL[n % EnemyKind::ALL.len()],
                lane,
                false,
            );
            a.enemies.last_mut().expect("within the raised cap").x =
                (5.0 + gap * 0.5 + (n / lanes) as f32 * gap).min(a.width_yards - 1.5);
        }
        a.events.clear();
        let molotov = crate::ordnance::catalog().definition(crate::ordnance::OrdnanceKind::Molotov);
        let center = a.lane_count / 2;
        let lanes = [
            center.saturating_sub(4),
            center,
            (center + 4).min(a.lane_count - 1),
        ];
        for (n, lane) in lanes.into_iter().enumerate() {
            let id = a.next_id;
            a.next_id += 1;
            a.ordnance.fire_zones.push(crate::ordnance::FireZone {
                id,
                cell: [(a.width_yards * 0.4) as u8 + 3 * n as u8, lane],
                radius: molotov.radius,
                damage: molotov.burn_damage,
                ticks_left: molotov.burn_ticks,
                tick_left: molotov.burn_interval,
                tick_interval: molotov.burn_interval,
            });
        }
        g.attempt = Some(a);
        Ok(g)
    }

    fn fixture_with_stash(items: usize) -> Result<Game, String> {
        let mut g = maximum_progression_render_fixture()?;
        g.attempt = None;
        g.campaign.reserved = None;
        // The first night stage of the fixture's country; earlier stages are
        // entitled clears, as in the base fixtures.
        let country = g
            .campaign
            .world
            .active_country()
            .ok_or("Stress fixture without a country")?
            .clone();
        let stage = (0..country.level_count())
            .find(|&stage| country.stage_theme(stage).lighting == "night")
            .ok_or("The stress fixture country has no night stage")?;
        for clear in g.campaign.world.next_stage()..stage {
            g.campaign.record_country_stage_clear(clear)?;
        }
        g.campaign.reward(1_000_000, 0, Mode::Personal);
        let machine_gun = content()
            .items
            .iter()
            .position(|item| {
                item.slot == crate::content::Slot::Ranged
                    && crate::weapons::WeaponSpec::from_item(item).class
                        == crate::weapons::WeaponClass::MachineGun
            })
            .ok_or("Missing machine gun")?;
        let id = g.campaign.buy_item(machine_gun)?;
        g.campaign.equip(Mode::Npc, id)?;
        let mut recruits = Vec::new();
        for liberated in g.campaign.world.cleared.clone() {
            while recruits.len() < EXPEDITIONS {
                match g.strategy.recruit(&mut g.campaign, &liberated) {
                    Ok(recruit) => recruits.push(recruit),
                    Err(_) => break,
                }
            }
        }
        if recruits.len() < EXPEDITIONS {
            return Err("The stress fixture could not recruit its expeditions".into());
        }
        stage_items(&mut g, items)?;

        g.deploy_country_stage(stage, Mode::Npc)?;
        let center = g.attempt.as_ref().expect("deployed").lane_count / 2;
        let secondary = (center + 4).min(g.attempt.as_ref().unwrap().lane_count - 1);
        g.place(StructureKind::Turret, center, 1)?;
        g.place(StructureKind::Turret, secondary, 2)?;
        g.place(StructureKind::Trap, 2, 3)?;
        for lane in 0..g.attempt.as_ref().expect("deployed").lane_count {
            if g.attempt
                .as_ref()
                .is_some_and(|a| a.supply >= content().structure(StructureKind::Wall).supply)
            {
                g.place(StructureKind::Wall, lane, 0)?;
            }
        }
        let mut a = g.attempt.take().expect("stress field deployed");
        a.progression.wave_tier = WAVE_TIER;
        let cap = a.enemy_cap();
        fill_field(&mut g, &mut a, cap, true);
        stage_corpses(&mut a, CORPSES);
        a.events.clear();
        g.attempt = Some(a);

        let active = g.campaign.world.active.clone();
        let mut deployed = 0;
        for country in &crate::world::world().countries {
            if deployed == EXPEDITIONS {
                break;
            }
            if Some(&country.id) == active.as_ref() || !g.campaign.world.can_enter(&country.id) {
                continue;
            }
            let stage = g
                .campaign
                .world
                .completed(&country.id)
                .min(country.level_count() - 1);
            if g.deploy_survivor(recruits[deployed], &country.id, stage)
                .is_ok()
            {
                deployed += 1;
            }
        }
        if deployed < EXPEDITIONS {
            return Err("The stress fixture could not deploy its expeditions".into());
        }
        for (n, battle) in g.battles.iter_mut().enumerate() {
            let worker = &mut battle.game;
            let mut a = worker.attempt.take().ok_or("Expedition without a battle")?;
            let (count, boss) = if n == 0 {
                (a.enemy_cap(), true)
            } else {
                (EXPEDITION_CROWD.min(a.enemy_cap()), false)
            };
            fill_field(worker, &mut a, count, boss);
            a.events.clear();
            worker.attempt = Some(a);
        }
        stage_history(&mut g, RUN_RECORDS)?;
        g.set_paused(true);
        validate_game(&g, 0)?;
        Ok(g)
    }

    /// Spreads `count` regular hostiles over the field ahead of its blockers
    /// and, with `boss`, moves the clock to the boss arrival and fields it at
    /// the far edge.
    fn fill_field(g: &mut Game, a: &mut Attempt, count: usize, boss: bool) {
        if boss {
            a.tick = u64::from(seconds(a.boss_time()));
        }
        let lanes = usize::from(a.lane_count);
        let gap = (a.width_yards - 8.0) / (count.div_ceil(lanes).max(2) - 1) as f32;
        for n in 0..count {
            let lane = (n % lanes) as u8;
            g.spawn_enemy(a, EnemyKind::ALL[n % EnemyKind::ALL.len()], lane, false);
            a.enemies.last_mut().expect("within cap").x = 5.0 + (n / lanes) as f32 * gap;
        }
        if boss {
            let center = a.lane_count / 2;
            g.spawn_enemy(a, EnemyKind::Brute, center, true);
            a.enemies.last_mut().expect("fixture boss").x = a.width_yards - 0.6;
            a.boss_spawned = true;
            a.telemetry.boss_arrival_tick = Some(a.tick);
        }
    }

    /// `count` settled corpses with fresh actor ids, recorded in the reward
    /// ledger (as a kill records them) at ascending ticks up to now.
    fn stage_corpses(a: &mut Attempt, count: usize) {
        let lanes = f32::from(a.lane_count);
        let span = a.width_yards - 2.0 - DEFENSE_LINE;
        for n in 0..count {
            let id = a.next_id;
            a.next_id += 1;
            a.reward_ledger.insert(id);
            let kind = EnemyKind::ALL[n % EnemyKind::ALL.len()];
            a.corpses.push(CorpseRecord {
                id,
                kind,
                boss_kind: None,
                at: [
                    DEFENSE_LINE + 0.5 + (n * 37 % 1000) as f32 / 1000.0 * span,
                    0.5 + (n * 61 % 1000) as f32 / 1000.0 * (lanes - 1.0),
                ],
                visual: Some(crate::enemy_catalog::resolve_kind(kind).visual),
                tick: a.tick * n as u64 / count as u64,
            });
        }
    }

    /// Extends the stash with ordinary equipment until it holds exactly `items`.
    fn stage_items(g: &mut Game, items: usize) -> Result<(), String> {
        let owned = g.campaign.items.len();
        if items < owned {
            return Err(format!("The stress fixture already owns {owned} items"));
        }
        let pool: Vec<usize> = (0..content().items.len())
            .filter(|&n| content().items[n].purchasable())
            .collect();
        for n in 0..items - owned {
            g.campaign
                .add_item_at_level(pool[n % pool.len()], 1 + (n % 30) as u32)?;
        }
        Ok(())
    }

    /// `count` run records shaped from the deployed main field, cycling outcomes.
    fn stage_history(g: &mut Game, count: usize) -> Result<(), String> {
        let a = g.attempt.as_ref().ok_or("History needs the main field")?;
        let defender_name = g.campaign.npc.name.clone();
        let records: Vec<_> = (0..count)
            .map(|n| {
                let outcome = [Outcome::Victory, Outcome::Defeat, Outcome::Withdrawal][n % 3];
                let kills = 40 + (n % 90) as u32;
                crate::stats::RunRecord {
                    id: 0,
                    attempt_id: 1 + n as u64,
                    worker_id: None,
                    recruit_id: None,
                    outcome,
                    mode: Mode::Npc,
                    country_id: a.country_id.clone(),
                    country_name: a.country_name.clone(),
                    defender_name: defender_name.clone(),
                    cause: outcome.label().into(),
                    stage: n % (a.stage + 1),
                    template: n % content().levels.len(),
                    survivor: None,
                    lane_count: a.lane_count,
                    width_yards: a.width_yards,
                    threat_rank: a.progression.rank,
                    balance: a.balance.clone(),
                    gear: a.gear_snapshot.clone(),
                    duration_ticks: 3_600 + 97 * n as u64,
                    kills,
                    shots: kills * 3,
                    hits: kills * 2,
                    misses: kills,
                    loot: 0,
                    damage_by_source: Default::default(),
                    average_dps: 12.5,
                    peak_dps: 40.0,
                    coins: u64::from(kills) * 5,
                    kill_xp: 0,
                    completion_xp: 0,
                    loot_by_quality: Default::default(),
                    end_hp: if outcome == Outcome::Defeat {
                        0
                    } else {
                        a.max_hp
                    },
                    max_hp: a.max_hp,
                    legacy: a.gear_snapshot.is_empty(),
                    auto_sold_count: 0,
                    auto_sold_coins: 0,
                    level_grant_coins: 0,
                    milestone_coins: 0,
                    elite_kills: 0,
                    boss_ttk_ticks: None,
                    boss_rank: Default::default(),
                }
            })
            .collect();
        for record in records {
            record.validate()?;
            g.campaign.append_run(record);
        }
        Ok(())
    }
}
