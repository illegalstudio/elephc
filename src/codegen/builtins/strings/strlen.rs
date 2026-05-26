//! Purpose:
//! Emits PHP `strlen` string builtin calls.
//! Coordinates string argument registers and runtime helper calls for PHP-compatible results.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - String ABI uses pointer/length pairs, with boxed results only where PHP returns mixed values.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `strlen` builtin call.
///
/// Takes one string argument and returns its length as an integer.
/// The string argument is emitted via `emit_string_arg` using the string ABI
/// (pointer/length pair). The result register receives the string-length value
/// from the ABI string result registers.
///
/// Returns `PhpType::Int` as the result type.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strlen()");
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    // -- return the string length as an integer --
    let (_, len_reg) = abi::string_result_regs(emitter);
    emitter.instruction(&format!("mov {}, {}", abi::int_result_reg(emitter), len_reg)); // move the ABI string-length register into the integer return register

    Some(PhpType::Int)
}
