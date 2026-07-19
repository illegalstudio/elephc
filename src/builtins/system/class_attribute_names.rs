//! Purpose:
//! Home of the PHP `class_attribute_names` builtin: its declaration, type-check hook,
//! and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a string literal class name, resolves the
//!   class at compile time, and returns `Array(Str)`.
//! - Dynamic class names are not yet supported; only string literals are accepted.
//! - `lower` delegates to `attributes::lower_class_attribute_names` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::system::attr_support::resolve_class_name;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "class_attribute_names",
    area: System,
    params: [class_name: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns the list of attribute names applied to a class.",
    extension: true,
}

/// Validates that the argument is a string literal class name, resolves the class,
/// and returns `Array(Str)`.
///
/// Requires a compile-time string literal: dynamic class names are not yet supported.
/// Emits a compile error if the class is not defined.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    // Resolve at compile time: only string-literal class names are
    // supported in this iteration. Dynamic class names would require
    // a runtime name→class_id lookup table that elephc does not yet
    // expose.
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(ty, PhpType::Str) {
        return Err(CompileError::new(
            cx.span,
            "class_attribute_names() argument must be a string class name",
        ));
    }
    let ExprKind::StringLiteral(class_name) = &cx.args[0].kind else {
        return Err(CompileError::new(
            cx.span,
            "class_attribute_names() requires a string literal class name (dynamic lookup is not yet supported)",
        ));
    };
    if resolve_class_name(cx.checker, class_name).is_none() {
        return Err(CompileError::new(
            cx.span,
            &format!(
                "class_attribute_names(): undefined class '{}'",
                class_name
            ),
        ));
    }
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers a `class_attribute_names` call by delegating to the shared attributes emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::attributes::lower_class_attribute_names(ctx, inst)
}
