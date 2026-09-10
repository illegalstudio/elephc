//! Purpose:
//! Declares the AOT array_filter binding and contextual predicate validation.
//!
//! Called from:
//! - The shared builtin registry's checker, EIR and callable consumers.
//!
//! Key details:
//! - Omitted/null callbacks remove empty values; filtering preserves PHP keys and value types.
//! - Result storage is boxed because indexed inputs may acquire holes or retain string keys.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "array_filter",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::with_argument_lowering(
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::ArrayFilter),
        crate::builtins::semantics::BuiltinArgumentLowering::MaterializeDefaults,
    ),
}

/// Validates array storage and callback context while leaving dynamic mode dispatch to the runtime.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let array = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(array.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_)) {
        return Err(CompileError::new(cx.span, "array_filter() first argument must be array"));
    }
    if let Some(mode) = cx.args.get(2) {
        let ty = cx.checker.infer_type(mode, cx.env)?;
        if !matches!(ty.codegen_repr(), PhpType::Int | PhpType::Bool) {
            return Err(CompileError::new(mode.span, "array_filter() third argument must be int"));
        }
    }
    if cx.args.get(1).is_none_or(|callback| matches!(callback.kind, ExprKind::Null)) {
        return Ok(PhpType::php_array());
    }
    let types = crate::types::checker::builtins::array_filter_callback_arg_types(&array, cx.args.get(2));
    super::predicate::check_callback(cx, &types)?;
    Ok(PhpType::php_array())
}
