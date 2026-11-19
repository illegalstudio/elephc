//! Purpose:
//! Home of the PHP `hash_file` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` reflecting PHP behaviour where `hash_file`
//!   returns the digest string or `false` when the file cannot be read.
//! - The `check` hook links `elephc_crypto`: `hash_file` reads the file then hashes
//!   through the crypto bridge (full algorithm set, raw `$binary` output).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "hash_file",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::HashFile,
    ),
}

/// Returns `Union(Str, Bool)` and links `elephc_crypto` for the digest routine.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    if crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php86 {
        if let Some(filename) = cx.args.get(1) {
            if matches!(&filename.kind, crate::parser::ast::ExprKind::StringLiteral(value) if value.contains('\0')) {
                return Err(CompileError::new(
                    filename.span,
                    "hash_file(): Argument #2 ($filename) must not contain any null bytes",
                ));
            }
        }
    }
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::False]))
}
