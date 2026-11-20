//! Headless, explicit world maintenance. No renderer or running host required.
use std::{error::Error, fs, io::Read, path::Path};
use terra_bevy::recovery::{
    create_recovery_bundle, read_recovery_config, restore_recovery_bundle, verify_recovery_bundle,
};
use terra_bevy::{WorldConfig, sandbox::LocalWorldSave};
use terra_persistence::{RegionArchive, RegionBackupOptions, RegionStore, write_new_private_file};
use terra_sandbox::Catalog;

const USAGE: &str = "Terra world operations\n\n  terra-worldctl inspect DATABASE\n  terra-worldctl verify DATABASE\n  terra-worldctl backup DATABASE NEW_DATABASE\n  terra-worldctl recover VERIFIED_BACKUP NEW_DATABASE\n  terra-worldctl export DATABASE NEW_ARCHIVE.json\n  terra-worldctl import ARCHIVE.json NEW_DATABASE\n  terra-worldctl import-local SAVE.json NEW_DATABASE CONFIG.json\n  terra-worldctl export-local DATABASE NEW_SAVE.json CONFIG.json\n  terra-worldctl bundle-create DATABASE CONFIG.json NEW_DIRECTORY\n  terra-worldctl bundle-verify DIRECTORY [--expect-bundle sha256:HASH]\n  terra-worldctl bundle-restore DIRECTORY NEW_DIRECTORY [--expect-bundle sha256:HASH]\n\nInspect/verify decode the committed checkpoint and validate complete domain data.\nBackups include SQLite WAL state, validate the copied snapshot and publish atomically.\nRecovery verifies a backup into a NEW path; it never alters a damaged database. Every output must be new; nothing is overwritten.\nArchives preserve catalog, source identity and private host metadata. They are not public snapshots.\nLocal JSON conversion validates the explicit matching WorldConfig and version-one save envelope.\nRecovery bundles include the last committed database, exact built-in catalog, portable resolved config, prepared terrain and optional political GeoJSON dependencies. They support offline reference local worlds without historical packs.\nBundle restore emits an absolute-path WorldConfig in a new private directory; start terra-demo --config NEW_DIRECTORY/world-config.json --no-default-politics. Keep bundles private and unmodified. Hashes are not publisher signatures or encryption.\nServer JSON migration: terra-server --import-json SAVE --region-store NEW_DATABASE [matching world flags].\nCheckpoint or stop the authoritative host first to include its latest active changes.";

fn main() {
    if let Err(error) = run() {
        eprintln!("terra-worldctl: {error}");
        std::process::exit(1);
    }
}

fn config(path: &str) -> Result<WorldConfig, Box<dyn Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return Err("configuration must be a regular JSON file no larger than 1 MiB".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err("configuration exceeds 1 MiB".into());
    }
    let config: WorldConfig = serde_json::from_slice(&bytes)?;
    config.validate()?;
    Ok(config)
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["--help"] | ["-h"] => println!("{USAGE}"),
        ["bundle-create", database, settings, output] => {
            let config = read_recovery_config(settings)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&create_recovery_bundle(database, &config, output)?)?
            );
        }
        ["bundle-verify", directory] => {
            println!(
                "{}",
                serde_json::to_string_pretty(&verify_recovery_bundle(directory, None)?)?
            );
        }
        ["bundle-verify", directory, "--expect-bundle", expected] => {
            println!(
                "{}",
                serde_json::to_string_pretty(&verify_recovery_bundle(directory, Some(expected))?)?
            );
        }
        ["bundle-restore", directory, output] => {
            println!(
                "{}",
                serde_json::to_string_pretty(&restore_recovery_bundle(directory, output, None)?)?
            );
        }
        [
            "bundle-restore",
            directory,
            output,
            "--expect-bundle",
            expected,
        ] => {
            println!(
                "{}",
                serde_json::to_string_pretty(&restore_recovery_bundle(
                    directory,
                    output,
                    Some(expected)
                )?)?
            );
        }
        ["inspect" | "verify", database] => {
            let store = RegionStore::open_read_only(database)?;
            println!("{}", serde_json::to_string_pretty(&store.inspect()?)?);
        }
        [verb @ ("backup" | "recover"), database, output] => {
            let store = RegionStore::open_read_only(database)?;
            let receipt = store.backup_with_options(output, RegionBackupOptions::default())?;
            println!(
                "{} checkpoint {} to {output}; validated {} regions / {} payload bytes. Open with the matching host configuration to validate private metadata.",
                if *verb == "recover" {
                    "Recovered"
                } else {
                    "Backed up"
                },
                receipt.generation,
                receipt.nonempty_regions,
                receipt.checkpoint_payload_bytes
            );
        }
        ["export", database, output] => {
            let store = RegionStore::open_read_only(database)?;
            store.check_output_path(output)?;
            let archive = store.archive()?;
            archive.write_new(output)?;
            println!(
                "Exported complete checkpoint {} to {output}",
                archive.source_generation
            );
        }
        ["import", input, output] => {
            let archive = RegionArchive::read(input)?;
            let status = archive.import_new(output)?;
            println!(
                "Imported source checkpoint {} into new database {output} at generation {}",
                archive.source_generation, status.generation
            );
        }
        ["import-local", input, output, settings] => {
            let config = config(settings)?;
            let catalog = Catalog::builtin();
            let save = LocalWorldSave::read(Path::new(input), &config, &catalog)?;
            let status = save
                .regional_archive(&config, &catalog)?
                .import_new(output)?;
            println!(
                "Imported validated local world into new database {output} at generation {}",
                status.generation
            );
        }
        ["export-local", database, output, settings] => {
            let config = config(settings)?;
            let store = RegionStore::open_read_only(database)?;
            store.check_output_path(output)?;
            let save = LocalWorldSave::from_regional_archive(store.archive()?, &config)?;
            let bytes = serde_json::to_vec(&save)?;
            if bytes.len() > 48 * 1024 * 1024 {
                return Err("local save exceeds 48 MiB limit".into());
            }
            write_new_private_file(output, &bytes)?;
            println!("Exported validated complete local world to {output}");
        }
        _ => return Err(format!("invalid command or arguments\n\n{USAGE}").into()),
    }
    Ok(())
}
