use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};

pub(crate) fn emit_store_mutating_arg(emitter: &mut Emitter, ctx: &Context, arg: &Expr) {
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };

    if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
        let label = format!("_gvar_{}", name);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.adrp("x9", &format!("{}", label));               // load page of global variable storage for the mutated array/hash
                emitter.add_lo12("x9", "x9", &format!("{}", label));     // resolve the global variable storage address
                emitter.instruction("str x0, [x9]");                     // overwrite the global slot with the updated container pointer
            }
            Arch::X86_64 => {
                abi::emit_store_reg_to_symbol(emitter, "rax", &label, 0);        // overwrite the global slot with the updated container pointer through the x86_64 symbol helper
            }
        }
    } else if ctx.ref_params.contains(name) {
        let offset = ctx
            .variables
            .get(name)
            .expect("codegen bug: missing ref-param slot for mutating array builtin")
            .stack_offset;
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x9", offset);                      // load the by-reference slot that points at the mutating argument storage
                emitter.instruction("str x0, [x9]");                             // overwrite the referenced slot with the updated container pointer
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r11", offset);                     // load the by-reference slot that points at the mutating argument storage
                abi::emit_store_to_address(emitter, "rax", "r11", 0);            // overwrite the referenced slot with the updated container pointer
            }
        }
    } else if let Some(var) = ctx.variables.get(name) {
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::store_at_offset(emitter, "x0", var.stack_offset);            // store the updated container pointer in the local variable slot
            }
            Arch::X86_64 => {
                abi::store_at_offset(emitter, "rax", var.stack_offset);           // store the updated container pointer in the local variable slot
            }
        }
    }
}
