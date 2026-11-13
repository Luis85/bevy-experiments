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

/// Slowest and fastest work pace, as a percentage of the base duration.
pub const WORK_PACE_SLOWEST: u32 = 115;
pub const WORK_PACE_FASTEST: u32 = 90;

/// Regional work duration multiplier from morale: 115% at zero morale down to
/// 90% at full morale. Actors without social life, and other profiles, get 100.
pub fn work_pace_percent(world: &World, actor: Entity) -> u32 {
    if !super::super::frontier::enabled(world) {
        return 100;
    }
    let Some(social) = world.get::<SocialLife>(actor) else {
        return 100;
    };
    let morale = social.morale.clamp(0, 100_000) as u32;
    let (slowest, fastest) = super::super::frontier::economy_spec::tuning(world).map_or(
        (WORK_PACE_SLOWEST, WORK_PACE_FASTEST),
        |tuning| {
            (
                tuning.work_pace_slowest_percent,
                tuning.work_pace_fastest_percent,
            )
        },
    );
    // Elders work slower; everybody else at 100%.
    (slowest - morale * (slowest - fastest) / 100_000)
        * super::super::demography::work_percent(world, actor)
        / 100
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
    // Only actors with a social life can be met, and distance is the cheapest
    // test, so it comes first. The ranking key ends in the identity.
    let others: Vec<(SimId, Entity)> = world
        .try_query::<(Entity, &SimId, &SocialLife)>()
        .map(|mut query| query.iter(world).map(|(e, id, _)| (*id, e)).collect())
        .unwrap_or_default();
    others
        .iter()
        .filter_map(|(id, entity)| {
            let target = world.get::<SimPos>(*entity).copied()?;
            let direct = pos.dist2(target);
            if id == me || direct > 15_000_i64.pow(2) || !social_available(world, *entity) {
                return None;
            }
            // Somebody inside a building is company only for those inside it.
            let indoors = super::super::indoors::inside_entity(world, *entity);
            if indoors.is_some() && indoors != super::super::indoors::inside_entity(world, actor) {
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
    let mut people: Vec<(SimId, Entity)> = world
        .try_query::<(Entity, &SimId, &SocialLife)>()
        .map(|mut query| {
            query
                .iter(world)
                .filter(|(entity, _, _)| living(world, *entity))
                .map(|(entity, id, _)| (*id, entity))
                .collect()
        })
        .unwrap_or_default();
    people.sort_unstable_by_key(|(id, _)| *id);
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
    // Spatial buckets of the meeting reach: a resident can only meet somebody
    // in its own or a neighbouring bucket, so the pass looks at neighbours
    // instead of at every pair. Candidates keep their identity order, so the
    // first partner found is the one a pass over all pairs would find.
    const BUCKET_MM: i32 = 1_500;
    let mut buckets: std::collections::BTreeMap<(i32, i32), Vec<usize>> =
        std::collections::BTreeMap::new();
    let mut cells = Vec::with_capacity(people.len());
    for (index, (_, actor)) in people.iter().enumerate() {
        let cell = world
            .get::<SimPos>(*actor)
            .map(|pos| (pos.x.div_euclid(BUCKET_MM), pos.z.div_euclid(BUCKET_MM)));
        if let Some(cell) = cell {
            buckets.entry(cell).or_default().push(index);
        }
        cells.push(cell);
    }
    for (index, (a_id, a)) in people.iter().enumerate() {
        let Some((cx, cz)) = cells[index] else {
            continue;
        };
        let mut near: Vec<usize> = (-1..=1)
            .flat_map(|dx| (-1..=1).map(move |dz| (cx + dx, cz + dz)))
            .filter_map(|cell| buckets.get(&cell))
            .flatten()
            .copied()
            .filter(|other| *other > index)
            .collect();
        if near.is_empty() {
            continue;
        }
        near.sort_unstable();
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
        for (b_id, b) in near.into_iter().map(|other| &people[other]) {
            if !economy::near(world, *a, *b)
                // A passer-by does not meet somebody who is indoors.
                || super::super::indoors::inside_entity(world, *a)
                    != super::super::indoors::inside_entity(world, *b)
                || !social_available(world, *b)
                || world
                    .get::<SocialLife>(*b)
                    .expect("social")
                    .last_social_tick
                    .saturating_add(300)
                    > now
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
