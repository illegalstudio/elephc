//! Codegen stub for `class_alias`.
//!
//! Top-level `class_alias("Original", "Alias")` calls with literal
//! arguments are consumed at compile time by the autoload pass, which
//! synthesises a `class Alias extends Original {}` declaration. Calls
//! that don't fit that pattern (variable args, conditional sites the
//! collector skipped) reach this stub, which always returns `true`. The
//! arguments are still evaluated for side effects.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("class_alias() — AOT stub returns true");
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
    }
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    Some(PhpType::Bool)
}
