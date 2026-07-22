//! Purpose:
//! Defines durable native cache roots, ABI-qualified artifact keys, advisory locks, and publication.
//!
//! Called from:
//! - Installation, downloader source publication, doctor, and compilation resolution.
//!
//! Key details:
//! - Resolution ignores staging/quarantine siblings and publication is an exact-key atomic rename.

use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fs2::FileExt;

use super::error::{NativeError, NativeErrorKind};
use super::project::lexical_absolute;
use super::util::{sha256_bytes, unique_sibling};

/// Absolute durable cache paths for sources, artifacts, locks, and staging siblings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheLayout {
    pub root: PathBuf,
    pub sources: PathBuf,
    pub artifacts: PathBuf,
    pub locks: PathBuf,
}

/// Every immutable dimension that separates installed native artifacts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactKey<'a> {
    pub package: &'a str,
    pub version: &'a str,
    pub recipe: u32,
    pub source_sha256: &'a str,
    pub target: &'a str,
    pub abi: &'a str,
    pub toolchain_fingerprint: &'a str,
}

/// Held advisory lock; dropping it releases the OS lock while retaining the reusable lock file.
pub struct AdvisoryLock {
    file: File,
}

impl CacheLayout {
    /// Resolves the frozen cache precedence from the process environment and cwd.
    pub fn from_environment(cwd: &Path) -> Result<Self, NativeError> {
        Self::from_values(
            cwd,
            std::env::var_os("ELEPHC_NATIVE_CACHE").as_deref(),
            std::env::var_os("XDG_CACHE_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    }

    /// Resolves cache precedence from injected values for deterministic tests.
    pub fn from_values(
        cwd: &Path,
        native: Option<&std::ffi::OsStr>,
        xdg: Option<&std::ffi::OsStr>,
        home: Option<&std::ffi::OsStr>,
    ) -> Result<Self, NativeError> {
        let selected = if let Some(path) = native.filter(|path| !path.is_empty()) {
            PathBuf::from(path)
        } else if native.is_some() {
            return Err(NativeError::new(NativeErrorKind::Cache, "ELEPHC_NATIVE_CACHE must not be empty"));
        } else if let Some(path) = xdg.filter(|path| !path.is_empty()) {
            PathBuf::from(path).join("elephc/native")
        } else if let Some(path) = home.filter(|path| !path.is_empty()) {
            PathBuf::from(path).join(".cache/elephc/native")
        } else {
            return Err(NativeError::new(NativeErrorKind::Cache, "no durable native cache root; set ELEPHC_NATIVE_CACHE, XDG_CACHE_HOME, or HOME"));
        };
        let root = lexical_absolute(&selected, cwd).map_err(|error| NativeError::new(NativeErrorKind::Cache, error.to_string()))?;
        Ok(Self { sources: root.join("sources"), artifacts: root.join("artifacts"), locks: root.join("locks"), root })
    }

    /// Returns the content-addressed source archive path.
    pub fn source_path(&self, sha256: &str) -> PathBuf {
        self.sources.join(format!("{sha256}.tar.gz"))
    }

    /// Returns the exact ABI/toolchain-qualified final artifact directory.
    pub fn artifact_path(&self, key: &ArtifactKey<'_>) -> Result<PathBuf, NativeError> {
        for component in [key.package, key.version, key.source_sha256, key.target, key.abi, key.toolchain_fingerprint] {
            validate_component(component)?;
        }
        Ok(self.artifacts
            .join(key.package)
            .join(key.version)
            .join(format!("r{}", key.recipe))
            .join(key.source_sha256)
            .join(key.target)
            .join(key.abi)
            .join(key.toolchain_fingerprint))
    }

    /// Returns the project mutation lock path derived from a canonical manifest path.
    pub fn project_lock_path(&self, manifest: &Path) -> PathBuf {
        self.locks.join("project").join(format!("{}.lock", sha256_bytes(manifest.to_string_lossy().as_bytes())))
    }

    /// Returns the source publication lock path for one content digest.
    pub fn source_lock_path(&self, sha256: &str) -> PathBuf {
        self.locks.join("source").join(format!("{sha256}.lock"))
    }

    /// Returns the artifact publication lock path for one exact cache key.
    pub fn artifact_lock_path(&self, key: &ArtifactKey<'_>) -> Result<PathBuf, NativeError> {
        let path = self.artifact_path(key)?;
        let relative = path.strip_prefix(&self.artifacts).expect("artifact path rooted in artifacts");
        Ok(self.locks.join("artifact").join(format!("{}.lock", sha256_bytes(relative.to_string_lossy().as_bytes()))))
    }

    /// Acquires an advisory lock with the normative 30-second timeout.
    pub fn lock(&self, path: &Path, command_kind: &str) -> Result<AdvisoryLock, NativeError> {
        acquire_lock(path, command_kind, Duration::from_secs(30))
    }
}

impl Drop for AdvisoryLock {
    /// Releases the advisory lock while intentionally retaining the reusable lock file.
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

/// Acquires an advisory lock and writes holder metadata after ownership is established.
pub(crate) fn acquire_lock(path: &Path, command_kind: &str, timeout: Duration) -> Result<AdvisoryLock, NativeError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| NativeError::io("create native lock directory", parent, error))?;
    let mut file = OpenOptions::new().read(true).write(true).create(true).open(path)
        .map_err(|error| NativeError::io("open native advisory lock", path, error))?;
    let start = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && start.elapsed() < timeout => thread::sleep(Duration::from_millis(50)),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                let owner = fs::read_to_string(path).unwrap_or_else(|_| "owner metadata unavailable".to_string());
                return Err(NativeError::new(NativeErrorKind::Cache, format!("timed out after {}s acquiring lock; owner: {}", timeout.as_secs(), owner.trim())).with_path(path));
            }
            Err(error) => return Err(NativeError::io("acquire native advisory lock", path, error)),
        }
    }
    file.set_len(0).map_err(|error| NativeError::io("truncate native lock metadata", path, error))?;
    file.seek(SeekFrom::Start(0)).map_err(|error| NativeError::io("seek native lock metadata", path, error))?;
    let started = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    writeln!(file, "pid={}\ncommand={}\nstarted={}", std::process::id(), command_kind, started)
        .map_err(|error| NativeError::io("write native lock metadata", path, error))?;
    file.sync_all().map_err(|error| NativeError::io("flush native lock metadata", path, error))?;
    Ok(AdvisoryLock { file })
}

