//! Purpose:
//! Collects runtime emitters for system builtins, JSON, dates, regex, argv, and fatal helpers.
//! The module owns re-export wiring for helpers that bridge PHP semantics to libc or emitted state machines.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` during the system runtime section.
//!
//! Key details:
//! - System helpers must preserve PHP-visible behavior while crossing libc, syscall, JSON, regex, and date formatter boundaries.

mod build_argv;
mod date;
mod date_data;
mod getenv;
mod json_decode;
mod json_decode_mixed;
mod json_decode_x86_64;
mod json_data;
mod json_depth;
mod json_last_error_msg;
mod json_validate;
mod json_encode_array_dynamic;
mod json_encode_array_int;
mod json_encode_array_str;
mod json_encode_assoc;
mod json_encode_bool;
mod json_encode_float;
mod json_encode_null;
mod json_encode_object;
mod json_encode_str;
mod json_encode_mixed;
mod json_pretty;
mod json_throw_error;
mod match_unhandled;
mod mktime;
mod pcre_to_posix;
mod preg_match;
mod preg_match_all;
mod preg_replace;
mod preg_replace_callback;
mod preg_split;
mod preg_strip;
mod regex_locale;
mod shell_exec;
mod strtotime;
mod time;
mod microtime;
mod php_uname;

pub(crate) use build_argv::emit_build_argv;
pub(crate) use date::emit_date;
pub(crate) use date_data::emit_date_data;
pub(crate) use getenv::emit_getenv;
pub(crate) use json_data::emit_json_data;
pub(crate) use json_decode::emit_json_decode;
pub(crate) use json_decode_mixed::emit_json_decode_mixed;
pub(crate) use json_depth::{emit_json_depth_enter, emit_json_depth_exit};
pub(crate) use json_last_error_msg::emit_json_last_error_msg;
pub(crate) use json_validate::emit_json_validate;
pub(crate) use json_encode_array_dynamic::emit_json_encode_array_dynamic;
pub(crate) use json_encode_array_int::emit_json_encode_array_int;
pub(crate) use json_encode_array_str::emit_json_encode_array_str;
pub(crate) use json_encode_assoc::emit_json_encode_assoc;
pub(crate) use json_encode_bool::emit_json_encode_bool;
pub(crate) use json_encode_float::emit_json_encode_float;
pub(crate) use json_encode_null::emit_json_encode_null;
pub(crate) use json_encode_object::emit_json_encode_object;
pub(crate) use json_encode_str::emit_json_encode_str;
pub(crate) use json_encode_mixed::emit_json_encode_mixed;
pub(crate) use json_pretty::emit_json_pretty_helpers;
pub(crate) use json_throw_error::emit_json_throw_error;
pub(crate) use match_unhandled::emit_match_unhandled;
pub(crate) use microtime::emit_microtime;
pub(crate) use mktime::emit_mktime;
pub(crate) use php_uname::emit_php_uname;
pub(crate) use pcre_to_posix::emit_pcre_to_posix;
pub(crate) use preg_match::emit_preg_match;
pub(crate) use preg_match_all::emit_preg_match_all;
pub(crate) use preg_replace::emit_preg_replace;
pub(crate) use preg_replace_callback::emit_preg_replace_callback;
pub(crate) use preg_split::emit_preg_split;
pub(crate) use preg_strip::emit_preg_strip;
pub(crate) use regex_locale::emit_prepare_regex_locale;
pub(crate) use shell_exec::emit_shell_exec;
pub(crate) use strtotime::emit_strtotime;
pub(crate) use strtotime::emit_strtotime_data;
pub(crate) use time::emit_time;
