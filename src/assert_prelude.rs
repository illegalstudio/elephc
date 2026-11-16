//! Purpose:
//! Injects an elephc-PHP implementation of `assert()`'s single-argument form.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness before type checking.
//!
//! Key details:
//! - A truthy assertion returns true; a falsy assertion throws `AssertionError`.
//! - The optional description form stays on the native builtin path until Throwable-valued
//!   descriptions can be forwarded without losing their dynamic exception type.
//! - Built as AST, never parsed from PHP source.

use crate::optimize::reachability::PreludeInventory;
use crate::parser::ast::{Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_bool, e_new, e_not, e_str, e_var, function, php_semantics_declarations, s_if, s_return,
    s_throw, t_mixed,
};

/// Reserved function name used for one-argument `assert()` calls.
pub const ASSERT_ONE_ARG_NAME: &str = "__elephc_assert_one_arg";

/// `__elephc_assert_one_arg(mixed $assertion): bool`: true for a truthy assertion, php's
/// `AssertionError('assertion failed')` otherwise.
fn assert_one_arg() -> Stmt {
    function(ASSERT_ONE_ARG_NAME)
        .param("assertion", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_var("assertion")),
                vec![s_throw(e_new("AssertionError", vec![e_str("assertion failed")]))],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Prepends the helper when the resolved program references `assert`.
pub fn inject_if_used(program: Program, inventory: &mut PreludeInventory) -> Program {
    if !crate::ast_usage::collect(&program).references("assert") {
        return program;
    }
    let mut combined = php_semantics_declarations(|| vec![assert_one_arg()]);
    inventory.record_program("assert", &combined);
    combined.extend(program);
    combined
}
