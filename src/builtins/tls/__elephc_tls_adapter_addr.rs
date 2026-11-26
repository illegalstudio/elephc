//! Purpose:
//! Home of the internal `__elephc_tls_adapter_addr` builtin: it materializes the
//! address of the shared codegen callable adapter that re-enters compiled PHP
//! from a TLS PSK handshake callback.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The PHP 8.6 stream-context prelude when it installs `ssl.psk_client_cb`.
//!
//! Key details:
//! - The address is `__rt_curl_invoke_callback`, the same generic adapter the curl
//!   bridge uses. It takes a descriptor, an optional object argument, and a
//!   `CallSpec`; TLS passes a null object and a string-return spec.
//! - This builtin reaches no bridge symbol, so it declares no library requirement.

builtin! {
    contract: "__elephc_tls_adapter_addr",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::TlsAdapterAddr,
    ),
}
