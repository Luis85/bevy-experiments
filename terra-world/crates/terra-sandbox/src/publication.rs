//! Private, same-directory file publication with explicit failure stages.
//!
//! File bytes are flushed before publication. Unix also synchronizes directory
//! entries and newly created parent directories. Other platforms retain checked
//! file flush and namespace publication, but this module does not claim a Unix
//! directory-fsync equivalent or physical power-loss qualification there.
//! Callers must preserve a visible destination after a `published()` error.
//! Cooperative, quiescent filesystem paths are required; this is not protection
//! against an administrator replacing path ancestors during an operation.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishMode {
    Replace,
    CreateNew,
}

#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    #[error("File was not published: {0}. Any previous destination was preserved.")]
    NotPublished(#[source] io::Error),
    #[error(
        "File was published but its durability could not be confirmed: {0}. Keep and verify the visible output before retrying; it was not rolled back."
    )]
    Published(#[source] io::Error),
}

impl PublicationError {
    pub fn published(&self) -> bool {
        matches!(self, Self::Published(_))
    }
}

pub type PublicationResult<T> = Result<T, PublicationError>;

/// Explicit Unix namespace flush. Non-Unix success means that this additional
/// operation is unavailable here, not that Windows crash durability was tested.
pub fn sync_directory(directory: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(directory)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        Ok(())
    }
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn prepare_parent(path: &Path) -> io::Result<()> {
    create_private_directories(parent(path))
}

/// Create missing save/recovery parents privately and flush every newly created
/// directory entry before any caller publishes payload beneath it. Existing
/// directories are retained with their current permissions.
pub fn create_private_directories(directory: &Path) -> io::Result<()> {
    create_private_directories_with_sync(directory, sync_directory)
}

/// Explicit directory synchronization boundary for host adapters and fault tests.
pub fn create_private_directories_with_sync(
    directory: &Path,
    sync: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut missing = Vec::new();
    for candidate in directory.ancestors() {
        if candidate.as_os_str().is_empty() {
            break;
        }
        match fs::metadata(candidate) {
            Ok(metadata) if metadata.is_dir() => break,
            Ok(_) => return Err(io::Error::other("publication parent is not a directory")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => missing.push(candidate),
            Err(error) => return Err(error),
        }
    }
    let mut owned = Vec::new();
    let result = (|| {
        for candidate in missing.iter().rev() {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(candidate) {
                Ok(()) => owned.push(*candidate),
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists
                        && fs::metadata(candidate).is_ok_and(|metadata| metadata.is_dir()) =>
                {
                    ()
                }
                Err(error) => return Err(error),
            }
        }
        // Synchronize the full newly missing chain, including a directory
        // another cooperating creator won. Keep this before payload publication.
        for created in &missing {
            sync(created)?;
            sync(parent(created))?;
        }
        Ok(())
    })();
    if result.is_err() {
        // Only remove empty directories this invocation exclusively created.
        // A rejected first attempt cannot make its unflushed empty parents look
        // previously admitted on retry. Nonempty/raced directories are retained.
        for created in owned.into_iter().rev() {
            let _ = fs::remove_dir(created);
        }
    }
    result
}

fn check_destination(path: &Path, mode: PublishMode) -> io::Result<()> {
    if path.file_name().is_none() {
        return Err(io::Error::other("publication needs a filename"));
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if mode == PublishMode::CreateNew => {
            let _ = metadata;
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination already exists",
            ))
        }
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => Err(
            io::Error::other("publication destination must be a regular file"),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn temporary(path: &Path) -> PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    parent(path).join(format!(
        ".terra-publish-{}-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SERIAL.fetch_add(1, Ordering::Relaxed),
    ))
}

struct OwnedTemporary(PathBuf);
impl Drop for OwnedTemporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    Write,
    FileSync,
    Publish,
    Cleanup,
}

/// Flush private bytes and publish them with the requested replacement policy.
pub fn write_private(path: &Path, bytes: &[u8], mode: PublishMode) -> PublicationResult<()> {
    write_private_with_sync(path, bytes, mode, sync_directory)
}

/// The callback receives the actual containing directory after publication.
/// This explicit boundary supports host filesystem adapters and fault testing;
/// a callback must honor the host's documented synchronization contract.
pub fn write_private_with_sync(
    path: &Path,
    bytes: &[u8],
    mode: PublishMode,
    sync: impl FnOnce(&Path) -> io::Result<()>,
) -> PublicationResult<()> {
    write_private_impl(path, bytes, mode, &temporary(path), sync, |_| Ok(()))
}

