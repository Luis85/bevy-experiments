//! Host-only file identities shared by notes and maps. Parsing remains cached,
//! but every refresh checks the bytes, including edits preserving mtime/size.
use super::NotesError;
use std::path::Path;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fingerprint {
    pub modified: Option<SystemTime>,
    pub len: u64,
    pub digest: [u8; 32],
}
pub(crate) fn fingerprint(meta: &std::fs::Metadata, bytes: &[u8]) -> Fingerprint {
    Fingerprint {
        modified: meta.modified().ok(),
        len: bytes.len() as u64,
        digest: *blake3::hash(bytes).as_bytes(),
    }
}
pub(crate) fn io_err(path: &Path, e: &std::io::Error) -> NotesError {
    NotesError::Io(format!("{}: {e}", path.display()))
}
pub(crate) fn read(path: &Path) -> Result<(Vec<u8>, Fingerprint), NotesError> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path).map_err(|e| io_err(path, &e))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| io_err(path, &e))?;
    let meta = file.metadata().map_err(|e| io_err(path, &e))?;
    let seen = fingerprint(&meta, &bytes);
    Ok((bytes, seen))
}
pub(crate) fn write_atomic(path: &Path, text: &str) -> Result<Fingerprint, NotesError> {
    use std::io::Write as _;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)?;
        std::fs::metadata(path)
    })();
    match result {
        Ok(meta) => Ok(fingerprint(&meta, text.as_bytes())),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(io_err(path, &e))
        }
    }
}
/// Returns true when removed/already absent, false when somebody changed it.
pub(crate) fn remove_unchanged(path: &Path, expected: Fingerprint) -> Result<bool, NotesError> {
    let (bytes, current) = match read(path) {
        Ok(v) => v,
        Err(e) => {
            if !path.exists() {
                return Ok(true);
            }
            return Err(e);
        }
    };
    if current.len != expected.len || current.digest != expected.digest {
        return Ok(false);
    }
    drop(bytes);
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(io_err(path, &e)),
    }
}
pub(crate) fn next_id(max: u32) -> Result<u32, NotesError> {
    max.checked_add(1).ok_or(NotesError::Exhausted("entry ids"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unchanged_removal_preserves_equal_size_edited_source() {
        let dir = std::env::temp_dir().join(format!("npc-file-identity-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("source.md");
        let expected = write_atomic(&path, "old").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::write(&path, "new").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert!(!remove_unchanged(&path, expected).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
