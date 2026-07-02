//! Purpose:
//! C-ABI surface for the elephc `--web` prefork HTTP server bridge. Exposes the
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
//! - One process per prefork worker means no shared-thread state: per-worker
//!   request/response data lives in plain process statics, not behind a mutex.

mod multipart;
mod request_state;
mod server;
mod session;
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
// C-ABI bridge symbols defined in `session.rs`.
pub use session::{
    elephc_web_session_abort, elephc_web_session_count_entries, elephc_web_session_create_id,
    elephc_web_session_destroy, elephc_web_session_entry_key, elephc_web_session_entry_value,
    elephc_web_session_gc, elephc_web_session_get_cache_expire, elephc_web_session_get_cache_limiter,
    elephc_web_session_get_cookie_domain, elephc_web_session_get_cookie_httponly,
    elephc_web_session_get_cookie_lifetime, elephc_web_session_get_cookie_path,
    elephc_web_session_get_cookie_samesite, elephc_web_session_get_cookie_secure,
    elephc_web_session_get_id, elephc_web_session_get_name, elephc_web_session_get_save_path,
    elephc_web_session_get_status, elephc_web_session_read, elephc_web_session_reset,
    elephc_web_session_set_cache_expire, elephc_web_session_set_cache_limiter,
    elephc_web_session_set_cookie_params, elephc_web_session_set_id, elephc_web_session_set_name,
    elephc_web_session_set_save_path, elephc_web_session_set_status, elephc_web_session_write,
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
