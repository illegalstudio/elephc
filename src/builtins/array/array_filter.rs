//! Purpose:
//! Home of the PHP `array_filter` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `optional(&["array","callback","mode"], 1, &[null, 0])`.
//!   The legacy CHECK arm required 2 or 3 arguments (`args.len() < 2 || args.len() > 3`),
//!   so `min_args: 2` reproduces that enforcement in `check_arity`; the derived max of 3
//!   from the optional signature already matches.
//! - `check` validates the first argument is an indexed array, derives callback argument types
//!   from the static mode value, and validates the callback signature. The return type
//!   preserves the input array element type.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_filter` emitter.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_filter",
    area: Array,
    params: [array: Mixed, callback: Mixed = DefaultSpec::Null, mode: Mixed = DefaultSpec::Int(0)],
    min_args: 2,
    returns: Mixed,
    check: check,
    lazy_check: true,
    lower: lower,
    summary: "Filters elements of an array using a callback function.",
    php_manual: "https://www.php.net/manual/en/function.array-filter.php",
}

/// Returns the filtered array type for an `array_filter` call.
///
/// Validates the first argument is an indexed array, derives callback argument types
/// from the optional mode argument, and validates the callback. Arity (2 or 3 args)
/// is pre-validated by `check_arity`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if let Some(mode) = cx.args.get(2) {
        cx.checker.infer_type(mode, cx.env)?;
    }
    match arr_ty {
        PhpType::Array(elem_ty) => {
            let arr_ty = PhpType::Array(elem_ty.clone());
            let callback_arg_types =
                crate::types::checker::builtins::array_filter_callback_arg_types(
                    &arr_ty,
                    cx.args.get(2),
                );
            crate::types::checker::builtins::check_array_callback_builtin_call(
                cx.checker,
                &cx.args[1],
                &callback_arg_types,
                cx.span,
                cx.env,
                "array_filter() callback",
            )?;
            Ok(PhpType::Array(elem_ty))
        }
        _ => Err(CompileError::new(
            cx.span,
            "array_filter() first argument must be array",
        )),
    }
}

/// Lowers an `array_filter` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_filter(ctx, inst)
}
