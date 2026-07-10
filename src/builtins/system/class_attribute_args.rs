//! Purpose:
//! Home of the PHP `class_attribute_args` builtin: its declaration, type-check hook,
//! and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that both arguments are string literals, resolves the class at
//!   compile time, verifies the attribute is supported by the flat helper, and returns
//!   `Array(Mixed)`.
//! - Dynamic class or attribute names are not yet supported; only string literals are accepted.
//! - `lower` delegates to `attributes::lower_class_attribute_args` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::system::attr_support::{class_attribute_args_unsupported, resolve_class_name};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "class_attribute_args",
    area: System,
    params: [class_name: Str, attribute_name: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns the constructor arguments of a named attribute applied to a class.",
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

/// Lowers a `class_attribute_args` call by delegating to the shared attributes emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::attributes::lower_class_attribute_args(ctx, inst)
}
