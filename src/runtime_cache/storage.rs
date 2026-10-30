//! Purpose:
//! Owns runtime-cache hardlink leases, request aliases, bounded pruning, and crash-litter
//! cleanup.
//!
//! Called from:
//! - `super::prepare_runtime_object()` after validating or publishing a canonical entry.
//! - `super::tests` for cross-process lease and stale-file regressions.
//!
//! Key details:
//! - Linker-visible snapshots are hardlinks protected by shared advisory-lock markers.
//! - Cleanup is best-effort and removes only canonical compiler-owned names whose holder is
//!   absent or no longer owns the marker lock.
//! - An alias names one content-addressed object by file name only; a dangling or malformed
//!   alias is ignored by lookups and removed by pruning.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;

use super::identity::runtime_object_is_intact;
use super::{PreparedRuntimeObject, RuntimeCacheStatus};

/// Maximum number of complete runtime object entries retained in the shared cache.
const MAX_RUNTIME_CACHE_OBJECTS: usize = 8;
/// Maximum number of request aliases retained; each new build identity adds one per request.
const MAX_RUNTIME_CACHE_ALIASES: usize = 64;
/// Monotonic process-local discriminator for lease names created in one clock tick.
static RUNTIME_CACHE_LEASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One linker-visible hardlink snapshot and its cross-process liveness marker.
#[derive(Debug)]
pub(super) struct RuntimeObjectLease {
    object_path: PathBuf,
    integrity_path: PathBuf,
    lock_path: PathBuf,
    lock_file: File,
}

impl Drop for RuntimeObjectLease {
    /// Removes this process's linker snapshot and releases its cross-process liveness marker.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.object_path);
        let _ = fs::remove_file(&self.integrity_path);
        let _ = FileExt::unlock(&self.lock_file);
        let _ = fs::remove_file(&self.lock_path);
    }
}
/// Creates a process-scoped hardlink snapshot for one intact cache entry.
///
/// The returned path, rather than the canonical cache name, is passed to the
/// linker. Canonical pruning or replacement therefore cannot invalidate a
/// prepared object that another compiler still owns. The marker lock closes the
/// crash-cleanup race: a live holder keeps a shared lock, while pruning removes
/// only markers on which it can acquire an exclusive lock without waiting.
pub(super) fn lease_runtime_object(
    cache_path: &Path,
    integrity_path: &Path,
    status: RuntimeCacheStatus,
) -> Result<Option<PreparedRuntimeObject>, String> {
    for _ in 0..4 {
        let sequence = RUNTIME_CACHE_LEASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let file_name = cache_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("runtime.o");
        let lease_name = format!(
            "{file_name}.lease-{}-{timestamp}-{sequence}",
            std::process::id()
        );
        let cache_dir = cache_path.parent().unwrap_or_else(|| Path::new("."));
        let object_path = cache_dir.join(&lease_name);
        let lease_integrity_path = cache_dir.join(format!("{lease_name}.integrity"));
        let lock_path = cache_dir.join(format!("{lease_name}.lock"));
        let lock_file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(format!(
                    "failed to create runtime cache lease '{}': {}",
                    lock_path.display(),
                    err
                ));
            }
        };
        FileExt::lock_shared(&lock_file).map_err(|err| {
            let _ = fs::remove_file(&lock_path);
            format!(
                "failed to lock runtime cache lease '{}': {}",
                lock_path.display(),
                err
            )
        })?;
        let lease = RuntimeObjectLease {
            object_path: object_path.clone(),
            integrity_path: lease_integrity_path,
            lock_path,
            lock_file,
        };

        // A stale-lease collector can win between marker creation and locking.
        // In that case the locked descriptor refers to an unlinked inode, so retry
        // under a fresh unique marker rather than publishing an invisible lease.
        if !lease.lock_path.exists() {
            continue;
        }
        if let Err(err) = fs::hard_link(cache_path, &lease.object_path) {
            if err.kind() == io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(format!(
                "failed to lease runtime cache object '{}': {}",
                cache_path.display(),
                err
            ));
        }
        if let Err(err) = fs::hard_link(integrity_path, &lease.integrity_path) {
            if err.kind() == io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(format!(
                "failed to lease runtime cache integrity '{}': {}",
                integrity_path.display(),
                err
            ));
        }
        if !runtime_object_is_intact(&lease.object_path, &lease.integrity_path) {
            return Ok(None);
        }
        return Ok(Some(PreparedRuntimeObject {
            path: object_path,
            status,
            _lease: lease,
        }));
    }
    Err(format!(
        "failed to allocate a unique runtime cache lease for '{}'",
        cache_path.display()
    ))
}

