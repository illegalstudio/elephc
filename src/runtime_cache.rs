//! Purpose:
//! Builds and caches the reusable runtime object that is linked beside generated user code.
//! Keys cache entries by compiler version, target, heap size, and runtime feature shape — by the
//! inputs that produce the assembly, not by a hash of the assembly itself, so a cache hit never
//! builds it.
//!
//! Called from:
//! - `crate::pipeline::compile()` before user assembly is linked into the final binary.
//!
//! Key details:
//! - Temporary assembly/object files are renamed into place to tolerate concurrent compiler runs.
//! - Prepared objects use short-lived hardlink leases so pruning cannot invalidate a concurrent link.
//! - Best-effort pruning retains at most eight published object/sidecar pairs outside live leases
//!   and removes only compiler-shaped temporary files whose owner process is known to be dead.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::codegen;
use crate::codegen::platform::Target;
use crate::codegen::{Emit, RuntimeFeatures};

mod identity;
mod storage;
#[cfg(test)]
mod tests;

use identity::{
    harden_runtime_cache_dir, runtime_cache_file_name, runtime_object_is_intact,
    write_runtime_object_integrity,
};
#[cfg(test)]
use identity::runtime_cache_key_with_build_identity;
use storage::{lease_runtime_object, prune_runtime_cache_objects, RuntimeObjectLease};

/// Runtime cache hit/miss status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeCacheStatus {
    Hit,
    Miss,
}

impl RuntimeCacheStatus {
    /// Returns a static string slice describing the cache status.
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeCacheStatus::Hit => "hit",
            RuntimeCacheStatus::Miss => "miss",
        }
    }
}

/// Prepared runtime object with cache status.
#[derive(Debug)]
pub struct PreparedRuntimeObject {
    /// Path to the leased linker-visible snapshot of the cached runtime object.
    pub path: PathBuf,
    /// Whether the object was found in the cache (hit) or built now (miss).
    pub status: RuntimeCacheStatus,
    /// Keeps the linker-visible hardlink snapshot alive until this prepared object is dropped.
    _lease: RuntimeObjectLease,
}

/// Builds (or retrieves from cache) the runtime object file for the given heap size, target, and features.
/// On cache miss, generates runtime assembly, assembles it to an object file, and caches the result.
/// The cache key includes compiler version, target, PHP profile, heap size, relocation and
/// library-boundary modes, and the typed runtime feature shape. A sidecar checksum validates
/// cache bytes before a hit is accepted.
/// `pic` selects position-independent emission for `--emit cdylib` artifacts so the runtime object can be
/// linked into a shared library without text-segment relocations. The returned path remains valid until
/// the `PreparedRuntimeObject` is dropped, even if another compiler prunes the canonical cache entry.
#[allow(dead_code)]
pub fn prepare_runtime_object(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
    pic: bool,
) -> Result<PreparedRuntimeObject, String> {
    prepare_runtime_object_with_mode(heap_size, target, features, pic, pic)
}

/// Builds or reuses the runtime object required by one output artifact kind.
///
/// A static library uses direct relocations (`pic = false`) but still needs the
/// recoverable host boundary enabled in runtime fatal paths.
pub fn prepare_runtime_object_for_emit(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
    emit: Emit,
) -> Result<PreparedRuntimeObject, String> {
    match emit {
        Emit::Executable => {
            prepare_runtime_object_with_mode(heap_size, target, features, false, false)
        }
        Emit::Cdylib => prepare_runtime_object_with_mode(heap_size, target, features, true, true),
        Emit::Staticlib => {
            prepare_runtime_object_with_mode(heap_size, target, features, false, true)
        }
    }
}

