//! Purpose:
//! Emits compiler-extension `ptr` pointer operations.
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
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits the `ptr` builtin: takes the address of a variable as a raw integer.
///
/// - **Local variable**: computes the frame-slot address and stores it in the integer result register.
/// - **Global variable**: materializes the static storage label address into the integer result register.
/// - **Unknown / non-variable**: returns a null pointer (all bits zero).
///
/// Returns `PhpType::Pointer(None)` — a raw address, not a PHP boxed value.
/// The result is placed in `abi::int_result_reg()` per the target ABI.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ptr() — take address of variable");
    // The argument must be a variable — we emit its stack address, not its value
    if let ExprKind::Variable(var_name) = &args[0].kind {
        if let Some(info) = ctx.variables.get(var_name) {
            let offset = info.stack_offset;
            // -- compute address of variable's stack slot --
            abi::emit_frame_slot_address(emitter, abi::int_result_reg(emitter), offset);
        } else if let Some(label) = ctx.global_vars.get(var_name) {
            // Global variable — use its static storage address
            let label = label.clone();
            abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &label); // materialize the global variable storage address for the active target ABI
        } else {
            // Variable not found — return null pointer
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, #0");                          // materialize a null pointer result when the requested variable storage does not exist on AArch64
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, 0");                          // materialize a null pointer result when the requested variable storage does not exist on x86_64
                }
            }
        }
    } else {
        // Non-variable argument — return null pointer
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #0");                              // materialize a null pointer when ptr() targets a non-addressable expression on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 0");                              // materialize a null pointer when ptr() targets a non-addressable expression on x86_64
            }
        }
    }
    Some(PhpType::Pointer(None))
}
