//! Managed local recovery. Launcher metadata stays beside the immutable bundle;
//! only its final record admits a completed backup into the browser.
use super::model::{self, RecentWorld, WorldProfile};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use terra_bevy::{
    recovery::{
        RecoveryVerification, create_recovery_bundle_with_sync, read_recovery_config,
        restore_recovery_bundle_with_sync, verify_recovery_bundle,
    },
    render::RenderQualitySettings,
    rules::CharacterProfile,
};

const RECORD: &str = "world-backup.json";
const BUNDLE: &str = "bundle";
const MAX_RECORD_BYTES: u64 = 8 * 1024;
const MAX_INSPECTED_BACKUPS: usize = 256;
pub(super) const MAX_RECENT_BACKUPS: usize = 24;
type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BackupRecord {
    schema_version: u32,
    pub title: String,
    pub character: CharacterProfile,
    pub quality: RenderQualitySettings,
    pub start_hour: u8,
    pub world_created_unix_seconds: u64,
    pub created_unix_seconds: u64,
    pub source_generation: u64,
    pub bundle_identity: String,
}

impl BackupRecord {
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.source_generation == 0 {
            return Err(
                "This backup uses an unsupported record or has no committed checkpoint.".into(),
            );
        }
        model::validate_name(&self.title, 48, "Backup world name")?;
        model::validate_character(&self.character)?;
        if self.title != self.title.trim()
            || self.character.name != self.character.name.trim()
            || self.created_unix_seconds > 253_402_300_799
            || self.world_created_unix_seconds > 253_402_300_799
        {
            return Err("Backup names or recorded dates are invalid.".into());
        }
        if self.start_hour > 23 || !valid_identity(&self.bundle_identity) {
            return Err("Backup character settings or checkpoint identity are invalid.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(super) struct Backup {
    pub directory: PathBuf,
    pub record: BackupRecord,
    // Retain the complete bounded metadata bytes, not a mutable list index.
    record_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(super) struct BackupReview {
    pub backup: Backup,
    pub verified: RecoveryVerification,
    pub restored_title: String,
}

#[derive(Debug, Clone)]
pub(super) struct WorldSelection {
    path: PathBuf,
    profile: WorldProfile,
}

#[derive(Debug)]
pub(super) struct RestoredWorld {
    pub profile_path: PathBuf,
    pub title: String,
    pub checkpoint: u64,
}

fn valid_identity(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| "The system clock is before 1970.".into())
}

/// Reject path aliases before following any existing component. Maintenance
/// requires quiescent trees; it does not defend against an administrator racing
/// filesystem ancestors while an operation is in progress.
fn checked_path(path: &Path, must_exist: bool) -> Result<PathBuf> {
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(
            "Choose a direct recovery folder path without parent-directory shortcuts.".into(),
        );
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("Recovery paths cannot use symbolic links.".into());
            }
            Ok(_) => (),
            Err(error) if !must_exist && error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(format!("{}: {error}", current.display())),
        }
    }
    if must_exist {
        path.canonicalize().map_err(|e| e.to_string())
    } else {
        Ok(path)
    }
}

fn private(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink()
        || metadata.is_dir() != directory
        || (!directory && !metadata.is_file())
    {
        return Err("Recovery content must use ordinary files and folders.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o077 != 0 || (!directory && metadata.nlink() != 1) {
            return Err("Recovery copies must be private (folders 0700, files 0600) and cannot use hard-linked files.".into());
        }
    }
    Ok(())
}

fn managed_parent(
    user_data: &Path,
    child: &str,
    protected: &[&Path],
    sync: &dyn Fn(&Path) -> std::io::Result<()>,
) -> Result<PathBuf> {
    let parent = checked_path(&user_data.join(child), false)?;
    // Check every ancestor before creating it: an absent DB journal must not
    // become a directory as a side effect of recovery output creation.
    for ancestor in parent.ancestors().filter(|path| path.file_name().is_some()) {
        for input in protected {
            terra_bevy::regions::validate_output_path(input, ancestor)
                .map_err(|e| e.to_string())?;
        }
    }
    model::private_directory_with_sync(&parent, sync)?;
    checked_path(&parent, true)?;
    private(&parent, true)?;
    Ok(parent)
}

