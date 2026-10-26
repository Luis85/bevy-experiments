//! The kernel's replay functions with the testbed as the app factory.
//!
//! A testbed replay names its scenario `"testbed"` (the default showcase)
//! or `"testbed:<name>"` for another embedded scenario file
//! ([`SCENARIO_FILES`](crate::SCENARIO_FILES)), so a tool that knows several
//! contents can tell from the replay alone which one rebuilds it
//! ([`is_testbed`]). [`try_new_replay_app`] is that factory: the embedded
//! scenario, built like [`try_new_sim_app`](crate::try_new_sim_app), with
//! `ScenarioInfo.name` set to the qualified name (so a recorder writes it
//! into the replay header).
//!
//! ```ignore
//! let mut app = testbed_content::replay::try_new_replay_app(1, "testbed:smithy", 0)?;
//! let replay = testbed_content::replay::record(&mut app, 2400, 100);
//! assert!(testbed_content::replay::verify(&replay).ok);
//! ```

use bevy_app::App;
use npc_framework::kernel::replay as core;
use npc_framework::kernel::replay::ScenarioInfo;

pub use core::{Divergence, Replay, VerifyReport, record};

/// The content name, and the replay name of the default scenario.
pub const PREFIX: &str = crate::scenario::CONTENT;

/// Whether a replay scenario name belongs to the testbed: `"testbed"` or
/// `"testbed:<name>"`.
pub fn is_testbed(name: &str) -> bool {
    name == PREFIX
        || name
            .strip_prefix(PREFIX)
            .is_some_and(|rest| rest.starts_with(':'))
}

/// The embedded scenario file a qualified name stands for: `"testbed"` →
/// `"testbed"`, `"testbed:smithy"` → `"smithy"`; `None` for a name that is
/// not the testbed's.
pub fn embedded_name(name: &str) -> Option<&str> {
    if name == PREFIX {
        return Some(PREFIX);
    }
    name.strip_prefix(PREFIX)?.strip_prefix(':')
}

/// The qualified replay name of embedded scenario `scenario` (`"testbed"`
/// stays `"testbed"`).
pub fn qualified(scenario: &str) -> String {
    if scenario == PREFIX {
        PREFIX.to_owned()
    } else {
        format!("{PREFIX}:{scenario}")
    }
}

/// The replay `AppFactory` of the testbed: `name` is `"testbed"` or
/// `"testbed:<embedded scenario>"` (`_units` is ignored: the scenario files
/// fix the actors). The app is at tick 0, content frozen, not finished, no
/// driver; `ScenarioInfo.name` is the qualified `name`.
pub fn try_new_replay_app(seed: u64, name: &str, units: u32) -> Result<App, String> {
    let scenario = embedded_name(name).ok_or_else(|| {
        format!(
            "`{name}` is not a testbed scenario (expected `testbed` or `testbed:<name>`, \
             <name> one of {:?})",
            crate::SCENARIO_FILES.map(|(n, _)| n)
        )
    })?;
    let mut app = crate::try_new_sim_app(seed, scenario, units)?;
    let world = app.world_mut();
    let villagers = world.resource::<ScenarioInfo>().villagers;
    world.insert_resource(ScenarioInfo {
        name: qualified(scenario),
        villagers,
    });
    Ok(app)
}

/// [`core::verify`] with the testbed factory.
pub fn verify(replay: &Replay) -> VerifyReport {
    core::verify(replay, try_new_replay_app)
}

/// [`core::replay_app`] with the testbed factory.
pub fn replay_app(replay: &Replay) -> Result<App, String> {
    core::replay_app(replay, try_new_replay_app)
}

/// [`core::try_diverge`] with the testbed factory.
pub fn try_diverge(a: &Replay, b: &Replay) -> Result<Option<Divergence>, String> {
    core::try_diverge(a, b, try_new_replay_app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_qualify_and_resolve() {
        assert!(is_testbed("testbed") && is_testbed("testbed:smithy"));
        assert!(!is_testbed("village") && !is_testbed("testbedx") && !is_testbed("doors"));
        assert_eq!(embedded_name("testbed"), Some("testbed"));
        assert_eq!(embedded_name("testbed:doors"), Some("doors"));
        assert_eq!(embedded_name("village"), None);
        assert_eq!(qualified("testbed"), "testbed");
        assert_eq!(qualified("doors"), "testbed:doors");
        let e = try_new_replay_app(1, "village", 1).err().unwrap();
        assert!(e.contains("not a testbed scenario"), "{e}");
        let e = try_new_replay_app(1, "testbed:nope", 1).err().unwrap();
        assert!(e.contains("unknown testbed scenario `nope`"), "{e}");
    }

    #[test]
    fn a_recorded_testbed_replay_verifies_with_its_qualified_name() {
        let mut app = try_new_replay_app(3, "testbed:doors", 0).unwrap();
        let replay = record(&mut app, 60, 20);
        assert_eq!(replay.scenario, "testbed:doors");
        assert_eq!(replay.checkpoints.len(), 3);
        let report = verify(&replay);
        assert!(report.ok && report.checked == 3, "{report:?}");
    }
}
