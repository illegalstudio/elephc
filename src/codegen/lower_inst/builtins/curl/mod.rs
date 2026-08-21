//! Purpose:
//! Target-aware lowerings for the internal `__elephc_curl_*` builtins. Each one
//! marshals its operands into the C argument registers the matching `__rt_curl_*` runtime
//! helper expects, publishes the `elephc_curl` bridge pointers, and calls the helper.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - THE DIVISION OF LABOUR IS DELIBERATE. Everything that needs a stack frame, a
//!   fail-closed branch, or a return-width fix-up lives in the runtime helper
//!   (`codegen_support::runtime::curl`), written once per target; these lowerings only
//!   move operands into registers. That is why there is no per-target assembly here
//!   beyond the register names the shared helpers below resolve.
//! - EVERY LOWERING PUBLISHES ALL BRIDGE POINTERS before calling, exactly like the
//!   crypto family. See `codegen_support::curl` for why the publication is unconditional
//!   rather than per-entry-point.
//! - THE HANDLE OPERAND IS ALWAYS A BOXED MIXED CELL, because the prelude reads it out of
//!   `CurlHandle::$__elephc_handle` (a `mixed` property). `load_stream_fd_to_result`
//!   unboxes it and raises PHP's TypeError for anything that is not a resource, which is
//!   the same guard `hash_update`/`hash_final` use for their context argument.

mod callbacks;
mod easy_body;
mod easy_errno;
mod easy_error;
mod easy_getinfo;
mod easy_init;
mod easy_perform;
mod easy_setopt;
mod easy_setopt_slist;
mod lifecycle;
mod mime;
mod multi;
mod option_kind;
mod share;
mod shared;
mod str_op;
mod warn_option;
mod version;

pub(crate) use easy_body::lower_curl_easy_body;
pub(crate) use callbacks::{lower_curl_adapter_addr, lower_curl_easy_set_callback};
pub(crate) use easy_errno::lower_curl_easy_errno;
pub(crate) use easy_error::lower_curl_easy_error;
pub(crate) use easy_getinfo::{lower_curl_easy_getinfo_double, lower_curl_easy_getinfo_long};
pub(crate) use easy_init::lower_curl_easy_init;
pub(crate) use easy_perform::lower_curl_easy_perform;
pub(crate) use easy_setopt::{lower_curl_easy_setopt_long, lower_curl_easy_setopt_str};
pub(crate) use easy_setopt_slist::lower_curl_easy_setopt_slist;
pub(crate) use lifecycle::{
    lower_curl_easy_copy, lower_curl_easy_pause, lower_curl_easy_reset, lower_curl_easy_upkeep,
    lower_curl_strerror,
};
pub(crate) use mime::{
    lower_curl_mime_abort, lower_curl_mime_add_part, lower_curl_mime_new,
    lower_curl_mime_part_field, lower_curl_mime_post,
};
pub(crate) use multi::{
    lower_curl_easy_id, lower_curl_multi_add, lower_curl_multi_errno, lower_curl_multi_exec,
    lower_curl_multi_info_read, lower_curl_multi_init, lower_curl_multi_remove,
    lower_curl_multi_select, lower_curl_multi_setopt, lower_curl_multi_strerror,
};
pub(crate) use option_kind::lower_curl_option_kind;
pub(crate) use share::{
    lower_curl_easy_set_share, lower_curl_share_errno, lower_curl_share_init,
    lower_curl_share_init_persistent, lower_curl_share_setopt, lower_curl_share_strerror,
};
pub(crate) use str_op::lower_curl_easy_str_op;
pub(crate) use version::lower_curl_version;
pub(crate) use warn_option::{
    lower_curl_multi_setopt_unsupported_warning, lower_curl_setopt_unsupported_warning,
};
