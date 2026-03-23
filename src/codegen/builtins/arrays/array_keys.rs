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
    emitter.comment("array_keys()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(arr_ty, PhpType::AssocArray { .. }) {
        // -- associative array: iterate hash table and collect string keys --
        emitter.instruction("str x0, [sp, #-16]!");                             // push hash table pointer

        // -- allocate new string array for keys --
        emitter.instruction("ldr x0, [x0]");                                    // x0 = hash table count
        emitter.instruction("mov x1, #16");                                     // element size = 16 bytes (string: ptr+len)
        emitter.instruction("bl __rt_array_new");                               // allocate new array → x0
        emitter.instruction("str x0, [sp, #-16]!");                             // push new array pointer

        // -- push iteration index onto stack --
        emitter.instruction("str xzr, [sp, #-16]!");                            // push iter_index = 0

        // Stack: [iter_index(16)] [result_array(16)] [hash_ptr(16)]

        let loop_label = ctx.next_label("akeys_assoc_loop");
        let end_label = ctx.next_label("akeys_assoc_end");
        emitter.label(&loop_label);

        emitter.instruction("ldr x0, [sp, #32]");                               // load hash table pointer
        emitter.instruction("ldr x1, [sp]");                                    // load current iteration index
        emitter.instruction("bl __rt_hash_iter_next");                          // → x0=next_idx, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi
        emitter.instruction("cmn x0, #1");                                      // check if done
        emitter.instruction(&format!("b.eq {}", end_label));                    // if done, exit loop
        emitter.instruction("str x0, [sp]");                                    // save updated iteration index

        // -- push key string into result array --
        // x1=key_ptr, x2=key_len already set by hash_iter_next
        emitter.instruction("str x1, [sp, #-16]!");                             // save key_ptr (clobbered by push_str)
        emitter.instruction("str x2, [sp, #-16]!");                             // save key_len
        emitter.instruction("ldr x0, [sp, #48]");                               // load result array pointer (sp+16+16+16)
        emitter.instruction("ldr x2, [sp]");                                    // reload key_len
        emitter.instruction("ldr x1, [sp, #16]");                               // reload key_ptr
        emitter.instruction("bl __rt_array_push_str");                          // push string into result array
        emitter.instruction("add sp, sp, #32");                                 // drop saved key ptr+len
        emitter.instruction(&format!("b {}", loop_label));                      // continue iterating

        emitter.label(&end_label);
        // -- clean up and return result --
        emitter.instruction("add sp, sp, #16");                                 // drop iter_index
        emitter.instruction("ldr x0, [sp], #16");                               // pop result array pointer
        emitter.instruction("add sp, sp, #16");                                 // drop hash table pointer

        return Some(PhpType::Array(Box::new(PhpType::Str)));
    }

    // -- indexed array: return [0, 1, 2, ...] --
    emitter.instruction("ldr x9, [x0]");                                        // load source array length into x9
    emitter.instruction("str x9, [sp, #-16]!");                                 // push array length onto stack (for loop bound)
    emitter.instruction("mov x0, x9");                                          // pass length as capacity for new array
    emitter.instruction("mov x1, #8");                                          // element size = 8 bytes (integer keys)
    emitter.instruction("bl __rt_array_new");                                   // call runtime: allocate new array
    emitter.instruction("str x0, [sp, #-16]!");                                 // push new array pointer onto stack
    emitter.instruction("str xzr, [sp, #-16]!");                                // push loop counter (0) onto stack
    let loop_label = ctx.next_label("akeys_loop");
    let end_label = ctx.next_label("akeys_end");
    emitter.label(&loop_label);
    // -- loop: push each index as a key into result array --
    emitter.instruction("ldr x12, [sp]");                                       // load current loop counter from stack
    emitter.instruction("ldr x9, [sp, #32]");                                   // load array length from stack (2 slots above)
    emitter.instruction("cmp x12, x9");                                         // compare counter with array length
    emitter.instruction(&format!("b.ge {}", end_label));                        // exit loop if counter >= length
    emitter.instruction("ldr x0, [sp, #16]");                                   // load result array pointer from stack
    emitter.instruction("mov x1, x12");                                         // pass current index as value to push
    emitter.instruction("bl __rt_array_push_int");                              // call runtime: push index into result array
    emitter.instruction("ldr x12, [sp]");                                       // reload loop counter from stack
    emitter.instruction("add x12, x12, #1");                                    // increment loop counter
    emitter.instruction("str x12, [sp]");                                       // store updated counter back to stack
    emitter.instruction(&format!("b {}", loop_label));                          // jump back to loop start
    emitter.label(&end_label);
    // -- clean up stack and return result array --
    emitter.instruction("add sp, sp, #16");                                     // drop loop counter from stack
    emitter.instruction("ldr x0, [sp], #16");                                   // pop result array pointer into x0
    emitter.instruction("add sp, sp, #16");                                     // drop saved array length from stack

    Some(PhpType::Array(Box::new(PhpType::Int)))
}
