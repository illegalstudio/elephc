//! Purpose:
//! Lists the `internal: true` `__elephc_curl_*` builtin homes that back PHP's
//! `ext/curl` easy-handle surface.
//!
//! Called from:
//! - `crate::builtins` (registry collection via `inventory`).
//!
//! Key details:
//! - One home per builtin, no logic here — the registry is populated by each leaf
//!   file's `builtin!` submission.
//! - Every builtin in this area is INTERNAL: the PHP-visible `curl_*` names are
//!   elephc-PHP wrappers declared by `crate::curl_prelude`, and a prelude function
//!   cannot shadow a builtin of the same name. Keeping the raw entry points internal
//!   also keeps them out of the builtin catalog, out of `function_exists()`, out of the
//!   generated docs, and out of the elephc/magician parity snapshots.
//! - HOME FILES MUST NOT IMPORT `crate::codegen` (locked decision 12): they name a typed
//!   `RuntimeFnId` and nothing else, so the backend stays reachable only through the
//!   typed dispatch tables.

pub mod __elephc_curl_adapter_addr;
pub mod __elephc_curl_easy_body;
pub mod __elephc_curl_easy_copy;
pub mod __elephc_curl_easy_errno;
pub mod __elephc_curl_easy_set_callback;
pub mod __elephc_curl_easy_error;
pub mod __elephc_curl_easy_getinfo_double;
pub mod __elephc_curl_easy_getinfo_long;
pub mod __elephc_curl_easy_id;
pub mod __elephc_curl_easy_init;
pub mod __elephc_curl_easy_pause;
pub mod __elephc_curl_easy_perform;
pub mod __elephc_curl_easy_reset;
pub mod __elephc_curl_easy_set_share;
pub mod __elephc_curl_easy_setopt_long;
pub mod __elephc_curl_easy_setopt_slist;
pub mod __elephc_curl_easy_setopt_str;
pub mod __elephc_curl_easy_str_op;
pub mod __elephc_curl_easy_upkeep;
pub mod __elephc_curl_mime_abort;
pub mod __elephc_curl_mime_add_part;
pub mod __elephc_curl_mime_new;
pub mod __elephc_curl_mime_part_field;
pub mod __elephc_curl_mime_post;
pub mod __elephc_curl_multi_add;
pub mod __elephc_curl_multi_errno;
pub mod __elephc_curl_multi_exec;
pub mod __elephc_curl_multi_info_read;
pub mod __elephc_curl_multi_init;
pub mod __elephc_curl_multi_remove;
pub mod __elephc_curl_multi_select;
pub mod __elephc_curl_multi_setopt;
pub mod __elephc_curl_multi_setopt_unsupported_warning;
pub mod __elephc_curl_multi_strerror;
pub mod __elephc_curl_option_kind;
pub mod __elephc_curl_setopt_unsupported_warning;
pub mod __elephc_curl_share_errno;
pub mod __elephc_curl_share_init;
pub mod __elephc_curl_share_init_persistent;
pub mod __elephc_curl_share_setopt;
pub mod __elephc_curl_share_strerror;
pub mod __elephc_curl_strerror;
pub mod __elephc_curl_version;
