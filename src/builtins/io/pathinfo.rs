//! Purpose:
//! Home of the PHP `pathinfo` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the optional `flags` argument is `Int`, evaluates it statically
//!   where possible via `pathinfo_static_flag_value`, and returns either `AssocArray{Str,Str}`,
//!   `Str`, or a union depending on the flag value.
//! - `pathinfo_static_flag_value` is a private helper that resolves `PATHINFO_*` constants
//!   at compile time; it was relocated verbatim from `src/types/checker/builtins/io/paths.rs`.
//! - The registry pre-infers arguments before calling the hook; the hook re-infers the
//!   optional `flags` argument (idempotent) to obtain its resolved type.
//! - `lower` is a thin wrapper over `io::lower_pathinfo` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    name: "pathinfo",
    area: Io,
    params: [path: Str, flags: Int = DefaultSpec::Int(15)],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns information about a file path.",
    php_manual: "function.pathinfo",
}

/// Validates `pathinfo()` flag argument and returns the refined return type.
///
/// Infers the optional `flags` argument (idempotent after registry pre-inference),
/// requires it to be `Int`, and resolves its static value via `pathinfo_static_flag_value`.
/// Returns `AssocArray{Str,Str}` for no-flag or `PATHINFO_ALL` (15), `Str` for a known
/// specific flag, or a union `Union(Str, AssocArray{Str,Str})` for a dynamic/unknown flag.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let flag = match cx.args.get(1) {
        Some(flag_expr) => {
            let flag_ty = cx.checker.infer_type(flag_expr, cx.env)?;
            if flag_ty != PhpType::Int {
                return Err(CompileError::new(
                    cx.args[1].span,
                    "pathinfo() flag must be int",
                ));
            }
            pathinfo_static_flag_value(flag_expr)
        }
        None => None,
    };
    if cx.args.get(1).is_none() || flag == Some(15) {
        Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Str),
        })
    } else if flag.is_none() {
        Ok(cx.checker.normalize_union_type(vec![
            PhpType::Str,
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Str),
            },
        ]))
    } else {
        Ok(PhpType::Str)
    }
}

/// Extracts a literal `PATHINFO_*` constant value from `flag` expression at compile time.
///
/// Handles integer literals, `PATHINFO_*` constants (`PATHINFO_DIRNAME`=1, `PATHINFO_BASENAME`=2,
/// `PATHINFO_EXTENSION`=4, `PATHINFO_FILENAME`=8, `PATHINFO_ALL`=15), negation, and bitwise
/// combinators (`|`, `&`, `^`). Returns `None` for non-static expressions (variables, function
/// calls, etc.) so the `check` hook can fall back to a union type.
fn pathinfo_static_flag_value(flag: &Expr) -> Option<i64> {
    match &flag.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => match name.as_str() {
            "PATHINFO_DIRNAME" => Some(1),
            "PATHINFO_BASENAME" => Some(2),
            "PATHINFO_EXTENSION" => Some(4),
            "PATHINFO_FILENAME" => Some(8),
            "PATHINFO_ALL" => Some(15),
            _ => None,
        },
        ExprKind::Negate(inner) => pathinfo_static_flag_value(inner).map(|value| -value),
        ExprKind::BinaryOp { left, op, right } => {
            let left = pathinfo_static_flag_value(left)?;
            let right = pathinfo_static_flag_value(right)?;
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

/// Lowers a `pathinfo` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_pathinfo(ctx, inst)
}
