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
            .filter(|(_, item)| item.slot == slot && item.tier <= g.campaign.unlocked)
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
            .filter(|(_, i)| i.slot == slot && i.tier > 0 && i.tier <= g.campaign.unlocked)
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
