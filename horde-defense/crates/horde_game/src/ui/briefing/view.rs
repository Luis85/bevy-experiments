//! Pure briefing view model: the selected defense and the deployment forecast.
use horde_core::{
    content::{LevelDefinition, Mode, Slot, content},
    defenders::DefenderId,
    progression::ProgressionSnapshot,
    sim::Game,
    world::Country,
};

/// Threat pressure for this deployment and after the next victory.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Forecast {
    pub pressure: ProgressionSnapshot,
    /// Falls back to `pressure` when the next victory cannot be previewed.
    pub next: ProgressionSnapshot,
    pub boss_seconds: f32,
    pub supply: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LoadoutLine {
    pub label: &'static str,
    pub name: String,
    pub level: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct BriefingView {
    pub country: &'static Country,
    pub levels: usize,
    /// Highest selectable local defense index (replays and the next one).
    pub max_stage: usize,
    /// The requested stage clamped to `max_stage`.
    pub stage: usize,
    pub level: &'static LevelDefinition,
    pub forecast: Result<Forecast, String>,
    pub lanes: u8,
    pub path_yards: f32,
    pub defender_lane: u8,
    pub completion_xp: u64,
    pub next_lane_unlock_level: Option<u32>,
    pub character_level: u32,
    pub level_after_victory: u32,
    pub lanes_after_victory: u8,
    pub max_level: bool,
    pub next_path_yards: f32,
    pub loadout: Vec<LoadoutLine>,
    pub heal_ready: bool,
    pub character_points: u32,
    /// Who the briefing is for: the next defender.
    pub defender: DefenderId,
    /// The defender's card name.
    pub defender_name: String,
    /// A survivor defender's unspent training points (0 for the player).
    pub training_unspent: u32,
    pub blueprints: usize,
}

/// The briefing for `defender` on `stage` of the active country, or `None`
/// before a front is chosen. The loadout and the training line read the
/// defender's own records; a recruit no longer on the roster reads as Mara.
pub(super) fn view_model(game: &Game, defender: DefenderId, stage: usize) -> Option<BriefingView> {
    let campaign = game.campaign();
    let mode = defender.mode();
    let recruit = defender
        .recruit()
        .and_then(|id| game.strategy().roster.iter().find(|r| r.id == id));
    let country = campaign.world.active_country()?;
    let levels = country.level_count();
    let max_stage = campaign.world.completed(&country.id).min(levels - 1);
    let stage = stage.min(max_stage);
    let level = &content().levels[stage % 5];
    let forecast = campaign.deployment_progression(mode).map(|pressure| {
        let next = campaign
            .next_victory_progression(mode)
            .unwrap_or_else(|_| pressure.clone());
        let balance = &campaign.balance_settings;
        Forecast {
            boss_seconds: level.boss_time.max(pressure.boss_time_floor) * balance.boss_time,
            supply: (level.supply as i64
                + i64::from(country.modifiers.supply)
                + i64::from(pressure.supply_bonus)
                + i64::from(balance.supply))
            .max(0),
            pressure,
            next,
        }
    });
    let lanes = campaign.lane_count();
    let loadout = game.defender_loadout(defender).unwrap_or(match mode {
        Mode::Personal => campaign.player.loadout,
        Mode::Npc => campaign.npc.loadout,
    });
    let npc = recruit.map_or(&campaign.npc, |r| &r.npc);
    Some(BriefingView {
        country,
        levels,
        max_stage,
        stage,
        level,
        forecast,
        lanes,
        path_yards: campaign.arena_geometry().1,
        defender_lane: lanes / 2 + 1,
        completion_xp: campaign.completion_xp_preview(),
        next_lane_unlock_level: campaign.next_lane_unlock_level(),
        character_level: campaign.player.level,
        level_after_victory: campaign.level_after_victory(),
        lanes_after_victory: campaign.lane_count_after_victory(),
        max_level: campaign.player.is_max_level(),
        next_path_yards: campaign
            .balance_settings
            .geometry(campaign.level_after_victory())
            .1,
        loadout: [
            (Slot::Ranged, "Ranged"),
            (Slot::Melee, "Melee"),
            (Slot::Outfit, "Outfit"),
            (Slot::Accessory, "Accessory"),
        ]
        .into_iter()
        .map(|(slot, label)| {
            let item = campaign
                .item(loadout[slot.index()])
                .expect("equipped item exists");
            LoadoutLine {
                label,
                name: item.definition().name.clone(),
                level: item.level,
            }
        })
        .collect(),
        heal_ready: campaign.profile(mode).heal,
        character_points: campaign.player.points,
        defender,
        defender_name: game
            .defender_card(defender)
            .map_or_else(|| npc.name.clone(), |card| card.name),
        training_unspent: match mode {
            Mode::Personal => 0,
            Mode::Npc => npc.unspent,
        },
        blueprints: campaign.blueprints.iter().filter(|v| **v).count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::survivors::SurvivorId;
    use horde_core::world::world;

    fn started() -> Game {
        let mut game = Game::default();
        game.campaign_mut()
            .choose_survivor(SurvivorId::Rook)
            .unwrap();
        game.campaign_mut().choose_start_country("DEU").unwrap();
        game
    }

    #[test]
    fn no_briefing_before_a_front_is_chosen() {
        assert_eq!(view_model(&Game::default(), DefenderId::Player, 0), None);
    }

    #[test]
    fn stage_is_clamped_to_the_next_unplayed_defense() {
        let mut game = started();
        let view = view_model(&game, DefenderId::Player, 7).unwrap();
        assert_eq!((view.max_stage, view.stage), (0, 0));
        assert_eq!(view.country.id, "DEU");
        assert_eq!(view.levels, world().country("DEU").unwrap().level_count());
        game.campaign_mut().record_country_stage_clear(0).unwrap();
        let view = view_model(&game, DefenderId::Player, 7).unwrap();
        assert_eq!((view.max_stage, view.stage), (1, 1));
        assert_eq!(view.level.name, content().levels[1].name);
    }

    #[test]
    fn forecast_and_deployment_facts_match_the_campaign() {
        let game = started();
        let c = game.campaign();
        let view = view_model(&game, DefenderId::Player, 0).unwrap();
        let forecast = view.forecast.as_ref().unwrap();
        let pressure = c.deployment_progression(Mode::Personal).unwrap();
        assert_eq!(forecast.pressure, pressure);
        assert_eq!(
            forecast.next,
            c.next_victory_progression(Mode::Personal).unwrap()
        );
        assert_eq!(
            forecast.boss_seconds,
            view.level.boss_time.max(pressure.boss_time_floor) * c.balance_settings.boss_time
        );
        assert!(forecast.supply >= 0);
        assert_eq!(view.lanes, c.lane_count());
        assert_eq!(view.defender_lane, c.lane_count() / 2 + 1);
        assert_eq!(view.completion_xp, c.completion_xp_preview());
        assert_eq!(view.character_level, c.player.level);
        assert_eq!(view.loadout.len(), 4);
        assert_eq!(
            view.loadout[0].name,
            c.equipped(Mode::Personal, Slot::Ranged).name
        );
        assert_eq!(view.heal_ready, c.profile(Mode::Personal).heal);
        assert_eq!(view.blueprints, c.blueprints.iter().filter(|v| **v).count());
    }

    #[test]
    fn npc_briefing_reads_the_npc_loadout() {
        let game = started();
        let view = view_model(&game, DefenderId::Mara, 0).unwrap();
        assert_eq!(
            view.loadout[1].name,
            game.campaign().equipped(Mode::Npc, Slot::Melee).name
        );
        assert_eq!(view.defender_name, game.campaign().npc.name);
        assert_eq!(view.training_unspent, game.campaign().npc.unspent);
        let player = view_model(&game, DefenderId::Player, 0).unwrap();
        assert_eq!(player.training_unspent, 0);
        assert_eq!(
            player.defender_name,
            game.defender_card(DefenderId::Player).unwrap().name
        );
    }
}
