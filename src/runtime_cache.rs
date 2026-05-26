//! Purpose:
//! Builds and caches the reusable runtime object that is linked beside generated user code.
//! Keys cache entries by compiler version, target, heap size, and runtime assembly hash.
//!
//! Called from:
//! - `crate::pipeline::compile()` before user assembly is linked into the final binary.
//!
//! Key details:
//! - Temporary assembly/object files are renamed into place to tolerate concurrent compiler runs.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::codegen;
use crate::codegen::platform::{Platform, Target};

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
    /// Path to the cached runtime object file.
    pub path: PathBuf,
    /// Whether the object was found in the cache (hit) or built now (miss).
    pub status: RuntimeCacheStatus,
}

/// Builds (or retrieves from cache) the runtime object file for the given heap size and target.
/// On cache miss, generates runtime assembly, assembles it to an object file, and caches the result.
/// The cache key includes compiler version, target, heap size, and a hash of the runtime assembly.
pub fn prepare_runtime_object(heap_size: usize, target: Target) -> Result<PreparedRuntimeObject, String> {
    let cache_dir = runtime_cache_dir();
    fs::create_dir_all(&cache_dir)
        .map_err(|err| format!("failed to create runtime cache '{}': {}", cache_dir.display(), err))?;

    let runtime_asm = codegen::generate_runtime(heap_size, target);
    let runtime_hash = runtime_asm_hash(&runtime_asm);
    let cache_path = cache_dir.join(runtime_cache_file_name(heap_size, target, runtime_hash));
    if cache_path.exists() {
        return Ok(PreparedRuntimeObject {
            path: cache_path,
            status: RuntimeCacheStatus::Hit,
        });
    }

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

    let mut assembler = Command::new(target.assembler_cmd());
    if target.platform == Platform::MacOS {
        assembler.args(["-arch", target.darwin_arch_name()]);
    }
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

    match fs::rename(&temp_obj_path, &cache_path) {
        Ok(()) => Ok(PreparedRuntimeObject {
            path: cache_path,
            status: RuntimeCacheStatus::Miss,
        }),
        Err(_err) if cache_path.exists() => {
            let _ = fs::remove_file(&temp_obj_path);
            Ok(PreparedRuntimeObject {
                path: cache_path,
                status: RuntimeCacheStatus::Hit,
            })
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

/// Builds the cache file name for a runtime object.
fn runtime_cache_file_name(heap_size: usize, target: Target, runtime_hash: u64) -> String {
    format!(
        "runtime-v{}-{}-rt{:016x}-heap{}.o",
        env!("CARGO_PKG_VERSION"),
        target.as_str(),
        runtime_hash,
        heap_size
    )
}

/// Computes a 64-bit FNV-1a hash of the given assembly string.
fn runtime_asm_hash(asm: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in asm.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