/// Publishes a verified staging directory, quarantining one corrupt exact-key final when necessary.
pub fn publish_artifact(staging: &Path, final_path: &Path, existing_valid: bool) -> Result<(), NativeError> {
    let final_exists = node_exists(final_path)?;
    if final_exists && existing_valid {
        fs::remove_dir_all(staging).map_err(|error| NativeError::io("discard redundant artifact staging", staging, error))?;
        return Ok(());
    }
    let quarantine = unique_sibling(final_path, "quarantine");
    let quarantined = if final_exists {
        fs::rename(final_path, &quarantine).map_err(|error| NativeError::io("quarantine corrupt native artifact", final_path, error))?;
        true
    } else {
        false
    };
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).map_err(|error| NativeError::io("create artifact publication parent", parent, error))?;
    }
    match fs::rename(staging, final_path) {
        Ok(()) => {
            if quarantined {
                remove_exact_node(&quarantine)?;
            }
            Ok(())
        }
        Err(error) => {
            if quarantined { let _ = fs::rename(&quarantine, final_path); }
            Err(NativeError::io("publish native artifact", final_path, error))
        }
    }
}

/// Detects every exact filesystem node, including broken symlinks, without following it.
fn node_exists(path: &Path) -> Result<bool, NativeError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(NativeError::io("inspect exact native artifact node", path, error)),
    }
}

/// Removes one quarantined exact node without following symlinks or assuming it is a directory.
pub(crate) fn remove_exact_node(path: &Path) -> Result<(), NativeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(NativeError::io("inspect exact native cache node", path, error)),
    };
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).map_err(|error| NativeError::io("remove exact native cache directory", path, error))
    } else {
        fs::remove_file(path).map_err(|error| NativeError::io("remove exact native cache node", path, error))
    }
}

