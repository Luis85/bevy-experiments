use crate::{Catalog, MAX_SAVE_BYTES, SandboxError, SandboxState};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

impl SandboxState {
    /// Validate and atomically rename a same-directory file. The previous save
    /// remains untouched if serialization/writing fails before rename.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SandboxError> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        if bytes.len() > MAX_SAVE_BYTES {
            return Err(SandboxError::Capacity);
        }
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let name = path
            .file_name()
            .ok_or_else(|| SandboxError::Invalid("save path".into()))?
            .to_string_lossy();
        let temporary = path.with_file_name(format!(".{name}.{}.{nonce}.tmp", std::process::id()));
        let result = (|| -> Result<(), SandboxError> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)?;
            // Best-effort directory sync on platforms that support directory
            // handles. The file data itself was synchronously flushed above.
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                if let Ok(directory) = File::open(parent) {
                    let _ = directory.sync_all();
                }
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    /// Bounded parse followed by complete version/ownership/reference checks.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, SandboxError> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take(MAX_SAVE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_SAVE_BYTES {
            return Err(SandboxError::Capacity);
        }
        let state: Self = serde_json::from_slice(&bytes)?;
        state.validate()?;
        Ok(state)
    }

    /// Restore under a particular game's rules as well as the structural save
    /// format. Changed blueprint/crop parameters require an explicit migration.
    pub fn load_with_catalog(
        path: impl AsRef<Path>,
        catalog: &Catalog,
    ) -> Result<Self, SandboxError> {
        let state = Self::load(path)?;
        state.validate_with_catalog(catalog)?;
        Ok(state)
    }
}
