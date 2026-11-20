//! Keep exports and their recursively created folders outside durable world files.
use crate::DemoOptions;
use std::path::Path;
use terra_bevy::WorldConfig;

pub(crate) fn check(config: &WorldConfig, output: &Path) -> Result<(), String> {
    if output.as_os_str().is_empty() || output.file_name().is_none() {
        return Err("Output needs a file path.".into());
    }
    if let Some(database) = &config.region_store {
        protect_file_ancestors(database, output)?;
    }
    protect_file_ancestors(&config.save_path, output)?;
    Ok(())
}

/// Managed profiles and their write-ahead start witnesses remain protected even
/// before the first checkpoint exists. Reserve ancestors too, so an export
/// cannot turn a future metadata filename into its output directory.
pub(crate) fn check_managed_metadata(profile: Option<&Path>, output: &Path) -> Result<(), String> {
    let Some(profile) = profile else {
        return Ok(());
    };
    let witness = crate::launcher::model::managed_start_witness_path(profile);
    let lease = crate::launcher::model::managed_session_lease_path(profile);
    for protected in [profile, witness.as_path(), lease.as_path()] {
        protect_file_ancestors(protected, output).map_err(|_| {
            "An export or its parent would occupy a managed world profile, initialization record or session lock; choose a separate path.".to_owned()
        })?;
    }
    Ok(())
}

pub(crate) fn check_managed(
    config: &WorldConfig,
    profile: Option<&Path>,
    output: &Path,
) -> Result<(), String> {
    check(config, output)?;
    check_managed_metadata(profile, output)
}

/// Creating an output parent must not occupy a missing save, capability or
/// SQLite sidecar pathname. The shared persistence guard resolves ordinary,
/// case-folded, symbolic-link and existing hard-link aliases.
fn protect_file_ancestors(protected: &Path, output: &Path) -> Result<(), String> {
    for candidate in output.ancestors().filter(|path| path.file_name().is_some()) {
        terra_bevy::regions::validate_output_path(protected, candidate).map_err(|error| {
            format!("Output or its parent would occupy a protected world file or journal: {error}")
        })?;
    }
    Ok(())
}

/// Session credentials are selected by the launcher, outside WorldConfig.
/// Apply their guard to every application export before creating directories.
pub(crate) fn validate_session_outputs(
    config: &WorldConfig,
    options: &DemoOptions,
    session: &Path,
) -> Result<(), String> {
    let politics = config.save_path.with_extension("politics.geojson");
    let diagnostic_lock = options
        .telemetry
        .as_deref()
        .map(crate::telemetry_capture::lock_path);
    for output in [
        options.telemetry.as_deref(),
        diagnostic_lock.as_deref(),
        options.screenshot.as_deref().map(Path::new),
        Some(politics.as_path()),
    ]
    .into_iter()
    .flatten()
    {
        protect_file_ancestors(session, output)
            .map_err(|_| "An export or its parent would occupy a multiplayer session identity; choose a separate path.".to_owned())?;
    }
    Ok(())
}

