//! Purpose:
//! Home of the PHP `array_reduce` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `optional(&["array","callback","initial"], 2, &[null])`.
//!   The legacy CHECK arm required exactly 3 arguments, so `min_args: 3, max_args: 3`
//!   reproduce that enforcement in `check_arity` only.
//! - `check` validates the callback with a two-element dummy args list (carry=int literal,
//!   element=array element dummy). The return type is `PhpType::Int`, matching the legacy arm.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_reduce` emitter.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    name: "array_reduce",
    area: Array,
    params: [array: Mixed, callback: Mixed, initial: Mixed = DefaultSpec::Null],
    min_args: 3,
    max_args: 3,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Iteratively reduces an array to a single value using a callback function.",
    php_manual: "https://www.php.net/manual/en/function.array-reduce.php",
}

/// Validates the callback for an `array_reduce` call and returns `PhpType::Int`.
///
/// Builds a two-element dummy args list: an integer literal as the carry placeholder
/// and a scalar element placeholder derived from the first-argument array type.
/// Arity (exactly 3 args) is pre-validated by `check_arity`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let dummy_args = vec![
        Expr::new(ExprKind::IntLiteral(0), cx.span),
        crate::types::checker::builtins::dummy_arg_for_array_scalar_elem(
            &arr_ty, cx.span,
        ),
    ];
    crate::types::checker::builtins::check_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &dummy_args,
        cx.span,
        cx.env,
        "array_reduce() callback",
    )?;
    Ok(PhpType::Int)
}

/// Lowers an `array_reduce` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_reduce(ctx, inst)
}
