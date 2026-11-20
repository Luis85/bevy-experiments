//! Local capability storage. A session belongs to one exact server address.
use std::{io::Read, net::SocketAddr, path::Path};

use serde::{Deserialize, Serialize};

use crate::{NetError, Result};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionIdentity {
    pub server: SocketAddr,
    /// Secret bearer capability. Deliberately omitted from Debug/log output.
    pub resume_token: String,
}

impl SessionIdentity {
    /// A missing file starts a guest session. Corrupt or differently pinned
    /// files are errors, never an implicit request for a new identity.
    pub fn load(path: impl AsRef<Path>, expected_server: SocketAddr) -> Result<Option<Self>> {
        let path = path.as_ref();
        match std::fs::metadata(path) {
            Ok(metadata) if !metadata.is_file() => {
                return Err(NetError::Protocol(
                    "session path must be a regular file".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let file = std::fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(NetError::Protocol("session file exceeds 4096 bytes".into()));
        }
        let identity: Self = serde_json::from_slice(&bytes)?;
        identity.validate()?;
        if identity.server != expected_server {
            return Err(NetError::Protocol(
                "session file belongs to a different server address; use a separate session path"
                    .into(),
            ));
        }
        Ok(Some(identity))
    }

    /// Same-directory atomic replacement, with the private permissions supplied
    /// by the shared publisher (0600 on Unix). A post-publication sync error
    /// retains the visible identity and rejects admission. Tokens are never printed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        self.save_with_sync(path.as_ref(), terra_sandbox::publication::sync_directory)
    }

    /// Explicit final namespace synchronization boundary for host adapters and
    /// fault testing. The capability is retained on a published/uncertain error.
    pub fn save_with_sync(
        &self,
        path: &Path,
        sync: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<()> {
        self.validate()?;
        terra_sandbox::publication::write_private_with_sync(
            path,
            &serde_json::to_vec_pretty(self)?,
            terra_sandbox::publication::PublishMode::Replace,
            sync,
        )?;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if self.server.port() == 0
            || self.resume_token.len() != 64
            || !self
                .resume_token
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(NetError::Protocol("invalid stored session identity".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertain_identity_publication_returns_error_and_retains_private_resume_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        let server = "127.0.0.1:7777".parse().unwrap();
        let identity = SessionIdentity {
            server,
            resume_token: "cd".repeat(32),
        };
        let error = identity
            .save_with_sync(&path, |_| {
                Err(std::io::Error::other("injected parent sync failure"))
            })
            .unwrap_err();
        assert!(matches!(&error, NetError::Publication(error) if error.published()));
        assert!(!error.to_string().contains(&identity.resume_token));
        assert_eq!(
            SessionIdentity::load(&path, server)
                .unwrap()
                .unwrap()
                .resume_token,
            identity.resume_token
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
        // Retrying to this same identity uses the visible validated file, not a
        // silently substituted guest identity.
        identity.save(&path).unwrap();
    }

    #[test]
    fn identity_is_atomic_bounded_and_pinned_to_server() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("player.json");
        let server = "127.0.0.1:7777".parse().unwrap();
        assert!(SessionIdentity::load(&path, server).unwrap().is_none());
        let identity = SessionIdentity {
            server,
            resume_token: "ab".repeat(32),
        };
        identity.save(&path).unwrap();
        assert_eq!(
            SessionIdentity::load(&path, server)
                .unwrap()
                .unwrap()
                .resume_token,
            identity.resume_token
        );
        assert!(SessionIdentity::load(&path, "127.0.0.1:7778".parse().unwrap()).is_err());
        let invalid = SessionIdentity {
            server,
            resume_token: "bad".into(),
        };
        assert!(invalid.save(&path).is_err());
        assert_eq!(
            SessionIdentity::load(&path, server)
                .unwrap()
                .unwrap()
                .resume_token,
            identity.resume_token
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
                0
            );
        }
        std::fs::write(&path, vec![b' '; 4097]).unwrap();
        assert!(SessionIdentity::load(&path, server).is_err());
    }
}
