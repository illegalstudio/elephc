//! Purpose:
//! Emits PHP `phpversion` environment/platform information builtin calls.
//! Delegates host environment lookup or platform string construction to runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Environment and platform state are observable and must not be folded as compile-time constants here.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `phpversion()` builtin call.
///
/// Returns the compiler's Cargo package version string as a PHP string.
/// The version string address is materialized in `ptr_reg` and its byte length
/// in `len_reg` per the target ABI string return convention.
///
/// # Arguments
/// * `_name` — the builtin name (unused, dispatch already occurred)
/// * `_args` — the call arguments (phpversion takes none, ignored)
/// * `emitter` — the assembly emitter
/// * `_ctx` — codegen context (unused by this builtin)
/// * `data` — the data section where the version string is stored
///
/// # Returns
/// `Some(PhpType::Str)` since phpversion() always returns a string
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("phpversion()");
    // -- return the Cargo package version string --
    let version = env!("CARGO_PKG_VERSION").as_bytes();
    let (label, len) = data.add_string(version);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);                         // materialize the Cargo package version string in the active string-pointer result register
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);                 // publish the Cargo package version string length in the paired string-length result register
    Some(PhpType::Str)
}