/// Resolves a request alias to the content-named object it records, if it names one.
///
/// The alias holds a bare canonical object file name, joined to the alias's own directory, so a
/// tampered alias can neither leave the cache directory nor name a temporary or lease.
pub(super) fn read_runtime_cache_alias(alias_path: &Path) -> Option<PathBuf> {
    let recorded = fs::read_to_string(alias_path).ok()?;
    let name = recorded.trim();
    if name.contains('/') || name.contains('\\') || !runtime_cache_object_name_is_canonical(Path::new(name)) {
        return None;
    }
    Some(alias_path.parent()?.join(name))
}

/// Atomically records which content-named object serves a request alias.
pub(super) fn write_runtime_cache_alias(alias_path: &Path, object_path: &Path) -> io::Result<()> {
    let Some(object_name) = object_path.file_name().and_then(|name| name.to_str()) else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "runtime object has no file name"));
    };
    let alias_name = alias_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("runtime.alias");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = alias_path.with_file_name(format!(
        "{alias_name}.{}_{nonce}.tmp",
        std::process::id()
    ));
    fs::write(&temporary, format!("{object_name}\n"))?;
    fs::rename(&temporary, alias_path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

/// Removes the oldest superseded runtime object/sidecar pairs beyond the cache bound.
///
/// Housekeeping is best-effort so an unreadable stale entry cannot fail compilation.
/// The active object is always excluded, temporary compiler outputs do not match the
/// canonical filename shape, and unrelated cache-directory files are left untouched.
pub(super) fn prune_runtime_cache_objects(cache_dir: &Path, active_object: &Path) {
    cleanup_stale_runtime_cache_leases(cache_dir);
    cleanup_stale_runtime_cache_temporaries(cache_dir);
    prune_objects_beyond_bound(cache_dir, active_object);
    prune_runtime_cache_aliases(cache_dir);
}

/// Removes the oldest canonical objects beyond `MAX_RUNTIME_CACHE_OBJECTS`, never the active one.
fn prune_objects_beyond_bound(cache_dir: &Path, active_object: &Path) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    let mut objects: Vec<(PathBuf, SystemTime)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !runtime_cache_object_name_is_canonical(&path) {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((path, metadata.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect();
    if objects.len() <= MAX_RUNTIME_CACHE_OBJECTS {
        return;
    }
    objects.sort_by_key(|(_, modified)| *modified);
    let remove_count = objects.len() - MAX_RUNTIME_CACHE_OBJECTS;
    for (object, _) in objects
        .into_iter()
        .filter(|(object, _)| object.as_path() != active_object)
        .take(remove_count)
    {
        if fs::remove_file(&object).is_err() {
            continue;
        }
        let Some(file_name) = object.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let _ = fs::remove_file(cache_dir.join(format!("{file_name}.integrity")));
    }
}

/// Removes aliases whose object is gone or that are malformed, then the oldest aliases beyond
/// `MAX_RUNTIME_CACHE_ALIASES`. A removed alias only costs its next request a regeneration.
fn prune_runtime_cache_aliases(cache_dir: &Path) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    let mut live: Vec<(PathBuf, SystemTime)> = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !runtime_cache_alias_name_is_canonical(&path) {
            continue;
        }
        if !read_runtime_cache_alias(&path).is_some_and(|object| object.exists()) {
            let _ = fs::remove_file(&path);
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        live.push((path, modified));
    }
    if live.len() <= MAX_RUNTIME_CACHE_ALIASES {
        return;
    }
    live.sort_by_key(|(_, modified)| *modified);
    let remove_count = live.len() - MAX_RUNTIME_CACHE_ALIASES;
    for (alias, _) in live.into_iter().take(remove_count) {
        let _ = fs::remove_file(alias);
    }
}

/// Removes compiler-owned runtime-cache temporaries after their owner process has exited.
fn cleanup_stale_runtime_cache_temporaries(cache_dir: &Path) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(owner_pid) = runtime_cache_temporary_owner_pid(&path) else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() || runtime_cache_process_is_alive(owner_pid) {
            continue;
        }
        let _ = fs::remove_file(path);
    }
}

