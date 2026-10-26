//! FX4: `npcctl run --all-needs` reports every registered need generically,
//! and leaves the default summary untouched.

use std::process::Command;

use serde_json::Value;

fn run(args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_npcctl"))
        .args(args)
        .output()
        .expect("npcctl runs");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    serde_json::from_slice(&out.stdout).expect("JSON summary")
}

#[test]
fn all_needs_is_opt_in_and_matches_the_village_ranges() {
    let plain = run(&["run", "--seed", "1", "--ticks", "300"]);
    assert!(plain.get("needs_all").is_none(), "absent by default");
    let all = run(&["run", "--seed", "1", "--ticks", "300", "--all-needs"]);
    let needs_all = all["needs_all"].as_object().expect("needs_all object");
    // The village registers Energy, Hunger and Thirst: hunger and energy
    // have the same ranges as `needs` (which reports only those two), thirst
    // only appears here.
    assert_eq!(needs_all.len(), 3, "{needs_all:?}");
    for key in ["hunger", "energy"] {
        assert_eq!(needs_all[key], all["needs"][key], "{key}");
    }
    assert!(all["needs"].get("thirst").is_none(), "{all}");
    let thirst = &needs_all["thirst"];
    let (min, max) = (
        thirst["min"].as_i64().unwrap(),
        thirst["max"].as_i64().unwrap(),
    );
    assert!(0 <= min && min <= max && max <= 100_000, "{thirst}");
    // Everything else is the same run.
    let mut rest = all.clone();
    rest.as_object_mut().unwrap().remove("needs_all");
    assert_eq!(rest, plain);
}

#[test]
fn all_needs_covers_other_contents() {
    let all = run(&[
        "run",
        "--scenario",
        "testbed",
        "--ticks",
        "120",
        "--all-needs",
    ]);
    let needs_all = all["needs_all"].as_object().expect("needs_all object");
    for (key, range) in needs_all {
        let (min, max) = (
            range["min"].as_i64().unwrap(),
            range["max"].as_i64().unwrap(),
        );
        assert!(min <= max && (0..=100_000).contains(&max), "{key}: {range}");
    }
}
