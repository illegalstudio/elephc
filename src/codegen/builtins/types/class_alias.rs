//! Purpose:
//! Provides a defensive codegen fallback for unsupported `class_alias` calls.
//! Keeps the builtin dispatcher total even though valid AOT alias calls are consumed earlier.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`
//!
//! Key details:
//! - Top-level literal alias calls are compiled into synthetic subclass declarations by autoload.
//! - Any call reaching this file should already have been rejected by the checker.

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
    emitter.comment("class_alias() unsupported fallback");
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
    }
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    Some(PhpType::Bool)
}
