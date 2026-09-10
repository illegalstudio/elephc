//! Purpose:
//! Validates array predicate inputs and contextual callback parameters.
//!
//! Called from:
//! - The array_find, array_any and array_all builtin bindings.
//!
//! Key details:
//! - Runtime iteration always supplies value and key, including for one-parameter callbacks.
//! - Unknown array storage keeps Mixed context instead of inventing integer elements.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

/// Checks a predicate without restricting array layout or narrowing explicitly typed callbacks.
pub(super) fn check(cx: &mut BuiltinCheckCtx) -> Result<(), CompileError> {
    let array = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !array.is_php_array() && !matches!(array.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_))
    {
        return Err(CompileError::new(cx.span, &format!("{}() first argument must be array", cx.name)));
    }
    let callback = &cx.args[1];
    let visible = match &callback.kind {
        ExprKind::Closure { params, variadic, .. } =>
            if variadic.is_some() { 2 } else { params.len().min(2) },
        ExprKind::StringLiteral(name) => {
            if let Some(sig) = cx.checker.functions.get(name.as_str()) {
                if sig.variadic.is_some() { 2 } else { sig.params.len().min(2) }
            } else if let Some(decl) = cx.checker.fn_decls.get(name.as_str()) {
                if decl.variadic.is_some() { 2 } else { decl.params.len().min(2) }
            } else { 2 }
        }
        _ => cx.checker.resolve_expr_callable_sig(callback, cx.env)?.map_or(2, |sig| {
            if sig.variadic.is_some() { 2 } else { sig.params.len().min(2) }
        }),
    };
    let types = [
        crate::types::checker::builtins::array_element_type(&array),
        crate::types::checker::builtins::array_key_type(&array),
    ];
    crate::types::checker::builtins::check_array_callback_builtin_call(
        cx.checker, callback, &types[..visible], cx.span, cx.env,
        &format!("{}() callback", cx.name),
    )?;
    Ok(())
}