/// Validates one cache-key component against path separators and traversal syntax.
fn validate_component(value: &str) -> Result<(), NativeError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')) || matches!(value, "." | "..") {
        return Err(NativeError::new(NativeErrorKind::Cache, format!("unsafe native cache-key component '{value}'")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Verifies explicit, XDG, and HOME cache precedence and lexical absolutization.
    #[test]
    fn cache_root_precedence_is_deterministic() {
        let cwd = Path::new("/work");
        let explicit = CacheLayout::from_values(cwd, Some(std::ffi::OsStr::new("cache")), Some(std::ffi::OsStr::new("/xdg")), Some(std::ffi::OsStr::new("/home/u"))).unwrap();
        assert_eq!(explicit.root, Path::new("/work/cache"));
        let xdg = CacheLayout::from_values(cwd, None, Some(std::ffi::OsStr::new("/xdg")), Some(std::ffi::OsStr::new("/home/u"))).unwrap();
        assert_eq!(xdg.root, Path::new("/xdg/elephc/native"));
        assert!(CacheLayout::from_values(cwd, None, None, None).is_err());
    }

    /// Verifies GNU/musl and toolchain fingerprints occupy distinct artifact paths.
    #[test]
    fn artifact_keys_separate_abi_and_toolchain() {
        let cache = CacheLayout::from_values(Path::new("/"), Some(std::ffi::OsStr::new("/cache")), None, None).unwrap();
        let mut key = ArtifactKey { package: "pcre2", version: "10.47", recipe: 1, source_sha256: "abc", target: "linux-x86_64", abi: "x86_64-linux-gnu", toolchain_fingerprint: "one" };
        let first = cache.artifact_path(&key).unwrap();
        key.abi = "x86_64-linux-musl";
        let second = cache.artifact_path(&key).unwrap();
        key.toolchain_fingerprint = "two";
        let third = cache.artifact_path(&key).unwrap();
        assert_ne!(first, second);
        assert_ne!(second, third);
    }

    /// Verifies valid finals are reused and corrupt finals are atomically replaced from staging.
    #[test]
    fn artifact_publication_reuses_or_replaces_exact_final() {
        let root = std::env::temp_dir().join(format!("elephc-publish-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let final_path = root.join("artifact");
        fs::create_dir_all(&final_path).unwrap();
        fs::write(final_path.join("value"), b"valid").unwrap();
        let redundant = root.join("staging-redundant");
        fs::create_dir_all(&redundant).unwrap();
        fs::write(redundant.join("value"), b"new").unwrap();
        publish_artifact(&redundant, &final_path, true).unwrap();
        assert_eq!(fs::read(final_path.join("value")).unwrap(), b"valid");
        assert!(!redundant.exists());
        let replacement = root.join("staging-replacement");
        fs::create_dir_all(&replacement).unwrap();
        fs::write(replacement.join("value"), b"replacement").unwrap();
        publish_artifact(&replacement, &final_path, false).unwrap();
        assert_eq!(fs::read(final_path.join("value")).unwrap(), b"replacement");
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies regular-file and broken-symlink finals are quarantined, replaced, and fully cleaned.
    #[test]
    #[cfg(unix)]
    fn artifact_publication_repairs_file_and_broken_symlink_finals() {
        let root = std::env::temp_dir().join(format!("elephc-publish-nodes-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&root).unwrap();
        let final_path = root.join("artifact");

        fs::write(&final_path, b"corrupt-file").unwrap();
        let file_staging = root.join("file-staging");
        fs::create_dir(&file_staging).unwrap();
        fs::write(file_staging.join("value"), b"from-file").unwrap();
        publish_artifact(&file_staging, &final_path, false).unwrap();
        assert_eq!(fs::read(final_path.join("value")).unwrap(), b"from-file");
        assert!(!fs::read_dir(&root).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().contains("quarantine")));

        fs::remove_dir_all(&final_path).unwrap();
        std::os::unix::fs::symlink(root.join("missing-target"), &final_path).unwrap();
        assert!(!final_path.exists());
        assert!(fs::symlink_metadata(&final_path).unwrap().file_type().is_symlink());
        let symlink_staging = root.join("symlink-staging");
        fs::create_dir(&symlink_staging).unwrap();
        fs::write(symlink_staging.join("value"), b"from-symlink").unwrap();
        publish_artifact(&symlink_staging, &final_path, false).unwrap();
        assert_eq!(fs::read(final_path.join("value")).unwrap(), b"from-symlink");
        assert!(!fs::symlink_metadata(&final_path).unwrap().file_type().is_symlink());
        assert!(!fs::read_dir(&root).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().contains("quarantine")));
        fs::remove_dir_all(root).unwrap();
    }
}
