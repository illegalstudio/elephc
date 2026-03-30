use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(super) fn emit_list_unpack_stmt(
    vars: &[String],
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("list unpack");

    let arr_ty = emit_expr(value, emitter, ctx, data);
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };

    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer onto stack

    for (i, var_name) in vars.iter().enumerate() {
        let var = match ctx.variables.get(var_name) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined variable ${}", var_name));
                continue;
            }
        };
        let offset = var.stack_offset;

        emitter.instruction("ldr x9, [sp]");                                    // peek array pointer from stack
        match &elem_ty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header
                emitter.instruction(&format!("ldr x0, [x9, #{}]", i * 8));      // load element at index
                abi::store_at_offset(emitter, "x0", offset);
            }
            PhpType::Str => {
                emitter.instruction(&format!("add x9, x9, #{}", 24 + i * 16));  // offset to string slot
                emitter.instruction("ldr x1, [x9]");                            // load string pointer
                emitter.instruction("ldr x2, [x9, #8]");                        // load string length
                abi::store_at_offset(emitter, "x1", offset);
                abi::store_at_offset(emitter, "x2", offset - 8);
            }
            PhpType::Float => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header
                emitter.instruction(&format!("ldr d0, [x9, #{}]", i * 8));      // load float at index
                abi::store_at_offset(emitter, "d0", offset);
            }
            _ => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header
                emitter.instruction(&format!("ldr x0, [x9, #{}]", i * 8));      // load element at index
                abi::store_at_offset(emitter, "x0", offset);
            }
        }
        ctx.update_var_type_and_ownership(
            var_name,
            elem_ty.clone(),
            super::super::HeapOwnership::borrowed_alias_for_type(&elem_ty),
        );
    }

    emitter.instruction("add sp, sp, #16");                                     // pop saved array pointer
}
