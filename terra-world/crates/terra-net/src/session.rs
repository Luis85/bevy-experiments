//! Local capability storage. A session belongs to one exact server address.
use std::{
    io::{Read, Write},
    net::SocketAddr,
    path::Path,
};

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
    /// by NamedTempFile (0600 on Unix). The capability is never printed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        self.validate()?;
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec_pretty(self)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| NetError::Io(error.error))?;
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
