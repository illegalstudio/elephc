//! Purpose:
//! Owns private staging directories for PHP SDK probes and adapter compilation.
//!
//! Called from:
//! - `super::sdk` before starting compiler subprocesses.
//!
//! Key details:
//! - Creation is exclusive and cleanup is scoped to the exact directory this owner created.
//! - Staging lives below the caller's build directory, not a shared global scratch path.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub(super) struct Scratch(PathBuf);

impl Scratch {
    /// Creates a private staging child without following or replacing an existing path.
    pub(super) fn create(parent: &Path) -> Result<Self, String> {
        for _ in 0..100 {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".elephc-phpext-{}-{id}", std::process::id()));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("create PHP extension staging directory: {error}")),
            }
        }
        Err("could not reserve a PHP extension staging directory".to_string())
    }

    /// Returns a child path within this owner's private directory.
    pub(super) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    /// Removes this owner's staging directory after subprocesses have finished, including failures.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
