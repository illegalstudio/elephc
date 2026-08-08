//! Purpose:
//! Builds the `--web` prelude's declarations as AST — the SAPI surface a `--web` binary gets
//! before its own statements: the superglobals, the session engine and its handler interfaces,
//! the header/cookie API, and the `ini_*` dispatchers. It replaces the PHP source this module
//! used to splice strings into.
//!
//! Called from:
//! - `crate::web_prelude::inject_if_web`, which prunes what the program cannot reach and wraps
//!   the result in the catch-all `try`.
//!
//! Key details:
//! - Each helper below is one declaration of the former `WEB_PRELUDE_SRC`, transcribed from its
//!   own parse and checked against it node for node before the text was deleted. The order in
//!   `web_declarations` is the order the source had, which matters for the executable BOOTSTRAP
//!   statements interleaved with the declarations (they populate the superglobals and may start
//!   the session, and they run where they sit).
//! - THE THREE `__PLACEHOLDER__` SLOTS ARE NOW PARAMETERS. `__ELEPHC_PHP_VERSION_ID__` is an
//!   argument to `decl_fn_elephc_php_version_id`; `__ELEPHC_OPCACHE_INI_HELPERS__` and
//!   `__ELEPHC_INI_MODULE_KNOWN__` are calls into `crate::opcache_prelude`, which returns
//!   declarations rather than text. That is the whole reason web and opcache had to be converted
//!   together: web spliced PHP that opcache GENERATED, so neither could stop producing text
//!   while the other still consumed it.
//! - A `--web` binary OWNS the `ELEPHC_INI_*` environment block and the `opcache.*` INI helpers;
//!   `opcache_prelude::inject_if_used` deliberately does not emit them when `web` is true,
//!   because two copies would be a redeclaration.

