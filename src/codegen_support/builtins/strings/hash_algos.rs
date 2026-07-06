//! Purpose:
//! Emits PHP `hash_algos()` calls — returns the array of hash algorithm names
//! elephc-crypto supports. Delegates to the `__rt_hash_algos_list` runtime helper
//! that builds the string array.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - Takes no arguments; the supported-algorithm list lives in the runtime helper
//!   (`runtime::strings::hash_algos::HASH_ALGOS`), kept in lockstep with the crate.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `hash_algos()` builtin call: invokes `__rt_hash_algos_list`, which
/// returns a PHP array of the supported algorithm-name strings. Returns
/// `PhpType::Array(Box::new(PhpType::Str))`.
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash_algos()");
    abi::emit_call_label(emitter, "__rt_hash_algos_list");                      // build and return the supported-algorithm string array
    Some(PhpType::Array(Box::new(PhpType::Str)))
}
