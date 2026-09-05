//! Purpose:
//! Defines runtime-cache identities, canonical object names, directory hardening, and
//! integrity-sidecar validation.
//!
//! Called from:
//! - `super::prepare_runtime_object()` while resolving or publishing a cache entry.
//! - `super::tests` for cache-key and integrity regressions.
//!
//! Key details:
//! - Every runtime-emission feature, PHP profile, relocation mode, and library-boundary mode
//!   participates in the source-identity hash.
//! - Published objects are accepted only when their FNV-1a checksum matches the sidecar.

use std::fs;
use std::path::Path;

use crate::codegen::platform::Target;
use crate::codegen::RuntimeFeatures;

/// Builds the cache file name for a runtime object.
pub(super) fn runtime_cache_file_name(heap_size: usize, target: Target, runtime_hash: u64) -> String {
    format!(
        "runtime-v{}-{}-rt{:016x}-heap{}.o",
        env!("CARGO_PKG_VERSION"),
        target.as_str(),
        runtime_hash,
        heap_size
    )
}
/// Combines runtime emission inputs with the compile-time runtime-emitter identity.
///
/// Keeping this separate makes cache-key invariants testable without assembling
/// an object. Production passes Cargo's source-derived build identity, so any
/// runtime emitter change invalidates the entry even if package versions match.
#[cfg(test)]
pub(super) fn runtime_cache_key_with_build_identity(
    heap_size: usize,
    target: Target,
    php_version_id: u32,
    features: RuntimeFeatures,
    pic: bool,
    build_identity: &[u8],
) -> u64 {
    runtime_cache_key_with_build_identity_and_boundary(
        heap_size,
        target,
        php_version_id,
        features,
        pic,
        pic,
        build_identity,
    )
}

/// Extends the runtime cache identity with the recoverable-library boundary mode.
pub(super) fn runtime_cache_key_with_build_identity_and_boundary(
    heap_size: usize,
    target: Target,
    php_version_id: u32,
    features: RuntimeFeatures,
    pic: bool,
    library_boundary: bool,
    build_identity: &[u8],
) -> u64 {
    // The feature bits come from `RuntimeFeatures` itself rather than being re-packed here: it
    // owns the layout, and a feature that gates emission but is missing from this key would name
    // two different runtime objects with one key. Relocation and boundary modes
    // ride in the high bits so appending a feature never shifts them.
    let feature_bits = features.cache_key_bits()
        | ((library_boundary as u64) << 62)
        | ((pic as u64) << 63);
    let mut identity = format!(
        "{}:{php_version_id}:{heap_size}:{feature_bits}:",
        target.as_str()
    )
    .into_bytes();
    identity.extend_from_slice(build_identity);
    runtime_bytes_hash(&identity)
}

/// Ensures the shared cache is not writable or readable by another local user.
#[cfg(unix)]
/// Restricts a cache directory to the invoking user before publishing objects.
pub(super) fn harden_runtime_cache_dir(cache_dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let metadata = fs::metadata(cache_dir).map_err(|err| {
        format!("failed to stat runtime cache '{}': {}", cache_dir.display(), err)
    })?;
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(format!("runtime cache '{}' is not owned by this user", cache_dir.display()));
    }
    if metadata.mode() & 0o077 != 0 {
        fs::set_permissions(cache_dir, fs::Permissions::from_mode(0o700)).map_err(|err| {
            format!("failed to restrict runtime cache '{}': {}", cache_dir.display(), err)
        })?;
    }
    Ok(())
}

/// Platforms without Unix ownership modes rely on per-user cache roots.
#[cfg(not(unix))]
/// Keeps the cache setup portable where Unix ownership modes are unavailable.
pub(super) fn harden_runtime_cache_dir(_cache_dir: &Path) -> Result<(), String> {
    Ok(())
}

/// Returns whether a cache object matches its atomically stored integrity sidecar.
pub(super) fn runtime_object_is_intact(cache_path: &Path, integrity_path: &Path) -> bool {
    let Ok(bytes) = fs::read(cache_path) else {
        return false;
    };
    let Ok(expected) = fs::read_to_string(integrity_path) else {
        return false;
    };
    expected.trim() == format!("{:016x}", runtime_bytes_hash(&bytes))
}

/// Writes the checksum sidecar only after the object has been published.
pub(super) fn write_runtime_object_integrity(
    cache_path: &Path,
    integrity_path: &Path,
) -> Result<(), String> {
    let bytes = fs::read(cache_path).map_err(|err| {
        format!("failed to read runtime cache '{}' for integrity: {}", cache_path.display(), err)
    })?;
    let temporary = integrity_path.with_extension(format!(
        "integrity.{}.tmp",
        std::process::id()
    ));
    fs::write(&temporary, format!("{:016x}\n", runtime_bytes_hash(&bytes))).map_err(|err| {
        format!("failed to write runtime cache integrity '{}': {}", temporary.display(), err)
    })?;
    fs::rename(&temporary, integrity_path).map_err(|err| {
        let _ = fs::remove_file(&temporary);
        format!("failed to publish runtime cache integrity '{}': {}", integrity_path.display(), err)
    })
}

/// Computes a 64-bit FNV-1a hash of arbitrary cache bytes.
fn runtime_bytes_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