/// Parses the owner PID from one of this module's assembler or integrity temp names.
fn runtime_cache_temporary_owner_pid(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    if let Some(without_tmp) = name.strip_suffix(".tmp") {
        if let Some((alias_name, owner_identity)) = without_tmp.rsplit_once(".alias.") {
            let (owner, nonce) = owner_identity.split_once('_')?;
            if nonce.is_empty()
                || !nonce.bytes().all(|byte| byte.is_ascii_digit())
                || !runtime_cache_alias_name_is_canonical(Path::new(&format!("{alias_name}.alias")))
            {
                return None;
            }
            return owner.parse::<u32>().ok();
        }
        let (object_name, owner) = without_tmp.rsplit_once(".integrity.")?;
        let canonical_name = if object_name.ends_with(".o") {
            object_name.to_string()
        } else {
            format!("{object_name}.o")
        };
        if !runtime_cache_object_name_is_canonical(Path::new(&canonical_name)) {
            return None;
        }
        return owner.parse::<u32>().ok();
    }

    let (temporary_stem, extension) = name.rsplit_once('.')?;
    if extension != "s" && extension != "o" {
        return None;
    }
    let (object_stem, owner_identity) = temporary_stem.rsplit_once('.')?;
    let (owner, nonce) = owner_identity.split_once('_')?;
    if nonce.is_empty() || !nonce.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let canonical_name = format!("{object_stem}.o");
    if !runtime_cache_object_name_is_canonical(Path::new(&canonical_name)) {
        return None;
    }
    owner.parse::<u32>().ok()
}

/// Returns whether a positive Unix PID still identifies a running process.
#[cfg(unix)]
fn runtime_cache_process_is_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Conservatively preserves temporaries where process liveness cannot be queried safely.
#[cfg(not(unix))]
fn runtime_cache_process_is_alive(_pid: u32) -> bool {
    true
}

/// Removes abandoned hardlink snapshots whose owner no longer holds their marker lock.
fn cleanup_stale_runtime_cache_leases(cache_dir: &Path) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    for lock_path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        let Some(holder_pid) = runtime_cache_lease_holder_pid(&lock_path) else {
            continue;
        };
        // Some flock implementations coalesce locks by process rather than open
        // file description. Never let this process's cleanup probe upgrade its
        // own shared lease and remove the snapshot it is about to return.
        if holder_pid == std::process::id() {
            continue;
        }
        let Ok(lock_file) = OpenOptions::new().read(true).write(true).open(&lock_path) else {
            continue;
        };
        match FileExt::try_lock_exclusive(&lock_file) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => continue,
            Err(_) => continue,
        }
        let Some(lock_name) = lock_path.file_name().and_then(|name| name.to_str()) else {
            let _ = FileExt::unlock(&lock_file);
            continue;
        };
        let Some(lease_name) = lock_name.strip_suffix(".lock") else {
            let _ = FileExt::unlock(&lock_file);
            continue;
        };
        let lease_object = cache_dir.join(lease_name);
        let _ = fs::remove_file(&lease_object);
        let _ = fs::remove_file(cache_dir.join(format!("{lease_name}.integrity")));
        let _ = FileExt::unlock(&lock_file);
        let _ = fs::remove_file(&lock_path);
    }
}

/// Parses the holder PID from one of this module's canonical lease lock markers.
fn runtime_cache_lease_holder_pid(path: &Path) -> Option<u32> {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return None;
    };
    let Some(lease_name) = name.strip_suffix(".lock") else {
        return None;
    };
    let Some((object_name, lease_identity)) = lease_name.rsplit_once(".lease-") else {
        return None;
    };
    let mut fields = lease_identity.split('-');
    let holder_pid = fields.next()?.parse::<u32>().ok()?;
    let timestamp = fields.next()?;
    let sequence = fields.next()?;
    if fields.next().is_some()
        || timestamp.is_empty()
        || !timestamp.bytes().all(|byte| byte.is_ascii_digit())
        || sequence.is_empty()
        || !sequence.bytes().all(|byte| byte.is_ascii_digit())
        || !runtime_cache_object_name_is_canonical(Path::new(object_name))
    {
        return None;
    }
    Some(holder_pid)
}

/// Returns whether a path names a published runtime-cache object rather than a temp file.
fn runtime_cache_object_name_is_canonical(path: &Path) -> bool {
    runtime_cache_name_has_shape(path, ".o")
}

/// Returns whether a path names a request alias written by `write_runtime_cache_alias`.
fn runtime_cache_alias_name_is_canonical(path: &Path) -> bool {
    runtime_cache_name_has_shape(path, ".alias")
}

/// Returns whether a file name is `runtime-v<version>-<target>-rt<16 hex>-heap<digits><extension>`.
fn runtime_cache_name_has_shape(path: &Path, extension: &str) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some((identity, heap_suffix)) = name.rsplit_once("-heap") else {
        return false;
    };
    let Some(heap_size) = heap_suffix.strip_suffix(extension) else {
        return false;
    };
    let Some((prefix, runtime_hash)) = identity.rsplit_once("-rt") else {
        return false;
    };
    prefix.starts_with("runtime-v")
        && runtime_hash.len() == 16
        && runtime_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        && !heap_size.is_empty()
        && heap_size.bytes().all(|byte| byte.is_ascii_digit())
}
