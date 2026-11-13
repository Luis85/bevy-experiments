//! A6-00: the audio crate stays Bevy-free, so `cargo test -p horde_audio`
//! never compiles the engine. The check reads `cargo tree` for every
//! dependency kind (normal, build and dev).

use std::path::Path;
use std::process::Command;

/// Engine and device crates that must never appear under `horde_audio`.
const FORBIDDEN_PREFIXES: [&str; 7] = ["bevy", "wgpu", "winit", "egui", "rodio", "cpal", "naga"];

/// The package this crate may never depend on: the game binary.
const FORBIDDEN_EXACT: [&str; 1] = ["horde-defense"];

/// Package names in `cargo tree --prefix none` output that break the rule.
fn forbidden_packages(tree: &str) -> Vec<String> {
    let mut found: Vec<String> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| {
            FORBIDDEN_EXACT.contains(name)
                || FORBIDDEN_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
        })
        .map(str::to_owned)
        .collect();
    found.sort();
    found.dedup();
    found
}

fn cargo_tree() -> String {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["tree", "--locked", "--offline", "-p", "horde_audio"])
        .args(["--edges", "normal,build,dev", "--prefix", "none"])
        .output()
        .expect("cargo tree runs");
    assert!(
        output.status.success(),
        "cargo tree -p horde_audio failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("cargo tree prints UTF-8")
}

#[test]
fn horde_audio_builds_without_bevy() {
    let tree = cargo_tree();
    assert!(
        tree.lines().any(|line| line.starts_with("horde_core ")),
        "the tree lists horde_core, so the scan read real output:\n{tree}"
    );
    assert_eq!(
        forbidden_packages(&tree),
        Vec::<String>::new(),
        "horde_audio must not depend on Bevy or a device crate:\n{tree}"
    );
}

/// Self-test: the scanner reports a planted engine crate and the game binary,
/// and ignores a crate that only contains the word.
#[test]
fn scanner_reports_a_planted_bevy_line() {
    let planted = "horde_audio v0.1.0 (C:\\x)\nserde v1.0.0\nbevy_audio v0.20.0\n\
                   bevy v0.20.0 (*)\nhorde-defense v0.1.0 (C:\\y)\nnot_bevy v1.0.0\n";
    assert_eq!(
        forbidden_packages(planted),
        ["bevy", "bevy_audio", "horde-defense"]
    );
}

/// The crate reaches the workspace's one SHA-256 through its dev-dependency
/// (FIPS 180-4 "abc" vector).
#[test]
fn sha256_is_reachable_through_the_dev_dependency() {
    assert_eq!(
        horde_core::sha256::hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
