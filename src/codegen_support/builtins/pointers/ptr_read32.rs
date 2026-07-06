//! Purpose:
//! Emits compiler-extension `ptr_read32` pointer operations.
//! Lowers raw address arithmetic, loads, or stores using the target ABI without PHP runtime boxing.
//!
//! Called from:
//! - `crate::codegen_support::builtins::pointers::emit()`.
//!
//! Key details:
//! - Pointer builtins are elephc extensions and must keep raw memory effects explicit and target-aware.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `ptr_read32` builtin: reads one 32-bit word through a pointer.
/// Checks non-null; zero-extends to `PhpType::Int`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ptr_read32() — read one 32-bit word at pointer address");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");                    // abort with a fatal error on null pointer dereference before reading from memory
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr w0, [x0]");                                // load one 32-bit word and zero-extend it through the AArch64 integer result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov eax, DWORD PTR [rax]");                    // load one 32-bit word and zero-extend it through the x86_64 integer result register
        }
    }
    Some(PhpType::Int)
}
