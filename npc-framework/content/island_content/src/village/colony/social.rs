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
/// A meeting under way (S3.3): both parties stand together until `until`,
/// when the meeting pays; walking apart or a critical need ends it early,
/// paying nothing.
#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    bevy_reflect::Reflect,
)]
#[reflect(Component)]
pub struct Chatting {
    pub with: SimId,
    pub until: u64,
}
impl npc_framework::prelude::SimHash for Chatting {
    const NAME: &'static str = "island_chatting_v1";
    fn sim_hash(&self, h: &mut npc_framework::prelude::Fnv64) {
        h.u32(self.with.0);
        h.u64(self.until);
    }
}

/// Ends or pays the meetings under way, lower identity first.
fn settle_meetings(world: &mut World, now: u64) {
    let mut chats: Vec<(SimId, Entity, Chatting)> = world
        .try_query::<(Entity, &SimId, &Chatting)>()
        .map(|mut query| {
            query
                .iter(world)
                .map(|(e, id, chat)| (*id, e, *chat))
                .collect()
        })
        .unwrap_or_default();
    chats.sort_unstable_by_key(|(id, _, _)| *id);
    let (break_mm,) =
        (super::super::resident::spec(world).map_or(3000, |spec| spec.schedule.chat_break_mm),);
    let critical = super::super::policy::active(world).map_or_else(
        || super::super::policy::IslandAiPolicy::default().critical_need,
        |policy| policy.critical_need,
    );
    for (id, actor, chat) in chats {
        if world.get::<Chatting>(actor).is_none() || chat.with < id {
            continue;
        }
        let other = world.resource::<SimIndex>().get(chat.with);
        let apart = other.is_none_or(|other| {
            !living(world, other)
                || world
                    .get::<SimPos>(actor)
                    .zip(world.get::<SimPos>(other))
                    .is_none_or(|(a, b)| a.dist2(*b) > i64::from(break_mm).pow(2))
        });
        let needy = [Some(actor), other].into_iter().flatten().any(|person| {
            ["Hunger", "Thirst", "Energy", "Warmth"]
                .iter()
                .any(|need| economy::need(world, person, need) < critical)
        });
        if !apart && !needy && chat.until > now {
            continue;
        }
        let paid = !apart && !needy;
        for (person, partner) in [(Some(actor), chat.with), (other, id)] {
            let Some(person) = person else {
                continue;
            };
            world.entity_mut(person).remove::<Chatting>();
            if paid && let Some(mut social) = world.get_mut::<SocialLife>(person) {
                social.morale = (social.morale + 1000).min(100_000);
                let affinity = social.affinity.entry(partner).or_default();
                *affinity = (*affinity + 250).min(100_000);
            }
        }
        if paid {
            world.resource_mut::<ColonyState>().social_events += 1;
        }
        record_effect(
            world,
            Some(id),
            "VillageSocialMeeting",
            &json!({"with":chat.with,"paid":paid}),
        );
    }
}

pub fn social_available(world: &World, actor: Entity) -> bool {
    if !world.contains_resource::<ColonyState>()
        || !living(world, actor)
        || world.get::<Chatting>(actor).is_some()
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
            let distance = {
                let route = super::super::frontier::routes::estimate(world, *pos, target)?;
                // Leisure is local in walking distance too. A person across a
                // pond must not keep winning over reachable nearby company.
                if route.distance_mm > 15_000 {
                    return None;
                }
                i64::from(route.distance_mm)
            };
            Some((distance, *id, target))
        })
        .min_by_key(|(distance, id, _)| (*distance, *id))
        .map(|(_, id, target)| (id, target))
}

pub(super) fn tick(world: &mut World, now: u64) {
    settle_meetings(world, now);
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
        // A meeting lasts (S3.3), so it starts only between residents who are
        // not on an errand: standing still, or walking in their free time
        // (a stroll, seeking company). Nobody's errand is stopped by a chat,
        // and nobody walking to the village's gathering stops on the way.
        let on_the_way = |entity: Entity| {
            world
                .get::<npc_framework::prelude::MoveIntent>(entity)
                .is_some()
                && (super::super::live::attending(world, entity)
                    || world
                        .get::<super::super::behavior::LifeDecision>(entity)
                        .is_none_or(|decision| {
                            decision.goal != super::super::behavior::LifeGoal::Leisure
                        }))
        };
        if on_the_way(*a)
            || !social_available(world, *a)
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
                || on_the_way(*b)
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
            // A meeting lasts `schedule.chat_ticks` and pays at its end.
            let until = now.saturating_add(
                super::super::resident::spec(world).map_or(200, |spec| spec.schedule.chat_ticks),
            );
            for (person, other) in [(*a, *b_id), (*b, *a_id)] {
                world
                    .get_mut::<SocialLife>(person)
                    .expect("social")
                    .last_social_tick = now;
                world
                    .entity_mut(person)
                    .insert(Chatting { with: other, until });
            }
            record_effect(
                world,
                Some(*a_id),
                "VillageSocialMeetingStarted",
                &json!({"with":b_id,"until":until}),
            );
            break;
        }
    }
}
