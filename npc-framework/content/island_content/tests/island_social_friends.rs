//! Friendship steers meetings (review npc-ai#10): a free resident prefers a
//! friend to a stranger at the same distance, prefers somebody new on its
//! seeded curious days, a bond kept up by meeting does not fade while one
//! nobody keeps up does, and all of it is deterministic and survives a save
//! taken in the middle of a meeting.
mod life_support;

use bevy_app::App;
use bevy_ecs::prelude::*;
use island_content::IslandIds;
use island_content::village::{
    self, Villager,
    colony::{self, Chatting, SocialLife},
    island_save,
    resident::ResidentTraits,
};
use life_support::*;
use npc_framework::kernel::hash::state_hash;
use npc_framework::prelude::*;

const DAY: u64 = 24_000;
/// A friend in the shipped tiers (`economy.live.tiers`: friend from 20000).
const FRIEND: i32 = 25_000;

/// Three founders in a clear field, everybody else asleep: the three are the
/// only company, at `day` and `permille` of that day. The subject has the
/// lowest identity, so it chooses first when the three stand together.
struct Trio {
    app: App,
    ada: (SimId, Entity),
    /// The neighbour with the lower identity, which wins a tie.
    first: (SimId, Entity),
    second: (SimId, Entity),
}

fn trio(day: u64, permille: u64, offsets: [i32; 2]) -> Trio {
    let mut app = island_content::new_sim_app(7, "living_world");
    let now = day * DAY + permille * DAY / 1000;
    app.world_mut().resource_mut::<SimClock>().tick = now;
    let people: Vec<Entity> = app
        .world()
        .resource::<SimIndex>()
        .map
        .values()
        .copied()
        .filter(|actor| app.world().get::<Villager>(*actor).is_some())
        .collect();
    for actor in people {
        app.world_mut()
            .get_mut::<Villager>(actor)
            .unwrap()
            .resting_until = now + DAY;
    }
    let ids = app.world().resource::<IslandIds>();
    let mut three = [ids.id("Ada"), ids.id("Rowan"), ids.id("Nell")];
    three.sort_unstable();
    let [ada, others @ ..] = three;
    let entity = |app: &App, id| app.world().resource::<SimIndex>().get(id).unwrap();
    for (id, x) in [(ada, 0), (others[0], offsets[0]), (others[1], offsets[1])] {
        let actor = entity(&app, id);
        app.world_mut()
            .entity_mut(actor)
            .insert(SimPos::new(x, 0))
            .remove::<MoveIntent>();
        app.world_mut()
            .get_mut::<Villager>(actor)
            .unwrap()
            .resting_until = 0;
        let mut traits = app.world_mut().get_mut::<ResidentTraits>(actor).unwrap();
        traits.sociability = 90;
        traits.chronotype = 0;
        traits.diligence = 50;
    }
    let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
    for z in -20..=20 {
        for x in -20..=20 {
            grid.set_static(CellCoord::new(x, z), CellInfo::default());
        }
    }
    Trio {
        ada: (ada, entity(&app, ada)),
        first: (others[0], entity(&app, others[0])),
        second: (others[1], entity(&app, others[1])),
        app,
    }
}

fn befriend(app: &mut App, actor: Entity, other: SimId, fondness: i32) {
    app.world_mut()
        .get_mut::<SocialLife>(actor)
        .unwrap()
        .affinity
        .insert(other, fondness);
}

/// The first day from 1 on which the subject is (or is not) curious and
/// the evening is not cold: in the cold nobody without a coat takes leisure.
fn day_when(curious: bool) -> u64 {
    let mut app = trio(0, 50, [4000, -4000]);
    (1..200)
        .find(|day| {
            app.app.world_mut().resource_mut::<SimClock>().tick = day * DAY + 660 * DAY / 1000;
            colony::curious_today(app.app.world(), app.ada.0) == curious
                && !village::living::cold(app.app.world())
        })
        .expect("a day")
}

