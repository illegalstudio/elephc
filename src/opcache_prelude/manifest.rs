//! Purpose:
//! Builds and renders the compile-time OPcache script manifest.
//!
//! Called from:
//! - The OPcache prelude facade and sibling rendering modules.
//!
//! Key details:
//! - Canonical paths, stable ordering, and filesystem metadata remain coupled.

#[allow(unused_imports)]
use super::*;

/// Returns whether a compile-time `--ini` override was supplied for directive `name`.
///
/// Only the PRESENCE matters here (it is what turns a NULL-defaulting directive into an assigned
/// one — see the `__elephc_opcache_ini_null` arms in [`render_opcache_ini_helpers`]), so this does
/// not duplicate `crate::opcache::directives`' value-resolving lookup.
pub(super) fn latest_ini_override<'a>(overrides: &'a [(String, String)], name: &str) -> Option<&'a str> {
    overrides
        .iter()
        .rev()
        .find(|(key, _)| key.as_str() == name)
        .map(|(_, value)| value.as_str())
}

/// The `opcache_get_configuration` function name (lowercase) the detector matches.
pub(super) const GET_CONFIGURATION_FN: &str = "opcache_get_configuration";

/// The `opcache_reset` function name (lowercase) the detector matches.
pub(super) const RESET_FN: &str = "opcache_reset";

/// The `opcache_get_status` function name (lowercase) the detector matches.
pub(super) const GET_STATUS_FN: &str = "opcache_get_status";

/// The `opcache_is_script_cached` function name (lowercase) the detector matches.
pub(super) const IS_SCRIPT_CACHED_FN: &str = "opcache_is_script_cached";

/// The `opcache_invalidate` function name (lowercase) the detector matches.
pub(super) const INVALIDATE_FN: &str = "opcache_invalidate";

/// The `opcache_compile_file` function name (lowercase) the detector matches.
pub(super) const COMPILE_FILE_FN: &str = "opcache_compile_file";

/// The `opcache_is_script_cached_in_file_cache` function name (lowercase) the detector matches.
pub(super) const IS_SCRIPT_CACHED_IN_FILE_CACHE_FN: &str = "opcache_is_script_cached_in_file_cache";

/// The `opcache_jit_blacklist` function name (lowercase) the detector matches.
pub(super) const JIT_BLACKLIST_FN: &str = "opcache_jit_blacklist";

/// One entry in the compile-time OPcache *script manifest* — a physical source file baked
/// into the AOT binary and therefore reported as "cached" by the OPcache status/query
/// functions (`opcache_get_status`, `opcache_is_script_cached`, `opcache_compile_file`).
///
/// Constructed in `crate::pipeline` by [`collect_manifest`] from THREE sources, each path
/// stat'd once at compile time:
/// 1. the canonicalized main entry file,
/// 2. every statically-resolved `include`/`require`/`include_once`/`require_once` target
///    (`resolver::resolve_collecting_includes`),
/// 3. every autoloaded file — eager files, PSR-4 / SPL-rule class files, and
///    the includes those files themselves pull in (`autoload::run_collecting_included`).
///
/// Together that is exactly the set of PHP/LFC source files compiled into the binary, which is
/// what "cached script" means for an AOT build. The `eval` interpreter has no AOT binary and
/// thus no manifest at all; its OPcache file functions stay empty-cache (see
/// `crates/elephc-magician/.../opcache_file_functions.rs`).
///
/// NOT represented (and cannot be): a file reached only through a DYNAMIC include whose path
/// the resolver could not fold to a constant — such a file is not compiled into the binary
/// either, so omitting it is correct rather than a shortfall.
pub struct ScriptEntry {
    /// Canonical (realpath-normalized) absolute path of the cached script. MUST match the
    /// canonicalization `__FILE__` bakes (`crate::magic_constants::file_pass`, which uses
    /// `Path::canonicalize`) so `opcache_is_script_cached(__FILE__)` and the `scripts` map
    /// key line up with a userland `realpath()` result.
    pub path: String,
    /// The script's source-file mtime as Unix seconds (from `std::fs::metadata`).
    /// Reference PHP reports this as the `timestamp` field (with `validate_timestamps` on,
    /// the default).
    pub timestamp: i64,
    /// A synthetic-but-stable per-script memory figure (implementation-defined; here it is
    /// derived from the source file size). No two real PHP builds agree on this value; only
    /// its presence and rough magnitude matter for callers.
    pub memory_consumption: i64,
}

