//! Purpose:
//! Declares the mixed-operand helper for PHP 8.6's `clamp()` through the synthetic AST pipeline.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness through `inject_if_used`,
//!   after include resolution and before name resolution.
//! - `crate::builtins::math::clamp`'s EIR lowering, which calls `__elephc_clamp()` whenever the
//!   three operands are not all int/float/string.
//!
//! Key details:
//! - The body is php-src's own userland polyfill: NaN bounds are rejected first, then
//!   `$max < $min`, then the upper-bound check, then the lower-bound check. PHP's ordinary
//!   comparison operators provide the cross-type rules, including internal object handlers.
//! - The helper copies one of its three arguments into a local and returns that local, so
//!   the result is an independently owned cell that still points at the same object.

use crate::parser::ast::Program;
use crate::synthetic_class::*;

/// Name of the internal helper invoked by the mixed-operand lowering.
pub(crate) const CLAMP_HELPER: &str = "__elephc_clamp";

/// Builds the `clamp()` mixed-operand helper declaration.
pub(crate) fn clamp_declarations() -> Program {
    internal_declarations(|| {
        vec![function(CLAMP_HELPER)
            .param("value", t_mixed())
            .param("min", t_mixed())
            .param("max", t_mixed())
            .returns(t_mixed())
            .body(vec![
                s_if(
                    e_binop(
                        e_call("\\is_float", vec![e_var("min")]),
                        crate::parser::ast::BinOp::And,
                        e_call("\\is_nan", vec![e_var("min")]),
                    ),
                    vec![s_throw(e_new_fq(
                        "\\ValueError",
                        vec![e_str(
                            "clamp(): Argument #2 ($min) must not be NAN",
                        )],
                    ))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(
                        e_call("\\is_float", vec![e_var("max")]),
                        crate::parser::ast::BinOp::And,
                        e_call("\\is_nan", vec![e_var("max")]),
                    ),
                    vec![s_throw(e_new_fq(
                        "\\ValueError",
                        vec![e_str(
                            "clamp(): Argument #3 ($max) must not be NAN",
                        )],
                    ))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("max"), crate::parser::ast::BinOp::Lt, e_var("min")),
                    vec![s_throw(e_new_fq(
                        "\\ValueError",
                        vec![e_str(
                            "clamp(): Argument #2 ($min) must be smaller than or equal to argument #3 ($max)",
                        )],
                    ))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("value"), crate::parser::ast::BinOp::Gt, e_var("max")),
                    vec![
                        s_assign("_clamp_result", e_var("max")),
                        s_return(e_var("_clamp_result")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("value"), crate::parser::ast::BinOp::Lt, e_var("min")),
                    vec![
                        s_assign("_clamp_result", e_var("min")),
                        s_return(e_var("_clamp_result")),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_clamp_result", e_var("value")),
                s_return(e_var("_clamp_result")),
            ])
            .build()]
    })
}

/// Returns whether the program calls `clamp()`.
pub(crate) fn program_uses_clamp(program: &[crate::parser::ast::Stmt]) -> bool {
    crate::prelude_prune::usage::collect(program)
        .functions
        .contains("clamp")
}

/// Injects the clamp helper when the program calls `clamp()`.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !force && !program_uses_clamp(&program) {
            return program;
        }
        let mut declarations = clamp_declarations();
        inventory.record_program("clamp", &declarations);
        declarations.extend(program);
        declarations
    })
}
