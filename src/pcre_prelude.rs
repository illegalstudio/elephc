//! Purpose:
//! Injects `preg_grep()` as a declared PHP function built on `preg_split()` and `preg_match()`,
//! `preg_quote()` as a byte loop over PHP's regex metacharacters, `preg_filter()` over
//! `preg_match()` and `preg_replace()`, and `preg_replace_callback_array()` over
//! `preg_replace_callback()`.
//!
//! Called from:
//! - `crate::pipeline::compile()`, next to the version prelude and before name resolution,
//!   so a namespaced caller still resolves to the injected declaration.
//! - `crate::autoload::prelude_probe`, through `program_uses_surface`, for a call that only
//!   an autoloaded class file makes.
//!
//! Key details:
//! - Pay-for-use with a redeclaration guard, like `crate::version_prelude`: a program that
//!   never mentions `preg_grep` carries nothing, and a program declaring its own global
//!   `preg_grep` keeps it. A namespaced `App\preg_grep` does not suppress the injection.
//! - The body is PHP's algorithm over the shared regex builtins, so every target and the
//!   regex runtime detection see ordinary `preg_split()` / `preg_match()` calls. The
//!   pattern is validated up front without matching anything: `preg_split()` with a limit
//!   of 1 compiles it and stops before its first match, so an invalid pattern is `false`
//!   even for an empty array, while a valid pattern that would fail on some subject
//!   (`/a|(?R)/` on `""`) never fails the call before an entry runs. Each entry is then
//!   matched by its own `preg_match()` call, which compiles the pattern again (no compiled
//!   pattern is cached across calls). Each entry is matched as its string value and keeps
//!   its key; a match error stops the scan and returns the entries collected so far with
//!   `preg_last_error()` still set, like php-src before 8.6. From PHP 8.6 (php-src
//!   GH-11936) an error left in `preg_last_error()` makes the call return `false`.
//! - php-src resets the error state once per `preg_grep()` call, so a nested `preg_*`
//!   error recorded by an entry's `__toString()` survives the later matches and, from 8.6,
//!   fails the call. Each generated `preg_match()` clears the state, so the body carries
//!   such an error in `$nested` for the 8.6 check. Two cases still differ from php-src:
//!   `preg_last_error()` after the call reports the last generated match's outcome rather
//!   than the carried nested code, and a later `__toString()` whose own `preg_*` call
//!   succeeds does not clear an earlier carried error.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    closure, e_array, e_binop, e_bool, e_call, e_cast, e_concat, e_const, e_index, e_int, e_null,
    e_post_inc, e_str, e_var, function, internal_declarations, s_array_assign, s_assign,
    s_break, s_else_if, s_expr, s_for, s_foreach, s_if, s_return, t_array, t_union,
};

/// The surface this prelude can inject, each guarded on its own.
const SURFACE: &[&str] =
    &["preg_grep", "preg_quote", "preg_filter", "preg_replace_callback_array"];

/// The bytes `preg_quote()` escapes besides NUL and the delimiter (php-src `ext/pcre`, PHP 7.3+
/// adds `#`).
const PREG_QUOTE_SPECIALS: &str = ".\\+*?[^]$(){}=!<>|:-#";

