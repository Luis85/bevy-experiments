use std::{path::Path, process::Command};

#[test]
fn asset_validator_reports_bundled_budgets_and_rejects_missing_input() {
    let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wildlife-pack/pack.json");
    let output = Command::new(env!("CARGO_BIN_EXE_terra-assets"))
        .current_dir(pack.parent().unwrap())
        .arg("validate")
        .arg("pack.json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["accepted"], true);
    assert_eq!(report["models"].as_array().unwrap().len(), 5);
    assert!(report["identity"].as_str().unwrap().starts_with("sha256:"));
    let rejected = Command::new(env!("CARGO_BIN_EXE_terra-assets"))
        .arg("validate")
        .arg(pack.with_file_name("missing-pack.json"))
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Asset pack rejected"));
    assert!(rejected.stdout.is_empty());
}
