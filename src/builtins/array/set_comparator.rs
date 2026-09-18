//! Purpose:
//! Shares array validation and boxed result typing for comparator-based set operations.
//!
//! Called from:
//! - The array_udiff and array_uintersect builtin bindings.
//!
//! Key details:
//! - Infers input element contexts before checking an unannotated comparator.
//! - Direct and callable paths return the same key-preserving boxed PHP array.

use crate::builtins::semantics::{BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

/// Gives static and dynamic callers an authoritative boxed result representation.
pub(super) const fn semantics(target: crate::ir::RuntimeFnId) -> BuiltinSemantics {
    let mut semantics = crate::builtins::semantics::runtime_fn_semantics(target);
    semantics.result_type = BuiltinResultType::Shared(result_type);
    semantics
}

/// Set selection may leave integer holes or string keys, regardless of input layout.
fn result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::php_array()
}

/// Validates both array operands and contextualizes the two comparator inputs.
pub(super) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let mut types = Vec::with_capacity(2);
    for (index, argument) in cx.args.iter().take(2).enumerate() {
        let array = cx.checker.infer_type(argument, cx.env)?;
        if !matches!(array.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_))
        {
            return Err(CompileError::new(argument.span,
                &format!("{}() argument #{} must be array", cx.name, index + 1)));
        }
        types.push(crate::types::checker::builtins::array_element_type(&array));
    }
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker, &cx.args[2], &types, cx.span, cx.env, &format!("{}() comparator", cx.name),
    )?;
    Ok(PhpType::php_array())
}
