use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
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
            if offset <= 4095 {
                emitter.instruction(&format!("sub x0, x29, #{}", offset));      // x0 = address of variable on stack
            } else {
                emitter.instruction(&format!("mov x9, #{}", offset));           // load large offset into scratch
                emitter.instruction("sub x0, x29, x9");                         // x0 = address of variable on stack
            }
        } else if let Some(label) = ctx.global_vars.get(var_name) {
            // Global variable — use its static storage address
            let label = label.clone();
            emitter.instruction(&format!("adrp x0, {}@PAGE", label));           // load page of global variable
            emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", label));     // resolve global variable address
        } else {
            // Variable not found — return null pointer
            emitter.instruction("mov x0, #0");                                  // null pointer for unknown variable
        }
    } else {
        // Non-variable argument — return null pointer
        emitter.instruction("mov x0, #0");                                      // null pointer (cannot take address of expression)
    }
    Some(PhpType::Pointer(None))
}
