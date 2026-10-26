//! Visual smoke test for `village_view` without `NpcPlugins`/`VillagePlugin`.
//!
//! Inserts `SimClock`/`SimConfig` itself and spawns sim entities directly.
//! A tiny stand-in `NpcSimTick` schedule (run by the kernel's driver alone)
//! walks the villager between the places' approach points so interpolation,
//! yaw and the walk cycle can be checked.
//!
//! ```text
//! cargo run -p village_view --example view_smoke -- [--screenshot PATH] [--after-secs S]
//!     [--tick N] [--activity idle|walk|work|eat|sleep] [--food N] [--speed N] [--no-tick]
//!     [--camera X,Z,DISTANCE]
//! ```
//! `--no-tick` installs no `NpcSimTick` schedule at all (driver must skip it).

use std::path::PathBuf;

use bevy::prelude::*;
use npc_framework::kernel::NpcDriverPlugin;
use npc_framework::needs::NeedRegistry;
use npc_framework::prelude::{
    AiRegistry, Approach, BbValue, Blackboard, Brain, BrainState, Needs, NpcSimTick, ParamValue,
    PrevSimPos, Reservations, SimClock, SimConfig, SimControl, SimId, SimIndex, SimPos, Status,
    StopReason, TreeBuilder,
};
use village_content::actions::{Sleep, SleepUntil};
use village_content::needs::{ENERGY_DEF, HUNGER, HUNGER_DEF};
use village_content::{Activity, Bed, Home, Place, PlaceKind, Stockpile, Villager, WorkSite};
use village_view::{BB_DEST, BB_TARGET, RtsCamera, ScreenshotRequest, VillageViewPlugin};

struct Args {
    screenshot: Option<PathBuf>,
    after_secs: f32,
    tick: u64,
    activity: Activity,
    food: u32,
    speed: u32,
    no_tick: bool,
    /// Camera override: focus x, focus z, distance (metres).
    camera: Option<(f32, f32, f32)>,
}

fn parse_args() -> Args {
    let mut args = Args {
        screenshot: None,
        after_secs: 3.0,
        tick: 300,
        activity: Activity::Walk,
        food: 4,
        speed: 1,
        no_tick: false,
        camera: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| panic!("missing value for {a}"));
        match a.as_str() {
            "--screenshot" => args.screenshot = Some(PathBuf::from(value())),
            "--after-secs" => {
                args.after_secs = value().parse().expect("--after-secs takes seconds")
            }
            "--tick" => args.tick = value().parse().expect("--tick takes a number"),
            "--food" => args.food = value().parse().expect("--food takes a number"),
            "--speed" => args.speed = value().parse().expect("--speed takes a number"),
            "--activity" => {
                args.activity = match value().as_str() {
                    "idle" => Activity::Idle,
                    "walk" => Activity::Walk,
                    "work" => Activity::Work,
                    "eat" => Activity::Eat,
                    "sleep" => Activity::Sleep,
                    "drink" => Activity::Drink,
                    other => panic!("unknown activity {other}"),
                }
            }
            "--no-tick" => args.no_tick = true,
            "--camera" => {
                let v: Vec<f32> = value()
                    .split(',')
                    .map(|n| n.parse().expect("--camera x,z,distance"))
                    .collect();
                args.camera = Some((v[0], v[1], v[2]));
            }
            other => panic!("unknown argument {other}"),
        }
    }
    args
}

/// Approach points the stand-in walker visits in order.
const ROUTE: [(u32, SimPos); 4] = [
    (4, SimPos::new(3000, 2200)),
    (3, SimPos::new(5000, -2800)),
    (2, SimPos::new(-2500, -4000)),
    (1, SimPos::new(-6000, -2600)),
];

/// The hunger id (the stand-in tick decays every need a little).
#[derive(Resource)]
struct SmokeNeeds(npc_framework::needs::NeedId);

#[derive(Resource)]
struct SmokeSetup {
    activity: Activity,
    food: u32,
}

fn main() {
    let args = parse_args();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "npc-framework — view_smoke".into(),
                    resolution: (1280, 720).into(),
                    ..default()
                }),
                ..default()
            })
            .set(village_view::asset_plugin()),
    )
    .insert_resource(SimControl {
        multiplier: args.speed,
        ..default()
    })
    .add_plugins((NpcDriverPlugin::new(20, Update), VillageViewPlugin))
    .insert_resource(SimClock { tick: args.tick })
    .insert_resource(SimConfig::default())
    // Harmless here; Task B's `SimId` hooks keep it current.
    .init_resource::<SimIndex>()
    .insert_resource(SmokeSetup {
        activity: args.activity,
        food: args.food,
    })
    .add_systems(Startup, spawn_sim_entities);
    if let Some((x, z, distance)) = args.camera {
        app.add_systems(PostStartup, move |mut cams: Query<&mut RtsCamera>| {
            for mut cam in &mut cams {
                cam.focus = Vec3::new(x, 0.0, z);
                cam.distance = distance;
            }
        });
    }
    if !args.no_tick {
        app.add_systems(NpcSimTick, smoke_tick);
    }
    if let Some(path) = args.screenshot {
        app.insert_resource(ScreenshotRequest {
            path,
            after_secs: args.after_secs,
        });
    }
    app.run();
}

