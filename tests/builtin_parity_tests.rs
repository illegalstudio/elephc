//! Purpose:
//! Integration tests for builtin catalog parity between static elephc and
//! elephc-magician's eval interpreter.
//!
//! Called from:
//! - `cargo test --test builtin_parity_tests` through Rust's test harness.
//!
//! Key details:
//! - Static builtin names and signatures are read from compiler metadata APIs.
//! - Eval builtin existence and signature shape are read from magician metadata APIs.

use std::collections::BTreeSet;

const EVAL_DIRECT_DISPATCH_SOURCES: &[&str] = &[include_str!(
    "../crates/elephc-magician/src/interpreter/expressions.rs"
)];

const EVAL_DYNAMIC_DISPATCH_SOURCES: &[&str] = &[
    include_str!("../crates/elephc-magician/src/interpreter/builtins/raw_memory.rs"),
    include_str!(
        "../crates/elephc-magician/src/interpreter/builtins/registry/dispatch/arrays.rs"
    ),
    include_str!(
        "../crates/elephc-magician/src/interpreter/builtins/registry/dispatch/core.rs"
    ),
    include_str!(
        "../crates/elephc-magician/src/interpreter/builtins/registry/dispatch/filesystem.rs"
    ),
    include_str!(
        "../crates/elephc-magician/src/interpreter/builtins/registry/dispatch/network_env.rs"
    ),
    include_str!(
        "../crates/elephc-magician/src/interpreter/builtins/registry/dispatch/scalars.rs"
    ),
    include_str!(
        "../crates/elephc-magician/src/interpreter/builtins/registry/dispatch/symbols.rs"
    ),
];

/// Eval-only reflection probes exist because magician can inspect dynamic eval metadata before the AOT catalog exposes them.
const EVAL_ONLY_REFLECTION_BUILTINS: &[&str] = &[
    "get_called_class",
    "get_class_methods",
    "get_class_vars",
    "get_object_vars",
];

/// Static-only registered builtins exist in the compiler before magician/eval has runtime support.
const STATIC_ONLY_REGISTRY_BUILTINS: &[&str] = &[
    "array_all",
    "array_any",
    "array_diff_assoc",
    "array_find",
    "array_intersect_assoc",
    "array_is_list",
    "array_key_first",
    "array_key_last",
    "array_merge_recursive",
    "array_multisort",
    "array_replace",
    "array_replace_recursive",
    "array_udiff",
    "array_uintersect",
    "array_walk_recursive",
    "serialize",
    "unserialize",
];

/// Eval supports these PHP optional parameters before the static backend does.
const EVAL_SIGNATURE_EXTENSION_BUILTINS: &[&str] = &[
    "array_reverse",
    "array_splice",
    "nl2br",
    "preg_match",
    "print_r",
];

/// Eval supports extra optional by-reference parameters before the static backend does.
const EVAL_BY_REF_SIGNATURE_EXTENSION_BUILTINS: &[&str] = &["is_callable", "preg_match_all"];

/// Eval supports variadic debug output before the static backend does.
const EVAL_VARIADIC_SIGNATURE_EXTENSION_BUILTINS: &[&str] = &["var_dump"];

