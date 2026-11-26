//! Purpose:
//! Home of the internal `__elephc_tls_set_psk_client` builtin: it stores the
//! normalized `ssl.psk_client_cb` descriptor and adapter address in the runtime
//! slots consumed by the `https://` helper.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The PHP 8.6 stream-context prelude after it wraps and roots the user callback.
//!
//! Key details:
//! - The descriptor is owned by the PHP-rooted wrapped callable; this builtin only
//!   borrows the pointer for the duration of a handshake.
//! - It intentionally declares no bridge requirement: the pipeline selects the
//!   libssl TLS bridge from the `psk_client_cb` source literal, so a program that
//!   never names the option keeps the rustls bridge and pays no OpenSSL cost.

builtin! {
    contract: "__elephc_tls_set_psk_client",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::TlsSetPskClient,
    ),
}
