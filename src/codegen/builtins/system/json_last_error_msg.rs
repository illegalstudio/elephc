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

/// Emits code for the PHP `json_last_error_msg()` builtin, which returns the error message
/// from the last `json_encode()`, `json_decode()`, or `json_validate()` call.
///
/// Calls the runtime routine `__rt_json_last_error_msg`, which reads the runtime-global
/// JSON error state and returns a pointer/length string to the appropriate error description.
/// Returns `PhpType::Str`. Arguments are ignored (the function takes no parameters).
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