/// Builds the compile-time OPcache script manifest: every physical source file compiled into this
/// binary. Called by `crate::pipeline` twice — once before `inject_if_used` (the placeholder
/// manifest, which cannot yet see the autoloaded set) and once after `autoload::run` with the
/// complete set, whose result [`bake_manifest`] bakes in. See [`bake_manifest`] for why the
/// split is necessary.
///
/// - `main_file` is `CliConfig.filename`; it is `canonicalize`d so the manifest path matches
///   the canonicalization `__FILE__` bakes (`crate::magic_constants::file_pass`).
/// - `included_files` are the statically-resolved include/require targets
///   (`resolver::resolve_collecting_includes`), already canonical.
/// - `autoloaded_files` are the files the autoload pass loaded — eager files,
///   PSR-4 / SPL-rule class files, and their own include targets
///   (`autoload::run_collecting_included`), already canonical.
///
/// ORDER (deterministic and documented; reference PHP's own `scripts` order is its internal
/// hash order and therefore not reproducible, so any stable order is as faithful):
/// 1. the entry file,
/// 2. the statically-included files, sorted by canonical path,
/// 3. the autoloaded files, sorted by canonical path.
/// Both callers pass groups 2 and 3 pre-sorted by their producing pass. Duplicates are
/// dropped across ALL groups, first occurrence winning — so a file that is both `require`d
/// and autoloaded appears once, in the include group, and the entry file never repeats.
///
/// Each path is stat'd once: `timestamp` = mtime as Unix seconds, `memory_consumption` =
/// the source file size in bytes (implementation-defined; see `ScriptEntry`). An entry whose
/// `canonicalize`/`metadata`/`modified` fails is SKIPPED rather than fabricated — an honest
/// omission. The `eval` interpreter has no manifest at all (documented on `ScriptEntry`).
pub fn collect_manifest(
    main_file: &str,
    included_files: &[PathBuf],
    autoloaded_files: &[PathBuf],
) -> Vec<ScriptEntry> {
    let mut manifest: Vec<ScriptEntry> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    push_manifest_entry(&mut manifest, &mut seen, Path::new(main_file));
    for file in included_files {
        push_manifest_entry(&mut manifest, &mut seen, file);
    }
    for file in autoloaded_files {
        push_manifest_entry(&mut manifest, &mut seen, file);
    }
    manifest
}

/// Stats `path` and appends a `ScriptEntry` (deduplicated by canonical string). Any I/O
/// failure — missing file, unreadable metadata, or a pre-epoch mtime — skips the entry
/// silently rather than baking a fabricated timestamp or size.
///
/// `path` is `canonicalize`d HERE rather than trusted: the producing passes already
/// canonicalize, but the entry file arrives as the raw CLI argument, and canonicalizing every
/// candidate is what guarantees the dedup key and the baked path both match `__FILE__`.
pub(super) fn push_manifest_entry(manifest: &mut Vec<ScriptEntry>, seen: &mut HashSet<String>, path: &Path) {
    let Ok(path) = path.canonicalize() else {
        return;
    };
    let path_str = path.display().to_string();
    if !seen.insert(path_str.clone()) {
        return;
    }
    let path = path.as_path();
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    let Ok(modified) = metadata.modified() else {
        return;
    };
    let Ok(elapsed) = modified.duration_since(UNIX_EPOCH) else {
        return;
    };
    manifest.push(ScriptEntry {
        path: path_str,
        timestamp: elapsed.as_secs() as i64,
        // Implementation-defined per-script memory: the source file's byte size.
        memory_consumption: metadata.len() as i64,
    });
}
