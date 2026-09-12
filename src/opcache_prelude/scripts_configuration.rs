//! Purpose:
//! Renders script maps, configuration, reset, and file-function bodies.
//!
//! Called from:
//! - The OPcache prelude facade and sibling rendering modules.
//!
//! Key details:
//! - Manifest clocks, invalidation state, and SAPI gates remain coherent.

#[allow(unused_imports)]
use super::*;

/// First `version_id` whose `opcache_get_status()` script entries carry a `revalidate` key.
///
/// php-src added it in 8.3; a `--php-version 8.2` build must not report it. Verified against the
/// official `php:8.2-cli` and `php:8.3-cli` images.
pub(super) const SCRIPTS_REVALIDATE_MIN_VERSION_ID: u32 = 80300;

/// The `opcache_get_status()['scripts']` map: keyed by each script's canonical `full_path`, each
/// value the reference-shaped entry. `revalidate` is emitted only from 8.3 on (php-src added it
/// there; a `--php-version 8.2` build must not report a key its target runtime never has).
///
/// `$__elephc_opcache_start_time` IS THE REQUEST CLOCK — `opcache_get_status`'s memoized `static`
/// — so every entry reports the same instant, exactly as reference PHP does. `timestamp` goes
/// through `__elephc_opcache_script_timestamp` rather than being the bare mtime so a
/// FORCE-INVALIDATED entry reports `0`.
pub(super) fn scripts_map_expr(
    manifest: &[ScriptEntry],
    revalidate_freq: i64,
    version_id: u32,
    preload_memory: Option<i64>,
) -> Expr {
    let mut entries = manifest
        .iter()
        .map(|entry| {
            let mut fields = vec![
                (e_str("full_path"), e_str(&entry.path)),
                (e_str("hits"), e_int(0)),
                (
                    e_str("memory_consumption"),
                    build::php_int(entry.memory_consumption),
                ),
                (
                    e_str("last_used"),
                    e_call(
                        "__elephc_opcache_asctime",
                        vec![e_var("__elephc_opcache_start_time")],
                    ),
                ),
                (
                    e_str("last_used_timestamp"),
                    e_var("__elephc_opcache_start_time"),
                ),
                (
                    e_str("timestamp"),
                    e_call(
                        "__elephc_opcache_script_timestamp",
                        vec![e_str(&entry.path), build::php_int(entry.timestamp)],
                    ),
                ),
            ];
            if version_id >= SCRIPTS_REVALIDATE_MIN_VERSION_ID {
                fields.push((
                    e_str("revalidate"),
                    e_binop(
                        e_var("__elephc_opcache_start_time"),
                        BinOp::Add,
                        build::php_int(revalidate_freq),
                    ),
                ));
            }
            (e_str(&entry.path), e_array_assoc(fields))
        })
        .collect::<Vec<_>>();
    if let Some(memory) = preload_memory {
        entries.insert(0, preload_marker_entry(memory, version_id));
    }
    build::php_assoc(entries)
}

/// The literal key reference PHP gives its synthetic preload entry. NOT a path: a caller
/// walking `scripts` meets it among real filenames, and `file_exists('$PRELOAD$')` is false.
const PRELOAD_MARKER: &str = "$PRELOAD$";

/// Reference PHP's synthetic `$PRELOAD$` entry, for the block preloading holds resident.
///
/// VERIFIED on PHP 8.5.10: `full_path` is the literal `$PRELOAD$`, `hits` is `0`,
/// `memory_consumption` equals `preload_statistics.memory_consumption` exactly, and all three
/// CLOCKS ARE ZERO — `timestamp`, `last_used_timestamp` and `revalidate` are `0` and
/// `last_used` is the epoch, because the entry stands for a block rather than a file there is
/// anything to stat. `num_cached_scripts` counts it.
///
/// POSITION: reference puts it second, after the file named by `opcache.preload`. That is hash
/// insertion order rather than a documented contract, and elephc's `scripts` is manifest order,
/// which already differs from reference's compile order. It goes FIRST here — it precedes what
/// it contains — and the position is not something either runtime guarantees.
fn preload_marker_entry(memory_consumption: i64, version_id: u32) -> (Expr, Expr) {
    let mut fields = vec![
        (e_str("full_path"), e_str(PRELOAD_MARKER)),
        (e_str("hits"), e_int(0)),
        (
            e_str("memory_consumption"),
            build::php_int(memory_consumption),
        ),
        // The epoch, spelled the way `asctime` spells it, rather than the request clock every
        // real entry carries.
        (
            e_str("last_used"),
            e_call("__elephc_opcache_asctime", vec![e_int(0)]),
        ),
        (e_str("last_used_timestamp"), e_int(0)),
        (e_str("timestamp"), e_int(0)),
    ];
    if version_id >= SCRIPTS_REVALIDATE_MIN_VERSION_ID {
        fields.push((e_str("revalidate"), e_int(0)));
    }
    (e_str(PRELOAD_MARKER), e_array_assoc(fields))
}

