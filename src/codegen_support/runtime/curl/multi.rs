//! Purpose:
//! Emits the `__rt_curl_multi_*` runtime helpers behind PHP's curl MULTI interface:
//! the handle producer (`curl_multi_init()`), the seven integer forwarders
//! (add/remove/exec/select/info_read/setopt/errno), the `CURLMcode` message copier
//! (`curl_multi_strerror()`), and the destructor `__rt_mixed_free_deep` reaches for
//! resource kind 7.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//! - `__rt_curl_multi_free` additionally from `__rt_mixed_free_deep`'s resource-kind
//!   ladder (kind 7), which is what makes a `CurlMultiHandle` object's teardown release
//!   the native multi handle.
//!
//! Key details:
//! - EVERY HELPER HERE IS ONE OF THE FOUR SHAPES THE EASY LANE ALREADY ESTABLISHED, and
//!   each one is emitted by that lane's own emitter rather than a copy of it: a handle
//!   producer (`easy_init`), an argument forwarder (`easy_scalar`), a message copier
//!   (`easy_error`), a destructor (`easy_free`). The multi interface adds no new assembly
//!   shape — only new slots — which is the whole reason its ABI was designed to speak in
//!   plain integers.
//! - RESOURCE KIND 7 IS THE MULTI HANDLE'S OWNER, the sibling of the easy handle's kind 6:
//!   `__rt_mixed_free_deep` routes it to `__rt_curl_multi_free`, and
//!   `__rt_mixed_from_value` excludes it from resource-id binding because a
//!   `CurlMultiHandle` is an OBJECT in PHP 8 and must not consume a `fopen()`-visible
//!   resource id.
//! - TWO HELPERS DO NOT WIDEN THEIR ANSWER AT ALL ([`IntResult::Wide`]), because their
//!   bridge entry points return a full `int64_t` rather than a C `int32_t`:
//!   `elephc_curl_multi_perform` packs BOTH of PHP's outputs into one integer (the
//!   still-running count in the high 32 bits, the `CURLMcode` in the low 32) and
//!   `elephc_curl_multi_info_read` answers a completion-queue field that may itself be a
//!   64-bit easy-handle id. Sign- or zero-extending either would corrupt the value.
//! - THE OTHER FIVE FORWARDERS ARE `CurlCode`-WIDENED, i.e. SIGN-extended, because every
//!   one of them can answer a negative value that PHP must see as negative:
//!   `CURLM_CALL_MULTI_PERFORM` is `-1`, `curl_multi_select()` reports `-1` on error, and
//!   `curl_multi_setopt()`'s "not a valid option" code is `-1`. Zero-extending any of
//!   them would turn `-1` into `4294967295`.

use crate::codegen_support::emit::Emitter;

use super::easy_error::emit_message_copier;
use super::easy_free::emit_free_helper;
use super::easy_init::emit_handle_producer;
use super::easy_scalar::{emit_forwarder, emit_forwarder_rethrowing, IntResult};

/// The `__rt_mixed_free_deep` resource kind that owns a libcurl MULTI handle
/// (`CurlMultiHandle`), the sibling of the easy handle's kind 6.
const CURL_MULTI_RESOURCE_KIND: u8 = 7;

/// Emits every `__rt_curl_multi_*` helper for the target.
pub(crate) fn emit_curl_multi(emitter: &mut Emitter) {
    emit_handle_producer(
        emitter,
        "__rt_curl_multi_init",
        "_elephc_curl_multi_init_fn",
        "curl_multi_init (allocate a libcurl multi handle)",
        CURL_MULTI_RESOURCE_KIND,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_multi_add",
        "_elephc_curl_multi_add_fn",
        "curl_multi_add (attach an easy handle to a multi handle)",
        IntResult::CurlCode,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_multi_remove",
        "_elephc_curl_multi_remove_fn",
        "curl_multi_remove (detach an easy handle from a multi handle)",
        IntResult::CurlCode,
    );
    // The multi twin of `__rt_curl_easy_perform`: this is the other place a PHP callback
    // can run (libcurl calls the attached easy handles' callbacks from inside
    // `curl_multi_perform`), so it is the other place a throwable the adapter's firewall
    // parked has to resume. Without this the exception would sit in `_exc_value` and
    // surface at some unrelated later throw site.
    emit_forwarder_rethrowing(
        emitter,
        "__rt_curl_multi_exec",
        "_elephc_curl_multi_perform_fn",
        "curl_multi_exec (drive attached transfers; packed running/CURLMcode)",
        IntResult::Wide,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_multi_select",
        "_elephc_curl_multi_select_fn",
        "curl_multi_select (wait for an attached transfer to be ready)",
        IntResult::CurlCode,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_multi_info_read",
        "_elephc_curl_multi_info_read_fn",
        "curl_multi_info_read (read one completion-queue field)",
        IntResult::Wide,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_multi_setopt",
        "_elephc_curl_multi_setopt_fn",
        "curl_multi_setopt (apply an integer-valued CURLMOPT option)",
        IntResult::CurlCode,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_multi_errno",
        "_elephc_curl_multi_errno_fn",
        "curl_multi_errno (report the last CURLMcode)",
        IntResult::CurlCode,
    );
    emit_message_copier(
        emitter,
        "__rt_curl_multi_strerror",
        "_elephc_curl_multi_strerror_fn",
        "curl_multi_strerror (copy libcurl's message for a CURLMcode)",
    );
    emit_free_helper(
        emitter,
        "__rt_curl_multi_free",
        "_elephc_curl_multi_free_fn",
        "curl_multi_free (release a libcurl multi handle)",
    );
}
