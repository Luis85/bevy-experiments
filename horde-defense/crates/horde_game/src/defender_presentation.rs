//! Read-only battle/event adapter for the modular player model. Cosmetic
//! release points live in source coordinates, never in authoritative saves.
use crate::{
    animation::DefenderPose,
    player_model::pose::{PlayerAction, PlayerPoseSample},
};
use horde_core::{
    ordnance::{MAX_ACTIVE_ORDNANCE, ThrownOrdnance},
    sim::{Attempt, Event, EventKind},
    survivors::EquipmentVisual,
};
use std::collections::{BTreeMap, BTreeSet};

const RECOVERY_TICKS: u64 = 24;

pub fn brace_at(attempt: &Attempt, tick: u64) -> f32 {
    if tick > attempt.tick {
        return 0.;
    }
    let remaining = attempt.brace_remaining_ticks();
    if remaining > 0 {
        let end = attempt.tick.saturating_add(u64::from(remaining));
        let start = end.saturating_sub(u64::from(horde_core::sim::BRACE_DURATION_TICKS));
        if tick >= start && tick < end {
            return 1.;
        }
    }
    if tick == attempt.tick {
        return 0.;
    }
    if attempt.events.iter().rev().any(|event| {
        event.kind == EventKind::Ability
            && event.value == 0
            && event.tick <= tick
            && tick - event.tick < u64::from(horde_core::sim::BRACE_DURATION_TICKS)
    }) {
        1.
    } else {
        0.
    }
}

pub fn release_sample(
    attempt: &Attempt,
    tick: u64,
    display_aim: [f32; 2],
    reduced_effects: bool,
) -> PlayerPoseSample {
    let (recoil, hit) = crate::animation::defender_pulses_at(attempt, tick);
    PlayerPoseSample {
        tick,
        aim: display_aim,
        action: PlayerAction::Throw { phase: 0. },
        recoil: if reduced_effects { 0. } else { recoil },
        brace: brace_at(attempt, tick),
        hit,
        reduced_effects,
    }
}

/// Saved release facts survive event pruning and an ended brace on resume.
/// Old projectiles without these optional facts retain the legacy sampler.
pub fn release_sample_for_projectile(
    attempt: &Attempt,
    projectile: &ThrownOrdnance,
    display_aim: [f32; 2],
    reduced_effects: bool,
) -> PlayerPoseSample {
    let Some(cues) = projectile.release_cues else {
        return release_sample(
            attempt,
            release_tick(attempt.tick, &attempt.events, projectile),
            display_aim,
            reduced_effects,
        );
    };
    let (recoil, hit) = crate::animation::defender_pulses_from_ticks(
        cues.tick,
        cues.last_shot_tick,
        cues.last_hit_tick,
    );
    PlayerPoseSample {
        tick: cues.tick,
        aim: display_aim,
        action: PlayerAction::Throw { phase: 0. },
        recoil: if reduced_effects { 0. } else { recoil },
        brace: if cues.brace { 1. } else { 0. },
        hit,
        reduced_effects,
    }
}

pub fn target_world(
    attempt: &Attempt,
    motion: &DefenderPose,
    action: PlayerAction,
    manual_aim: [f32; 2],
) -> [f32; 2] {
    let event_kind = match action {
        PlayerAction::Throw { .. } => Some(EventKind::Throw),
        PlayerAction::Melee { .. } => Some(EventKind::Melee),
        _ => None,
    };
    if let Some(kind) = event_kind
        && let Some(event) = attempt.events.iter().rev().find(|event| {
            event.kind == kind
                && (kind == EventKind::Throw || event.actor_id.is_none())
                && event.tick <= attempt.tick
        })
    {
        return event.to;
    }
    if action == PlayerAction::Hold
        && let Some(target) = motion.shot_target
    {
        return target;
    }
    if attempt.mode == horde_core::content::Mode::Npc {
        motion.target
    } else {
        manual_aim
    }
}

