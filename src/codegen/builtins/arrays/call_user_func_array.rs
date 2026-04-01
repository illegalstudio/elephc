use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func_array()");

    // -- resolve callback function address and signature --
    let is_callable_expr = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let sig = if is_callable_expr {
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction("mov x19, x0");                                         // move synthesized callback address to x19
        ctx.deferred_closures
            .last()
            .expect("call_user_func_array: missing synthesized callable signature")
            .sig
            .clone()
    } else if let ExprKind::Variable(var_name) = &args[0].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, "x19", offset);                                // load callback address from callable variable
        ctx.closure_sigs
            .get(var_name)
            .expect("call_user_func_array: callable variable signature not found")
            .clone()
    } else {
        let func_name = match &args[0].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("call_user_func_array() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        emitter.instruction(&format!("adrp x19, {}@PAGE", label));                  // load page address of callback function
        emitter.instruction(&format!("add x19, x19, {}@PAGEOFF", label));           // resolve full address of callback
        ctx.functions
            .get(&func_name)
            .expect("call_user_func_array: function not found")
            .clone()
    };

    // Evaluate the array argument (second arg)
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);

    // Determine element type and size from the array type
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };
    let elem_size = match &elem_ty {
        PhpType::Str => 16,
        _ => 8,
    };

    // -- save array pointer --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack

    // -- extract elements from array into ABI registers --
    let mut int_reg = 0usize;
    let mut float_reg = 0usize;
    for (i, (_pname, pty)) in sig.params.iter().enumerate() {
        emitter.instruction("ldr x9, [sp]");                                    // peek array pointer from stack
        match pty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header
                emitter.instruction(&format!(                                   // load int element at index
                    "ldr x{}, [x9, #{}]", int_reg, i * elem_size
                ));
                int_reg += 1;
            }
            PhpType::Float => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header
                emitter.instruction(&format!(                                   // load float element at index
                    "ldr d{}, [x9, #{}]", float_reg, i * elem_size
                ));
                float_reg += 1;
            }
            PhpType::Str => {
                emitter.instruction(&format!(                                   // offset to string slot
                    "add x9, x9, #{}", 24 + i * elem_size
                ));
                emitter.instruction(&format!(                                   // load string pointer
                    "ldr x{}, [x9]", int_reg
                ));
                emitter.instruction(&format!(                                   // load string length
                    "ldr x{}, [x9, #8]", int_reg + 1
                ));
                int_reg += 2;
            }
            _ => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header
                emitter.instruction(&format!(                                   // load element at index
                    "ldr x{}, [x9, #{}]", int_reg, i * elem_size
                ));
                int_reg += 1;
            }
        }
    }

    // -- pop saved array pointer --
    emitter.instruction("add sp, sp, #16");                                     // clean up saved array pointer

    let ret_ty = sig.return_type.clone();

    // -- call callback via the resolved address in x19 --
    crate::codegen::expr::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // call callback via indirect branch
    crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, &ret_ty);

    Some(ret_ty)
}