#[test]
fn a_friend_is_preferred_over_a_stranger_at_the_same_distance() {
    let ordinary = day_when(false);
    // Without a bond the tie goes to the lower identity.
    let Trio {
        app, ada, first, ..
    } = trio(ordinary, 660, [4000, -4000]);
    assert_eq!(
        colony::social_target(app.world(), ada.1).map(|(id, _)| id),
        Some(first.0)
    );
    // A friend at the same distance wins over the stranger.
    let Trio {
        mut app,
        ada,
        second,
        ..
    } = trio(ordinary, 660, [4000, -4000]);
    befriend(&mut app, ada.1, second.0, FRIEND);
    let before = state_hash(app.world_mut());
    assert_eq!(
        colony::social_target(app.world(), ada.1).map(|(id, _)| id),
        Some(second.0)
    );
    // The choice is a pure query.
    assert_eq!(state_hash(app.world_mut()), before);
    // A friend is worth a little more walking (`bond_pull_mm` 3000 a tier,
    // two tiers), not an unbounded detour: 9 m against 4 m wins, 12 m does not.
    for (far, friend_wins) in [(9_000, true), (12_000, false)] {
        let Trio {
            mut app,
            ada,
            first,
            second,
        } = trio(ordinary, 660, [4000, -far]);
        befriend(&mut app, ada.1, second.0, FRIEND);
        let chosen = colony::social_target(app.world(), ada.1).map(|(id, _)| id);
        let expected = if friend_wins { second.0 } else { first.0 };
        assert_eq!(chosen, Some(expected), "friend {far} mm away");
    }
}

#[test]
fn on_a_curious_day_somebody_new_comes_first() {
    let curious = day_when(true);
    let Trio {
        mut app,
        ada,
        first,
        second,
    } = trio(curious, 660, [6000, -4000]);
    befriend(&mut app, ada.1, second.0, FRIEND);
    assert_eq!(
        colony::social_target(app.world(), ada.1).map(|(id, _)| id),
        Some(first.0),
        "the farther stranger comes before the nearer friend on a curious day"
    );
}

#[test]
fn standing_together_a_resident_chats_with_its_friend() {
    for (curious, friend_wins) in [(false, true), (true, false)] {
        let Trio {
            mut app,
            ada,
            first,
            second,
        } = trio(day_when(curious), 660, [1_200, -600]);
        befriend(&mut app, ada.1, second.0, FRIEND);
        befriend(&mut app, second.1, ada.0, FRIEND);
        let met = until(&mut app, 120, 1, |app| {
            app.world().get::<Chatting>(ada.1).is_some()
        });
        assert!(met, "Ada met nobody (curious {curious})");
        let with = app.world().get::<Chatting>(ada.1).unwrap().with;
        let expected = if friend_wins { second.0 } else { first.0 };
        assert_eq!(with, expected, "curious {curious}");
        // Under way, the meeting view names the partner and the walk's
        // record is cleared.
        let meeting = village::view::meeting(app.world(), ada.1).expect("a meeting");
        assert!(meeting.under_way && meeting.with == expected);
        assert_eq!(app.world().get::<SocialLife>(ada.1).unwrap().seeking, None);
    }
}

#[test]
fn strangers_are_still_met_at_the_seeded_rate() {
    let mut curious = 0_u32;
    let mut total = 0_u32;
    for seed in [7, 11, 23] {
        let mut app = island_content::new_sim_app(seed, "living_world");
        let people: Vec<SimId> = all_residents(app.world())
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        for day in 0..40 {
            app.world_mut().resource_mut::<SimClock>().tick = day * DAY + 100;
            for id in &people {
                let today = colony::curious_today(app.world(), *id);
                // The same question gets the same answer.
                assert_eq!(today, colony::curious_today(app.world(), *id));
                curious += u32::from(today);
                total += 1;
            }
        }
    }
    let permille = curious * 1000 / total;
    println!("CURIOUS {curious} of {total} resident-days: {permille} permille");
    // `schedule.curious_permille` is 250.
    assert!(
        (220..=280).contains(&permille),
        "{permille} permille curious days"
    );
}

#[test]
fn a_bond_kept_up_by_meeting_does_not_fade_and_a_neglected_one_does() {
    let mut app = world_for(7, |world| short_days(world, 2_000));
    let day = app.world().resource::<SimConfig>().day_length;
    let ada = app.world().resource::<IslandIds>().id("Ada");
    let actor = entity(app.world(), ada);
    let (kept, neglected) = (SimId(900_001), SimId(900_002));
    // Shortly before the sixth morning; calendar days need not start at 0.
    run_ticks(app.world_mut(), 6 * day - 20);
    let today = village::calendar(app.world()).0;
    assert!(today > 3, "day {today}");
    {
        let mut social = app.world_mut().get_mut::<SocialLife>(actor).unwrap();
        social.affinity.insert(kept, 10_000);
        social.affinity.insert(neglected, 10_000);
        social.met_day.insert(kept, today);
        social.met_day.remove(&neglected);
    }
    let fondness = |app: &App, of: SimId| {
        app.world()
            .get::<SocialLife>(actor)
            .unwrap()
            .affinity
            .get(&of)
            .copied()
            .unwrap_or(0)
    };
    // Next morning: met today is within `resident_neglect_after_days` (3);
    // never met fades by `resident_neglect_points_per_day` (100).
    run_ticks(app.world_mut(), 40);
    assert_eq!(village::calendar(app.world()).0, today + 1);
    assert_eq!(fondness(&app, kept), 10_000);
    assert_eq!(fondness(&app, neglected), 9_900);
    // Three mornings on, more than three days have passed since the meeting.
    run_ticks(app.world_mut(), 3 * day);
    assert_eq!(village::calendar(app.world()).0, today + 4);
    assert_eq!(fondness(&app, kept), 9_900);
    assert_eq!(fondness(&app, neglected), 9_600);
}

