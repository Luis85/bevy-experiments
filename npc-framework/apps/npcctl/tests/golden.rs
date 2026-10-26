//! Task P2: insta golden snapshot (JSON) of the `npcctl run` summary.
//!
//! The snapshot in `tests/snapshots/` is an oracle: never edit or re-accept
//! it to make the test pass; re-baselining needs explicit approval. CI runs
//! with `INSTA_UPDATE=no`.

use std::process::Command;

use serde_json::Value;

#[test]
fn golden_run_seed1_7200() {
    let out = Command::new(env!("CARGO_BIN_EXE_npcctl"))
        .args(["run", "--seed", "1", "--ticks", "7200"])
        .output()
        .expect("npcctl runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "npcctl run failed: {stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let summary: Value = serde_json::from_str(&stdout).expect("run prints one JSON object");
    insta::assert_json_snapshot!("run_seed1_7200", summary);
}
