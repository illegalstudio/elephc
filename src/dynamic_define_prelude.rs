//! Purpose:
//! Builds the run-time constant table behind `define()` calls whose name is not a literal, as a
//! prelude declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends it when the program calls
//!   `define()` with a computed name (`crate::ast_usage::Usage::dynamic_define`).
//! - `crate::ir_lower::expr::constants`, which routes `define($name, ...)`, `defined()`,
//!   `constant()` and reads of names nothing defines statically to it.
//!
//! Key details:
//! - Every constant the compiler can name gets its own storage at compile time, so
//!   `define($k, $v)` in a loop or a helper was refused outright ("define() first argument must
//!   be a string literal"). The names such a call creates live in one static array instead.
//! - The table answers after the compiled constants: a defined name is never redefined
//!   (php's `Constant X already defined` warning, `false`), and a lookup falls back to the
//!   ordinary `defined()` / `constant()`, which this function's own calls reach unredirected.
//! - The declaration is BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_concat_all, e_const, e_index, e_int, e_str, e_var,
    function, php_semantics_declarations, s_array_assign, s_expr, s_if, s_return, s_static, t_mixed,
};

/// The table function every redirected call reaches: `(op, name, value)`.
pub(crate) const DYNAMIC_CONSTANTS_NAME: &str = "__elephc_dynamic_constants";
/// `define($name, $value)`: returns whether the name was free.
pub(crate) const OP_DEFINE: i64 = 0;
/// `defined($name)`.
pub(crate) const OP_DEFINED: i64 = 1;
/// `constant($name)` and a bare read of a name nothing defines statically.
pub(crate) const OP_FETCH: i64 = 2;

/// Returns the table declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![dynamic_constants()])
}

/// Builds `__elephc_dynamic_constants(int $op, string $name, mixed $value): mixed`.
fn dynamic_constants() -> Stmt {
    let known = || {
        e_binop(
            e_call("array_key_exists", vec![e_var("name"), e_var("values")]),
            BinOp::Or,
            e_call("defined", vec![e_var("name")]),
        )
    };
    function(DYNAMIC_CONSTANTS_NAME)
        .param("op", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .param("value", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_static("values", e_array(Vec::new())),
            s_if(
                e_binop(e_var("op"), BinOp::StrictEq, e_int(OP_DEFINE)),
                vec![
                    s_if(
                        known(),
                        vec![
                            s_expr(e_call(
                                "trigger_error",
                                vec![
                                    e_concat_all(vec![
                                        e_str("Constant "),
                                        e_var("name"),
                                        e_str(" already defined"),
                                    ]),
                                    // `trigger_error()` accepts only the E_USER_* levels (php throws a
                                    // ValueError for E_WARNING); E_USER_WARNING prints as "Warning".
                                    e_const("E_USER_WARNING"),
                                ],
                            )),
                            s_return(e_bool(false)),
                        ],
                        Vec::new(),
                        None,
                    ),
                    s_array_assign("values", e_var("name"), e_var("value")),
                    s_return(e_bool(true)),
                ],
                Vec::new(),
                None,
            ),
            s_if(
                e_binop(e_var("op"), BinOp::StrictEq, e_int(OP_DEFINED)),
                vec![s_return(known())],
                Vec::new(),
                None,
            ),
            s_if(
                e_call("array_key_exists", vec![e_var("name"), e_var("values")]),
                vec![s_return(e_index(e_var("values"), e_var("name")))],
                Vec::new(),
                None,
            ),
            s_return(e_call("constant", vec![e_var("name")])),
        ])
        .build()
}
