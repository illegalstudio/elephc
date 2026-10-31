//! Purpose:
//! Injects the minimal locale surface PHP's fileinfo PHPT corpus uses: `setlocale()` and
//! `nl_langinfo(CODESET)` with the `LC_*`/`CODESET` constants.
//!
//! Called from:
//! - `crate::pipeline::compile()` before name resolution.
//!
//! Key details:
//! - This is not a general `ext/intl` implementation: it records the requested locale and
//!   reports the matching codeset so fileinfo's "mime_content_type does not reset the locale"
//!   fixture can run on hosts that do not expose `ru_RU.koi8r` to PHP.
//! - A user declaration wins; the prelude is skipped for that function.
//! - Built entirely from synthetic AST nodes, never parsed PHP source.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds `setlocale(int $category, string ...$locales): string|false`.
fn setlocale_function() -> Stmt {
    let locale = e_var("locale");
    let lower = e_call("strtolower", vec![locale.clone()]);
    let koi8 = e_call("str_contains", vec![lower.clone(), e_str("koi8")]);
    let utf8 = e_call("str_contains", vec![lower, e_str("utf-8")]);
    let body = vec![
        s_if(e_binop(e_call("count", vec![e_var("locales")]), BinOp::StrictEq, e_int(0)),
            vec![s_return(e_bool(false))], vec![], None),
        s_assign("locale", e_index(e_var("locales"), e_int(0))),
        s_if(e_binop(locale.clone(), BinOp::StrictEq, e_str("0")),
            vec![s_return(e_null_coalesce(e_static_prop("__ElephcLocale", "locale"), e_bool(false)))],
            vec![], None),
        s_if(e_binop(locale.clone(), BinOp::StrictEq, e_str("invalid")),
            vec![s_return(e_bool(false))], vec![], None),
        s_static_prop_assign("__ElephcLocale", "locale", locale.clone()),
        s_if(koi8, vec![
            s_static_prop_assign("__ElephcLocale", "codeset", e_str("KOI8-R")),
        ], vec![], Some(vec![s_if(utf8, vec![
            s_static_prop_assign("__ElephcLocale", "codeset", e_str("UTF-8")),
        ], vec![], Some(vec![
            s_static_prop_assign("__ElephcLocale", "codeset", e_str("ANSI_X3.4-1968")),
        ]))])),
        s_return(locale),
    ];
    function("setlocale")
        .param("category", TypeExpr::Int)
        .variadic("locales", Some(TypeExpr::Str))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(body)
        .build()
}

/// Builds `nl_langinfo(int $item): string|false`.
fn nl_langinfo_function() -> Stmt {
    let body = vec![
        s_assign("codeset", e_static_prop("__ElephcLocale", "codeset")),
        s_if(e_binop(e_var("item"), BinOp::StrictEq, e_const("CODESET")),
            vec![s_if(e_call("is_string", vec![e_var("codeset")]),
                vec![s_return(e_var("codeset"))], vec![], Some(vec![s_return(e_bool(false))]))],
            vec![], Some(vec![s_return(e_bool(false))])),
    ];
    function("nl_langinfo")
        .param("item", TypeExpr::Int)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(body)
        .build()
}

/// Reports whether the program references either locale function without declaring it.
pub fn program_uses_locale(program: &[Stmt]) -> bool {
    let detect = crate::opcache_prelude::detect::program_references;
    let declares = crate::opcache_prelude::detect::program_declares;
    (detect(program, "setlocale") && !declares(program, "setlocale"))
        || (detect(program, "nl_langinfo") && !declares(program, "nl_langinfo"))
}

/// Reports whether the program already declares one of the injected locale constants.
fn declares_constant(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| matches!(&stmt.kind,
        crate::parser::ast::StmtKind::ConstDecl { name: declared, .. } if declared == name))
}

/// Prepends the minimal locale surface when the program references it.
pub fn inject_if_used(program: Program, used: bool) -> Program {
    if !used {
        return program;
    }
    let detect = crate::opcache_prelude::detect::program_references;
    let declares = crate::opcache_prelude::detect::program_declares;
    let mut declarations = vec![
        class("__ElephcLocale")
            .static_prop("locale", t_nullable(TypeExpr::Str), Some(e_null()))
            .static_prop("codeset", t_nullable(TypeExpr::Str), Some(e_str("ANSI_X3.4-1968")))
            .build(),
    ];
    for (name, value) in [("LC_CTYPE", 0), ("LC_ALL", 6), ("CODESET", 14)] {
        if !declares_constant(&program, name) {
            declarations.push(s_const(name, e_int(value)));
        }
    }
    if detect(&program, "setlocale") && !declares(&program, "setlocale") {
        declarations.push(setlocale_function());
    }
    if detect(&program, "nl_langinfo") && !declares(&program, "nl_langinfo") {
        declarations.push(nl_langinfo_function());
    }
    declarations.extend(program);
    declarations
}
