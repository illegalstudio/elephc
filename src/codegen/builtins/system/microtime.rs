//! Purpose:
//! Emits PHP `microtime` time/date builtin calls.
//! Marshals timestamp and format arguments into runtime helpers that consult wall-clock state.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Time calls are effectful/non-deterministic and must preserve PHP scalar return conventions.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the `microtime([get_as_float])` builtin.
///
/// `microtime()` returns the current Unix timestamp with microsecond precision as a
/// float when `get_as_float` is true. The arguments are ignored—callers always invoke
/// `__rt_microtime` regardless of argument values.
///
/// Calls the target-aware runtime helper `__rt_microtime`, which is effectful
/// (reads wall-clock state) and non-deterministic. The result is returned in the
/// native float register.
///
/// Returns `PhpType::Float` unconditionally.
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("microtime(true)");
    abi::emit_call_label(emitter, "__rt_microtime");                            // call the target-aware runtime helper that returns the current Unix timestamp with microsecond precision in the native float result register
    Some(PhpType::Float)
}
