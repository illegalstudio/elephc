use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::parser::ast::{Expr, ExprKind};

pub(crate) fn emit_store_mutating_arg(emitter: &mut Emitter, ctx: &Context, arg: &Expr) {
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };

    if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
        let label = format!("_gvar_{}", name);
        emitter.instruction(&format!("adrp x9, {}@PAGE", label));                  // load page of global variable storage for the mutated array/hash
        emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", label));            // resolve the global variable storage address
        emitter.instruction("str x0, [x9]");                                       // overwrite the global slot with the updated container pointer
    } else if ctx.ref_params.contains(name) {
        let offset = ctx
            .variables
            .get(name)
            .expect("codegen bug: missing ref-param slot for mutating array builtin")
            .stack_offset;
        abi::load_at_offset(emitter, "x9", offset); // load ref pointer
        emitter.instruction("str x0, [x9]");                                       // overwrite the referenced slot with the updated container pointer
    } else if let Some(var) = ctx.variables.get(name) {
        abi::store_at_offset(emitter, "x0", var.stack_offset); // store updated pointer in the local variable slot
    }
}
