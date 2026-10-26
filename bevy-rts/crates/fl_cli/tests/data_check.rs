use std::path::PathBuf;
use std::process::Command;

use fl_catalog::base_mod_dir;
use fl_cli::{data_check, hash_dump};
use fl_sim::BuildError;

fn bad_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bad_mod")
}

fn content_errors() -> Vec<String> {
    match data_check(&[bad_mod()]) {
        Err(BuildError::Content(errors)) => errors.0.iter().map(ToString::to_string).collect(),
        other => panic!("expected content errors, got {other:?}"),
    }
}

#[test]
fn base_mod_passes() {
    let report = data_check(&[base_mod_dir()]).expect("base mod loads");
    assert_eq!(report.units, 10);
    assert_eq!(report.mods, ["base"]);
}

#[test]
fn bad_mod_lists_all_errors() {
    let errors = content_errors();
    assert!(errors.len() >= 3, "{errors:#?}");
    for error in &errors {
        assert!(error.contains("bad_mod"), "{error}");
    }
}

#[test]
fn data_check_bad_mod_exit_code() {
    let output = Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .arg("data-check")
        .arg(bad_mod())
        .output()
        .expect("run fl_cli");
    assert_eq!(output.status.code(), Some(1));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for error in content_errors() {
        assert!(text.contains(&error), "missing {error:?} in:\n{text}");
    }
}

#[test]
fn hash_dump_is_repeatable() {
    let mods = [base_mod_dir()];
    let first = hash_dump(&mods, "base.testbed", 100, 10).expect("first run");
    let second = hash_dump(&mods, "base.testbed", 100, 10).expect("second run");
    assert_eq!(first.len(), 10);
    assert_eq!(first, second);
}

fn fl_cli(args: &[&str]) -> (Option<i32>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_fl_cli"))
        .args(args)
        .output()
        .expect("run fl_cli");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn data_check_base_mod_succeeds() {
    let base = base_mod_dir();
    let base = base.to_str().expect("UTF-8 path");
    let (code, stdout) = fl_cli(&["data-check", base]);
    assert_eq!(code, Some(0), "{stdout}");
    let hash = data_check(&[base_mod_dir()])
        .expect("base loads")
        .rules_hash;
    assert!(
        stdout
            .lines()
            .any(|line| line == format!("rules hash: 0x{hash:016x}")),
        "{stdout}"
    );

    let (code, stdout) = fl_cli(&["--json", "data-check", base]);
    assert_eq!(code, Some(0), "{stdout}");
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(json["rules_hash"], format!("0x{hash:016x}"));
    assert_eq!(json["mods"], serde_json::json!(["base"]));
}

#[test]
fn data_check_kinds_sorted() {
    let report = data_check(&[base_mod_dir()]).expect("base mod loads");
    let names: Vec<&str> = report.kinds.iter().map(|(kind, _)| *kind).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    assert!(names.contains(&"scenario"), "{names:?}");
}

#[test]
fn hash_dump_text_output() {
    let base = base_mod_dir();
    let (code, stdout) = fl_cli(&[
        "hash-dump",
        "--mods",
        base.to_str().expect("UTF-8 path"),
        "--scenario",
        "base.testbed",
        "--ticks",
        "40",
        "--interval",
        "20",
    ]);
    assert_eq!(code, Some(0), "{stdout}");
    let expected: Vec<String> = hash_dump(&[base_mod_dir()], "base.testbed", 40, 20)
        .expect("hash dump")
        .iter()
        .map(|(tick, hash)| format!("{} {hash:016x}", tick.0))
        .collect();
    assert_eq!(expected.len(), 2);
    assert_eq!(stdout.lines().collect::<Vec<_>>(), expected);
}
