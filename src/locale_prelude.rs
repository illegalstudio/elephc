//! Purpose:
//! Injects a real libc-backed `setlocale()` plus `nl_langinfo(CODESET)` for programs
//! that reference them.
//!
//! Called from:
//! - `crate::pipeline::compile()` before name resolution, and the codegen test helper.
//!
//! Key details:
//! - `setlocale()` is a prelude because PHP accepts a variadic list of locale names;
//!   each name is tried in order through the internal `__elephc_setlocale_set` builtin.
//! - `LC_*` constants use the host libc's category numbers, which differ between
//!   Apple's BSD libc and glibc.
//! - A user declaration wins; the prelude is skipped for that function.
//! - Built entirely from synthetic AST nodes, never parsed PHP source.

use crate::codegen_support::platform::Platform;
use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds `setlocale(int $category, mixed ...$locales): string|false`.
fn setlocale_function() -> Stmt {
    let locale = e_var("locale");
    let first_locale = e_index(e_var("locales"), e_int(0));
    let body = vec![
        s_if(
            e_binop(e_call("count", vec![e_var("locales")]), BinOp::StrictEq, e_int(0)),
            vec![s_return(e_call(
                "__elephc_setlocale_query",
                vec![e_var("category")],
            ))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_binop(first_locale, BinOp::StrictEq, e_int(0)),
                BinOp::Or,
                e_binop(e_index(e_var("locales"), e_int(0)), BinOp::StrictEq, e_str("0")),
            ),
            vec![s_return(e_call(
                "__elephc_setlocale_query",
                vec![e_var("category")],
            ))],
            vec![],
            None,
        ),
        s_foreach(
            e_var("locales"),
            None,
            "locale",
            vec![
                s_assign(
                    "applied",
                    e_call(
                        "__elephc_setlocale_set",
                        vec![e_var("category"), e_cast(CastType::String, locale)],
                    ),
                ),
                s_if(
                    e_binop(e_var("applied"), BinOp::StrictNotEq, e_bool(false)),
                    vec![s_return(e_var("applied"))],
                    vec![],
                    None,
                ),
            ],
        ),
        s_return(e_bool(false)),
    ];
    function("setlocale")
        .param("category", TypeExpr::Int)
        .variadic("locales", Some(t_mixed()))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(body)
        .build()
}

/// Builds `nl_langinfo(int $item): string|false`.
fn nl_langinfo_function() -> Stmt {
    let codeset_body = vec![
        s_assign(
            "locale",
            e_call("setlocale", vec![e_const("LC_CTYPE"), e_str("0")]),
        ),
        s_if(
            e_call("is_string", vec![e_var("locale")]),
            vec![
                s_assign("lower", e_call("strtolower", vec![e_var("locale")])),
                s_if(
                    e_call("str_contains", vec![e_var("lower"), e_str("utf-8")]),
                    vec![s_return(e_str("UTF-8"))],
                    vec![],
                    Some(vec![s_if(
                        e_call("str_contains", vec![e_var("lower"), e_str("koi8")]),
                        vec![s_return(e_str("KOI8-R"))],
                        vec![],
                        None,
                    )]),
                ),
                s_return(e_str("ANSI_X3.4-1968")),
            ],
            vec![],
            Some(vec![s_return(e_bool(false))]),
        ),
    ];
    let body = vec![s_if(
        e_binop(e_var("item"), BinOp::StrictEq, e_const("CODESET")),
        codeset_body,
        vec![],
        Some(vec![s_return(e_bool(false))]),
    )];
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

/// Returns the libc category numbers for the selected target platform.
fn locale_category_constants(platform: Platform) -> [(&'static str, i64); 7] {
    let lc_ctype = i64::from(platform.lc_ctype());
    let lc_all = i64::from(platform.lc_all());
    match platform {
        Platform::MacOS => [
            ("LC_ALL", lc_all),
            ("LC_COLLATE", 1),
            ("LC_CTYPE", lc_ctype),
            ("LC_MONETARY", 3),
            ("LC_NUMERIC", 4),
            ("LC_TIME", 5),
            ("LC_MESSAGES", 6),
        ],
        Platform::Linux => [
            ("LC_CTYPE", lc_ctype),
            ("LC_NUMERIC", 1),
            ("LC_TIME", 2),
            ("LC_COLLATE", 3),
            ("LC_MONETARY", 4),
            ("LC_MESSAGES", 5),
            ("LC_ALL", lc_all),
        ],
        Platform::Windows => [
            ("LC_CTYPE", lc_ctype),
            ("LC_NUMERIC", 1),
            ("LC_TIME", 2),
            ("LC_COLLATE", 3),
            ("LC_MONETARY", 4),
            ("LC_MESSAGES", 5),
            ("LC_ALL", lc_all),
        ],
    }
}

/// Reports whether the program already declares one of the injected locale constants.
fn declares_constant(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| matches!(&stmt.kind,
        crate::parser::ast::StmtKind::ConstDecl { name: declared, .. } if declared == name))
}

/// Prepends the real locale surface when the program references it.
pub fn inject_if_used(program: Program, used: bool, platform: Platform) -> Program {
    if !used {
        return program;
    }
    let detect = crate::opcache_prelude::detect::program_references;
    let declares = crate::opcache_prelude::detect::program_declares;
    let mut declarations = Vec::new();
    for (name, value) in locale_category_constants(platform) {
        if !declares_constant(&program, name) {
            declarations.push(s_const(name, e_int(value)));
        }
    }
    if !declares_constant(&program, "CODESET") {
        declarations.push(s_const("CODESET", e_int(14)));
    }
    let needs_nl_langinfo =
        detect(&program, "nl_langinfo") && !declares(&program, "nl_langinfo");
    // `nl_langinfo()` reads the current codeset through `setlocale(..., "0")`, so a
    // program that only references the former still needs the latter injected.
    if needs_nl_langinfo || (detect(&program, "setlocale") && !declares(&program, "setlocale")) {
        declarations.push(setlocale_function());
    }
    if needs_nl_langinfo {
        declarations.push(nl_langinfo_function());
    }
    declarations.extend(program);
    declarations
}
