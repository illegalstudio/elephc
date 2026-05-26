//! Purpose:
//! Emits PHP `json_last_error` JSON builtin calls.
//! Loads the runtime-global JSON error state as an integer result.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - JSON error state is runtime-global observable state and must stay coupled to json_last_error().

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `json_last_error()` builtin call.
///
/// Loads the last JSON error code from the runtime's `_json_last_error` BSS symbol
/// into the ABI-defined integer result register and returns `PhpType::Int`.
///
/// The symbol is updated by `json_encode`/`json_decode`/`json_validate` runtime
/// routines and zeroed on each successful entry; until those wirings land it
/// stays at 0 (JSON_ERROR_NONE).
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("json_last_error()");
    // Loads the last JSON error code from the runtime's BSS symbol. The
    // symbol is updated by encode/decode/validate runtimes and zeroed at
    // each successful entry; until those wirings land it stays at 0
    // (JSON_ERROR_NONE).
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), "_json_last_error", 0);
    Some(PhpType::Int)
}
