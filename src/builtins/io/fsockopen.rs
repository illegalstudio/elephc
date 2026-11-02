//! Purpose:
//! Home of the PHP `fsockopen` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that `error_code` (arg[2]) and `error_message` (arg[3]), if provided,
//!   are plain variables (they are written by reference). Returns `Union(stream_resource, Bool)`.
//! - Arguments are pre-inferred by the registry before the hook runs.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "fsockopen",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Fsockopen,
    ),
}

/// Validates ref output params are plain variables, then returns `Union(stream_resource, Bool)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(ec) = cx.args.get(2) {
        if !is_writable_place(ec) {
            return Err(CompileError::new(
                ec.span,
                &format!("{}() parameter $error_code must be passed a variable", cx.name),
            ));
        }
    }
    if let Some(em) = cx.args.get(3) {
        if !is_writable_place(em) {
            return Err(CompileError::new(
                em.span,
                &format!("{}() parameter $error_message must be passed a variable", cx.name),
            ));
        }
    }
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::False]))
}

/// A by-reference output the call writes back: a variable, or a property, static property or
/// array element (`@pfsockopen($host, -1, $this->errno, $this->errstr)` in Monolog's
/// `SocketHandler`), which take the by-reference builtin write-back.
pub(super) fn is_writable_place(arg: &crate::parser::ast::Expr) -> bool {
    matches!(
        arg.kind,
        ExprKind::Variable(_)
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::ArrayAccess { .. }
    )
}
