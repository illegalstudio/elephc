//! Purpose:
//! Builds strict_types entry points for the weak-mode filter prelude.
//!
//! Called from:
//! - `crate::filter_prelude::filter_declarations`.
//! - `crate::ir_lower::expr::function_calls` when the caller's file declared strict_types.
//!
//! Key details:
//! - Public filter functions keep mixed parameters so weak-mode coercions stay at runtime.
//! - A strict wrapper validates the PHP signature first and then calls the public function.
//! - Wrappers use the same parameter names as the PHP signatures, so named arguments keep working.

use crate::parser::ast::{Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_concat_all, e_const, e_int, e_new_fq, e_str, e_var, function,
    s_assign, s_if, s_return, s_throw, t_bool, t_int, t_mixed,
};

/// Public filter functions and the strict wrapper that replaces them in strict files.
pub(crate) const WRAPPERS: &[(&str, &str)] = &[
    ("filter_var", "__elephc_filter_var_strict"),
    ("filter_var_array", "__elephc_filter_var_array_strict"),
    ("filter_input", "__elephc_filter_input_strict"),
    ("filter_input_array", "__elephc_filter_input_array_strict"),
    ("filter_has_var", "__elephc_filter_has_var_strict"),
    ("filter_id", "__elephc_filter_id_strict"),
];

/// Returns the strict wrapper for one canonical filter function name.
pub(crate) fn wrapper_name(name: &str) -> Option<&'static str> {
    let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    WRAPPERS
        .iter()
        .find_map(|(public, wrapper)| (*public == key).then_some(*wrapper))
}

/// Returns every strict wrapper, which lowering may reference without a source spelling.
pub(crate) fn retained_functions() -> impl Iterator<Item = &'static str> {
    WRAPPERS.iter().map(|(_, wrapper)| *wrapper)
}

/// Builds the strict validators and wrappers.
pub(super) fn declarations() -> Program {
    vec![
        strict_int_decl(),
        strict_string_decl(),
        strict_bool_decl(),
        strict_array_decl(),
        strict_options_decl(),
        wrapper_var_decl(),
        wrapper_var_array_decl(),
        wrapper_input_decl(),
        wrapper_input_array_decl(),
        wrapper_has_var_decl(),
        wrapper_id_decl(),
    ]
}

/// Builds one strict TypeError for a filter argument from the validator's caller/arg/param.
fn type_error(value: Expr, expected: &str) -> Stmt {
    s_throw(e_new_fq(
        "TypeError",
        vec![e_concat_all(vec![
            e_var("caller"),
            e_str("(): Argument #"),
            e_var("arg"),
            e_str(" ($"),
            e_var("param"),
            e_str(") must be of type "),
            e_str(expected),
            e_str(", "),
            e_call("__elephc_filter_type_name", vec![value]),
            e_str(" given"),
        ])],
    ))
}

/// Builds `__elephc_filter_strict_int(mixed, string, int, string): int`.
fn strict_int_decl() -> Stmt {
    function("__elephc_filter_strict_int")
        .param("value", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .param("param", TypeExpr::Str)
        .returns(t_int())
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            type_error(e_var("value"), "int"),
        ])
        .build()
}

/// Builds `__elephc_filter_strict_string(mixed, string, int, string): string`.
fn strict_string_decl() -> Stmt {
    function("__elephc_filter_strict_string")
        .param("value", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .param("param", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            type_error(e_var("value"), "string"),
        ])
        .build()
}

/// Builds `__elephc_filter_strict_bool(mixed, string, int, string): bool`.
fn strict_bool_decl() -> Stmt {
    function("__elephc_filter_strict_bool")
        .param("value", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .param("param", TypeExpr::Str)
        .returns(t_bool())
        .body(vec![
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            type_error(e_var("value"), "bool"),
        ])
        .build()
}

/// Builds `__elephc_filter_strict_array(mixed, string, int, string): array`.
fn strict_array_decl() -> Stmt {
    function("__elephc_filter_strict_array")
        .param("value", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .param("param", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            type_error(e_var("value"), "array"),
        ])
        .build()
}

/// Builds `__elephc_filter_strict_options(mixed, string, int, string): int|array`.
fn strict_options_decl() -> Stmt {
    function("__elephc_filter_strict_options")
        .param("value", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .param("param", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(
                    e_call("is_array", vec![e_var("value")]),
                    crate::parser::ast::BinOp::Or,
                    e_call("is_int", vec![e_var("value")]),
                ),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            type_error(e_var("value"), "array|int"),
        ])
        .build()
}

/// Builds `filter_var()`'s strict entry point.
fn wrapper_var_decl() -> Stmt {
    function("__elephc_filter_var_strict")
        .param("value", t_mixed())
        .param_default("filter", t_mixed(), e_const("FILTER_DEFAULT"))
        .param_default("options", t_mixed(), e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_assign(
                "filter",
                e_call("__elephc_filter_strict_int", vec![
                    e_var("filter"),
                    e_str("filter_var"),
                    e_int(2),
                    e_str("filter"),
                ]),
            ),
            s_assign(
                "options",
                e_call("__elephc_filter_strict_options", vec![
                    e_var("options"),
                    e_str("filter_var"),
                    e_int(3),
                    e_str("options"),
                ]),
            ),
            s_return(e_call("filter_var", vec![
                e_var("value"),
                e_var("filter"),
                e_var("options"),
            ])),
        ])
        .build()
}

