//! Headless authoring and CI over the exact native runtime importer.
use std::path::Path;
use terra_bevy::asset_pack::{
    ValidatedAssetPack,
    authoring::{PreparedAssetPack, inspect_model},
};

const USAGE: &str = "Terra asset tools\n\
Usage:\n\
  terra-assets inspect MODEL.glb\n\
  terra-assets build RECIPE.json NEW_DIRECTORY\n\
  terra-assets validate PACK.json\n\n\
Inspect validates one self-contained GLB and lists exact clip/node names and budgets.\n\
Build snapshots recipe-relative GLBs, validates complete bindings, computes hashes,\n\
and writes a new content-identified pack. The output parent must exist. Existing\n\
outputs, symlink paths and parent traversal are rejected; no source is overwritten.\n\
Validate checks immutable model bytes, binding policies and import budgets.\n\
Every success prints JSON; every rejection exits nonzero without acceptance JSON.\n\
See docs/ASSET_AUTHORING.md for recipes, export settings and visual checks.";

fn json_report(report: &impl serde::Serialize) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(report).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--help"] || args.as_slice() == ["-h"] {
        println!("{USAGE}");
        return Ok(());
    }
    if args.len() == 2 && args[0] == "inspect" {
        let model = inspect_model(Path::new(&args[1]))?;
        return json_report(&serde_json::json!({
            "schema_version":1, "accepted":true, "model":model,
            "scope":"Validated supported GLB bytes and all import budgets. Clip names do not prove animation intent, scale, axes, foot placement or native readiness.",
            "next":"Map these exact clip names in an authoring recipe, then build and inspect the resulting pack in the native game."
        }));
    }
    if args.len() == 3 && args[0] == "build" {
        let pack = PreparedAssetPack::read(Path::new(&args[1]))?;
        pack.write_new(Path::new(&args[2]))?;
        return json_report(pack.report());
    }
    if args.len() != 2 || args[0] != "validate" {
        return Err(USAGE.into());
    }
    let pack = ValidatedAssetPack::read(Path::new(&args[1]))?;
    let models: Vec<_> = pack
        .budgets()
        .map(|(id, budget)| serde_json::json!({"id":id,"budget":budget}))
        .collect();
    let report = serde_json::json!({
        "schema_version":1,"accepted":true,"name":pack.name(),"identity":pack.identity(),"models":models,
        "scope":"Validated source bytes and supported format; native renderer readiness is checked at runtime"
    });
    json_report(&report)
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Asset pack rejected: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
