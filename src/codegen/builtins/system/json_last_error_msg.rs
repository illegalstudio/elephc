//! Purpose:
//! Emits codegen for `json_last_error_msg()`.
//! Bridges the PHP builtin to the runtime lookup table that materializes the current JSON error string.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()` when lowering builtin calls.
//!
//! Key details:
//! - The runtime owns message selection; this emitter only performs the call and reports a string result.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("json_last_error_msg()");
    abi::emit_call_label(emitter, "__rt_json_last_error_msg");
    Some(PhpType::Str)
}
