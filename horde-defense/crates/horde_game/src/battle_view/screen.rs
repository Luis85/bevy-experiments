use super::readouts::{
    FieldHint, FieldReadout, LOG_TAIL, OrdnanceSlotView, ReticleReadout, WeaponView,
};
use horde_core::{
    content::{Mode, StructureKind, content},
    kind_map::KindMap,
    ordnance::OrdnanceKind,
    quality::LootEvent,
    sim::Game,
    world::world,
};

/// The arena the battle screen projects onto: the attempt's, else the
/// campaign's next deployment (lanes, width in yards).
pub fn arena_geometry(game: &Game) -> (u8, f32) {
    game.attempt().map_or_else(
        || {
            (
                game.campaign().lane_count(),
                game.campaign().arena_geometry().1,
            )
        },
        |a| (a.lane_count, a.width_yards),
    )
}

/// Every attempt value the battle screen lays out around the painter.
#[derive(Clone, Debug, PartialEq)]
pub struct BattleScreen {
    pub mode: Mode,
    pub tick: u64,
    pub paused: bool,
    pub lanes: u8,
    pub width_yards: f32,
    /// Backdrop and terrain theme index.
    pub theme: usize,
    pub last_loot: Option<LootEvent>,
    pub weapon_class: &'static str,
    pub weapon: WeaponView,
    pub ordnance: KindMap<OrdnanceKind, OrdnanceSlotView>,
    pub heal: bool,
    pub brace: bool,
    pub melee_cooldown: u32,
    pub supply: u32,
    pub field: FieldReadout,
    /// Newest first, at most `LOG_TAIL` lines.
    pub log_tail: Vec<String>,
    /// Every enemy's position, in attempt order, for aim assist.
    pub targets: Vec<[f32; 2]>,
    pub blueprints: KindMap<StructureKind, bool>,
    pub global_paused: bool,
    /// The campaign has not finished a deployment yet.
    pub first_deployment: bool,
}
impl BattleScreen {
    /// The active battle's screen, or `None` outside a battle.
    pub fn build(game: &Game) -> Option<Self> {
        #[cfg(test)]
        super::builds::record(|b| &mut b.screen);
        let a = game.attempt()?;
        let country = world().country(a.country_id.as_deref().unwrap_or(""));
        let theme = country
            .map(|country| match country.biome.to_lowercase().as_str() {
                "desert" | "arid" => 2,
                "tundra" | "polar" | "boreal" => 3,
                "tropical" | "rainforest" => 4,
                "mediterranean" => 1,
                _ => 0,
            })
            .unwrap_or(a.level);
        let hint = if a.tick == 0 {
            FieldHint::Opening
        } else if a.shots == 0 {
            FieldHint::Shoot
        } else if a.reload_ticks == 0 && a.ammo < a.magazine {
            FieldHint::Reload
        } else if a.first_contact.is_none() {
            FieldHint::Contact
        } else {
            FieldHint::Banked
        };
        Some(Self {
            mode: a.mode,
            tick: a.tick,
            paused: a.paused,
            lanes: a.lane_count,
            width_yards: a.width_yards,
            theme,
            last_loot: a.loot_events.last().cloned(),
            weapon_class: a.profile.weapon.class.name(),
            weapon: WeaponView::of(a),
            ordnance: KindMap::from_fn(|kind| OrdnanceSlotView::of(a, kind)),
            heal: a.profile.heal,
            brace: a.profile.brace,
            melee_cooldown: a.melee_cooldown,
            supply: a.supply,
            field: FieldReadout {
                threat_rank: a.progression.rank,
                hostiles: a.enemies.len(),
                enemy_cap: a.enemy_cap(),
                arrival_rate: a.arrival_rate(),
                difficulty: a.balance.preset.name(),
                batch_size: a.progression.batch_size,
                gear_level: a.progression.item_level,
                level_name: &content().levels[a.level].name,
                hint,
                setting: country.map(|country| country.stage_theme(a.stage)),
            },
            log_tail: a.log.iter().rev().take(LOG_TAIL).cloned().collect(),
            targets: a.enemies.iter().map(|e| [e.x, e.y()]).collect(),
            blueprints: game.campaign().blueprints.clone(),
            global_paused: game.global_paused(),
            first_deployment: game.campaign().attempts == 0,
        })
    }
    pub fn personal(&self) -> bool {
        self.mode == Mode::Personal
    }
    pub fn any_blueprint(&self) -> bool {
        self.blueprints.iter().any(|owned| *owned)
    }
    pub fn slot(&self, kind: OrdnanceKind) -> &OrdnanceSlotView {
        &self.ordnance[kind]
    }
    /// The reticle readout with `selected` throwable, if any.
    pub fn reticle(&self, selected: Option<OrdnanceKind>) -> ReticleReadout {
        ReticleReadout::from_parts(
            self.mode,
            &self.weapon,
            selected.map(|kind| self.slot(kind)),
        )
    }
    /// The 1-based lane the aim selects, clamped onto the arena.
    pub fn selected_lane(&self, aim: [f32; 2]) -> u8 {
        aim[1]
            .floor()
            .clamp(0.0, f32::from(self.lanes.saturating_sub(1))) as u8
            + 1
    }
    /// The nearest enemy within `radius` yards of `aim`, else `aim` itself.
    pub fn assist(&self, aim: [f32; 2], radius: f32) -> [f32; 2] {
        if radius <= 0.0 {
            return aim;
        }
        let distance = |p: &[f32; 2]| ((p[0] - aim[0]).powi(2) + (p[1] - aim[1]).powi(2)).sqrt();
        self.targets
            .iter()
            .filter(|p| distance(p) < radius)
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
            .copied()
            .unwrap_or(aim)
    }
}
