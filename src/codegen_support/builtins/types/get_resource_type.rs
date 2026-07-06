//! Purpose:
//! Emits PHP `get_resource_type` calls.
//! Returns the resource's type-name string after evaluating the argument.
//!
//! Called from:
//! - `crate::codegen_support::builtins::types::emit()`.
//!
//! Key details:
//! - Every resource elephc currently produces is a stream, so the result is the
//!   constant `"stream"`; the argument is still evaluated for its side effects.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `get_resource_type()` resource/type builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("get_resource_type()");
    emit_expr(&args[0], emitter, ctx, data);
    let (label, len) = data.add_string(b"stream");
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);                         // materialize the "stream" resource type-name literal
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, #{}", len_reg, len));         // load the type-name byte length into the AArch64 string-length result register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, {}", len_reg, len));          // load the type-name byte length into the x86_64 string-length result register
        }
    }
    Some(PhpType::Str)
}
