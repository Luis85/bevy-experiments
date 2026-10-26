//! Criterion bench of one `step_tick` on the scale scenario (spec §12.6, §15 D; ADR-0010).
//!
//! Groups `tick/500`, `tick/3000` and `tick/5000` on the `core_mod` fixture content (ruling
//! R3), each iteration one tick after a 40-tick warmup. Wall clock only: informational, the
//! gates are work counters.

use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use fl_protocol::Tick;
use fl_sim::{MatchSetup, RunConfig, ScaleScenario, build_sim_app, load_content, step_tick};
use fl_view::TickInput;

const WARMUP_TICKS: u32 = 40;

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

fn tick(c: &mut Criterion) {
    let content = load_content(&[], &[core_mod()]).expect("core_mod loads");
    let mut group = c.benchmark_group("tick");
    for units in [500_u32, 3000, 5000] {
        let setup = MatchSetup {
            seed: 1,
            cheats_allowed: false,
        };
        let mut app = build_sim_app(&[], content.clone(), setup, RunConfig::default())
            .expect("the sim builds");
        let scale = ScaleScenario {
            units,
            ..ScaleScenario::default()
        };
        scale.populate(app.world_mut()).expect("scale populates");
        let mut next = 0;
        let mut step = || {
            let input = TickInput {
                tick: Tick(next),
                commands: Vec::new(),
            };
            next += 1;
            step_tick(app.world_mut(), input)
        };
        for _ in 0..WARMUP_TICKS {
            step();
        }
        group.bench_function(units.to_string(), |b| b.iter(&mut step));
    }
    group.finish();
}

criterion_group!(benches, tick);
criterion_main!(benches);
