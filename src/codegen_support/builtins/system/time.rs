//! Purpose:
//! Emits PHP `time` time/date builtin calls.
//! Marshals timestamp and format arguments into runtime helpers that consult wall-clock state.
//!
//! Called from:
//! - `crate::codegen_support::builtins::system::emit()`.
//!
//! Key details:
//! - Time calls are effectful/non-deterministic and must preserve PHP scalar return conventions.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `time()` builtin, which returns the current Unix timestamp.
///
/// `name` and `args` are ignored—`time()` takes no arguments. Calls the `__rt_time`
/// runtime helper, which returns the current wall-clock Unix timestamp in the native
/// integer result register (`x0` on ARM64). Returns `PhpType::Int` to indicate the
/// result type is a signed integer.
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("time()");
    abi::emit_call_label(emitter, "__rt_time");                                 // call the target-aware runtime helper that returns the current Unix timestamp in the native integer result register
    Some(PhpType::Int)
}
