//! Purpose:
//! Home of PHP's `openssl_get_cipher_methods` builtin and default-provider inventory target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through the builtin registry.
//!
//! Key details:
//! - The AOT backend returns the managed OpenSSL 3.6.4 default-provider catalog;
//!   `eval()` retains its smaller pure-Rust cipher subset.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "openssl_get_cipher_methods",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::OpensslGetCipherMethods,
    ),
}

/// Returns an indexed string-array type for the method inventory.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}
