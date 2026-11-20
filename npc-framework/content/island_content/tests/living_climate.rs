use island_content::village::{self};
use npc_framework::prelude::*;

#[test]
fn living_world_snapshot_keeps_climate_and_market_state() {
    npc_framework::tools::snapshot_check::assert_resume_equivalent(
        || island_content::new_sim_app(7, "living_world"),
        400,
        1600,
        40,
    );
}

#[test]
fn observer_calendar_matches_the_actual_late_autumn_start_and_winter_boundary() {
    let app = island_content::new_sim_app(7, "living_world");
    let config = app.world().resource::<SimConfig>();
    assert_eq!(config.day_length, 24_000);
    for (tick, expected) in [
        (0, (19, village::Season::Autumn, false)),
        (17_001, (19, village::Season::Autumn, true)),
        (48_000, (21, village::Season::Winter, false)),
    ] {
        assert_eq!(village::calendar_at(tick, config, true), expected);
    }
    assert_eq!(
        village::calendar_at(0, config, false),
        (0, village::Season::Spring, false),
        "the fixed island retains its original spring calendar"
    );
}
