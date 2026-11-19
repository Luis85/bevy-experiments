//! Proximity meetings, friendship and morale. Social state never funds
//! production; walking to company is the resident action's (`life.socialise`).
use super::super::{economy, living};
use super::{ColonyState, SocialLife, home_of};
use bevy_ecs::prelude::*;
use npc_framework::kernel::rng::sim_rand;
use npc_framework::prelude::*;
use serde_json::json;

pub(super) fn welcome(world: &mut World, actor: Entity) {
    if world.get::<SocialLife>(actor).is_none() {
        world.entity_mut(actor).insert(SocialLife::default());
    }
}

/// Seeds the day on which a resident prefers somebody new.
const CURIOUS_PURPOSE: u32 = 0x534f_4301;

/// Whom a resident prefers to meet (`behavior.schedule`, review npc-ai#10).
#[derive(Clone, Copy)]
struct Steer {
    pull_mm: i64,
    curious_permille: u64,
    friend_tier: usize,
    seek_mm: i64,
}

fn steer(world: &World) -> Steer {
    let default = super::super::resident::ScheduleSpec::default();
    let schedule = super::super::resident::spec(world).map_or(&default, |spec| &spec.schedule);
    Steer {
        pull_mm: i64::from(schedule.bond_pull_mm),
        curious_permille: u64::from(schedule.curious_permille),
        friend_tier: schedule.friend_tier as usize,
        seek_mm: i64::from(schedule.seek_friend_mm),
    }
}

/// Friendship steers only free time: in its leisure block, or at a gathering
/// of the calendar. Otherwise (the tree's last resort, "Seek company", in
/// working hours) company is the nearest, as before, and nobody walks off to
/// a friend: a pass-time must not draw a resident away from its work.
fn steer_for(world: &World, actor: Entity) -> Option<Steer> {
    (super::super::resident::in_block(world, actor, super::super::resident::BlockKind::Leisure)
        || super::super::live::attending(world, actor))
    .then(|| steer(world))
}

/// Ranking without friendship: nearest first, then the identity.
const UNSTEERED: Steer = Steer {
    pull_mm: 0,
    curious_permille: 0,
    friend_tier: usize::MAX,
    seek_mm: 0,
};

/// The bond tier (`economy.live.tiers`, by index) `actor` holds towards
/// `other`: its own fondness, which meetings pay to both alike.
pub fn bond_tier(world: &World, actor: Entity, other: SimId) -> usize {
    let fondness = world
        .get::<SocialLife>(actor)
        .and_then(|social| social.affinity.get(&other).copied())
        .unwrap_or(0);
    super::super::live::tier_index(world, fondness)
}

/// Whether today is one of `id`'s curious days, on which it prefers
/// somebody it is not yet friends with (`schedule.curious_permille`).
pub fn curious_today(world: &World, id: SimId) -> bool {
    let permille = steer(world).curious_permille;
    let day = super::super::calendar(world).0;
    permille > 0
        && sim_rand(world.resource::<WorldSeed>().0, id, day, CURIOUS_PURPOSE) % 1000 < permille
}

/// Ranking key of a partner, lowest first: each tier of the bond counts as
/// `bond_pull_mm` less distance; on a curious day non-friends come first,
/// nearest first. The key ends in the identity.
fn rank(
    steer: Steer,
    curious: bool,
    tier: usize,
    distance_mm: i64,
    id: SimId,
) -> (bool, i64, SimId) {
    if curious {
        (tier >= steer.friend_tier, distance_mm, id)
    } else {
        (false, distance_mm - steer.pull_mm * tier as i64, id)
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
    let day = super::super::calendar(world).0;
    let (break_mm, pay) = super::super::resident::spec(world).map_or((3000, 1500), |spec| {
        (spec.schedule.chat_break_mm, spec.schedule.meet_affinity)
    });
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
                *affinity = (*affinity + pay).min(100_000);
                social.met_day.insert(partner, day);
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

/// An unsociable resident spends its free time alone: in its leisure it is
/// nobody's company, neither for a chat in passing nor for a neighbour who
/// seeks company (`life.sociable` decides whether the evening is spent in
/// company). S4: without this a loner strolling to the square was met as
/// often as a sociable resident seeking company, and the trait had no effect.
fn spends_free_time_alone(world: &World, actor: Entity) -> bool {
    super::super::resident::spec(world).is_some()
        && world
            .get::<super::super::behavior::LifeDecision>(actor)
            .is_some_and(|decision| decision.goal == super::super::behavior::LifeGoal::Leisure)
        && !super::super::resident::sociable(world, actor)
}

/// Leisure remains a low-priority action beneath survival and funded work.
/// Friendship steers the choice (review npc-ai#10): among free company in
/// walking distance (15 m) a closer bond counts as nearer, by
/// `schedule.bond_pull_mm` a tier, except on a curious day, when somebody
/// who is not yet a friend comes first. With nobody in walking distance a
/// sociable resident walks to a free friend up to `schedule.seek_friend_mm`.
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
    let steered = steer_for(world, actor);
    let steer = steered.unwrap_or(UNSTEERED);
    let curious = steered.is_some() && curious_today(world, *me);
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
            if id == me || direct > 15_000_i64.pow(2) || !free_company(world, actor, *entity) {
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
            let tier = bond_tier(world, actor, *id);
            Some((rank(steer, curious, tier, distance, *id), target))
        })
        .min_by_key(|(key, _)| *key)
        .map(|((_, _, id), target)| (id, target))
        .or_else(|| seek_friend(world, actor, *pos, steer))
}