fn claim_directory(parent: &Path, prefix: &str) -> Result<PathBuf> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    for _ in 0..16 {
        let candidate = parent.join(format!(
            "{prefix}-{}-{}-{}",
            now()?,
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                if let Err(error) = model::private_permissions(&candidate, true) {
                    let _ = fs::remove_dir(&candidate);
                    return Err(error);
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => {
                return Err(format!(
                    "Could not allocate a separate recovery folder: {error}"
                ));
            }
        }
    }
    Err("Could not allocate a separate recovery folder. Try again.".into())
}

pub(super) fn unavailable(world: &RecentWorld) -> Option<&'static str> {
    if world.profile.config.region_store.is_none() {
        Some(
            "Portable JSON backup is manual: retain the full save, profile/config, source, politics and matching app. See Recovery bundles.",
        )
    } else if world.profile.config.live_data
        || world.profile.config.historical_pack.is_some()
        || world.profile.config.data_epoch_unix.is_some()
    {
        Some(
            "Dependency recovery for historical or live sources is not supported here. Retain all original inputs; see Recovery bundles.",
        )
    } else if !world.has_checkpoint_file {
        Some("Play and save a first checkpoint before making a backup.")
    } else {
        None
    }
}

pub(super) fn select_world(world: &RecentWorld) -> Result<WorldSelection> {
    if let Some(reason) = unavailable(world) {
        return Err(reason.into());
    }
    Ok(WorldSelection {
        path: checked_path(&world.path, true)?,
        profile: world.profile.clone(),
    })
}

/// A menu name belongs to the selected profile, never the gameplay database or
/// the retained backup. Recheck the complete displayed profile before replacing
/// that small metadata file while holding the managed world's maintenance lease;
/// list indices and an old name are not an identity.
pub(super) fn rename_world(world: &RecentWorld, user_data: &Path, title: &str) -> Result<String> {
    model::validate_name(title, 48, "World name")?;
    let parent = checked_path(&user_data.join("worlds"), true)?;
    let path = checked_path(&world.path, true)?;
    if path.file_name().is_none_or(|name| name != "world.json")
        || path.parent().and_then(Path::parent) != Some(parent.as_path())
    {
        return Err("Choose a managed world from Your worlds before renaming it.".into());
    }
    private(path.parent().unwrap(), true)?;
    private(&path, false)?;
    // Reject an already stale selection without creating coordination metadata.
    // The second read under the lease is the authoritative one: a session or
    // another maintenance operation may have published after this preflight.
    let mut current = WorldProfile::read(&path)?;
    if serde_json::to_value(&current).map_err(|e| e.to_string())?
        != serde_json::to_value(&world.profile).map_err(|e| e.to_string())?
    {
        return Err(
            "The selected world changed. Return to Your worlds and review its name again.".into(),
        );
    }
    let _lease = model::begin_managed_maintenance(&path, &current.config)?;
    current = WorldProfile::read(&path)?;
    if serde_json::to_value(&current).map_err(|e| e.to_string())?
        != serde_json::to_value(&world.profile).map_err(|e| e.to_string())?
    {
        return Err(
            "The selected world changed. Return to Your worlds and review its name again.".into(),
        );
    }
    current.title = title.trim().into();
    model::atomic_json(&path, &current)?;
    Ok(current.title)
}

pub(super) fn create_backup(selected: WorldSelection, user_data: &Path) -> Result<Backup> {
    create_backup_with_sync(
        selected,
        user_data,
        &terra_bevy::rules::publication::sync_directory,
    )
}

