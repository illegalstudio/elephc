//! Purpose:
//! Injects PHP's `setlocale` as a prelude function over the compiler's supported locales.
//!
//! Called from:
//! - `crate::pipeline::compile()`, the codegen test harness, and the error-test harness.
//!
//! Key details:
//! - `setlocale` is a shared `PreludeProvided` contract; the injected PHP declaration is its
//!   implementation.
//! - elephc runs with C-locale semantics. The prelude therefore accepts PHP's canonical `C` and
//!   `POSIX` locale names and returns the matched name, exactly as php-src does, while every
//!   other locale name reports `false` (unavailable).
//! - Category values are the seven `LC_*` constants, whose numeric values are target-dependent
//!   and supplied by the shared constant catalog. A category outside that range is rejected.
//! - Multiple locale arguments are tried left to right, matching PHP's variadic signature.

use crate::parser::ast::{BinOp, CastType, Program, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_cast, e_int, e_str, e_var, function, s_if, s_return, t_mixed, t_union,
};

/// Returns the public `setlocale` declaration.
pub(crate) fn locale_declarations() -> Program {
    let supported = |name: &str| {
        e_binop(
            e_binop(e_var(name), BinOp::StrictEq, e_str("C")),
            BinOp::Or,
            e_binop(e_var(name), BinOp::StrictEq, e_str("POSIX")),
        )
    };
    vec![function("setlocale")
        .param("category", TypeExpr::Int)
        .param("locales", t_mixed())
        .variadic("rest", Some(t_mixed()))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            // PHP rejects category values outside the platform's LC_* range.
            s_if(
                e_binop(
                    e_binop(e_var("category"), BinOp::Lt, e_int(0)),
                    BinOp::Or,
                    e_binop(e_var("category"), BinOp::Gt, e_int(6)),
                ),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_if(
                supported("locales"),
                vec![s_return(e_cast(CastType::String, e_var("locales")))],
                vec![],
                None,
            ),
            crate::synthetic_class::s_foreach(
                e_var("rest"),
                None,
                "locale",
                vec![s_if(
                    supported("locale"),
                    vec![s_return(e_cast(CastType::String, e_var("locale")))],
                    vec![],
                    None,
                )],
            ),
            s_return(e_bool(false)),
        ])
        .build()]
}

/// Prepends `setlocale` when the program references it directly or through a literal name.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let usage = crate::prelude_prune::usage::collect(&program);
    if !usage.references("setlocale") && !usage.literals.contains("setlocale") {
        return program;
    }
    let mut declarations = locale_declarations();
    inventory.record_program("locale", &declarations);
    declarations.extend(program);
    declarations
}