/// Builtins migrated to magician's declarative eval registry.
const EVAL_DECLARATIVE_REGISTRY_BUILTINS: &[&str] = &[
    "abs",
    "acos",
    "addslashes",
    "array_flip",
    "array_key_exists",
    "array_keys",
    "array_pad",
    "array_product",
    "array_rand",
    "array_reverse",
    "array_search",
    "array_slice",
    "array_sum",
    "array_unique",
    "array_values",
    "asin",
    "atan",
    "atan2",
    "basename",
    "base64_decode",
    "base64_encode",
    "bin2hex",
    "boolval",
    "ceil",
    "checkdate",
    "chdir",
    "chgrp",
    "chmod",
    "chown",
    "closedir",
    "chr",
    "chop",
    "clearstatcache",
    "clamp",
    "cos",
    "cosh",
    "copy",
    "count",
    "crc32",
    "ctype_alnum",
    "ctype_alpha",
    "ctype_digit",
    "ctype_space",
    "date",
    "date_default_timezone_get",
    "date_default_timezone_set",
    "deg2rad",
    "dirname",
    "disk_free_space",
    "disk_total_space",
    "explode",
    "exp",
    "fdiv",
    "file",
    "file_exists",
    "file_get_contents",
    "file_put_contents",
    "fileatime",
    "filectime",
    "filegroup",
    "fileinode",
    "filemtime",
    "fileowner",
    "fileperms",
    "filesize",
    "filetype",
    "fclose",
    "fdatasync",
    "feof",
    "fflush",
    "fgetc",
    "fgets",
    "floatval",
    "floor",
    "fmod",
    "fnmatch",
    "fpassthru",
    "fread",
    "fseek",
    "fstat",
    "ftruncate",
    "fsync",
    "ftell",
    "fwrite",
    "getdate",
    "getcwd",
    "gettype",
    "gmdate",
    "gmmktime",
    "glob",
    "grapheme_strrev",
    "gzcompress",
    "gzdeflate",
    "gzinflate",
    "gzuncompress",
    "hash",
    "hash_algos",
    "hash_copy",
    "hash_equals",
    "hash_file",
    "hash_final",
    "hash_hmac",
    "hash_init",
    "hash_update",
    "header",
    "hex2bin",
    "html_entity_decode",
    "htmlentities",
    "htmlspecialchars",
    "hrtime",
    "http_response_code",
    "hypot",
    "implode",
    "intdiv",
    "intval",
    "is_array",
    "is_bool",
    "is_dir",
    "is_double",
    "is_executable",
    "is_file",
    "is_finite",
    "is_float",
    "is_infinite",
    "is_int",
    "is_integer",
    "is_iterable",
    "is_link",
    "is_long",
    "is_nan",
    "is_null",
    "is_numeric",
    "is_object",
    "is_readable",
    "is_real",
    "is_resource",
    "is_scalar",
    "is_string",
    "in_array",
    "is_writable",
    "is_writeable",
    "json_decode",
    "json_encode",
    "json_last_error",
    "json_last_error_msg",
    "json_validate",
    "lchgrp",
    "lchown",
    "lcfirst",
    "link",
    "linkinfo",
    "localtime",
    "log",
    "log10",
    "log2",
    "lstat",
    "ltrim",
    "max",
    "microtime",
    "md5",
    "min",
    "mkdir",
    "mktime",
    "nl2br",
    "number_format",
    "opendir",
    "ord",
    "pathinfo",
    "pclose",
    "pi",
    "popen",
    "pow",
    "printf",
    "preg_match",
    "preg_match_all",
    "preg_replace",
    "preg_replace_callback",
    "preg_split",
    "rad2deg",
    "range",
    "rawurldecode",
    "rawurlencode",
    "readdir",
    "readfile",
    "readlink",
    "realpath",
    "realpath_cache_get",
    "realpath_cache_size",
    "rename",
    "round",
    "rtrim",
    "rewind",
    "rewinddir",
    "rmdir",
    "scandir",
    "sha1",
    "sin",
    "sinh",
    "sleep",
    "sqrt",
    "sprintf",
    "sscanf",
    "stat",
    "stream_copy_to_stream",
    "stream_get_contents",
    "stream_get_filters",
    "stream_get_line",
    "stream_get_meta_data",
    "stream_get_transports",
    "stream_get_wrappers",
    "stream_is_local",
    "stream_isatty",
    "str_contains",
    "str_ends_with",
    "str_ireplace",
    "str_pad",
    "str_replace",
    "str_split",
    "str_starts_with",
    "strcasecmp",
    "strcmp",
    "strlen",
    "str_repeat",
    "strrev",
    "strpos",
    "strrpos",
    "strstr",
    "strtolower",
    "stripslashes",
    "strtoupper",
    "strtotime",
    "substr",
    "substr_replace",
    "strval",
    "stream_resolve_include_path",
    "stream_set_blocking",
    "stream_set_chunk_size",
    "stream_set_read_buffer",
    "stream_set_timeout",
    "stream_set_write_buffer",
    "stream_supports_lock",
    "symlink",
    "sys_get_temp_dir",
    "tan",
    "tanh",
    "tempnam",
    "time",
    "tmpfile",
    "touch",
    "trim",
    "ucfirst",
    "ucwords",
    "umask",
    "unlink",
    "urldecode",
    "urlencode",
    "usleep",
    "vprintf",
    "vsprintf",
    "wordwrap",
];

/// Verifies every static builtin is visible through eval's function lookup.
#[test]
fn static_php_visible_builtins_are_visible_to_eval() {
    let missing = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .filter(|name| !STATIC_ONLY_REGISTRY_BUILTINS.contains(name))
        .filter(|name| !elephc_magician::builtin_metadata::php_visible_builtin_exists(name))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "static builtins missing from eval function lookup: {missing:?}"
    );
}

