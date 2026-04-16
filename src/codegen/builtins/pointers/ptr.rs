use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

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
