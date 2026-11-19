//! Social probe of the shipped island (ignored; run in release): who meets
//! whom over PROBE_DAYS (default 14) on PROBE_SEEDS (default 7,11,23).
//! Prints one `SOCIAL_PROBE` line per seed: meetings per resident-day, the
//! share of meetings by the bond tier at their start (the resident's own
//! fondness for the partner), distinct partners per resident, residents with
//! at least one friend at the end, and the unsociable residents' meetings and
//! the share of their leisure spent alone. Asserts nothing.
//!
//! ```sh
//! CARGO_INCREMENTAL=0 cargo test --locked --release -p island_content \
//!   --test island_social_probe -- --ignored --nocapture
//! ```
use bevy_ecs::prelude::*;
use island_content::village::{
    self, Villager,
    behavior::{LifeDecision, LifeGoal},
    colony::{Chatting, SocialLife},
    live, resident,
};
use npc_framework::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// Shorter than the shortest meeting, so no meeting is missed.
const SAMPLE: u64 = 100;

fn env_list(name: &str, default: &[u64]) -> Vec<u64> {
    std::env::var(name)
        .ok()
        .map(|text| {
            text.split(',')
                .filter_map(|part| part.trim().parse().ok())
                .collect()
        })
        .unwrap_or_else(|| default.to_vec())
}

fn residents(world: &World) -> Vec<(SimId, Entity)> {
    let mut out: Vec<(SimId, Entity)> = world
        .resource::<SimIndex>()
        .map
        .iter()
        .filter(|(_, entity)| {
            world
                .get::<Villager>(**entity)
                .is_some_and(|person| person.role != village::Role::Visitor)
                && world.get::<SocialLife>(**entity).is_some()
                && village::living(world, **entity)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect();
    out.sort_unstable_by_key(|(id, _)| *id);
    out
}

#[derive(Default)]
struct Probe {
    resident_samples: u64,
    /// Meetings by participant, by the tier index at their start.
    by_tier: BTreeMap<usize, u64>,
    partners: BTreeMap<SimId, BTreeSet<SimId>>,
    seen: BTreeSet<(SimId, SimId, u64)>,
    unsociable_samples: u64,
    unsociable_meetings: u64,
    unsociable_leisure: u64,
    unsociable_leisure_alone: u64,
    sociable_samples: u64,
    sociable_meetings: u64,
}

fn probe(seed: u64, days: u64) -> String {
    let mut app = island_content::new_sim_app(seed, "living_world");
    let day = app.world().resource::<SimConfig>().day_length;
    let end = days * day;
    let mut probe = Probe::default();
    while app.world().resource::<SimClock>().tick < end {
        run_ticks(app.world_mut(), SAMPLE);
        let world = app.world();
        for (id, actor) in residents(world) {
            probe.resident_samples += 1;
            let sociable = resident::sociable(world, actor);
            if sociable {
                probe.sociable_samples += 1;
            } else {
                probe.unsociable_samples += 1;
            }
            let chat = world.get::<Chatting>(actor).copied();
            if !sociable
                && world
                    .get::<LifeDecision>(actor)
                    .is_some_and(|decision| decision.goal == LifeGoal::Leisure)
            {
                probe.unsociable_leisure += 1;
                probe.unsociable_leisure_alone += u64::from(chat.is_none());
            }
            let Some(chat) = chat else {
                continue;
            };
            if !probe.seen.insert((id, chat.with, chat.until)) {
                continue;
            }
            let fondness = world
                .get::<SocialLife>(actor)
                .and_then(|social| social.affinity.get(&chat.with).copied())
                .unwrap_or(0);
            *probe
                .by_tier
                .entry(live::tier_of(world, fondness).0)
                .or_default() += 1;
            probe.partners.entry(id).or_default().insert(chat.with);
            if sociable {
                probe.sociable_meetings += 1;
            } else {
                probe.unsociable_meetings += 1;
            }
        }
    }
    let world = app.world();
    let people = residents(world);
    let living: BTreeSet<SimId> = people.iter().map(|(id, _)| *id).collect();
    let with_friend = people
        .iter()
        .filter(|(_, actor)| {
            world.get::<SocialLife>(*actor).is_some_and(|social| {
                social.affinity.iter().any(|(other, score)| {
                    live::tier_of(world, *score).0 >= 2 && living.contains(other)
                })
            })
        })
        .count();
    let meetings: u64 = probe.by_tier.values().sum();
    let resident_days = probe.resident_samples * SAMPLE / day;
    let per_mille = |part: u64, whole: u64| part * 1000 / whole.max(1);
    let tiers: Vec<String> = (0..4)
        .map(|tier| {
            let count = probe.by_tier.get(&tier).copied().unwrap_or(0);
            format!("{count} ({}‰)", per_mille(count, meetings))
        })
        .collect();
    let distinct: u64 = probe.partners.values().map(|set| set.len() as u64).sum();
    format!(
        "SOCIAL_PROBE seed={seed} days={days} residents_end={} resident_days={resident_days} \
         meetings={meetings} per_resident_day_x1000={} stranger/acquaintance/friend/close=[{}] \
         distinct_partners_per_resident_x100={} with_friend_end={}/{} \
         sociable_meetings_per_day_x1000={} unsociable_meetings_per_day_x1000={} \
         unsociable_leisure_alone={}‰ ({} of {} samples) hash={:016x}",
        people.len(),
        meetings * 1000 / resident_days.max(1),
        tiers.join(", "),
        distinct * 100 / (probe.partners.len().max(people.len()) as u64).max(1),
        with_friend,
        people.len(),
        probe.sociable_meetings * 1000 * day / (probe.sociable_samples * SAMPLE).max(1),
        probe.unsociable_meetings * 1000 * day / (probe.unsociable_samples * SAMPLE).max(1),
        per_mille(probe.unsociable_leisure_alone, probe.unsociable_leisure),
        probe.unsociable_leisure_alone,
        probe.unsociable_leisure,
        npc_framework::kernel::hash::state_hash(app.world_mut()),
    )
}

#[test]
#[ignore = "social probe over PROBE_DAYS (default 14) on PROBE_SEEDS (default 7,11,23); run in release"]
fn social_probe_who_meets_whom() {
    let days = std::env::var("PROBE_DAYS")
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(14);
    let runs: Vec<_> = env_list("PROBE_SEEDS", &[7, 11, 23])
        .into_iter()
        .map(|seed| std::thread::spawn(move || probe(seed, days)))
        .collect();
    for run in runs {
        println!("{}", run.join().expect("probe"));
    }
}