/// Verifies every static builtin appears in eval's direct and dynamic dispatch sources.
#[test]
fn static_php_visible_builtins_have_eval_dispatch_literals() {
    let direct_dispatch_names = php_symbol_string_literals(EVAL_DIRECT_DISPATCH_SOURCES);
    let dynamic_dispatch_names = php_symbol_string_literals(EVAL_DYNAMIC_DISPATCH_SOURCES);

    let missing_direct = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .filter(|name| !STATIC_ONLY_REGISTRY_BUILTINS.contains(name))
        .filter(|name| !elephc_magician::builtin_metadata::php_visible_builtin_is_registry_declared(name))
        .filter(|name| !direct_dispatch_names.contains(*name))
        .collect::<Vec<_>>();
    let missing_dynamic = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .filter(|name| !STATIC_ONLY_REGISTRY_BUILTINS.contains(name))
        .filter(|name| !elephc_magician::builtin_metadata::php_visible_builtin_is_registry_declared(name))
        .filter(|name| !dynamic_dispatch_names.contains(*name))
        .collect::<Vec<_>>();

    assert!(
        missing_direct.is_empty(),
        "static builtins missing from eval direct dispatcher literals: {missing_direct:?}"
    );
    assert!(
        missing_dynamic.is_empty(),
        "static builtins missing from eval dynamic dispatcher literals: {missing_dynamic:?}"
    );
}

/// Verifies migrated builtins are backed by magician's declarative registry.
#[test]
fn migrated_eval_builtins_are_registry_declared() {
    for name in EVAL_DECLARATIVE_REGISTRY_BUILTINS {
        assert!(
            elephc_magician::builtin_metadata::php_visible_builtin_is_registry_declared(name),
            "{name} should be declared through the eval builtin registry"
        );
    }
}

/// Extracts lowercase PHP-symbol string literals from Rust source snippets.
fn php_symbol_string_literals(sources: &[&str]) -> BTreeSet<String> {
    let mut literals = BTreeSet::new();
    for source in sources {
        collect_php_symbol_string_literals(source, &mut literals);
    }
    literals
}

/// Adds simple double-quoted PHP-symbol literals from one Rust source string.
fn collect_php_symbol_string_literals(source: &str, literals: &mut BTreeSet<String>) {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }

        index += 1;
        let mut literal = String::new();
        let mut escaped = false;
        while index < bytes.len() {
            let byte = bytes[index];
            index += 1;
            if escaped {
                literal.push(byte as char);
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                break;
            } else {
                literal.push(byte as char);
            }
        }

        if is_php_symbol_literal(&literal) {
            literals.insert(literal);
        }
    }
}

