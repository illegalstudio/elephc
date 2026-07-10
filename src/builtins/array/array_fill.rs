//! Purpose:
//! Home of the PHP `array_fill` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` computes the actual return type based on the `start_index` argument:
//!   a literal-zero start produces an indexed array; any other start produces an
//!   associative array with Int keys and Mixed values.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_fill` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_fill",
    area: Array,
    params: [start_index: Mixed, count: Mixed, value: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Fill an array with values.",
    php_manual: "https://www.php.net/manual/en/function.array-fill.php",
}

/// Computes the return array type based on whether `start_index` is a literal zero.
///
/// The registry's `check_arity` handles arity enforcement (exactly 3 arguments).
/// A non-literal-zero start builds a keyed assoc array (Int → Mixed); a literal-zero
/// start builds an indexed array preserving the value type. This mirrors the codegen
/// emitter's branch logic so static types stay consistent with runtime behavior.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    let val_ty = cx.checker.infer_type(&cx.args[2], cx.env)?;
    let start_is_literal_zero =
        matches!(cx.args[0].kind, crate::parser::ast::ExprKind::IntLiteral(0));
    if !start_is_literal_zero {
        Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Mixed),
        })
    } else {
        Ok(PhpType::Array(Box::new(val_ty)))
    }
}

/// Lowers an `array_fill` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_fill(ctx, inst)
}
