//! Purpose:
//! Walks class properties and methods during magic-constant substitution.
//! Applies expression and statement walkers to defaults, bodies, and promoted-property assignments.
//!
//! Called from:
//! - `crate::magic_constants::walker::stmts` and trait binding passes.
//!
//! Key details:
//! - Member traversal preserves declaration metadata while updating only magic-constant-bearing children.

use crate::parser::ast::{ClassMethod, ClassProperty};

use super::exprs::walk_expr;
use super::stmts::walk_program;
use super::Pass;

/// Walks a class property, applying `pass` to its default-value expression if present.
///
/// - `prop`: The class property to walk.
/// - `pass`: The pass (visitor) to apply to child expressions.
///
/// Returns a new `ClassProperty` with the default expression replaced by the result
/// of walking it, or the original default if none existed. Other fields are preserved unchanged.
pub(in crate::magic_constants) fn walk_class_property<P: Pass>(
    prop: ClassProperty,
    pass: &mut P,
) -> ClassProperty {
    ClassProperty {
        default: prop.default.map(|e| walk_expr(e, pass)),
        ..prop
    }
}

/// Walks a class method, applying `pass` to parameter defaults and the method body.
///
/// Calls `pass.enter_method` before walking and `pass.leave_method` after, so the pass
/// can track method entry/exit for context (e.g., `__METHOD__` constant).
///
/// - `method`: The class method to walk.
/// - `pass`: The pass (visitor) to apply to expressions and statements.
///
/// Returns a new `ClassMethod` with defaults and body walked; declaration metadata (name,
/// visibility, static, etc.) is preserved unchanged.
pub(in crate::magic_constants) fn walk_class_method<P: Pass>(
    method: ClassMethod,
    pass: &mut P,
) -> ClassMethod {
    pass.enter_method(&method.name);
    let new_params = method
        .params
        .into_iter()
        .map(|(n, t, default, by_ref)| (n, t, default.map(|d| walk_expr(d, pass)), by_ref))
        .collect();
    let new_body = walk_program(method.body, pass);
    pass.leave_method();
    ClassMethod {
        params: new_params,
        body: new_body,
        ..method
    }
}
