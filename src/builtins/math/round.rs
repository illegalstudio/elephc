//! Purpose:
//! Home of the PHP `round` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `round(num, precision = 0, mode = PHP_ROUND_HALF_UP)`: the optional `precision` and
//!   `mode` parameters give the registry a 1-3 arg signature.
//! - A `check` hook rejects a statically-known `PHP_ROUND_HALF_DOWN` / `PHP_ROUND_HALF_ODD`
//!   mode, which are not yet specialized. `PHP_ROUND_HALF_UP` (the default, ties away from
//!   zero) and `PHP_ROUND_HALF_EVEN` (banker's rounding) are supported; the return type is
//!   always `Float`.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::round_constants::round_mode_value;
use crate::types::PhpType;

builtin! {
    name: "round",
    area: Math,
    params: [num: Float, precision: Int = DefaultSpec::Int(0), mode: Int = DefaultSpec::Int(1)],
    returns: Float,
    check: check,
    lower: lower,
    summary: "Rounds a float.",
    php_manual: "https://www.php.net/manual/en/function.round.php",
}

/// Resolves a statically-known `round()` mode value from an integer literal or a
/// `PHP_ROUND_HALF_*` constant reference. Returns `None` for a runtime (non-constant) mode.
fn static_round_mode(arg: &Expr) -> Option<i64> {
    match &arg.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(name) => round_mode_value(name.as_str()),
        _ => None,
    }
}

/// Type-checks `round($num, $precision, $mode)`, always returning `Float`.
///
/// Rejects a statically-known `PHP_ROUND_HALF_DOWN` (2) or `PHP_ROUND_HALF_ODD` (4) mode with a
/// diagnostic, since only `PHP_ROUND_HALF_UP` (1) and `PHP_ROUND_HALF_EVEN` (3) are specialized.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    if let Some(mode) = cx.args.get(2).and_then(static_round_mode) {
        if mode == 2 || mode == 4 {
            return Err(CompileError::new(
                cx.span,
                "round(): only PHP_ROUND_HALF_UP and PHP_ROUND_HALF_EVEN modes are supported",
            ));
        }
    }
    Ok(PhpType::Float)
}

/// Lowers a `round` call by dispatching to the shared float-rounding emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::math::lower_round(ctx, inst)
}
