//! Purpose:
//! Home of the internal `__elephc_sodium_box` builtin: one sealed-box operation that the
//! compiler-injected sodium prelude wraps as PHP's `sodium_crypto_box_*` functions.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//! - The elephc-PHP bodies in `crate::sodium_prelude`.
//!
//! Key details:
//! - The result is always a fresh owned string (empty on failure); the status the bridge
//!   reported is read back through `__elephc_sodium_status`.

builtin! {
    contract: "__elephc_sodium_box",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::SodiumBox,
    ),
}
