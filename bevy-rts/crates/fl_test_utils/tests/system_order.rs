//! The `SimTick` system order of the game's feature set (spec §5.6, §12.7, ADR-0011): one
//! `<hook or sync set> <system>` line per system, in executed order, pinned by an insta
//! snapshot. A new, moved or reordered system shows up as a diff the lead blesses.

use fl_sim::{MatchSetup, RunConfig, build_sim_app, system_order};
use fl_test_utils::base_content;

#[test]
fn smoke_sim_tick_system_order() {
    let mut app = build_sim_app(
        &fl_catalog::features(),
        base_content(),
        MatchSetup::default(),
        RunConfig::default(),
    )
    .expect("the sim builds");
    insta::assert_snapshot!("sim_tick", system_order(&mut app));
}
