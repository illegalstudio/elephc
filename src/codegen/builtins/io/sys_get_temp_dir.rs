//! Purpose:
//! Emits PHP `sys_get_temp_dir` path-oriented builtin calls.
//! Marshals path strings into runtime helpers that normalize, split, or enumerate filesystem paths.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returned strings and arrays must use runtime allocation/layout compatible with PHP false-on-failure behavior.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the `sys_get_temp_dir` builtin, which returns "/tmp".
///
/// On call, this function:
///
/// 1. Adds the literal string "/tmp" to the data section and obtains its label and length.
/// 2. Materializes the string address into the ABI-defined string-pointer result register.
/// 3. Loads the string length into the ABI-defined string-length result register.
/// 4. Returns `PhpType::Str` to indicate the call produces a string result.
///
/// Arguments (`_args`) are ignored — `sys_get_temp_dir` takes no parameters in PHP.
///
/// # Returns
/// `Some(PhpType::Str)` on success, or `None` if the call should produce no value (not used here).
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sys_get_temp_dir()");
    let (lbl, len) = data.add_string(b"/tmp");
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &lbl);                           // materialize the hardcoded temp-directory string in the active string-pointer result register
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);                 // publish the hardcoded temp-directory string length in the paired string-length result register
    Some(PhpType::Str)
}