fn short_world(world: &mut serde_json::Value) {
    short_days(world, 4_000);
}

#[test]
fn meetings_are_deterministic_and_a_save_mid_meeting_continues() {
    let mut a = world_for(11, short_world);
    let mut b = world_for(11, short_world);
    // Run to the first meeting under way, in both.
    let chatting = |app: &mut App| {
        app.world_mut()
            .query::<&Chatting>()
            .iter(app.world())
            .next()
            .is_some()
    };
    assert!(until(&mut a, 12_000, 10, chatting), "nobody met");
    let at = a.world().resource::<SimClock>().tick;
    run_ticks(b.world_mut(), at);
    assert_eq!(state_hash(a.world_mut()), state_hash(b.world_mut()));
    // Save in the middle of the meeting and continue as Continue does.
    let saved = island_save::save_island(a.world_mut()).expect("saved");
    let mut fresh = world_for(11, short_world);
    island_save::load_island_save(fresh.world_mut(), &saved).expect("loads");
    assert_eq!(state_hash(fresh.world_mut()), state_hash(a.world_mut()));
    for _ in 0..30 {
        run_ticks(a.world_mut(), 20);
        run_ticks(b.world_mut(), 20);
        run_ticks(fresh.world_mut(), 20);
        let hash = state_hash(a.world_mut());
        assert_eq!(hash, state_hash(b.world_mut()));
        assert_eq!(hash, state_hash(fresh.world_mut()));
    }
    // The meeting paid and was remembered, in every run alike.
    let met: usize = all_residents(a.world())
        .iter()
        .filter_map(|(_, actor)| a.world().get::<SocialLife>(*actor))
        .map(|social| social.met_day.len())
        .sum();
    assert!(met > 0, "no finished meeting was remembered");
}

#[test]
fn the_meeting_view_names_the_friend_the_leaf_set_out_to_meet() {
    let Trio {
        mut app,
        ada,
        second,
        ..
    } = trio(day_when(false), 660, [4000, -4000]);
    befriend(&mut app, ada.1, second.0, FRIEND);
    let walking = until(&mut app, 60, 1, |app| {
        village::resident::label(app.world(), ada.1) == Some("meeting a neighbour")
    });
    assert!(walking, "Ada never set out to meet anybody");
    // Recorded by the leaf, read without a search.
    assert_eq!(
        app.world().get::<SocialLife>(ada.1).unwrap().seeking,
        Some(second.0)
    );
    let meeting = village::view::meeting(app.world(), ada.1).expect("a meeting");
    assert_eq!(meeting.with, second.0);
    assert_eq!(meeting.tier, "friend");
}

#[test]
fn outside_free_time_the_nearest_company_wins_and_nobody_walks_to_a_friend() {
    // Morning, in working hours: the tree's last resort ("Seek company") is
    // a pass-time, not friendship.
    let ordinary = day_when(false);
    let Trio {
        mut app,
        ada,
        first,
        second,
    } = trio(ordinary, 50, [4000, -9000]);
    befriend(&mut app, ada.1, second.0, FRIEND);
    assert_eq!(
        colony::social_target(app.world(), ada.1).map(|(id, _)| id),
        Some(first.0),
        "the nearer stranger, not the friend 9 m away"
    );
    // A friend 30 m away, nobody else in reach: sought in free time only.
    let far = |permille| {
        let Trio {
            mut app,
            ada,
            first,
            second,
        } = trio(ordinary, permille, [-30_000, 30_000]);
        befriend(&mut app, ada.1, second.0, FRIEND);
        let _ = first;
        let mut grid = app.world_mut().resource_mut::<SpatialGrid>();
        for z in -2..=2 {
            for x in -35..=35 {
                grid.set_static(CellCoord::new(x, z), CellInfo::default());
            }
        }
        colony::social_target(app.world(), ada.1).map(|(id, _)| id) == Some(second.0)
    };
    assert!(far(660), "in free time a friend 30 m away is sought out");
    assert!(!far(50), "in working hours nobody walks 30 m to a friend");
}
