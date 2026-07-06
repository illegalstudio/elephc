//! Purpose:
//! Emits PHP `gethostname` calls.
//! Returns the system host name as an elephc string.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Delegates to the `__rt_gethostname` runtime helper, which leaves the host
//!   name in the standard string pointer/length result registers.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `gethostname()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    _args: &[Expr],
    emitter: &mut Emitter,
    _ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("gethostname()");
    abi::emit_call_label(emitter, "__rt_gethostname");
    Some(PhpType::Str)
}
