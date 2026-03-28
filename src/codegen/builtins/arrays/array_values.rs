use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_values()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);

    if let PhpType::AssocArray { value, .. } = &arr_ty {
        let val_ty = *value.clone();
        // -- associative array: iterate hash table and collect values --
        emitter.instruction("str x0, [sp, #-16]!"); // push hash table pointer

        // -- allocate new indexed array for values --
        emitter.instruction("ldr x0, [x0]"); // x0 = hash table count
        let elem_size = match &val_ty {
            PhpType::Str => 16,
            _ => 8,
        };
        emitter.instruction(&format!("mov x1, #{}", elem_size)); // element size
        emitter.instruction("bl __rt_array_new"); // allocate new array → x0
        emitter.instruction("str x0, [sp, #-16]!"); // push new array pointer

        // -- push iteration index onto stack --
        emitter.instruction("str xzr, [sp, #-16]!"); // push iter_index = 0

        // Stack: [iter_index(16)] [result_array(16)] [hash_ptr(16)]

        let loop_label = ctx.next_label("avals_assoc_loop");
        let end_label = ctx.next_label("avals_assoc_end");
        emitter.label(&loop_label);

        emitter.instruction("ldr x0, [sp, #32]"); // load hash table pointer
        emitter.instruction("ldr x1, [sp]"); // load current iteration index
        emitter.instruction("bl __rt_hash_iter_next"); // → x0=next_idx, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi
        emitter.instruction("cmn x0, #1"); // check if done
        emitter.instruction(&format!("b.eq {}", end_label)); // if done, exit loop
        emitter.instruction("str x0, [sp]"); // save updated iteration index

        // -- push value into result array --
        match &val_ty {
            PhpType::Str => {
                // -- save value before function call --
                emitter.instruction("stp x3, x4, [sp, #-16]!"); // save val_ptr + val_len
                emitter.instruction("ldr x0, [sp, #32]"); // load result array (sp+16+16)
                emitter.instruction("ldr x1, [sp]"); // reload val_ptr
                emitter.instruction("ldr x2, [sp, #8]"); // reload val_len
                emitter.instruction("bl __rt_array_push_str"); // push string value
                emitter.instruction("add sp, sp, #16"); // drop saved val
            }
            _ => {
                emitter.instruction("str x3, [sp, #-16]!"); // save value
                if val_ty.is_refcounted() {
                    emitter.instruction("ldr x0, [sp]"); // reload borrowed heap pointer before result array takes ownership
                    emitter.instruction("bl __rt_incref"); // retain copied heap value for the new indexed array
                }
                emitter.instruction("ldr x0, [sp, #32]"); // load result array (sp+16+16)
                emitter.instruction("ldr x1, [sp]"); // reload value
                emitter.instruction("bl __rt_array_push_int"); // push int value
                emitter.instruction("add sp, sp, #16"); // drop saved val
            }
        }
        emitter.instruction(&format!("b {}", loop_label)); // continue iterating

        emitter.label(&end_label);
        // -- clean up and return result --
        emitter.instruction("add sp, sp, #16"); // drop iter_index
        emitter.instruction("ldr x0, [sp], #16"); // pop result array pointer
        emitter.instruction("add sp, sp, #16"); // drop hash table pointer

        return Some(PhpType::Array(Box::new(val_ty)));
    }

    // -- indexed array: array_values is a no-op, return same array type --
    Some(arr_ty)
}
