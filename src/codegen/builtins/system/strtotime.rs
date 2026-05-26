//! Purpose:
//! Emits PHP `strtotime` time/date builtin calls.
//! Marshals timestamp and format arguments into runtime helpers that consult wall-clock state.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Time calls are effectful/non-deterministic and must preserve PHP scalar return conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `strtotime` builtin call.
///
/// Parses a date/time string and returns a Unix timestamp (seconds since epoch).
/// The first argument (date string) is evaluated and passed as a runtime string
/// to `__rt_strtotime`. On x86_64, the string pointer and length are loaded into
/// the SysV string-argument registers (rdi, rsi) before the call.
///
/// # Returns
/// `PhpType::Int` — the parsed timestamp, or -1 on parse failure (matching PHP behavior).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strtotime()");

    // -- evaluate date string argument --
    emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the input string pointer into the first SysV string-argument register
        emitter.instruction("mov rsi, rdx");                                    // move the input string length into the paired SysV string-argument register
    }

    // -- call runtime to parse date string and return timestamp --
    abi::emit_call_label(emitter, "__rt_strtotime");                            // parse the supported date/time string formats through the target-aware runtime helper

    Some(PhpType::Int)
}
