//! Purpose:
//! Home of the internal `__elephc_tls_mark_early_data` builtin: it copies the
//! last HTTPS client early-data status onto a PHP response stream's metadata
//! flag table.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The PHP 8.6 stream `fopen` wrapper after it buffers an `https://` response
//!   into a temporary stream.
//!
//! Key details:
//! - The stream operand is lowered to its native descriptor; the status globals
//!   are published by the HTTPS runtime helper during the preceding request.
//! - It reaches no bridge symbol and declares no library requirement.

builtin! {
    contract: "__elephc_tls_mark_early_data",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::TlsMarkEarlyData,
    ),
}
