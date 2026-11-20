//! Offline developer handoff for the same prepared-source importer as play.
use std::{collections::BTreeMap, path::Path, process::ExitCode};
use terra_core::GeoBounds;
use terra_stream::source_distribution::{
    SourceInspectionOptions, export_source_package, inspect_prepared_source,
    install_source_package, verify_source_package,
};

const HELP: &str = "Terra prepared source distribution (offline)

terra-sources inspect MANIFEST [--level 15] [--resolution 65]
                      [--bounds west,south,east,north] [--require-complete]
terra-sources export MANIFEST NEW_DIRECTORY [same inspection options]
terra-sources verify PACKAGE_DIRECTORY [--expect-package blake3:HASH]
                     [--expect-source ID] [--require-complete]
terra-sources install PACKAGE_DIRECTORY NEW_DIRECTORY [same verification options]

Inspect validates raster, semantics, seams and exact complete-tile coverage.
Export writes a self-contained directory and source-package.json last.
Verify rehashes every file and revalidates original identity/provenance/coverage.
Install repeats those checks before copying into a new directory; no overwrite.
Destination parents must exist. Source files must remain unchanged during work.
All commands print JSON; failures return nonzero. --require-complete rejects
missing tiles in the requested extent. Dense grayscale has no NoData-mask support.
Package hashes are content identity, not publisher signatures. No network, save
migration, source revision replacement or geographic completeness is inferred.
";

fn run(args: Vec<String>) -> Result<serde_json::Value, String> {
    let command = args.first().ok_or("missing command; use --help")?;
    let positional = match command.as_str() {
        "inspect" | "verify" => 2,
        "export" | "install" => 3,
        _ => return Err("unknown command; use --help".into()),
    };
    if args.len() < positional || args[1..positional].iter().any(|arg| arg.starts_with("--")) {
        return Err("missing manifest/package or new destination; use --help".into());
    }
    let mut options = BTreeMap::new();
    let mut require_complete = false;
    let mut cursor = positional;
    while cursor < args.len() {
        let name = &args[cursor];
        if name == "--require-complete" {
            if require_complete {
                return Err("duplicate --require-complete".into());
            }
            require_complete = true;
            cursor += 1;
            continue;
        }
        if !name.starts_with("--") || cursor + 1 >= args.len() || args[cursor + 1].starts_with("--")
        {
            return Err(format!("option {name:?} requires a value"));
        }
        if options
            .insert(name.as_str(), args[cursor + 1].as_str())
            .is_some()
        {
            return Err(format!("duplicate {name}"));
        }
        cursor += 2;
    }
    let source = Path::new(&args[1]);
    let report = if matches!(command.as_str(), "inspect" | "export") {
        let mut inspection_options = SourceInspectionOptions::default();
        if let Some(value) = options.remove("--level") {
            inspection_options.coverage_level =
                value.parse().map_err(|_| "--level must be 0..20")?;
        }
        if let Some(value) = options.remove("--resolution") {
            inspection_options.terrain_resolution =
                value.parse().map_err(|_| "--resolution must be 2..257")?;
        }
        if let Some(value) = options.remove("--bounds") {
            let values = value
                .split(',')
                .map(|v| v.parse::<f64>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "--bounds needs west,south,east,north numbers")?;
            if values.len() != 4 {
                return Err("--bounds needs four comma-separated numbers".into());
            }
            inspection_options.requested_bounds = Some(GeoBounds {
                west: values[0],
                south: values[1],
                east: values[2],
                north: values[3],
            });
        }
        if !options.is_empty() {
            return Err(format!(
                "unsupported inspection options: {:?}",
                options.keys()
            ));
        }
        let inspection =
            inspect_prepared_source(source, inspection_options).map_err(|e| e.to_string())?;
        if require_complete && !inspection.coverage.complete {
            return Err(format!(
                "{} requested tiles lack complete coverage",
                inspection.coverage.missing_tiles.len()
            ));
        }
        if command == "export" {
            serde_json::to_value(
                export_source_package(source, &args[2], inspection_options)
                    .map_err(|e| e.to_string())?,
            )
        } else {
            serde_json::to_value(inspection)
        }
    } else {
        let expected_package = options.remove("--expect-package");
        let expected_source = options.remove("--expect-source");
        if !options.is_empty() {
            return Err(format!(
                "unsupported verification options: {:?}",
                options.keys()
            ));
        }
        let verified = verify_source_package(source, expected_package, expected_source)
            .map_err(|e| e.to_string())?;
        if require_complete && !verified.inspection.coverage.complete {
            return Err(format!(
                "{} requested tiles lack complete coverage",
                verified.inspection.coverage.missing_tiles.len()
            ));
        }
        if command == "install" {
            serde_json::to_value(
                install_source_package(
                    source,
                    &args[2],
                    Some(&verified.package_identity),
                    expected_source,
                )
                .map_err(|e| e.to_string())?,
            )
        } else {
            serde_json::to_value(verified)
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(report)
}
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    match run(args) {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Source package rejected: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_cli_options_fail_before_touching_sources() {
        for args in [
            vec!["inspect", "missing.json", "--level", "15", "--level", "16"],
            vec!["export", "missing.json"],
            vec!["inspect", "missing.json", "--expect-source", "anything"],
            vec!["verify", "missing", "--resolution", "65"],
            vec!["inspect", "missing.json", "--bounds", "1,2,3"],
        ] {
            assert!(run(args.into_iter().map(String::from).collect()).is_err());
        }
    }
}
