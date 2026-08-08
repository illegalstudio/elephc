//! Purpose:
//! Renders OPcache status fragments and PHP literals.
//!
//! Called from:
//! - The OPcache prelude facade and sibling rendering modules.
//!
//! Key details:
//! - Preload and interned-string keys retain reference ordering and omission rules.

#[allow(unused_imports)]
use super::*;

/// One directive value as its baked PHP literal.
pub(super) fn directive_value_expr(value: &DirectiveValue) -> Expr {
    match value {
        DirectiveValue::Bool(boolean) => e_bool(*boolean),
        DirectiveValue::Int(int) => build::php_int(*int),
        DirectiveValue::Float(float) => e_float(*float),
        DirectiveValue::Str(string) => e_str(string),
    }
}

/// A list of strings as a flat PHP array literal. An empty slice renders `[]`.
pub(super) fn php_string_list_expr(values: &[String]) -> Expr {
    e_array(values.iter().map(|value| e_str(value)).collect())
}

/// The manifest's canonical paths as the `in_array(..., true)` haystack `opcache_is_script_cached`,
/// `opcache_invalidate` and `opcache_compile_file` test against. An empty manifest renders `[]`
/// (valid PHP; membership is always `false`).
pub(super) fn manifest_paths_expr(manifest: &[ScriptEntry]) -> Expr {
    e_array(manifest.iter().map(|entry| e_str(&entry.path)).collect())
}

/// The `preload_statistics` value in the VERIFIED reference key order, omitting `functions` /
/// `classes` when empty exactly as reference PHP does (see [`PreloadStatistics`]).
pub(super) fn preload_statistics_expr(stats: &PreloadStatistics) -> Expr {
    let mut entries = vec![(
        e_str("memory_consumption"),
        build::php_int(stats.memory_consumption),
    )];
    if !stats.functions.is_empty() {
        entries.push((
            e_str("functions"),
            php_string_list_expr(&stats.functions),
        ));
    }
    if !stats.classes.is_empty() {
        entries.push((e_str("classes"), php_string_list_expr(&stats.classes)));
    }
    entries.push((e_str("scripts"), php_string_list_expr(&stats.scripts)));
    e_array_assoc(entries)
}

/// The `interned_strings_usage` sub-array, or `None` when the buffer was never stood up.
///
/// php-src emits it only under `if (ZCSG(interned_strings).start && ZCSG(interned_strings).end)`,
/// and `opcache.interned_strings_buffer=0` allocates neither — so the KEY is absent, not empty
/// and not zeroed. `used + free == buffer` EXACTLY and `free > 0` whenever it is reported at all;
/// see [`render_interned_used_memory`].
pub(super) fn interned_strings_usage_expr(buffer_size: i64) -> Option<Expr> {
    if buffer_size <= 0 {
        return None;
    }
    let used = render_interned_used_memory(buffer_size);
    Some(e_array_assoc(vec![
        (e_str("buffer_size"), build::php_int(buffer_size)),
        (e_str("used_memory"), build::php_int(used)),
        (
            e_str("free_memory"),
            build::php_int(buffer_size - used),
        ),
        (
            e_str("number_of_strings"),
            e_int(STATUS_INTERNED_NUMBER_OF_STRINGS),
        ),
    ]))
}

/// The `opcache_get_status()` declaration baked with the compile-time cache-enabled gate and the
/// target's directive-derived figures. `web` selects the SAPI-gated enabled constant; `restricted`
/// FORCES it `false` (so the function always returns `false`, as reference PHP does when the API
/// is restricted) and adds the warning. The array exit is kept either way, which preserves the
/// reference `array|false` signature so a caller's `is_array()` guard still narrows.
pub(super) fn get_status_declaration(
    php_version: PhpVersion,
    web: bool,
    manifest: &[ScriptEntry],
    overrides: &[(String, String)],
    restricted: bool,
    preload: Option<&PreloadStatistics>,
) -> Stmt {
    let version_id = php_version.version_id();

    // One manifest entry ≈ one cached script ≈ one cache key (reference OPcache keys a
    // script by full path plus optional aliases; the MVP has one key per script).
    let num_cached_scripts = manifest.len() as i64;

    // Sum the per-script memory so `used_memory` covers the reported scripts (coherence).
    let scripts_memory_total: i64 = manifest.iter().map(|entry| entry.memory_consumption).sum();
    let memory_total = directive_int(version_id, "opcache.memory_consumption", overrides);
    let memory_used = STATUS_USED_MEMORY + scripts_memory_total;

    let revalidate_freq = directive_int(version_id, "opcache.revalidate_freq", overrides);
    // Reference reports `interned_strings_buffer` (MiB) as a byte count here.
    let interned_buffer_size =
        directive_int(version_id, "opcache.interned_strings_buffer", overrides) * BYTES_PER_MIB;
    let jit = render_jit_status(version_id, overrides);

    build::get_status_decl(build::StatusFacts {
        // A restricted API always returns false, so the gate constant is forced regardless of SAPI.
        enabled: !restricted
            && opcache_cache_enabled_with_overrides(version_id, web, overrides),
        warning: restrict_api_warning(restricted),
        memory_used,
        // INVARIANT (class-B): free = total - used - wasted, with wasted = 0.
        memory_free: memory_total - memory_used,
        interned_strings_usage: interned_strings_usage_expr(interned_buffer_size),
        num_cached_scripts,
        num_cached_keys: num_cached_scripts,
        // `max_cached_keys` is OPcache's prime-rounded hash capacity derived from
        // `max_accelerated_files` — the exact php-src table, byte-verified boundary by boundary.
        max_cached_keys: accel_hash_max_num_entries(directive_int(
            version_id,
            "opcache.max_accelerated_files",
            overrides,
        )),
        preload_statistics: preload.map(preload_statistics_expr),
        scripts_map: scripts_map_expr(manifest, revalidate_freq, version_id),
        jit: build::JitFacts {
            enabled: jit.enabled,
            on: jit.on,
            kind: jit.kind,
            opt_level: jit.opt_level,
            opt_flags: jit.opt_flags,
            buffer_size: jit.buffer_size,
            buffer_free: jit.buffer_free,
        },
    })
}
