//! Opening the save folder in the OS file manager (design-ux-flow §3
//! "Runner", UX-08). `AppCommand::OpenSaveFolder` runs through the
//! [`FolderOpener`] the `Runtime` holds, so a test swaps in a double that
//! records the path and nothing is ever spawned from widget code.
use std::path::{Path, PathBuf};

/// Shows a directory in the OS file manager.
pub(crate) trait FolderOpener: Send + Sync + 'static {
    fn open(&self, dir: &Path) -> Result<(), String>;
}

/// The shipped opener: `explorer.exe` on Windows, `open` on macOS and
/// `xdg-open` elsewhere, started detached. Only starting the program is
/// checked; what the file manager then does is the OS's business.
#[cfg_attr(test, expect(dead_code, reason = "tests never start a file manager"))]
pub(crate) struct SystemFolderOpener;
impl FolderOpener for SystemFolderOpener {
    fn open(&self, dir: &Path) -> Result<(), String> {
        let program = if cfg!(windows) {
            "explorer.exe"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(program)
            .arg(dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(drop)
            .map_err(|error| format!("{program} could not be started: {error}"))
    }
}

/// The test double: records every directory it is asked to open, and fails
/// with `error` when one is set. Clones share the record.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct RecordingOpener {
    opened: std::sync::Arc<std::sync::Mutex<Vec<PathBuf>>>,
    error: Option<String>,
}
#[cfg(test)]
impl RecordingOpener {
    /// An opener that records the directory and then fails with `error`.
    pub(crate) fn failing(error: &str) -> Self {
        Self {
            error: Some(error.into()),
            ..Self::default()
        }
    }
    pub(crate) fn opened(&self) -> Vec<PathBuf> {
        self.opened.lock().expect("opener record").clone()
    }
}
#[cfg(test)]
impl FolderOpener for RecordingOpener {
    fn open(&self, dir: &Path) -> Result<(), String> {
        self.opened
            .lock()
            .expect("opener record")
            .push(dir.to_path_buf());
        self.error.clone().map_or(Ok(()), Err)
    }
}

/// The `Runtime`'s opener. The default is the system opener; under `cfg(test)`
/// it is an inert recorder, so no test can start a file manager by accident.
pub(super) struct FolderOpenerSlot(pub(super) Box<dyn FolderOpener>);
impl Default for FolderOpenerSlot {
    fn default() -> Self {
        #[cfg(not(test))]
        let opener = SystemFolderOpener;
        #[cfg(test)]
        let opener = RecordingOpener::default();
        Self(Box::new(opener))
    }
}
impl std::fmt::Debug for FolderOpenerSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FolderOpenerSlot")
    }
}

/// The directory to show for `profile`: the profile directory itself, or its
/// nearest existing ancestor while nothing has been saved there yet (a file
/// manager asked for a missing directory opens somewhere unrelated).
pub(super) fn folder_to_open(profile: &Path) -> PathBuf {
    profile
        .ancestors()
        .find(|dir| dir.is_dir())
        .unwrap_or(profile)
        .to_path_buf()
}
