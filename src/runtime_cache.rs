//! Purpose:
//! Builds and caches the reusable runtime object that is linked beside generated user code.
//! A request is keyed by compiler version, target, heap size, runtime feature shape and the
//! source-derived build identity — the inputs that produce the assembly — so a warm hit never
//! builds it. Objects themselves are named by the hash of their assembly, and an alias file maps
//! each request key to one: a new build identity whose runtime assembly did not change finds the
//! object already assembled and repays only the generation, not the assembler run.
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
    harden_runtime_cache_dir, runtime_cache_alias_name, runtime_cache_file_name,
    runtime_content_key, runtime_object_is_intact, write_runtime_object_integrity,
};
#[cfg(test)]
use identity::runtime_cache_key_with_build_identity;
use storage::{
    lease_runtime_object, prune_runtime_cache_objects, read_runtime_cache_alias,
    write_runtime_cache_alias, RuntimeObjectLease,
};

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
        Emit::Phpext => Err("PHP extensions build an isolated combined runtime object".to_string()),
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
    // Worktrees commonly share a package version and cache directory while their
    // runtime emitters differ. Cargo supplies a source-derived emitter identity
    // at build time, letting a warm cache hit avoid regenerating the full runtime
    // assembly while still rejecting an object from a different source revision.
    prepare_runtime_object_with_identity(
        heap_size,
        target,
        features,
        pic,
        library_boundary,
        env!("ELEPHC_RUNTIME_BUILD_ID").as_bytes(),
    )
}

/// Prepares the runtime object for one request under an explicit build identity.
///
/// The request key resolves through its alias to a content-named object. When the alias is
/// missing or stale, the runtime assembly is generated and hashed; an intact object for that
/// assembly is reused without running the assembler, and only a new assembly is assembled.
fn prepare_runtime_object_with_identity(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
    pic: bool,
    library_boundary: bool,
    build_identity: &[u8],
) -> Result<PreparedRuntimeObject, String> {
    let cache_dir = usable_runtime_object_cache_dir()?;

    let request_key = identity::runtime_cache_key_with_build_identity_and_boundary(
        heap_size,
        target,
        crate::codegen::compile_php_version().version_id(),
        features,
        pic,
        library_boundary,
        build_identity,
    );
    let alias_path = cache_dir.join(runtime_cache_alias_name(heap_size, target, request_key));
    if let Some(cache_path) = read_runtime_cache_alias(&alias_path) {
        let integrity_path = runtime_integrity_path(&cache_path);
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
    }

    let runtime_asm = codegen::generate_runtime_with_features_mode(
        heap_size,
        target,
        features,
        pic,
        library_boundary,
    );
    let content_key = runtime_content_key(target, &runtime_asm);
    let cache_path = cache_dir.join(runtime_cache_file_name(heap_size, target, content_key));
    let integrity_path = runtime_integrity_path(&cache_path);
    let status = if cache_path.exists() && runtime_object_is_intact(&cache_path, &integrity_path) {
        RuntimeCacheStatus::Hit
    } else {
        assemble_runtime_object(&cache_dir, &cache_path, &integrity_path, target, runtime_asm)?
    };
    // The alias only spares the next request the generation; failing to record it costs
    // speed, never correctness, so it does not fail the compile.
    let _ = write_runtime_cache_alias(&alias_path, &cache_path);

    let Some(prepared) = lease_runtime_object(&cache_path, &integrity_path, status)? else {
        return prepare_runtime_object_with_identity(
            heap_size,
            target,
            features,
            pic,
            library_boundary,
            build_identity,
        );
    };
    prune_runtime_cache_objects(&cache_dir, &cache_path);
    Ok(prepared)
}

