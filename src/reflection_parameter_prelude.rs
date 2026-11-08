//! Purpose:
//! Answers `new ReflectionParameter($function, $param)` in a closed world, for a function or
//! method named at run time and for a parameter or member that does not exist.
//!
//! Called from:
//! - `crate::pipeline::compile()`, in a closed-world build, after the autoload pass and before type
//!   checking: [`rewrite`] turns each two-argument construction into a call of the helper, and the
//!   pipeline prepends [`declarations`] when it did.
//!
//! Key details:
//! - The compile-time path needs literals naming a member that exists: php raises a
//!   `ReflectionException` at run time instead, and a closed world has no interpreter to defer to.
//!   `new ReflectionParameter([Host::class, $method], 0)` was refused with "requires a string
//!   literal method name", and `[Host::class, 'missing']` with "undefined method".
//! - The helper goes through `ReflectionMethod` / `ReflectionFunction`, which the reflection table
//!   builds for names known only at run time, and returns the reflector `getParameters()` built,
//!   so its messages for a missing class, function or method are php's own. The two parameter
//!   messages are php 8.5's.
//! - The declarations are BUILT, never parsed.

use crate::parser::ast::visit::{walk_nodes_mut, NodeMut};
use crate::parser::ast::{ExprKind, Program};
use crate::synthetic_class::{
    class, e_call, e_index, e_int, e_method_call, e_new_fq, e_str, e_ternary, e_var, method,
    php_semantics_declarations, s_assign, s_foreach, s_if, s_return, s_throw, t_class,
};

/// Reserved class holding the helper.
pub(crate) const CLASS: &str = "__ElephcReflectionParameter";

/// The helper method, `__ElephcReflectionParameter::make(mixed $function, mixed $param)`.
pub(crate) const METHOD: &str = "make";

/// Rewrites every positional two-argument `new ReflectionParameter(...)` into a call of the
/// helper; returns whether it rewrote any.
pub(crate) fn rewrite(program: &mut Program) -> bool {
    let mut rewrote = false;
    walk_nodes_mut(program, &mut |node| {
        let NodeMut::Expr(expr) = node else { return };
        let ExprKind::NewObject { class_name, args } = &mut expr.kind else { return };
        if !class_name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("ReflectionParameter")
            || args.len() != 2
            || args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
        {
            return;
        }
        let args = std::mem::take(args);
        let span = expr.span;
        let mut call = crate::synthetic_class::e_static_call(&format!("\\{CLASS}"), METHOD, args);
        call.span = span;
        *expr = call;
        rewrote = true;
    });
    rewrote
}

/// Returns the `__ElephcReflectionParameter` class declaration.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        let by_offset = e_call("is_int", vec![e_var("param")]);
        let matches = e_ternary(
            e_call("is_int", vec![e_var("param")]),
            crate::synthetic_class::e_binop(
                e_method_call(e_var("parameter"), "getPosition", vec![]),
                crate::parser::ast::BinOp::StrictEq,
                e_var("param"),
            ),
            crate::synthetic_class::e_binop(
                e_method_call(e_var("parameter"), "getName", vec![]),
                crate::parser::ast::BinOp::StrictEq,
                e_var("param"),
            ),
        );
        vec![class(CLASS)
            .method(
                method(METHOD)
                    .static_()
                    .param("function", t_class("mixed"))
                    .param("param", t_class("mixed"))
                    .returns(t_class("\\ReflectionParameter"))
                    .body(vec![
                        s_if(
                            e_call("is_array", vec![e_var("function")]),
                            vec![s_assign(
                                "reflector",
                                e_new_fq(
                                    "ReflectionMethod",
                                    vec![e_index(e_var("function"), e_int(0)), e_index(e_var("function"), e_int(1))],
                                ),
                            )],
                            vec![],
                            Some(vec![s_assign(
                                "reflector",
                                e_new_fq("ReflectionFunction", vec![e_var("function")]),
                            )]),
                        ),
                        s_foreach(
                            e_method_call(e_var("reflector"), "getParameters", vec![]),
                            None,
                            "parameter",
                            vec![s_if(matches, vec![s_return(e_var("parameter"))], vec![], None)],
                        ),
                        s_throw(e_new_fq(
                            "ReflectionException",
                            vec![e_ternary(
                                by_offset,
                                e_str("The parameter specified by its offset could not be found"),
                                e_str("The parameter specified by its name could not be found"),
                            )],
                        )),
                    ]),
            )
            .build()]
    })
}
