//! Pure battle HUD view model. Every value is read from the authoritative
//! attempt at its current tick; the HUD only lays it out.
use horde_core::{
    content::Mode,
    sim::{Attempt, EnemyPhase, EventKind, Game},
    survivors::SurvivorId,
};

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct ActionReadout {
    pub label: String,
    /// Completion of the current timed action, when it has one.
    pub progress: Option<f32>,
    /// The attempt's own action description (hover text).
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Danger {
    Hit,
    Contact,
    Closing,
    Holding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct DangerReadout {
    pub danger: Danger,
    pub label: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct DpsReadout {
    pub live: f32,
    /// `None` for legacy checkpoints that never recorded earlier damage.
    pub average_and_peak: Option<(f32, f32)>,
    pub totals: horde_core::stats::DamageTotals,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum BossReadout {
    /// Whole seconds until the boss arrives.
    Incoming { seconds: u32 },
    Present {
        health: f32,
        /// Critically wounded bosses report their injury HP.
        critical: bool,
        hp: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::ui) struct HudView {
    pub defender: &'static str,
    pub female_portrait: bool,
    pub hp: i32,
    pub max_hp: i32,
    pub ammo: u32,
    pub magazine: u32,
    pub stamina_label: &'static str,
    pub stamina: f32,
    pub max_stamina: f32,
    pub action: ActionReadout,
    pub danger: DangerReadout,
    pub dps: DpsReadout,
    pub boss: BossReadout,
    pub elapsed: f32,
    pub kills: u32,
    pub coins: u64,
    pub lanes: u8,
    pub width_yards: f32,
    pub threat_rank: u32,
    pub difficulty: &'static str,
    pub batch_size: u32,
    pub gear_level: u32,
}

/// The player's own battle HUD, or `None` outside a battle.
pub(in crate::ui) fn view_model(game: &Game) -> Option<HudView> {
    let a = game.attempt()?;
    Some(HudView {
        defender: if a.mode == Mode::Personal {
            game.campaign()
                .selected_survivor
                .map_or("YOU", SurvivorId::name)
        } else {
            "MARA"
        },
        female_portrait: a.mode == Mode::Npc
            || a.profile.appearance == horde_core::survivors::Appearance::Female,
        hp: a.hp.max(0),
        max_hp: a.max_hp,
        ammo: a.ammo,
        magazine: a.magazine,
        stamina_label: if a.mode == Mode::Npc { "FP" } else { "Stamina" },
        stamina: a.stamina,
        max_stamina: a.max_stamina,
        action: action(a),
        danger: danger(a),
        dps: dps(a),
        boss: boss(a),
        elapsed: a.elapsed(),
        kills: a.kills,
        coins: a.coins,
        lanes: a.lane_count,
        width_yards: a.width_yards,
        threat_rank: a.progression.rank,
        difficulty: a.balance.preset.name(),
        batch_size: a.progression.batch_size,
        gear_level: a.progression.item_level,
    })
}

fn boss(a: &Attempt) -> BossReadout {
    match a.enemies.iter().find(|e| e.boss) {
        Some(boss) => {
            let critical = boss.critically_wounded(a.mode);
            BossReadout::Present {
                health: boss.hp.max(0) as f32 / boss.max_hp as f32,
                critical,
                hp: if critical {
                    boss.injury.hp
                } else {
                    boss.hp.max(0)
                },
            }
        }
        None => BossReadout::Incoming {
            seconds: a.boss_countdown() as u32,
        },
    }
}

pub(in crate::ui) fn action(attempt: &Attempt) -> ActionReadout {
    let (label, progress) = if attempt.reload_left > 0 {
        let total = attempt.reload_duration_ticks() as f32;
        (
            format!("Reload · {:.1}s", attempt.reload_left as f32 / 60.0),
            Some(1.0 - attempt.reload_left as f32 / total),
        )
    } else if attempt.melee_commit > 0 {
        (
            format!(
                "Melee recovery · {:.1}s",
                attempt.melee_commit as f32 / 60.0
            ),
            Some(1.0 - attempt.melee_commit as f32 / 30.0),
        )
    } else if attempt.mode == Mode::Npc {
        (attempt.action(), Some((attempt.tick % 60) as f32 / 60.0))
    } else if attempt.melee_cooldown > 0 {
        (
            format!(
                "Melee ready in {:.1}s",
                attempt.melee_cooldown as f32 / 60.0
            ),
            Some(1.0 - attempt.melee_cooldown as f32 / 72.0),
        )
    } else {
        (attempt.action(), None)
    };
    ActionReadout {
        label,
        progress,
        detail: attempt.action(),
    }
}

pub(in crate::ui) fn danger(attempt: &Attempt) -> DangerReadout {
    let mut lanes: Vec<_> = attempt
        .enemies
        .iter()
        .filter(|enemy| enemy.contact())
        .map(|enemy| enemy.lane + 1)
        .collect();
    lanes.sort_unstable();
    lanes.dedup();
    let recent_hit = attempt.events.iter().rev().find(|event| {
        event.kind == EventKind::Damage
            && event.to == attempt.defender_position()
            && attempt.tick.saturating_sub(event.tick) < 30
            && event.value > 0
    });
    let (danger, label) = if let Some(hit) = recent_hit {
        (
            Danger::Hit,
            format!(
                "HIT · LANE {}",
                hit.lane.unwrap_or_else(|| hit.from[1]
                    .floor()
                    .clamp(0.0, f32::from(attempt.lane_count.saturating_sub(1)))
                    as u8)
                    + 1
            ),
        )
    } else if !lanes.is_empty() {
        (
            Danger::Contact,
            format!(
                "{} CONTACT LANE{}",
                lanes.len(),
                if lanes.len() == 1 { "" } else { "S" }
            ),
        )
    } else if attempt.enemies.iter().any(|enemy| enemy.x < 6.0) {
        (Danger::Closing, "CLOSING IN".into())
    } else {
        (Danger::Holding, "LINE HOLDING".into())
    };
    let winding = attempt
        .enemies
        .iter()
        .filter(|enemy| enemy.contact() && enemy.attack_phase == EnemyPhase::Windup)
        .count();
    let detail = if lanes.is_empty() {
        "No enemy is at the shared defense line.".into()
    } else {
        format!(
            "Contact lanes: {}. {winding} attacks winding up. Any contact lane can hurt the same defender; target that lane for melee.",
            lanes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    DangerReadout {
        danger,
        label,
        detail,
    }
}

pub(in crate::ui) fn dps(attempt: &Attempt) -> DpsReadout {
    DpsReadout {
        live: shown_dps(attempt.live_dps()),
        average_and_peak: (!attempt.gear_snapshot.is_empty()).then(|| {
            (
                shown_dps(attempt.average_dps()),
                shown_dps(attempt.peak_dps()),
            )
        }),
        totals: attempt.dps.totals,
    }
}
/// A DPS value fit for display: never negative (so `{:.1}` cannot print the
/// `-0.0` an empty window yields) and zero when not finite.
pub(in crate::ui) fn shown_dps(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::sim::Input;

    #[test]
    fn dps_readout_never_prints_negative_zero() {
        for value in [-0.0, -0.04, f32::NAN, f32::NEG_INFINITY, f32::INFINITY] {
            assert_eq!(format!("{:.1}", shown_dps(value)), "0.0", "{value}");
        }
        assert_eq!(format!("{:.1}", shown_dps(12.345)), "12.3");
        let game = battle(Mode::Personal);
        let readout = dps(game.attempt().unwrap());
        assert_eq!(format!("DPS {:.1}", readout.live), "DPS 0.0");
        assert!(readout.live.is_sign_positive());
    }
    fn battle(mode: Mode) -> Game {
        let mut game = Game::default();
        if mode == Mode::Npc {
            // Legacy campaigns field Mara without the companion skill path.
            game.campaign_mut().legacy_features = true;
            game.campaign_mut().balance_settings = horde_core::balance::BalanceSettings::legacy();
            game.campaign_mut().attempts = 1;
        }
        game.deploy(0, mode).unwrap();
        game
    }

    #[test]
    fn view_model_is_absent_outside_battle() {
        assert_eq!(view_model(&Game::default()), None);
    }

    #[test]
    fn view_model_reads_personal_vitals_and_a_quiet_line() {
        let game = battle(Mode::Personal);
        let a = game.attempt().unwrap();
        let view = view_model(&game).unwrap();
        assert_eq!(view.defender, "YOU");
        assert_eq!((view.hp, view.max_hp), (a.hp, a.max_hp));
        assert_eq!((view.ammo, view.magazine), (a.ammo, a.magazine));
        assert_eq!(view.stamina_label, "Stamina");
        assert_eq!(view.danger.danger, Danger::Holding);
        assert_eq!(view.danger.label, "LINE HOLDING");
        assert_eq!(
            view.danger.detail,
            "No enemy is at the shared defense line."
        );
        assert_eq!(view.action.detail, a.action());
        assert_eq!(view.dps.live, a.live_dps());
        assert_eq!(
            view.boss,
            BossReadout::Incoming {
                seconds: a.boss_countdown() as u32
            }
        );
        assert_eq!((view.kills, view.coins), (a.kills, a.coins));
        assert_eq!(view.lanes, a.lane_count);
        assert_eq!(view.threat_rank, a.progression.rank);
        assert_eq!(view.difficulty, a.balance.preset.name());
    }

    #[test]
    fn npc_view_uses_mara_fatigue_and_a_turn_progress() {
        let game = battle(Mode::Npc);
        let view = view_model(&game).unwrap();
        assert_eq!(view.defender, "MARA");
        assert!(view.female_portrait);
        assert_eq!(view.stamina_label, "FP");
        assert!(view.action.progress.is_some());
    }

    #[test]
    fn reload_readout_reports_remaining_time_and_progress() {
        let mut game = battle(Mode::Personal);
        let a = game.attempt_mut().unwrap();
        a.reload_left = 30;
        let total = (a.profile.personal.reload.max(0.5) * 60.0).ceil();
        assert_eq!(a.reload_duration_ticks() as f32, total);
        let readout = action(a);
        assert_eq!(readout.label, "Reload · 0.5s");
        assert_eq!(readout.progress, Some(1.0 - 30.0 / total));
    }

    #[test]
    fn contact_lanes_are_counted_once_and_listed() {
        let mut game = battle(Mode::Personal);
        for _ in 0..3600 {
            if !game.attempt().unwrap().enemies.is_empty() {
                break;
            }
            game.tick(Input::default());
        }
        let a = game.attempt_mut().unwrap();
        a.events.clear();
        for enemy in &mut a.enemies {
            enemy.x = 100.0;
        }
        assert_eq!(danger(a).danger, Danger::Holding);
        a.enemies[0].x = 0.0;
        let lane = a.enemies[0].lane + 1;
        let readout = danger(a);
        assert_eq!(readout.danger, Danger::Contact);
        assert_eq!(readout.label, "1 CONTACT LANE");
        assert!(
            readout
                .detail
                .starts_with(&format!("Contact lanes: {lane}. "))
        );
    }
}
