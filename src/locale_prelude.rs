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
//! - Every top-level argument is checked against `array|string|null` with weak coercion
//!   before the first libc call; strict files get the same contract at compile time through
//!   the checker's prelude strict adapter.
//! - The `LC_*` and `CODESET` constants are predefined, target-dependent catalog entries;
//!   the prelude injects only the function declarations.
//! - A global user declaration (including a libc `extern`) wins; a declaration inside a
//!   named namespace does not hide the global function. Computed `eval()` source keeps both
//!   functions available.
//! - Built entirely from synthetic AST nodes, never parsed PHP source.

use crate::codegen_support::platform::Platform;
use crate::opcache_prelude::detect::{program_has_bridge_eval, program_references};
use crate::parser::ast::{BinOp, CastType, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::*;

/// PHP refuses a locale name of this many bytes or more before calling libc.
const LOCALE_NAME_LIMIT: i64 = 255;

/// Builds `setlocale(int $category, mixed $locales, mixed ...$rest): string|false`.
///
/// Mirrors php-src's `PHP_FUNCTION(setlocale)` in two passes. The first pass checks every
/// top-level argument against `array|string|null` with weak-mode coercion before any locale
/// changes, so an invalid later argument throws even when an earlier candidate would apply.
/// The second pass tries each candidate in order: array elements are expanded in place,
/// `null` and `""` take the locale from the environment, only `"0"` queries the current
/// locale, and a name of 255 bytes or more warns and moves on to the next candidate.
fn setlocale_function() -> Stmt {
    let coercible_scalar = e_binop(
        e_binop(
            e_call("is_string", vec![e_var("argument")]),
            BinOp::Or,
            e_call("is_int", vec![e_var("argument")]),
        ),
        BinOp::Or,
        e_binop(
            e_binop(
                e_call("is_float", vec![e_var("argument")]),
                BinOp::Or,
                e_call("is_bool", vec![e_var("argument")]),
            ),
            BinOp::Or,
            e_binop(
                e_call("is_object", vec![e_var("argument")]),
                BinOp::And,
                e_call(
                    "is_callable",
                    vec![e_array(vec![e_var("argument"), e_str("__toString")])],
                ),
            ),
        ),
    );
    // Weak coercion accepts every scalar and Stringable object, so only objects without
    // `__toString()` and resources reach the error. PHP names only the declared `$locales`
    // parameter; variadic positions have no parameter name in the message.
    let type_error = s_throw(e_new(
        "TypeError",
        vec![e_concat_all(vec![
            e_str("setlocale(): Argument #"),
            e_binop(e_var("position"), BinOp::Add, e_int(2)),
            e_ternary(
                e_binop(e_var("position"), BinOp::StrictEq, e_int(0)),
                e_str(" ($locales)"),
                e_str(""),
            ),
            e_str(" must be of type array|string|null, "),
            e_ternary(
                e_call("is_object", vec![e_var("argument")]),
                e_call("get_class", vec![e_var("argument")]),
                e_str("resource"),
            ),
            e_str(" given"),
        ])],
    ));
    let validate_argument = s_if(
        e_call("is_array", vec![e_var("argument")]),
        vec![s_foreach(
            e_var("argument"),
            None,
            "element",
            vec![s_array_push("candidates", e_var("element"))],
        )],
        vec![
            (
                e_binop(e_var("argument"), BinOp::StrictEq, e_null()),
                vec![s_array_push("candidates", e_str(""))],
            ),
            (
                coercible_scalar,
                vec![s_array_push(
                    "candidates",
                    e_cast(CastType::String, e_var("argument")),
                )],
            ),
        ],
        Some(vec![type_error]),
    );
    let try_candidate = vec![
        // Array elements convert here, in order, like php-src's `try_setlocale_zval()`:
        // `null` becomes "", a nested array warns and becomes "Array", and an object without
        // `__toString()` throws.
        s_assign("name", e_cast(CastType::String, e_var("locale"))),
        s_if(
            e_binop(e_var("name"), BinOp::StrictEq, e_str("0")),
            vec![s_return(e_call(
                "__elephc_setlocale_query",
                vec![e_var("category")],
            ))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("strlen", vec![e_var("name")]),
                BinOp::GtEq,
                e_int(LOCALE_NAME_LIMIT),
            ),
            vec![
                s_expr(e_call(
                    "__elephc_builtin_warning",
                    vec![e_str("setlocale(): Specified locale name is too long")],
                )),
                s_continue(1),
            ],
            vec![],
            None,
        ),
        s_assign(
            "applied",
            e_call(
                "__elephc_setlocale_set",
                vec![e_var("category"), e_var("name")],
            ),
        ),
        s_if(
            e_binop(e_var("applied"), BinOp::StrictNotEq, e_bool(false)),
            vec![s_return(e_var("applied"))],
            vec![],
            None,
        ),
    ];
    let body = vec![
        s_assign("arguments", e_array(vec![e_var("locales")])),
        s_foreach(
            e_var("rest"),
            None,
            "extra",
            vec![s_array_push("arguments", e_var("extra"))],
        ),
        s_assign("candidates", e_array(vec![])),
        s_foreach(e_var("arguments"), Some("position"), "argument", vec![validate_argument]),
        s_foreach(e_var("candidates"), None, "locale", try_candidate),
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

/// Reports whether the program references the locale surface without a global declaration.
///
/// A namespaced declaration does not satisfy a global `\setlocale()` call. Computed
/// `eval()` source can reach either function at runtime, which the literal-string detector
/// cannot see.
pub fn program_uses_locale(program: &[Stmt]) -> bool {
    program_has_bridge_eval(program)
        || (program_references(program, "setlocale")
            && !declares_global_locale_function(program, "setlocale"))
        || (program_references(program, "nl_langinfo")
            && !declares_global_locale_function(program, "nl_langinfo"))
}

/// Reports whether a top-level or global-namespace declaration owns `name`.
///
/// Declarations inside a named namespace are local to that namespace and must not suppress
/// the global prelude function a `\setlocale()` call still needs.
fn declares_global_locale_function(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name: declared, .. }
        | StmtKind::ExternFunctionDecl { name: declared, .. } => {
            declared.eq_ignore_ascii_case(name)
        }
        StmtKind::NamespaceBlock { name: namespace, body } => {
            namespace.is_none() && declares_global_locale_function(body, name)
        }
        StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
            declares_global_locale_function(body, name)
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
    if (runtime_eval || program_references(&program, "setlocale"))
        && !declares_global_locale_function(&program, "setlocale")
    {
        declarations.push(setlocale_function());
    }
    if (runtime_eval || program_references(&program, "nl_langinfo"))
        && !declares_global_locale_function(&program, "nl_langinfo")
    {
        declarations.push(nl_langinfo_function(platform));
    }
    declarations.extend(program);
    declarations
}