/// Returns whether a string literal can be a lowercase PHP builtin symbol.
fn is_php_symbol_literal(literal: &str) -> bool {
    !literal.is_empty()
        && literal
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Verifies eval has signature metadata for each shared static builtin.
#[test]
fn shared_builtin_signature_shape_matches_static_signatures() {
    let mut missing_static_signature = Vec::new();
    let mut missing_eval_signature = Vec::new();
    let mut mismatched_signatures = Vec::new();

    for name in elephc::builtin_metadata::php_visible_builtin_names() {
        if STATIC_ONLY_REGISTRY_BUILTINS.contains(name) {
            continue;
        }
        let Some(static_meta) = elephc::builtin_metadata::builtin_signature_metadata(name) else {
            missing_static_signature.push(*name);
            continue;
        };
        let Some(eval_meta) = elephc_magician::builtin_metadata::builtin_signature_metadata(name) else {
            missing_eval_signature.push(*name);
            continue;
        };
        if EVAL_SIGNATURE_EXTENSION_BUILTINS.contains(name) {
            assert_eval_signature_extends_static_signature(name, &static_meta, &eval_meta);
            continue;
        }
        if EVAL_BY_REF_SIGNATURE_EXTENSION_BUILTINS.contains(name) {
            assert_eval_by_ref_signature_extends_static_signature(name, &static_meta, &eval_meta);
            continue;
        }
        if EVAL_VARIADIC_SIGNATURE_EXTENSION_BUILTINS.contains(name) {
            assert_eval_variadic_signature_extends_static_signature(
                name,
                &static_meta,
                &eval_meta,
            );
            continue;
        }

        if static_meta.params != eval_meta.params
            || static_meta.required_param_count != eval_meta.required_param_count
            || static_meta.default_param_count != eval_meta.default_param_count
            || static_meta.variadic != eval_meta.variadic
            || static_meta.by_ref_params != eval_meta.by_ref_params
        {
            mismatched_signatures.push((*name, static_meta, eval_meta));
        }
    }

    assert!(
        missing_static_signature.is_empty(),
        "static catalog entries without signature metadata: {missing_static_signature:?}"
    );
    assert!(
        missing_eval_signature.is_empty(),
        "shared builtins without eval parameter metadata: {missing_eval_signature:?}"
    );
    assert!(
        mismatched_signatures.is_empty(),
        "shared builtin signature-shape mismatches: {mismatched_signatures:#?}"
    );
}

/// Documents compiler-visible builtins whose eval support has not landed yet.
#[test]
fn static_only_registry_builtins_remain_documented_until_eval_support_lands() {
    let static_names = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    for name in STATIC_ONLY_REGISTRY_BUILTINS {
        assert!(
            static_names.contains(name),
            "{name} is no longer compiler-visible; remove it from the static-only allowlist"
        );
        assert!(
            !elephc_magician::builtin_metadata::php_visible_builtin_exists(name),
            "{name} is now eval-visible; remove it from the static-only allowlist"
        );
    }
}

/// Verifies a documented eval signature extension keeps the static prefix contract.
fn assert_eval_signature_extends_static_signature(
    name: &str,
    static_meta: &elephc::builtin_metadata::BuiltinSignatureMetadata,
    eval_meta: &elephc_magician::builtin_metadata::BuiltinSignatureMetadata,
) {
    assert!(
        eval_meta.params.starts_with(&static_meta.params),
        "{name} eval extension must preserve static parameter prefix: static={static_meta:#?} eval={eval_meta:#?}"
    );
    assert_eq!(
        static_meta.required_param_count, eval_meta.required_param_count,
        "{name} eval extension must preserve required parameter count"
    );
    assert_eq!(
        static_meta.variadic, eval_meta.variadic,
        "{name} eval extension must not change variadic behavior"
    );
    assert_eq!(
        static_meta.by_ref_params, eval_meta.by_ref_params,
        "{name} eval extension must preserve by-reference parameters"
    );
    assert!(
        eval_meta.default_param_count >= static_meta.default_param_count,
        "{name} eval extension must not remove defaults"
    );
}

/// Verifies a documented eval by-reference extension keeps the static prefix contract.
fn assert_eval_by_ref_signature_extends_static_signature(
    name: &str,
    static_meta: &elephc::builtin_metadata::BuiltinSignatureMetadata,
    eval_meta: &elephc_magician::builtin_metadata::BuiltinSignatureMetadata,
) {
    assert!(
        eval_meta.params.starts_with(&static_meta.params),
        "{name} eval by-ref extension must preserve static parameter prefix: static={static_meta:#?} eval={eval_meta:#?}"
    );
    assert_eq!(
        static_meta.required_param_count, eval_meta.required_param_count,
        "{name} eval by-ref extension must preserve required parameter count"
    );
    assert_eq!(
        static_meta.variadic, eval_meta.variadic,
        "{name} eval by-ref extension must not change variadic behavior"
    );
    assert!(
        eval_meta.by_ref_params.starts_with(&static_meta.by_ref_params),
        "{name} eval by-ref extension must preserve static by-reference prefix"
    );
    assert!(
        eval_meta.default_param_count >= static_meta.default_param_count,
        "{name} eval by-ref extension must not remove defaults"
    );
}

/// Verifies a documented eval variadic extension keeps the static prefix contract.
fn assert_eval_variadic_signature_extends_static_signature(
    name: &str,
    static_meta: &elephc::builtin_metadata::BuiltinSignatureMetadata,
    eval_meta: &elephc_magician::builtin_metadata::BuiltinSignatureMetadata,
) {
    assert!(
        eval_meta.params.starts_with(&static_meta.params),
        "{name} eval variadic extension must preserve static parameter prefix: static={static_meta:#?} eval={eval_meta:#?}"
    );
    assert_eq!(
        static_meta.required_param_count, eval_meta.required_param_count,
        "{name} eval variadic extension must preserve required parameter count"
    );
    assert!(
        static_meta.variadic.is_none() && eval_meta.variadic.is_some(),
        "{name} eval variadic extension must add, not remove, variadic behavior"
    );
    assert_eq!(
        static_meta.by_ref_params, eval_meta.by_ref_params,
        "{name} eval variadic extension must preserve by-reference parameters"
    );
    assert!(
        eval_meta.default_param_count >= static_meta.default_param_count,
        "{name} eval variadic extension must not remove defaults"
    );
}

/// Documents the current eval-only reflection builtins so the drift is explicit.
#[test]
fn eval_only_reflection_builtins_remain_visible_until_static_catalog_catches_up() {
    let static_names = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    for name in EVAL_ONLY_REFLECTION_BUILTINS {
        assert!(
            !static_names.contains(name),
            "{name} moved into the static catalog; remove it from the eval-only allowlist"
        );
        assert!(
            elephc_magician::builtin_metadata::php_visible_builtin_exists(name),
            "{name} should stay visible to eval while it is documented as eval-only"
        );
    }
}

/// Verifies magician does not expose unexpected builtin names outside the static catalog.
#[test]
fn eval_php_visible_builtins_are_static_or_documented_eval_only() {
    let static_names = elephc::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let eval_only = EVAL_ONLY_REFLECTION_BUILTINS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let unexpected = elephc_magician::builtin_metadata::php_visible_builtin_names()
        .iter()
        .copied()
        .filter(|name| !static_names.contains(name) && !eval_only.contains(name))
        .collect::<Vec<_>>();

    assert!(
        unexpected.is_empty(),
        "eval exposes builtins outside the static catalog and eval-only allowlist: {unexpected:?}"
    );
}
