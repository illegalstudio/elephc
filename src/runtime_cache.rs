//! Purpose:
//! Builds and caches the reusable runtime object that is linked beside generated user code.
//! Keys cache entries by compiler version, target, heap size, and the feature/PIC inputs that
//! produce the assembly — not by a hash of the assembly itself, so a cache hit never builds it.
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
use crate::codegen::RuntimeFeatures;

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

/// Builds (or retrieves from cache) the runtime object file for the given heap size, target, and features.
/// On cache miss, generates runtime assembly, assembles it to an object file, and caches the result.
/// The cache key includes compiler version, target, heap size, the PIC mode, and the feature set.
/// `pic` selects position-independent emission for `--emit cdylib` artifacts so the runtime object can be
/// linked into a shared library without text-segment relocations.
pub fn prepare_runtime_object(
    heap_size: usize,
    target: Target,
    features: RuntimeFeatures,
    pic: bool,
) -> Result<PreparedRuntimeObject, String> {
    let cache_dir = runtime_cache_dir();
    fs::create_dir_all(&cache_dir)
        .map_err(|err| format!("failed to create runtime cache '{}': {}", cache_dir.display(), err))?;

    // Keyed on the INPUTS, not on a hash of the generated assembly. The emission is a pure
    // function of (heap size, target, features, pic), so the two keys discriminate identically —
    // but hashing the output meant building 1.31 MB of assembly on every compile, cache hit
    // included, to look up a file already on disk. `runtime_emission_is_deterministic` pins the
    // purity; the assembly is now generated only on the miss path below.
    let cache_path = cache_dir.join(runtime_cache_file_name(
        heap_size,
        target,
        runtime_cache_key(features, pic),
    ));
    if cache_path.exists() {
        return Ok(PreparedRuntimeObject {
            path: cache_path,
            status: RuntimeCacheStatus::Hit,
        });
    }

    let runtime_asm =
        codegen::generate_runtime_with_features_pic(heap_size, target, features, pic);

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
        "runtime-v{}-{}-b{:016x}-rt{:016x}-heap{}.o",
        env!("CARGO_PKG_VERSION"),
        target.as_str(),
        compiler_build_id(),
        runtime_hash,
        heap_size
    )
}

/// Identifies the compiler build that would emit this runtime.
///
/// Keying on inputs rather than on a hash of the output removed something the old key gave for
/// free: when the emitter itself changed, the output hash changed and the cache invalidated. The
/// inputs do not, so without this a rebuilt compiler serves the PREVIOUS build's object under a
/// matching key — silently. That is not hypothetical; it happened while developing this change,
/// and the symptom was an optimisation that appeared to do nothing.
///
/// The version alone is not enough because it only moves at release time, which is precisely when
/// the emitter is NOT changing. The executable's size and modification time move on every rebuild
/// and cost one `stat`. A compiler that cannot inspect itself falls back to a constant: the cache
/// then behaves as it did before this was added, which is a stale-object risk, not a wrong answer.
fn compiler_build_id() -> u64 {
    fn identity() -> Option<u64> {
        let exe = std::env::current_exe().ok()?;
        let meta = fs::metadata(exe).ok()?;
        let modified = meta
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos() as u64;
        Some(modified ^ (meta.len().rotate_left(32)))
    }
    identity().unwrap_or(0)
}

/// Identifies one runtime object from the inputs that produce it.
///
/// `pic` occupies the high bit rather than extending the feature word, so adding a feature never
/// shifts it. Two different keys may name byte-identical objects — that only costs a duplicate
/// cache entry. The direction that must not happen is one key naming two different objects, which
/// is exactly the determinism `runtime_emission_is_deterministic` asserts.
fn runtime_cache_key(features: RuntimeFeatures, pic: bool) -> u64 {
    features.cache_key_bits() | ((pic as u64) << 63)
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a feature set with one bit set, by index into the cache-key bit order.
    fn features_with_bit(bit: u32) -> RuntimeFeatures {
        let mut features = RuntimeFeatures::none();
        match bit {
            0 => features.regex = true,
            1 => features.mb_strlen = true,
            2 => features.phar_archive = true,
            3 => features.descriptor_invoker = true,
            4 => features.eval_bridge = true,
            5 => features.eval_scope = true,
            6 => features.web = true,
            7 => features.pdo_udf = true,
            _ => unreachable!("cache_key_bits packs eight features"),
        }
        features
    }

    /// The property the input-keyed cache rests on: one key names exactly one runtime object.
    ///
    /// Keying on the inputs instead of on a hash of the emitted assembly is what lets a cache hit
    /// skip generating 1.31 MB of text. That is only sound while emission is a pure function of
    /// those inputs — if the same key could ever name two different objects, a compile would link
    /// a stale runtime and nothing would say so. Checked both ways: emitting twice for one key
    /// gives identical bytes, and no two keys in the sweep disagree about their object.
    #[test]
    fn runtime_emission_is_deterministic() {
        let target = Target {
            platform: Platform::MacOS,
            arch: crate::codegen::platform::Arch::AArch64,
        };
        let mut seen: std::collections::HashMap<u64, String> = std::collections::HashMap::new();

        for bit in 0..8u32 {
            for pic in [false, true] {
                let features = features_with_bit(bit);
                let key = runtime_cache_key(features, pic);
                let first =
                    codegen::generate_runtime_with_features_pic(8 * 1024 * 1024, target, features, pic);
                let second =
                    codegen::generate_runtime_with_features_pic(8 * 1024 * 1024, target, features, pic);
                assert_eq!(
                    first, second,
                    "runtime emission for key {key:#018x} is not deterministic"
                );
                if let Some(previous) = seen.insert(key, first.clone()) {
                    assert_eq!(
                        previous, first,
                        "cache key {key:#018x} names two different runtime objects"
                    );
                }
            }
        }
    }

    /// Verifies `pic` cannot be confused with a feature, now or after features are added.
    ///
    /// It rides in the high bit precisely so that appending a ninth feature never shifts it. A
    /// collision here would link the position-independent runtime into an executable, or the
    /// reverse, under a key that looked right.
    #[test]
    fn pic_never_collides_with_a_feature_bit() {
        for bit in 0..8u32 {
            let features = features_with_bit(bit);
            assert_ne!(
                runtime_cache_key(features, false),
                runtime_cache_key(features, true),
                "pic must change the key"
            );
            assert_ne!(
                runtime_cache_key(features, true),
                runtime_cache_key(RuntimeFeatures::none(), true),
                "each feature must change the key"
            );
        }
    }
}
