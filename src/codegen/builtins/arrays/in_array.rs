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
    emitter.comment("in_array()");

    // -- evaluate array (second arg) first to get its type --
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);

    if let PhpType::AssocArray { value, .. } = &arr_ty {
        let val_ty = *value.clone();
        // -- save hash table pointer, evaluate needle --
        emitter.instruction("str x0, [sp, #-16]!");                                 // push hash table pointer

        let _needle_ty = emit_expr(&args[0], emitter, ctx, data);

        let found_label = ctx.next_label("in_array_assoc_found");
        let end_label = ctx.next_label("in_array_assoc_end");
        let loop_label = ctx.next_label("in_array_assoc_loop");
        let skip_label = ctx.next_label("in_array_assoc_skip");

        match &val_ty {
            PhpType::Str => {
                // -- needle is a string in x1/x2, save it --
                emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push needle ptr+len
            }
            _ => {
                // -- needle is an integer/bool in x0, save it --
                emitter.instruction("str x0, [sp, #-16]!");                          // push needle value
            }
        }

        // -- push iteration index onto stack --
        emitter.instruction("str xzr, [sp, #-16]!");                                 // push iter_index = 0

        // Stack layout (top to bottom):
        //   sp+0:  iter_index (16 bytes)
        //   sp+16: needle (16 bytes)
        //   sp+32: hash_table_ptr (16 bytes)

        emitter.label(&loop_label);
        emitter.instruction("ldr x0, [sp, #32]");                                   // load hash table pointer
        emitter.instruction("ldr x1, [sp]");                                         // load current iteration index
        emitter.instruction("bl __rt_hash_iter_next");                               // → x0=next_idx, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi
        // -- check if iteration is done --
        emitter.instruction("cmn x0, #1");                                           // check if x0 == -1 (end of iteration)
        emitter.instruction(&format!("b.eq {}", end_label));                         // if done, needle not found
        emitter.instruction("str x0, [sp]");                                         // save updated iteration index

        // -- compare value with needle --
        match &val_ty {
            PhpType::Str => {
                // -- compare string value (x3=ptr, x4=len) with needle --
                emitter.instruction("mov x1, x3");                                   // val ptr → x1
                emitter.instruction("mov x2, x4");                                   // val len → x2
                emitter.instruction("ldp x3, x4, [sp, #16]");                       // reload needle ptr+len from stack
                emitter.instruction("bl __rt_str_eq");                               // x0 = 1 if equal
                emitter.instruction(&format!("cbnz x0, {}", found_label));           // if equal, found
            }
            _ => {
                // -- compare integer value (x3) with needle --
                emitter.instruction("ldr x5, [sp, #16]");                            // reload needle value from stack
                emitter.instruction("cmp x3, x5");                                   // compare entry value with needle
                emitter.instruction(&format!("b.eq {}", found_label));               // if equal, found
            }
        }
        emitter.instruction(&format!("b {}", loop_label));                           // continue iterating

        // -- needle found --
        emitter.label(&found_label);
        emitter.instruction("mov x0, #1");                                           // return true
        emitter.instruction(&format!("b {}", skip_label));                           // jump to cleanup

        // -- needle not found --
        emitter.label(&end_label);
        emitter.instruction("mov x0, #0");                                           // return false

        emitter.label(&skip_label);
        // -- clean up stack (3 pushes of 16 bytes each) --
        emitter.instruction("add sp, sp, #48");                                      // drop iter_index + needle + hash_ptr
    } else {
        // -- indexed array: linear scan (existing logic) --
        emitter.instruction("str x0, [sp, #-16]!");                                  // push array pointer
        emit_expr(&args[0], emitter, ctx, data);

        let found_label = ctx.next_label("in_array_found");
        let end_label = ctx.next_label("in_array_end");
        let done_label = ctx.next_label("in_array_done");

        // -- set up loop to search array for needle --
        emitter.instruction("mov x11, x0");                                          // save needle value in x11
        emitter.instruction("ldr x0, [sp], #16");                                    // pop array pointer
        emitter.instruction("ldr x9, [x0]");                                         // load array length into x9
        emitter.instruction("add x10, x0, #24");                                     // x10 = pointer to data (past 24-byte header)
        emitter.instruction("mov x12, #0");                                          // initialize loop counter to 0
        let loop_label = ctx.next_label("in_array_loop");
        emitter.label(&loop_label);
        // -- compare each element against needle --
        emitter.instruction("cmp x12, x9");                                          // check if counter reached array length
        emitter.instruction(&format!("b.ge {}", end_label));                         // exit loop if all elements checked
        emitter.instruction("ldr x13, [x10, x12, lsl #3]");                          // load element at index x12 (offset = x12 * 8)
        emitter.instruction("cmp x13, x11");                                         // compare element with needle
        emitter.instruction(&format!("b.eq {}", found_label));                       // branch to found if element matches
        emitter.instruction("add x12, x12, #1");                                     // increment loop counter
        emitter.instruction(&format!("b {}", loop_label));                           // jump back to loop start
        // -- needle found --
        emitter.label(&found_label);
        emitter.instruction("mov x0, #1");                                           // set return value to 1 (true)
        emitter.instruction(&format!("b {}", done_label));                           // jump to done
        // -- needle not found --
        emitter.label(&end_label);
        emitter.instruction("mov x0, #0");                                           // set return value to 0 (false)
        emitter.label(&done_label);
    }

    Some(PhpType::Int)
}
