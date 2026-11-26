//! Purpose:
//! Home of the internal `__elephc_tls_set_session_new_cb` builtin: it installs
//! or clears the normalized `ssl.session_new_cb` descriptor in the TLS bridge.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The PHP 8.6 stream-context prelude after it wraps and roots the callback.
//!
//! Key details:
//! - The call goes through the published `_elephc_tls_set_session_new_cb_fn`
//!   slot, so an ordinary TLS program reaches no bridge symbol and the
//!   rustls-only build leaves the slot null.
//! - The descriptor is owned by the PHP-rooted wrapped callable; this setter
//!   only borrows the pointer for future handshakes.

builtin! {
    contract: "__elephc_tls_set_session_new_cb",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::TlsSetSessionNewCb,
    ),
}
