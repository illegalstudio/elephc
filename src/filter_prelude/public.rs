//! Purpose:
//! Builds `filter_var()`, the primary entry point of PHP's filter extension.
//!
//! Called from:
//! - `crate::filter_prelude::filter_declarations`.
//!
//! Key details:
//! - The top-level filter id is validated before options are parsed, matching php-src.
//! - Options parsing may replace the filter with the array form's `filter` key; unknown ids
//!   selected there fall back to unsafe_raw inside the scalar dispatcher.

use crate::parser::ast::{Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_bool, e_call, e_concat_all, e_const, e_int, e_not, e_null, e_str, e_var, function, s_assign,
    s_expr, s_if, s_return, t_mixed,
};

/// Returns the `filter_var()` declaration.
pub(super) fn declarations() -> Program {
    vec![filter_var_decl()]
}

/// Builds `filter_var(mixed $value, mixed $filter = FILTER_DEFAULT, mixed $options = 0):
/// mixed`.
fn filter_var_decl() -> Stmt {
    function("filter_var")
        .param("value", t_mixed())
        .param_default("filter", t_mixed(), e_const("FILTER_DEFAULT"))
        .param_default("options", t_mixed(), e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_assign("filter", e_call("__elephc_filter_int_arg", vec![
                e_var("filter"),
                e_str("filter_var"),
                e_int(2),
                e_str("filter"),
            ])),
            s_if(
                e_not(e_call("__elephc_filter_filter_exists", vec![e_var("filter")])),
                vec![
                    s_expr(e_call("trigger_error", vec![
                        e_concat_all(vec![
                            e_str("filter_var(): Unknown filter with ID "),
                            e_var("filter"),
                        ]),
                        e_const("E_USER_WARNING"),
                    ])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("state", crate::synthetic_class::e_array(vec![
                crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_var("filter")),
                e_const("FILTER_REQUIRE_SCALAR"),
                e_null(),
            ])),
            s_expr(e_call("__elephc_filter_parse_options", vec![
                e_var("options"),
                e_var("state"),
                e_str("filter_var"),
                e_int(3),
            ])),
            s_expr(e_call("__elephc_filter_check_flags", vec![
                crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, crate::synthetic_class::e_index(e_var("state"), e_int(1))),
                e_str("filter_var"),
                e_int(3),
            ])),
            s_return(e_call("__elephc_filter_walk", vec![
                e_var("value"),
                crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, crate::synthetic_class::e_index(e_var("state"), e_int(0))),
                crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, crate::synthetic_class::e_index(e_var("state"), e_int(1))),
                crate::synthetic_class::e_cast(
                    crate::parser::ast::CastType::Array,
                    crate::synthetic_class::e_ternary(
                        e_call("is_array", vec![e_var("options")]),
                        e_var("options"),
                        crate::synthetic_class::e_array(vec![]),
                    ),
                ),
                crate::synthetic_class::e_index(e_var("state"), e_int(2)),
                e_str("filter_var"),
                e_bool(true),
            ])),
        ])
        .build()
}

/// Keeps the unused expression/type imports attached to the module surface.
#[allow(dead_code)]
fn _markers() -> (Expr, TypeExpr) { (e_int(0), t_mixed()) }
