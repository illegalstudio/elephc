//! Purpose:
//! Home of the PHP `class_attribute_args` builtin: its declaration, type-check hook, and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that both arguments are string literals, resolves the class at
//!   compile time, verifies the attribute is supported by the flat helper, and returns
//!   `Array(Mixed)`.
//! - Dynamic class or attribute names are not yet supported; only string literals are accepted.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinResultType, BuiltinSemanticInput, BuiltinSemantics,
};
use crate::builtins::system::attr_support::{class_attribute_args_unsupported, resolve_class_name};
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "class_attribute_args",
    area: System,
    params: [class_name: Str, attribute_name: Str],
    returns: Mixed,
    check: check,
    semantics: class_attribute_args_semantics(),
    summary: "Returns the constructor arguments of a named attribute applied to a class.",
    extension: true,
}

/// Builds semantics with the associative Mixed container layout emitted by the backend.
const fn class_attribute_args_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::ClassAttributeArgs);
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics
}

/// Returns the representation-safe EIR type for positional and named attribute keys.
fn eir_result_type(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    }
}

/// Validates both arguments are string literals, resolves the class and attribute,
/// checks support, and returns `Array(Mixed)`.
///
/// Requires compile-time string literals for both class and attribute names.
/// Rejects attributes whose argument metadata cannot be faithfully represented by
/// the flat helper (keyed arguments, symbolic references); directs users to
/// `ReflectionClass::getAttributes()->getArguments()` for those cases.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let class_arg_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(class_arg_ty, PhpType::Str) {
        return Err(CompileError::new(
            cx.span,
            "class_attribute_args() first argument must be a string class name",
        ));
    }
    let attr_arg_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(attr_arg_ty, PhpType::Str) {
        return Err(CompileError::new(
            cx.span,
            "class_attribute_args() second argument must be a string attribute name",
        ));
    }
    let ExprKind::StringLiteral(class_name) = &cx.args[0].kind else {
        return Err(CompileError::new(
            cx.span,
            "class_attribute_args() requires a string literal class name (dynamic lookup is not yet supported)",
        ));
    };
    if !matches!(cx.args[1].kind, ExprKind::StringLiteral(_)) {
        return Err(CompileError::new(
            cx.span,
            "class_attribute_args() requires a string literal attribute name (dynamic lookup is not yet supported)",
        ));
    }
    if resolve_class_name(cx.checker, class_name).is_none() {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "class_attribute_args(): undefined class '{}'",
                class_name
            ),
        ));
    }
    let ExprKind::StringLiteral(attr_name) = &cx.args[1].kind else {
        unreachable!("attribute argument literal checked above");
    };
    if class_attribute_args_unsupported(cx.checker, class_name, attr_name) {
        return Err(CompileError::new(
            cx.span,
            "class_attribute_args(): requested attribute uses argument metadata that is not supported yet",
        ));
    }
    Ok(PhpType::Array(Box::new(PhpType::Mixed)))
}
