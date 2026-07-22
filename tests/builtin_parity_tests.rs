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

const EVAL_DYNAMIC_DISPATCH_SOURCES: &[&str] = &[include_str!(
    "../crates/elephc-magician/src/interpreter/builtins/raw_memory/mod.rs"
)];

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
    "zval_free",
    "zval_pack",
    "zval_type",
    "zval_unpack",
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

/// Eval supports variadic behavior before the static backend does. Empty: the
/// static `var_dump` signature is variadic, so its former entry became an exact
/// shape match; keep the slice for the next genuine variadic extension.
const EVAL_VARIADIC_SIGNATURE_EXTENSION_BUILTINS: &[&str] = &[];

/// Builtins migrated to magician's declarative eval registry.
const EVAL_DECLARATIVE_REGISTRY_BUILTINS: &[&str] = &[
    "abs",
    "acos",
    "addslashes",
    "array_chunk",
    "array_column",
    "array_combine",
    "array_diff",
    "array_diff_key",
    "array_fill",
    "array_fill_keys",
    "array_filter",
    "array_flip",
    "array_intersect",
    "array_intersect_key",
    "array_key_exists",
    "array_keys",
    "array_map",
    "array_merge",
    "array_pad",
    "array_pop",
    "array_product",
    "array_push",
    "array_rand",
    "array_reduce",
    "array_reverse",
    "array_search",
    "array_shift",
    "array_slice",
    "array_splice",
    "array_sum",
    "array_unique",
    "array_unshift",
    "array_values",
    "array_walk",
    "arsort",
    "asin",
    "asort",
    "atan",
    "atan2",
    "basename",
    "base64_decode",
    "base64_encode",
    "bin2hex",
    "boolval",
    "buffer_free",
    "buffer_len",
    "buffer_new",
    "call_user_func",
    "call_user_func_array",
    "ceil",
    "checkdate",
    "chdir",
    "chgrp",
    "chmod",
    "chown",
    "class_alias",
    "class_attribute_args",
    "class_attribute_names",
    "class_exists",
    "class_get_attributes",
    "class_implements",
    "class_parents",
    "class_uses",
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
    "define",
    "defined",
    "deg2rad",
    "die",
    "dirname",
    "disk_free_space",
    "disk_total_space",
    "exec",
    "exit",
    "empty",
    "enum_exists",
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
    "fgetcsv",
    "fgetc",
    "fgets",
    "floatval",
    "floor",
    "flock",
    "fmod",
    "fnmatch",
    "fopen",
    "fprintf",
    "fpassthru",
    "fputcsv",
    "fread",
    "fscanf",
    "fseek",
    "fsockopen",
    "fstat",
    "ftruncate",
    "fsync",
    "ftell",
    "fwrite",
    "function_exists",
    "getdate",
    "get_called_class",
    "get_class",
    "get_class_methods",
    "get_class_vars",
    "getcwd",
    "get_declared_classes",
    "get_declared_interfaces",
    "get_declared_traits",
    "getenv",
    "gethostbyaddr",
    "gethostbyname",
    "gethostname",
    "getprotobyname",
    "getprotobynumber",
    "get_object_vars",
    "get_parent_class",
    "get_resource_id",
    "get_resource_type",
    "getservbyname",
    "getservbyport",
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
    "inet_ntop",
    "inet_pton",
    "interface_exists",
    "intdiv",
    "intval",
    "is_a",
    "is_array",
    "is_bool",
    "is_callable",
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
    "is_subclass_of",
    "in_array",
    "isset",
    "is_writable",
    "is_writeable",
    "ip2long",
    "json_decode",
    "json_encode",
    "json_last_error",
    "json_last_error_msg",
    "json_validate",
    "iterator_apply",
    "iterator_count",
    "iterator_to_array",
    "krsort",
    "ksort",
    "lchgrp",
    "lchown",
    "lcfirst",
    "link",
    "linkinfo",
    "localtime",
    "log",
    "log10",
    "log2",
    "long2ip",
    "lstat",
    "ltrim",
    "max",
    "method_exists",
    "mb_strlen",
    "microtime",
    "md5",
    "min",
    "mkdir",
    "mktime",
    "mt_rand",
    "natcasesort",
    "natsort",
    "nl2br",
    "number_format",
    "ob_clean",
    "ob_end_clean",
    "ob_end_flush",
    "ob_flush",
    "ob_get_clean",
    "ob_get_contents",
    "ob_get_flush",
    "ob_get_length",
    "ob_get_level",
    "ob_get_status",
    "ob_implicit_flush",
    "ob_list_handlers",
    "ob_start",
    "opendir",
    "ord",
    "pathinfo",
    "pclose",
    "passthru",
    "pfsockopen",
    "pi",
    "php_uname",
    "phpversion",
    "popen",
    "pow",
    "property_exists",
    "print_r",
    "printf",
    "ptr",
    "ptr_get",
    "ptr_is_null",
    "ptr_null",
    "ptr_offset",
    "ptr_read8",
    "ptr_read16",
    "ptr_read32",
    "ptr_read_string",
    "ptr_set",
    "ptr_sizeof",
    "ptr_write8",
    "ptr_write16",
    "ptr_write32",
    "ptr_write_string",
    "putenv",
    "preg_match",
    "preg_match_all",
    "preg_replace",
    "preg_replace_callback",
    "preg_split",
    "rad2deg",
    "rand",
    "random_int",
    "range",
    "rawurldecode",
    "rawurlencode",
    "readdir",
    "readfile",
    "readline",
    "readlink",
    "realpath",
    "realpath_cache_get",
    "realpath_cache_size",
    "rename",
    "round",
    "rsort",
    "rtrim",
    "rewind",
    "rewinddir",
    "rmdir",
    "scandir",
    "sha1",
    "shell_exec",
    "settype",
    "shuffle",
    "sin",
    "sinh",
    "sleep",
    "sort",
    "sqrt",
    "sprintf",
    "spl_autoload",
    "spl_autoload_call",
    "spl_autoload_extensions",
    "spl_autoload_functions",
    "spl_autoload_register",
    "spl_autoload_unregister",
    "spl_classes",
    "spl_object_hash",
    "spl_object_id",
    "sscanf",
    "stat",
    "stream_bucket_append",
    "stream_bucket_make_writeable",
    "stream_bucket_new",
    "stream_bucket_prepend",
    "stream_context_create",
    "stream_context_get_default",
    "stream_context_get_options",
    "stream_context_get_params",
    "stream_context_set_default",
    "stream_context_set_option",
    "stream_context_set_params",
    "stream_copy_to_stream",
    "stream_filter_append",
    "stream_filter_prepend",
    "stream_filter_register",
    "stream_filter_remove",
    "stream_get_contents",
    "stream_get_filters",
    "stream_get_line",
    "stream_get_meta_data",
    "stream_get_transports",
    "stream_get_wrappers",
    "stream_is_local",
    "stream_isatty",
    "stream_select",
    "stream_socket_accept",
    "stream_socket_client",
    "stream_socket_enable_crypto",
    "stream_socket_get_name",
    "stream_socket_pair",
    "stream_socket_recvfrom",
    "stream_socket_sendto",
    "stream_socket_server",
    "stream_socket_shutdown",
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
    "stream_wrapper_register",
    "stream_wrapper_restore",
    "stream_wrapper_unregister",
    "system",
    "symlink",
    "sys_get_temp_dir",
    "tan",
    "tanh",
    "tempnam",
    "time",
    "tmpfile",
    "touch",
    "trait_exists",
    "trim",
    "uasort",
    "uksort",
    "ucfirst",
    "ucwords",
    "umask",
    "unlink",
    "unset",
    "urldecode",
    "urlencode",
    "usleep",
    "usort",
    "var_dump",
    "vfprintf",
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

/// Verifies the two registries agree on which builtins are elephc extensions:
/// the eval interpreter's extension set must equal the compiler's PHP-visible
/// extension set minus the static-only names (`zval_*`, which magician does not
/// implement). `--strict-php` relies on this agreement to hide the same names
/// on the AOT surface (catalog) and inside runtime eval (magician dispatch).
#[test]
fn extension_builtin_sets_agree_across_registries() {
    let static_only = STATIC_ONLY_REGISTRY_BUILTINS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let expected = elephc::builtin_metadata::extension_builtin_names()
        .iter()
        .copied()
        .filter(|name| !static_only.contains(name))
        .collect::<BTreeSet<_>>();
    let eval_extensions = elephc_magician::builtin_metadata::extension_builtin_names()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    assert_eq!(
        eval_extensions, expected,
        "eval extension set must match the compiler's extension set minus static-only builtins"
    );
}