use crate::opcache_prelude;
use crate::parser::ast::{BinOp, CType, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{class, e_array, e_array_assoc, e_binop, e_bool, e_call, e_cast,
    e_const, e_index, e_instance_of, e_int, e_method_call, e_neg, e_new, e_not, e_null,
    e_post_inc, e_static_prop, e_str, e_ternary, e_this, e_this_prop, e_var, extern_fn,
    function, interface, internal_declarations, method, s_array_assign, s_assign, s_continue,
    s_do_while, s_expr, s_for, s_foreach, s_if, s_prop_assign, s_return, s_return_void,
    s_static_prop_assign, s_throw, t_array, t_class, t_mixed, t_nullable, t_ptr, t_union
};
use crate::web_prelude::PhpVersion;

/// `elephc_web_method` — transcribed from the PHP form.
fn decl_extern_elephc_web_method() -> Stmt {
    extern_fn("elephc_web_method", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_uri` — transcribed from the PHP form.
fn decl_extern_elephc_web_uri() -> Stmt {
    extern_fn("elephc_web_uri", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_path` — transcribed from the PHP form.
fn decl_extern_elephc_web_path() -> Stmt {
    extern_fn("elephc_web_path", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_query_string` — transcribed from the PHP form.
fn decl_extern_elephc_web_query_string() -> Stmt {
    extern_fn("elephc_web_query_string", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_header_count` — transcribed from the PHP form.
fn decl_extern_elephc_web_header_count() -> Stmt {
    extern_fn("elephc_web_header_count", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_header_name` — transcribed from the PHP form.
fn decl_extern_elephc_web_header_name() -> Stmt {
    extern_fn("elephc_web_header_name", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_web_header_value` — transcribed from the PHP form.
fn decl_extern_elephc_web_header_value() -> Stmt {
    extern_fn("elephc_web_header_value", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_web_body_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_web_body_ptr() -> Stmt {
    extern_fn("elephc_web_body_ptr", "elephc_web")
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_body_len` — transcribed from the PHP form.
fn decl_extern_elephc_web_body_len() -> Stmt {
    extern_fn("elephc_web_body_len", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_remote_addr` — transcribed from the PHP form.
fn decl_extern_elephc_web_remote_addr() -> Stmt {
    extern_fn("elephc_web_remote_addr", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_remote_port` — transcribed from the PHP form.
fn decl_extern_elephc_web_remote_port() -> Stmt {
    extern_fn("elephc_web_remote_port", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_server_addr` — transcribed from the PHP form.
fn decl_extern_elephc_web_server_addr() -> Stmt {
    extern_fn("elephc_web_server_addr", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_server_port` — transcribed from the PHP form.
fn decl_extern_elephc_web_server_port() -> Stmt {
    extern_fn("elephc_web_server_port", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_protocol` — transcribed from the PHP form.
fn decl_extern_elephc_web_protocol() -> Stmt {
    extern_fn("elephc_web_protocol", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_request_time` — transcribed from the PHP form.
fn decl_extern_elephc_web_request_time() -> Stmt {
    extern_fn("elephc_web_request_time", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_env_count` — transcribed from the PHP form.
fn decl_extern_elephc_web_env_count() -> Stmt {
    extern_fn("elephc_web_env_count", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_env_name` — transcribed from the PHP form.
fn decl_extern_elephc_web_env_name() -> Stmt {
    extern_fn("elephc_web_env_name", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_web_env_value` — transcribed from the PHP form.
fn decl_extern_elephc_web_env_value() -> Stmt {
    extern_fn("elephc_web_env_value", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_web_multipart_count` — transcribed from the PHP form.
fn decl_extern_elephc_web_multipart_count() -> Stmt {
    extern_fn("elephc_web_multipart_count", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_multipart_name` — transcribed from the PHP form.
fn decl_extern_elephc_web_multipart_name() -> Stmt {
    extern_fn("elephc_web_multipart_name", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_web_multipart_filename` — transcribed from the PHP form.
fn decl_extern_elephc_web_multipart_filename() -> Stmt {
    extern_fn("elephc_web_multipart_filename", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_web_multipart_type` — transcribed from the PHP form.
fn decl_extern_elephc_web_multipart_type() -> Stmt {
    extern_fn("elephc_web_multipart_type", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_web_multipart_value_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_web_multipart_value_ptr() -> Stmt {
    extern_fn("elephc_web_multipart_value_ptr", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_multipart_value_len` — transcribed from the PHP form.
fn decl_extern_elephc_web_multipart_value_len() -> Stmt {
    extern_fn("elephc_web_multipart_value_len", "elephc_web")
        .param("i", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_reset` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_reset() -> Stmt {
    extern_fn("elephc_web_session_reset", "elephc_web")
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_name` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_name() -> Stmt {
    extern_fn("elephc_web_session_get_name", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_name` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_name() -> Stmt {
    extern_fn("elephc_web_session_set_name", "elephc_web")
        .param("name", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_id` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_id() -> Stmt {
    extern_fn("elephc_web_session_get_id", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_id` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_id() -> Stmt {
    extern_fn("elephc_web_session_set_id", "elephc_web")
        .param("id", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_get_status` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_status() -> Stmt {
    extern_fn("elephc_web_session_get_status", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_status` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_status() -> Stmt {
    extern_fn("elephc_web_session_set_status", "elephc_web")
        .param("status", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_save_path` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_save_path() -> Stmt {
    extern_fn("elephc_web_session_get_save_path", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_save_path` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_save_path() -> Stmt {
    extern_fn("elephc_web_session_set_save_path", "elephc_web")
        .param("path", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_cache_limiter` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cache_limiter() -> Stmt {
    extern_fn("elephc_web_session_get_cache_limiter", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_cache_limiter` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_cache_limiter() -> Stmt {
    extern_fn("elephc_web_session_set_cache_limiter", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_cache_expire` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cache_expire() -> Stmt {
    extern_fn("elephc_web_session_get_cache_expire", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_cache_expire` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_cache_expire() -> Stmt {
    extern_fn("elephc_web_session_set_cache_expire", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_cookie_lifetime` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cookie_lifetime() -> Stmt {
    extern_fn("elephc_web_session_get_cookie_lifetime", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_get_cookie_path` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cookie_path() -> Stmt {
    extern_fn("elephc_web_session_get_cookie_path", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_get_cookie_domain` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cookie_domain() -> Stmt {
    extern_fn("elephc_web_session_get_cookie_domain", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_get_cookie_secure` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cookie_secure() -> Stmt {
    extern_fn("elephc_web_session_get_cookie_secure", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_get_cookie_partitioned` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cookie_partitioned() -> Stmt {
    extern_fn("elephc_web_session_get_cookie_partitioned", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_get_cookie_httponly` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cookie_httponly() -> Stmt {
    extern_fn("elephc_web_session_get_cookie_httponly", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_get_cookie_samesite` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_cookie_samesite() -> Stmt {
    extern_fn("elephc_web_session_get_cookie_samesite", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_cookie_params` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_cookie_params() -> Stmt {
    extern_fn("elephc_web_session_set_cookie_params", "elephc_web")
        .param("lifetime", CType::Int)
        .param("path", CType::Str)
        .param("domain", CType::Str)
        .param("secure", CType::Int)
        .param("partitioned", CType::Int)
        .param("httponly", CType::Int)
        .param("samesite", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_data_stage` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_data_stage() -> Stmt {
    extern_fn("elephc_web_session_data_stage", "elephc_web")
        .param("len", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_session_data_len` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_data_len() -> Stmt {
    extern_fn("elephc_web_session_data_len", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_read_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_read_bytes() -> Stmt {
    extern_fn("elephc_web_session_read_bytes", "elephc_web")
        .param("id", CType::Str)
        .param("save_path", CType::Str)
        .param("read_and_close", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_session_last_read_ok` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_last_read_ok() -> Stmt {
    extern_fn("elephc_web_session_last_read_ok", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_write_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_write_bytes() -> Stmt {
    extern_fn("elephc_web_session_write_bytes", "elephc_web")
        .param("id", CType::Str)
        .param("save_path", CType::Str)
        .param("data", CType::Ptr)
        .param("data_len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_destroy` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_destroy() -> Stmt {
    extern_fn("elephc_web_session_destroy", "elephc_web")
        .param("id", CType::Str)
        .param("save_path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_abort` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_abort() -> Stmt {
    extern_fn("elephc_web_session_abort", "elephc_web")
        .param("id", CType::Str)
        .param("save_path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_create_id` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_create_id() -> Stmt {
    extern_fn("elephc_web_session_create_id", "elephc_web")
        .param("prefix", CType::Str)
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_gc` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_gc() -> Stmt {
    extern_fn("elephc_web_session_gc", "elephc_web")
        .param("save_path", CType::Str)
        .param("maxlifetime", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_count_entries_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_count_entries_bytes() -> Stmt {
    extern_fn("elephc_web_session_count_entries_bytes", "elephc_web")
        .param("data", CType::Ptr)
        .param("data_len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_entry_key_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_entry_key_bytes() -> Stmt {
    extern_fn("elephc_web_session_entry_key_bytes", "elephc_web")
        .param("data", CType::Ptr)
        .param("data_len", CType::Int)
        .param("idx", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_session_entry_value_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_entry_value_bytes() -> Stmt {
    extern_fn("elephc_web_session_entry_value_bytes", "elephc_web")
        .param("data", CType::Ptr)
        .param("data_len", CType::Int)
        .param("idx", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_session_snapshot_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_snapshot_bytes() -> Stmt {
    extern_fn("elephc_web_session_snapshot_bytes", "elephc_web")
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_session_file_exists` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_file_exists() -> Stmt {
    extern_fn("elephc_web_session_file_exists", "elephc_web")
        .param("id", CType::Str)
        .param("save_path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_touch` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_touch() -> Stmt {
    extern_fn("elephc_web_session_touch", "elephc_web")
        .param("id", CType::Str)
        .param("save_path", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_should_gc` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_should_gc() -> Stmt {
    extern_fn("elephc_web_session_should_gc", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_get_strict_mode` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_strict_mode() -> Stmt {
    extern_fn("elephc_web_session_get_strict_mode", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_strict_mode` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_strict_mode() -> Stmt {
    extern_fn("elephc_web_session_set_strict_mode", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_serialize_handler` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_serialize_handler() -> Stmt {
    extern_fn("elephc_web_session_get_serialize_handler", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_serialize_handler` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_serialize_handler() -> Stmt {
    extern_fn("elephc_web_session_set_serialize_handler", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_gc_probability` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_gc_probability() -> Stmt {
    extern_fn("elephc_web_session_get_gc_probability", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_gc_probability` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_gc_probability() -> Stmt {
    extern_fn("elephc_web_session_set_gc_probability", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_gc_divisor` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_gc_divisor() -> Stmt {
    extern_fn("elephc_web_session_get_gc_divisor", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_gc_divisor` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_gc_divisor() -> Stmt {
    extern_fn("elephc_web_session_set_gc_divisor", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_gc_maxlifetime` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_gc_maxlifetime() -> Stmt {
    extern_fn("elephc_web_session_get_gc_maxlifetime", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_gc_maxlifetime` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_gc_maxlifetime() -> Stmt {
    extern_fn("elephc_web_session_set_gc_maxlifetime", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_sid_length` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_sid_length() -> Stmt {
    extern_fn("elephc_web_session_get_sid_length", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_sid_length` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_sid_length() -> Stmt {
    extern_fn("elephc_web_session_set_sid_length", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_get_sid_bits_per_character` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_sid_bits_per_character() -> Stmt {
    extern_fn("elephc_web_session_get_sid_bits_per_character", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_sid_bits_per_character` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_sid_bits_per_character() -> Stmt {
    extern_fn("elephc_web_session_set_sid_bits_per_character", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_count_entries_bin_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_count_entries_bin_bytes() -> Stmt {
    extern_fn("elephc_web_session_count_entries_bin_bytes", "elephc_web")
        .param("data", CType::Ptr)
        .param("data_len", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_entry_key_bin_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_entry_key_bin_bytes() -> Stmt {
    extern_fn("elephc_web_session_entry_key_bin_bytes", "elephc_web")
        .param("data", CType::Ptr)
        .param("data_len", CType::Int)
        .param("idx", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_session_entry_value_bin_bytes` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_entry_value_bin_bytes() -> Stmt {
    extern_fn("elephc_web_session_entry_value_bin_bytes", "elephc_web")
        .param("data", CType::Ptr)
        .param("data_len", CType::Int)
        .param("idx", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_web_session_get_referer_check` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_referer_check() -> Stmt {
    extern_fn("elephc_web_session_get_referer_check", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_referer_check` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_referer_check() -> Stmt {
    extern_fn("elephc_web_session_set_referer_check", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_use_only_cookies` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_use_only_cookies() -> Stmt {
    extern_fn("elephc_web_session_get_use_only_cookies", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_use_only_cookies` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_use_only_cookies() -> Stmt {
    extern_fn("elephc_web_session_set_use_only_cookies", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_use_cookies` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_use_cookies() -> Stmt {
    extern_fn("elephc_web_session_get_use_cookies", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_use_cookies` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_use_cookies() -> Stmt {
    extern_fn("elephc_web_session_set_use_cookies", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_lazy_write` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_lazy_write() -> Stmt {
    extern_fn("elephc_web_session_get_lazy_write", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_lazy_write` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_lazy_write() -> Stmt {
    extern_fn("elephc_web_session_set_lazy_write", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_use_trans_sid` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_use_trans_sid() -> Stmt {
    extern_fn("elephc_web_session_get_use_trans_sid", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_use_trans_sid` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_use_trans_sid() -> Stmt {
    extern_fn("elephc_web_session_set_use_trans_sid", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_trans_sid_tags` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_trans_sid_tags() -> Stmt {
    extern_fn("elephc_web_session_get_trans_sid_tags", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_trans_sid_tags` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_trans_sid_tags() -> Stmt {
    extern_fn("elephc_web_session_set_trans_sid_tags", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_trans_sid_hosts` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_trans_sid_hosts() -> Stmt {
    extern_fn("elephc_web_session_get_trans_sid_hosts", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_trans_sid_hosts` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_trans_sid_hosts() -> Stmt {
    extern_fn("elephc_web_session_set_trans_sid_hosts", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_upload_progress_enabled` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_upload_progress_enabled() -> Stmt {
    extern_fn("elephc_web_session_get_upload_progress_enabled", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_upload_progress_enabled` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_upload_progress_enabled() -> Stmt {
    extern_fn("elephc_web_session_set_upload_progress_enabled", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_upload_progress_cleanup` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_upload_progress_cleanup() -> Stmt {
    extern_fn("elephc_web_session_get_upload_progress_cleanup", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_upload_progress_cleanup` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_upload_progress_cleanup() -> Stmt {
    extern_fn("elephc_web_session_set_upload_progress_cleanup", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_upload_progress_prefix` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_upload_progress_prefix() -> Stmt {
    extern_fn("elephc_web_session_get_upload_progress_prefix", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_upload_progress_prefix` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_upload_progress_prefix() -> Stmt {
    extern_fn("elephc_web_session_set_upload_progress_prefix", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_upload_progress_name` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_upload_progress_name() -> Stmt {
    extern_fn("elephc_web_session_get_upload_progress_name", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_upload_progress_name` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_upload_progress_name() -> Stmt {
    extern_fn("elephc_web_session_set_upload_progress_name", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_upload_progress_freq` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_upload_progress_freq() -> Stmt {
    extern_fn("elephc_web_session_get_upload_progress_freq", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_upload_progress_freq` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_upload_progress_freq() -> Stmt {
    extern_fn("elephc_web_session_set_upload_progress_freq", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_upload_progress_min_freq` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_upload_progress_min_freq() -> Stmt {
    extern_fn("elephc_web_session_get_upload_progress_min_freq", "elephc_web")
        .returns(CType::Str)
        .build()
}

/// `elephc_web_session_set_upload_progress_min_freq` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_upload_progress_min_freq() -> Stmt {
    extern_fn("elephc_web_session_set_upload_progress_min_freq", "elephc_web")
        .param("v", CType::Str)
        .returns(CType::Void)
        .build()
}

/// `elephc_web_session_get_auto_start` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_get_auto_start() -> Stmt {
    extern_fn("elephc_web_session_get_auto_start", "elephc_web")
        .returns(CType::Int)
        .build()
}

/// `elephc_web_session_set_auto_start` — transcribed from the PHP form.
fn decl_extern_elephc_web_session_set_auto_start() -> Stmt {
    extern_fn("elephc_web_session_set_auto_start", "elephc_web")
        .param("v", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `__elephc_php_version_id` — transcribed from the PHP form.
fn decl_fn_elephc_php_version_id(version_id: u32) -> Stmt {
    function("__elephc_php_version_id")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_int(i64::from(version_id))),
        ])
        .build()
}

/// `bootstrap 1` — transcribed from the PHP form.
fn decl_stmt_bootstrap_1() -> Stmt {
    s_expr(e_call("elephc_web_session_reset", vec![]))
}

/// `bootstrap 2` — transcribed from the PHP form.
fn decl_stmt_bootstrap_2() -> Stmt {
    s_assign("_SERVER", e_array(vec![]))
}

/// `bootstrap 3` — transcribed from the PHP form.
fn decl_stmt_bootstrap_3() -> Stmt {
    s_assign("_SESSION", e_array(vec![]))
}

/// `bootstrap 4` — transcribed from the PHP form.
fn decl_stmt_bootstrap_4() -> Stmt {
    s_array_assign("_SERVER", e_str("REQUEST_METHOD"), e_call("elephc_web_method", vec![]))
}

/// `bootstrap 5` — transcribed from the PHP form.
fn decl_stmt_bootstrap_5() -> Stmt {
    s_array_assign("_SERVER", e_str("REQUEST_URI"), e_call("elephc_web_uri", vec![]))
}

/// `bootstrap 6` — transcribed from the PHP form.
fn decl_stmt_bootstrap_6() -> Stmt {
    s_array_assign("_SERVER", e_str("QUERY_STRING"), e_call("elephc_web_query_string", vec![]))
}

/// `bootstrap 7` — transcribed from the PHP form.
fn decl_stmt_bootstrap_7() -> Stmt {
    s_assign("__elephc_hc", e_call("elephc_web_header_count", vec![]))
}

/// `bootstrap 8` — transcribed from the PHP form.
fn decl_stmt_bootstrap_8() -> Stmt {
    s_for(Some(s_assign("__elephc_i", e_int(0))), Some(e_binop(e_var("__elephc_i"), BinOp::Lt, e_var("__elephc_hc"))), Some(s_expr(e_post_inc("__elephc_i"))), vec![
        s_assign("__elephc_hn", e_call("elephc_web_header_name", vec![e_var("__elephc_i")])),
        s_assign("__elephc_hv", e_call("elephc_web_header_value", vec![e_var("__elephc_i")])),
        s_array_assign("_SERVER", e_binop(e_str("HTTP_"), BinOp::Concat, e_call("strtoupper", vec![e_call("str_replace", vec![e_str("-"), e_str("_"), e_var("__elephc_hn")])])), e_var("__elephc_hv")),
        s_assign("__elephc_up", e_call("strtoupper", vec![e_var("__elephc_hn")])),
        s_if(
            e_binop(e_var("__elephc_up"), BinOp::StrictEq, e_str("CONTENT-TYPE")),
            vec![
                s_array_assign("_SERVER", e_str("CONTENT_TYPE"), e_var("__elephc_hv")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("__elephc_up"), BinOp::StrictEq, e_str("CONTENT-LENGTH")),
            vec![
                s_array_assign("_SERVER", e_str("CONTENT_LENGTH"), e_var("__elephc_hv")),
            ],
            vec![],
            None,
        ),
    ])
}

/// `bootstrap 9` — transcribed from the PHP form.
fn decl_stmt_bootstrap_9() -> Stmt {
    s_array_assign("_SERVER", e_str("REMOTE_ADDR"), e_call("elephc_web_remote_addr", vec![]))
}

/// `bootstrap 10` — transcribed from the PHP form.
fn decl_stmt_bootstrap_10() -> Stmt {
    s_array_assign("_SERVER", e_str("REMOTE_PORT"), e_call("elephc_web_remote_port", vec![]))
}

/// `bootstrap 11` — transcribed from the PHP form.
fn decl_stmt_bootstrap_11() -> Stmt {
    s_array_assign("_SERVER", e_str("SERVER_ADDR"), e_call("elephc_web_server_addr", vec![]))
}

/// `bootstrap 12` — transcribed from the PHP form.
fn decl_stmt_bootstrap_12() -> Stmt {
    s_array_assign("_SERVER", e_str("SERVER_PORT"), e_call("elephc_web_server_port", vec![]))
}

/// `bootstrap 13` — transcribed from the PHP form.
fn decl_stmt_bootstrap_13() -> Stmt {
    s_array_assign("_SERVER", e_str("SERVER_NAME"), e_call("elephc_web_server_addr", vec![]))
}

/// `bootstrap 14` — transcribed from the PHP form.
fn decl_stmt_bootstrap_14() -> Stmt {
    s_array_assign("_SERVER", e_str("SERVER_PROTOCOL"), e_call("elephc_web_protocol", vec![]))
}

/// `bootstrap 15` — transcribed from the PHP form.
fn decl_stmt_bootstrap_15() -> Stmt {
    s_array_assign("_SERVER", e_str("REQUEST_TIME"), e_call("elephc_web_request_time", vec![]))
}

/// `bootstrap 16` — transcribed from the PHP form.
fn decl_stmt_bootstrap_16() -> Stmt {
    s_array_assign("_SERVER", e_str("REQUEST_SCHEME"), e_str("http"))
}

/// `bootstrap 17` — transcribed from the PHP form.
fn decl_stmt_bootstrap_17() -> Stmt {
    s_array_assign("_SERVER", e_str("GATEWAY_INTERFACE"), e_str("CGI/1.1"))
}

/// `bootstrap 18` — transcribed from the PHP form.
fn decl_stmt_bootstrap_18() -> Stmt {
    s_array_assign("_SERVER", e_str("SERVER_SOFTWARE"), e_str("elephc"))
}

/// `bootstrap 19` — transcribed from the PHP form.
fn decl_stmt_bootstrap_19() -> Stmt {
    s_assign("_GET", e_array(vec![]))
}

/// `bootstrap 20` — transcribed from the PHP form.
fn decl_stmt_bootstrap_20() -> Stmt {
    s_assign("__elephc_qs", e_call("elephc_web_query_string", vec![]))
}

/// `bootstrap 21` — transcribed from the PHP form.
fn decl_stmt_bootstrap_21() -> Stmt {
    s_if(
        e_binop(e_var("__elephc_qs"), BinOp::StrictNotEq, e_str("")),
        vec![
            s_assign("__elephc_pairs", e_call("explode", vec![e_str("&"), e_var("__elephc_qs")])),
            s_foreach(e_var("__elephc_pairs"), None, "__elephc_pair", vec![
                s_assign("__elephc_eq", e_call("strpos", vec![e_var("__elephc_pair"), e_str("=")])),
                s_if(
                    e_binop(e_var("__elephc_eq"), BinOp::StrictEq, e_bool(false)),
                    vec![
                        s_if(
                            e_binop(e_var("__elephc_pair"), BinOp::StrictNotEq, e_str("")),
                            vec![
                                s_array_assign("_GET", e_call("rawurldecode", vec![e_var("__elephc_pair")]), e_str("")),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("__elephc_gk", e_call("rawurldecode", vec![e_call("substr", vec![e_var("__elephc_pair"), e_int(0), e_var("__elephc_eq")])])),
                    s_assign("__elephc_gv", e_call("rawurldecode", vec![e_call("substr", vec![e_var("__elephc_pair"), e_binop(e_var("__elephc_eq"), BinOp::Add, e_int(1))])])),
                    s_array_assign("_GET", e_var("__elephc_gk"), e_var("__elephc_gv")),
                ]),
                ),
            ]),
        ],
        vec![],
        None,
    )
}

/// `bootstrap 22` — transcribed from the PHP form.
fn decl_stmt_bootstrap_22() -> Stmt {
    s_assign("_POST", e_array(vec![]))
}

/// `bootstrap 23` — transcribed from the PHP form.
fn decl_stmt_bootstrap_23() -> Stmt {
    s_assign("__elephc_ct", e_ternary(e_call("isset", vec![e_index(e_var("_SERVER"), e_str("CONTENT_TYPE"))]), e_index(e_var("_SERVER"), e_str("CONTENT_TYPE")), e_str("")))
}

/// `bootstrap 24` — transcribed from the PHP form.
fn decl_stmt_bootstrap_24() -> Stmt {
    s_if(
        e_binop(e_call("strpos", vec![e_call("strtoupper", vec![e_var("__elephc_ct")]), e_str("APPLICATION/X-WWW-FORM-URLENCODED")]), BinOp::StrictNotEq, e_bool(false)),
        vec![
            s_assign("__elephc_body_len", e_call("elephc_web_body_len", vec![])),
            s_assign("__elephc_body", e_str("")),
            s_if(
                e_binop(e_var("__elephc_body_len"), BinOp::Gt, e_int(0)),
                vec![
                    s_assign("__elephc_body", e_call("__elephc_ptr_read_string", vec![e_call("elephc_web_body_ptr", vec![]), e_var("__elephc_body_len")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_body"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_assign("__elephc_ppairs", e_call("explode", vec![e_str("&"), e_var("__elephc_body")])),
                    s_foreach(e_var("__elephc_ppairs"), None, "__elephc_ppair", vec![
                        s_assign("__elephc_peq", e_call("strpos", vec![e_var("__elephc_ppair"), e_str("=")])),
                        s_if(
                            e_binop(e_var("__elephc_peq"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_if(
                                    e_binop(e_var("__elephc_ppair"), BinOp::StrictNotEq, e_str("")),
                                    vec![
                                        s_array_assign("_POST", e_call("rawurldecode", vec![e_var("__elephc_ppair")]), e_str("")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("__elephc_pk", e_call("rawurldecode", vec![e_call("substr", vec![e_var("__elephc_ppair"), e_int(0), e_var("__elephc_peq")])])),
                            s_assign("__elephc_pv", e_call("rawurldecode", vec![e_call("substr", vec![e_var("__elephc_ppair"), e_binop(e_var("__elephc_peq"), BinOp::Add, e_int(1))])])),
                            s_array_assign("_POST", e_var("__elephc_pk"), e_var("__elephc_pv")),
                        ]),
                        ),
                    ]),
                ],
                vec![],
                None,
            ),
        ],
        vec![],
        None,
    )
}

/// `bootstrap 25` — transcribed from the PHP form.
fn decl_stmt_bootstrap_25() -> Stmt {
    s_assign("_FILES", e_array(vec![]))
}

/// `bootstrap 26` — transcribed from the PHP form.
fn decl_stmt_bootstrap_26() -> Stmt {
    s_if(
        e_binop(e_call("strpos", vec![e_call("strtoupper", vec![e_var("__elephc_ct")]), e_str("MULTIPART/FORM-DATA")]), BinOp::StrictNotEq, e_bool(false)),
        vec![
            s_assign("__elephc_mpc", e_call("elephc_web_multipart_count", vec![])),
            s_for(Some(s_assign("__elephc_mpi", e_int(0))), Some(e_binop(e_var("__elephc_mpi"), BinOp::Lt, e_var("__elephc_mpc"))), Some(s_expr(e_post_inc("__elephc_mpi"))), vec![
                s_assign("__elephc_mpn", e_call("elephc_web_multipart_name", vec![e_var("__elephc_mpi")])),
                s_assign("__elephc_mpf", e_call("elephc_web_multipart_filename", vec![e_var("__elephc_mpi")])),
                s_assign("__elephc_mpv_len", e_call("elephc_web_multipart_value_len", vec![e_var("__elephc_mpi")])),
                s_assign("__elephc_mpv", e_str("")),
                s_if(
                    e_binop(e_var("__elephc_mpv_len"), BinOp::Gt, e_int(0)),
                    vec![
                        s_assign("__elephc_mpv", e_call("__elephc_ptr_read_string", vec![e_call("elephc_web_multipart_value_ptr", vec![e_var("__elephc_mpi")]), e_var("__elephc_mpv_len")])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_mpf"), BinOp::StrictEq, e_str("")),
                    vec![
                        s_array_assign("_POST", e_var("__elephc_mpn"), e_var("__elephc_mpv")),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("__elephc_mptmp", e_call("tempnam", vec![e_call("sys_get_temp_dir", vec![]), e_str("elephc_up")])),
                    s_if(
                        e_binop(e_var("__elephc_mptmp"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_expr(e_call("file_put_contents", vec![e_var("__elephc_mptmp"), e_var("__elephc_mpv")])),
                            s_array_assign("_FILES", e_var("__elephc_mpn"), e_array_assoc(vec![(e_str("name"), e_var("__elephc_mpf")), (e_str("type"), e_call("elephc_web_multipart_type", vec![e_var("__elephc_mpi")])), (e_str("tmp_name"), e_var("__elephc_mptmp")), (e_str("error"), e_int(0)), (e_str("size"), e_call("strlen", vec![e_var("__elephc_mpv")]))])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                ),
            ]),
        ],
        vec![],
        None,
    )
}

/// `bootstrap 27` — transcribed from the PHP form.
fn decl_stmt_bootstrap_27() -> Stmt {
    s_assign("_COOKIE", e_array(vec![]))
}

/// `bootstrap 28` — transcribed from the PHP form.
fn decl_stmt_bootstrap_28() -> Stmt {
    s_assign("__elephc_ck", e_ternary(e_call("isset", vec![e_index(e_var("_SERVER"), e_str("HTTP_COOKIE"))]), e_index(e_var("_SERVER"), e_str("HTTP_COOKIE")), e_str("")))
}

/// `bootstrap 29` — transcribed from the PHP form.
fn decl_stmt_bootstrap_29() -> Stmt {
    s_if(
        e_binop(e_var("__elephc_ck"), BinOp::StrictNotEq, e_str("")),
        vec![
            s_assign("__elephc_cpairs", e_call("explode", vec![e_str(";"), e_var("__elephc_ck")])),
            s_foreach(e_var("__elephc_cpairs"), None, "__elephc_cpair", vec![
                s_assign("__elephc_ceq", e_call("strpos", vec![e_var("__elephc_cpair"), e_str("=")])),
                s_if(
                    e_binop(e_var("__elephc_ceq"), BinOp::StrictNotEq, e_bool(false)),
                    vec![
                        s_assign("__elephc_cknm", e_call("trim", vec![e_call("substr", vec![e_var("__elephc_cpair"), e_int(0), e_var("__elephc_ceq")])])),
                        s_assign("__elephc_cv", e_call("rawurldecode", vec![e_call("trim", vec![e_call("substr", vec![e_var("__elephc_cpair"), e_binop(e_var("__elephc_ceq"), BinOp::Add, e_int(1))])])])),
                        s_if(
                            e_binop(e_var("__elephc_cknm"), BinOp::StrictNotEq, e_str("")),
                            vec![
                                s_array_assign("_COOKIE", e_var("__elephc_cknm"), e_var("__elephc_cv")),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
            ]),
        ],
        vec![],
        None,
    )
}

/// `bootstrap 30` — transcribed from the PHP form.
fn decl_stmt_bootstrap_30() -> Stmt {
    s_assign("_REQUEST", e_array(vec![]))
}

/// `bootstrap 31` — transcribed from the PHP form.
fn decl_stmt_bootstrap_31() -> Stmt {
    s_foreach(e_var("_GET"), Some("__elephc_rqk"), "__elephc_rqv", vec![
        s_array_assign("_REQUEST", e_var("__elephc_rqk"), e_var("__elephc_rqv")),
    ])
}

/// `bootstrap 32` — transcribed from the PHP form.
fn decl_stmt_bootstrap_32() -> Stmt {
    s_foreach(e_var("_POST"), Some("__elephc_rqk"), "__elephc_rqv", vec![
        s_array_assign("_REQUEST", e_var("__elephc_rqk"), e_var("__elephc_rqv")),
    ])
}

/// `bootstrap 33` — transcribed from the PHP form.
fn decl_stmt_bootstrap_33() -> Stmt {
    s_assign("_ENV", e_array(vec![]))
}

/// `bootstrap 34` — transcribed from the PHP form.
fn decl_stmt_bootstrap_34() -> Stmt {
    s_assign("__elephc_envc", e_call("elephc_web_env_count", vec![]))
}

/// `bootstrap 35` — transcribed from the PHP form.
fn decl_stmt_bootstrap_35() -> Stmt {
    s_for(Some(s_assign("__elephc_envi", e_int(0))), Some(e_binop(e_var("__elephc_envi"), BinOp::Lt, e_var("__elephc_envc"))), Some(s_expr(e_post_inc("__elephc_envi"))), vec![
        s_array_assign("_ENV", e_call("elephc_web_env_name", vec![e_var("__elephc_envi")]), e_call("elephc_web_env_value", vec![e_var("__elephc_envi")])),
    ])
}

/// `__elephc_emit_cookie` — transcribed from the PHP form.
fn decl_fn_elephc_emit_cookie() -> Stmt {
    function("__elephc_emit_cookie")
        .param_untyped("name")
        .param_untyped("value")
        .param_untyped("expires")
        .param_untyped("path")
        .param_untyped("domain")
        .param_untyped("secure")
        .param_untyped("httponly")
        .body(vec![
            s_assign("c", e_binop(e_binop(e_var("name"), BinOp::Concat, e_str("=")), BinOp::Concat, e_var("value"))),
            s_if(
                e_binop(e_var("expires"), BinOp::NotEq, e_int(0)),
                vec![
                    s_assign("c", e_binop(e_binop(e_binop(e_var("c"), BinOp::Concat, e_str("; expires=")), BinOp::Concat, e_call("gmdate", vec![e_str("D, d-M-Y H:i:s"), e_var("expires")])), BinOp::Concat, e_str(" GMT"))),
                    s_assign("c", e_binop(e_binop(e_var("c"), BinOp::Concat, e_str("; Max-Age=")), BinOp::Concat, e_binop(e_var("expires"), BinOp::Sub, e_call("time", vec![])))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("path"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_assign("c", e_binop(e_binop(e_var("c"), BinOp::Concat, e_str("; path=")), BinOp::Concat, e_var("path"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("domain"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_assign("c", e_binop(e_binop(e_var("c"), BinOp::Concat, e_str("; domain=")), BinOp::Concat, e_var("domain"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("secure"),
                vec![
                    s_assign("c", e_binop(e_var("c"), BinOp::Concat, e_str("; secure"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("httponly"),
                vec![
                    s_assign("c", e_binop(e_var("c"), BinOp::Concat, e_str("; HttpOnly"))),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("header", vec![e_binop(e_str("Set-Cookie: "), BinOp::Concat, e_var("c")), e_bool(false)])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `setcookie` — transcribed from the PHP form.
fn decl_fn_setcookie() -> Stmt {
    function("setcookie")
        .param_untyped("name")
        .param_untyped_default("value", e_str(""))
        .param_untyped_default("expires", e_int(0))
        .param_untyped_default("path", e_str(""))
        .param_untyped_default("domain", e_str(""))
        .param_untyped_default("secure", e_bool(false))
        .param_untyped_default("httponly", e_bool(false))
        .body(vec![
            s_return(e_call("__elephc_emit_cookie", vec![e_var("name"), e_call("rawurlencode", vec![e_var("value")]), e_var("expires"), e_var("path"), e_var("domain"), e_var("secure"), e_var("httponly")])),
        ])
        .build()
}

/// `setrawcookie` — transcribed from the PHP form.
fn decl_fn_setrawcookie() -> Stmt {
    function("setrawcookie")
        .param_untyped("name")
        .param_untyped_default("value", e_str(""))
        .param_untyped_default("expires", e_int(0))
        .param_untyped_default("path", e_str(""))
        .param_untyped_default("domain", e_str(""))
        .param_untyped_default("secure", e_bool(false))
        .param_untyped_default("httponly", e_bool(false))
        .body(vec![
            s_return(e_call("__elephc_emit_cookie", vec![e_var("name"), e_var("value"), e_var("expires"), e_var("path"), e_var("domain"), e_var("secure"), e_var("httponly")])),
        ])
        .build()
}

/// `bootstrap 36` — transcribed from the PHP form.
fn decl_stmt_bootstrap_36() -> Stmt {
    interface("SessionHandlerInterface")
        .method(
            method("open")
                .param("path", TypeExpr::Str)
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool),
        )
        .method(
            method("close")
                .returns(TypeExpr::Bool),
        )
        .method(
            method("read")
                .param("id", TypeExpr::Str)
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False])),
        )
        .method(
            method("write")
                .param("id", TypeExpr::Str)
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool),
        )
        .method(
            method("destroy")
                .param("id", TypeExpr::Str)
                .returns(TypeExpr::Bool),
        )
        .method(
            method("gc")
                .param("max_lifetime", TypeExpr::Int)
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::False])),
        )
        .build()
}

/// `bootstrap 37` — transcribed from the PHP form.
fn decl_stmt_bootstrap_37() -> Stmt {
    interface("SessionIdInterface")
        .method(
            method("create_sid")
                .returns(TypeExpr::Str),
        )
        .build()
}

/// `bootstrap 38` — transcribed from the PHP form.
fn decl_stmt_bootstrap_38() -> Stmt {
    interface("SessionUpdateTimestampHandlerInterface")
        .method(
            method("validateId")
                .param("id", TypeExpr::Str)
                .returns(TypeExpr::Bool),
        )
        .method(
            method("updateTimestamp")
                .param("id", TypeExpr::Str)
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool),
        )
        .build()
}

/// `__elephc_session_stage_bytes` — transcribed from the PHP form.
fn decl_fn_elephc_session_stage_bytes() -> Stmt {
    function("__elephc_session_stage_bytes")
        .param("data", TypeExpr::Str)
        .returns(t_ptr())
        .body(vec![
            s_assign("__elephc_sb_len", e_call("strlen", vec![e_var("data")])),
            s_assign("__elephc_sb_ptr", e_call("elephc_web_session_data_stage", vec![e_var("__elephc_sb_len")])),
            s_if(
                e_binop(e_var("__elephc_sb_len"), BinOp::Gt, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_ptr_write_string", vec![e_var("__elephc_sb_ptr"), e_var("data")])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("__elephc_sb_ptr")),
        ])
        .build()
}

/// `__elephc_session_copy_bytes` — transcribed from the PHP form.
fn decl_fn_elephc_session_copy_bytes() -> Stmt {
    function("__elephc_session_copy_bytes")
        .param("data", t_ptr())
        .param("len", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("len"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_return(e_call("str_repeat", vec![e_call("__elephc_ptr_read_string", vec![e_var("data"), e_var("len")]), e_int(1)])),
        ])
        .build()
}

/// `__elephc_session_read_file` — transcribed from the PHP form.
fn decl_fn_elephc_session_read_file() -> Stmt {
    function("__elephc_session_read_file")
        .param("id", TypeExpr::Str)
        .param("save_path", TypeExpr::Str)
        .param("read_and_close", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("__elephc_rf_ptr", e_call("elephc_web_session_read_bytes", vec![e_var("id"), e_var("save_path"), e_var("read_and_close")])),
            s_assign("__elephc_rf_len", e_call("elephc_web_session_data_len", vec![])),
            s_return(e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_rf_ptr"), e_var("__elephc_rf_len")])),
        ])
        .build()
}

/// `__elephc_session_write_file` — transcribed from the PHP form.
fn decl_fn_elephc_session_write_file() -> Stmt {
    function("__elephc_session_write_file")
        .param("id", TypeExpr::Str)
        .param("save_path", TypeExpr::Str)
        .param("data", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("__elephc_wf_len", e_call("strlen", vec![e_var("data")])),
            s_assign("__elephc_wf_ptr", e_call("__elephc_session_stage_bytes", vec![e_var("data")])),
            s_return(e_call("elephc_web_session_write_bytes", vec![e_var("id"), e_var("save_path"), e_var("__elephc_wf_ptr"), e_var("__elephc_wf_len")])),
        ])
        .build()
}

/// `__elephc_session_snapshot_bytes` — transcribed from the PHP form.
fn decl_fn_elephc_session_snapshot_bytes() -> Stmt {
    function("__elephc_session_snapshot_bytes")
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("__elephc_snap_ptr", e_call("elephc_web_session_snapshot_bytes", vec![])),
            s_assign("__elephc_snap_len", e_call("elephc_web_session_data_len", vec![])),
            s_return(e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_snap_ptr"), e_var("__elephc_snap_len")])),
        ])
        .build()
}

/// `__elephc_session_entry_count` — transcribed from the PHP form.
fn decl_fn_elephc_session_entry_count() -> Stmt {
    function("__elephc_session_entry_count")
        .param("data", TypeExpr::Str)
        .param("binary", TypeExpr::Bool)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("__elephc_ec_len", e_call("strlen", vec![e_var("data")])),
            s_assign("__elephc_ec_ptr", e_call("__elephc_session_stage_bytes", vec![e_var("data")])),
            s_if(
                e_var("binary"),
                vec![
                    s_return(e_call("elephc_web_session_count_entries_bin_bytes", vec![e_var("__elephc_ec_ptr"), e_var("__elephc_ec_len")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_web_session_count_entries_bytes", vec![e_var("__elephc_ec_ptr"), e_var("__elephc_ec_len")])),
        ])
        .build()
}

/// `__elephc_session_entry_bytes` — transcribed from the PHP form.
fn decl_fn_elephc_session_entry_bytes() -> Stmt {
    function("__elephc_session_entry_bytes")
        .param("data", TypeExpr::Str)
        .param("idx", TypeExpr::Int)
        .param("value", TypeExpr::Bool)
        .param("binary", TypeExpr::Bool)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("__elephc_eb_len", e_call("strlen", vec![e_var("data")])),
            s_assign("__elephc_eb_ptr", e_call("__elephc_session_stage_bytes", vec![e_var("data")])),
            s_if(
                e_var("binary"),
                vec![
                    s_if(
                        e_var("value"),
                        vec![
                            s_assign("__elephc_eb_out", e_call("elephc_web_session_entry_value_bin_bytes", vec![e_var("__elephc_eb_ptr"), e_var("__elephc_eb_len"), e_var("idx")])),
                            s_return(e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_eb_out"), e_call("elephc_web_session_data_len", vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("__elephc_eb_out", e_call("elephc_web_session_entry_key_bin_bytes", vec![e_var("__elephc_eb_ptr"), e_var("__elephc_eb_len"), e_var("idx")])),
                    s_return(e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_eb_out"), e_call("elephc_web_session_data_len", vec![])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("value"),
                vec![
                    s_assign("__elephc_eb_out", e_call("elephc_web_session_entry_value_bytes", vec![e_var("__elephc_eb_ptr"), e_var("__elephc_eb_len"), e_var("idx")])),
                    s_return(e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_eb_out"), e_call("elephc_web_session_data_len", vec![])])),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_eb_out", e_call("elephc_web_session_entry_key_bytes", vec![e_var("__elephc_eb_ptr"), e_var("__elephc_eb_len"), e_var("idx")])),
            s_return(e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_eb_out"), e_call("elephc_web_session_data_len", vec![])])),
        ])
        .build()
}

/// `SessionHandler` — transcribed from the PHP form.
fn decl_class_sessionhandler() -> Stmt {
    class("SessionHandler")
        .implements("SessionHandlerInterface")
        .implements("SessionIdInterface")
        .method(
            method("open")
                .param("path", TypeExpr::Str)
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_call("elephc_web_session_set_save_path", vec![e_var("path")])),
                    s_expr(e_call("elephc_web_session_set_name", vec![e_var("name")])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_web_session_abort", vec![e_call("elephc_web_session_get_id", vec![]), e_call("elephc_web_session_get_save_path", vec![])]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("read")
                .param("id", TypeExpr::Str)
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_return(e_call("__elephc_session_read_file", vec![e_var("id"), e_call("elephc_web_session_get_save_path", vec![]), e_int(0)])),
                ]),
        )
        .method(
            method("write")
                .param("id", TypeExpr::Str)
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("__elephc_session_write_file", vec![e_var("id"), e_call("elephc_web_session_get_save_path", vec![]), e_var("data")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("destroy")
                .param("id", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_web_session_destroy", vec![e_var("id"), e_call("elephc_web_session_get_save_path", vec![])]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("gc")
                .param("max_lifetime", TypeExpr::Int)
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
                .body(vec![
                    s_return(e_call("elephc_web_session_gc", vec![e_call("elephc_web_session_get_save_path", vec![]), e_var("max_lifetime")])),
                ]),
        )
        .method(
            method("create_sid")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("elephc_web_session_create_id", vec![e_str("")])),
                ]),
        )
        .build()
}

/// `__ElephcSessionState` — transcribed from the PHP form.
fn decl_class_elephcsessionstate() -> Stmt {
    class("__ElephcSessionState")
        .static_prop("handler", t_nullable(t_class("SessionHandlerInterface")), Some(e_null()))
        .static_prop("shutdown", TypeExpr::Bool, Some(e_bool(true)))
        .static_prop("snapshot", TypeExpr::Str, Some(e_str("")))
        .static_prop("snapshotValid", TypeExpr::Bool, Some(e_bool(false)))
        .static_prop("sendCookie", TypeExpr::Bool, Some(e_bool(false)))
        .build()
}

/// `__ElephcCallableSessionHandler` — transcribed from the PHP form.
fn decl_class_elephccallablesessionhandler() -> Stmt {
    class("__ElephcCallableSessionHandler")
        .implements("SessionHandlerInterface")
        .implements("SessionIdInterface")
        .implements("SessionUpdateTimestampHandlerInterface")
        .prop("openCb", t_mixed(), None)
        .prop("closeCb", t_mixed(), None)
        .prop("readCb", t_mixed(), None)
        .prop("writeCb", t_mixed(), None)
        .prop("destroyCb", t_mixed(), None)
        .prop("gcCb", t_mixed(), None)
        .prop("createSidCb", t_mixed(), None)
        .prop("validateIdCb", t_mixed(), None)
        .prop("updateTimestampCb", t_mixed(), None)
        .method(
            method("__construct")
                .param("open", t_mixed())
                .param("close", t_mixed())
                .param("read", t_mixed())
                .param("write", t_mixed())
                .param("destroy", t_mixed())
                .param("gc", t_mixed())
                .param("create_sid", t_mixed())
                .param("validate_id", t_mixed())
                .param("update_timestamp", t_mixed())
                .body(vec![
                    s_prop_assign(e_this(), "openCb", e_var("open")),
                    s_prop_assign(e_this(), "closeCb", e_var("close")),
                    s_prop_assign(e_this(), "readCb", e_var("read")),
                    s_prop_assign(e_this(), "writeCb", e_var("write")),
                    s_prop_assign(e_this(), "destroyCb", e_var("destroy")),
                    s_prop_assign(e_this(), "gcCb", e_var("gc")),
                    s_prop_assign(e_this(), "createSidCb", e_var("create_sid")),
                    s_prop_assign(e_this(), "validateIdCb", e_var("validate_id")),
                    s_prop_assign(e_this(), "updateTimestampCb", e_var("update_timestamp")),
                ]),
        )
        .method(
            method("open")
                .param("path", TypeExpr::Str)
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_cast(CastType::Bool, e_call("call_user_func", vec![e_this_prop("openCb"), e_var("path"), e_var("name")]))),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_cast(CastType::Bool, e_call("call_user_func", vec![e_this_prop("closeCb")]))),
                ]),
        )
        .method(
            method("read")
                .param("id", TypeExpr::Str)
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_assign("__elephc_r", e_call("call_user_func", vec![e_this_prop("readCb"), e_var("id")])),
                    s_if(
                        e_binop(e_var("__elephc_r"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_cast(CastType::String, e_var("__elephc_r"))),
                ]),
        )
        .method(
            method("write")
                .param("id", TypeExpr::Str)
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_cast(CastType::Bool, e_call("call_user_func", vec![e_this_prop("writeCb"), e_var("id"), e_var("data")]))),
                ]),
        )
        .method(
            method("destroy")
                .param("id", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_cast(CastType::Bool, e_call("call_user_func", vec![e_this_prop("destroyCb"), e_var("id")]))),
                ]),
        )
        .method(
            method("gc")
                .param("max_lifetime", TypeExpr::Int)
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
                .body(vec![
                    s_assign("__elephc_g", e_call("call_user_func", vec![e_this_prop("gcCb"), e_var("max_lifetime")])),
                    s_if(
                        e_binop(e_var("__elephc_g"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_cast(CastType::Int, e_var("__elephc_g"))),
                ]),
        )
        .method(
            method("create_sid")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("__elephc_c", e_this_prop("createSidCb")),
                    s_if(
                        e_binop(e_var("__elephc_c"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_return(e_cast(CastType::String, e_call("call_user_func", vec![e_var("__elephc_c")]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("elephc_web_session_create_id", vec![e_str("")])),
                ]),
        )
        .method(
            method("validateId")
                .param("id", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("__elephc_vc", e_this_prop("validateIdCb")),
                    s_if(
                        e_binop(e_var("__elephc_vc"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_return(e_cast(CastType::Bool, e_call("call_user_func", vec![e_var("__elephc_vc"), e_var("id")]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("updateTimestamp")
                .param("id", TypeExpr::Str)
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("__elephc_uc", e_this_prop("updateTimestampCb")),
                    s_if(
                        e_binop(e_var("__elephc_uc"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_return(e_cast(CastType::Bool, e_call("call_user_func", vec![e_var("__elephc_uc"), e_var("id"), e_var("data")]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_cast(CastType::Bool, e_call("call_user_func", vec![e_this_prop("writeCb"), e_var("id"), e_var("data")]))),
                ]),
        )
        .build()
}

/// `error_log` — transcribed from the PHP form.
fn decl_fn_error_log() -> Stmt {
    function("error_log")
        .param("message", TypeExpr::Str)
        .param_default("message_type", TypeExpr::Int, e_int(0))
        .param_default("destination", t_nullable(TypeExpr::Str), e_null())
        .param_default("additional_headers", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("message_type"), BinOp::StrictEq, e_int(3)),
                vec![
                    s_if(
                        e_binop(e_var("destination"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("__elephc_el_fh", e_call("fopen", vec![e_cast(CastType::String, e_var("destination")), e_str("a")])),
                    s_if(
                        e_binop(e_var("__elephc_el_fh"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("fwrite", vec![e_var("__elephc_el_fh"), e_var("message")])),
                    s_expr(e_call("fclose", vec![e_var("__elephc_el_fh")])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("message_type"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_assign("__elephc_el_m", e_var("message")),
                    s_if(
                        e_binop(e_binop(e_var("__elephc_el_m"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_call("substr", vec![e_var("__elephc_el_m"), e_neg(e_int(1))]), BinOp::StrictNotEq, e_str("\n"))),
                        vec![
                            s_assign("__elephc_el_m", e_binop(e_var("__elephc_el_m"), BinOp::Concat, e_str("\n"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("fwrite", vec![e_const("STDERR"), e_var("__elephc_el_m")])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("message_type"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_binop(e_binop(e_binop(e_str("error_log(): mail delivery (type 1) is not supported under --web"), BinOp::Concat, e_str(" [to=")), BinOp::Concat, e_cast(CastType::String, e_var("destination"))), BinOp::Concat, e_str(", headers=")), BinOp::Concat, e_cast(CastType::String, e_var("additional_headers"))), BinOp::Concat, e_str("]\n"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `trigger_error` — transcribed from the PHP form.
fn decl_fn_trigger_error() -> Stmt {
    function("trigger_error")
        .param("message", TypeExpr::Str)
        .param_default("error_level", TypeExpr::Int, e_const("E_USER_NOTICE"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("__elephc_te_prefix", e_str("Notice")),
            s_if(
                e_binop(e_var("error_level"), BinOp::StrictEq, e_const("E_USER_ERROR")),
                vec![
                    s_assign("__elephc_te_prefix", e_str("Fatal error")),
                ],
                vec![
                (e_binop(e_binop(e_var("error_level"), BinOp::StrictEq, e_const("E_USER_WARNING")), BinOp::Or, e_binop(e_var("error_level"), BinOp::StrictEq, e_const("E_WARNING"))), vec![
                    s_assign("__elephc_te_prefix", e_str("Warning")),
                ]),
                (e_binop(e_binop(e_var("error_level"), BinOp::StrictEq, e_const("E_USER_DEPRECATED")), BinOp::Or, e_binop(e_var("error_level"), BinOp::StrictEq, e_const("E_DEPRECATED"))), vec![
                    s_assign("__elephc_te_prefix", e_str("Deprecated")),
                ]),
            ],
                None,
            ),
            s_expr(e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_binop(e_var("__elephc_te_prefix"), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("message")), BinOp::Concat, e_str("\n"))])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_session_start_option_known` — transcribed from the PHP form.
fn decl_fn_elephc_session_start_option_known() -> Stmt {
    function("__elephc_session_start_option_known")
        .param("key", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("cookie_partitioned")),
                vec![
                    s_return(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80500))),
                ],
                vec![],
                None,
            ),
            s_foreach(e_array(vec![e_str("name"), e_str("save_path"), e_str("read_and_close"), e_str("cookie_lifetime"), e_str("cookie_path"), e_str("cookie_domain"), e_str("cookie_secure"), e_str("cookie_httponly"), e_str("cookie_samesite"), e_str("cache_limiter"), e_str("cache_expire"), e_str("use_strict_mode"), e_str("serialize_handler"), e_str("gc_probability"), e_str("gc_divisor"), e_str("gc_maxlifetime"), e_str("sid_length"), e_str("sid_bits_per_character"), e_str("referer_check"), e_str("use_cookies"), e_str("use_only_cookies"), e_str("lazy_write"), e_str("use_trans_sid"), e_str("trans_sid_tags"), e_str("trans_sid_hosts")]), None, "__elephc_known_option", vec![
                s_if(
                    e_binop(e_var("key"), BinOp::StrictEq, e_var("__elephc_known_option")),
                    vec![
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `session_start` — transcribed from the PHP form.
fn decl_fn_session_start() -> Stmt {
    function("session_start")
        .param_default("options", t_mixed(), e_array(vec![]))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_call("is_array", vec![e_var("options")])),
                vec![
                    s_throw(e_new("TypeError", vec![e_str("session_start(): Argument #1 ($options) must be of type array")])),
                ],
                vec![],
                None,
            ),
            s_assign("status", e_call("elephc_web_session_get_status", vec![])),
            s_if(
                e_binop(e_var("status"), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_start(): Ignoring session_start() because a session is already active"), e_const("E_NOTICE")])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_opt_name", e_null()),
            s_assign("__elephc_opt_save_path", e_null()),
            s_assign("__elephc_opt_read_and_close", e_bool(false)),
            s_assign("__elephc_opt_cl", e_null()),
            s_assign("__elephc_opt_cp", e_null()),
            s_assign("__elephc_opt_cd", e_null()),
            s_assign("__elephc_opt_cs", e_null()),
            s_assign("__elephc_opt_cpart", e_null()),
            s_assign("__elephc_opt_ch", e_null()),
            s_assign("__elephc_opt_css", e_null()),
            s_assign("__elephc_opt_cachelim", e_null()),
            s_assign("__elephc_opt_cacheexp", e_null()),
            s_assign("__elephc_opt_strict", e_null()),
            s_assign("__elephc_opt_serialize", e_null()),
            s_assign("__elephc_opt_gcprob", e_null()),
            s_assign("__elephc_opt_gcdiv", e_null()),
            s_assign("__elephc_opt_gcmax", e_null()),
            s_assign("__elephc_opt_sidlen", e_null()),
            s_assign("__elephc_opt_sidbits", e_null()),
            s_assign("__elephc_opt_referer", e_null()),
            s_assign("__elephc_opt_usecookies", e_null()),
            s_assign("__elephc_opt_useonly", e_null()),
            s_assign("__elephc_opt_lazy", e_null()),
            s_assign("__elephc_opt_transsid", e_null()),
            s_assign("__elephc_opt_transtags", e_null()),
            s_assign("__elephc_opt_transhosts", e_null()),
            s_foreach(e_var("options"), Some("__elephc_ok"), "__elephc_ov", vec![
                s_if(
                    e_call("is_int", vec![e_var("__elephc_ok")]),
                    vec![
                        s_if(
                            e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80500)),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("session_start(): Argument #1 ($options) must be of type array with keys as string")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_not(e_call("is_string", vec![e_var("__elephc_ov")])), BinOp::And, e_not(e_call("is_int", vec![e_var("__elephc_ov")]))), BinOp::And, e_not(e_call("is_bool", vec![e_var("__elephc_ov")]))),
                    vec![
                        s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("session_start(): Option \""), BinOp::Concat, e_var("__elephc_ok")), BinOp::Concat, e_str("\" must be of type string|int|bool"))])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_not(e_call("__elephc_session_start_option_known", vec![e_var("__elephc_ok")])),
                    vec![
                        s_expr(e_call("trigger_error", vec![e_binop(e_binop(e_str("session_start(): Setting option \""), BinOp::Concat, e_var("__elephc_ok")), BinOp::Concat, e_str("\" failed")), e_const("E_WARNING")])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80500)), BinOp::And, e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("read_and_close"))), BinOp::And, e_call("is_string", vec![e_var("__elephc_ov")])), BinOp::And, e_not(e_call("is_numeric", vec![e_var("__elephc_ov")]))),
                    vec![
                        s_throw(e_new("TypeError", vec![e_str("session_start(): Option \"read_and_close\" value must be of type compatible with int")])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("name")),
                    vec![
                        s_assign("__elephc_opt_name", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("save_path")),
                    vec![
                        s_assign("__elephc_opt_save_path", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("read_and_close")),
                    vec![
                        s_assign("__elephc_opt_read_and_close", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cookie_lifetime")),
                    vec![
                        s_assign("__elephc_opt_cl", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cookie_path")),
                    vec![
                        s_assign("__elephc_opt_cp", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cookie_domain")),
                    vec![
                        s_assign("__elephc_opt_cd", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cookie_secure")),
                    vec![
                        s_assign("__elephc_opt_cs", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80500)), BinOp::And, e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cookie_partitioned"))),
                    vec![
                        s_assign("__elephc_opt_cpart", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cookie_httponly")),
                    vec![
                        s_assign("__elephc_opt_ch", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cookie_samesite")),
                    vec![
                        s_assign("__elephc_opt_css", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cache_limiter")),
                    vec![
                        s_assign("__elephc_opt_cachelim", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("cache_expire")),
                    vec![
                        s_assign("__elephc_opt_cacheexp", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("use_strict_mode")),
                    vec![
                        s_assign("__elephc_opt_strict", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("serialize_handler")),
                    vec![
                        s_assign("__elephc_opt_serialize", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("gc_probability")),
                    vec![
                        s_assign("__elephc_opt_gcprob", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("gc_divisor")),
                    vec![
                        s_assign("__elephc_opt_gcdiv", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("gc_maxlifetime")),
                    vec![
                        s_assign("__elephc_opt_gcmax", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("sid_length")),
                    vec![
                        s_assign("__elephc_opt_sidlen", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("sid_bits_per_character")),
                    vec![
                        s_assign("__elephc_opt_sidbits", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("referer_check")),
                    vec![
                        s_assign("__elephc_opt_referer", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("use_cookies")),
                    vec![
                        s_assign("__elephc_opt_usecookies", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("use_only_cookies")),
                    vec![
                        s_assign("__elephc_opt_useonly", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("lazy_write")),
                    vec![
                        s_assign("__elephc_opt_lazy", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("use_trans_sid")),
                    vec![
                        s_assign("__elephc_opt_transsid", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("trans_sid_tags")),
                    vec![
                        s_assign("__elephc_opt_transtags", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_ok"), BinOp::StrictEq, e_str("trans_sid_hosts")),
                    vec![
                        s_assign("__elephc_opt_transhosts", e_var("__elephc_ov")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_if(
                e_binop(e_var("__elephc_opt_name"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_call("__elephc_session_name_valid", vec![e_cast(CastType::String, e_var("__elephc_opt_name"))]),
                        vec![
                            s_expr(e_call("elephc_web_session_set_name", vec![e_cast(CastType::String, e_var("__elephc_opt_name"))])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_call("trigger_error", vec![e_str("session_start(): Setting option \"name\" failed"), e_const("E_WARNING")])),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_save_path"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_save_path", vec![e_cast(CastType::String, e_var("__elephc_opt_save_path"))])),
                ],
                vec![],
                None,
            ),
            s_assign("read_and_close", e_int(0)),
            s_if(
                e_binop(e_cast(CastType::Int, e_var("__elephc_opt_read_and_close")), BinOp::Gt, e_int(0)),
                vec![
                    s_assign("read_and_close", e_int(1)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_ss_cl", e_call("elephc_web_session_get_cookie_lifetime", vec![])),
            s_assign("__elephc_ss_cp", e_call("elephc_web_session_get_cookie_path", vec![])),
            s_assign("__elephc_ss_cd", e_call("elephc_web_session_get_cookie_domain", vec![])),
            s_assign("__elephc_ss_cs", e_call("elephc_web_session_get_cookie_secure", vec![])),
            s_assign("__elephc_ss_cpart", e_call("elephc_web_session_get_cookie_partitioned", vec![])),
            s_assign("__elephc_ss_ch", e_call("elephc_web_session_get_cookie_httponly", vec![])),
            s_assign("__elephc_ss_css", e_call("elephc_web_session_get_cookie_samesite", vec![])),
            s_if(
                e_binop(e_var("__elephc_opt_cl"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_cast(CastType::Int, e_var("__elephc_opt_cl")), BinOp::GtEq, e_int(0)),
                        vec![
                            s_assign("__elephc_ss_cl", e_cast(CastType::Int, e_var("__elephc_opt_cl"))),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_call("trigger_error", vec![e_str("session_start(): Setting option \"cookie_lifetime\" failed"), e_const("E_WARNING")])),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_cp"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_assign("__elephc_ss_cp", e_cast(CastType::String, e_var("__elephc_opt_cp"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_cd"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_assign("__elephc_ss_cd", e_cast(CastType::String, e_var("__elephc_opt_cd"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_cs"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_assign("__elephc_ss_cs", e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_cs")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_cpart"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_assign("__elephc_ss_cpart", e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_cpart")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_ch"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_assign("__elephc_ss_ch", e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_ch")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("__elephc_opt_css"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_var("__elephc_opt_css"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("__elephc_opt_css"), BinOp::StrictEq, e_str("Strict"))), BinOp::Or, e_binop(e_var("__elephc_opt_css"), BinOp::StrictEq, e_str("Lax"))), BinOp::Or, e_binop(e_var("__elephc_opt_css"), BinOp::StrictEq, e_str("None")))),
                vec![
                    s_assign("__elephc_ss_css", e_cast(CastType::String, e_var("__elephc_opt_css"))),
                ],
                vec![
                (e_binop(e_var("__elephc_opt_css"), BinOp::StrictNotEq, e_null()), vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_start(): Setting option \"cookie_samesite\" failed"), e_const("E_WARNING")])),
                ]),
            ],
                None,
            ),
            s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_var("__elephc_ss_cl"), e_var("__elephc_ss_cp"), e_var("__elephc_ss_cd"), e_var("__elephc_ss_cs"), e_var("__elephc_ss_cpart"), e_var("__elephc_ss_ch"), e_var("__elephc_ss_css")])),
            s_if(
                e_binop(e_var("__elephc_opt_cachelim"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_cache_limiter", vec![e_cast(CastType::String, e_var("__elephc_opt_cachelim"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_cacheexp"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_cache_expire", vec![e_cast(CastType::Int, e_var("__elephc_opt_cacheexp"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_strict"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_strict_mode", vec![e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_strict")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_serialize"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("__elephc_opt_serialize"), BinOp::StrictEq, e_str("php")), BinOp::Or, e_binop(e_var("__elephc_opt_serialize"), BinOp::StrictEq, e_str("php_serialize"))), BinOp::Or, e_binop(e_var("__elephc_opt_serialize"), BinOp::StrictEq, e_str("php_binary"))),
                        vec![
                            s_expr(e_call("elephc_web_session_set_serialize_handler", vec![e_cast(CastType::String, e_var("__elephc_opt_serialize"))])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_call("trigger_error", vec![e_str("session_start(): Setting option \"serialize_handler\" failed"), e_const("E_WARNING")])),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_gcprob"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_cast(CastType::Int, e_var("__elephc_opt_gcprob")), BinOp::Lt, e_int(0))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): session.gc_probability must be greater than or equal to 0"), e_const("E_WARNING")])),
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Setting option \"gc_probability\" failed"), e_const("E_WARNING")])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_call("elephc_web_session_set_gc_probability", vec![e_cast(CastType::Int, e_var("__elephc_opt_gcprob"))])),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_gcdiv"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_cast(CastType::Int, e_var("__elephc_opt_gcdiv")), BinOp::LtEq, e_int(0))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): session.gc_divisor must be greater than 0"), e_const("E_WARNING")])),
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Setting option \"gc_divisor\" failed"), e_const("E_WARNING")])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_call("elephc_web_session_set_gc_divisor", vec![e_cast(CastType::Int, e_var("__elephc_opt_gcdiv"))])),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_gcmax"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_gc_maxlifetime", vec![e_cast(CastType::Int, e_var("__elephc_opt_gcmax"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_sidlen"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_cast(CastType::Int, e_var("__elephc_opt_sidlen")), BinOp::StrictNotEq, e_int(32))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): session.sid_length INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_web_session_set_sid_length", vec![e_cast(CastType::Int, e_var("__elephc_opt_sidlen"))]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Setting option \"sid_length\" failed"), e_const("E_WARNING")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_sidbits"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_cast(CastType::Int, e_var("__elephc_opt_sidbits")), BinOp::StrictNotEq, e_int(4))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): session.sid_bits_per_character INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_web_session_set_sid_bits_per_character", vec![e_cast(CastType::Int, e_var("__elephc_opt_sidbits"))]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Setting option \"sid_bits_per_character\" failed"), e_const("E_WARNING")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_referer"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_cast(CastType::String, e_var("__elephc_opt_referer")), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Usage of session.referer_check INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_web_session_set_referer_check", vec![e_cast(CastType::String, e_var("__elephc_opt_referer"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_usecookies"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_use_cookies", vec![e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_usecookies")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_useonly"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_useonly")]), BinOp::StrictEq, e_int(0))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Disabling session.use_only_cookies INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_web_session_set_use_only_cookies", vec![e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_useonly")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_lazy"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_lazy_write", vec![e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_lazy")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_transsid"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_transsid")]), BinOp::StrictEq, e_int(1))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Enabling session.use_trans_sid INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_web_session_set_use_trans_sid", vec![e_call("__elephc_session_ini_bool", vec![e_var("__elephc_opt_transsid")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_transtags"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_cast(CastType::String, e_var("__elephc_opt_transtags")), BinOp::StrictNotEq, e_str("a=href,area=href,frame=src,form="))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Usage of session.trans_sid_tags INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_web_session_set_trans_sid_tags", vec![e_cast(CastType::String, e_var("__elephc_opt_transtags"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_opt_transhosts"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_cast(CastType::String, e_var("__elephc_opt_transhosts")), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("session_start(): Usage of session.trans_sid_hosts INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_web_session_set_trans_sid_hosts", vec![e_cast(CastType::String, e_var("__elephc_opt_transhosts"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_session_start_core", vec![e_var("read_and_close")])),
        ])
        .build()
}

/// `__elephc_session_start_core` — transcribed from the PHP form.
fn decl_fn_elephc_session_start_core() -> Stmt {
    function("__elephc_session_start_core")
        .param("read_and_close", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("status", e_call("elephc_web_session_get_status", vec![])),
            s_if(
                e_binop(e_var("status"), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_start(): Ignoring session_start() because a session is already active"), e_const("E_NOTICE")])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_h", e_static_prop("__ElephcSessionState", "handler")),
            s_assign("name", e_call("elephc_web_session_get_name", vec![])),
            s_assign("save_path", e_call("elephc_web_session_get_save_path", vec![])),
            s_assign("id", e_call("elephc_web_session_get_id", vec![])),
            s_assign("__elephc_use_cookies", e_call("elephc_web_session_get_use_cookies", vec![])),
            s_assign("__elephc_use_only", e_call("elephc_web_session_get_use_only_cookies", vec![])),
            s_assign("__elephc_supplied_id", e_binop(e_var("id"), BinOp::StrictNotEq, e_str(""))),
            s_assign("__elephc_from_cookie", e_bool(false)),
            s_assign("__elephc_from_global", e_bool(false)),
            s_if(
                e_binop(e_binop(e_var("id"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_var("__elephc_use_cookies"), BinOp::StrictEq, e_int(1))),
                vec![
                    s_if(
                        e_call("isset", vec![e_index(e_var("_COOKIE"), e_var("name"))]),
                        vec![
                            s_assign("id", e_cast(CastType::String, e_index(e_var("_COOKIE"), e_var("name")))),
                            s_assign("__elephc_from_cookie", e_bool(true)),
                            s_assign("__elephc_supplied_id", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("id"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_var("__elephc_use_only"), BinOp::StrictEq, e_int(0))),
                vec![
                    s_if(
                        e_call("isset", vec![e_index(e_var("_GET"), e_var("name"))]),
                        vec![
                            s_assign("id", e_cast(CastType::String, e_index(e_var("_GET"), e_var("name")))),
                            s_assign("__elephc_supplied_id", e_bool(true)),
                            s_assign("__elephc_from_global", e_bool(true)),
                        ],
                        vec![
                        (e_call("isset", vec![e_index(e_var("_POST"), e_var("name"))]), vec![
                            s_assign("id", e_cast(CastType::String, e_index(e_var("_POST"), e_var("name")))),
                            s_assign("__elephc_supplied_id", e_bool(true)),
                            s_assign("__elephc_from_global", e_bool(true)),
                        ]),
                    ],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_refchk", e_call("elephc_web_session_get_referer_check", vec![])),
            s_if(
                e_binop(e_binop(e_binop(e_var("id"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_var("__elephc_use_only"), BinOp::StrictEq, e_int(0))), BinOp::And, e_binop(e_var("__elephc_refchk"), BinOp::StrictNotEq, e_str(""))),
                vec![
                    s_assign("__elephc_referer", e_ternary(e_call("isset", vec![e_index(e_var("_SERVER"), e_str("HTTP_REFERER"))]), e_cast(CastType::String, e_index(e_var("_SERVER"), e_str("HTTP_REFERER"))), e_str(""))),
                    s_if(
                        e_binop(e_call("strpos", vec![e_var("__elephc_referer"), e_var("__elephc_refchk")]), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_assign("id", e_str("")),
                            s_assign("__elephc_supplied_id", e_bool(false)),
                            s_assign("__elephc_from_cookie", e_bool(false)),
                            s_assign("__elephc_from_global", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("id"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_call("strpos", vec![e_var("id"), e_str("\r")]), BinOp::StrictNotEq, e_bool(false)), BinOp::Or, e_binop(e_call("strpos", vec![e_var("id"), e_str("\n")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("id"), e_str("\t")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("id"), e_str(" ")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("id"), e_str("<")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("id"), e_str(">")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("id"), e_str("'")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("id"), e_str("\"")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("id"), e_str("\\")]), BinOp::StrictNotEq, e_bool(false)))),
                vec![
                    s_assign("id", e_str("")),
                    s_assign("__elephc_supplied_id", e_bool(false)),
                    s_assign("__elephc_from_cookie", e_bool(false)),
                    s_assign("__elephc_from_global", e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_ACTIVE")])),
            s_if(
                e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                vec![
                    s_if(
                        e_not(e_method_call(e_var("__elephc_h"), "open", vec![e_var("save_path"), e_var("name")])),
                        vec![
                            s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("id"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_call("elephc_web_session_get_strict_mode", vec![]), BinOp::StrictEq, e_int(1))),
                vec![
                    s_assign("__elephc_id_ok", e_binop(e_call("elephc_web_session_file_exists", vec![e_var("id"), e_var("save_path")]), BinOp::StrictEq, e_int(1))),
                    s_if(
                        e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("__elephc_id_ok", e_bool(true)),
                            s_if(
                                e_instance_of(e_var("__elephc_h"), "SessionUpdateTimestampHandlerInterface"),
                                vec![
                                    s_assign("__elephc_id_ok", e_method_call(e_var("__elephc_h"), "validateId", vec![e_var("id")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_var("__elephc_id_ok")),
                        vec![
                            s_assign("id", e_str("")),
                            s_assign("__elephc_supplied_id", e_bool(false)),
                            s_assign("__elephc_from_cookie", e_bool(false)),
                            s_assign("__elephc_from_global", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("id"), BinOp::StrictEq, e_str("")),
                vec![
                    s_assign("__elephc_attempt", e_int(0)),
                    s_do_while(vec![
                        s_assign("id", e_call("elephc_web_session_create_id", vec![e_str("")])),
                        s_assign("__elephc_collision", e_binop(e_call("elephc_web_session_file_exists", vec![e_var("id"), e_var("save_path")]), BinOp::StrictEq, e_int(1))),
                        s_assign("__elephc_attempt", e_binop(e_var("__elephc_attempt"), BinOp::Add, e_int(1))),
                    ], e_binop(e_var("__elephc_collision"), BinOp::And, e_binop(e_var("__elephc_attempt"), BinOp::Lt, e_int(3)))),
                    s_if(
                        e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_if(
                                e_instance_of(e_var("__elephc_h"), "SessionIdInterface"),
                                vec![
                                    s_assign("id", e_method_call(e_var("__elephc_h"), "create_sid", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_static_prop_assign("__ElephcSessionState", "sendCookie", e_binop(e_binop(e_binop(e_var("__elephc_use_cookies"), BinOp::StrictEq, e_int(1)), BinOp::And, e_not(e_var("__elephc_from_cookie"))), BinOp::And, e_not(e_var("__elephc_from_global")))),
            s_expr(e_call("elephc_web_session_set_id", vec![e_cast(CastType::String, e_var("id"))])),
            s_assign("_SESSION", e_array(vec![])),
            s_static_prop_assign("__ElephcSessionState", "snapshot", e_str("")),
            s_static_prop_assign("__ElephcSessionState", "snapshotValid", e_bool(false)),
            s_if(
                e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                vec![
                    s_assign("__elephc_hraw", e_method_call(e_var("__elephc_h"), "read", vec![e_cast(CastType::String, e_var("id"))])),
                    s_if(
                        e_binop(e_var("__elephc_hraw"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_expr(e_method_call(e_var("__elephc_h"), "close", vec![])),
                            s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_static_prop_assign("__ElephcSessionState", "snapshot", e_cast(CastType::String, e_var("__elephc_hraw"))),
                    s_static_prop_assign("__ElephcSessionState", "snapshotValid", e_bool(true)),
                    s_if(
                        e_binop(e_var("__elephc_hraw"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_expr(e_call("__elephc_session_decode", vec![e_cast(CastType::String, e_var("__elephc_hraw"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("read_and_close"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_expr(e_method_call(e_var("__elephc_h"), "close", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                Some(vec![
                s_assign("__elephc_fraw", e_call("__elephc_session_read_file", vec![e_cast(CastType::String, e_var("id")), e_var("save_path"), e_var("read_and_close")])),
                s_if(
                    e_binop(e_call("elephc_web_session_last_read_ok", vec![]), BinOp::StrictNotEq, e_int(1)),
                    vec![
                        s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__elephc_fraw"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_expr(e_call("__elephc_session_decode", vec![e_var("__elephc_fraw")])),
                    ],
                    vec![],
                    None,
                ),
            ]),
            ),
            s_if(
                e_binop(e_call("elephc_web_session_should_gc", vec![]), BinOp::StrictEq, e_int(1)),
                vec![
                    s_expr(e_call("session_gc", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_static_prop("__ElephcSessionState", "sendCookie"),
                vec![
                    s_if(
                        e_not(e_call("__elephc_session_send_cookie", vec![])),
                        vec![
                            s_expr(e_call("session_abort", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_session_send_cache_headers", vec![])),
            s_if(
                e_binop(e_var("read_and_close"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `session_write_close` — transcribed from the PHP form.
fn decl_fn_session_write_close() -> Stmt {
    function("session_write_close")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("id", e_call("elephc_web_session_get_id", vec![])),
            s_assign("save_path", e_call("elephc_web_session_get_save_path", vec![])),
            s_assign("__elephc_h", e_static_prop("__ElephcSessionState", "handler")),
            s_assign("__elephc_encoded", e_call("__elephc_session_encode", vec![])),
            s_if(
                e_binop(e_var("__elephc_encoded"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                        vec![
                            s_expr(e_method_call(e_var("__elephc_h"), "write", vec![e_var("id"), e_str("")])),
                            s_expr(e_method_call(e_var("__elephc_h"), "close", vec![])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_call("__elephc_session_write_file", vec![e_var("id"), e_var("save_path"), e_str("")])),
                    ]),
                    ),
                    s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_data", e_cast(CastType::String, e_var("__elephc_encoded"))),
            s_if(
                e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_assign("__elephc_ts_done", e_bool(false)),
                    s_if(
                        e_binop(e_binop(e_binop(e_call("elephc_web_session_get_lazy_write", vec![]), BinOp::StrictEq, e_int(1)), BinOp::And, e_static_prop("__ElephcSessionState", "snapshotValid")), BinOp::And, e_binop(e_var("__elephc_data"), BinOp::StrictEq, e_static_prop("__ElephcSessionState", "snapshot"))),
                        vec![
                            s_if(
                                e_instance_of(e_var("__elephc_h"), "SessionUpdateTimestampHandlerInterface"),
                                vec![
                                    s_expr(e_method_call(e_var("__elephc_h"), "updateTimestamp", vec![e_var("id"), e_var("__elephc_data")])),
                                    s_assign("__elephc_ts_done", e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_var("__elephc_ts_done")),
                        vec![
                            s_expr(e_method_call(e_var("__elephc_h"), "write", vec![e_var("id"), e_var("__elephc_data")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_var("__elephc_h"), "close", vec![])),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_binop(e_call("elephc_web_session_get_lazy_write", vec![]), BinOp::StrictEq, e_int(1)), BinOp::And, e_binop(e_var("__elephc_data"), BinOp::StrictEq, e_call("__elephc_session_snapshot_bytes", vec![]))),
                    vec![
                        s_expr(e_call("elephc_web_session_touch", vec![e_var("id"), e_var("save_path")])),
                    ],
                    vec![],
                    Some(vec![
                    s_expr(e_call("__elephc_session_write_file", vec![e_var("id"), e_var("save_path"), e_var("__elephc_data")])),
                ]),
                ),
            ]),
            ),
            s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `session_destroy` — transcribed from the PHP form.
fn decl_fn_session_destroy() -> Stmt {
    function("session_destroy")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_destroy(): Trying to destroy uninitialized session"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("id", e_call("elephc_web_session_get_id", vec![])),
            s_assign("save_path", e_call("elephc_web_session_get_save_path", vec![])),
            s_assign("__elephc_h", e_static_prop("__ElephcSessionState", "handler")),
            s_assign("__elephc_destroyed", e_bool(true)),
            s_if(
                e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                vec![
                    s_assign("__elephc_destroyed", e_method_call(e_var("__elephc_h"), "destroy", vec![e_var("id")])),
                    s_expr(e_method_call(e_var("__elephc_h"), "close", vec![])),
                ],
                vec![],
                Some(vec![
                s_assign("__elephc_destroyed", e_binop(e_call("elephc_web_session_destroy", vec![e_var("id"), e_var("save_path")]), BinOp::StrictEq, e_int(1))),
            ]),
            ),
            s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
            s_expr(e_call("elephc_web_session_set_id", vec![e_str("")])),
            s_return(e_var("__elephc_destroyed")),
        ])
        .build()
}

/// `session_id` — transcribed from the PHP form.
fn decl_fn_session_id() -> Stmt {
    function("session_id")
        .param_default("id", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("id"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE"))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_id(): Session ID cannot be changed when a session is active"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("old", e_call("elephc_web_session_get_id", vec![])),
            s_if(
                e_binop(e_var("id"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_id", vec![e_cast(CastType::String, e_var("id"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("old")),
        ])
        .build()
}

/// `session_name` — transcribed from the PHP form.
fn decl_fn_session_name() -> Stmt {
    function("session_name")
        .param_default("name", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("name"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE"))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_name(): Session name cannot be changed when a session is active"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("old", e_call("elephc_web_session_get_name", vec![])),
            s_if(
                e_binop(e_var("name"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_if(
                        e_not(e_call("__elephc_session_name_valid", vec![e_cast(CastType::String, e_var("name"))])),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_binop(e_binop(e_str("session_name(): session.name \""), BinOp::Concat, e_cast(CastType::String, e_var("name"))), BinOp::Concat, e_str("\" must not be numeric, empty, contain null bytes or any of the following characters \"=,;.[ \\t\\r\\n\\013\\014\"")), e_const("E_WARNING")])),
                            s_return(e_var("old")),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("elephc_web_session_set_name", vec![e_cast(CastType::String, e_var("name"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("old")),
        ])
        .build()
}

/// `session_status` — transcribed from the PHP form.
fn decl_fn_session_status() -> Stmt {
    function("session_status")
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_web_session_get_status", vec![])),
        ])
        .build()
}

/// `session_unset` — transcribed from the PHP form.
fn decl_fn_session_unset() -> Stmt {
    function("session_unset")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_SESSION", e_array(vec![])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `session_encode` — transcribed from the PHP form.
fn decl_fn_session_encode() -> Stmt {
    function("session_encode")
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_session_encode", vec![])),
        ])
        .build()
}

/// `session_decode` — transcribed from the PHP form.
fn decl_fn_session_decode() -> Stmt {
    function("session_decode")
        .param("data", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_session_decode", vec![e_var("data")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `session_save_path` — transcribed from the PHP form.
fn decl_fn_session_save_path() -> Stmt {
    function("session_save_path")
        .param_default("path", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("path"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE"))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_save_path(): Session save path cannot be changed when a session is active"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("old", e_call("elephc_web_session_get_save_path", vec![])),
            s_if(
                e_binop(e_var("path"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_save_path", vec![e_cast(CastType::String, e_var("path"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("old")),
        ])
        .build()
}

/// `session_regenerate_id` — transcribed from the PHP form.
fn decl_fn_session_regenerate_id() -> Stmt {
    function("session_regenerate_id")
        .param_default("delete_old", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_regenerate_id(): Session ID cannot be regenerated when there is no active session"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("old_id", e_call("elephc_web_session_get_id", vec![])),
            s_assign("save_path", e_call("elephc_web_session_get_save_path", vec![])),
            s_assign("__elephc_h", e_static_prop("__ElephcSessionState", "handler")),
            s_assign("__elephc_encoded", e_call("__elephc_session_encode", vec![])),
            s_if(
                e_binop(e_var("__elephc_encoded"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_current", e_cast(CastType::String, e_var("__elephc_encoded"))),
            s_if(
                e_var("delete_old"),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_var("__elephc_h"), "destroy", vec![e_var("old_id")])),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_call("elephc_web_session_destroy", vec![e_var("old_id"), e_var("save_path")]), BinOp::StrictNotEq, e_int(1)),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    ),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                    vec![
                        s_if(
                            e_not(e_method_call(e_var("__elephc_h"), "write", vec![e_var("old_id"), e_var("__elephc_current")])),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_call("__elephc_session_write_file", vec![e_var("old_id"), e_var("save_path"), e_var("__elephc_current")]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                ),
            ]),
            ),
            s_if(
                e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                vec![
                    s_expr(e_method_call(e_var("__elephc_h"), "close", vec![])),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_attempt", e_int(0)),
            s_do_while(vec![
                s_assign("new_id", e_call("elephc_web_session_create_id", vec![e_str("")])),
                s_if(
                    e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()),
                    vec![
                        s_if(
                            e_instance_of(e_var("__elephc_h"), "SessionIdInterface"),
                            vec![
                                s_assign("new_id", e_method_call(e_var("__elephc_h"), "create_sid", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_assign("__elephc_collision", e_binop(e_call("elephc_web_session_file_exists", vec![e_var("new_id"), e_var("save_path")]), BinOp::StrictEq, e_int(1))),
                s_if(
                    e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()),
                    vec![
                        s_if(
                            e_instance_of(e_var("__elephc_h"), "SessionUpdateTimestampHandlerInterface"),
                            vec![
                                s_assign("__elephc_collision", e_method_call(e_var("__elephc_h"), "validateId", vec![e_var("new_id")])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_assign("__elephc_attempt", e_binop(e_var("__elephc_attempt"), BinOp::Add, e_int(1))),
            ], e_binop(e_var("__elephc_collision"), BinOp::And, e_binop(e_var("__elephc_attempt"), BinOp::Lt, e_int(3)))),
            s_if(
                e_binop(e_binop(e_var("new_id"), BinOp::StrictEq, e_str("")), BinOp::Or, e_var("__elephc_collision")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("elephc_web_session_set_id", vec![e_var("new_id")])),
            s_if(
                e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                vec![
                    s_if(
                        e_not(e_method_call(e_var("__elephc_h"), "open", vec![e_var("save_path"), e_call("elephc_web_session_get_name", vec![])])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_var("__elephc_h"), "read", vec![e_var("new_id")]), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_expr(e_method_call(e_var("__elephc_h"), "close", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_static_prop_assign("__ElephcSessionState", "snapshot", e_str("")),
                    s_static_prop_assign("__ElephcSessionState", "snapshotValid", e_bool(false)),
                ],
                vec![],
                Some(vec![
                s_expr(e_call("__elephc_session_read_file", vec![e_var("new_id"), e_var("save_path"), e_int(0)])),
            ]),
            ),
            s_if(
                e_binop(e_call("elephc_web_session_get_use_cookies", vec![]), BinOp::StrictEq, e_int(1)),
                vec![
                    s_if(
                        e_not(e_call("__elephc_session_send_cookie", vec![])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `session_create_id` — transcribed from the PHP form.
fn decl_fn_session_create_id() -> Stmt {
    function("session_create_id")
        .param_default("prefix", TypeExpr::Str, e_str(""))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_call("strpos", vec![e_var("prefix"), e_str("\0")]), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("session_create_id(): Argument #1 ($prefix) must not contain any null bytes")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_call("strlen", vec![e_var("prefix")]), BinOp::Gt, e_int(256))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("session_create_id(): Argument #1 ($prefix) cannot be longer than 256 characters")])),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_created_id", e_call("elephc_web_session_create_id", vec![e_var("prefix")])),
            s_if(
                e_binop(e_binop(e_var("__elephc_created_id"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_var("prefix"), BinOp::StrictNotEq, e_str(""))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_create_id(): Prefix cannot contain special characters. Only the A-Z, a-z, 0-9, \"-\", and \",\" characters are allowed"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_var("__elephc_created_id")),
        ])
        .build()
}

/// `session_gc` — transcribed from the PHP form.
fn decl_fn_session_gc() -> Stmt {
    function("session_gc")
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_gc(): Session cannot be garbage collected when there is no active session"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_h", e_static_prop("__ElephcSessionState", "handler")),
            s_assign("maxlifetime", e_call("elephc_web_session_get_gc_maxlifetime", vec![])),
            s_if(
                e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                vec![
                    s_return(e_method_call(e_var("__elephc_h"), "gc", vec![e_var("maxlifetime")])),
                ],
                vec![],
                None,
            ),
            s_assign("save_path", e_call("elephc_web_session_get_save_path", vec![])),
            s_return(e_call("elephc_web_session_gc", vec![e_var("save_path"), e_var("maxlifetime")])),
        ])
        .build()
}

/// `session_abort` — transcribed from the PHP form.
fn decl_fn_session_abort() -> Stmt {
    function("session_abort")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("id", e_call("elephc_web_session_get_id", vec![])),
            s_assign("save_path", e_call("elephc_web_session_get_save_path", vec![])),
            s_assign("__elephc_h", e_static_prop("__ElephcSessionState", "handler")),
            s_if(
                e_binop(e_binop(e_var("__elephc_h"), BinOp::StrictNotEq, e_null()), BinOp::And, e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface")),
                vec![
                    s_expr(e_method_call(e_var("__elephc_h"), "close", vec![])),
                ],
                vec![],
                Some(vec![
                s_expr(e_call("elephc_web_session_abort", vec![e_var("id"), e_var("save_path")])),
            ]),
            ),
            s_expr(e_call("elephc_web_session_set_status", vec![e_const("PHP_SESSION_NONE")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `session_reset` — transcribed from the PHP form.
fn decl_fn_session_reset() -> Stmt {
    function("session_reset")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictNotEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_h", e_static_prop("__ElephcSessionState", "handler")),
            s_assign("_SESSION", e_array(vec![])),
            s_if(
                e_instance_of(e_var("__elephc_h"), "SessionHandlerInterface"),
                vec![
                    s_assign("id", e_call("elephc_web_session_get_id", vec![])),
                    s_assign("raw", e_cast(CastType::String, e_method_call(e_var("__elephc_h"), "read", vec![e_var("id")]))),
                    s_if(
                        e_binop(e_var("raw"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_expr(e_call("__elephc_session_decode", vec![e_var("raw")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                Some(vec![
                s_assign("raw", e_call("__elephc_session_snapshot_bytes", vec![])),
                s_if(
                    e_binop(e_var("raw"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_expr(e_call("__elephc_session_decode", vec![e_var("raw")])),
                    ],
                    vec![],
                    None,
                ),
            ]),
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `session_cache_limiter` — transcribed from the PHP form.
fn decl_fn_session_cache_limiter() -> Stmt {
    function("session_cache_limiter")
        .param_default("value", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("value"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("old", e_call("elephc_web_session_get_cache_limiter", vec![])),
            s_if(
                e_binop(e_var("value"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_cache_limiter", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("old")),
        ])
        .build()
}

/// `session_cache_expire` — transcribed from the PHP form.
fn decl_fn_session_cache_expire() -> Stmt {
    function("session_cache_expire")
        .param_default("value", t_nullable(TypeExpr::Int), e_null())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("value"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("old", e_call("elephc_web_session_get_cache_expire", vec![])),
            s_if(
                e_binop(e_var("value"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_web_session_set_cache_expire", vec![e_cast(CastType::Int, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("old")),
        ])
        .build()
}

/// `session_get_cookie_params` — transcribed from the PHP form.
fn decl_fn_session_get_cookie_params() -> Stmt {
    function("session_get_cookie_params")
        .returns(t_array())
        .body(vec![
            s_assign("__elephc_cookie_params", e_array_assoc(vec![(e_str("lifetime"), e_call("elephc_web_session_get_cookie_lifetime", vec![])), (e_str("path"), e_call("elephc_web_session_get_cookie_path", vec![])), (e_str("domain"), e_call("elephc_web_session_get_cookie_domain", vec![])), (e_str("secure"), e_cast(CastType::Bool, e_call("elephc_web_session_get_cookie_secure", vec![]))), (e_str("httponly"), e_cast(CastType::Bool, e_call("elephc_web_session_get_cookie_httponly", vec![]))), (e_str("samesite"), e_call("elephc_web_session_get_cookie_samesite", vec![]))])),
            s_if(
                e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80500)),
                vec![
                    s_assign("__elephc_cookie_params", e_array_assoc(vec![(e_str("lifetime"), e_call("elephc_web_session_get_cookie_lifetime", vec![])), (e_str("path"), e_call("elephc_web_session_get_cookie_path", vec![])), (e_str("domain"), e_call("elephc_web_session_get_cookie_domain", vec![])), (e_str("secure"), e_cast(CastType::Bool, e_call("elephc_web_session_get_cookie_secure", vec![]))), (e_str("partitioned"), e_cast(CastType::Bool, e_call("elephc_web_session_get_cookie_partitioned", vec![]))), (e_str("httponly"), e_cast(CastType::Bool, e_call("elephc_web_session_get_cookie_httponly", vec![]))), (e_str("samesite"), e_call("elephc_web_session_get_cookie_samesite", vec![]))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("__elephc_cookie_params")),
        ])
        .build()
}

/// `session_set_cookie_params` — transcribed from the PHP form.
fn decl_fn_session_set_cookie_params() -> Stmt {
    function("session_set_cookie_params")
        .variadic("args", Some(t_mixed()))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_use_cookies", vec![]), BinOp::StrictEq, e_int(0)),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_set_cookie_params(): Session cookies cannot be used when session.use_cookies is disabled"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_set_cookie_params(): Session cookie parameters cannot be changed when a session is active"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_scp_cl", e_call("elephc_web_session_get_cookie_lifetime", vec![])),
            s_assign("__elephc_scp_cp", e_call("elephc_web_session_get_cookie_path", vec![])),
            s_assign("__elephc_scp_cd", e_call("elephc_web_session_get_cookie_domain", vec![])),
            s_assign("__elephc_scp_cs", e_call("elephc_web_session_get_cookie_secure", vec![])),
            s_assign("__elephc_scp_cpart", e_call("elephc_web_session_get_cookie_partitioned", vec![])),
            s_assign("__elephc_scp_ch", e_call("elephc_web_session_get_cookie_httponly", vec![])),
            s_assign("__elephc_scp_css", e_call("elephc_web_session_get_cookie_samesite", vec![])),
            s_if(
                e_binop(e_binop(e_call("count", vec![e_var("args")]), BinOp::StrictEq, e_int(1)), BinOp::And, e_call("is_array", vec![e_index(e_var("args"), e_int(0))])),
                vec![
                    s_array_assign("_ENV", e_str("_elephc_scp"), e_index(e_var("args"), e_int(0))),
                    s_assign("__elephc_scp_found", e_int(0)),
                    s_foreach(e_index(e_var("_ENV"), e_str("_elephc_scp")), Some("__elephc_scp_key"), "__elephc_scp_value", vec![
                        s_if(
                            e_call("is_int", vec![e_var("__elephc_scp_key")]),
                            vec![
                                s_expr(e_call("trigger_error", vec![e_str("session_set_cookie_params(): Argument #1 ($lifetime_or_options) cannot contain numeric keys"), e_const("E_WARNING")])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("__elephc_scp_normalized", e_call("strtolower", vec![e_cast(CastType::String, e_var("__elephc_scp_key"))])),
                        s_if(
                            e_binop(e_var("__elephc_scp_normalized"), BinOp::StrictEq, e_str("lifetime")),
                            vec![
                                s_assign("__elephc_scp_cl", e_cast(CastType::Int, e_var("__elephc_scp_value"))),
                                s_expr(e_post_inc("__elephc_scp_found")),
                            ],
                            vec![
                            (e_binop(e_var("__elephc_scp_normalized"), BinOp::StrictEq, e_str("path")), vec![
                                s_assign("__elephc_scp_cp", e_cast(CastType::String, e_var("__elephc_scp_value"))),
                                s_expr(e_post_inc("__elephc_scp_found")),
                            ]),
                            (e_binop(e_var("__elephc_scp_normalized"), BinOp::StrictEq, e_str("domain")), vec![
                                s_assign("__elephc_scp_cd", e_cast(CastType::String, e_var("__elephc_scp_value"))),
                                s_expr(e_post_inc("__elephc_scp_found")),
                            ]),
                            (e_binop(e_var("__elephc_scp_normalized"), BinOp::StrictEq, e_str("secure")), vec![
                                s_assign("__elephc_scp_cs", e_ternary(e_var("__elephc_scp_value"), e_int(1), e_int(0))),
                                s_expr(e_post_inc("__elephc_scp_found")),
                            ]),
                            (e_binop(e_binop(e_var("__elephc_scp_normalized"), BinOp::StrictEq, e_str("partitioned")), BinOp::And, e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80500))), vec![
                                s_assign("__elephc_scp_cpart", e_ternary(e_var("__elephc_scp_value"), e_int(1), e_int(0))),
                                s_expr(e_post_inc("__elephc_scp_found")),
                            ]),
                            (e_binop(e_var("__elephc_scp_normalized"), BinOp::StrictEq, e_str("httponly")), vec![
                                s_assign("__elephc_scp_ch", e_ternary(e_var("__elephc_scp_value"), e_int(1), e_int(0))),
                                s_expr(e_post_inc("__elephc_scp_found")),
                            ]),
                            (e_binop(e_var("__elephc_scp_normalized"), BinOp::StrictEq, e_str("samesite")), vec![
                                s_assign("__elephc_scp_css", e_cast(CastType::String, e_var("__elephc_scp_value"))),
                                s_expr(e_post_inc("__elephc_scp_found")),
                            ]),
                        ],
                            Some(vec![
                            s_expr(e_call("trigger_error", vec![e_binop(e_binop(e_str("session_set_cookie_params(): Argument #1 ($lifetime_or_options) contains an unrecognized key \""), BinOp::Concat, e_cast(CastType::String, e_var("__elephc_scp_key"))), BinOp::Concat, e_str("\"")), e_const("E_WARNING")])),
                        ]),
                        ),
                    ]),
                    s_expr(e_call("unset", vec![e_index(e_var("_ENV"), e_str("_elephc_scp"))])),
                    s_if(
                        e_binop(e_var("__elephc_scp_found"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("session_set_cookie_params(): Argument #1 ($lifetime_or_options) must contain at least 1 valid key")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_call("count", vec![e_var("args")]), BinOp::Gt, e_int(0)),
                    vec![
                        s_assign("__elephc_scp_cl", e_cast(CastType::Int, e_index(e_var("args"), e_int(0)))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("count", vec![e_var("args")]), BinOp::Gt, e_int(1)),
                    vec![
                        s_assign("__elephc_scp_cp", e_cast(CastType::String, e_index(e_var("args"), e_int(1)))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("count", vec![e_var("args")]), BinOp::Gt, e_int(2)),
                    vec![
                        s_assign("__elephc_scp_cd", e_cast(CastType::String, e_index(e_var("args"), e_int(2)))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("count", vec![e_var("args")]), BinOp::Gt, e_int(3)),
                    vec![
                        s_assign("__elephc_scp_cs", e_cast(CastType::Int, e_index(e_var("args"), e_int(3)))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("count", vec![e_var("args")]), BinOp::Gt, e_int(4)),
                    vec![
                        s_assign("__elephc_scp_ch", e_cast(CastType::Int, e_index(e_var("args"), e_int(4)))),
                    ],
                    vec![],
                    None,
                ),
            ]),
            ),
            s_if(
                e_binop(e_var("__elephc_scp_cl"), BinOp::Lt, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("__elephc_scp_css"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_var("__elephc_scp_css"), BinOp::StrictNotEq, e_str("Strict"))), BinOp::And, e_binop(e_var("__elephc_scp_css"), BinOp::StrictNotEq, e_str("Lax"))), BinOp::And, e_binop(e_var("__elephc_scp_css"), BinOp::StrictNotEq, e_str("None"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_var("__elephc_scp_cl"), e_var("__elephc_scp_cp"), e_var("__elephc_scp_cd"), e_var("__elephc_scp_cs"), e_var("__elephc_scp_cpart"), e_var("__elephc_scp_ch"), e_var("__elephc_scp_css")])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `session_commit` — transcribed from the PHP form.
fn decl_fn_session_commit() -> Stmt {
    function("session_commit")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_call("session_write_close", vec![])),
        ])
        .build()
}

/// `session_register_shutdown` — transcribed from the PHP form.
fn decl_fn_session_register_shutdown() -> Stmt {
    function("session_register_shutdown")
        .returns(TypeExpr::Void)
        .body(vec![
            s_static_prop_assign("__ElephcSessionState", "shutdown", e_bool(true)),
        ])
        .build()
}

/// `session_module_name` — transcribed from the PHP form.
fn decl_fn_session_module_name() -> Stmt {
    function("session_module_name")
        .param_default("module", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("__elephc_old_module", e_ternary(e_binop(e_static_prop("__ElephcSessionState", "handler"), BinOp::StrictNotEq, e_null()), e_str("user"), e_str("files"))),
            s_if(
                e_binop(e_var("module"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_var("__elephc_old_module")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("strtolower", vec![e_var("module")]), BinOp::StrictNotEq, e_str("files")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_static_prop_assign("__ElephcSessionState", "handler", e_null()),
            s_return(e_var("__elephc_old_module")),
        ])
        .build()
}

/// `session_set_save_handler` — transcribed from the PHP form.
fn decl_fn_session_set_save_handler() -> Stmt {
    function("session_set_save_handler")
        .param_untyped_default("handler_or_open", e_null())
        .param_untyped_default("register_or_close", e_bool(true))
        .param_untyped_default("read", e_null())
        .param_untyped_default("write", e_null())
        .param_untyped_default("destroy", e_null())
        .param_untyped_default("gc", e_null())
        .param_untyped_default("create_sid", e_null())
        .param_untyped_default("validate_id", e_null())
        .param_untyped_default("update_timestamp", e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("handler_or_open"), "SessionHandlerInterface"),
                vec![
                    s_static_prop_assign("__ElephcSessionState", "handler", e_var("handler_or_open")),
                    s_static_prop_assign("__ElephcSessionState", "shutdown", e_cast(CastType::Bool, e_var("register_or_close"))),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("handler_or_open"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("register_or_close"), BinOp::StrictEq, e_null())), BinOp::Or, e_binop(e_var("read"), BinOp::StrictEq, e_null())), BinOp::Or, e_binop(e_var("write"), BinOp::StrictEq, e_null())), BinOp::Or, e_binop(e_var("destroy"), BinOp::StrictEq, e_null())), BinOp::Or, e_binop(e_var("gc"), BinOp::StrictEq, e_null())),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_set_save_handler(): Providing individual callbacks instead of an object implementing SessionHandlerInterface is deprecated"), e_const("E_DEPRECATED")])),
                ],
                vec![],
                None,
            ),
            s_static_prop_assign("__ElephcSessionState", "handler", e_new("__ElephcCallableSessionHandler", vec![e_var("handler_or_open"), e_var("register_or_close"), e_var("read"), e_var("write"), e_var("destroy"), e_var("gc"), e_var("create_sid"), e_var("validate_id"), e_var("update_timestamp")])),
            s_static_prop_assign("__ElephcSessionState", "shutdown", e_bool(true)),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_session_encode` — transcribed from the PHP form.
fn decl_fn_elephc_session_encode() -> Stmt {
    function("__elephc_session_encode")
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("__elephc_sh", e_call("elephc_web_session_get_serialize_handler", vec![])),
            s_if(
                e_binop(e_var("__elephc_sh"), BinOp::StrictEq, e_str("php_serialize")),
                vec![
                    s_return(e_call("serialize", vec![e_var("_SESSION")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__elephc_sh"), BinOp::StrictEq, e_str("php_binary")),
                vec![
                    s_assign("out", e_str("")),
                    s_foreach(e_var("_SESSION"), Some("k"), "v", vec![
                        s_assign("ks", e_cast(CastType::String, e_var("k"))),
                        s_if(
                            e_binop(e_call("strlen", vec![e_var("ks")]), BinOp::Gt, e_int(127)),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_call("chr", vec![e_call("strlen", vec![e_var("ks")])]), BinOp::Concat, e_var("ks")), BinOp::Concat, e_call("serialize", vec![e_var("v")])))),
                    ]),
                    s_return(e_var("out")),
                ],
                vec![],
                None,
            ),
            s_assign("out", e_str("")),
            s_foreach(e_var("_SESSION"), Some("k"), "v", vec![
                s_assign("ks", e_cast(CastType::String, e_var("k"))),
                s_if(
                    e_binop(e_call("strpos", vec![e_var("ks"), e_str("|")]), BinOp::StrictNotEq, e_bool(false)),
                    vec![
                        s_if(
                            e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80500)),
                            vec![
                                s_expr(e_call("trigger_error", vec![e_binop(e_binop(e_str("session_encode(): Failed to write session data. Data contains invalid key \""), BinOp::Concat, e_var("ks")), BinOp::Concat, e_str("\"")), e_const("E_WARNING")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_var("ks"), BinOp::Concat, e_str("|")), BinOp::Concat, e_call("serialize", vec![e_var("v")])))),
            ]),
            s_return(e_var("out")),
        ])
        .build()
}

/// `__elephc_session_decode` — transcribed from the PHP form.
fn decl_fn_elephc_session_decode() -> Stmt {
    function("__elephc_session_decode")
        .param("raw", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("__elephc_sh", e_call("elephc_web_session_get_serialize_handler", vec![])),
            s_if(
                e_binop(e_var("__elephc_sh"), BinOp::StrictEq, e_str("php_serialize")),
                vec![
                    s_assign("__elephc_decoded", e_call("unserialize", vec![e_var("raw")])),
                    s_if(
                        e_call("is_array", vec![e_var("__elephc_decoded")]),
                        vec![
                            s_assign("_SESSION", e_array(vec![])),
                            s_foreach(e_var("__elephc_decoded"), Some("__elephc_key"), "__elephc_value", vec![
                                s_array_assign("_SESSION", e_var("__elephc_key"), e_var("__elephc_value")),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_dec_len", e_call("strlen", vec![e_var("raw")])),
            s_assign("__elephc_dec_ptr", e_call("__elephc_session_stage_bytes", vec![e_var("raw")])),
            s_if(
                e_binop(e_var("__elephc_sh"), BinOp::StrictEq, e_str("php_binary")),
                vec![
                    s_assign("count", e_call("elephc_web_session_count_entries_bin_bytes", vec![e_var("__elephc_dec_ptr"), e_var("__elephc_dec_len")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("__elephc_key_ptr", e_call("elephc_web_session_entry_key_bin_bytes", vec![e_var("__elephc_dec_ptr"), e_var("__elephc_dec_len"), e_var("i")])),
                        s_assign("key", e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_key_ptr"), e_call("elephc_web_session_data_len", vec![])])),
                        s_assign("__elephc_val_ptr", e_call("elephc_web_session_entry_value_bin_bytes", vec![e_var("__elephc_dec_ptr"), e_var("__elephc_dec_len"), e_var("i")])),
                        s_assign("val", e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_val_ptr"), e_call("elephc_web_session_data_len", vec![])])),
                        s_array_assign("_SESSION", e_var("key"), e_call("unserialize", vec![e_var("val")])),
                    ]),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("count", e_call("elephc_web_session_count_entries_bytes", vec![e_var("__elephc_dec_ptr"), e_var("__elephc_dec_len")])),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("__elephc_key_ptr", e_call("elephc_web_session_entry_key_bytes", vec![e_var("__elephc_dec_ptr"), e_var("__elephc_dec_len"), e_var("i")])),
                s_assign("key", e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_key_ptr"), e_call("elephc_web_session_data_len", vec![])])),
                s_assign("__elephc_val_ptr", e_call("elephc_web_session_entry_value_bytes", vec![e_var("__elephc_dec_ptr"), e_var("__elephc_dec_len"), e_var("i")])),
                s_assign("val", e_call("__elephc_session_copy_bytes", vec![e_var("__elephc_val_ptr"), e_call("elephc_web_session_data_len", vec![])])),
                s_array_assign("_SESSION", e_var("key"), e_call("unserialize", vec![e_var("val")])),
            ]),
        ])
        .build()
}

/// `__elephc_session_send_cookie` — transcribed from the PHP form.
fn decl_fn_elephc_session_send_cookie() -> Stmt {
    function("__elephc_session_send_cookie")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("name", e_call("elephc_web_session_get_name", vec![])),
            s_assign("id", e_call("elephc_web_session_get_id", vec![])),
            s_assign("lifetime", e_call("elephc_web_session_get_cookie_lifetime", vec![])),
            s_assign("path", e_call("elephc_web_session_get_cookie_path", vec![])),
            s_assign("domain", e_call("elephc_web_session_get_cookie_domain", vec![])),
            s_assign("secure", e_cast(CastType::Bool, e_call("elephc_web_session_get_cookie_secure", vec![]))),
            s_assign("partitioned", e_cast(CastType::Bool, e_call("elephc_web_session_get_cookie_partitioned", vec![]))),
            s_assign("httponly", e_cast(CastType::Bool, e_call("elephc_web_session_get_cookie_httponly", vec![]))),
            s_assign("samesite", e_call("elephc_web_session_get_cookie_samesite", vec![])),
            s_if(
                e_binop(e_var("partitioned"), BinOp::And, e_not(e_var("secure"))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("session_start(): Partitioned session cookie cannot be used without also configuring it as secure"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("cookie", e_binop(e_binop(e_var("name"), BinOp::Concat, e_str("=")), BinOp::Concat, e_var("id"))),
            s_if(
                e_binop(e_var("lifetime"), BinOp::Gt, e_int(0)),
                vec![
                    s_assign("cookie", e_binop(e_var("cookie"), BinOp::Concat, e_binop(e_binop(e_str("; expires="), BinOp::Concat, e_call("gmdate", vec![e_str("D, d-M-Y H:i:s"), e_binop(e_call("time", vec![]), BinOp::Add, e_var("lifetime"))])), BinOp::Concat, e_str(" GMT")))),
                    s_assign("cookie", e_binop(e_var("cookie"), BinOp::Concat, e_binop(e_str("; Max-Age="), BinOp::Concat, e_var("lifetime")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("path"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_assign("cookie", e_binop(e_var("cookie"), BinOp::Concat, e_binop(e_str("; path="), BinOp::Concat, e_var("path")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("domain"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_assign("cookie", e_binop(e_var("cookie"), BinOp::Concat, e_binop(e_str("; domain="), BinOp::Concat, e_var("domain")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("secure"),
                vec![
                    s_assign("cookie", e_binop(e_var("cookie"), BinOp::Concat, e_str("; secure"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("partitioned"),
                vec![
                    s_assign("cookie", e_binop(e_var("cookie"), BinOp::Concat, e_str("; Partitioned"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("httponly"),
                vec![
                    s_assign("cookie", e_binop(e_var("cookie"), BinOp::Concat, e_str("; HttpOnly"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("samesite"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_assign("cookie", e_binop(e_var("cookie"), BinOp::Concat, e_binop(e_str("; SameSite="), BinOp::Concat, e_var("samesite")))),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("header", vec![e_binop(e_str("Set-Cookie: "), BinOp::Concat, e_var("cookie")), e_bool(false)])),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_session_send_cache_headers` — transcribed from the PHP form.
fn decl_fn_elephc_session_send_cache_headers() -> Stmt {
    function("__elephc_session_send_cache_headers")
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("limiter", e_call("elephc_web_session_get_cache_limiter", vec![])),
            s_if(
                e_binop(e_var("limiter"), BinOp::StrictEq, e_str("nocache")),
                vec![
                    s_expr(e_call("header", vec![e_str("Expires: Thu, 19 Nov 1981 08:52:00 GMT")])),
                    s_expr(e_call("header", vec![e_str("Cache-Control: no-store, no-cache, must-revalidate")])),
                    s_expr(e_call("header", vec![e_str("Pragma: no-cache")])),
                ],
                vec![
                (e_binop(e_var("limiter"), BinOp::StrictEq, e_str("public")), vec![
                    s_assign("expire", e_call("elephc_web_session_get_cache_expire", vec![])),
                    s_expr(e_call("header", vec![e_binop(e_binop(e_str("Expires: "), BinOp::Concat, e_call("gmdate", vec![e_str("D, d M Y H:i:s"), e_binop(e_call("time", vec![]), BinOp::Add, e_binop(e_var("expire"), BinOp::Mul, e_int(60)))])), BinOp::Concat, e_str(" GMT"))])),
                    s_expr(e_call("header", vec![e_binop(e_str("Cache-Control: public, max-age="), BinOp::Concat, e_binop(e_var("expire"), BinOp::Mul, e_int(60)))])),
                    s_expr(e_call("header", vec![e_binop(e_binop(e_str("Last-Modified: "), BinOp::Concat, e_call("gmdate", vec![e_str("D, d M Y H:i:s"), e_call("time", vec![])])), BinOp::Concat, e_str(" GMT"))])),
                ]),
                (e_binop(e_var("limiter"), BinOp::StrictEq, e_str("private")), vec![
                    s_assign("expire", e_call("elephc_web_session_get_cache_expire", vec![])),
                    s_expr(e_call("header", vec![e_str("Expires: Thu, 19 Nov 1981 08:52:00 GMT")])),
                    s_expr(e_call("header", vec![e_binop(e_str("Cache-Control: private, max-age="), BinOp::Concat, e_binop(e_var("expire"), BinOp::Mul, e_int(60)))])),
                    s_expr(e_call("header", vec![e_binop(e_binop(e_str("Last-Modified: "), BinOp::Concat, e_call("gmdate", vec![e_str("D, d M Y H:i:s"), e_call("time", vec![])])), BinOp::Concat, e_str(" GMT"))])),
                ]),
                (e_binop(e_var("limiter"), BinOp::StrictEq, e_str("private_no_expire")), vec![
                    s_assign("expire", e_call("elephc_web_session_get_cache_expire", vec![])),
                    s_expr(e_call("header", vec![e_binop(e_str("Cache-Control: private, max-age="), BinOp::Concat, e_binop(e_var("expire"), BinOp::Mul, e_int(60)))])),
                    s_expr(e_call("header", vec![e_binop(e_binop(e_str("Last-Modified: "), BinOp::Concat, e_call("gmdate", vec![e_str("D, d M Y H:i:s"), e_call("time", vec![])])), BinOp::Concat, e_str(" GMT"))])),
                ]),
            ],
                None,
            ),
        ])
        .build()
}

/// `__elephc_ini_session_keys` — transcribed from the PHP form.
fn decl_fn_elephc_ini_session_keys() -> Stmt {
    function("__elephc_ini_session_keys")
        .returns(t_array())
        .body(vec![
            s_if(
                e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::Lt, e_int(80500)),
                vec![
                    s_return(e_array(vec![e_str("session.name"), e_str("session.save_path"), e_str("session.save_handler"), e_str("session.cache_limiter"), e_str("session.cache_expire"), e_str("session.cookie_lifetime"), e_str("session.cookie_path"), e_str("session.cookie_domain"), e_str("session.cookie_secure"), e_str("session.cookie_httponly"), e_str("session.cookie_samesite"), e_str("session.use_cookies"), e_str("session.use_strict_mode"), e_str("session.use_only_cookies"), e_str("session.lazy_write"), e_str("session.use_trans_sid"), e_str("session.referer_check"), e_str("session.trans_sid_tags"), e_str("session.trans_sid_hosts"), e_str("session.serialize_handler"), e_str("session.gc_probability"), e_str("session.gc_divisor"), e_str("session.gc_maxlifetime"), e_str("session.sid_length"), e_str("session.sid_bits_per_character"), e_str("session.auto_start"), e_str("session.upload_progress.enabled"), e_str("session.upload_progress.cleanup"), e_str("session.upload_progress.prefix"), e_str("session.upload_progress.name"), e_str("session.upload_progress.freq"), e_str("session.upload_progress.min_freq")])),
                ],
                vec![],
                None,
            ),
            s_return(e_array(vec![e_str("session.name"), e_str("session.save_path"), e_str("session.save_handler"), e_str("session.cache_limiter"), e_str("session.cache_expire"), e_str("session.cookie_lifetime"), e_str("session.cookie_path"), e_str("session.cookie_domain"), e_str("session.cookie_secure"), e_str("session.cookie_partitioned"), e_str("session.cookie_httponly"), e_str("session.cookie_samesite"), e_str("session.use_cookies"), e_str("session.use_strict_mode"), e_str("session.use_only_cookies"), e_str("session.lazy_write"), e_str("session.use_trans_sid"), e_str("session.referer_check"), e_str("session.trans_sid_tags"), e_str("session.trans_sid_hosts"), e_str("session.serialize_handler"), e_str("session.gc_probability"), e_str("session.gc_divisor"), e_str("session.gc_maxlifetime"), e_str("session.sid_length"), e_str("session.sid_bits_per_character"), e_str("session.auto_start"), e_str("session.upload_progress.enabled"), e_str("session.upload_progress.cleanup"), e_str("session.upload_progress.prefix"), e_str("session.upload_progress.name"), e_str("session.upload_progress.freq"), e_str("session.upload_progress.min_freq")])),
        ])
        .build()
}

/// `__elephc_is_session_ini` — transcribed from the PHP form.
fn decl_fn_elephc_is_session_ini() -> Stmt {
    function("__elephc_is_session_ini")
        .param("option", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_foreach(e_call("__elephc_ini_session_keys", vec![]), None, "__elephc_ik", vec![
                s_if(
                    e_binop(e_var("__elephc_ik"), BinOp::StrictEq, e_var("option")),
                    vec![
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `__elephc_ini_get_raw` — transcribed from the PHP form.
fn decl_fn_elephc_ini_get_raw() -> Stmt {
    function("__elephc_ini_get_raw")
        .param("key", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.name")),
                vec![
                    s_return(e_call("elephc_web_session_get_name", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.save_path")),
                vec![
                    s_return(e_call("elephc_web_session_get_save_path", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.save_handler")),
                vec![
                    s_return(e_cast(CastType::String, e_call("session_module_name", vec![]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cache_limiter")),
                vec![
                    s_return(e_call("elephc_web_session_get_cache_limiter", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cookie_path")),
                vec![
                    s_return(e_call("elephc_web_session_get_cookie_path", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cookie_domain")),
                vec![
                    s_return(e_call("elephc_web_session_get_cookie_domain", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cookie_samesite")),
                vec![
                    s_return(e_call("elephc_web_session_get_cookie_samesite", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.serialize_handler")),
                vec![
                    s_return(e_call("elephc_web_session_get_serialize_handler", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.referer_check")),
                vec![
                    s_return(e_call("elephc_web_session_get_referer_check", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.trans_sid_tags")),
                vec![
                    s_return(e_call("elephc_web_session_get_trans_sid_tags", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.trans_sid_hosts")),
                vec![
                    s_return(e_call("elephc_web_session_get_trans_sid_hosts", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.upload_progress.prefix")),
                vec![
                    s_return(e_call("elephc_web_session_get_upload_progress_prefix", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.upload_progress.name")),
                vec![
                    s_return(e_call("elephc_web_session_get_upload_progress_name", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.upload_progress.freq")),
                vec![
                    s_return(e_call("elephc_web_session_get_upload_progress_freq", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.upload_progress.min_freq")),
                vec![
                    s_return(e_call("elephc_web_session_get_upload_progress_min_freq", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cache_expire")),
                vec![
                    s_return(e_cast(CastType::String, e_call("elephc_web_session_get_cache_expire", vec![]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cookie_lifetime")),
                vec![
                    s_return(e_cast(CastType::String, e_call("elephc_web_session_get_cookie_lifetime", vec![]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.gc_probability")),
                vec![
                    s_return(e_cast(CastType::String, e_call("elephc_web_session_get_gc_probability", vec![]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.gc_divisor")),
                vec![
                    s_return(e_cast(CastType::String, e_call("elephc_web_session_get_gc_divisor", vec![]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.gc_maxlifetime")),
                vec![
                    s_return(e_cast(CastType::String, e_call("elephc_web_session_get_gc_maxlifetime", vec![]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.sid_length")),
                vec![
                    s_return(e_cast(CastType::String, e_call("elephc_web_session_get_sid_length", vec![]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.sid_bits_per_character")),
                vec![
                    s_return(e_cast(CastType::String, e_call("elephc_web_session_get_sid_bits_per_character", vec![]))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cookie_secure")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_cookie_secure", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cookie_partitioned")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_cookie_partitioned", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.cookie_httponly")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_cookie_httponly", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.use_strict_mode")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_strict_mode", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.use_cookies")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_use_cookies", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.use_only_cookies")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_use_only_cookies", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.lazy_write")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_lazy_write", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.use_trans_sid")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_use_trans_sid", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.upload_progress.enabled")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_upload_progress_enabled", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.upload_progress.cleanup")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_upload_progress_cleanup", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("key"), BinOp::StrictEq, e_str("session.auto_start")),
                vec![
                    s_return(e_ternary(e_binop(e_call("elephc_web_session_get_auto_start", vec![]), BinOp::StrictEq, e_int(1)), e_str("1"), e_str(""))),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `ini_get` — transcribed from the PHP form.
fn decl_fn_ini_get() -> Stmt {
    function("ini_get")
        .param("option", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("__elephc_oc", e_call("__elephc_opcache_ini_string", vec![e_var("option")])),
            s_if(
                e_binop(e_var("__elephc_oc"), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_return(e_var("__elephc_oc")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("__elephc_is_session_ini", vec![e_var("option")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_ini_get_raw", vec![e_var("option")])),
        ])
        .build()
}

/// `__elephc_session_ini_perdir` — transcribed from the PHP form.
fn decl_fn_elephc_session_ini_perdir() -> Stmt {
    function("__elephc_session_ini_perdir")
        .param("option", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.auto_start")), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.enabled"))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.cleanup"))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.prefix"))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.name"))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.freq"))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.min_freq")))),
        ])
        .build()
}

/// `__elephc_session_ini_bool` — transcribed from the PHP form.
fn decl_fn_elephc_session_ini_bool() -> Stmt {
    function("__elephc_session_ini_bool")
        .param("value", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_assign("__elephc_bv", e_call("strtolower", vec![e_call("trim", vec![e_var("value")])])),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("__elephc_bv"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("__elephc_bv"), BinOp::StrictEq, e_str("0"))), BinOp::Or, e_binop(e_var("__elephc_bv"), BinOp::StrictEq, e_str("off"))), BinOp::Or, e_binop(e_var("__elephc_bv"), BinOp::StrictEq, e_str("no"))), BinOp::Or, e_binop(e_var("__elephc_bv"), BinOp::StrictEq, e_str("false"))), BinOp::Or, e_binop(e_var("__elephc_bv"), BinOp::StrictEq, e_str("none"))),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_return(e_ternary(e_var("value"), e_int(1), e_int(0))),
        ])
        .build()
}

/// `__elephc_session_name_valid` — transcribed from the PHP form.
fn decl_fn_elephc_session_name_valid() -> Stmt {
    function("__elephc_session_name_valid")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_call("strpos", vec![e_var("name"), e_str("\0")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_call("is_numeric", vec![e_var("name")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_foreach(e_array(vec![e_str("="), e_str(","), e_str(";"), e_str("."), e_str("["), e_str(" "), e_str("\t"), e_str("\r"), e_str("\n"), e_str("\u{b}"), e_str("\u{c}")]), None, "__elephc_nc", vec![
                s_if(
                    e_binop(e_call("strpos", vec![e_var("name"), e_var("__elephc_nc")]), BinOp::StrictNotEq, e_bool(false)),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `ini_set` — transcribed from the PHP form.
fn decl_fn_ini_set() -> Stmt {
    function("ini_set")
        .param("option", TypeExpr::Str)
        .param_untyped("value")
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("old", e_str("")),
            s_if(
                e_binop(e_call("__elephc_opcache_ini_string", vec![e_var("option")]), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("__elephc_is_session_ini", vec![e_var("option")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("__elephc_session_ini_perdir", vec![e_var("option")]),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_web_session_get_status", vec![]), BinOp::StrictEq, e_const("PHP_SESSION_ACTIVE")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.name")), BinOp::And, e_not(e_call("__elephc_session_name_valid", vec![e_cast(CastType::String, e_var("value"))]))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.serialize_handler")), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str("php"))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str("php_serialize"))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str("php_binary"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.save_handler")), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str("files"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_samesite")), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str(""))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str("Strict"))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str("Lax"))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str("None"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_lifetime")), BinOp::And, e_binop(e_cast(CastType::Int, e_var("value")), BinOp::Lt, e_int(0))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_var("option"), BinOp::StrictEq, e_str("session.gc_probability"))), BinOp::And, e_binop(e_cast(CastType::Int, e_var("value")), BinOp::Lt, e_int(0))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("ini_set(): session.gc_probability must be greater than or equal to 0"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)), BinOp::And, e_binop(e_var("option"), BinOp::StrictEq, e_str("session.gc_divisor"))), BinOp::And, e_binop(e_cast(CastType::Int, e_var("value")), BinOp::LtEq, e_int(0))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("ini_set(): session.gc_divisor must be greater than 0"), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("old", e_call("__elephc_ini_get_raw", vec![e_var("option")])),
            s_if(
                e_binop(e_call("__elephc_php_version_id", vec![]), BinOp::GtEq, e_int(80400)),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.sid_length")), BinOp::And, e_binop(e_cast(CastType::Int, e_var("value")), BinOp::StrictNotEq, e_int(32))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("ini_set(): session.sid_length INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.sid_bits_per_character")), BinOp::And, e_binop(e_cast(CastType::Int, e_var("value")), BinOp::StrictNotEq, e_int(4))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("ini_set(): session.sid_bits_per_character INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.use_only_cookies")), BinOp::And, e_binop(e_call("__elephc_session_ini_bool", vec![e_var("value")]), BinOp::StrictEq, e_int(0))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("ini_set(): Disabling session.use_only_cookies INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.use_trans_sid")), BinOp::And, e_binop(e_call("__elephc_session_ini_bool", vec![e_var("value")]), BinOp::StrictEq, e_int(1))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("ini_set(): Enabling session.use_trans_sid INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.referer_check")), BinOp::And, e_binop(e_cast(CastType::String, e_var("value")), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("ini_set(): Usage of session.referer_check INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.trans_sid_tags")), BinOp::And, e_binop(e_cast(CastType::String, e_var("value")), BinOp::StrictNotEq, e_str("a=href,area=href,frame=src,form="))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("ini_set(): Usage of session.trans_sid_tags INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.trans_sid_hosts")), BinOp::And, e_binop(e_cast(CastType::String, e_var("value")), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("ini_set(): Usage of session.trans_sid_hosts INI setting is deprecated"), e_const("E_DEPRECATED")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.name")),
                vec![
                    s_expr(e_call("elephc_web_session_set_name", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.save_path")),
                vec![
                    s_expr(e_call("elephc_web_session_set_save_path", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.save_handler")),
                vec![
                    s_static_prop_assign("__ElephcSessionState", "handler", e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cache_limiter")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cache_limiter", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_samesite")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_call("elephc_web_session_get_cookie_lifetime", vec![]), e_call("elephc_web_session_get_cookie_path", vec![]), e_call("elephc_web_session_get_cookie_domain", vec![]), e_call("elephc_web_session_get_cookie_secure", vec![]), e_call("elephc_web_session_get_cookie_partitioned", vec![]), e_call("elephc_web_session_get_cookie_httponly", vec![]), e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_path")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_call("elephc_web_session_get_cookie_lifetime", vec![]), e_cast(CastType::String, e_var("value")), e_call("elephc_web_session_get_cookie_domain", vec![]), e_call("elephc_web_session_get_cookie_secure", vec![]), e_call("elephc_web_session_get_cookie_partitioned", vec![]), e_call("elephc_web_session_get_cookie_httponly", vec![]), e_call("elephc_web_session_get_cookie_samesite", vec![])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_domain")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_call("elephc_web_session_get_cookie_lifetime", vec![]), e_call("elephc_web_session_get_cookie_path", vec![]), e_cast(CastType::String, e_var("value")), e_call("elephc_web_session_get_cookie_secure", vec![]), e_call("elephc_web_session_get_cookie_partitioned", vec![]), e_call("elephc_web_session_get_cookie_httponly", vec![]), e_call("elephc_web_session_get_cookie_samesite", vec![])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_lifetime")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_cast(CastType::Int, e_var("value")), e_call("elephc_web_session_get_cookie_path", vec![]), e_call("elephc_web_session_get_cookie_domain", vec![]), e_call("elephc_web_session_get_cookie_secure", vec![]), e_call("elephc_web_session_get_cookie_partitioned", vec![]), e_call("elephc_web_session_get_cookie_httponly", vec![]), e_call("elephc_web_session_get_cookie_samesite", vec![])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_secure")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_call("elephc_web_session_get_cookie_lifetime", vec![]), e_call("elephc_web_session_get_cookie_path", vec![]), e_call("elephc_web_session_get_cookie_domain", vec![]), e_call("__elephc_session_ini_bool", vec![e_var("value")]), e_call("elephc_web_session_get_cookie_partitioned", vec![]), e_call("elephc_web_session_get_cookie_httponly", vec![]), e_call("elephc_web_session_get_cookie_samesite", vec![])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_partitioned")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_call("elephc_web_session_get_cookie_lifetime", vec![]), e_call("elephc_web_session_get_cookie_path", vec![]), e_call("elephc_web_session_get_cookie_domain", vec![]), e_call("elephc_web_session_get_cookie_secure", vec![]), e_call("__elephc_session_ini_bool", vec![e_var("value")]), e_call("elephc_web_session_get_cookie_httponly", vec![]), e_call("elephc_web_session_get_cookie_samesite", vec![])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cookie_httponly")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cookie_params", vec![e_call("elephc_web_session_get_cookie_lifetime", vec![]), e_call("elephc_web_session_get_cookie_path", vec![]), e_call("elephc_web_session_get_cookie_domain", vec![]), e_call("elephc_web_session_get_cookie_secure", vec![]), e_call("elephc_web_session_get_cookie_partitioned", vec![]), e_call("__elephc_session_ini_bool", vec![e_var("value")]), e_call("elephc_web_session_get_cookie_samesite", vec![])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.serialize_handler")),
                vec![
                    s_expr(e_call("elephc_web_session_set_serialize_handler", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.referer_check")),
                vec![
                    s_expr(e_call("elephc_web_session_set_referer_check", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.trans_sid_tags")),
                vec![
                    s_expr(e_call("elephc_web_session_set_trans_sid_tags", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.trans_sid_hosts")),
                vec![
                    s_expr(e_call("elephc_web_session_set_trans_sid_hosts", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.prefix")),
                vec![
                    s_expr(e_call("elephc_web_session_set_upload_progress_prefix", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.name")),
                vec![
                    s_expr(e_call("elephc_web_session_set_upload_progress_name", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.freq")),
                vec![
                    s_expr(e_call("elephc_web_session_set_upload_progress_freq", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.upload_progress.min_freq")),
                vec![
                    s_expr(e_call("elephc_web_session_set_upload_progress_min_freq", vec![e_cast(CastType::String, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.cache_expire")),
                vec![
                    s_expr(e_call("elephc_web_session_set_cache_expire", vec![e_cast(CastType::Int, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.gc_probability")),
                vec![
                    s_expr(e_call("elephc_web_session_set_gc_probability", vec![e_cast(CastType::Int, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.gc_divisor")),
                vec![
                    s_expr(e_call("elephc_web_session_set_gc_divisor", vec![e_cast(CastType::Int, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.gc_maxlifetime")),
                vec![
                    s_expr(e_call("elephc_web_session_set_gc_maxlifetime", vec![e_cast(CastType::Int, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.sid_length")), BinOp::And, e_binop(e_call("elephc_web_session_set_sid_length", vec![e_cast(CastType::Int, e_var("value"))]), BinOp::StrictNotEq, e_int(1))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_str("session.sid_bits_per_character")), BinOp::And, e_binop(e_call("elephc_web_session_set_sid_bits_per_character", vec![e_cast(CastType::Int, e_var("value"))]), BinOp::StrictNotEq, e_int(1))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.use_strict_mode")),
                vec![
                    s_expr(e_call("elephc_web_session_set_strict_mode", vec![e_call("__elephc_session_ini_bool", vec![e_var("value")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.use_cookies")),
                vec![
                    s_expr(e_call("elephc_web_session_set_use_cookies", vec![e_call("__elephc_session_ini_bool", vec![e_var("value")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.use_only_cookies")),
                vec![
                    s_expr(e_call("elephc_web_session_set_use_only_cookies", vec![e_call("__elephc_session_ini_bool", vec![e_var("value")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.lazy_write")),
                vec![
                    s_expr(e_call("elephc_web_session_set_lazy_write", vec![e_call("__elephc_session_ini_bool", vec![e_var("value")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("session.use_trans_sid")),
                vec![
                    s_expr(e_call("elephc_web_session_set_use_trans_sid", vec![e_call("__elephc_session_ini_bool", vec![e_var("value")])])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("old")),
        ])
        .build()
}

/// `__elephc_ini_session_details` — transcribed from the PHP form.
fn decl_fn_elephc_ini_session_details() -> Stmt {
    function("__elephc_ini_session_details")
        .returns(t_array())
        .body(vec![
            s_assign("__elephc_all", e_array(vec![])),
            s_foreach(e_call("__elephc_ini_session_keys", vec![]), None, "__elephc_ak", vec![
                s_assign("__elephc_av", e_call("__elephc_ini_session_detail_value", vec![e_var("__elephc_ak")])),
                s_assign("__elephc_access", e_ternary(e_call("__elephc_session_ini_perdir", vec![e_var("__elephc_ak")]), e_int(2), e_int(7))),
                s_array_assign("__elephc_all", e_var("__elephc_ak"), e_array_assoc(vec![(e_str("global_value"), e_var("__elephc_av")), (e_str("local_value"), e_var("__elephc_av")), (e_str("access"), e_var("__elephc_access"))])),
            ]),
            s_return(e_var("__elephc_all")),
        ])
        .build()
}

/// `__elephc_ini_session_detail_value` — transcribed from the PHP form.
fn decl_fn_elephc_ini_session_detail_value() -> Stmt {
    function("__elephc_ini_session_detail_value")
        .param("option", TypeExpr::Str)
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_assign("__elephc_sv", e_call("__elephc_ini_get_raw", vec![e_var("option")])),
            s_return(e_var("__elephc_sv")),
        ])
        .build()
}

/// `__elephc_ini_opcache_details` — transcribed from the PHP form.
fn decl_fn_elephc_ini_opcache_details() -> Stmt {
    function("__elephc_ini_opcache_details")
        .returns(t_array())
        .body(vec![
            s_assign("__elephc_all", e_array(vec![])),
            s_foreach(e_call("__elephc_opcache_ini_keys", vec![]), None, "__elephc_ok", vec![
                s_assign("__elephc_ov", e_call("__elephc_opcache_ini_detail_value", vec![e_var("__elephc_ok")])),
                s_array_assign("__elephc_all", e_var("__elephc_ok"), e_array_assoc(vec![(e_str("global_value"), e_var("__elephc_ov")), (e_str("local_value"), e_var("__elephc_ov")), (e_str("access"), e_call("__elephc_opcache_ini_access", vec![e_var("__elephc_ok")]))])),
            ]),
            s_return(e_var("__elephc_all")),
        ])
        .build()
}

/// `__elephc_ini_combined_details` — transcribed from the PHP form.
fn decl_fn_elephc_ini_combined_details() -> Stmt {
    function("__elephc_ini_combined_details")
        .returns(t_array())
        .body(vec![
            s_assign("__elephc_all", e_array(vec![])),
            s_foreach(e_call("__elephc_ini_session_keys", vec![]), None, "__elephc_ak", vec![
                s_assign("__elephc_av", e_call("__elephc_ini_session_detail_value", vec![e_var("__elephc_ak")])),
                s_assign("__elephc_access", e_ternary(e_call("__elephc_session_ini_perdir", vec![e_var("__elephc_ak")]), e_int(2), e_int(7))),
                s_array_assign("__elephc_all", e_var("__elephc_ak"), e_array_assoc(vec![(e_str("global_value"), e_var("__elephc_av")), (e_str("local_value"), e_var("__elephc_av")), (e_str("access"), e_var("__elephc_access"))])),
            ]),
            s_foreach(e_call("__elephc_opcache_ini_keys", vec![]), None, "__elephc_ok", vec![
                s_assign("__elephc_ov", e_call("__elephc_opcache_ini_detail_value", vec![e_var("__elephc_ok")])),
                s_array_assign("__elephc_all", e_var("__elephc_ok"), e_array_assoc(vec![(e_str("global_value"), e_var("__elephc_ov")), (e_str("local_value"), e_var("__elephc_ov")), (e_str("access"), e_call("__elephc_opcache_ini_access", vec![e_var("__elephc_ok")]))])),
            ]),
            s_return(e_var("__elephc_all")),
        ])
        .build()
}

/// `__elephc_ini_session_plain` — transcribed from the PHP form.
fn decl_fn_elephc_ini_session_plain() -> Stmt {
    function("__elephc_ini_session_plain")
        .returns(t_array())
        .body(vec![
            s_assign("__elephc_all", e_array(vec![])),
            s_foreach(e_call("__elephc_ini_session_keys", vec![]), None, "__elephc_ak", vec![
                s_array_assign("__elephc_all", e_var("__elephc_ak"), e_call("__elephc_ini_session_detail_value", vec![e_var("__elephc_ak")])),
            ]),
            s_return(e_var("__elephc_all")),
        ])
        .build()
}

/// `__elephc_ini_opcache_plain` — transcribed from the PHP form.
fn decl_fn_elephc_ini_opcache_plain() -> Stmt {
    function("__elephc_ini_opcache_plain")
        .returns(t_array())
        .body(vec![
            s_assign("__elephc_all", e_array(vec![])),
            s_foreach(e_call("__elephc_opcache_ini_keys", vec![]), None, "__elephc_ok", vec![
                s_array_assign("__elephc_all", e_var("__elephc_ok"), e_call("__elephc_opcache_ini_detail_value", vec![e_var("__elephc_ok")])),
            ]),
            s_return(e_var("__elephc_all")),
        ])
        .build()
}

/// `__elephc_ini_combined_plain` — transcribed from the PHP form.
fn decl_fn_elephc_ini_combined_plain() -> Stmt {
    function("__elephc_ini_combined_plain")
        .returns(t_array())
        .body(vec![
            s_assign("__elephc_all", e_array(vec![])),
            s_foreach(e_call("__elephc_ini_session_keys", vec![]), None, "__elephc_ak", vec![
                s_array_assign("__elephc_all", e_var("__elephc_ak"), e_call("__elephc_ini_session_detail_value", vec![e_var("__elephc_ak")])),
            ]),
            s_foreach(e_call("__elephc_opcache_ini_keys", vec![]), None, "__elephc_ok", vec![
                s_array_assign("__elephc_all", e_var("__elephc_ok"), e_call("__elephc_opcache_ini_detail_value", vec![e_var("__elephc_ok")])),
            ]),
            s_return(e_var("__elephc_all")),
        ])
        .build()
}

/// `__elephc_ini_all_details` — transcribed from the PHP form.
fn decl_fn_elephc_ini_all_details() -> Stmt {
    function("__elephc_ini_all_details")
        .param_default("extension", t_nullable(TypeExpr::Str), e_null())
        .returns(t_array())
        .body(vec![
            s_if(
                e_binop(e_var("extension"), BinOp::StrictEq, e_str("session")),
                vec![
                    s_return(e_call("__elephc_ini_session_details", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("extension"), BinOp::StrictEq, e_str("zend opcache")),
                vec![
                    s_return(e_call("__elephc_ini_opcache_details", vec![])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_ini_combined_details", vec![])),
        ])
        .build()
}

/// `__elephc_ini_all_plain` — transcribed from the PHP form.
fn decl_fn_elephc_ini_all_plain() -> Stmt {
    function("__elephc_ini_all_plain")
        .param_default("extension", t_nullable(TypeExpr::Str), e_null())
        .returns(t_array())
        .body(vec![
            s_if(
                e_binop(e_var("extension"), BinOp::StrictEq, e_str("session")),
                vec![
                    s_return(e_call("__elephc_ini_session_plain", vec![])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("extension"), BinOp::StrictEq, e_str("zend opcache")),
                vec![
                    s_return(e_call("__elephc_ini_opcache_plain", vec![])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_ini_combined_plain", vec![])),
        ])
        .build()
}

/// `ini_get_all` — transcribed from the PHP form.
fn decl_fn_ini_get_all() -> Stmt {
    function("ini_get_all")
        .param_default("extension", t_nullable(TypeExpr::Str), e_null())
        .param_default("details", TypeExpr::Bool, e_bool(true))
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("extension"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("session"))), BinOp::And, e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("zend opcache"))), BinOp::And, e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("core"))),
                vec![
                    s_if(
                        e_call("__elephc_ini_module_known", vec![e_var("extension")]),
                        vec![
                            s_return(e_array(vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("trigger_error", vec![e_binop(e_binop(e_str("ini_get_all(): Extension \""), BinOp::Concat, e_var("extension")), BinOp::Concat, e_str("\" cannot be found")), e_const("E_WARNING")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("details"),
                vec![
                    s_return(e_call("__elephc_ini_all_details", vec![e_var("extension")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_ini_all_plain", vec![e_var("extension")])),
        ])
        .build()
}

/// `bootstrap 39` — transcribed from the PHP form.
fn decl_stmt_bootstrap_39() -> Stmt {
    s_static_prop_assign("__ElephcSessionState", "handler", e_null())
}

/// `bootstrap 40` — transcribed from the PHP form.
fn decl_stmt_bootstrap_40() -> Stmt {
    s_static_prop_assign("__ElephcSessionState", "shutdown", e_bool(true))
}

/// `bootstrap 41` — transcribed from the PHP form.
fn decl_stmt_bootstrap_41() -> Stmt {
    s_static_prop_assign("__ElephcSessionState", "snapshot", e_str(""))
}

/// `bootstrap 42` — transcribed from the PHP form.
fn decl_stmt_bootstrap_42() -> Stmt {
    s_static_prop_assign("__ElephcSessionState", "snapshotValid", e_bool(false))
}

/// `bootstrap 43` — transcribed from the PHP form.
fn decl_stmt_bootstrap_43() -> Stmt {
    s_static_prop_assign("__ElephcSessionState", "sendCookie", e_bool(false))
}

/// `bootstrap 44` — transcribed from the PHP form.
fn decl_stmt_bootstrap_44() -> Stmt {
    s_if(
        e_binop(e_call("elephc_web_session_get_auto_start", vec![]), BinOp::StrictEq, e_int(1)),
        vec![
            s_expr(e_call("__elephc_session_start_core", vec![e_int(0)])),
        ],
        vec![],
        None,
    )
}

/// Builds the whole `--web` surface, one declaration per helper above, in source order.
pub(crate) fn web_declarations(
    php_version: PhpVersion,
    ini_overrides: &[(String, String)],
) -> Program {
    internal_declarations(|| {
        let mut declarations: Program = vec![
            decl_extern_elephc_web_method(),
            decl_extern_elephc_web_uri(),
            decl_extern_elephc_web_path(),
            decl_extern_elephc_web_query_string(),
            decl_extern_elephc_web_header_count(),
            decl_extern_elephc_web_header_name(),
            decl_extern_elephc_web_header_value(),
            decl_extern_elephc_web_body_ptr(),
            decl_extern_elephc_web_body_len(),
            decl_extern_elephc_web_remote_addr(),
            decl_extern_elephc_web_remote_port(),
            decl_extern_elephc_web_server_addr(),
            decl_extern_elephc_web_server_port(),
            decl_extern_elephc_web_protocol(),
            decl_extern_elephc_web_request_time(),
            decl_extern_elephc_web_env_count(),
            decl_extern_elephc_web_env_name(),
            decl_extern_elephc_web_env_value(),
            decl_extern_elephc_web_multipart_count(),
            decl_extern_elephc_web_multipart_name(),
            decl_extern_elephc_web_multipart_filename(),
            decl_extern_elephc_web_multipart_type(),
            decl_extern_elephc_web_multipart_value_ptr(),
            decl_extern_elephc_web_multipart_value_len(),
            decl_extern_elephc_web_session_reset(),
            decl_extern_elephc_web_session_get_name(),
            decl_extern_elephc_web_session_set_name(),
            decl_extern_elephc_web_session_get_id(),
            decl_extern_elephc_web_session_set_id(),
            decl_extern_elephc_web_session_get_status(),
            decl_extern_elephc_web_session_set_status(),
            decl_extern_elephc_web_session_get_save_path(),
            decl_extern_elephc_web_session_set_save_path(),
            decl_extern_elephc_web_session_get_cache_limiter(),
            decl_extern_elephc_web_session_set_cache_limiter(),
            decl_extern_elephc_web_session_get_cache_expire(),
            decl_extern_elephc_web_session_set_cache_expire(),
            decl_extern_elephc_web_session_get_cookie_lifetime(),
            decl_extern_elephc_web_session_get_cookie_path(),
            decl_extern_elephc_web_session_get_cookie_domain(),
            decl_extern_elephc_web_session_get_cookie_secure(),
            decl_extern_elephc_web_session_get_cookie_partitioned(),
            decl_extern_elephc_web_session_get_cookie_httponly(),
            decl_extern_elephc_web_session_get_cookie_samesite(),
            decl_extern_elephc_web_session_set_cookie_params(),
            decl_extern_elephc_web_session_data_stage(),
            decl_extern_elephc_web_session_data_len(),
            decl_extern_elephc_web_session_read_bytes(),
            decl_extern_elephc_web_session_last_read_ok(),
            decl_extern_elephc_web_session_write_bytes(),
            decl_extern_elephc_web_session_destroy(),
            decl_extern_elephc_web_session_abort(),
            decl_extern_elephc_web_session_create_id(),
            decl_extern_elephc_web_session_gc(),
            decl_extern_elephc_web_session_count_entries_bytes(),
            decl_extern_elephc_web_session_entry_key_bytes(),
            decl_extern_elephc_web_session_entry_value_bytes(),
            decl_extern_elephc_web_session_snapshot_bytes(),
            decl_extern_elephc_web_session_file_exists(),
            decl_extern_elephc_web_session_touch(),
            decl_extern_elephc_web_session_should_gc(),
            decl_extern_elephc_web_session_get_strict_mode(),
            decl_extern_elephc_web_session_set_strict_mode(),
            decl_extern_elephc_web_session_get_serialize_handler(),
            decl_extern_elephc_web_session_set_serialize_handler(),
            decl_extern_elephc_web_session_get_gc_probability(),
            decl_extern_elephc_web_session_set_gc_probability(),
            decl_extern_elephc_web_session_get_gc_divisor(),
            decl_extern_elephc_web_session_set_gc_divisor(),
            decl_extern_elephc_web_session_get_gc_maxlifetime(),
            decl_extern_elephc_web_session_set_gc_maxlifetime(),
            decl_extern_elephc_web_session_get_sid_length(),
            decl_extern_elephc_web_session_set_sid_length(),
            decl_extern_elephc_web_session_get_sid_bits_per_character(),
            decl_extern_elephc_web_session_set_sid_bits_per_character(),
            decl_extern_elephc_web_session_count_entries_bin_bytes(),
            decl_extern_elephc_web_session_entry_key_bin_bytes(),
            decl_extern_elephc_web_session_entry_value_bin_bytes(),
            decl_extern_elephc_web_session_get_referer_check(),
            decl_extern_elephc_web_session_set_referer_check(),
            decl_extern_elephc_web_session_get_use_only_cookies(),
            decl_extern_elephc_web_session_set_use_only_cookies(),
            decl_extern_elephc_web_session_get_use_cookies(),
            decl_extern_elephc_web_session_set_use_cookies(),
            decl_extern_elephc_web_session_get_lazy_write(),
            decl_extern_elephc_web_session_set_lazy_write(),
            decl_extern_elephc_web_session_get_use_trans_sid(),
            decl_extern_elephc_web_session_set_use_trans_sid(),
            decl_extern_elephc_web_session_get_trans_sid_tags(),
            decl_extern_elephc_web_session_set_trans_sid_tags(),
            decl_extern_elephc_web_session_get_trans_sid_hosts(),
            decl_extern_elephc_web_session_set_trans_sid_hosts(),
            decl_extern_elephc_web_session_get_upload_progress_enabled(),
            decl_extern_elephc_web_session_set_upload_progress_enabled(),
            decl_extern_elephc_web_session_get_upload_progress_cleanup(),
            decl_extern_elephc_web_session_set_upload_progress_cleanup(),
            decl_extern_elephc_web_session_get_upload_progress_prefix(),
            decl_extern_elephc_web_session_set_upload_progress_prefix(),
            decl_extern_elephc_web_session_get_upload_progress_name(),
            decl_extern_elephc_web_session_set_upload_progress_name(),
            decl_extern_elephc_web_session_get_upload_progress_freq(),
            decl_extern_elephc_web_session_set_upload_progress_freq(),
            decl_extern_elephc_web_session_get_upload_progress_min_freq(),
            decl_extern_elephc_web_session_set_upload_progress_min_freq(),
            decl_extern_elephc_web_session_get_auto_start(),
            decl_extern_elephc_web_session_set_auto_start(),
            decl_fn_elephc_php_version_id(php_version.version_id()),
            decl_stmt_bootstrap_1(),
            decl_stmt_bootstrap_2(),
            decl_stmt_bootstrap_3(),
            decl_stmt_bootstrap_4(),
            decl_stmt_bootstrap_5(),
            decl_stmt_bootstrap_6(),
            decl_stmt_bootstrap_7(),
            decl_stmt_bootstrap_8(),
            decl_stmt_bootstrap_9(),
            decl_stmt_bootstrap_10(),
            decl_stmt_bootstrap_11(),
            decl_stmt_bootstrap_12(),
            decl_stmt_bootstrap_13(),
            decl_stmt_bootstrap_14(),
            decl_stmt_bootstrap_15(),
            decl_stmt_bootstrap_16(),
            decl_stmt_bootstrap_17(),
            decl_stmt_bootstrap_18(),
            decl_stmt_bootstrap_19(),
            decl_stmt_bootstrap_20(),
            decl_stmt_bootstrap_21(),
            decl_stmt_bootstrap_22(),
            decl_stmt_bootstrap_23(),
            decl_stmt_bootstrap_24(),
            decl_stmt_bootstrap_25(),
            decl_stmt_bootstrap_26(),
            decl_stmt_bootstrap_27(),
            decl_stmt_bootstrap_28(),
            decl_stmt_bootstrap_29(),
            decl_stmt_bootstrap_30(),
            decl_stmt_bootstrap_31(),
            decl_stmt_bootstrap_32(),
            decl_stmt_bootstrap_33(),
            decl_stmt_bootstrap_34(),
            decl_stmt_bootstrap_35(),
            decl_fn_elephc_emit_cookie(),
            decl_fn_setcookie(),
            decl_fn_setrawcookie(),
            decl_stmt_bootstrap_36(),
            decl_stmt_bootstrap_37(),
            decl_stmt_bootstrap_38(),
            decl_fn_elephc_session_stage_bytes(),
            decl_fn_elephc_session_copy_bytes(),
            decl_fn_elephc_session_read_file(),
            decl_fn_elephc_session_write_file(),
            decl_fn_elephc_session_snapshot_bytes(),
            decl_fn_elephc_session_entry_count(),
            decl_fn_elephc_session_entry_bytes(),
            decl_class_sessionhandler(),
            decl_class_elephcsessionstate(),
            decl_class_elephccallablesessionhandler(),
            decl_fn_error_log(),
            decl_fn_trigger_error(),
            decl_fn_elephc_session_start_option_known(),
            decl_fn_session_start(),
            decl_fn_elephc_session_start_core(),
            decl_fn_session_write_close(),
            decl_fn_session_destroy(),
            decl_fn_session_id(),
            decl_fn_session_name(),
            decl_fn_session_status(),
            decl_fn_session_unset(),
            decl_fn_session_encode(),
            decl_fn_session_decode(),
            decl_fn_session_save_path(),
            decl_fn_session_regenerate_id(),
            decl_fn_session_create_id(),
            decl_fn_session_gc(),
            decl_fn_session_abort(),
            decl_fn_session_reset(),
            decl_fn_session_cache_limiter(),
            decl_fn_session_cache_expire(),
            decl_fn_session_get_cookie_params(),
            decl_fn_session_set_cookie_params(),
            decl_fn_session_commit(),
            decl_fn_session_register_shutdown(),
            decl_fn_session_module_name(),
            decl_fn_session_set_save_handler(),
            decl_fn_elephc_session_encode(),
            decl_fn_elephc_session_decode(),
            decl_fn_elephc_session_send_cookie(),
            decl_fn_elephc_session_send_cache_headers(),
            decl_fn_elephc_ini_session_keys(),
            decl_fn_elephc_is_session_ini(),
            decl_fn_elephc_ini_get_raw(),
        ];

        // The shared `opcache.*` INI helpers (`__elephc_opcache_ini_string` / `_access` / `_keys`
        // / `_all_details` / `_all_plain`), baked for the compile target so `ini_get`/`ini_set`/
        // `ini_get_all` can answer opcache directive queries ahead of the session dispatch. The
        // `_all_*` helpers are opcache-only and unreachable under `--web` (the combined
        // session+opcache all-helpers below own that job), so `prune_unreachable_prelude_functions`
        // drops them from a `--web` binary.
        //
        // The runtime `ELEPHC_INI_*` environment block goes in FIRST and only here on the `--web`
        // path: the raw-string arms above call into it, and so does the injected
        // `opcache_get_configuration()` (which `opcache_prelude::inject_if_used` emits BEFORE this
        // runs, so `prune_unreachable_prelude_functions` already sees those calls as roots).
        declarations.extend(opcache_prelude::env_override_declarations());
        declarations.extend(opcache_prelude::ini_helper_declarations(
            php_version,
            ini_overrides,
        ));

        // `__elephc_ini_module_known($m)`: the KNOWN-MODULE predicate `ini_get_all`'s extension
        // filter uses to tell "known module with no INI directives" (`[]`) from "no such module"
        // (`E_WARNING` + `false`). Derived from `CORE_LOADED_EXTENSIONS`, lowercased, plus
        // `'session'` (the extra module a `--web` binary registers), so the filter list cannot
        // drift from the `extension_loaded()`/`get_loaded_extensions()` set.
        declarations.push(opcache_prelude::ini_module_known_declaration(true));

        declarations.extend([
            decl_fn_ini_get(),
            decl_fn_elephc_session_ini_perdir(),
            decl_fn_elephc_session_ini_bool(),
            decl_fn_elephc_session_name_valid(),
            decl_fn_ini_set(),
            decl_fn_elephc_ini_session_details(),
            decl_fn_elephc_ini_session_detail_value(),
            decl_fn_elephc_ini_opcache_details(),
            decl_fn_elephc_ini_combined_details(),
            decl_fn_elephc_ini_session_plain(),
            decl_fn_elephc_ini_opcache_plain(),
            decl_fn_elephc_ini_combined_plain(),
            decl_fn_elephc_ini_all_details(),
            decl_fn_elephc_ini_all_plain(),
            decl_fn_ini_get_all(),
            decl_stmt_bootstrap_39(),
            decl_stmt_bootstrap_40(),
            decl_stmt_bootstrap_41(),
            decl_stmt_bootstrap_42(),
            decl_stmt_bootstrap_43(),
            decl_stmt_bootstrap_44(),
        ]);

        declarations
    })
}
