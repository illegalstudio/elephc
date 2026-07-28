//! Purpose:
//! C-ABI surface for the elephc `--web` HTTP server bridge. Exposes the
//! server entry point and (in later phases) request/response marshaling under
//! `#[no_mangle] extern "C"` symbols the compiled PHP program calls/links.
//!
//! Called from:
//! - Compiled `--web` binaries: the emitted process entry tail-calls
//!   `elephc_web_run`; the staticlib is linked via the `BRIDGES` table in
//!   `crate::linker`.
//! - Tests: directly through the `rlib` crate type.
//!
//! Key details:
//! - Unix uses one process per prefork worker; Windows uses one process and one
//!   PHP execution thread. Request/response data therefore lives in plain
//!   process statics, not behind a mutex.

mod multipart;
mod request_state;
mod server;
mod session;
mod trans_sid;
mod worker;

// Re-exported so the compiled `--web` runtime's `__rt_stdout_write` capture
// branch links against the real per-request output sink (defined in
// `request_state`), which replaced the Phase-1 no-op stub.
pub use request_state::elephc_web_write;

// Re-exported so the compiled `--web` web prelude can link against all
// request-inspection getters exposed as C-ABI symbols by the bridge.
pub use request_state::{
    elephc_web_body_len, elephc_web_body_ptr, elephc_web_env_count, elephc_web_env_name,
    elephc_web_env_value, elephc_web_header_count, elephc_web_header_name, elephc_web_header_value,
    elephc_web_method, elephc_web_multipart_count, elephc_web_multipart_filename,
    elephc_web_multipart_name, elephc_web_multipart_type, elephc_web_multipart_value_len,
    elephc_web_multipart_value_ptr, elephc_web_path, elephc_web_protocol, elephc_web_query_string,
    elephc_web_remote_addr, elephc_web_remote_port, elephc_web_request_time, elephc_web_server_addr,
    elephc_web_server_port, elephc_web_uri,
};

// Re-exported so the compiled `--web` runtime routines (`__rt_header`,
// `__rt_http_response_code`) can link against the response-control setters.
pub use request_state::{elephc_web_header, elephc_web_set_status};

// Re-exported so the compiled `--web` web prelude can link against all session
// C-ABI bridge symbols defined in `session/` (state, file I/O, ID generation,
// and wire-format entry accessors — see `session::mod` for the submodule
// breakdown). ⚠️ Every new `#[no_mangle]` session symbol must be added here —
// a miss is invisible until a `--web` link fails.
pub use session::{
    elephc_web_session_abort, elephc_web_session_count_entries,
    elephc_web_session_count_entries_bin, elephc_web_session_count_entries_bin_bytes,
    elephc_web_session_count_entries_bytes, elephc_web_session_create_id,
    elephc_web_session_data_len, elephc_web_session_data_stage,
    elephc_web_session_destroy, elephc_web_session_entry_key, elephc_web_session_entry_key_bin,
    elephc_web_session_entry_key_bin_bytes, elephc_web_session_entry_key_bytes,
    elephc_web_session_entry_value, elephc_web_session_entry_value_bin,
    elephc_web_session_entry_value_bin_bytes, elephc_web_session_entry_value_bytes,
    elephc_web_session_file_exists, elephc_web_session_gc, elephc_web_session_get_auto_start,
    elephc_web_session_get_cache_expire, elephc_web_session_get_cache_limiter,
    elephc_web_session_get_cookie_domain, elephc_web_session_get_cookie_httponly,
    elephc_web_session_get_cookie_lifetime, elephc_web_session_get_cookie_partitioned,
    elephc_web_session_get_cookie_path,
    elephc_web_session_get_cookie_samesite, elephc_web_session_get_cookie_secure,
    elephc_web_session_get_gc_divisor, elephc_web_session_get_gc_maxlifetime,
    elephc_web_session_get_gc_probability, elephc_web_session_get_id,
    elephc_web_session_get_lazy_write, elephc_web_session_get_name,
    elephc_web_session_get_referer_check, elephc_web_session_get_save_path,
    elephc_web_session_get_serialize_handler, elephc_web_session_get_sid_bits_per_character,
    elephc_web_session_get_sid_length, elephc_web_session_get_status,
    elephc_web_session_get_strict_mode, elephc_web_session_get_trans_sid_hosts,
    elephc_web_session_get_trans_sid_tags, elephc_web_session_get_upload_progress_cleanup,
    elephc_web_session_get_upload_progress_enabled, elephc_web_session_get_upload_progress_freq,
    elephc_web_session_get_upload_progress_min_freq, elephc_web_session_get_upload_progress_name,
    elephc_web_session_get_upload_progress_prefix, elephc_web_session_get_use_cookies,
    elephc_web_session_get_use_only_cookies, elephc_web_session_get_use_trans_sid,
    elephc_web_session_last_read_ok, elephc_web_session_read, elephc_web_session_read_bytes,
    elephc_web_session_reset,
    elephc_web_session_set_auto_start, elephc_web_session_set_cache_expire,
    elephc_web_session_set_cache_limiter, elephc_web_session_set_cookie_params,
    elephc_web_session_set_gc_divisor, elephc_web_session_set_gc_maxlifetime,
    elephc_web_session_set_gc_probability, elephc_web_session_set_id,
    elephc_web_session_set_lazy_write, elephc_web_session_set_name,
    elephc_web_session_set_referer_check, elephc_web_session_set_save_path,
    elephc_web_session_set_serialize_handler, elephc_web_session_set_sid_bits_per_character,
    elephc_web_session_set_sid_length, elephc_web_session_set_status,
    elephc_web_session_set_strict_mode, elephc_web_session_set_trans_sid_hosts,
    elephc_web_session_set_trans_sid_tags, elephc_web_session_set_upload_progress_cleanup,
    elephc_web_session_set_upload_progress_enabled, elephc_web_session_set_upload_progress_freq,
    elephc_web_session_set_upload_progress_min_freq, elephc_web_session_set_upload_progress_name,
    elephc_web_session_set_upload_progress_prefix, elephc_web_session_set_use_cookies,
    elephc_web_session_set_use_only_cookies, elephc_web_session_set_use_trans_sid,
    elephc_web_session_should_gc, elephc_web_session_snapshot,
    elephc_web_session_snapshot_bytes, elephc_web_session_touch,
    elephc_web_session_write, elephc_web_session_write_bytes,
};

/// Returns the elephc-web C ABI version. Bumped when the exported symbol set or
/// any symbol's signature changes shape.
#[no_mangle]
pub extern "C" fn elephc_web_version() -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the crate links and the ABI version constant is the v1 value.
    #[test]
    fn version_is_one() {
        assert_eq!(elephc_web_version(), 1);
    }
}
