//! Purpose:
//! Emits PHP `fgetcsv` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits code to call the PHP `fgetcsv` builtin, which reads one row from a CSV file into an array of strings.
///
/// Inputs:
/// - `args[0]`: a PHP stream resource whose file descriptor is extracted and passed to the runtime helper.
/// - `emitter`: target-aware instruction emitter.
/// - `ctx`: codegen context carrying target, layout, and state.
/// - `data`: mutable data section for relocatable labels.
///
/// Side effects:
/// - Calls `emit_stream_fd_arg` to unbox the stream resource to a raw file descriptor.
/// - On x86_64, moves the descriptor into `rdi` per the SysV ABI.
/// - Calls `__rt_fgetcsv` runtime helper which reads one CSV row and returns a string array.
///
/// Output:
/// - Returns `Some(PhpType::Array(Box::new(PhpType::Str)))` indicating the result is an array of strings.
/// - Returns `None` only on error path (handled by caller).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fgetcsv()");
    emit_stream_fd_arg("fgetcsv", &args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the file descriptor into the first SysV fgetcsv helper argument register
    }
    abi::emit_call_label(emitter, "__rt_fgetcsv");                              // read one CSV row through the target-aware runtime helper and return the resulting string array
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