/// Builds `filter_var_array()`'s strict entry point.
fn wrapper_var_array_decl() -> Stmt {
    function("__elephc_filter_var_array_strict")
        .param("array", t_mixed())
        .param_default("options", t_mixed(), e_const("FILTER_DEFAULT"))
        .param_default("add_empty", t_mixed(), e_bool(true))
        .returns(t_mixed())
        .body(vec![
            s_assign(
                "array",
                e_call("__elephc_filter_strict_array", vec![
                    e_var("array"),
                    e_str("filter_var_array"),
                    e_int(1),
                    e_str("array"),
                ]),
            ),
            s_assign(
                "options",
                e_call("__elephc_filter_strict_options", vec![
                    e_var("options"),
                    e_str("filter_var_array"),
                    e_int(2),
                    e_str("options"),
                ]),
            ),
            s_assign(
                "add_empty",
                e_call("__elephc_filter_strict_bool", vec![
                    e_var("add_empty"),
                    e_str("filter_var_array"),
                    e_int(3),
                    e_str("add_empty"),
                ]),
            ),
            s_return(e_call("filter_var_array", vec![
                e_var("array"),
                e_var("options"),
                e_var("add_empty"),
            ])),
        ])
        .build()
}

/// Builds `filter_input()`'s strict entry point.
fn wrapper_input_decl() -> Stmt {
    function("__elephc_filter_input_strict")
        .param("type", t_mixed())
        .param("var_name", t_mixed())
        .param_default("filter", t_mixed(), e_const("FILTER_DEFAULT"))
        .param_default("options", t_mixed(), e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_assign(
                "type",
                e_call("__elephc_filter_strict_int", vec![
                    e_var("type"),
                    e_str("filter_input"),
                    e_int(1),
                    e_str("type"),
                ]),
            ),
            s_assign(
                "var_name",
                e_call("__elephc_filter_strict_string", vec![
                    e_var("var_name"),
                    e_str("filter_input"),
                    e_int(2),
                    e_str("var_name"),
                ]),
            ),
            s_assign(
                "filter",
                e_call("__elephc_filter_strict_int", vec![
                    e_var("filter"),
                    e_str("filter_input"),
                    e_int(3),
                    e_str("filter"),
                ]),
            ),
            s_assign(
                "options",
                e_call("__elephc_filter_strict_options", vec![
                    e_var("options"),
                    e_str("filter_input"),
                    e_int(4),
                    e_str("options"),
                ]),
            ),
            s_return(e_call("filter_input", vec![
                e_var("type"),
                e_var("var_name"),
                e_var("filter"),
                e_var("options"),
            ])),
        ])
        .build()
}

/// Builds `filter_input_array()`'s strict entry point.
fn wrapper_input_array_decl() -> Stmt {
    function("__elephc_filter_input_array_strict")
        .param("type", t_mixed())
        .param_default("options", t_mixed(), e_const("FILTER_DEFAULT"))
        .param_default("add_empty", t_mixed(), e_bool(true))
        .returns(t_mixed())
        .body(vec![
            s_assign(
                "type",
                e_call("__elephc_filter_strict_int", vec![
                    e_var("type"),
                    e_str("filter_input_array"),
                    e_int(1),
                    e_str("type"),
                ]),
            ),
            s_assign(
                "options",
                e_call("__elephc_filter_strict_options", vec![
                    e_var("options"),
                    e_str("filter_input_array"),
                    e_int(2),
                    e_str("options"),
                ]),
            ),
            s_assign(
                "add_empty",
                e_call("__elephc_filter_strict_bool", vec![
                    e_var("add_empty"),
                    e_str("filter_input_array"),
                    e_int(3),
                    e_str("add_empty"),
                ]),
            ),
            s_return(e_call("filter_input_array", vec![
                e_var("type"),
                e_var("options"),
                e_var("add_empty"),
            ])),
        ])
        .build()
}

/// Builds `filter_has_var()`'s strict entry point.
fn wrapper_has_var_decl() -> Stmt {
    function("__elephc_filter_has_var_strict")
        .param("input_type", t_mixed())
        .param("var_name", t_mixed())
        .returns(t_bool())
        .body(vec![
            s_assign(
                "input_type",
                e_call("__elephc_filter_strict_int", vec![
                    e_var("input_type"),
                    e_str("filter_has_var"),
                    e_int(1),
                    e_str("input_type"),
                ]),
            ),
            s_assign(
                "var_name",
                e_call("__elephc_filter_strict_string", vec![
                    e_var("var_name"),
                    e_str("filter_has_var"),
                    e_int(2),
                    e_str("var_name"),
                ]),
            ),
            s_return(e_call("filter_has_var", vec![
                e_var("input_type"),
                e_var("var_name"),
            ])),
        ])
        .build()
}

/// Builds `filter_id()`'s strict entry point.
fn wrapper_id_decl() -> Stmt {
    function("__elephc_filter_id_strict")
        .param("name", t_mixed())
        .returns(crate::synthetic_class::t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(vec![
            s_assign(
                "name",
                e_call("__elephc_filter_strict_string", vec![
                    e_var("name"),
                    e_str("filter_id"),
                    e_int(1),
                    e_str("name"),
                ]),
            ),
            s_return(e_call("filter_id", vec![e_var("name")])),
        ])
        .build()
}
