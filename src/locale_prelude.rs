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
use crate::opcache_prelude::detect::{
    first_reference, program_has_bridge_eval, program_references, Symbol,
};
use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::*;

/// Builds `setlocale(int $category, mixed $locales, mixed ...$rest): string|false`.
fn setlocale_function() -> Stmt {
    let query = e_binop(
        e_binop(e_var("locales"), BinOp::StrictEq, e_int(0)),
        BinOp::Or,
        e_binop(
            e_binop(e_var("locales"), BinOp::StrictEq, e_str("0")),
            BinOp::Or,
            e_binop(e_var("locales"), BinOp::StrictEq, e_null()),
        ),
    );
    let add_candidate = |value: Expr| s_array_push("candidates", value);
    let flatten_into_candidates = |array: Expr, item: &str| {
        s_foreach(
            array,
            None,
            item,
            vec![add_candidate(e_var(item))],
        )
    };
    let body = vec![
        s_if(
            query,
            vec![s_return(e_call(
                "__elephc_setlocale_query",
                vec![e_var("category")],
            ))],
            vec![],
            None,
        ),
        s_assign("candidates", e_array(vec![])),
        s_if(
            e_call("is_array", vec![e_var("locales")]),
            vec![flatten_into_candidates(e_var("locales"), "locale")],
            vec![],
            Some(vec![add_candidate(e_var("locales"))]),
        ),
        s_foreach(
            e_var("rest"),
            None,
            "extra",
            vec![s_if(
                e_call("is_array", vec![e_var("extra")]),
                vec![flatten_into_candidates(e_var("extra"), "extra_locale")],
                vec![],
                Some(vec![add_candidate(e_var("extra"))]),
            )],
        ),
        s_foreach(
            e_var("candidates"),
            None,
            "locale",
            vec![
                s_assign(
                    "applied",
                    e_call(
                        "__elephc_setlocale_set",
                        vec![e_var("category"), e_cast(CastType::String, e_var("locale"))],
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
        .param("locales", t_mixed())
        .variadic("rest", Some(t_mixed()))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(body)
        .build()
}

/// Builds `nl_langinfo(int $item): string|false` for one target platform.
fn nl_langinfo_function(platform: Platform) -> Stmt {
    let codeset = match platform {
        Platform::MacOS => 0,
        Platform::Linux | Platform::Windows => 14,
    };
    let body = vec![s_if(
        e_binop(e_var("item"), BinOp::StrictEq, e_int(codeset)),
        vec![s_return(e_call(
            "__elephc_nl_langinfo",
            vec![e_var("item")],
        ))],
        vec![],
        Some(vec![s_return(e_bool(false))]),
    )];
    function("nl_langinfo")
        .param("item", TypeExpr::Int)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(body)
        .build()
}

/// Returns every PHP-visible locale declaration for the prelude parity audit.
#[cfg(test)]
pub(crate) fn locale_declarations() -> Program {
    vec![setlocale_function(), nl_langinfo_function(Platform::Linux)]
}

/// The locale constants the prelude can inject.
const LOCALE_CONSTANTS: [&str; 8] = [
    "CODESET",
    "LC_ALL",
    "LC_COLLATE",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LC_MONETARY",
    "LC_NUMERIC",
    "LC_TIME",
];

/// Reports whether the program references the locale surface without a global declaration.
///
/// A namespaced declaration does not satisfy a global `\setlocale()` call, and a constant
/// reference is enough to need the prelude even when no locale function is called. Computed
/// `eval()` source can reach either function at runtime, which the literal-string detector
/// cannot see.
pub fn program_uses_locale(program: &[Stmt]) -> bool {
    program_has_bridge_eval(program)
        || (program_references(program, "setlocale")
            && !declares_global_locale_function(program, "setlocale"))
        || (program_references(program, "nl_langinfo")
            && !declares_global_locale_function(program, "nl_langinfo"))
        || LOCALE_CONSTANTS
            .iter()
            .any(|name| first_reference(program, Symbol::constant(name)).is_some())
}

/// Reports whether a top-level or global-namespace declaration owns `name`.
///
/// Declarations inside a named namespace are local to that namespace and must not suppress
/// the global prelude function a `\setlocale()` call still needs.
fn declares_global_locale_function(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name: declared, .. } => declared.eq_ignore_ascii_case(name),
        StmtKind::NamespaceBlock { name: namespace, body } => {
            namespace.is_none() && declares_global_locale_function(body, name)
        }
        StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
            declares_global_locale_function(body, name)
        }
        _ => false,
    })
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

/// Reports whether a top-level or global-namespace declaration owns the constant `name`.
fn declares_global_locale_constant(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::ConstDecl { name: declared, .. } => declared == name,
        StmtKind::NamespaceBlock { name: namespace, body } => {
            namespace.is_none() && declares_global_locale_constant(body, name)
        }
        StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
            declares_global_locale_constant(body, name)
        }
        _ => false,
    })
}

/// Prepends the real locale surface when the program references it.
pub fn inject_if_used(program: Program, used: bool, platform: Platform) -> Program {
    if !used {
        return program;
    }
    let runtime_eval = program_has_bridge_eval(&program);
    let mut declarations = Vec::new();
    // Computed eval can reach the injected functions, but the constants are only needed
    // when the program names one; injecting unused constants would pollute the user
    // constant inventory (`get_defined_constants(true)`).
    let references_constants = LOCALE_CONSTANTS
        .iter()
        .any(|name| first_reference(&program, Symbol::constant(name)).is_some());
    if references_constants {
        for (name, value) in locale_category_constants(platform) {
            if !declares_global_locale_constant(&program, name) {
                declarations.push(s_const(name, e_int(value)));
            }
        }
        if !declares_global_locale_constant(&program, "CODESET") {
            let codeset = match platform {
                Platform::MacOS => 0,
                Platform::Linux | Platform::Windows => 14,
            };
            declarations.push(s_const("CODESET", e_int(codeset)));
        }
    }
    if runtime_eval
        || (program_references(&program, "setlocale")
            && !declares_global_locale_function(&program, "setlocale"))
    {
        declarations.push(setlocale_function());
    }
    if runtime_eval
        || (program_references(&program, "nl_langinfo")
            && !declares_global_locale_function(&program, "nl_langinfo"))
    {
        declarations.push(nl_langinfo_function(platform));
    }
    declarations.extend(program);
    declarations
}
