//! Purpose:
//! Coordinates emission of all runtime helper labels for supported targets.
//! Orders core, managed-value, and platform-facing helper groups so dependencies are available.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emit_runtime()`.
//!
//! Key details:
//! - Emission order is part of the runtime contract because helpers branch to labels and data symbols emitted elsewhere.

mod managed;
mod platform;

use super::{
    bcmath, callables, diagnostics, exceptions, generators, numeric, round_mode, strings,
    system,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::RuntimeFeatures;

/// Emits all runtime helper labels in dependency order for supported targets.
///
/// Emits core helpers first, followed by managed-value helpers and platform-facing helpers.
/// Each category is emitted before any code that depends on it, ensuring labels
/// are available when branches are assembled.
pub(crate) fn emit_runtime(emitter: &mut Emitter, features: RuntimeFeatures) {
    diagnostics::emit_diagnostics(emitter);

    // Shared numeric coercions. Emitted first because string, array, and cast helpers all
    // branch into `__rt_php_float_to_int` for PHP's float→int rules.
    numeric::emit_php_float_to_int(emitter);
    round_mode::emit_round_mode(emitter);

    // String runtime functions
    strings::emit_concat_scratch(emitter);
    strings::emit_itoa(emitter);
    strings::emit_resource_to_string(emitter);
    strings::emit_resource_type_name(emitter);
    strings::emit_resource_write_stdout(emitter);
    strings::emit_php_num_scan(emitter);
    strings::emit_ftoa(emitter);
    strings::emit_ftoa_repr(emitter);
    strings::emit_concat(emitter);
    strings::emit_atoi(emitter);
    strings::emit_str_eq(emitter);
    strings::emit_str_to_number(emitter);
    strings::emit_str_looks_like_int_for_coercion(emitter);
    strings::emit_str_to_int(emitter);
    strings::emit_str_to_int_base(emitter);
    strings::emit_str_loose_eq(emitter);
    strings::emit_number_format(emitter);
    strings::emit_strcopy(emitter);
    strings::emit_str_persist(emitter);
    strings::emit_str_inc_dec(emitter);
    strings::emit_mixed_inc_dec(emitter);
    strings::emit_strtolower(emitter);
    strings::emit_strtoupper(emitter);
    strings::emit_trim(emitter);
    strings::emit_ltrim(emitter);
    strings::emit_rtrim(emitter);
    strings::emit_strpos(emitter);
    strings::emit_strrpos(emitter);
    strings::emit_stripos(emitter);
    strings::emit_strripos(emitter);
    strings::emit_str_repeat(emitter);
    strings::emit_strrev(emitter);
    strings::emit_grapheme_strrev(emitter);
    strings::emit_chr(emitter);
    strings::emit_strcmp(emitter);
    strings::emit_strcasecmp(emitter);
    strings::emit_strncmp(emitter);
    strings::emit_strncasecmp(emitter);
    strings::emit_str_starts_with(emitter);
    strings::emit_str_ends_with(emitter);
    strings::emit_str_replace(emitter);
    strings::emit_explode(emitter);
    strings::emit_implode(emitter);
    strings::emit_implode_int(emitter);
    strings::emit_implode_bool(emitter);
    strings::emit_ucwords(emitter);
    strings::emit_str_ireplace(emitter);
    strings::emit_substr_replace(emitter);
    strings::emit_substr_count(emitter);
    strings::emit_str_pad(emitter);
    strings::emit_str_split(emitter);
    strings::emit_addslashes(emitter);
    strings::emit_stripslashes(emitter);
    strings::emit_nl2br(emitter);
    strings::emit_chunk_split(emitter);
    strings::emit_quotemeta(emitter);
    strings::emit_quoted_printable_encode(emitter);
    strings::emit_str_word_count(emitter);
    strings::emit_count_chars(emitter);
    strings::emit_strtr(emitter);
    strings::emit_wordwrap(emitter);
    strings::emit_bin2hex(emitter);
    strings::emit_dec_to_base(emitter);
    strings::emit_base_to_number(emitter);
    strings::emit_base_convert(emitter);
    strings::emit_long2ip(emitter);
    strings::emit_ip2long(emitter);
    strings::emit_inet_ntop(emitter);
    strings::emit_inet_pton(emitter);
    strings::emit_hex2bin(emitter);
    strings::emit_htmlspecialchars(emitter);
    strings::emit_html_entity_decode(emitter);
    strings::emit_urlencode(emitter);
    strings::emit_urldecode(emitter);
    strings::emit_rawurlencode(emitter);
    strings::emit_parse_url(emitter);
    strings::emit_md5(emitter);
    strings::emit_sha1(emitter);
    strings::emit_crc32(emitter);
    if features.mb_strlen {
        strings::emit_mb_strlen(emitter);
    }
    strings::emit_iconv(emitter);
    strings::emit_hash(emitter);
    strings::emit_hash_hmac(emitter);
    strings::emit_hash_equals(emitter);
    strings::emit_hash_algos_list(emitter);
    strings::emit_hash_context(emitter);
    strings::emit_openssl_methods(emitter);
    strings::emit_openssl_cipher(emitter);
    strings::emit_digest_to_string(emitter);
    strings::emit_base64_encode(emitter);
    strings::emit_base64_decode(emitter);
    strings::emit_sprintf(emitter);
    strings::emit_sprintf_pack_mixed(emitter);
    strings::emit_sprintf_mixed_casts(emitter, features.eval_bridge);
    strings::emit_vsprintf(emitter);
    strings::emit_sscanf(emitter);
    strings::emit_rtrim_mask(emitter);
    strings::emit_ltrim_mask(emitter);
    strings::emit_trim_mask(emitter);
    bcmath::emit_bcmath(emitter);

    // Callable introspection runtime functions
    callables::emit_is_callable_runtime(emitter);
    callables::emit_function_exists_lookup(emitter);
    callables::emit_callable_descriptor_release(emitter);
    callables::emit_closure_bind(emitter);

    // System runtime functions
    system::emit_build_argv(emitter);
    system::emit_time(emitter);
    system::emit_microtime(emitter);
    system::emit_microtime_build_into(emitter);
    system::emit_microtime_str(emitter);
    system::emit_microtime_mixed(emitter);
    system::emit_php_uname(emitter);
    system::emit_getenv(emitter);
    system::emit_shell_exec(emitter);
    if features.timelib {
        system::emit_date(emitter);
    }
    system::emit_date_default_timezone(emitter);
    system::emit_checkdate(emitter);
    system::emit_getdate(emitter);
    system::emit_localtime(emitter);
    system::emit_hrtime(emitter);
    system::emit_mktime(emitter);
    system::emit_strtotime(emitter, features.timelib);
    system::emit_json_encode_bool(emitter);
    system::emit_json_encode_null(emitter);
    system::emit_json_encode_str(emitter);
    system::emit_json_encode_mixed(emitter);
    system::emit_json_encode_float(emitter);
    system::emit_json_ftoa(emitter);
    system::emit_json_encode_object(emitter);
    system::emit_json_pretty_helpers(emitter);
    system::emit_json_throw_error(emitter);
    system::emit_json_depth_enter(emitter);
    system::emit_json_depth_exit(emitter);
    system::emit_json_encode_array_dynamic(emitter);
    system::emit_json_encode_array_int(emitter);
    system::emit_json_encode_array_str(emitter);
    system::emit_json_encode_assoc(emitter);
    system::emit_json_decode(emitter);
    system::emit_json_decode_mixed(emitter);
    system::emit_json_last_error_msg(emitter);
    system::emit_json_validate(emitter);
    system::emit_serialize(emitter);
    system::emit_unserialize(emitter);
    if features.regex {
        system::emit_preg_strip(emitter);
        system::emit_pcre_to_posix(emitter);
        system::emit_mb_ereg_match(emitter);
        system::emit_preg_match(emitter);
        system::emit_preg_match_all(emitter);
        system::emit_preg_replace(emitter);
        system::emit_preg_replace_callback(emitter);
        system::emit_preg_split(emitter);
    }
    system::emit_match_unhandled(emitter);
    system::emit_stack_limit_init(emitter);
    system::emit_stack_overflow(emitter);

    // Exception runtime functions
    exceptions::emit_exception_cleanup_frames(emitter);
    exceptions::emit_class_implements_interface(emitter);
    exceptions::emit_dynamic_instanceof(emitter);
    exceptions::emit_exception_matches(emitter);
    exceptions::emit_report_uncaught_exception(emitter);
    exceptions::emit_throw_current(emitter);
    exceptions::emit_rethrow_current(emitter);

    // Generator runtime helpers for Iterator methods, send/throw, and return-value retrieval.
    generators::emit_generator_runtime(emitter);

    managed::emit_managed_runtime(emitter, features);
    platform::emit_platform_runtime(emitter, features);
}

#[cfg(test)]
mod tests;
