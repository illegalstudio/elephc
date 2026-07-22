//! Purpose:
//! Home of the PHP `ptr_sizeof` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the argument is a known string literal type name and
//!   returns `PhpType::Int` (the byte size of the named type).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "ptr_sizeof",
    area: Pointers,
    params: [r#type: Mixed],
    returns: Int,
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::PtrSizeof,
    ),
    summary: "Returns the byte size of the named pointer target type.",
    extension: true,
}

/// Validates that the argument is a known string literal type name and returns `PhpType::Int`.
///
/// The registry's `check_arity` handles arity enforcement (exactly 1 argument).
/// The argument must be a string literal (not a variable) containing a recognized
/// pointer target type name such as `"int"`, `"float"`, `"string"`, or a class name.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    match &cx.args[0].kind {
        ExprKind::StringLiteral(type_name) => {
            if cx.checker.normalize_pointer_target_type(type_name).is_none() {
                return Err(CompileError::new(
                    cx.span,
                    &format!("Unknown type for ptr_sizeof(): {}", type_name),
                ));
            }
        }
        _ => {
            return Err(CompileError::new(
                cx.span,
                "ptr_sizeof() argument must be a string literal",
            ));
        }
    }
    Ok(PhpType::Int)
}
