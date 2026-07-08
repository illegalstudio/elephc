//! Purpose:
//! Home of the PHP `uksort` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(fixed(["array", "callback"]))`: exactly 2
//!   arguments, the `array` param is by-reference. The `ref` marker drives in-place
//!   mutation (ir_lower reads `ref_params` from the registry sig).
//! - `check` validates the comparator with two integer dummy arguments — `uksort` compares
//!   array keys (always integer in the supported subset), not values. Returns `Void`.
//! - `lower` is a thin wrapper over the shared `arrays::lower_uksort` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

builtin! {
    name: "uksort",
    area: Array,
    params: [ref array: Mixed, callback: Mixed],
    returns: Void,
    check: check,
    lower: lower,
    summary: "Sorts an array by keys using a user-defined comparison function.",
    php_manual: "https://www.php.net/manual/en/function.uksort.php",
}

/// Validates the array and comparator callback arguments for a `uksort` call.
///
/// `uksort` compares array keys, which are always integers in the supported subset.
/// The comparator is validated with two integer literal dummy arguments. Arity
/// (exactly 2) is pre-validated by the registry. Returns `Ok(PhpType::Void)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    let cmp_arg = Expr::new(ExprKind::IntLiteral(0), Span::dummy());
    let dummy_args = vec![cmp_arg.clone(), cmp_arg];
    let label = format!("{}() callback", cx.name);
    crate::types::checker::builtins::check_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &dummy_args,
        cx.span,
        cx.env,
        &label,
    )?;
    Ok(PhpType::Void)
}

/// Lowers a `uksort` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_uksort(ctx, inst)
}
