//! Purpose:
//! Materializes catalogued HTTPS source archives through an injected bounded downloader.
//!
//! Called from:
//! - Native artifact installation while holding the source content-address lock.
//!
//! Key details:
//! - Offline mode never invokes the downloader and all bytes are size/SHA verified before publication.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ureq::{Agent, ResponseExt};
use sha2::{Digest, Sha256};

use super::catalog::SourceArchive;
use super::error::{NativeError, NativeErrorKind};
use super::util::{hash_file, unique_sibling};

/// Injected source transport used by production and network-free tests.
pub trait Downloader {
    /// Writes at most the catalog body limit to a unique unpublished destination.
    fn download_to(&self, source: &SourceArchive, destination: &Path) -> Result<(), NativeError>;
}

/// Blocking Rustls-backed production HTTPS downloader.
pub struct HttpsDownloader {
    agent: Agent,
}

impl HttpsDownloader {
    /// Builds a client with frozen redirect, connection, read, and total request bounds.
    pub fn new() -> Result<Self, NativeError> {
        let config = Agent::config_builder()
            .https_only(true)
            .max_redirects(5)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(60)))
            .timeout_global(Some(Duration::from_secs(5 * 60)))
            .build();
        Ok(Self { agent: config.into() })
    }
}

impl Downloader for HttpsDownloader {
    /// Streams one HTTPS response to disk and rejects a body beyond the catalog cap.
    fn download_to(&self, source: &SourceArchive, destination: &Path) -> Result<(), NativeError> {
        if !source.https_url.starts_with("https://") {
            return Err(NativeError::new(NativeErrorKind::Network, "catalog source URL is not HTTPS"));
        }
        let mut response = self.agent.get(source.https_url).call()
            .map_err(|error| NativeError::new(NativeErrorKind::Network, format!("download failed for trusted source '{}': {error}", source.https_url)))?;
        if response.get_uri().scheme_str() != Some("https") {
            return Err(NativeError::new(NativeErrorKind::Network, "native download ended at a non-HTTPS URL"));
        }
        if response.headers().get("content-length").and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<u64>().ok()).is_some_and(|length| length > source.body_limit) {
            return Err(NativeError::new(NativeErrorKind::Integrity, format!("native source body exceeds {} byte limit", source.body_limit)));
        }
        let mut output = OpenOptions::new().write(true).create_new(true).open(destination)
            .map_err(|error| NativeError::io("create unique native download temporary file", destination, error))?;
        let mut reader = response.body_mut().as_reader();
        let mut total = 0_u64;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer).map_err(|error| NativeError::new(NativeErrorKind::Network, format!("read native HTTPS response: {error}")))?;
            if read == 0 { break; }
            total = total.checked_add(read as u64).ok_or_else(|| NativeError::new(NativeErrorKind::Integrity, "native source length overflow"))?;
            if total > source.body_limit {
                return Err(NativeError::new(NativeErrorKind::Integrity, format!("native source body exceeds {} byte limit", source.body_limit)));
            }
            digest.update(&buffer[..read]);
            output.write_all(&buffer[..read]).map_err(|error| NativeError::io("write native download temporary file", destination, error))?;
        }
        let sha256 = format!("{:x}", digest.finalize());
        if total != source.exact_size || sha256 != source.sha256 {
            return Err(NativeError::new(
                NativeErrorKind::Integrity,
                format!("native source stream checksum/size mismatch: expected {} bytes and {}, got {} bytes and {}", source.exact_size, source.sha256, total, sha256),
            ));
        }
        output.sync_all().map_err(|error| NativeError::io("flush native download temporary file", destination, error))
    }
}

/// Reuses or downloads a verified source archive and atomically publishes it by digest.
pub fn ensure_source(
    cached: &Path,
    source: &SourceArchive,
    offline: bool,
    downloader: &dyn Downloader,
) -> Result<PathBuf, NativeError> {
    ensure_source_with_publish(
        cached,
        source,
        offline,
        downloader,
        |temporary, cached| fs::rename(temporary, cached),
    )
}