fn write_private_impl(
    path: &Path,
    bytes: &[u8],
    mode: PublishMode,
    temporary: &Path,
    sync: impl FnOnce(&Path) -> io::Result<()>,
    boundary: impl Fn(Boundary) -> io::Result<()>,
) -> PublicationResult<()> {
    let before = PublicationError::NotPublished;
    prepare_parent(path).map_err(before)?;
    check_destination(path, mode).map_err(before)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(temporary).map_err(before)?;
    // Do not claim cleanup ownership until exclusive creation succeeded.
    let _owned = OwnedTemporary(temporary.to_owned());
    boundary(Boundary::Write).map_err(before)?;
    file.write_all(bytes).map_err(before)?;
    boundary(Boundary::FileSync).map_err(before)?;
    file.sync_all().map_err(before)?;
    drop(file);
    boundary(Boundary::Publish).map_err(before)?;
    match mode {
        PublishMode::Replace => fs::rename(temporary, path),
        PublishMode::CreateNew => fs::hard_link(temporary, path),
    }
    .map_err(before)?;
    // Everything after namespace publication must retain the destination and
    // identify uncertainty. A failed unlink is not a prepublication failure.
    let cleanup = boundary(Boundary::Cleanup).and_then(|()| {
        if mode == PublishMode::CreateNew {
            fs::remove_file(temporary)
        } else {
            Ok(())
        }
    });
    let synchronized = sync(parent(path));
    cleanup.map_err(PublicationError::Published)?;
    synchronized.map_err(PublicationError::Published)
}

/// Publish an already prepared regular file without replacing a destination.
/// The source remains owned by the caller (for example a private backup tempdir).
/// The caller supplies an already private source; hard links preserve its inode
/// permissions instead of changing another owned file's security attributes.
pub fn link_private_file(source: &Path, destination: &Path) -> PublicationResult<()> {
    link_private_file_with_sync(source, destination, sync_directory)
}

pub fn link_private_file_with_sync(
    source: &Path,
    destination: &Path,
    sync: impl FnOnce(&Path) -> io::Result<()>,
) -> PublicationResult<()> {
    let before = PublicationError::NotPublished;
    prepare_parent(destination).map_err(before)?;
    check_destination(destination, PublishMode::CreateNew).map_err(before)?;
    let metadata = fs::symlink_metadata(source).map_err(before)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(before(io::Error::other(
            "staged publication must be a regular file",
        )));
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(source)
        .and_then(|file| file.sync_all())
        .map_err(before)?;
    fs::hard_link(source, destination).map_err(before)?;
    sync(parent(destination)).map_err(PublicationError::Published)
}