/// `preg_grep(string $pattern, array $array, int $flags = 0): array|false`.
fn preg_grep_decl() -> Stmt {
    let matched = || e_var("matched");
    function("preg_grep")
        .param("pattern", TypeExpr::Str)
        .param("array", t_array())
        .param_default("flags", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(vec![
            // An invalid pattern fails before any entry is inspected. A one-piece split
            // compiles the pattern but runs no match, so no subject can fail this check.
            s_if(
                e_binop(
                    e_call("preg_split", vec![e_var("pattern"), e_str(""), e_int(1)]),
                    BinOp::StrictEq,
                    e_bool(false),
                ),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_assign(
                "invert",
                e_binop(
                    e_binop(e_var("flags"), BinOp::BitAnd, e_const("PREG_GREP_INVERT")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
            ),
            s_assign("result", e_array(vec![])),
            // php-src resets the error state once per call, and an entry's `__toString()` can
            // record a nested `preg_*` error that php-src keeps. Each generated `preg_match()`
            // clears the state again, so a nested error is carried here instead.
            s_assign("nested", e_const("PREG_NO_ERROR")),
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![
                    // The state is clean before the conversion (the previous match succeeded),
                    // so a code read right after it was recorded by `__toString()`.
                    s_assign("subject", e_cast(CastType::String, e_var("value"))),
                    s_if(
                        e_binop(
                            e_call("preg_last_error", vec![]),
                            BinOp::StrictNotEq,
                            e_const("PREG_NO_ERROR"),
                        ),
                        vec![s_assign("nested", e_call("preg_last_error", vec![]))],
                        vec![],
                        None,
                    ),
                    s_assign(
                        "matched",
                        e_call("preg_match", vec![e_var("pattern"), e_var("subject")]),
                    ),
                    s_if(
                        e_binop(matched(), BinOp::StrictEq, e_bool(false)),
                        vec![s_break(1)],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(
                            e_binop(matched(), BinOp::StrictEq, e_int(1)),
                            BinOp::StrictNotEq,
                            e_var("invert"),
                        ),
                        vec![s_array_assign("result", e_var("key"), e_var("value"))],
                        vec![],
                        None,
                    ),
                ],
            ),
            // PHP 8.6 (php-src GH-11936) returns false instead of the partial result when the
            // call's own match or an entry's conversion left an error.
            s_if(
                e_binop(
                    e_binop(e_const("PHP_VERSION_ID"), BinOp::GtEq, e_int(80600)),
                    BinOp::And,
                    e_binop(
                        e_binop(
                            e_call("preg_last_error", vec![]),
                            BinOp::StrictNotEq,
                            e_const("PREG_NO_ERROR"),
                        ),
                        BinOp::Or,
                        e_binop(e_var("nested"), BinOp::StrictNotEq, e_const("PREG_NO_ERROR")),
                    ),
                ),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// `preg_quote(string $str, ?string $delimiter = null): string`.
///
/// Escapes each regex metacharacter and the delimiter's first byte with a backslash, and NUL as
/// `\000`, like php-src. An empty delimiter escapes nothing extra.
fn preg_quote_decl() -> Stmt {
    let c = || e_var("c");
    function("preg_quote")
        .param("str", TypeExpr::Str)
        .param_default("delimiter", TypeExpr::Nullable(Box::new(TypeExpr::Str)), e_null())
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("d", e_str("")),
            s_if(
                e_binop(
                    e_binop(e_var("delimiter"), BinOp::StrictNotEq, e_null()),
                    BinOp::And,
                    e_binop(e_var("delimiter"), BinOp::StrictNotEq, e_str("")),
                ),
                vec![s_assign("d", e_index(e_var("delimiter"), e_int(0)))],
                vec![],
                None,
            ),
            s_assign("out", e_str("")),
            s_assign("n", e_call("strlen", vec![e_var("str")])),
            s_for(
                Some(s_assign("i", e_int(0))),
                Some(e_binop(e_var("i"), BinOp::Lt, e_var("n"))),
                Some(s_expr(e_post_inc("i"))),
                vec![
                    s_assign("c", e_index(e_var("str"), e_var("i"))),
                    s_else_if(
                        vec![
                            (
                                e_binop(c(), BinOp::StrictEq, e_str("\0")),
                                vec![s_assign("out", e_concat(e_var("out"), e_str("\\000")))],
                            ),
                            (
                                e_binop(
                                    e_binop(
                                        e_call("strpos", vec![e_str(PREG_QUOTE_SPECIALS), c()]),
                                        BinOp::StrictNotEq,
                                        e_bool(false),
                                    ),
                                    BinOp::Or,
                                    e_binop(c(), BinOp::StrictEq, e_var("d")),
                                ),
                                vec![s_assign(
                                    "out",
                                    e_concat(e_concat(e_var("out"), e_str("\\")), c()),
                                )],
                            ),
                        ],
                        Some(vec![s_assign("out", e_concat(e_var("out"), c()))]),
                    ),
                ],
            ),
            s_return(e_var("out")),
        ])
        .build()
}

/// `preg_filter(string $pattern, string $replacement, string $subject): ?string`.
///
/// `preg_replace()` that answers `null` when nothing matched (or the pattern is invalid), over the
/// same string-only subset elephc's `preg_replace()` accepts.
fn preg_filter_decl() -> Stmt {
    function("preg_filter")
        .param("pattern", TypeExpr::Str)
        .param("replacement", TypeExpr::Str)
        .param("subject", TypeExpr::Str)
        .returns(TypeExpr::Nullable(Box::new(TypeExpr::Str)))
        .body(vec![
            s_if(
                e_binop(
                    e_call("preg_match", vec![e_var("pattern"), e_var("subject")]),
                    BinOp::StrictNotEq,
                    e_int(1),
                ),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_return(e_call(
                "preg_replace",
                vec![e_var("pattern"), e_var("replacement"), e_var("subject")],
            )),
        ])
        .build()
}

/// `preg_replace_callback_array(array $pattern, string $subject): ?string`.
///
/// Applies each `pattern => callback` entry in order through `preg_replace_callback()`, over the
/// same string-subject subset; a failing entry makes the whole call `null`, like php-src.
fn preg_replace_callback_array_decl() -> Stmt {
    function("preg_replace_callback_array")
        .param("pattern", t_array())
        .param("subject", TypeExpr::Str)
        .returns(TypeExpr::Nullable(Box::new(TypeExpr::Str)))
        .body(vec![
            s_foreach(
                e_var("pattern"),
                Some("regex"),
                "callback",
                vec![
                    // The backend needs a statically typed callback, so the mixed array value is
                    // reached through `call_user_func()`. The closure is bound to a local first:
                    // passed inline as a temporary, a capturing closure leaks one block per call.
                    s_assign(
                        "wrapped",
                        closure()
                            .param("m", t_array())
                            .captures("callback")
                            .body(vec![s_return(e_cast(
                                CastType::String,
                                e_call("call_user_func", vec![e_var("callback"), e_var("m")]),
                            ))])
                            .build(),
                    ),
                    s_assign(
                        "subject",
                        e_call(
                            "preg_replace_callback",
                            vec![
                                e_cast(CastType::String, e_var("regex")),
                                e_var("wrapped"),
                                e_var("subject"),
                            ],
                        ),
                    ),
                    s_if(
                        e_binop(e_var("subject"), BinOp::StrictEq, e_null()),
                        vec![s_return(e_null())],
                        vec![],
                        None,
                    ),
                ],
            ),
            s_return(e_var("subject")),
        ])
        .build()
}

/// Whether `program` calls a function this prelude declares: the usage half of the test `inject_if_used` applies.
///
/// Also run by `crate::autoload::prelude_probe` over the autoload closure, whose files are only
/// loaded after this prelude has been decided.
pub(crate) fn program_uses_surface(program: &[Stmt]) -> bool {
    SURFACE
        .iter()
        .any(|name| crate::opcache_prelude::detect::program_references(program, name))
}

/// Prepends `preg_grep()` when the program (or, with `autoloaded`, a file of its autoload
/// closure) references it and the program does not declare it.
///
/// Runs under `crate::compiler_stack::with_compiler_stack` like the version prelude, whose
/// usage scan (`opcache_prelude::detect`) it shares.
pub fn inject_if_used(
    program: Program,
    autoloaded: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !(autoloaded || program_uses_surface(&program)) {
            return program;
        }
        // Each function is guarded on its own: a user `preg_grep()` keeps `preg_quote()`.
        let mut combined: Program = pcre_declarations()
            .into_iter()
            .filter(|stmt| {
                !declared_name(stmt).is_some_and(|name| {
                    crate::opcache_prelude::detect::program_declares_global(&program, name)
                })
            })
            .collect();
        if combined.is_empty() {
            return program;
        }
        inventory.record_program("pcre", &combined);
        combined.extend(program);
        combined
    })
}

/// Builds the injected PCRE declarations.
pub(crate) fn pcre_declarations() -> Program {
    internal_declarations(|| vec![
            preg_grep_decl(),
            preg_quote_decl(),
            preg_filter_decl(),
            preg_replace_callback_array_decl(),
        ])
}

/// The function name a prelude declaration declares.
fn declared_name(stmt: &Stmt) -> Option<&str> {
    match &stmt.kind {
        crate::parser::ast::StmtKind::FunctionDecl { name, .. } => Some(name.as_str()),
        _ => None,
    }
}
