//! Purpose:
//! Builds and atomically publishes cached REPL hosts with verified dependency receipts.
//!
//! Called from:
//! - `super::run_inner` before replacing the launcher with a native session.
//!
//! Key details:
//! - A per-configuration file lock protects validation, building, and publication.
//! - Receipts cover executable bytes and exact linked archives, including Magician.

mod identity;
#[cfg(test)]
mod tests;
pub(super) use identity::identity;

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, IsTerminal, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use sha2::{Digest, Sha256};

/// One immutable input, with its bridge identity retained for source freshness checks.
#[derive(Serialize, Deserialize)]
struct Dependency {
    path: PathBuf,
    bridge: Option<String>,
    digest: String,
    stamp: FileStamp,
}

/// Detects replacement and in-place changes before deciding whether a large archive needs hashing.
#[derive(Serialize, Deserialize, PartialEq)]
struct FileStamp {
    size: u64,
    inode: u64,
    device: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl FileStamp {
    /// Reads nanosecond timestamps, identity, and length from one regular file.
    fn read(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        if !metadata.is_file() { return Err(format!("'{}' is not a file", path.display())); }
        Ok(Self { size: metadata.len(), inode: metadata.ino(), device: metadata.dev(),
            modified: (metadata.mtime(), metadata.mtime_nsec()), changed: (metadata.ctime(), metadata.ctime_nsec()) })
    }
}

/// Cache metadata published only after the executable and all dependencies are complete.
#[derive(Serialize, Deserialize)]
struct Receipt {
    identity: String,
    executable: String,
    dependencies: Vec<Dependency>,
}

/// Removes only the staging directory created under the held configuration lock.
struct Stage(PathBuf);

impl Drop for Stage {
    /// Cleans partial assembler/linker products on both ordinary success and errors.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Reuses an intact host or builds it through this exact compiler binary.
pub(super) fn prepare(root: &Path, key: &str, compiler: &Path, args: &[String], quiet: bool) -> Result<PathBuf, String> {
    private_directory(root)?;
    let lock_path = root.join(format!("{key}.lock"));
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .mode(0o600).custom_flags(libc::O_NOFOLLOW).open(&lock_path)
        .map_err(|error| format!("cannot open cache lock: {error}"))?;
    lock.lock_exclusive().map_err(|error| format!("cannot lock cache: {error}"))?;
    let directory = root.join(key);
    private_directory(&directory)?;
    let host = directory.join("host");
    let receipt_path = directory.join("receipt.json");
    if valid(&host, &receipt_path, key) { return Ok(host); }

    if !quiet && io::stderr().is_terminal() {
        eprintln!("Preparing the Elephc REPL (cached for subsequent sessions)...");
    }
    // This fixed staging name is exclusively owned by the same held lock. An interrupted
    // previous build can be removed without racing another configuration or a live builder.
    let staging = directory.join("build");
    if staging.try_exists().map_err(|error| error.to_string())? {
        private_directory(&staging)?;
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    private_directory(&staging)?;
    let staging = Stage(staging);
    let source_path = staging.0.join("host.php");
    let source_arg = source_path.to_str().ok_or("cache path is not UTF-8")?;
    let output = Command::new(compiler).arg("repl")
        .arg(format!("--internal-build-host={source_arg}"))
        .args(args).stdin(Stdio::null()).output()
        .map_err(|error| format!("cannot build session host: {error}"))?;
    if !output.status.success() {
        return Err(format!("session host build failed ({})\n{}{}", output.status,
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)));
    }
    let inputs: Vec<(PathBuf, Option<String>)> = serde_json::from_slice(
        &fs::read(staging.0.join("host.inputs.json")).map_err(|error| error.to_string())?
    ).map_err(|error| format!("invalid host dependency list: {error}"))?;
    let dependencies = inputs.into_iter().map(|(path, bridge)| {
        Ok(Dependency { digest: digest_file(&path)?, stamp: FileStamp::read(&path)?, path, bridge })
    }).collect::<Result<Vec<_>, String>>()?;
    let built_host = staging.0.join("host");
    fs::set_permissions(&built_host, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
    let receipt = Receipt { identity: key.into(), executable: digest_file(&built_host)?, dependencies };
    let built_receipt = staging.0.join("receipt.json");
    fs::write(&built_receipt, serde_json::to_vec(&receipt).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    // Rename keeps executing instances intact. A missing/stale receipt can only cause a miss.
    fs::rename(&built_host, &host).map_err(|error| format!("cannot publish host: {error}"))?;
    fs::rename(&built_receipt, &receipt_path).map_err(|error| format!("cannot publish receipt: {error}"))?;
    Ok(host)
}

/// Checks bytes, ownership, execute permissions, and development-source freshness before reuse.
fn valid(host: &Path, receipt: &Path, key: &str) -> bool {
    let Ok(metadata) = fs::symlink_metadata(host) else { return false; };
    if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0 || metadata.mode() & 0o100 == 0 {
        return false;
    }
    let Ok(bytes) = fs::read(receipt) else { return false; };
    let Ok(receipt) = serde_json::from_slice::<Receipt>(&bytes) else { return false; };
    receipt.identity == key && digest_file(host).is_ok_and(|hash| hash == receipt.executable)
        && !receipt.dependencies.is_empty()
        && receipt.dependencies.iter().all(|dependency| {
            dependency.bridge.as_ref().is_none_or(|bridge| {
                crate::linker::cached_bridge_is_current(bridge, &dependency.path)
            }) && FileStamp::read(&dependency.path).is_ok_and(|stamp| {
                stamp == dependency.stamp || digest_file(&dependency.path).is_ok_and(|hash| hash == dependency.digest)
            })
        })
}

/// Creates owner-only cache directories and refuses symlink or foreign-owned leaves.
fn private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| format!("cannot create '{}': {error}", path.display()))?;
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(format!("cache directory '{}' is not an owned directory", path.display()));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())
}

/// Streams a file into SHA-256 so large bridge archives do not require matching heap buffers.
fn digest_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let size = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if size == 0 { break; }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
