//! ADR 0013, game menu plan Task 2: a village saved at tick T and loaded
//! into a fresh app runs exactly like the village that was never saved
//! (state hashes equal at every checkpoint).

use npc_framework::tools::snapshot_check::assert_resume_equivalent;

#[test]
fn resume_equals_uninterrupted_village() {
    for at in [1, 600, 1800, 4000] {
        assert_resume_equivalent(
            || village_content::scenario::new_sim_app(1, "village", 25),
            at,
            1200,
            20,
        );
    }
}

#[test]
fn resume_equals_uninterrupted_village_n_and_nav() {
    for scenario in ["village_n", "village_nav"] {
        assert_resume_equivalent(
            || village_content::scenario::new_sim_app(1, scenario, 6),
            900,
            600,
            20,
        );
    }
}
