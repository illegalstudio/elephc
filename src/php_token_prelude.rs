//! Purpose:
//! Declares php 8's `PhpToken` class over the tokenizer prelude's `token_get_all()` and
//! `token_name()`.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, when the program names `PhpToken` (nikic's
//!   php-parser declares `Internal\TokenPolyfill extends \PhpToken` on php 8) and declares no
//!   class of that name itself.
//!
//! Key details:
//! - `tokenize()` follows php's: a one-character token's id is its byte (the last byte of `b"`),
//!   every token carries the 1-based line it starts on and its 0-based byte offset, and the
//!   objects are built with `new static`, so a subclass's `tokenize()` answers its own class.
//! - `getTokenName()` is the character itself below 256, `token_name()` above, null for an id
//!   php does not name. `isIgnorable()` is php's whitespace / comment / doc comment / open tag.
//! - The declarations are BUILT, never parsed.

use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{
    BinOp, CastType, Expr, ExprKind, InstanceOfTarget, Program, StaticReceiver, StmtKind,
    TypeExpr,
};
use crate::synthetic_class::{
    class, e_array, e_binop, e_call, e_cast, e_const, e_index, e_int, e_new_static, e_null,
    e_str, e_this_prop, e_ternary, e_var, method, php_semantics_declarations, s_array_push,
    s_assign, s_foreach, s_if, s_prop_assign, s_return, t_array, t_mixed, t_nullable,
};

/// The class this prelude declares.
pub(crate) const PHP_TOKEN_CLASS: &str = "PhpToken";

/// Whether `name` is `PhpToken` (any case, with or without the leading `\`).
fn is_php_token_class(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case(PHP_TOKEN_CLASS)
}

/// Whether the program names `PhpToken` and does not declare it itself.
pub(crate) fn needed_by(program: &Program) -> bool {
    let mut named = false;
    let mut declared = false;
    walk_nodes(program, &mut |node| match node {
        Node::Expr(expr) => match &expr.kind {
            ExprKind::NewObject { class_name, .. } => named |= is_php_token_class(class_name.as_str()),
            ExprKind::StaticMethodCall { receiver: StaticReceiver::Named(name), .. }
            | ExprKind::ClassConstant { receiver: StaticReceiver::Named(name) } => {
                named |= is_php_token_class(name.as_str());
            }
            ExprKind::InstanceOf { target: InstanceOfTarget::Name(name), .. } => {
                named |= is_php_token_class(name.as_str());
            }
            _ => {}
        },
        Node::Stmt(stmt) => {
            if let StmtKind::ClassDecl { name, extends, .. } = &stmt.kind {
                declared |= is_php_token_class(name);
                named |= extends.as_ref().is_some_and(|parent| is_php_token_class(parent.as_str()));
            }
        }
    });
    named && !declared
}

/// `$this->id`.
fn id() -> Expr {
    e_this_prop("id")
}

/// `left === right`.
fn same(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::StrictEq, right)
}

