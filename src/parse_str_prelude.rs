//! Purpose:
//! Injects PHP's `parse_str` as a native call plus the configured `filter.default` post-pass.
//!
//! Called from:
//! - `crate::pipeline::compile()`, the codegen test harness, and the error-test harness.
//!
//! Key details:
//! - `parse_str` is a shared `PreludeProvided` contract; the injected PHP declaration is its
//!   implementation and declares the documented `array` by-reference result.
//! - Parsing reuses the native query engine through the internal byte-preserving builtin.
//!   With a configured `filter.default`, the parsed tree is filtered through the filter
//!   prelude's recursive default helper, which also converts rejected scalar values to
//!   the empty string PHP's input filter produces.
//! - The filter helper is referenced only when `filter.default` names a real filter, so a
//!   program using plain `parse_str` does not link the filter bridge.

use crate::parser::ast::{BinOp, CastType, Program, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_concat_all, e_int, e_str, e_var, function,
    s_assign, s_expr, s_if, s_return_void, t_array,
};

use crate::filter_prelude::request::FilterDefault;

/// Returns the public `parse_str` declaration for one baked filter.default configuration.
pub(crate) fn parse_str_declarations(config: &FilterDefault) -> Program {
    // Parse into a private array, then publish it before warning. The parser returns the
    // exceeded `max_input_vars` instead of warning, so an error handler sees the accepted
    // fields through `$result` and a throwing handler leaves them there.
    let mut body = vec![
        s_assign("parsed", e_array(vec![])),
        s_assign(
            "limit",
            e_call(
                "__elephc_parse_str",
                vec![e_var("string"), e_var("parsed")],
            ),
        ),
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
                        e_bool(true),
                    ],
                ),
            )],
            vec![],
            None,
        ));
    }
    body.push(s_assign("result", e_var("parsed")));
    body.push(s_if(
        e_binop(e_var("limit"), BinOp::Gt, e_int(0)),
        vec![s_expr(e_call(
            "__elephc_builtin_warning",
            vec![e_concat_all(vec![
                e_str("parse_str(): Input variables exceeded "),
                e_binop(e_var("limit"), BinOp::Sub, e_int(1)),
                e_str(". To increase the limit change max_input_vars in php.ini."),
            ])],
        ))],
        vec![],
        None,
    ));
    body.push(s_return_void());
    vec![function("parse_str")
        .param("string", TypeExpr::Str)
        // The documented `array &$result` keeps initialized arrays' storage; lowering seeds
        // an undefined, null, or non-array output with the empty array `zend_try_array_init()`
        // creates, so scalars and objects are replaced by the parsed array like PHP.
        .param_by_ref("result", Some(t_array()))
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
