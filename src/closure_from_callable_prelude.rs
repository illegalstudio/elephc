//! Purpose:
//! Builds the method a run-time `['Closure', 'fromCallable']` callable resolves to.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, when the program spells `fromCallable` as a
//!   string; `crate::codegen::lower_inst::callables` adds the `Closure::fromCallable` row of the
//!   run-time static-method table over it, and treats the class as a scope that sees every method.
//!
//! Key details:
//! - The run-time static-method table lists the compiled classes' public static methods, and
//!   `Closure` is no compiled class: Symfony's container calls `['Closure', 'fromCallable']` as a
//!   service factory (`$factory(...$arguments)`), and `lint:container` died with "callable array
//!   did not resolve to an invokable target".
//! - php checks the callable's access from the CALLER of `fromCallable()`: the container turns
//!   `[$container, 'getEnv']` (a protected method) into a closure from inside `ContainerBuilder`.
//!   The caller's scope does not reach this helper, so its class reaches every method, like the
//!   reflection classes do; only a call php would refuse behaves differently.
//! - The declarations are BUILT, never parsed.

use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{ExprKind, Program};
use crate::synthetic_class::{class, e_static_call, e_var, method, php_semantics_declarations, s_return, t_class};

/// Reserved class holding the helper; a scope that reaches every method.
pub(crate) const CLOSURE_FACTORY_CLASS: &str = "__ElephcClosureFactory";

/// The helper method, `__ElephcClosureFactory::fromCallable(callable $callback): \Closure`.
pub(crate) const CLOSURE_FACTORY_METHOD: &str = "fromCallable";

/// Whether the program spells `fromCallable` (or `Closure::fromCallable`) as a string, the only
/// way a run-time callable can name it.
pub(crate) fn named_by(program: &Program) -> bool {
    let mut named = false;
    walk_nodes(program, &mut |node| {
        if let Node::Expr(expr) = node {
            if let ExprKind::StringLiteral(text) = &expr.kind {
                named |= names_it(text);
            }
        }
    });
    named
}

/// Whether a string literal is a run-time spelling of `Closure::fromCallable` or its method name.
pub(crate) fn names_it(text: &str) -> bool {
    let text = text.trim_start_matches('\\');
    text.eq_ignore_ascii_case("fromCallable") || text.eq_ignore_ascii_case("Closure::fromCallable")
}

/// Returns the `__ElephcClosureFactory` class declaration.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![class(CLOSURE_FACTORY_CLASS)
            .method(
                method(CLOSURE_FACTORY_METHOD)
                    .static_()
                    // `mixed`, not `callable`: a `callable` parameter would resolve the method at
                    // the call boundary, outside this class's all-access scope.
                    .param("callback", t_class("mixed"))
                    .returns(t_class("\\Closure"))
                    .body(vec![s_return(e_static_call(
                        "\\Closure",
                        "fromCallable",
                        vec![e_var("callback")],
                    ))]),
            )
            .build()]
    })
}