/// Whether `other` is company for `actor` now: free, not spending its free
/// time alone, and not inside a building `actor` is not in.
fn free_company(world: &World, actor: Entity, other: Entity) -> bool {
    if !social_available(world, other) || spends_free_time_alone(world, other) {
        return false;
    }
    // Somebody inside a building is company only for those inside it.
    let indoors = super::super::indoors::inside_entity(world, other);
    indoors.is_none() || indoors == super::super::indoors::inside_entity(world, actor)
}

/// A free friend (`schedule.friend_tier` or closer) within
/// `schedule.seek_friend_mm` of walking, for a sociable resident with nobody
/// in walking distance. Only the resident's own bonds are looked at.
fn seek_friend(world: &World, actor: Entity, pos: SimPos, steer: Steer) -> Option<(SimId, SimPos)> {
    if steer.seek_mm <= 0 || !super::super::resident::sociable(world, actor) {
        return None;
    }
    let social = world.get::<SocialLife>(actor)?;
    social
        .affinity
        .iter()
        .filter_map(|(id, fondness)| {
            let tier = super::super::live::tier_index(world, *fondness);
            if tier < steer.friend_tier {
                return None;
            }
            let friend = world.resource::<SimIndex>().get(*id)?;
            let target = world.get::<SimPos>(friend).copied()?;
            if pos.dist2(target) > steer.seek_mm.pow(2)
                || !living(world, friend)
                || !free_company(world, actor, friend)
            {
                return None;
            }
            let route = super::super::frontier::routes::estimate(world, pos, target)?;
            let distance = i64::from(route.distance_mm);
            // The walk and the meeting end before its own bedtime (review
            // npc-ai#11), or it does not set out.
            let speed = world
                .get::<super::super::Villager>(actor)
                .map_or(1, |person| person.base_speed.max(1)) as u64;
            let chat =
                super::super::resident::spec(world).map_or(200, |spec| spec.schedule.chat_ticks);
            let errand = u64::from(route.distance_mm).div_ceil(speed) + chat;
            (distance <= steer.seek_mm
                && super::super::resident::fits_before_bed(world, actor, errand))
            .then(|| (rank(steer, false, tier, distance, *id), target))
        })
        .min_by_key(|(key, _)| *key)
        .map(|((_, _, id), target)| (id, target))
}

/// Whom `actor` is meeting: the partner of a meeting under way (`true`), or
/// the one it walks to for a meeting (`false`). For presentation.
pub fn meeting_partner(world: &World, actor: Entity) -> Option<(SimId, bool)> {
    if let Some(chat) = world.get::<Chatting>(actor) {
        return Some((chat.with, true));
    }
    (super::super::resident::label(world, actor) == Some("meeting a neighbour"))
        .then(|| social_target(world, actor))
        .flatten()
        .map(|(id, _)| (id, false))
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
    let cooldown =
        super::super::resident::spec(world).map_or(2400, |spec| spec.schedule.meet_cooldown_ticks);
    // Spatial buckets of the meeting reach: a resident can only meet somebody
    // in its own or a neighbouring bucket, so the pass looks at neighbours
    // instead of at every pair. Residents choose in identity order, each the
    // partner it ranks first among those within reach (`rank`: the bond,
    // then the distance, then the identity).

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
            || spends_free_time_alone(world, *a)
            || !social_available(world, *a)
            || world
                .get::<SocialLife>(*a)
                .expect("social")
                .last_social_tick
                .saturating_add(cooldown)
                > now
        {
            continue;
        }
        let steered = steer_for(world, *a);
        let steer = steered.unwrap_or(UNSTEERED);
        let curious = steered.is_some() && curious_today(world, *a_id);
        let chosen = near
            .into_iter()
            .map(|other| &people[other])
            .filter(|(_, b)| {
                economy::near(world, *a, *b)
                    // A passer-by does not meet somebody who is indoors.
                    && super::super::indoors::inside_entity(world, *a)
                        == super::super::indoors::inside_entity(world, *b)
                    && !on_the_way(*b)
                    && !spends_free_time_alone(world, *b)
                    && social_available(world, *b)
                    && world
                        .get::<SocialLife>(*b)
                        .expect("social")
                        .last_social_tick
                        .saturating_add(cooldown)
                        <= now
            })
            .filter_map(|(b_id, b)| {
                let apart = world
                    .get::<SimPos>(*a)
                    .zip(world.get::<SimPos>(*b))
                    .map(|(here, there)| here.dist2(*there).isqrt())?;
                let tier = bond_tier(world, *a, *b_id);
                Some((rank(steer, curious, tier, apart, *b_id), *b, tier))
            })
            .min_by_key(|(key, _, _)| *key);
        let Some(((_, _, b_id), b, tier)) = chosen else {
            continue;
        };
        // A meeting lasts `schedule.chat_ticks` and pays at its end.
        let until = now.saturating_add(
            super::super::resident::spec(world).map_or(200, |spec| spec.schedule.chat_ticks),
        );
        for (person, other) in [(*a, b_id), (b, *a_id)] {
            let mut social = world.get_mut::<SocialLife>(person).expect("social");
            social.last_social_tick = now;
            social.seeking = None;
            world
                .entity_mut(person)
                .insert(Chatting { with: other, until });
        }
        record_effect(
            world,
            Some(*a_id),
            "VillageSocialMeetingStarted",
            &json!({"with":b_id,"until":until,"tier":tier,"curious":curious}),
        );
    }
}
