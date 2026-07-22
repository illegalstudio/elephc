//! Purpose:
//! Home of the PHP `class_get_attributes` builtin: its declaration, type-check hook, and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a string literal class name, resolves the
//!   class at compile time, checks that all attributes are supported, and returns
//!   `Array(Object("ReflectionAttribute"))`.
//! - Dynamic class names are not yet supported; only string literals are accepted.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::system::attr_support::{class_get_attributes_unsupported, resolve_class_name};
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "class_get_attributes",
    area: System,
    params: [class_name: Str],
    returns: Mixed,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ClassGetAttributes,
    ),
    summary: "Returns an array of ReflectionAttribute objects for all attributes of a class.",
    extension: true,
}

/// Validates that the argument is a string literal class name, resolves the class,
/// checks that all attributes are supported, and returns `Array(Object("ReflectionAttribute"))`.
///
/// Requires a compile-time string literal: dynamic class names are not yet supported.
/// Rejects classes where any attribute has unsupported argument metadata (slot count
/// mismatch or `None` slot); directs users to `ReflectionClass::getAttributes()` for those.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Str) {
        return Err(CompileError::new(
            cx.span,
            "class_get_attributes() argument must be a string class name",
        ));
    }
    let ExprKind::StringLiteral(class_name) = &cx.args[0].kind else {
        return Err(CompileError::new(
            cx.span,
            "class_get_attributes() requires a string literal class name (dynamic lookup is not yet supported)",
        ));
    };
    if resolve_class_name(cx.checker, class_name).is_none() {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "class_get_attributes(): undefined class '{}'",
                class_name
            ),
        ));
    }
    if class_get_attributes_unsupported(cx.checker, class_name) {
        return Err(CompileError::new(
            cx.span,
            "class_get_attributes(): class has attribute argument metadata that is not supported yet",
        ));
    }
    Ok(PhpType::Array(Box::new(PhpType::Object(
        "ReflectionAttribute".to_string(),
    ))))
}