/// The full `opcache_get_configuration()` return array for the given compile target.
///
/// Every entry outside the runtime-override scope is a plain literal. Each reporting-only entry
/// is instead a CALL to the typed environment helper carrying its compile-time value as the
/// default, which is what makes `opcache_get_configuration()['directives']` and `ini_get()` move
/// TOGETHER under an `ELEPHC_INI_*` override, the way `-d` moves both surfaces in reference PHP.
pub(super) fn configuration_expr(
    php_version: PhpVersion,
    overrides: &[(String, String)],
    ini_set_injected: bool,
) -> Expr {
    let version_id = php_version.version_id();
    let directives = effective_opcache_directives(version_id, overrides)
        .into_iter()
        .map(|(name, value)| {
            (
                e_str(name),
                directive_runtime_value_expr(name, &value, ini_set_injected),
            )
        })
        .collect();
    e_array_assoc(vec![
        (e_str("directives"), e_array_assoc(directives)),
        (
            e_str("version"),
            e_array_assoc(vec![
                (
                    e_str("version"),
                    e_str(opcache_version_string(version_id)),
                ),
                (
                    e_str("opcache_product_name"),
                    e_str(OPCACHE_PRODUCT_NAME),
                ),
            ]),
        ),
        // Filled by `build::blacklist_prologue`, which runs immediately before this array is
        // returned. Reference PHP lists the resolved `opcache.blacklist_filename` patterns here.
        (e_str("blacklist"), e_var("__elephc_blacklist")),
    ])
}

/// The compile-time cache-enabled state for this target: a web/FPM SAPI follows `opcache.enable`
/// (enabled), CLI follows `opcache.enable_cli` (disabled), read from the shared directive table
/// so the value stays correct if a default ever flips.
pub(super) fn cache_enabled(php_version: PhpVersion, web: bool, overrides: &[(String, String)]) -> bool {
    opcache_cache_enabled_with_overrides(php_version.version_id(), web, overrides)
}

/// The `opcache_is_script_cached()` declaration: disabled → always `false`; enabled →
/// `realpath`-normalized membership in the baked manifest.
pub(super) fn is_script_cached_declaration(
    php_version: PhpVersion,
    web: bool,
    manifest: &[ScriptEntry],
    overrides: &[(String, String)],
) -> Stmt {
    build::is_script_cached_decl(
        cache_enabled(php_version, web, overrides),
        manifest_paths_expr(manifest),
    )
}

/// The `opcache_invalidate()` declaration: the disabled gate short-circuits to `false`; the
/// enabled path returns whether the argument resolves, and a FORCED call on a manifest member
/// also records the discard — or, under `--strict-opcache`, throws.
pub(super) fn invalidate_declaration(
    php_version: PhpVersion,
    web: bool,
    manifest: &[ScriptEntry],
    overrides: &[(String, String)],
    strict: bool,
) -> Stmt {
    build::invalidate_decl(
        cache_enabled(php_version, web, overrides),
        manifest_paths_expr(manifest),
        strict,
    )
}

/// The `opcache_compile_file()` declaration: the disabled gate emits the `Notice:` to STDERR then
/// returns `false`; the enabled path returns `true` for a manifest member (already compiled into
/// the binary) and `false` otherwise.
pub(super) fn compile_file_declaration(
    php_version: PhpVersion,
    web: bool,
    manifest: &[ScriptEntry],
    overrides: &[(String, String)],
) -> Stmt {
    build::compile_file_decl(
        cache_enabled(php_version, web, overrides),
        manifest_paths_expr(manifest),
    )
}