pub fn sample(
    attempt: &Attempt,
    motion: &DefenderPose,
    display_aim: [f32; 2],
    brace: f32,
    reduced_effects: bool,
) -> PlayerPoseSample {
    let latest_action = attempt.events.iter().rev().find(|event| {
        matches!(
            event.kind,
            EventKind::Throw | EventKind::Melee | EventKind::Shot
        ) && (event.kind == EventKind::Throw || event.actor_id.is_none())
            && event.tick <= attempt.tick
    });
    let reload_start = motion.reload_progress.map(|_| {
        let duration = if attempt.mode == horde_core::content::Mode::Npc {
            attempt.profile.npc.reload_turns.saturating_mul(60)
        } else {
            (attempt.profile.personal.reload.max(0.5) * 60.).ceil() as u32
        };
        attempt
            .tick
            .saturating_sub(u64::from(duration.saturating_sub(attempt.reload_left)))
    });
    let recent_action = latest_action.filter(|event| {
        attempt.tick - event.tick < RECOVERY_TICKS
            && reload_start.is_none_or(|start| event.tick >= start)
    });
    // The simulation emits Throw at release. Phase zero is recovery start,
    // not an invented wind-up that would precede the authoritative projectile.
    // A new legal shot, melee or reload interrupts the earlier cosmetic
    // recovery. A throw does not impose a gameplay action lock.
    let action = match recent_action {
        Some(event) if event.kind == EventKind::Throw => PlayerAction::Throw {
            phase: (attempt.tick - event.tick) as f32 / RECOVERY_TICKS as f32,
        },
        Some(event) if event.kind == EventKind::Melee => PlayerAction::Melee {
            phase: (attempt.tick - event.tick) as f32 / RECOVERY_TICKS as f32,
        },
        Some(_) => PlayerAction::Hold,
        None => motion
            .reload_progress
            .map_or(PlayerAction::Hold, |phase| PlayerAction::Reload { phase }),
    };
    PlayerPoseSample {
        tick: attempt.tick,
        aim: display_aim,
        action,
        recoil: motion.recoil,
        brace,
        hit: motion.hit,
        reduced_effects,
    }
}

pub fn release_tick(tick: u64, events: &[Event], projectile: &ThrownOrdnance) -> u64 {
    if let Some(cues) = projectile.release_cues {
        return cues.tick;
    }
    events
        .iter()
        .rev()
        .find(|event| {
            event.kind == EventKind::Throw
                && event.actor_id == Some(projectile.id)
                && event.tick <= tick
        })
        .map_or_else(
            || {
                tick.saturating_sub(u64::from(
                    projectile
                        .flight_ticks
                        .saturating_sub(projectile.flight_left),
                ))
            },
            |event| event.tick,
        )
}