fn spawn_sim_entities(mut commands: Commands, setup: Res<SmokeSetup>) {
    let house = SimId(1);
    commands.spawn((
        house,
        Place {
            kind: PlaceKind::House,
            yaw_deg: 0,
        },
        SimPos::new(-6000, -5000),
        Approach(SimPos::new(-6000, -2600)),
        Home,
    ));
    commands.spawn((
        SimId(2),
        Place {
            kind: PlaceKind::Bed,
            yaw_deg: 90,
        },
        SimPos::new(-2500, -5500),
        Approach(SimPos::new(-2500, -4000)),
        Bed { home: house },
    ));
    commands.spawn((
        SimId(3),
        Place {
            kind: PlaceKind::Stockpile,
            yaw_deg: 0,
        },
        SimPos::new(5000, -4000),
        Approach(SimPos::new(5000, -2800)),
        Stockpile {
            food: setup.food,
            capacity: 10,
        },
    ));
    commands.spawn((
        SimId(4),
        Place {
            kind: PlaceKind::Field,
            yaw_deg: 0,
        },
        SimPos::new(3000, 5000),
        Approach(SimPos::new(3000, 2200)),
        WorkSite {
            work_ticks: 300,
            yield_food: 3,
            stockpile: SimId(3),
        },
    ));

    // A fake tree so the overlay shows a path.
    fn run(_: &mut World, _: Entity, _: &ParamValue) -> Status {
        Status::Running
    }
    fn stop(_: &mut World, _: Entity, _: StopReason) {}
    let mut registry = AiRegistry::default();
    registry.add_action("MoveTo", run, stop);
    let mut b = TreeBuilder::new("smoke");
    b.selector("Root", |b| {
        b.sequence("Work", |b| {
            b.leaf("MoveTo", "MoveTo", ());
        });
    });
    registry.add_tree(b.build());
    registry.freeze();
    let tree = registry.tree_id("smoke").expect("smoke tree");
    commands.insert_resource(registry);
    // The village's needs, so the overlay can name them.
    let mut needs = NeedRegistry::default();
    needs.add(HUNGER_DEF);
    needs.add(ENERGY_DEF);
    needs.freeze();
    let fresh = needs.full();
    let hunger = needs.id(HUNGER).expect("hunger");
    commands.insert_resource(needs);
    commands.insert_resource(SmokeNeeds(hunger));
    let mut blackboard = Blackboard::default();
    blackboard.set(BB_TARGET, BbValue::U32(ROUTE[0].0));
    blackboard.set(BB_DEST, BbValue::Pos(ROUTE[0].1.x, ROUTE[0].1.z));
    let start = SimPos::new(0, 0);
    let mut villager = commands.spawn((
        SimId(5),
        Villager {
            home: house,
            speed: 60,
        },
        setup.activity,
        start,
        PrevSimPos(start),
        fresh,
        Brain { tree },
        BrainState {
            running_path: vec![0, 1, 2],
            mem: vec![],
        },
        blackboard,
    ));
    if setup.activity == Activity::Sleep {
        // The view lies the villager in the bed named by its Sleep action.
        villager.insert(Sleep {
            bed: SimId(2),
            until: SleepUntil::Morning,
        });
    }
    let mut reservations = Reservations::default();
    reservations.claims.insert(SimId(4), vec![SimId(5)]);
    commands.insert_resource(reservations);
}

/// Stand-in for the real sim: walk the villager along `ROUTE`, decay needs a
/// little, advance the clock.
fn smoke_tick(
    mut clock: ResMut<SimClock>,
    smoke: Res<SmokeNeeds>,
    mut reservations: ResMut<Reservations>,
    mut villagers: Query<(
        &SimId,
        &Villager,
        &Activity,
        &mut SimPos,
        &mut PrevSimPos,
        &mut Blackboard,
        &mut Needs,
    )>,
) {
    for (id, villager, activity, mut pos, mut prev, mut bb, mut needs) in &mut villagers {
        prev.0 = *pos;
        for (i, need) in needs.values.iter_mut().enumerate() {
            let decay = if i == smoke.0.0 as usize { 40 } else { 25 };
            need.value = (need.value - decay).max(0);
        }
        if *activity != Activity::Walk {
            continue;
        }
        let BbValue::Pos(gx, gz) = bb.get(BB_DEST) else {
            continue;
        };
        let (dx, dz) = ((gx - pos.x) as f64, (gz - pos.z) as f64);
        let dist = (dx * dx + dz * dz).sqrt();
        let step = villager.speed as f64;
        if dist <= step {
            *pos = SimPos::new(gx, gz);
            let i = ROUTE
                .iter()
                .position(|(_, p)| *p == *pos)
                .map_or(0, |i| (i + 1) % ROUTE.len());
            let (target, next) = ROUTE[i];
            bb.set(BB_TARGET, BbValue::U32(target));
            bb.set(BB_DEST, BbValue::Pos(next.x, next.z));
            reservations.claims.clear();
            reservations.claims.insert(SimId(target), vec![*id]);
        } else {
            pos.x += (dx / dist * step).round() as i32;
            pos.z += (dz / dist * step).round() as i32;
        }
    }
    clock.tick += 1;
}
