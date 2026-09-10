//! Purpose:
//! Declares the bounded PHP `get_extension_funcs` registry builtin.
//!
//! Called from:
//! - The shared builtin registry through checker, EIR, and codegen consumers.
//!
//! Key details:
//! - The active backend exposes the frozen DOM/libxml/SimpleXML function registries only and
//!   accepts literal, runtime-string, and non-strict scalar-coercible extension names.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "get_extension_funcs",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::GetExtensionFuncs,
    ),
}

/// Validates the bounded registry selector is a literal, string, or coercible scalar.
///
/// PHP 8.5.8 coerces non-strict scalar values to the internal `string` parameter, but raises a
/// `TypeError` for arrays. AOT rejects non-scalar shapes at check time because the lowering has
/// no catchable dynamic type-error path; accepted inputs lower through the runtime
/// case-insensitive registry lookup.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let extension_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(cx.args[0].kind, ExprKind::StringLiteral(_))
        && !matches!(
            extension_ty.codegen_repr(),
            PhpType::Str
                | PhpType::Int
                | PhpType::Float
                | PhpType::Bool
                | PhpType::Void
                | PhpType::TaggedScalar
        )
    {
        return Err(CompileError::new(
            cx.span,
            "get_extension_funcs() first argument must be a string in AOT mode",
        ));
    }
    Ok(PhpType::Mixed)
}
