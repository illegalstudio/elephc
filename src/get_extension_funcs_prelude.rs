//! Purpose:
//! Injects the direct-AST weak string binder used by `get_extension_funcs()`.
//!
//! Called from:
//! - `crate::pipeline::compile()` before name resolution and type checking.
//!
//! Key details:
//! - PHP source is never embedded or parsed in production; the helper body is Rust-built AST.
//! - Injection is pay-for-use and rooted explicitly because the EIR builtin lowering, rather
//!   than the source AST, references the helper.

use crate::parser::ast::{BinOp, CastType, Program, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_call, e_cast, e_const, e_instance_of, e_new, e_not, e_null, e_str, e_ternary,
    e_var, function, internal_declarations, s_expr, s_if, s_return, s_throw,
};

/// Reachability group rooted when the builtin appears in user code.
pub(crate) const REACHABILITY_GROUP: &str = "get_extension_funcs_coercion";

/// Internal function called by EIR argument normalization.
pub(crate) const HELPER_NAME: &str = "__elephc_get_extension_funcs_name";

const TYPE_ERROR_PREFIX: &str =
    "get_extension_funcs(): Argument #1 ($extension) must be of type string, ";
const NULL_DEPRECATION: &str =
    "\nDeprecated: get_extension_funcs(): Passing null to parameter #1 ($extension) of type string is deprecated";

/// Returns whether the source program calls `get_extension_funcs()`.
pub(crate) fn program_uses_get_extension_funcs(program: &Program) -> bool {
    crate::prelude_prune::usage::collect(program).references("get_extension_funcs")
}

/// Builds the hidden weak string binder as direct Rust AST.
pub(crate) fn declarations() -> Program {
    internal_declarations(|| {
        let non_stringable_object = e_binop(
            e_call("is_object", vec![e_var("value")]),
            BinOp::And,
            e_not(e_instance_of(e_var("value"), "Stringable")),
        );
        let weak_rejected = e_binop(
            e_binop(
                e_call("is_array", vec![e_var("value")]),
                BinOp::Or,
                e_call("is_resource", vec![e_var("value")]),
            ),
            BinOp::Or,
            non_stringable_object,
        );
        let strict_rejected = e_binop(
            e_var("strict"),
            BinOp::And,
            e_not(e_call("is_string", vec![e_var("value")])),
        );
        let actual_type = e_ternary(
            e_call("is_int", vec![e_var("value")]),
            e_str("int"),
            e_ternary(
                e_call("is_float", vec![e_var("value")]),
                e_str("float"),
                e_ternary(
                    e_call("is_bool", vec![e_var("value")]),
                    e_str("bool"),
                    e_ternary(
                        e_call("is_null", vec![e_var("value")]),
                        e_str("null"),
                        e_ternary(
                            e_call("is_object", vec![e_var("value")]),
                            e_call("get_class", vec![e_var("value")]),
                            e_call("gettype", vec![e_var("value")]),
                        ),
                    ),
                ),
            ),
        );
        let type_error_message = e_binop(
            e_binop(e_str(TYPE_ERROR_PREFIX), BinOp::Concat, actual_type),
            BinOp::Concat,
            e_str(" given"),
        );

        vec![function(HELPER_NAME)
            .param("value", crate::synthetic_class::t_mixed())
            .param("line", TypeExpr::Int)
            .param("strict", TypeExpr::Bool)
            .returns(TypeExpr::Str)
            .body(vec![
                s_if(
                    strict_rejected,
                    vec![s_throw(e_new("TypeError", vec![type_error_message.clone()]))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                    vec![
                        s_expr(e_call(
                            "__elephc_diag_warning",
                            vec![e_str(NULL_DEPRECATION), e_var("line"), e_const("E_DEPRECATED")],
                        )),
                        s_return(e_str("")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    weak_rejected,
                    vec![s_throw(e_new("TypeError", vec![type_error_message]))],
                    vec![],
                    None,
                ),
                s_return(e_cast(CastType::String, e_var("value"))),
            ])
            .build()]
    })
}

/// Prepends the helper when the builtin is referenced and records its reachability group.
pub(crate) fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !program_uses_get_extension_funcs(&program) {
        return program;
    }
    let mut combined = declarations();
    inventory.record_program(REACHABILITY_GROUP, &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies injection is pay-for-use and contributes exactly one direct-AST declaration.
    #[test]
    fn helper_is_injected_only_for_get_extension_funcs() {
        let parse = |source: &str| {
            let tokens = crate::lexer::tokenize(source).expect("tokenize test source");
            crate::parser::parse(&tokens).expect("parse test source")
        };
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let unused = inject_if_used(parse("<?php echo 1;"), &mut inventory);
        assert_eq!(unused.len(), 1);

        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let used = inject_if_used(
            parse("<?php get_extension_funcs('date');"),
            &mut inventory,
        );
        assert_eq!(used.len(), 2);
    }
}
