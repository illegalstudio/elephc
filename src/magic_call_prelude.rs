//! Purpose:
//! Builds the one helper a method call on an UNKNOWN receiver reaches `__call()` through.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which declares it whenever the program
//!   declares a class (or trait) with a `__call()` method.
//! - `crate::ir_lower::expr::magic_call_fallback`, which rewrites a method call on a `mixed` or
//!   union receiver into `method_exists($r, 'm') ? $r->m(...) : __elephc_magic_call($r, 'm', [...])`.
//!
//! Key details:
//! - The class-id dispatch ladder of a call on an unknown receiver lists only the classes that
//!   DECLARE the method. An object whose class has `__call()` but no such method fell through to
//!   "Call to undefined method" -- Termwind's `Element::__call()` forwards `getLength()` to its
//!   styles, and `array_reduce($elements, fn ($carry, $e) => $carry += $e->getLength())` calls it on
//!   an untyped closure parameter. A receiver whose static class is known already takes the
//!   `__call()` route at lowering (`method_calls::magic_call_dispatch_args`).
//! - Keeping the `__call()` ladder in ONE function means every call site pays a guard and a call,
//!   not a second ladder over every class that declares `__call()`.
//! - The helper is BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    closure, e_call, e_concat_all, e_method_call, e_new_fq, e_not, e_null, e_str, e_var, function,
    php_semantics_declarations, s_if, s_return, s_throw, t_array, t_class, t_mixed, t_nullable,
};

/// Reserved name of the helper, shared with the lowering that calls it.
pub(crate) const MAGIC_CALL_HELPER_NAME: &str = "__elephc_magic_call";

/// Reserved name of the helper that turns `[$object, 'name']` into a callable through `__call()`.
pub(crate) const MAGIC_CALLABLE_HELPER_NAME: &str = "__elephc_magic_callable";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![magic_callable_helper(), function(MAGIC_CALL_HELPER_NAME)
            .param_untyped("object")
            .param("method", TypeExpr::Str)
            .param("arguments", t_array())
            .returns(t_mixed())
            .body(vec![
                // An object whose class has no `__call()` either gets php's own message, naming
                // the method that was asked for rather than `__call`.
                s_if(
                    e_not(e_call("method_exists", vec![e_var("object"), e_str("__call")])),
                    vec![s_throw(e_new_fq(
                        "Error",
                        vec![e_concat_all(vec![
                            e_str("Call to undefined method "),
                            e_call("get_class", vec![e_var("object")]),
                            e_str("::"),
                            e_var("method"),
                            e_str("()"),
                        ])],
                    ))],
                    vec![],
                    None,
                ),
                s_return(e_method_call(
                    e_var("object"),
                    "__call",
                    vec![e_var("method"), e_var("arguments")],
                )),
            ])
            .build()]
    })
}

/// Builds `__elephc_magic_callable(object $object, string $method): ?Closure`.
///
/// php resolves a callable array whose method the class does not declare, or declares out of the
/// caller's reach, to the class's `__call()` when it has one: `call_user_func([$object, 'm'], 1)`
/// then runs `$object->__call('m', [1])`. The compiled candidate tables only list real methods,
/// so the backend asks this helper on the miss path. The closure forwards its arguments through
/// the single `__elephc_magic_call` ladder, which keeps one `__call()` dispatch per program
/// rather than one per callable site. `null` means the class has no `__call()` and the miss
/// stands.
fn magic_callable_helper() -> Stmt {
    function(MAGIC_CALLABLE_HELPER_NAME)
        // Typed `object`, not untyped: the backend calls this with the bare receiver pointer it
        // holds in its selector slot, which is an object operand rather than a boxed `mixed`.
        .param("object", TypeExpr::Named(crate::names::Name::unqualified("object")))
        .param("method", TypeExpr::Str)
        .returns(t_nullable(t_class("Closure")))
        .body(vec![
            s_if(
                e_not(e_call("method_exists", vec![e_var("object"), e_str("__call")])),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_return(
                closure()
                    .variadic("arguments")
                    .captures("object")
                    .captures("method")
                    .body(vec![s_return(e_call(
                        MAGIC_CALL_HELPER_NAME,
                        vec![e_var("object"), e_var("method"), e_var("arguments")],
                    ))])
                    .build(),
            ),
        ])
        .build()
}

/// Returns whether any class-like declaration in `program` defines a `__call()` method.
pub(crate) fn program_declares_magic_call(program: &Program) -> bool {
    use crate::parser::ast::StmtKind;
    fn declares(stmts: &[crate::parser::ast::Stmt]) -> bool {
        stmts.iter().any(|stmt| match &stmt.kind {
            StmtKind::ClassDecl { methods, .. } | StmtKind::TraitDecl { methods, .. } => methods
                .iter()
                .any(|method| method.name.eq_ignore_ascii_case("__call")),
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => declares(body),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                declares(then_body)
                    || elseif_clauses.iter().any(|(_, body)| declares(body))
                    || else_body.as_deref().is_some_and(declares)
            }
            _ => false,
        })
    }
    declares(program)
}