fn create_backup_with_sync(
    selected: WorldSelection,
    user_data: &Path,
    sync: &dyn Fn(&Path) -> std::io::Result<()>,
) -> Result<Backup> {
    let mut profile = WorldProfile::read(&selected.path)?;
    if serde_json::to_value(&profile).map_err(|e| e.to_string())?
        != serde_json::to_value(&selected.profile).map_err(|e| e.to_string())?
    {
        return Err("The selected world changed. Refresh Your worlds and review it again.".into());
    }
    // Menu backups retain both native metadata and a committed regional world.
    // Refuse active play/maintenance before allocating any output. The backend
    // online SQLite backup API remains available to its explicit callers.
    let _lease = model::begin_managed_maintenance(&selected.path, &profile.config)?;
    profile = WorldProfile::read(&selected.path)?;
    if serde_json::to_value(&profile).map_err(|e| e.to_string())?
        != serde_json::to_value(&selected.profile).map_err(|e| e.to_string())?
    {
        return Err("The selected world changed. Refresh Your worlds and review it again.".into());
    }
    let database = profile
        .config
        .region_store
        .clone()
        .ok_or("This world does not use regional checkpoints.")?;
    if !database.is_absolute()
        || [
            &profile.config.heightmap_manifest,
            &profile.config.politics_file,
        ]
        .into_iter()
        .flatten()
        .any(|path| !path.is_absolute())
    {
        return Err("The world needs resolved absolute source and checkpoint paths before it can be backed up.".into());
    }
    let witness = model::managed_start_witness_path(&selected.path);
    let lease = model::managed_session_lease_path(&selected.path);
    let parent = managed_parent(
        user_data,
        "backups",
        &[
            &database,
            &selected.path,
            &profile.config.save_path,
            &witness,
            &lease,
        ],
        sync,
    )?;
    let directory = claim_directory(&parent, "backup")?;
    let mut published = false;
    let result = (|| {
        let verified = create_recovery_bundle_with_sync(
            &database,
            &profile.config,
            directory.join(BUNDLE),
            sync,
        )
        .map_err(|error| {
            published |= error.published();
            error.to_string()
        })?;
        // The complete backend bundle is a published artifact even before its
        // optional launcher wrapper is ready. Retain it on any later failure.
        published = true;
        // A profile change during copying needs a fresh user selection too.
        let current = WorldProfile::read(&selected.path)?;
        if serde_json::to_value(&current).map_err(|e| e.to_string())?
            != serde_json::to_value(&profile).map_err(|e| e.to_string())?
        {
            return Err("The world profile changed while its backup was being created. Try again after the world is closed.".into());
        }
        let record = BackupRecord {
            schema_version: 1,
            title: profile.title,
            character: profile.character,
            quality: profile.quality,
            start_hour: profile.start_hour,
            world_created_unix_seconds: profile.created_unix_seconds,
            created_unix_seconds: now()?,
            source_generation: verified.source_generation,
            bundle_identity: verified.bundle_identity,
        };
        record.validate()?;
        // Flush the complete payload before exposing the launcher marker.
        terra_bevy::rules::publication::sync_tree(&directory, 2200, 640 * 1024 * 1024)
            .map_err(|error| error.to_string())?;
        model::write_new_json_with_sync(&directory.join(RECORD), &record, sync).map_err(
            |error| {
                published |= error.published();
                error.to_string()
            },
        )?;
        read_backup(&directory)
    })();
    if result.is_err() && !published {
        let _ = fs::remove_dir_all(&directory);
    }
    result.map_err(|error| {
        if published {
            format!("{error} Published recovery output was retained at {}. Verify the retained bundle before using it; this attempt did not confirm a completed backup.", directory.display())
        } else { error }
    })
}

pub(super) fn read_backup(directory: &Path) -> Result<Backup> {
    let directory = checked_path(directory, true)?;
    private(&directory, true)?;
    let entries = fs::read_dir(&directory).map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for entry in entries.take(3) {
        names.push(entry.map_err(|e| e.to_string())?.file_name());
    }
    names.sort();
    if names
        != [
            std::ffi::OsString::from(BUNDLE),
            std::ffi::OsString::from(RECORD),
        ]
    {
        return Err("Choose a completed Terra backup folder containing only its bundle and world-backup.json record.".into());
    }
    private(&directory.join(BUNDLE), true)?;
    private(
        &directory
            .join(BUNDLE)
            .join(terra_bevy::recovery::RECOVERY_INDEX),
        false,
    )?;
    let path = directory.join(RECORD);
    private(&path, false)?;
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > MAX_RECORD_BYTES {
        return Err("The backup record exceeds its size limit.".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .map_err(|e| e.to_string())?
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("The backup record exceeds its size limit.".into());
    }
    let record: BackupRecord =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid backup record: {e}"))?;
    record.validate()?;
    if serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())? != bytes {
        return Err("The backup record is not in its original canonical form.".into());
    }
    Ok(Backup {
        directory,
        record,
        record_bytes: bytes,
    })
}

fn unchanged(backup: &Backup) -> Result<()> {
    let current = read_backup(&backup.directory)?;
    if current.directory != backup.directory || current.record_bytes != backup.record_bytes {
        return Err("This backup's name, character or checkpoint record changed. Select and verify it again.".into());
    }
    Ok(())
}

pub(super) fn recent_backups(user_data: &Path) -> (Vec<Backup>, Vec<String>) {
    let parent = user_data.join("backups");
    if !parent.exists() {
        return (Vec::new(), Vec::new());
    }
    let entries = checked_path(&parent, true).and_then(|path| {
        private(&path, true)?;
        fs::read_dir(path).map_err(|e| e.to_string())
    });
    let entries = match entries {
        Ok(entries) => entries,
        Err(error) => {
            return (
                Vec::new(),
                vec![format!("Backups could not be listed: {error}")],
            );
        }
    };
    let mut backups = Vec::new();
    let mut warnings = Vec::new();
    for (index, entry) in entries.take(MAX_INSPECTED_BACKUPS + 1).enumerate() {
        if index == MAX_INSPECTED_BACKUPS {
            warnings.push("Only 256 recovery folders were inspected. An older copy can still be opened by its folder path.".into());
            break;
        }
        let result = entry
            .map_err(|e| e.to_string())
            .and_then(|entry| read_backup(&entry.path()));
        match result {
            Ok(backup) => backups.push(backup),
            Err(error) if warnings.len() < 3 => {
                warnings.push(format!("Skipped an incomplete or invalid backup: {error}"))
            }
            Err(_) => (),
        }
    }
    backups.sort_by(|a, b| {
        b.record
            .created_unix_seconds
            .cmp(&a.record.created_unix_seconds)
            .then_with(|| b.directory.cmp(&a.directory))
    });
    backups.truncate(MAX_RECENT_BACKUPS);
    (backups, warnings)
}

