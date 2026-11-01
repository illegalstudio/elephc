//! Purpose:
//! Declares the `Locale::getDefault()` / `Locale::setDefault()` half of php's intl `Locale` class
//! for programs that name it.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, when the program names `Locale` statically
//!   (`\Locale::getDefault()`, `\Locale::class`) and declares no `Locale` class of its own.
//!
//! Key details:
//! - Symfony asks `class_exists(\Locale::class)` and then reads the default locale: the translator
//!   falls back to it (`TranslatorTrait::getLocale()`), a request publishes its locale through
//!   `setDefault()`, and `bin/console about` prints it. Without the class every such site took its
//!   `'en'` / `'n/a'` branch where php, built with intl, answers `en_US_POSIX`.
//! - The default follows ICU's POSIX resolution, which is what php reads when `intl.default_locale`
//!   is empty: the first of `LC_ALL`, `LC_MESSAGES` and `LANG` that is set, its codeset (`.UTF-8`)
//!   dropped and its modifier appended in upper case (`de_DE@euro` is `de_DE_EURO`); `C`, `POSIX`
//!   or nothing at all is `en_US_POSIX`. `setDefault('')` goes back to that default.
//! - Only these two methods: the rest of intl (`getDisplayRegion()`, the formatters) stays absent,
//!   and `extension_loaded('intl')` keeps answering false.
//! - The declarations are BUILT, never parsed.

use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{BinOp, ExprKind, Program, StaticReceiver, StmtKind, TypeExpr};
use crate::synthetic_class::{
    class, e_binop, e_bool, e_call, e_concat, e_int, e_null, e_self_call, e_self_static_prop,
    e_str, e_var, method, php_semantics_declarations, s_assign, s_if, s_return,
    s_self_static_prop_assign, t_nullable,
};

/// The class this prelude declares.
const LOCALE_CLASS: &str = "Locale";

/// Whether the program names `Locale` through a static receiver and does not declare it itself.
pub(crate) fn needed_by(program: &Program) -> bool {
    let mut named = false;
    let mut declared = false;
    walk_nodes(program, &mut |node| match node {
        Node::Expr(expr) => match &expr.kind {
            ExprKind::StaticMethodCall { receiver: StaticReceiver::Named(name), .. }
            | ExprKind::ClassConstant { receiver: StaticReceiver::Named(name) } => {
                named |= is_locale(name.as_str());
            }
            _ => {}
        },
        Node::Stmt(stmt) => {
            if let StmtKind::ClassDecl { name, .. } = &stmt.kind {
                declared |= is_locale(name);
            }
        }
    });
    named && !declared
}

/// Whether a class name is `Locale` (global namespace, any case).
fn is_locale(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case(LOCALE_CLASS)
}

/// `$name === false`
fn is_false(name: &str) -> crate::parser::ast::Expr {
    e_binop(e_var(name), BinOp::StrictEq, e_bool(false))
}

/// The body of `Locale::systemDefault()` (see the module notes).
fn system_default_body() -> Vec<crate::parser::ast::Stmt> {
    let getenv = |name: &str| e_call("getenv", vec![e_str(name)]);
    let posix = || e_str("en_US_POSIX");
    vec![
        s_assign("id", getenv("LC_ALL")),
        s_if(is_false("id"), vec![s_assign("id", getenv("LC_MESSAGES"))], vec![], None),
        s_if(is_false("id"), vec![s_assign("id", getenv("LANG"))], vec![], None),
        s_if(is_false("id"), vec![s_return(posix())], vec![], None),
        s_assign("modifier", e_str("")),
        s_assign("at", e_call("strpos", vec![e_var("id"), e_str("@")])),
        s_if(
            e_binop(e_var("at"), BinOp::StrictNotEq, e_bool(false)),
            vec![
                s_assign(
                    "modifier",
                    e_call(
                        "substr",
                        vec![e_var("id"), e_binop(e_var("at"), BinOp::Add, e_int(1))],
                    ),
                ),
                s_assign("id", e_call("substr", vec![e_var("id"), e_int(0), e_var("at")])),
            ],
            vec![],
            None,
        ),
        s_assign("dot", e_call("strpos", vec![e_var("id"), e_str(".")])),
        s_if(
            e_binop(e_var("dot"), BinOp::StrictNotEq, e_bool(false)),
            vec![s_assign("id", e_call("substr", vec![e_var("id"), e_int(0), e_var("dot")]))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_binop(e_var("id"), BinOp::StrictEq, e_str("C")),
                BinOp::Or,
                e_binop(e_var("id"), BinOp::StrictEq, e_str("POSIX")),
            ),
            vec![s_return(posix())],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("modifier"), BinOp::StrictNotEq, e_str("")),
            vec![s_return(e_concat(
                e_concat(e_var("id"), e_str("_")),
                e_call("strtoupper", vec![e_var("modifier")]),
            ))],
            vec![],
            None,
        ),
        s_return(e_var("id")),
    ]
}

/// Returns the `Locale` class declaration.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        let system_default = || e_self_call("systemDefault", vec![]);
        vec![class(LOCALE_CLASS)
            .private_static_prop("current", t_nullable(TypeExpr::Str), Some(e_null()))
            .method(
                method("getDefault").static_().returns(TypeExpr::Str).body(vec![
                    s_if(
                        e_binop(e_self_static_prop("current"), BinOp::StrictEq, e_null()),
                        vec![s_self_static_prop_assign("current", system_default())],
                        vec![],
                        None,
                    ),
                    s_return(e_self_static_prop("current")),
                ]),
            )
            .method(
                method("setDefault")
                    .static_()
                    .param("locale", TypeExpr::Str)
                    .returns(TypeExpr::Bool)
                    .body(vec![
                        s_if(
                            e_binop(e_var("locale"), BinOp::StrictEq, e_str("")),
                            vec![
                                s_assign("locale", system_default()),
                                // An empty system default (`LANG=""`) keeps the current one, as php.
                                s_if(
                                    e_binop(e_var("locale"), BinOp::StrictEq, e_str("")),
                                    vec![s_return(e_bool(true))],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_self_static_prop_assign("current", e_var("locale")),
                        s_return(e_bool(true)),
                    ]),
            )
            .method(
                method("systemDefault")
                    .private()
                    .static_()
                    .returns(TypeExpr::Str)
                    .body(system_default_body()),
            )
            .build()]
    })
}
