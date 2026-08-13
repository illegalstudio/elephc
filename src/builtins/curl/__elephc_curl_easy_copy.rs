//! Purpose:
//! Home of the internal `__elephc_curl_easy_copy` builtin: duplicates an easy handle and
//! boxes the copy as a NEW resource-kind-6 Mixed cell.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP body of `curl_copy_handle()` in `crate::curl_prelude`.
//!
//! Key details:
//! - See `__elephc_curl_easy_init` for why the curl builtins are internal, and for the
//!   resource-kind-6 boxing this shares with it — the copy is a handle in its own right,
//!   owned by its own Mixed cell, freed by its own `__rt_curl_easy_free`.
//! - IT IS `Fresh`, NOT `MayAliasArguments`: the answer is a brand-new handle id that
//!   shares no storage with the handle it was copied from. Getting that wrong would keep
//!   the SOURCE handle's temporary alive for the copy's whole lifetime — a leaked socket
//!   and TLS session, not just a leaked block.
//! - The bridge copies the PHP-layer state (RETURNTRANSFER, the captured body, the last
//!   error) alongside libcurl's own options; the prelude copies the object-side mirrors.

builtin! {
    name: "__elephc_curl_easy_copy",
    area: Curl,
    params: [handle: Mixed],
    returns: Mixed,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::CurlEasyCopy,
    ),
    summary: "Duplicates a libcurl easy handle for the curl prelude.",
    internal: true,
}
