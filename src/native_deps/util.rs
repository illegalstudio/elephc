//! Purpose:
//! Provides hashing, path, and durable atomic-write primitives shared by native dependency modules.
//!
//! Called from:
//! - Cache, downloader, receipts, toolchain fingerprints, and project publication.
//!
//! Key details:
//! - Publications use sibling temporary files and flush data before same-filesystem rename.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::error::NativeError;

/// Returns the lowercase SHA-256 digest of bytes.
pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Streams a file to compute its byte length and lowercase SHA-256 digest.
pub(crate) fn hash_file(path: &Path) -> Result<(u64, String), NativeError> {
    let mut file = File::open(path).map_err(|error| NativeError::io("open file for hashing", path, error))?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| NativeError::io("hash file", path, error))?;
        if read == 0 { break; }
        total = total.saturating_add(read as u64);
        digest.update(&buffer[..read]);
    }
    Ok((total, format!("{:x}", digest.finalize())))
}

/// Writes a sibling temporary file, flushes it, and atomically replaces the destination.
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), NativeError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| NativeError::io("create publication directory", parent, error))?;
    let temporary = unique_sibling(path, "tmp");
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)
        .map_err(|error| NativeError::io("create publication temporary file", &temporary, error))?;
    let result = (|| {
        file.write_all(contents).map_err(|error| NativeError::io("write publication temporary file", &temporary, error))?;
        file.sync_all().map_err(|error| NativeError::io("flush publication temporary file", &temporary, error))?;
        fs::rename(&temporary, path).map_err(|error| NativeError::io("publish file atomically", path, error))?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Constructs a collision-resistant sibling path without inspecting the destination contents.
pub(crate) fn unique_sibling(path: &Path, marker: &str) -> PathBuf {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("native");
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    path.with_file_name(format!(".{name}.{marker}.{}.{}", std::process::id(), nonce))
}
