//! Purpose:
//! Emits the `__rt_curl_share_*` runtime helpers behind PHP's curl SHARE interface: the
//! two handle producers (`curl_share_init()` and PHP 8.5's `curl_share_init_persistent()`
//! — the SAME shape, differing only in the slot they call and the operand already staged
//! for them), the three forwarders (`curl_share_setopt()`, `curl_share_errno()`, and
//! `CURLOPT_SHARE`'s `curl_setopt()` attach point), the `CURLSHcode` message copier
//! (`curl_share_strerror()`), and the destructor `__rt_mixed_free_deep` reaches for
//! resource kind 8.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//! - `__rt_curl_share_free` additionally from `__rt_mixed_free_deep`'s resource-kind
//!   ladder (kind 8), which is what makes a `CurlShareHandle`/`CurlSharePersistentHandle`
//!   object's teardown release the native share handle (or, for a PERSISTENT one, do
//!   nothing at all — see `crates/elephc-curl/src/share.rs`'s module doc).
//!
//! Key details:
//! - EVERY HELPER HERE IS ONE OF THE FOUR SHAPES THE EASY/MULTI LANES ALREADY
//!   ESTABLISHED — a handle producer (`easy_init`'s `emit_handle_producer`), an argument
//!   forwarder (`easy_scalar`'s `emit_forwarder`), a message copier (`easy_error`'s
//!   `emit_message_copier`), a destructor (`easy_free`'s `emit_free_helper`) — exactly as
//!   `multi.rs`'s own header documents for the multi interface. The share interface adds
//!   NO new assembly shape either.
//! - RESOURCE KIND 8 IS THE SHARE HANDLE'S OWNER, the sibling of the easy handle's kind 6
//!   and the multi handle's kind 7: `__rt_mixed_free_deep` routes it to
//!   `__rt_curl_share_free`, and `__rt_mixed_from_value` excludes it from resource-id
//!   binding because `CurlShareHandle`/`CurlSharePersistentHandle` are OBJECTS in PHP 8.
//! - `__rt_curl_share_setopt`/`__rt_curl_share_errno` SIGN-EXTEND (`IntResult::CurlCode`):
//!   `curl_share_setopt()`'s three-way answer includes `-1`, and while no real `CURLSHcode`
//!   is currently negative, sign-extending keeps this helper honest about the type it is
//!   forwarding rather than assuming today's libcurl never adds one.
//!   `__rt_curl_easy_set_share` ZERO-extends (`IntResult::Boolean`): `curl_setopt()`'s own
//!   contract is a plain `0`/`1` acceptance flag.

use crate::codegen_support::emit::Emitter;

use super::easy_error::emit_message_copier;
use super::easy_free::emit_free_helper;
use super::easy_init::emit_handle_producer;
use super::easy_scalar::{emit_forwarder, IntResult};

/// The `__rt_mixed_free_deep` resource kind that owns a libcurl SHARE handle
/// (`CurlShareHandle`/`CurlSharePersistentHandle`), the sibling of the easy handle's
/// kind 6 and the multi handle's kind 7.
const CURL_SHARE_RESOURCE_KIND: u8 = 8;

/// Emits every `__rt_curl_share_*` helper for the target.
pub(crate) fn emit_curl_share(emitter: &mut Emitter) {
    emit_handle_producer(
        emitter,
        "__rt_curl_share_init",
        "_elephc_curl_share_init_fn",
        "curl_share_init (allocate a libcurl share handle)",
        CURL_SHARE_RESOURCE_KIND,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_share_setopt",
        "_elephc_curl_share_setopt_fn",
        "curl_share_setopt (apply an integer-valued CURLSHOPT option)",
        IntResult::CurlCode,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_share_errno",
        "_elephc_curl_share_errno_fn",
        "curl_share_errno (report the last CURLSHcode)",
        IntResult::CurlCode,
    );
    emit_message_copier(
        emitter,
        "__rt_curl_share_strerror",
        "_elephc_curl_share_strerror_fn",
        "curl_share_strerror (copy libcurl's message for a CURLSHcode)",
    );
    emit_forwarder(
        emitter,
        "__rt_curl_easy_set_share",
        "_elephc_curl_easy_set_share_fn",
        "curl_easy_set_share (attach an easy handle to a share via CURLOPT_SHARE)",
        IntResult::Boolean,
    );
    emit_handle_producer(
        emitter,
        "__rt_curl_share_init_persistent",
        "_elephc_curl_share_persistent_init_fn",
        "curl_share_init_persistent (build or find the process-lifetime share, PHP 8.5)",
        CURL_SHARE_RESOURCE_KIND,
    );
    emit_free_helper(
        emitter,
        "__rt_curl_share_free",
        "_elephc_curl_share_free_fn",
        "curl_share_free (release a libcurl share handle, unless it is persistent)",
    );
}
