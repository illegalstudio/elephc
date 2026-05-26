//! Purpose:
//! Emits compiler-extension `ptr_read16` pointer operations.
//! Lowers little-endian 16-bit raw memory reads with target-specific load instructions.
//!
//! Called from:
//! - `crate::codegen::builtins::pointers::emit()`.
//!
//! Key details:
//! - Reads must check null pointers first and zero-extend the 16-bit payload to PHP int.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `ptr_read16` builtin: reads one unsigned 16-bit word through a pointer.
/// Checks for null before reading; aborts with a fatal error if the pointer is null.
/// Zero-extends the 16-bit payload into a PHP integer (`PhpType::Int`).
///
/// # Arguments
/// - `_name`: unused, matches the builtin dispatcher signature
/// - `args`: single expression producing the pointer value
/// - `emitter`: target assembly emitter
/// - `ctx`: codegen context (contains target, variable layout, etc.)
/// - `data`: mutable data section for literals/relocs
///
/// # Returns
/// `Some(PhpType::Int)` — the result type is always a PHP integer.
///
/// # Side effects
/// - Calls `__rt_ptr_check_nonnull` which aborts if the pointer is null
/// - Clobbers `x0`/`rax` (the loaded integer value) and `x1`/`rax` may be used as scratch
/// - Returns the result value in the integer register per target ABI
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ptr_read16() — read two bytes at pointer address");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");                    // abort with a fatal error on null pointer dereference before reading from memory
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldrh w0, [x0]");                               // load one unsigned 16-bit word and zero-extend it through the AArch64 integer result register
        }
        Arch::X86_64 => {
            emitter.instruction("movzx eax, WORD PTR [rax]");                   // load one unsigned 16-bit word and zero-extend it through the x86_64 integer result register
        }
    }
    Some(PhpType::Int)
}
