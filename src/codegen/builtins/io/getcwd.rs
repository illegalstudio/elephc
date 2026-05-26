//! Purpose:
//! Emits PHP `getcwd` I/O builtin calls.
//! Marshals PHP values into runtime helpers that interact with files, paths, streams, or stdout.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - I/O helpers are effectful and their false/null failure conventions are part of PHP compatibility.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a call to the `__rt_getcwd` runtime helper for PHP's `getcwd()` function.
///
/// `getcwd()` takes no arguments; the function name and argument list are ignored.
/// Returns `Some(PhpType::Str)` on success, or `None` if the current working directory
/// cannot be determined (the runtime helper handles the false/null failure convention).
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("getcwd()");
    abi::emit_call_label(emitter, "__rt_getcwd");                               // call the target-aware runtime helper that returns the current working directory as an owned string
    Some(PhpType::Str)
}