/// Run before providers, output directories, or the native window are created.
/// Writers repeat the check because paths can change after startup.
pub(crate) fn validate(config: &WorldConfig, options: &DemoOptions) -> Result<(), String> {
    let profile = options.world_profile_path.as_deref();
    if let Some(path) = &options.telemetry {
        crate::telemetry_capture::validate_managed_destination(config, profile, path)
            .map_err(|error| format!("unsafe telemetry output: {error}"))?;
    }
    if let Some(path) = &options.screenshot {
        check_managed(config, profile, Path::new(path))
            .map_err(|error| format!("unsafe screenshot output: {error}"))?;
    }
    check_managed(
        config,
        profile,
        &config.save_path.with_extension("politics.geojson"),
    )
    .map_err(|error| format!("unsafe political export destination: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config(label: &str) -> WorldConfig {
        let directory = std::env::temp_dir().join(format!(
            "terra-output-preflight-{label}-{}",
            std::process::id()
        ));
        WorldConfig {
            region_store: Some(directory.join("world.db")),
            save_path: directory.join("manual-world.json"),
            ..Default::default()
        }
    }

    #[test]
    fn telemetry_cannot_overwrite_the_database_or_its_sidecar_before_a_window_opens() {
        let config = config("telemetry");
        let database = config.region_store.as_ref().unwrap();
        for output in [
            database.parent().unwrap().join("other/../world.db"),
            PathBuf::from(format!("{}-wal", database.display())),
        ] {
            let options = DemoOptions {
                telemetry: Some(output),
                ..Default::default()
            };
            assert!(
                validate(&config, &options)
                    .unwrap_err()
                    .contains("telemetry")
            );
        }
        assert!(!database.exists());
    }

    #[test]
    fn screenshot_cannot_target_the_database() {
        let config = config("screenshot");
        let options = DemoOptions {
            screenshot: Some(config.region_store.as_ref().unwrap().display().to_string()),
            ..Default::default()
        };
        assert!(
            validate(&config, &options)
                .unwrap_err()
                .contains("screenshot")
        );
    }

    #[test]
    fn screenshot_parent_creation_cannot_occupy_a_save_or_sqlite_sidecar() {
        let config = config("screenshot-parent");
        let directory = config.region_store.as_ref().unwrap().parent().unwrap();
        for protected in [
            "world.db",
            "world.db-wal",
            "world.db-shm",
            "world.db-journal",
            "WORLD.DB-WAL",
            "manual-world.json",
        ] {
            let parent = directory.join(protected);
            let output = parent.join("captures/frame.png");
            let options = DemoOptions {
                screenshot: Some(output.display().to_string()),
                ..Default::default()
            };
            assert!(
                validate(&config, &options)
                    .unwrap_err()
                    .contains("screenshot")
            );
            assert!(
                check(&config, &output).is_err(),
                "the runtime writer must repeat the same protection"
            );
            assert!(
                !parent.exists(),
                "must not create protected directory {protected}"
            );
        }
        assert!(
            !directory.exists(),
            "preflight must not create any output parents"
        );
    }

    #[test]
    fn derived_political_export_cannot_create_a_database_journal_directory() {
        let mut config = config("politics-parent");
        let database = config.region_store.as_ref().unwrap().clone();
        let journal = PathBuf::from(format!("{}-journal", database.display()));
        config.save_path = journal.join("manual.json");
        assert!(
            validate(&config, &DemoOptions::default())
                .unwrap_err()
                .contains("political export")
        );
        assert!(!journal.exists());
        assert!(!database.exists());
    }

    #[test]
    fn all_export_kinds_preserve_current_and_future_session_paths() {
        let config = config("session-exports");
        let directory = config.region_store.as_ref().unwrap().parent().unwrap();
        let session = directory.join("player-session.json");
        for options in [
            DemoOptions {
                screenshot: Some(session.display().to_string()),
                ..Default::default()
            },
            DemoOptions {
                screenshot: Some(session.join("frames/one.png").display().to_string()),
                ..Default::default()
            },
            DemoOptions {
                telemetry: Some(session.join("reports/one.json")),
                ..Default::default()
            },
        ] {
            assert!(validate_session_outputs(&config, &options, &session).is_err());
            assert!(!session.exists());
        }
        let political_destination = config.save_path.with_extension("politics.geojson");
        assert!(
            validate_session_outputs(&config, &DemoOptions::default(), &political_destination)
                .is_err()
        );
        assert!(!political_destination.exists());
        assert!(!directory.exists());
    }

    #[test]
    fn diagnostic_lock_cannot_occupy_a_current_or_future_session_identity() {
        let config = config("diagnostic-lock-session");
        let directory = config.region_store.as_ref().unwrap().parent().unwrap();
        let report = directory.join("latest.json");
        let session = crate::telemetry_capture::lock_path(&report);
        let options = DemoOptions {
            telemetry: Some(report),
            ..Default::default()
        };
        assert!(validate_session_outputs(&config, &options, &session).is_err());
        assert!(!directory.exists());
    }

    #[test]
    fn derived_political_export_is_also_protected() {
        let mut config = config("politics");
        config.region_store = Some(config.save_path.with_extension("politics.geojson"));
        assert!(
            validate(&config, &DemoOptions::default())
                .unwrap_err()
                .contains("political export")
        );
    }

    #[test]
    fn managed_outputs_reserve_profile_and_start_witness_before_parent_creation() {
        let config = config("managed-metadata-parents");
        let directory = config.save_path.parent().unwrap();
        let profile = directory.join("world.json");
        let witness = crate::launcher::model::managed_start_witness_path(&profile);
        let lease = crate::launcher::model::managed_session_lease_path(&profile);
        for protected in [&profile, &witness, &lease] {
            for destination in [protected.clone(), protected.join("captures/frame.png")] {
                let options = DemoOptions {
                    world_profile_path: Some(profile.clone()),
                    screenshot: Some(destination.display().to_string()),
                    ..Default::default()
                };
                assert!(
                    validate(&config, &options)
                        .unwrap_err()
                        .contains("screenshot")
                );
                assert!(check_managed(&config, Some(&profile), &destination).is_err());
            }
            let options = DemoOptions {
                world_profile_path: Some(profile.clone()),
                telemetry: Some(protected.join("reports/capture.json")),
                ..Default::default()
            };
            assert!(
                validate(&config, &options)
                    .unwrap_err()
                    .contains("telemetry")
            );
            let mut political_config = config.clone();
            political_config.save_path = protected.join("manual.json");
            assert!(
                validate(
                    &political_config,
                    &DemoOptions {
                        world_profile_path: Some(profile.clone()),
                        ..Default::default()
                    },
                )
                .unwrap_err()
                .contains("political export")
            );
            assert!(!protected.exists());
        }
        assert!(
            !directory.exists(),
            "preflight must not create export parents"
        );
    }

    #[test]
    fn managed_diagnostic_lock_reserves_a_future_profile_path() {
        let config = config("managed-diagnostic-lock");
        let directory = config.save_path.parent().unwrap();
        let report = directory.join("report.json");
        let profile = crate::telemetry_capture::lock_path(&report);
        let options = DemoOptions {
            world_profile_path: Some(profile),
            telemetry: Some(report),
            ..Default::default()
        };
        assert!(
            validate(&config, &options)
                .unwrap_err()
                .contains("telemetry")
        );
        assert!(!directory.exists());
    }

    #[cfg(unix)]
    #[test]
    fn managed_output_aliases_are_rechecked_after_preflight() {
        use std::fs;
        use std::os::unix::fs::symlink;
        let config = config("managed-metadata-aliases");
        let directory = config.save_path.parent().unwrap();
        fs::create_dir_all(directory).unwrap();
        let profile = directory.join("world.json");
        let witness = crate::launcher::model::managed_start_witness_path(&profile);
        fs::write(&profile, b"keep the managed profile").unwrap();
        fs::write(&witness, b"keep the initialization witness").unwrap();
        let lease = crate::launcher::model::managed_session_lease_path(&profile);
        fs::write(&lease, b"keep the managed session lock").unwrap();
        for protected in [&profile, &witness, &lease] {
            let original = fs::read(protected).unwrap();
            for hard_link in [false, true] {
                let screenshot = directory.join("allowed-before-start.png");
                let report = directory.join("allowed-before-start.json");
                let lock = crate::telemetry_capture::lock_path(&report);
                let options = DemoOptions {
                    world_profile_path: Some(profile.clone()),
                    screenshot: Some(screenshot.display().to_string()),
                    telemetry: Some(report.clone()),
                    ..Default::default()
                };
                validate(&config, &options).unwrap();
                if hard_link {
                    fs::hard_link(protected, &screenshot).unwrap();
                    fs::hard_link(protected, &lock).unwrap();
                } else {
                    symlink(protected, &screenshot).unwrap();
                    symlink(protected, &lock).unwrap();
                }
                assert!(validate(&config, &options).is_err());
                assert!(check_managed(&config, Some(&profile), &screenshot).is_err());
                assert!(
                    crate::telemetry_capture::validate_managed_destination(
                        &config,
                        Some(&profile),
                        &report,
                    )
                    .is_err()
                );
                assert_eq!(fs::read(protected).unwrap(), original);
                assert!(!report.exists());
                fs::remove_file(screenshot).unwrap();
                fs::remove_file(lock).unwrap();
            }
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn distinct_artifacts_pass_without_creating_any_output() {
        let config = config("distinct");
        let directory = config.region_store.as_ref().unwrap().parent().unwrap();
        let options = DemoOptions {
            telemetry: Some(directory.join("report.json")),
            screenshot: Some(directory.join("scene.png").display().to_string()),
            world_profile_path: Some(directory.join("world.json")),
            ..Default::default()
        };
        validate(&config, &options).unwrap();
        assert!(!directory.exists());
    }
}
