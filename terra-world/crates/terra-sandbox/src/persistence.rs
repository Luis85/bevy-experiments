use crate::{Catalog, MAX_SAVE_BYTES, SandboxError, SandboxState};
use std::fs::File;
use std::io::Read;
use std::path::Path;

impl SandboxState {
    /// Validate and atomically rename a same-directory file. The previous save
    /// remains untouched on failure before publication. A published/uncertain
    /// error retains the new visible bytes and must not be treated as rollback.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SandboxError> {
        self.require_complete()?;
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        if bytes.len() > MAX_SAVE_BYTES {
            return Err(SandboxError::Capacity);
        }
        crate::publication::write_private(
            path.as_ref(),
            &bytes,
            crate::publication::PublishMode::Replace,
        )?;
        Ok(())
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
        state.require_complete()?;
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
