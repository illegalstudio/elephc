//! Purpose:
//! Home of the PHP `scandir` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Array<Str>` (the directory entries). A check hook is required
//!   because the array return type cannot be expressed through the scalar `returns:`
//!   field.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "scandir",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Scandir,
    ),
}

/// Returns `Array<Str>` reflecting that `scandir` yields directory entry names.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if crate::codegen_support::compile_php_version() >= crate::php_version::PhpVersion::Php86 {
        if let Some(path) = cx.args.first() {
            if matches!(&path.kind, ExprKind::StringLiteral(value) if value.is_empty()) {
                return Err(CompileError::new(
                    path.span,
                    "scandir(): Argument #1 ($directory) cannot be empty",
                ));
            }
        }
        if let Some(flags) = cx.args.get(1) {
            if let Some(value) = static_int(flags) {
                if !(0..=2).contains(&value) {
                    return Err(CompileError::new(
                        flags.span,
                        "scandir(): Argument #2 ($flags) must be one of the SCANDIR_SORT_ASCENDING, SCANDIR_SORT_DESCENDING, or SCANDIR_SORT_NONE constants",
                    ));
                }
            }
        }
    }
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Resolves a literal or `SCANDIR_SORT_*` constant and simple bitwise forms.
fn static_int(expr: &Expr) -> Option<i64> {
    match &expr.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.as_str() {
            "SCANDIR_SORT_ASCENDING" => Some(0),
            "SCANDIR_SORT_DESCENDING" => Some(1),
            "SCANDIR_SORT_NONE" => Some(2),
            _ => None,
        },
        ExprKind::Negate(inner) => static_int(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = static_int(left)?;
            let right = static_int(right)?;
            match op {
                BinOp::BitAnd => Some(left & right),
                BinOp::BitOr => Some(left | right),
                BinOp::BitXor => Some(left ^ right),
                _ => None,
            }
        }
        _ => None,
    }
}
