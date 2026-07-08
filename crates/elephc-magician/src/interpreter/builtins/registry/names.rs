//! Purpose:
//! Builtin existence name table used by eval function probes.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - The slice is the source of truth for PHP-visible eval builtin names.
//! - Lookup callers pass canonical lowercase PHP symbol names.

use std::sync::OnceLock;

use super::{eval_declared_builtin_exists, eval_declared_builtin_function_names};

/// PHP-visible builtin names implemented by the eval interpreter.
pub(in crate::interpreter) const EVAL_PHP_VISIBLE_BUILTIN_FUNCTIONS: &[&str] = &[
    "array_chunk",
    "array_column",
    "array_combine",
    "array_diff",
    "array_diff_key",
    "array_fill",
    "array_fill_keys",
    "array_filter",
    "array_intersect",
    "array_intersect_key",
    "array_map",
    "array_merge",
    "array_pop",
    "array_push",
    "array_reduce",
    "array_shift",
    "array_splice",
    "array_unshift",
    "array_walk",
    "arsort",
    "asort",
    "buffer_free",
    "buffer_len",
    "buffer_new",
    "class_alias",
    "class_attribute_args",
    "class_attribute_names",
    "class_exists",
    "class_get_attributes",
    "class_implements",
    "class_parents",
    "class_uses",
    "empty",
    "enum_exists",
    "fgetcsv",
    "flock",
    "fopen",
    "fprintf",
    "fputcsv",
    "fscanf",
    "fsockopen",
    "function_exists",
    "get_called_class",
    "get_class",
    "get_class_methods",
    "get_class_vars",
    "get_declared_classes",
    "get_declared_interfaces",
    "get_declared_traits",
    "get_object_vars",
    "get_parent_class",
    "get_resource_id",
    "get_resource_type",
    "interface_exists",
    "is_a",
    "is_callable",
    "is_subclass_of",
    "isset",
    "iterator_apply",
    "iterator_count",
    "iterator_to_array",
    "krsort",
    "ksort",
    "method_exists",
    "natcasesort",
    "natsort",
    "pfsockopen",
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
    "print_r",
    "property_exists",
    "readline",
    "rsort",
    "shuffle",
    "sort",
    "spl_autoload",
    "spl_autoload_call",
    "spl_autoload_extensions",
    "spl_autoload_functions",
    "spl_autoload_register",
    "spl_autoload_unregister",
    "spl_classes",
    "spl_object_hash",
    "spl_object_id",
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
    "stream_filter_append",
    "stream_filter_prepend",
    "stream_filter_register",
    "stream_filter_remove",
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
    "stream_wrapper_register",
    "stream_wrapper_restore",
    "stream_wrapper_unregister",
    "trait_exists",
    "uasort",
    "uksort",
    "unset",
    "usort",
    "var_dump",
    "vfprintf",
];

/// Combined PHP-visible builtin names from legacy and declarative registries.
static EVAL_PHP_VISIBLE_BUILTIN_FUNCTION_NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();

/// Returns the eval interpreter's PHP-visible builtin names.
pub(in crate::interpreter) fn eval_php_visible_builtin_function_names() -> &'static [&'static str] {
    EVAL_PHP_VISIBLE_BUILTIN_FUNCTION_NAMES
        .get_or_init(|| {
            let mut names = EVAL_PHP_VISIBLE_BUILTIN_FUNCTIONS.to_vec();
            for name in eval_declared_builtin_function_names() {
                if !names.contains(name) {
                    names.push(name);
                }
            }
            names.sort_unstable();
            names
        })
        .as_slice()
}

/// Returns true for PHP-visible builtin names implemented by the eval interpreter.
pub(in crate::interpreter) fn eval_php_visible_builtin_exists(name: &str) -> bool {
    eval_declared_builtin_exists(name) || EVAL_PHP_VISIBLE_BUILTIN_FUNCTIONS.contains(&name)
}