/// Flush a bounded, already validated payload tree before publishing its final
/// completion marker. Reject links/special files. Include the root's parent so
/// the newly created tree has a checked namespace boundary too.
pub fn sync_tree(root: &Path, max_entries: usize, max_bytes: u64) -> io::Result<()> {
    let mut pending = vec![root.to_owned()];
    let mut directories = Vec::new();
    let mut count = 0usize;
    let mut bytes = 0u64;
    while let Some(path) = pending.pop() {
        count = count
            .checked_add(1)
            .ok_or_else(|| io::Error::other("tree entry overflow"))?;
        if count > max_entries {
            return Err(io::Error::other("publication tree exceeds its entry bound"));
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::other(
                "publication trees cannot contain symbolic links",
            ));
        }
        if metadata.is_dir() {
            directories.push(path.clone());
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
                if pending.len() > max_entries {
                    return Err(io::Error::other("publication tree exceeds its entry bound"));
                }
            }
        } else if metadata.is_file() {
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or_else(|| io::Error::other("tree byte overflow"))?;
            if bytes > max_bytes {
                return Err(io::Error::other("publication tree exceeds its byte bound"));
            }
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)?
                .sync_all()?;
        } else {
            return Err(io::Error::other("publication tree contains a special file"));
        }
    }
    for directory in directories.into_iter().rev() {
        sync_directory(&directory)?;
    }
    sync_directory(parent(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = temporary(&std::env::temp_dir().join("test")).with_extension("dir");
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fault() -> io::Error {
        io::Error::other("injected storage failure")
    }

    #[test]
    fn failures_before_publication_preserve_old_bytes_and_only_clean_owned_staging() {
        let scratch = Scratch::new();
        let destination = scratch.0.join("save.json");
        let staged = scratch.0.join("staging");
        fs::write(&destination, b"old bytes").unwrap();
        for failed in [Boundary::Write, Boundary::FileSync, Boundary::Publish] {
            let error = write_private_impl(
                &destination,
                b"new bytes",
                PublishMode::Replace,
                &staged,
                sync_directory,
                |boundary| {
                    if boundary == failed {
                        Err(fault())
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
            assert!(!error.published());
            assert_eq!(fs::read(&destination).unwrap(), b"old bytes");
            assert!(!staged.exists());
        }
        fs::write(&staged, b"belongs to another writer").unwrap();
        assert!(
            !write_private_impl(
                &destination,
                b"new",
                PublishMode::Replace,
                &staged,
                sync_directory,
                |_| Ok(())
            )
            .unwrap_err()
            .published()
        );
        assert_eq!(fs::read(&staged).unwrap(), b"belongs to another writer");
        assert_eq!(fs::read(&destination).unwrap(), b"old bytes");
    }

    #[test]
    fn sync_failures_after_replace_or_create_keep_exact_visible_bytes() {
        let scratch = Scratch::new();
        for mode in [PublishMode::Replace, PublishMode::CreateNew] {
            let destination = scratch.0.join(format!("{mode:?}.json"));
            if mode == PublishMode::Replace {
                fs::write(&destination, b"previous").unwrap();
            }
            let error =
                write_private_with_sync(&destination, b"new complete bytes", mode, |directory| {
                    assert_eq!(directory, scratch.0);
                    assert_eq!(fs::read(&destination).unwrap(), b"new complete bytes");
                    Err(fault())
                })
                .unwrap_err();
            assert!(error.published());
            assert!(error.to_string().contains("not rolled back"));
            assert_eq!(fs::read(&destination).unwrap(), b"new complete bytes");
            assert!(
                !write_private(&destination, b"would overwrite", PublishMode::CreateNew)
                    .unwrap_err()
                    .published()
            );
            assert_eq!(fs::read(&destination).unwrap(), b"new complete bytes");
        }
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 2);
    }

    #[test]
    fn failed_post_link_cleanup_is_a_published_outcome_and_keeps_destination() {
        let scratch = Scratch::new();
        let destination = scratch.0.join("new.json");
        let staged = scratch.0.join("staged");
        let error = write_private_impl(
            &destination,
            b"complete",
            PublishMode::CreateNew,
            &staged,
            sync_directory,
            |boundary| {
                if boundary == Boundary::Cleanup {
                    Err(fault())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
        assert!(error.published());
        assert_eq!(fs::read(&destination).unwrap(), b"complete");
        assert!(!staged.exists());
    }

    #[test]
    fn linked_snapshot_reports_uncertainty_without_deleting_source_or_destination() {
        let scratch = Scratch::new();
        let source = scratch.0.join("snapshot");
        let destination = scratch.0.join("backup");
        fs::write(&source, b"validated snapshot").unwrap();
        let error =
            link_private_file_with_sync(&source, &destination, |_| Err(fault())).unwrap_err();
        assert!(error.published());
        assert_eq!(fs::read(&source).unwrap(), b"validated snapshot");
        assert_eq!(fs::read(&destination).unwrap(), b"validated snapshot");
        assert!(
            !link_private_file(&source, &destination)
                .unwrap_err()
                .published()
        );
    }

    #[test]
    fn newly_created_parent_chain_is_synced_before_payload_publication() {
        let scratch = Scratch::new();
        let directory = scratch.0.join("new/deep");
        let observed = std::cell::RefCell::new(Vec::new());
        create_private_directories_with_sync(&directory, |path| {
            observed.borrow_mut().push(path.to_owned());
            sync_directory(path)
        })
        .unwrap();
        let observed = observed.into_inner();
        assert!(observed.contains(&directory));
        assert!(observed.contains(&scratch.0.join("new")));
        assert!(observed.contains(&scratch.0));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [&directory, &scratch.0.join("new")] {
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o700
                );
            }
        }
        let refused = scratch.0.join("unconfirmed/new-parent");
        assert!(create_private_directories_with_sync(&refused, |_| Err(fault())).is_err());
        assert!(!refused.join("save.json").exists());
    }

    #[test]
    fn payload_tree_flush_is_bounded_and_accepts_nested_regular_files() {
        let scratch = Scratch::new();
        fs::create_dir(scratch.0.join("nested")).unwrap();
        fs::write(scratch.0.join("one"), b"1234").unwrap();
        fs::write(scratch.0.join("nested/two"), b"5678").unwrap();
        sync_tree(&scratch.0, 4, 8).unwrap();
        assert!(sync_tree(&scratch.0, 3, 8).is_err());
        assert!(sync_tree(&scratch.0, 4, 7).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("one", scratch.0.join("alias")).unwrap();
            assert!(sync_tree(&scratch.0, 8, 16).is_err());
        }
    }

    #[test]
    fn relative_filename_flushes_dot_and_private_permissions_survive_replace() {
        const CHILD: &str = "TERRA_PUBLICATION_CHILD";
        if std::env::var_os(CHILD).is_some() {
            for mode in [PublishMode::CreateNew, PublishMode::Replace] {
                write_private_with_sync(
                    Path::new("relative.json"),
                    b"private bytes",
                    mode,
                    |directory| {
                        assert_eq!(directory, Path::new("."));
                        sync_directory(directory)
                    },
                )
                .unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        fs::metadata("relative.json").unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                }
            }
            return;
        }
        let scratch = Scratch::new();
        #[cfg(unix)]
        let mut command = {
            let mut command = std::process::Command::new("sh");
            command.args(["-c", "umask 000; exec \"$1\" --exact publication::tests::relative_filename_flushes_dot_and_private_permissions_survive_replace --nocapture", "terra-publication"]);
            command.arg(std::env::current_exe().unwrap());
            command
        };
        #[cfg(not(unix))]
        let mut command = {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", "publication::tests::relative_filename_flushes_dot_and_private_permissions_survive_replace", "--nocapture"]);
            command
        };
        let output = command
            .env(CHILD, "1")
            .current_dir(&scratch.0)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(scratch.0.join("relative.json")).unwrap(),
            b"private bytes"
        );
    }
}