/// Runs source materialization with an injected verified-cache publication operation.
fn ensure_source_with_publish<F>(
    cached: &Path,
    source: &SourceArchive,
    offline: bool,
    downloader: &dyn Downloader,
    publish: F,
) -> Result<PathBuf, NativeError>
where
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    let mut quarantine = None;
    if source_node_exists(cached)? {
        match verify_source(cached, source) {
            Ok(()) => return Ok(cached.to_path_buf()),
            Err(error) if offline => return Err(error),
            Err(_) => {
                let path = unique_sibling(cached, "quarantine");
                fs::rename(cached, &path).map_err(|error| NativeError::io("quarantine corrupt native source", cached, error))?;
                quarantine = Some(path);
            }
        }
    }
    if offline {
        return Err(NativeError::new(NativeErrorKind::Network, format!("offline mode: verified source {} is not cached", source.sha256)).with_path(cached));
    }
    if let Some(parent) = cached.parent() {
        fs::create_dir_all(parent).map_err(|error| NativeError::io("create native source cache", parent, error))?;
    }
    let temporary = unique_sibling(cached, "download");
    let result = (|| {
        downloader.download_to(source, &temporary)?;
        verify_source(&temporary, source)?;
        match publish(&temporary, cached) {
            Ok(()) => Ok(cached.to_path_buf()),
            Err(error) => {
                if source_node_exists(cached)? {
                    verify_source(cached, source)?;
                    Ok(cached.to_path_buf())
                } else {
                    Err(NativeError::io("publish verified native source", cached, error))
                }
            }
        }
    })();
    let _ = fs::remove_file(&temporary);
    if result.is_err() {
        if let Some(quarantine) = &quarantine {
            if source_node_exists(cached).is_ok_and(|exists| !exists) { let _ = fs::rename(quarantine, cached); }
        }
    } else if let Some(quarantine) = &quarantine {
        remove_quarantine(quarantine);
    }
    result
}

/// Detects an exact source-cache node, including a broken symlink, without following it.
fn source_node_exists(path: &Path) -> Result<bool, NativeError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(NativeError::io("inspect exact native source node", path, error)),
    }
}

/// Requires the catalog's exact compressed size and SHA-256 identity.
pub fn verify_source(path: &Path, source: &SourceArchive) -> Result<(), NativeError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| NativeError::io("inspect cached native source", path, error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(NativeError::new(NativeErrorKind::Integrity, "cached native source is not a regular non-symlink file").with_path(path));
    }
    let (size, sha256) = hash_file(path)?;
    if size != source.exact_size || sha256 != source.sha256 {
        return Err(NativeError::new(
            NativeErrorKind::Integrity,
            format!("native source checksum/size mismatch: expected {} bytes and {}, got {} bytes and {}", source.exact_size, source.sha256, size, sha256),
        ).with_path(path));
    }
    Ok(())
}

