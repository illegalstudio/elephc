//! Purpose:
//! Injects PHP's `parse_str` as a native call plus the configured `filter.default` post-pass.
//!
//! Called from:
//! - `crate::pipeline::compile()`, the codegen test harness, and the error-test harness.
//!
//! Key details:
//! - `parse_str` is a shared `PreludeProvided` contract; the injected PHP declaration is its
//!   implementation and preserves the untyped by-reference result parameter.
//! - Parsing reuses the native `mb_parse_str` query engine. With a configured
//!   `filter.default`, the parsed tree is filtered through the filter prelude's recursive
//!   default helper, which also converts rejected scalar values to the empty string PHP's
//!   input filter produces.
//! - The filter helper is referenced only when `filter.default` names a real filter, so a
//!   program using plain `parse_str` does not link the filter bridge.

use crate::parser::ast::{BinOp, CastType, Program, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_int, e_str, e_var, function, s_assign, s_expr,
    s_if, s_return_void, t_mixed,
};

use crate::filter_prelude::request::FilterDefault;

/// Returns the public `parse_str` declaration for one baked filter.default configuration.
pub(crate) fn parse_str_declarations(config: &FilterDefault) -> Program {
    let mut body = vec![
        s_assign("parsed", e_array(vec![])),
        s_expr(e_call(
            "mb_parse_str",
            vec![e_var("string"), e_var("parsed")],
        )),
    ];
    if filter_applies(config) {
        body.push(s_if(
            e_binop(
                e_call("filter_id", vec![e_str(&config.name)]),
                BinOp::StrictNotEq,
                e_bool(false),
            ),
            vec![s_assign(
                "parsed",
                e_call(
                    "__elephc_filter_apply_default",
                    vec![
                        e_var("parsed"),
                        e_cast(
                            CastType::Int,
                            e_call("filter_id", vec![e_str(&config.name)]),
                        ),
                        e_int(config.flags),
                    ],
                ),
            )],
            vec![],
            None,
        ));
    }
    body.push(s_assign("result", e_var("parsed")));
    body.push(s_return_void());
    vec![function("parse_str")
        .param("string", TypeExpr::Str)
        .param_by_ref("result", Some(t_mixed()))
        .returns(TypeExpr::Void)
        .body(body)
        .build()]
}

/// Returns whether the baked directive names a filter other than raw pass-through.
fn filter_applies(config: &FilterDefault) -> bool {
    !config.name.is_empty() && config.name != "unsafe_raw"
}

/// Prepends `parse_str` when the program references it directly or through a literal name.
pub fn inject_if_used(
    program: Program,
    ini_overrides: &[(String, String)],
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let usage = crate::prelude_prune::usage::collect(&program);
    if !usage.references("parse_str") && !usage.literals.contains("parse_str") {
        return program;
    }
    let config = crate::filter_prelude::request::config_from_overrides(ini_overrides);
    let mut declarations = parse_str_declarations(&config);
    inventory.record_program("parse_str", &declarations);
    declarations.extend(program);
    declarations
}
