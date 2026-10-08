//! Purpose:
//! Walks declaration member types and expressions through frontend substitution passes.
//! Applies lexical magic-constant binding and generic type substitution to member children.
//!
//! Called from:
//! - `crate::magic_constants::walker::stmts` and trait binding passes.
//!
//! Key details:
//! - Constant traversal must preserve both declared type substitution and lexical initializer binding.

use crate::parser::ast::{ClassConst, ClassMethod, ClassProperty};

use super::exprs::walk_expr;
use super::stmts::walk_program;
use super::Pass;

/// Walks a declaration constant's type and initializer in its lexical scope.
pub(in crate::magic_constants) fn walk_class_constant<P: Pass>(
    constant: ClassConst, pass: &mut P,
) -> ClassConst {
    let constant = walk_class_const(constant, pass);
    ClassConst { value: walk_expr(constant.value, pass), ..constant }
}

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
    let span = prop.span;
    ClassProperty {
        type_expr: prop.type_expr.map(|ty| pass.transform_type(ty, span)),
        default: prop.default.map(|e| walk_expr(e, pass)),
        ..prop
    }
}

/// Walks a class constant, applying `pass` to its declared type.
///
/// The VALUE is deliberately left alone: this walker's other users substitute magic constants,
/// and routing constant initializers through them would change what `const A = __LINE__;`
/// compiles to in programs that have nothing to do with types.
pub(in crate::magic_constants) fn walk_class_const<P: Pass>(
    konst: ClassConst,
    pass: &mut P,
) -> ClassConst {
    let span = konst.span;
    ClassConst {
        type_expr: konst.type_expr.map(|ty| pass.transform_type(ty, span)),
        ..konst
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
    pass.enter_method(&method.name, &method.type_params);
    let span = method.span;
    let new_params = method
        .params
        .into_iter()
        .map(|(n, t, default, by_ref)| {
            (
                n,
                t.map(|ty| pass.transform_type(ty, span)),
                default.map(|d| walk_expr(d, pass)),
                by_ref,
            )
        })
        .collect();
    let new_body = walk_program(method.body, pass);
    // The return and variadic types are computed INSIDE the method's scope, before leaving it.
    // Inside a struct literal they would be evaluated after `leave_method()`, and a pass that
    // treats a generic method as a template would then see `Box<U>` with the guard already
    // lifted — and emit a class literally called `Box<U>`. The same ordering bug was fixed for
    // functions; it could not show here until a method could carry type parameters.
    let type_params = super::stmts::walk_type_params(method.type_params, pass, span);
    let variadic_type = method.variadic_type.map(|ty| pass.transform_type(ty, span));
    let return_type = method.return_type.map(|ty| pass.transform_type(ty, span));
    pass.leave_method();
    ClassMethod {
        type_params,
        params: new_params,
        variadic_type,
        return_type,
        body: new_body,
        ..method
    }
}
