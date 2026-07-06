//! Purpose:
//! Emits PHP `file` file input builtin calls.
//! Coordinates path or stream arguments with runtime helpers that allocate returned strings or arrays.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Failure paths must distinguish PHP false from empty string or empty array results.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `file` builtin call.
///
/// Reads an entire file into an array of lines, each line as a string.
/// Uses `__rt_file` runtime helper which returns a refcounted array of strings.
///
/// # Arguments
/// * `args[0]` - Path or stream expression to read from
///
/// # Returns
/// `PhpType::Array(Box::new(PhpType::Str))` on success; runtime helper handles
/// PHP false (file not found/empty) by returning an empty array.
///
/// # ABI
/// Calls `__rt_file` which materializes the path arg and returns the allocated array.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("file()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_file");                                 // call the target-aware runtime helper that reads the file into an array of lines
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