/// The class declaration.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        let construct_body = vec![
            s_prop_assign(crate::synthetic_class::e_this(), "id", e_var("id")),
            s_prop_assign(crate::synthetic_class::e_this(), "text", e_var("text")),
            s_prop_assign(crate::synthetic_class::e_this(), "line", e_var("line")),
            s_prop_assign(crate::synthetic_class::e_this(), "pos", e_var("pos")),
        ];
        let tokenize_body = vec![
            s_assign("line", e_int(1)),
            s_assign("position", e_int(0)),
            s_assign("tokens", e_array(Vec::new())),
            s_foreach(
                e_call("token_get_all", vec![e_var("code"), e_var("flags")]),
                None,
                "token",
                vec![
                    s_if(
                        e_call("is_string", vec![e_var("token")]),
                        vec![
                            // `b"` is the only character token longer than one byte.
                            s_assign(
                                "id",
                                e_call("ord", vec![e_call("substr", vec![e_var("token"), e_int(-1)])]),
                            ),
                            s_assign("text", e_var("token")),
                        ],
                        vec![],
                        Some(vec![
                            s_assign("id", e_index(e_var("token"), e_int(0))),
                            s_assign("text", e_index(e_var("token"), e_int(1))),
                            s_assign("line", e_index(e_var("token"), e_int(2))),
                        ]),
                    ),
                    s_array_push(
                        "tokens",
                        e_new_static(vec![
                            e_var("id"),
                            e_var("text"),
                            e_var("line"),
                            e_var("position"),
                        ]),
                    ),
                    s_assign(
                        "position",
                        e_binop(e_var("position"), BinOp::Add, e_call("strlen", vec![e_var("text")])),
                    ),
                    // php counts `\r\n`, `\r` and `\n` as one line break each; done without a
                    // regex so tokenizing needs no pcre2.
                    s_assign(
                        "line",
                        e_binop(
                            e_var("line"),
                            BinOp::Add,
                            e_call(
                                "substr_count",
                                vec![
                                    e_call(
                                        "str_replace",
                                        vec![
                                            e_array(vec![e_str("\r\n"), e_str("\r")]),
                                            e_str("\n"),
                                            e_var("text"),
                                        ],
                                    ),
                                    e_str("\n"),
                                ],
                            ),
                        ),
                    ),
                ],
            ),
            s_return(e_var("tokens")),
        ];
        let is_body = vec![
            s_foreach(
                e_cast(CastType::Array, e_var("kind")),
                None,
                "value",
                vec![s_if(
                    e_binop(
                        same(e_var("value"), id()),
                        BinOp::Or,
                        same(e_var("value"), e_this_prop("text")),
                    ),
                    vec![s_return(crate::synthetic_class::e_bool(true))],
                    vec![],
                    None,
                )],
            ),
            s_return(crate::synthetic_class::e_bool(false)),
        ];
        let token_name_body = vec![
            s_if(
                e_binop(id(), BinOp::Lt, e_int(256)),
                vec![s_return(e_call("chr", vec![id()]))],
                vec![],
                None,
            ),
            s_assign("name", e_call("token_name", vec![id()])),
            s_return(e_ternary(same(e_var("name"), e_str("UNKNOWN")), e_null(), e_var("name"))),
        ];
        let ignorable = e_array(
            ["T_WHITESPACE", "T_COMMENT", "T_DOC_COMMENT", "T_OPEN_TAG"]
                .into_iter()
                .map(e_const)
                .collect(),
        );
        let token = class(PHP_TOKEN_CLASS)
            .implements("Stringable")
            .prop("id", TypeExpr::Int, None)
            .prop("text", TypeExpr::Str, None)
            .prop("line", TypeExpr::Int, None)
            .prop("pos", TypeExpr::Int, None)
            .method(
                method("__construct")
                    .final_()
                    .param("id", TypeExpr::Int)
                    .param("text", TypeExpr::Str)
                    .param_default("line", TypeExpr::Int, e_int(-1))
                    .param_default("pos", TypeExpr::Int, e_int(-1))
                    .body(construct_body),
            )
            .method(
                method("tokenize")
                    .static_()
                    .param("code", TypeExpr::Str)
                    .param_default("flags", TypeExpr::Int, e_int(0))
                    .returns(t_array())
                    .body(tokenize_body),
            )
            .method(
                method("getTokenName")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(token_name_body),
            )
            .method(method("is").param("kind", t_mixed()).returns(TypeExpr::Bool).body(is_body))
            .method(
                method("isIgnorable")
                    .returns(TypeExpr::Bool)
                    .body(vec![s_return(e_call(
                        "in_array",
                        vec![id(), ignorable, crate::synthetic_class::e_bool(true)],
                    ))]),
            )
            .method(
                method("__toString")
                    .returns(TypeExpr::Str)
                    .body(vec![s_return(e_this_prop("text"))]),
            )
            .build();
        vec![token]
    })
}
