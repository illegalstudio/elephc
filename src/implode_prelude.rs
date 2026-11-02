//! Purpose:
//! Builds the `implode()` fallback for element types the native joiners cannot render, as a
//! prelude declaration.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends it when the program references
//!   `implode` or `join`.
//! - `crate::ir_lower::expr::compat_preludes::lower_backend_gap_builtin_shape`, which redirects a
//!   call here when its array's element type needs it.
//!
//! Key details:
//! - The native renderers cover string, int and bool elements, and boxed `mixed` ones whose
//!   payload is a scalar. An array of objects or floats was refused at compile time
//!   (`unsupported EIR backend feature: implode array element PHP type Object("T")`), and a
//!   boxed object joined as an empty string where php calls `__toString()`:
//!
//!   ```text
//!       $a = [new T("a"), 1];     echo implode(",", $a);
//!       php     ia,1                 elephc  ,1
//!   ```
//!
//! - Joining by concatenation reuses the compiler's own string conversion of each element, which
//!   is php's: `__toString()` (or php's `Error` for an object without one), php's float format,
//!   `"Array"` with its warning.
//! - The declaration is BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_bool, e_concat, e_not, e_str, e_var, function, php_semantics_declarations, s_assign, s_foreach,
    s_if, s_return, t_array,
};

/// Reserved helper the lowering redirects such `implode()` / `join()` calls to.
pub(crate) const IMPLODE_VALUES_NAME: &str = "__elephc_implode_values";

/// Returns the `implode()` fallback declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![implode_values()])
}

/// Builds `__elephc_implode_values(string $separator, array $array): string`.
///
/// Keys are ignored, as php ignores them.
fn implode_values() -> Stmt {
    function(IMPLODE_VALUES_NAME)
        .param("separator", TypeExpr::Str)
        .param("array", t_array())
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("joined", e_str("")),
            s_assign("first", e_bool(true)),
            s_foreach(
                e_var("array"),
                None,
                "value",
                vec![
                    s_if(
                        e_not(e_var("first")),
                        vec![s_assign(
                            "joined",
                            e_concat(e_var("joined"), e_var("separator")),
                        )],
                        Vec::new(),
                        None,
                    ),
                    s_assign("first", e_bool(false)),
                    s_assign("joined", e_concat(e_var("joined"), e_var("value"))),
                ],
            ),
            s_return(e_var("joined")),
        ])
        .build()
}