pub(super) fn review_backup(backup: Backup) -> Result<BackupReview> {
    unchanged(&backup)?;
    let verified = verify_recovery_bundle(
        backup.directory.join(BUNDLE),
        Some(&backup.record.bundle_identity),
    )?;
    if verified.source_generation != backup.record.source_generation {
        return Err("The backup's named checkpoint does not match its verified world.".into());
    }
    unchanged(&backup)?;
    let restored_title = format!(
        "{} (recovered)",
        backup
            .record
            .title
            .trim()
            .chars()
            .take(36)
            .collect::<String>()
    );
    Ok(BackupReview {
        backup,
        verified,
        restored_title,
    })
}

pub(super) fn restore_reviewed(review: BackupReview, user_data: &Path) -> Result<RestoredWorld> {
    restore_reviewed_with_sync(
        review,
        user_data,
        &terra_bevy::rules::publication::sync_directory,
    )
}

fn restore_reviewed_with_sync(
    review: BackupReview,
    user_data: &Path,
    sync: &dyn Fn(&Path) -> std::io::Result<()>,
) -> Result<RestoredWorld> {
    model::validate_name(&review.restored_title, 48, "Recovered world name")?;
    unchanged(&review.backup)?;
    review.backup.record.validate()?;
    // Revalidate prior to claiming a managed world, then the backend validates
    // again while copying. A profile published before a later synchronization
    // failure is retained and reported as uncertain, never silently deleted.
    let verified = verify_recovery_bundle(
        review.backup.directory.join(BUNDLE),
        Some(&review.backup.record.bundle_identity),
    )?;
    if verified.source_generation != review.verified.source_generation
        || verified.source_generation != review.backup.record.source_generation
    {
        return Err("The reviewed checkpoint changed. Verify the backup again.".into());
    }
    let candidate_parent = checked_path(&user_data.join("worlds"), false)?;
    if candidate_parent.starts_with(&review.backup.directory) {
        return Err("Recovered worlds must be kept outside the retained backup folder.".into());
    }
    let input_database = review.backup.directory.join(BUNDLE).join("world.db");
    let parent = managed_parent(user_data, "worlds", &[&input_database], sync)?;
    let destination = claim_directory(&parent, "recovered")?;
    let mut published = false;
    let result = (|| {
        let receipt = restore_recovery_bundle_with_sync(
            review.backup.directory.join(BUNDLE),
            destination.join("world"),
            Some(&review.backup.record.bundle_identity),
            sync,
        )
        .map_err(|error| {
            published |= error.published();
            error.to_string()
        })?;
        published = true;
        if receipt.committed_generation != verified.source_generation {
            return Err("The restored checkpoint differs from the reviewed checkpoint.".into());
        }
        unchanged(&review.backup)?;
        let config = read_recovery_config(destination.join("world/world-config.json"))?;
        let record = &review.backup.record;
        let profile = WorldProfile {
            schema_version: 1,
            title: review.restored_title.trim().into(),
            created_unix_seconds: now()?,
            last_played_unix_seconds: now()?,
            checkpoint_present: true,
            config,
            character: record.character.clone(),
            quality: record.quality,
            start_hour: record.start_hour,
        };
        let pending = destination.join(".world-profile.pending");
        model::write_new_json(&pending, &profile)?;
        WorldProfile::read(&pending)?;
        fs::remove_file(&pending).map_err(|error| error.to_string())?;
        terra_bevy::rules::publication::sync_tree(&destination, 2200, 640 * 1024 * 1024)
            .map_err(|error| error.to_string())?;
        let profile_path = destination.join("world.json");
        model::write_new_json_with_sync(&profile_path, &profile, sync).map_err(|error| {
            published |= error.published();
            error.to_string()
        })?;
        Ok(RestoredWorld {
            profile_path,
            title: profile.title,
            checkpoint: receipt.committed_generation,
        })
    })();
    if result.is_err() && !published {
        let _ = fs::remove_dir_all(&destination);
    }
    result.map_err(|error| {
        if published {
            format!("{error} Published recovery output was retained at {}. Verify its checkpoint before reopening; this attempt did not confirm a completed recovery.", destination.display())
        } else { error }
    })
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
