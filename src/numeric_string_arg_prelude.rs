//! Purpose:
//! Builds the weak-mode conversion of a `string` argument to an `int|float` math parameter
//! (`ceil()`, `floor()`, `round()`, `abs()`) as a prelude declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program calls one of those builtins.
//! - `crate::ir_lower::expr::compat_preludes`, which wraps a `string`-typed first argument in it.
//!
//! Key details:
//! - The native lowerings read an int or a float; a `string` operand was refused at code
//!   generation ("round for PHP type Str"), which interpreted symfony/form's
//!   `NumberToLocalizedStringTransformer` and symfony/validator's `FileValidator`.
//! - php accepts exactly the `is_numeric()` strings and converts them to the number they spell
//!   (`"-7"` is an int, `"2.5"` a float); every other string is its `TypeError`, measured with
//!   `php -n` 8.5: `ceil("12abc")` and `floor("")` both throw.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Program, Stmt};
use crate::synthetic_class::{
    e_binop, e_call, e_concat_all, e_int, e_new, e_str, e_var, function,
    php_semantics_declarations, s_if, s_return, s_throw,
};

/// Reserved helper a `string` first argument of a math builtin is passed through.
pub(crate) const NUMERIC_STRING_ARG_NAME: &str = "__elephc_numeric_string_arg";

/// Builtins whose first parameter is php's `int|float $num`.
pub(crate) const NUMERIC_ARG_BUILTINS: &[&str] = &["abs", "ceil", "floor", "round"];

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_numeric_string_arg()])
}

/// Builds `__elephc_numeric_string_arg($value, $function)`.
fn decl_fn_numeric_string_arg() -> Stmt {
    let body = vec![
        s_if(
            e_call("is_numeric", vec![e_var("value")]),
            vec![s_return(e_binop(e_var("value"), BinOp::Add, e_int(0)))],
            vec![],
            None,
        ),
        s_throw(e_new(
            "TypeError",
            vec![e_concat_all(vec![
                e_var("function"),
                e_str("(): Argument #1 ($num) must be of type int|float, string given"),
            ])],
        )),
    ];
    function(NUMERIC_STRING_ARG_NAME)
        .param_untyped("value")
        .param_untyped("function")
        .body(body)
        .build()
}
