//! Purpose:
//! Home of the PHP `fopen` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The shared catalog declares `resource|false`; the checker refines the native stream kind.
//! - The semantic requirements hook selects URL-scheme libraries before backend linking.
//! - Arguments are inferred by the registry before the check hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "fopen",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fopen,
    ),
    requirements: crate::builtins::semantics::fopen_requirements,
}

/// Refines the catalog's resource-or-false result with the native stream resource kind.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::False,
    ]))
}
