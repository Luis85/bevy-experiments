//! Proximity meetings and morale. Social effects never steer movement or fund production.
use super::super::{economy, living};
use super::{ColonyState, SocialLife, home_of};
use bevy_ecs::prelude::*;
use npc_framework::prelude::*;
use serde_json::json;

pub(super) fn welcome(world: &mut World, actor: Entity) {
    if world.get::<SocialLife>(actor).is_none() {
        world.entity_mut(actor).insert(SocialLife::default());
    }
}

/// Shared availability for seeking and settling a leisure meeting. Walking may
/// be the actor's own social approach; committed work and its approach intent
/// remain owned by their existing action systems and are never interrupted here.
pub fn social_available(world: &World, actor: Entity) -> bool {
    if !world.contains_resource::<ColonyState>()
        || !living(world, actor)
        || world.get::<SocialLife>(actor).is_none()
        || world
            .get::<super::super::production::JobIntent>(actor)
            .is_some()
        || !matches!(super::super::activity(world, actor).0, "idle" | "walking")
    {
        return false;
    }
    let critical = super::super::policy::active(world).map_or_else(
        || super::super::policy::IslandAiPolicy::default().critical_need,
        |policy| policy.critical_need,
    );
    ["Hunger", "Thirst", "Energy", "Warmth"]
        .iter()
        .all(|need| economy::need(world, actor, need) >= critical)
}

/// Leisure remains a low-priority action beneath survival and funded work.
pub fn social_target(world: &World, actor: Entity) -> Option<(SimId, SimPos)> {
    let policy = world.get_resource::<ColonyState>()?;
    let social = world.get::<SocialLife>(actor)?;
    let now = world.resource::<SimClock>().tick;
    if !policy.gatherings
        || !social_available(world, actor)
        || now < social.last_social_tick.saturating_add(600)
    {
        return None;
    }
    let me = world.get::<SimId>(actor)?;
    let pos = world.get::<SimPos>(actor)?;
    world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(id, entity)| *id != me && social_available(world, **entity))
        .filter_map(|(id, entity)| {
            let target = world.get::<SimPos>(*entity).copied()?;
            let direct = pos.dist2(target);
            if direct > 15_000_i64.pow(2) {
                return None;
            }
            let distance = if super::super::frontier::enabled(world) {
                let route = super::super::frontier::routes::estimate(world, *pos, target)?;
                // Leisure is local in walking distance too. A person across a
                // pond must not keep winning over reachable nearby company.
                if route.distance_mm > 15_000 {
                    return None;
                }
                i64::from(route.distance_mm)
            } else {
                direct
            };
            Some((distance, *id, target))
        })
        .min_by_key(|(distance, id, _)| (*distance, *id))
        .map(|(_, id, target)| (id, target))
}

pub(super) fn tick(world: &mut World, now: u64) {
    let people: Vec<_> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world.get::<SocialLife>(**entity).is_some() && living(world, **entity)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect();
    for (id, actor) in &people {
        let housed = home_of(world, *id).is_some();
        let social = world.get::<SocialLife>(*actor).expect("social");
        let companionship = if now.saturating_sub(social.last_social_tick) < 2400 {
            20_000
        } else {
            0
        };
        let bodily = ["Hunger", "Thirst", "Energy", "Warmth"]
            .iter()
            .map(|need| economy::need(world, *actor, need).max(0) / 8)
            .sum::<i32>();
        let target = (bodily + if housed { 30_000 } else { 0 } + companionship).clamp(0, 100_000);
        let mut social = world.get_mut::<SocialLife>(*actor).expect("social");
        social.morale += (target - social.morale).clamp(-200, 200);
    }
    if !world.resource::<ColonyState>().gatherings {
        return;
    }
    for (index, (a_id, a)) in people.iter().enumerate() {
        if !social_available(world, *a)
            || world
                .get::<SocialLife>(*a)
                .expect("social")
                .last_social_tick
                .saturating_add(300)
                > now
        {
            continue;
        }
        for (b_id, b) in people.iter().skip(index + 1) {
            if !social_available(world, *b)
                || world
                    .get::<SocialLife>(*b)
                    .expect("social")
                    .last_social_tick
                    .saturating_add(300)
                    > now
                || !economy::near(world, *a, *b)
            {
                continue;
            }
            for (person, other) in [(*a, *b_id), (*b, *a_id)] {
                let mut social = world.get_mut::<SocialLife>(person).expect("social");
                social.last_social_tick = now;
                social.morale = (social.morale + 1000).min(100_000);
                let affinity = social.affinity.entry(other).or_default();
                *affinity = (*affinity + 250).min(100_000);
            }
            world.resource_mut::<ColonyState>().social_events += 1;
            record_effect(
                world,
                Some(*a_id),
                "VillageSocialMeeting",
                &json!({"with":b_id}),
            );
            break;
        }
    }
}