/// Implements runtime preparation for independent relocation and library-boundary modes.
fn prepare_runtime_object_with_mode(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
    pic: bool,
    library_boundary: bool,
) -> Result<PreparedRuntimeObject, String> {
    let cache_dir = runtime_cache_dir();
    fs::create_dir_all(&cache_dir)
        .map_err(|err| format!("failed to create runtime cache '{}': {}", cache_dir.display(), err))?;
    harden_runtime_cache_dir(&cache_dir)?;

    // Worktrees commonly share a package version and cache directory while their
    // runtime emitters differ. Cargo supplies a source-derived emitter identity
    // at build time, letting a warm cache hit avoid regenerating the full runtime
    // assembly while still rejecting an object from a different source revision.
    let cache_key = identity::runtime_cache_key_with_build_identity_and_boundary(
        heap_size,
        target,
        crate::codegen::compile_php_version().version_id(),
        features,
        pic,
        library_boundary,
        env!("ELEPHC_RUNTIME_BUILD_ID").as_bytes(),
    );
    let cache_path = cache_dir.join(runtime_cache_file_name(heap_size, target, cache_key));
    let integrity_path = cache_dir.join(format!(
        "{}.integrity",
        cache_path.file_name().and_then(|name| name.to_str()).unwrap_or("runtime.o")
    ));
    if cache_path.exists() && runtime_object_is_intact(&cache_path, &integrity_path) {
        if let Some(prepared) = lease_runtime_object(
            &cache_path,
            &integrity_path,
            RuntimeCacheStatus::Hit,
        )? {
            prune_runtime_cache_objects(&cache_dir, &cache_path);
            return Ok(prepared);
        }
    }

    let runtime_asm = codegen::generate_runtime_with_features_mode(
        heap_size,
        target,
        features,
        pic,
        library_boundary,
    );

    let unique = format!(
        "{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let stem = cache_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("runtime");
    let temp_asm_path = cache_dir.join(format!("{stem}.{unique}.s"));
    let temp_obj_path = cache_dir.join(format!("{stem}.{unique}.o"));
    fs::write(&temp_asm_path, runtime_asm).map_err(|err| {
        format!(
            "failed to write temporary runtime assembly '{}': {}",
            temp_asm_path.display(),
            err
        )
    })?;

    // Shared with the user object's assembly: both must carry the same Mach-O
    // platform, or ld rejects whichever one disagrees.
    let mut assembler = crate::linker::assembler_command(target);
    assembler.arg("-o").arg(&temp_obj_path).arg(&temp_asm_path);
    let assembler_status = assembler.status().map_err(|err| {
        format!(
            "failed to run runtime assembler '{}' for '{}': {}",
            target.assembler_cmd(),
            temp_obj_path.display(),
            err
        )
    })?;
    let _ = fs::remove_file(&temp_asm_path);
    if !assembler_status.success() {
        let _ = fs::remove_file(&temp_obj_path);
        return Err(format!(
            "runtime assembler failed while building '{}'",
            cache_path.display()
        ));
    }

    let status = match fs::rename(&temp_obj_path, &cache_path) {
        Ok(()) => {
            write_runtime_object_integrity(&cache_path, &integrity_path)?;
            RuntimeCacheStatus::Miss
        }
        Err(_err) if cache_path.exists() && runtime_object_is_intact(&cache_path, &integrity_path) => {
            let _ = fs::remove_file(&temp_obj_path);
            RuntimeCacheStatus::Hit
        }
        Err(err) => {
            let _ = fs::remove_file(&temp_obj_path);
            return Err(format!(
                "failed to store runtime cache '{}': {}",
                cache_path.display(),
                err
            ));
        }
    };
    let Some(prepared) = lease_runtime_object(&cache_path, &integrity_path, status)? else {
        return prepare_runtime_object_with_mode(
            heap_size,
            target,
            features,
            pic,
            library_boundary,
        );
    };
    prune_runtime_cache_objects(&cache_dir, &cache_path);
    Ok(prepared)
}

/// Returns the platform-specific cache directory path for runtime objects.
fn runtime_cache_dir() -> PathBuf {
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(path).join("elephc")
    } else if let Some(home) = env::var_os("HOME") {
        PathBuf::from(home).join(".cache").join("elephc")
    } else {
        env::temp_dir().join("elephc-cache")
    }
}
