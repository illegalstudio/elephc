//! Purpose:
//! Home of the PHP `array_find` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `fixed(&["array","callback"])` (exactly 2 required params).
//!   The legacy CHECK arm also required exactly 2 arguments; no arity override is needed.
//! - `check` validates the first argument is an indexed array and validates the predicate
//!   callback with a dummy element argument. Returns `PhpType::Mixed` (the matching element
//!   or null).
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_find` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_find",
    area: Array,
    params: [array: Mixed, callback: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns the first element satisfying a predicate callback, or null.",
    php_manual: "https://www.php.net/manual/en/function.array-find.php",
}

/// Validates the predicate callback for an `array_find` call and returns `PhpType::Mixed`.
///
/// The first argument must be an indexed array. The callback is validated with a single
/// dummy element argument derived from the array element type. Arity (exactly 2 args) is
/// pre-validated by `check_arity`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(arr_ty, PhpType::Array(_)) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    let dummy_args = vec![
        crate::types::checker::builtins::dummy_arg_for_array_scalar_elem(
            &arr_ty, cx.span,
        ),
    ];
    let label = format!("{}() callback", cx.name);
    crate::types::checker::builtins::check_callback_builtin_call(
        cx.checker,
        &cx.args[1],
        &dummy_args,
        cx.span,
        cx.env,
        &label,
    )?;
    Ok(PhpType::Mixed)
}

/// Lowers an `array_find` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_find(ctx, inst)
}
