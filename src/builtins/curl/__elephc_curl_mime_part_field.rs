//! Purpose:
//! Home of the internal `__elephc_curl_mime_part_field` builtin: sets one field (name,
//! binary data, a local file's path, MIME type, or posted filename) on the pending
//! `curl_mime` builder's current part.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP array walker in `crate::curl_prelude`, once per field a
//!   `CURLOPT_POSTFIELDS` array item (a scalar, a `CURLFile`, or a `CURLStringFile`)
//!   contributes.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal.
//! - `kind` mirrors `crates/elephc-curl/src/mime.rs`'s `FIELD_*` constants (`0` name, `1`
//!   binary data, `2` local file path, `3` MIME type, `4` posted filename). The value is a
//!   byte string, not required to be UTF-8, and is fully binary-safe only for `kind == 1`
//!   (`FIELD_DATA`, `curl_mime_data`'s explicit-length setter); every other kind goes
//!   through a NUL-terminated libcurl setter and fails (`false`) on an embedded NUL rather
//!   than silently truncating, the same contract `__elephc_curl_easy_setopt_str` already
//!   has for every other string option.

builtin! {
    contract: "__elephc_curl_mime_part_field",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlMimePartField,
    ),
}
