use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func()");

    // -- resolve callback function address at compile time --
    let func_name = match &args[0].kind {
        ExprKind::StringLiteral(name) => name.clone(),
        _ => panic!("call_user_func() callback must be a string literal"),
    };
    let label = format!("_fn_{}", func_name);

    // -- evaluate remaining arguments and push onto stack --
    let mut arg_types = Vec::new();
    for arg in &args[1..] {
        let ty = emit_expr(arg, emitter, ctx, data);
        match &ty {
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push string ptr+len onto stack
            }
            _ => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push int/bool/array arg onto stack
            }
        }
        arg_types.push(ty);
    }

    // -- pop arguments back into ABI registers in reverse --
    let mut int_reg = 0usize;
    let mut reg_assignments: Vec<(PhpType, usize)> = Vec::new();
    for ty in &arg_types {
        reg_assignments.push((ty.clone(), int_reg));
        int_reg += ty.register_count();
    }
    for i in (0..arg_types.len()).rev() {
        let (ty, start_reg) = &reg_assignments[i];
        match ty {
            PhpType::Str => {
                emitter.instruction(&format!(                                   // pop string ptr+len into registers
                    "ldp x{}, x{}, [sp], #16",
                    start_reg, start_reg + 1
                ));
            }
            _ => {
                emitter.instruction(&format!(                                   // pop int/bool/array arg into register
                    "ldr x{}, [sp], #16",
                    start_reg
                ));
            }
        }
    }

    // -- load callback address and call via blr --
    emitter.instruction(&format!("adrp x19, {}@PAGE", label));                  // load page address of callback function
    emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));           // resolve full address of callback function
    emitter.instruction("blr x19");                                             // call callback function via indirect branch

    // Return the callback's return type
    let ret_ty = ctx.functions
        .get(&func_name)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Int);

    Some(ret_ty)
}
