//! Purpose:
//! Gives replacement callbacks the boxed indexed-or-associative shape of PHP capture arrays.
//!
//! Called from:
//! - The mb_ereg_replace_callback AOT semantic descriptor before ordinary argument validation.
//!
//! Key details:
//! - The runtime resolves callable validity at parameter two; compile-time context only shapes known bodies.
//! - Capture elements remain `Mixed`, matching the boxed runtime array storage used by EIR.

use crate::{
    builtins::spec::BuiltinCheckCtx,
    errors::CompileError,
    parser::ast::ExprKind,
    types::PhpType,
};

/// Returns the indexed-or-associative capture array type materialized by the runtime bridge.
fn capture_array_type() -> PhpType {
    PhpType::Union(vec![
        PhpType::Array(Box::new(PhpType::Mixed)),
        PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    ])
}

/// Resolves a known user function body with capture context without simulating an invocation.
fn contextualize_named_function(
    cx: &mut BuiltinCheckCtx,
    name: &str,
    captures: &PhpType,
) -> Result<(), CompileError> {
    let canonical = cx
        .checker
        .canonical_function_name_folded(name)
        .unwrap_or_else(|| crate::names::php_symbol_key(name.trim_start_matches('\\')));
    let Some(decl) = cx.checker.fn_decls.get(&canonical).cloned() else {
        return Ok(());
    };
    let initial = cx.checker.initial_function_param_types(&canonical, &decl)?;
    if initial.is_empty() {
        return Ok(());
    }
    let mut param_types = cx
        .checker
        .functions
        .get(&canonical)
        .map(|signature| signature.params.clone())
        .unwrap_or_else(|| initial.clone());
    let declared = decl
        .param_types
        .first()
        .is_some_and(|type_annotation| type_annotation.is_some());
    param_types[0].1 = if declared {
        crate::types::checker::Checker::specialize_generic_array_param_hint(
            &initial[0].1,
            captures,
        )
    } else {
        captures.clone()
    };
    cx.checker
        .resolve_function_signature(&canonical, &decl, param_types)?;
    Ok(())
}

/// Contextualizes known callback parameters before delegating ordinary PHP argument contracts.
pub(crate) fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let callback_type = if let Some(callback) = cx.args.get(1).cloned() {
        let captures = capture_array_type();
        let named = match &callback.kind {
            ExprKind::StringLiteral(name) => {
                let name = crate::names::php_symbol_key(name.trim_start_matches('\\'));
                cx.checker.fn_decls.contains_key(&name).then_some(name)
            },
            _ => None,
        };
        if let Some(name) = named {
            contextualize_named_function(cx, &name, &captures)?;
            cx.checker.infer_type(&callback, cx.env)?
        } else if matches!(callback.kind, ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)) {
            crate::types::checker::builtins::check_array_callback_builtin_call(cx.checker, &callback, &[captures],
                cx.span, cx.env, "mb_ereg_replace_callback() callback")?;
            PhpType::Callable
        } else {
            cx.checker.infer_type(&callback, cx.env)?
        }
    } else {
        PhpType::Mixed
    };
    super::mbstring::check_with_known_argument(cx, 1, callback_type)
}