/// Assembles runtime assembly into a content-named cache object and publishes its sidecar.
///
/// Returns `Hit` when a concurrent compiler published the same object first.
fn assemble_runtime_object(
    cache_dir: &std::path::Path,
    cache_path: &std::path::Path,
    integrity_path: &std::path::Path,
    target: Target,
    runtime_asm: String,
) -> Result<RuntimeCacheStatus, String> {
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

    match fs::rename(&temp_obj_path, cache_path) {
        Ok(()) => {
            write_runtime_object_integrity(cache_path, integrity_path)?;
            Ok(RuntimeCacheStatus::Miss)
        }
        Err(_err) if cache_path.exists() && runtime_object_is_intact(cache_path, integrity_path) => {
            let _ = fs::remove_file(&temp_obj_path);
            Ok(RuntimeCacheStatus::Hit)
        }
        Err(err) => {
            let _ = fs::remove_file(&temp_obj_path);
            Err(format!(
                "failed to store runtime cache '{}': {}",
                cache_path.display(),
                err
            ))
        }
    }
}

/// Returns the integrity sidecar path published beside one cache object.
fn runtime_integrity_path(cache_path: &std::path::Path) -> PathBuf {
    cache_path.with_file_name(format!(
        "{}.integrity",
        cache_path.file_name().and_then(|name| name.to_str()).unwrap_or("runtime.o")
    ))
}

/// Returns the directory holding prepared runtime objects: `ELEPHC_RUNTIME_CACHE_DIR` when it
/// is set and non-empty, else [`runtime_cache_dir`].
///
/// The override moves only the runtime objects, which are keyed by everything that shapes them
/// and safe to share between concurrent compilers; REPL hosts and `elephc test` outputs stay
/// under the cache root. Test suites that isolate `XDG_CACHE_HOME` per test use it to share
/// one runtime object per shape instead of regenerating it on every compile.
fn runtime_object_cache_dir() -> PathBuf {
    match env::var_os("ELEPHC_RUNTIME_CACHE_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => runtime_cache_dir(),
    }
}

/// Returns a runtime-object cache directory this process may write, or a temporary one.
///
/// A cache the process cannot create, or one it does not own, must never stop a compile: a
/// read-only checkout, or a target tree restored from another user's CI artifact, is otherwise
/// indistinguishable from a broken install. The runtime is then assembled into the OS temporary
/// directory, which keeps the per-user ownership the hardening requires without silently
/// trusting the rejected directory.
fn usable_runtime_object_cache_dir() -> Result<PathBuf, String> {
    let requested = runtime_object_cache_dir();
    if directory_accepts_files(&requested) && harden_runtime_cache_dir(&requested).is_ok() {
        return Ok(requested);
    }
    let fallback = env::temp_dir().join(temporary_runtime_cache_dir_name());
    if !directory_accepts_files(&fallback) {
        return Err(format!(
            "failed to create runtime cache '{}': the directory does not accept files",
            fallback.display()
        ));
    }
    harden_runtime_cache_dir(&fallback)?;
    Ok(fallback)
}

/// Returns whether `dir` exists and accepts a new file.
///
/// `create_dir_all` succeeds for a directory that already exists, and the ownership hardening
/// only inspects the group and other bits, so a cache this user owns with mode `0500` would be
/// selected and then fail while writing the assembly or a lease. A real file probe also covers
/// ACLs and read-only mounts, which the mode bits cannot describe.
fn directory_accepts_files(dir: &std::path::Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(
        ".write-probe.{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default()
    ));
    match fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Returns the per-user directory name of the runtime cache under the OS temporary directory.
///
/// `/tmp` serves every user on a Linux machine and the cache directory is hardened to
/// owner-only, so one fixed name would let the first user's directory reject every other user's
/// compile once it exists. The effective user id keeps each user's fallback theirs; the
/// ownership check still refuses a directory this user does not own. The integration-test helper
/// `tests/integration.rs::shared_runtime_cache_dir` derives the same name.
fn temporary_runtime_cache_dir_name() -> String {
    #[cfg(unix)]
    let owner = unsafe { libc::geteuid() };
    #[cfg(not(unix))]
    let owner = 0;
    format!("elephc-runtime-cache-{owner}")
}

/// Returns the platform-specific cache root for runtime objects, REPL hosts and test outputs.
pub(crate) fn runtime_cache_dir() -> PathBuf {
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(path).join("elephc")
    } else if let Some(home) = env::var_os("HOME") {
        PathBuf::from(home).join(".cache").join("elephc")
    } else {
        env::temp_dir().join("elephc-cache")
    }
}
