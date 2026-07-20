//! Purpose:
//! Assembles the per-worker PHP session bridge for `--web` mode from its
//! submodules — state (`state`), flock'd file I/O (`file_io`), ID generation/
//! validation (`id`), and the session data wire format (`wire_format`) — and
//! re-exports every `#[no_mangle]` C-ABI symbol at the `session::` path so
//! `crate::lib` can re-export them for the compiled `--web` web prelude to link
//! against.
//!
//! Called from:
//! - `crate::lib`, via `mod session;` and `pub use session::{...}`.
//!
//! Key details:
//! - `#[no_mangle]` visibility is unaffected by module nesting: every function
//!   below keeps its exact linker symbol name regardless of which submodule
//!   defines it.
//! - Submodules cross-reference each other's `pub(super)` items (statics in
//!   `state`, `validate_session_id` in `id`, `release_lock` in `file_io`) since
//!   `pub(super)` here resolves to "visible within `session` and its
//!   descendants."

mod file_io;
mod id;
mod state;
pub(crate) mod upload_progress;
mod wire_format;

// Session state getters/setters + per-request reset (owns SESSION_NAME,
// SESSION_ID, SESSION_STATUS, SESSION_SAVE_PATH, cache limiter/expire, all six
// cookie parameters, the v3 strict-mode/serialize-handler/GC/SID config, and the
// v4 ini config layer — referer_check, use_only_cookies, use_trans_sid,
// trans_sid tags/hosts, upload_progress.*, and the auto_start working copy).
pub use state::{
    elephc_web_session_data_len, elephc_web_session_data_stage, elephc_web_session_get_auto_start,
    elephc_web_session_get_cache_expire, elephc_web_session_get_cache_limiter,
    elephc_web_session_get_cookie_domain, elephc_web_session_get_cookie_httponly,
    elephc_web_session_get_cookie_lifetime, elephc_web_session_get_cookie_partitioned,
    elephc_web_session_get_cookie_path, elephc_web_session_get_cookie_samesite,
    elephc_web_session_get_cookie_secure, elephc_web_session_get_gc_divisor,
    elephc_web_session_get_gc_maxlifetime, elephc_web_session_get_gc_probability,
    elephc_web_session_get_id, elephc_web_session_get_lazy_write, elephc_web_session_get_name,
    elephc_web_session_get_referer_check, elephc_web_session_get_save_path,
    elephc_web_session_get_serialize_handler, elephc_web_session_get_sid_bits_per_character,
    elephc_web_session_get_sid_length, elephc_web_session_get_status,
    elephc_web_session_get_strict_mode, elephc_web_session_get_trans_sid_hosts,
    elephc_web_session_get_trans_sid_tags, elephc_web_session_get_upload_progress_cleanup,
    elephc_web_session_get_upload_progress_enabled, elephc_web_session_get_upload_progress_freq,
    elephc_web_session_get_upload_progress_min_freq, elephc_web_session_get_upload_progress_name,
    elephc_web_session_get_upload_progress_prefix, elephc_web_session_get_use_cookies,
    elephc_web_session_get_use_only_cookies, elephc_web_session_get_use_trans_sid,
    elephc_web_session_reset, elephc_web_session_set_auto_start,
    elephc_web_session_set_cache_expire, elephc_web_session_set_cache_limiter,
    elephc_web_session_set_cookie_params, elephc_web_session_set_gc_divisor,
    elephc_web_session_set_gc_maxlifetime, elephc_web_session_set_gc_probability,
    elephc_web_session_set_id, elephc_web_session_set_lazy_write, elephc_web_session_set_name,
    elephc_web_session_set_referer_check, elephc_web_session_set_save_path,
    elephc_web_session_set_serialize_handler, elephc_web_session_set_sid_bits_per_character,
    elephc_web_session_set_sid_length, elephc_web_session_set_status,
    elephc_web_session_set_strict_mode, elephc_web_session_set_trans_sid_hosts,
    elephc_web_session_set_trans_sid_tags, elephc_web_session_set_upload_progress_cleanup,
    elephc_web_session_set_upload_progress_enabled, elephc_web_session_set_upload_progress_freq,
    elephc_web_session_set_upload_progress_min_freq, elephc_web_session_set_upload_progress_name,
    elephc_web_session_set_upload_progress_prefix, elephc_web_session_set_use_cookies,
    elephc_web_session_set_use_only_cookies, elephc_web_session_set_use_trans_sid,
};

// Flock'd session file I/O: read/write/destroy/abort/gc, the session_reset/
// lazy_write snapshot+touch primitives, the strict-mode existence check, and
// the auto-GC probability gate.
pub use file_io::{
    elephc_web_session_abort, elephc_web_session_destroy, elephc_web_session_file_exists,
    elephc_web_session_gc, elephc_web_session_last_read_ok, elephc_web_session_read,
    elephc_web_session_read_bytes,
    elephc_web_session_should_gc, elephc_web_session_snapshot, elephc_web_session_snapshot_bytes,
    elephc_web_session_touch, elephc_web_session_write, elephc_web_session_write_bytes,
};

// Session ID generation.
pub use id::elephc_web_session_create_id;

// Session data wire format entry accessors: `php` (key|serialize) and
// `php_binary` (chr(len)+key+serialize) serialize handlers.
pub use wire_format::{
    elephc_web_session_count_entries, elephc_web_session_count_entries_bin,
    elephc_web_session_count_entries_bin_bytes, elephc_web_session_count_entries_bytes,
    elephc_web_session_entry_key, elephc_web_session_entry_key_bin,
    elephc_web_session_entry_key_bin_bytes, elephc_web_session_entry_key_bytes,
    elephc_web_session_entry_value, elephc_web_session_entry_value_bin,
    elephc_web_session_entry_value_bin_bytes, elephc_web_session_entry_value_bytes,
};
