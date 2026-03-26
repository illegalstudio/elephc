use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("unset()");
    if let crate::parser::ast::ExprKind::Variable(name) = &args[0].kind {
        let var = ctx.variables.get(name).expect("undefined variable");
        let offset = var.stack_offset;
        let old_ty = var.ty.clone();

        // -- free old heap value before unsetting --
        if matches!(&old_ty, PhpType::Str) {
            abi::load_at_offset(emitter, "x0", offset);                          // load heap pointer from variable
            emitter.instruction("bl __rt_heap_free_safe");                      // free old string if on heap
        } else if matches!(&old_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            abi::load_at_offset(emitter, "x0", offset);                          // load heap pointer from variable
            emitter.instruction("bl __rt_array_free_deep");                     // deep free array + string elements
        }

        // -- set variable to null sentinel value (0x7FFFFFFFFFFFFFFFE) --
        emitter.instruction("movz x0, #0xFFFE");                                // load null sentinel bits [15:0]
        emitter.instruction("movk x0, #0xFFFF, lsl #16");                       // load null sentinel bits [31:16]
        emitter.instruction("movk x0, #0xFFFF, lsl #32");                       // load null sentinel bits [47:32]
        emitter.instruction("movk x0, #0x7FFF, lsl #48");                       // load null sentinel bits [63:48]
        abi::store_at_offset(emitter, "x0", offset);                              // store null sentinel to variable's stack slot
        ctx.variables.get_mut(name).unwrap().ty = PhpType::Void;
    }
    Some(PhpType::Void)
}
