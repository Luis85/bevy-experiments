//! A tiny versioned, bounded, atomic disk cache. Only our own `.cache` files are
//! managed; corrupt or expired entries are treated as misses.
use crate::{StreamConfig, StreamError};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

const MAGIC: &[u8; 8] = b"TERRA001";
const HEADER: usize = 32;
static WRITES: Mutex<()> = Mutex::new(());
static SERIAL: AtomicU64 = AtomicU64::new(0);

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Stable FNV-1a key checksum. This is a corruption detector, not authentication.
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |state, byte| {
        (state ^ *byte as u64).wrapping_mul(0x100000001b3)
    })
}

fn file_path(config: &StreamConfig, key: &str) -> PathBuf {
    config
        .cache_dir
        .join("terra-v1")
        .join(format!("{:016x}.cache", fingerprint(key.as_bytes())))
}

pub(crate) fn read(config: &StreamConfig, key: &str, ttl: u64) -> Option<Vec<u8>> {
    read_timed(config, key, ttl).map(|(body, _)| body)
}

/// Preserve the retrieval timestamp when cached source data are assembled into
/// a newly requested geographic chunk; reading the cache is not a fresh survey.
pub(crate) fn read_timed(config: &StreamConfig, key: &str, ttl: u64) -> Option<(Vec<u8>, i64)> {
    let path = file_path(config, key);
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file()
        || metadata.len() > config.max_response_bytes as u64 + HEADER as u64
        || metadata.len() < HEADER as u64
    {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    let mut data = Vec::with_capacity(metadata.len() as usize);
    file.take(config.max_response_bytes as u64 + HEADER as u64 + 1)
        .read_to_end(&mut data)
        .ok()?;
    if data.len() < HEADER || &data[..8] != MAGIC {
        return None;
    }
    let stored = i64::from_le_bytes(data[8..16].try_into().ok()?);
    let length = u64::from_le_bytes(data[16..24].try_into().ok()?);
    let hash = u64::from_le_bytes(data[24..32].try_into().ok()?);
    let age = now().checked_sub(stored)?;
    if age < 0
        || age as u64 > ttl
        || length as usize != data.len() - HEADER
        || length > config.max_response_bytes as u64
    {
        return None;
    }
    let body = data.split_off(HEADER);
    (fingerprint(&body) == hash).then_some((body, stored))
}

pub(crate) fn write(config: &StreamConfig, key: &str, body: &[u8]) -> Result<(), StreamError> {
    if body.len() > config.max_response_bytes {
        return Err(StreamError::Payload(
            "cache entry exceeds response budget".into(),
        ));
    }
    if body.len() as u64 + HEADER as u64 > config.max_disk_cache_bytes {
        return Err(StreamError::Payload(
            "cache entry including its header exceeds total disk budget".into(),
        ));
    }
    let _lock = WRITES.lock().unwrap_or_else(|p| p.into_inner());
    let path = file_path(config, key);
    let directory = path.parent().expect("cache path has parent");
    fs::create_dir_all(directory)?;
    let temp = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(MAGIC)?;
        file.write_all(&now().to_le_bytes())?;
        file.write_all(&(body.len() as u64).to_le_bytes())?;
        file.write_all(&fingerprint(body).to_le_bytes())?;
        file.write_all(body)?;
        file.sync_all()?;
        fs::rename(&temp, &path)?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    let mut entries: Vec<_> = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "cache"))
        .filter_map(|entry| {
            entry
                .metadata()
                .ok()
                .map(|m| (entry.path(), m.len(), m.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect();
    let mut total: u64 = entries.iter().map(|(_, len, _)| len).sum();
    entries.sort_by_key(|(_, _, modified)| *modified);
    for (old_path, size, _) in entries {
        if total <= config.max_disk_cache_bytes {
            break;
        }
        // The just-written item can exceed the total only if misconfigured, which validation rejects.
        if old_path == path {
            continue;
        }
        if fs::remove_file(old_path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(name: &str) -> StreamConfig {
        StreamConfig {
            cache_dir: std::env::temp_dir().join(format!(
                "terra-cache-test-{}-{}-{name}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            )),
            ..Default::default()
        }
    }

    #[test]
    fn cache_detects_corruption_and_separates_provider_keys() {
        let c = config("integrity");
        write(&c, "provider-one", b"hello").unwrap();
        assert_eq!(read(&c, "provider-one", 100), Some(b"hello".to_vec()));
        assert!(read(&c, "provider-two", 100).is_none());
        fs::write(file_path(&c, "provider-one"), b"truncated").unwrap();
        assert!(read(&c, "provider-one", 100).is_none());
        fs::remove_dir_all(c.cache_dir).unwrap();
    }

    #[test]
    fn cache_budget_evicts_old_entries_without_touching_unrelated_files() {
        let mut c = config("budget");
        c.max_disk_cache_bytes = 1500;
        c.max_response_bytes = 1024;
        write(&c, "one", &vec![1; 900]).unwrap();
        let unrelated = c.cache_dir.join("terra-v1").join("notes.txt");
        fs::write(&unrelated, b"keep me").unwrap();
        write(&c, "two", &vec![2; 900]).unwrap();
        assert!(read(&c, "one", 100).is_none());
        assert!(read(&c, "two", 100).is_some());
        assert!(unrelated.exists());
        fs::remove_dir_all(c.cache_dir).unwrap();
    }

    #[test]
    fn cached_sources_keep_original_timestamp_and_obey_ttl() {
        let c = config("timestamps");
        write(&c, "terrain", b"source").unwrap();
        let path = file_path(&c, "terrain");
        let mut envelope = fs::read(&path).unwrap();
        let retrieved = now() - 50;
        envelope[8..16].copy_from_slice(&retrieved.to_le_bytes());
        fs::write(path, envelope).unwrap();
        assert_eq!(
            read_timed(&c, "terrain", 100),
            Some((b"source".to_vec(), retrieved))
        );
        assert!(read(&c, "terrain", 10).is_none());
        fs::remove_dir_all(c.cache_dir).unwrap();
    }
}