/// Removes one exact corrupt-source quarantine after verified replacement.
fn remove_quarantine(path: &Path) {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink()) {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::cell::{Cell, RefCell};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// In-memory fake downloader that records whether network transport was requested.
    struct FakeDownloader<'a> { bytes: &'a [u8], calls: Cell<usize> }

    impl Downloader for FakeDownloader<'_> {
        /// Writes fixture bytes and increments the observable call counter.
        fn download_to(&self, _source: &SourceArchive, destination: &Path) -> Result<(), NativeError> {
            self.calls.set(self.calls.get() + 1);
            fs::write(destination, self.bytes).map_err(|error| NativeError::io("write fake download", destination, error))
        }
    }

    /// Fake downloader that simulates another process publishing the verified cache entry.
    struct ConcurrentPublisherDownloader<'a> {
        bytes: &'a [u8],
        cached: &'a Path,
        calls: Cell<usize>,
    }

    impl Downloader for ConcurrentPublisherDownloader<'_> {
        /// Writes both the unpublished temporary and a concurrent verified cache winner.
        fn download_to(&self, _source: &SourceArchive, destination: &Path) -> Result<(), NativeError> {
            self.calls.set(self.calls.get() + 1);
            fs::write(destination, self.bytes)
                .map_err(|error| NativeError::io("write concurrent fake download", destination, error))?;
            fs::write(self.cached, self.bytes)
                .map_err(|error| NativeError::io("publish concurrent fake cache", self.cached, error))
        }
    }

    /// Fake downloader that leaves a partial temporary before returning a network error.
    struct PartialFailingDownloader<'a> {
        bytes: &'a [u8],
        calls: Cell<usize>,
    }

    impl Downloader for PartialFailingDownloader<'_> {
        /// Writes a partial response and then reports the injected transport failure.
        fn download_to(&self, _source: &SourceArchive, destination: &Path) -> Result<(), NativeError> {
            self.calls.set(self.calls.get() + 1);
            fs::write(destination, self.bytes)
                .map_err(|error| NativeError::io("write partial fake download", destination, error))?;
            Err(NativeError::new(
                NativeErrorKind::Network,
                "injected download failure",
            ))
        }
    }

    /// Creates a source cache fixture path.
    fn cache_file() -> PathBuf {
        std::env::temp_dir().join(format!("elephc-source-{}-{}.tar.gz", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
    }

    /// Verifies offline misses never call transport and verified hits are reusable offline.
    #[test]
    fn offline_never_invokes_downloader() {
        let bytes = b"fixture";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let source = SourceArchive { https_url: "https://example.invalid/source", sha256: Box::leak(digest.into_boxed_str()), exact_size: bytes.len() as u64, body_limit: 1024 };
        let path = cache_file();
        let downloader = FakeDownloader { bytes, calls: Cell::new(0) };
        assert!(ensure_source(&path, &source, true, &downloader).is_err());
        assert_eq!(downloader.calls.get(), 0);
        ensure_source(&path, &source, false, &downloader).unwrap();
        assert_eq!(downloader.calls.get(), 1);
        ensure_source(&path, &source, true, &downloader).unwrap();
        assert_eq!(downloader.calls.get(), 1);
        fs::remove_file(path).unwrap();
    }

    /// Verifies an online command replaces a corrupt content-addressed cache entry.
    #[test]
    fn online_repairs_corrupt_cached_source() {
        let bytes = b"fixture";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let source = SourceArchive { https_url: "https://example.invalid/source", sha256: Box::leak(digest.into_boxed_str()), exact_size: bytes.len() as u64, body_limit: 1024 };
        let path = cache_file();
        fs::write(&path, b"corrupt").unwrap();
        let downloader = FakeDownloader { bytes, calls: Cell::new(0) };
        ensure_source(&path, &source, false, &downloader).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(downloader.calls.get(), 1);
        fs::remove_file(path).unwrap();
    }

    /// Verifies fallback success removes the losing verified download temporary.
    #[test]
    fn fallback_success_removes_download_temporary() {
        let bytes = b"fixture";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let source = SourceArchive { https_url: "https://example.invalid/source", sha256: Box::leak(digest.into_boxed_str()), exact_size: bytes.len() as u64, body_limit: 1024 };
        let directory = unique_sibling(
            &std::env::temp_dir().join("elephc-source-race"),
            "test",
        );
        fs::create_dir_all(&directory).unwrap();
        let cached = directory.join("source.tar.gz");
        assert!(!cached.exists());

        let downloader = ConcurrentPublisherDownloader {
            bytes,
            cached: &cached,
            calls: Cell::new(0),
        };
        let publish_calls = Cell::new(0);
        let published_temporary = RefCell::new(None::<PathBuf>);
        let result = ensure_source_with_publish(
            &cached,
            &source,
            false,
            &downloader,
            |temporary, destination| {
                publish_calls.set(publish_calls.get() + 1);
                let _ = published_temporary.replace(Some(temporary.to_path_buf()));
                assert_eq!(destination, cached.as_path());
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "injected publish failure",
                ))
            },
        )
        .unwrap();

        assert_eq!(downloader.calls.get(), 1);
        assert_eq!(publish_calls.get(), 1);
        assert_eq!(result, cached);
        let temporary = published_temporary.into_inner().unwrap();
        assert!(!temporary.exists());
        verify_source(&cached, &source).unwrap();
        let metadata = fs::symlink_metadata(&cached).unwrap();
        assert!(metadata.file_type().is_file());
        assert!(!metadata.file_type().is_symlink());
        assert_eq!(metadata.len(), bytes.len() as u64);

        let cached_file_name = cached.file_name().unwrap().to_string_lossy();
        let entries = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![cached.file_name().unwrap().to_os_string()]
        );
        assert!(!entries.iter().any(|entry| entry
            .to_string_lossy()
            .starts_with(&format!(".{cached_file_name}.download."))));
        assert!(!entries.iter().any(|entry| entry
            .to_string_lossy()
            .starts_with(&format!(".{cached_file_name}.quarantine."))));
        fs::remove_dir_all(directory).unwrap();
    }

    /// Verifies a partial download failure skips publication and removes its temporary.
    #[test]
    fn download_failure_skips_publish_and_removes_temporary() {
        let bytes = b"fixture";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let source = SourceArchive { https_url: "https://example.invalid/source", sha256: Box::leak(digest.into_boxed_str()), exact_size: bytes.len() as u64, body_limit: 1024 };
        let directory = unique_sibling(
            &std::env::temp_dir().join("elephc-source-download-failure"),
            "test",
        );
        fs::create_dir_all(&directory).unwrap();
        let cached = directory.join("source.tar.gz");
        assert!(!cached.exists());

        let downloader = PartialFailingDownloader {
            bytes: b"partial",
            calls: Cell::new(0),
        };
        let publish_calls = Cell::new(0);
        let error = ensure_source_with_publish(
            &cached,
            &source,
            false,
            &downloader,
            |_, _| {
                publish_calls.set(publish_calls.get() + 1);
                Ok(())
            },
        )
        .unwrap_err();

        assert_eq!(downloader.calls.get(), 1);
        assert_eq!(publish_calls.get(), 0);
        assert!(error.to_string().contains("injected download failure"));
        let cached_file_name = cached.file_name().unwrap().to_string_lossy();
        let entries = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert!(!entries.iter().any(|entry| entry
            .to_string_lossy()
            .starts_with(&format!(".{cached_file_name}.download."))));
        assert!(entries.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    /// Verifies a broken symlink at the content-addressed path is quarantined and replaced.
    #[test]
    #[cfg(unix)]
    fn online_repairs_broken_symlink_cached_source() {
        let bytes = b"fixture";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let source = SourceArchive { https_url: "https://example.invalid/source", sha256: Box::leak(digest.into_boxed_str()), exact_size: bytes.len() as u64, body_limit: 1024 };
        let path = cache_file();
        std::os::unix::fs::symlink(path.with_extension("missing"), &path).unwrap();
        assert!(!path.exists());
        let downloader = FakeDownloader { bytes, calls: Cell::new(0) };
        ensure_source(&path, &source, false, &downloader).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        assert_eq!(downloader.calls.get(), 1);
        let parent = path.parent().unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(!fs::read_dir(parent).unwrap().flatten().any(|entry| {
            let entry = entry.file_name();
            let entry = entry.to_string_lossy();
            entry.starts_with(&format!(".{name}.quarantine."))
        }));
        fs::remove_file(path).unwrap();
    }
}