/// At most the simulation's live projectile budget, reset on encounter/skin
/// replacement or a rewound clock. Repeated rendering and viewport resizing
/// reuse the exact original release point instead of following the idle hand.
#[derive(Clone, Debug, Default)]
pub struct ReleaseOrigins {
    encounter: Option<u64>,
    tick: u64,
    model: String,
    appearance: EquipmentVisual,
    points: BTreeMap<(u64, u64), [f32; 2]>,
}
impl ReleaseOrigins {
    pub fn begin_frame(
        &mut self,
        encounter: u64,
        tick: u64,
        model: &str,
        appearance: &EquipmentVisual,
        live: &BTreeSet<(u64, u64)>,
    ) {
        if self.encounter != Some(encounter)
            || tick < self.tick
            || self.model != model
            || self.appearance != *appearance
        {
            self.points.clear();
        }
        self.encounter = Some(encounter);
        self.tick = tick;
        self.model = model.into();
        if self.appearance != *appearance {
            self.appearance.clone_from(appearance);
        }
        self.points.retain(|key, _| live.contains(key));
    }
    pub fn source_point(
        &mut self,
        key: (u64, u64),
        resolve: impl FnOnce() -> Option<[f32; 2]>,
    ) -> Option<[f32; 2]> {
        if let Some(point) = self.points.get(&key) {
            return Some(*point);
        }
        let point = resolve()?;
        if point.iter().any(|value| !value.is_finite()) {
            return None;
        }
        if self.points.len() < MAX_ACTIVE_ORDNANCE {
            self.points.insert(key, point);
        }
        Some(point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: EventKind, tick: u64, actor_id: Option<u64>) -> Event {
        Event {
            kind,
            from: [0.5, 1.5],
            to: [5., 1.5],
            value: 0,
            tick,
            actor_id,
            target_id: None,
            lane: Some(1),
            duration_ticks: 36,
        }
    }

    #[test]
    fn brace_pose_and_historical_release_survive_event_expiry_and_save_resume() {
        use horde_core::{
            content::Mode,
            sim::{Game, Input},
        };
        let mut game = Game::default();
        game.deploy(0, Mode::Personal).unwrap();
        game.attempt.as_mut().unwrap().profile.brace = true;
        game.tick(Input {
            brace: true,
            ..Default::default()
        });
        let born = game.attempt.as_ref().unwrap().tick;
        for _ in 0..40 {
            game.tick(Input::default());
        }
        let attempt = game.attempt.as_ref().unwrap();
        assert!(attempt.brace_remaining_ticks() > 0);
        assert!(
            !attempt
                .events
                .iter()
                .any(|event| event.kind == EventKind::Ability && event.value == 0)
        );
        let resumed: Attempt =
            serde_json::from_str(&serde_json::to_string(attempt).unwrap()).unwrap();
        for sampled in [attempt, &resumed] {
            assert_eq!(brace_at(sampled, sampled.tick), 1.);
            assert_eq!(brace_at(sampled, born.saturating_sub(1)), 0.);
            assert_eq!(
                release_sample(sampled, born + 5, [500., 300.], false).brace,
                1.
            );
            assert_eq!(brace_at(sampled, sampled.tick + 1), 0.);
        }
    }

    #[test]
    fn ended_brace_and_expired_event_history_preserve_an_airborne_release_after_save_resume() {
        use horde_core::{
            balance::BalanceSettings,
            content::Mode,
            ordnance::{OrdnanceKind, ThrowRequest},
            sim::{Game, Input},
        };
        let mut game = Game::default();
        game.campaign
            .set_balance_settings(BalanceSettings::legacy())
            .unwrap();
        game.campaign.coins = 1000;
        game.campaign.purchase_node("brace").unwrap();
        game.campaign.purchase_node("grenade_training").unwrap();
        game.deploy(0, Mode::Personal).unwrap();
        game.set_paused(false);
        game.tick(Input {
            brace: true,
            ..Default::default()
        });
        for _ in 0..287 {
            game.tick(Input::default());
        }
        game.tick(Input {
            fire: true,
            aim: [8., 5.5],
            ..Default::default()
        });
        game.tick(Input {
            throw: Some(ThrowRequest {
                kind: OrdnanceKind::Grenade,
                cell: [8, 5],
            }),
            ..Default::default()
        });
        let attempt = game.attempt.as_ref().unwrap();
        assert_eq!(attempt.tick, 290);
        let projectile = &attempt.ordnance.projectiles[0];
        let cues = projectile.release_cues.unwrap();
        assert_eq!(cues.tick, 290);
        assert!(cues.brace);
        assert_eq!(cues.last_shot_tick, Some(289));
        let expected = release_sample_for_projectile(attempt, projectile, [500., 300.], false);
        assert_eq!(expected, release_sample(attempt, 290, [500., 300.], false));
        for _ in 0..15 {
            game.tick(Input::default());
        }
        let attempt = game.attempt.as_mut().unwrap();
        assert_eq!(attempt.tick, 305);
        assert_eq!(attempt.brace_remaining_ticks(), 0);
        assert!(!attempt.ordnance.projectiles.is_empty());
        // A bounded event queue may lose all older cues independently of flight.
        attempt.events.clear();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lastlight-release-resume-{}-{nonce}.json",
            std::process::id()
        ));
        game.save(&path).unwrap();
        let resumed = Game::load(&path).unwrap();
        let attempt = resumed.attempt.as_ref().unwrap();
        let projectile = &attempt.ordnance.projectiles[0];
        assert_eq!(release_tick(attempt.tick, &attempt.events, projectile), 290);
        assert_eq!(
            release_sample_for_projectile(attempt, projectile, [500., 300.], false),
            expected
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn newer_melee_reload_and_shot_interrupt_throw_recovery() {
        let mut game = horde_core::sim::Game::default();
        game.deploy(0, horde_core::content::Mode::Personal).unwrap();
        let attempt = game.attempt.as_mut().unwrap();
        attempt.tick = 101;
        let thrown = event(EventKind::Throw, 100, Some(17));
        attempt.events = vec![thrown.clone(), event(EventKind::Melee, 101, None)];
        let aim = [500., 300.];
        assert_eq!(
            sample(attempt, &DefenderPose::default(), aim, 0., false).action,
            PlayerAction::Melee { phase: 0. }
        );
        attempt.events = vec![thrown.clone()];
        attempt.reload_left = (attempt.profile.personal.reload.max(0.5) * 60.).ceil() as u32;
        let reloading = DefenderPose {
            reload_progress: Some(0.),
            ..Default::default()
        };
        assert_eq!(
            sample(attempt, &reloading, aim, 0., false).action,
            PlayerAction::Reload { phase: 0. }
        );
        attempt.events.push(event(EventKind::Shot, 101, None));
        attempt.reload_left = 0;
        assert_eq!(
            sample(attempt, &DefenderPose::default(), aim, 0., false).action,
            PlayerAction::Hold
        );
        attempt.events = vec![thrown];
        assert_eq!(
            sample(attempt, &DefenderPose::default(), aim, 0., false).action,
            PlayerAction::Throw { phase: 1. / 24. }
        );
    }

    #[test]
    fn each_projectile_uses_its_own_release_and_resumed_flight_recovers_evicted_events() {
        let projectile = ThrownOrdnance {
            id: 17,
            kind: horde_core::ordnance::OrdnanceKind::Grenade,
            cell: [3, 1],
            flight_left: 20,
            flight_ticks: 36,
            damage: 20,
            radius: 1.,
            edge_damage_fraction: 0.5,
            burn_damage: 0,
            burn_ticks: 0,
            burn_interval: 1,
            release_cues: None,
        };
        let events = [
            event(EventKind::Throw, 124, Some(17)),
            event(EventKind::Throw, 132, Some(18)),
        ];
        assert_eq!(release_tick(140, &events, &projectile), 124);
        assert_eq!(release_tick(140, &[], &projectile), 124);
    }

    #[test]
    fn historical_release_uses_its_own_effects_and_throw_target_over_an_older_shot() {
        let mut game = horde_core::sim::Game::default();
        game.deploy(0, horde_core::content::Mode::Personal).unwrap();
        let attempt = game.attempt.as_mut().unwrap();
        attempt.tick = 104;
        let mut thrown = event(EventKind::Throw, 100, Some(17));
        thrown.to = [9., 2.5];
        let mut later_damage = event(EventKind::Damage, 101, None);
        later_damage.value = 5;
        attempt.events = vec![event(EventKind::Shot, 98, None), thrown, later_damage];
        let release = release_sample(attempt, 100, [500., 300.], false);
        assert_eq!(release.recoil, (1_f32 - 2. / 10.).powi(2));
        assert_eq!(
            release.hit, 0.,
            "A later hit cannot move an earlier release"
        );
        let motion = DefenderPose {
            shot_target: Some([5., 1.5]),
            ..Default::default()
        };
        assert_eq!(
            target_world(
                attempt,
                &motion,
                PlayerAction::Throw { phase: 0. },
                [4., 0.5]
            ),
            [9., 2.5]
        );
    }

    #[test]
    fn a_release_stays_at_its_source_point_across_frames_and_new_projection() {
        let mut cache = ReleaseOrigins::default();
        let key = (17, 100);
        let live = BTreeSet::from([key]);
        let equipment = EquipmentVisual::default();
        cache.begin_frame(1, 100, "rook", &equipment, &live);
        assert_eq!(
            cache.source_point(key, || Some([560., 410.])),
            Some([560., 410.])
        );
        cache.begin_frame(1, 108, "rook", &equipment, &live);
        let source = cache
            .source_point(key, || panic!("Do not resample the idle hand"))
            .unwrap();
        let project = |scale: f32, offset: f32| source.map(|value| value * scale + offset);
        assert_eq!(project(0.1, 20.), [76., 61.]);
        assert_eq!(project(0.2, 40.), [152., 122.]);
    }

    #[test]
    fn run_replacement_rewind_skin_change_and_projectile_removal_cannot_reuse_a_stale_hand() {
        let key = (3, 10);
        let live = BTreeSet::from([key]);
        let mut cache = ReleaseOrigins::default();
        for (encounter, tick, appearance, expected) in [
            (1, 20, "pistol", 1.),
            (2, 30, "pistol", 2.),
            (2, 15, "pistol", 3.),
            (2, 16, "rifle", 4.),
        ] {
            let equipment = EquipmentVisual {
                weapon_class: Some(if appearance == "pistol" {
                    horde_core::weapons::WeaponClass::Pistol
                } else {
                    horde_core::weapons::WeaponClass::Rifle
                }),
                ..Default::default()
            };
            cache.begin_frame(encounter, tick, "june", &equipment, &live);
            assert_eq!(
                cache.source_point(key, || Some([expected; 2])),
                Some([expected; 2])
            );
        }
        let equipment = cache.appearance.clone();
        cache.begin_frame(2, 17, "june", &equipment, &BTreeSet::new());
        assert!(cache.points.is_empty());
    }
}
