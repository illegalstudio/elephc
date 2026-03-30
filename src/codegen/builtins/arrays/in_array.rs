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
        emitter.instruction("str x0, [sp, #-16]!");                             // push hash table pointer

        let needle_ty = emit_expr(&args[0], emitter, ctx, data);

        let found_label = ctx.next_label("in_array_assoc_found");
        let end_label = ctx.next_label("in_array_assoc_end");
        let loop_label = ctx.next_label("in_array_assoc_loop");
        let skip_label = ctx.next_label("in_array_assoc_skip");

        match &val_ty {
            PhpType::Str => {
                // -- needle is a string in x1/x2, save it --
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push needle ptr+len
            }
            PhpType::Mixed if matches!(needle_ty, PhpType::Str) => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push string needle ptr+len for mixed-entry comparison
            }
            PhpType::Mixed if matches!(needle_ty, PhpType::Float) => {
                emitter.instruction("fmov x0, d0");                             // move float needle bits into an integer register for mixed-entry comparison
                emitter.instruction("str x0, [sp, #-16]!");                     // push float needle bits
            }
            _ => {
                // -- needle is an integer/bool in x0, save it --
                emitter.instruction("str x0, [sp, #-16]!");                     // push needle value
            }
        }

        // -- push iteration index onto stack --
        emitter.instruction("str xzr, [sp, #-16]!");                            // push iter_cursor = 0 (start from hash header head)

        // Stack layout (top to bottom):
        //   sp+0:  iter_index (16 bytes)
        //   sp+16: needle (16 bytes)
        //   sp+32: hash_table_ptr (16 bytes)

        emitter.label(&loop_label);
        emitter.instruction("ldr x0, [sp, #32]");                               // load hash table pointer
        emitter.instruction("ldr x1, [sp]");                                    // load current iteration cursor
        emitter.instruction("bl __rt_hash_iter_next");                          // → x0=next_cursor, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag
        // -- check if iteration is done --
        emitter.instruction("cmn x0, #1");                                      // check if x0 == -1 (end of iteration)
        emitter.instruction(&format!("b.eq {}", end_label));                    // if done, needle not found
        emitter.instruction("str x0, [sp]");                                    // save updated iteration cursor

        // -- compare value with needle --
        match &val_ty {
            PhpType::Str => {
                // -- compare string value (x3=ptr, x4=len) with needle --
                emitter.instruction("mov x1, x3");                              // val ptr → x1
                emitter.instruction("mov x2, x4");                              // val len → x2
                emitter.instruction("ldp x3, x4, [sp, #16]");                   // reload needle ptr+len from stack
                emitter.instruction("bl __rt_str_eq");                          // x0 = 1 if equal
                emitter.instruction(&format!("cbnz x0, {}", found_label));      // if equal, found
            }
            PhpType::Mixed => {
                let mixed_mismatch_label = ctx.next_label("in_array_assoc_mixed_mismatch");
                let expected_tag = crate::codegen::runtime_value_tag(&needle_ty);
                emitter.instruction(&format!("mov x6, #{}", expected_tag));     // materialize the expected mixed-entry value_tag for the needle
                emitter.instruction("cmp x5, x6");                              // does this entry's runtime value_tag match the needle type?
                emitter.instruction(&format!("b.ne {}", mixed_mismatch_label)); // skip entries whose runtime value kind differs from the needle
                match &needle_ty {
                    PhpType::Str => {
                        emitter.instruction("mov x1, x3");                      // val ptr → x1
                        emitter.instruction("mov x2, x4");                      // val len → x2
                        emitter.instruction("ldp x3, x4, [sp, #16]");           // reload needle ptr+len from stack
                        emitter.instruction("bl __rt_str_eq");                  // x0 = 1 if equal
                        emitter.instruction(&format!("cbnz x0, {}", found_label)); //if equal, found
                    }
                    PhpType::Void => {
                        emitter.instruction(&format!("b {}", found_label));     // null needles match any entry tagged null
                    }
                    _ => {
                        emitter.instruction("ldr x6, [sp, #16]");               // reload the saved needle payload for mixed-entry comparison
                        emitter.instruction("cmp x3, x6");                      // compare entry value_lo against the needle payload
                        emitter.instruction(&format!("b.eq {}", found_label));  // if equal, found
                    }
                }
                emitter.label(&mixed_mismatch_label);
            }
            _ => {
                // -- compare integer value (x3) with needle --
                emitter.instruction("ldr x5, [sp, #16]");                       // reload needle value from stack
                emitter.instruction("cmp x3, x5");                              // compare entry value with needle
                emitter.instruction(&format!("b.eq {}", found_label));          // if equal, found
            }
        }
        emitter.instruction(&format!("b {}", loop_label));                      // continue iterating

        // -- needle found --
        emitter.label(&found_label);
        emitter.instruction("mov x0, #1");                                      // return true
        emitter.instruction(&format!("b {}", skip_label));                      // jump to cleanup

        // -- needle not found --
        emitter.label(&end_label);
        emitter.instruction("mov x0, #0");                                      // return false

        emitter.label(&skip_label);
        // -- clean up stack (3 pushes of 16 bytes each) --
        emitter.instruction("add sp, sp, #48");                                 // drop iter_index + needle + hash_ptr
    } else {
        // -- indexed array: linear scan --
        let elem_ty = match &arr_ty {
            PhpType::Array(t) => *t.clone(),
            _ => PhpType::Int,
        };

        emitter.instruction("str x0, [sp, #-16]!");                             // push array pointer
        let _needle_ty = emit_expr(&args[0], emitter, ctx, data);

        let found_label = ctx.next_label("in_array_found");
        let end_label = ctx.next_label("in_array_end");
        let done_label = ctx.next_label("in_array_done");
        let loop_label = ctx.next_label("in_array_loop");

        match &elem_ty {
            PhpType::Str => {
                // -- save needle string (x1=ptr, x2=len) and set up loop --
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push needle ptr+len
                emitter.instruction("ldr x0, [sp, #16]");                       // reload array pointer
                emitter.instruction("ldr x9, [x0]");                            // load array length
                emitter.instruction("add x10, x0, #24");                        // x10 = pointer to data region
                emitter.instruction("mov x12, #0");                             // initialize loop counter

                // Stack layout:
                //   sp+0:  needle ptr+len (16 bytes)
                //   sp+16: array pointer (16 bytes)

                emitter.label(&loop_label);
                // -- check if all elements have been scanned --
                emitter.instruction("cmp x12, x9");                             // check if counter reached array length
                emitter.instruction(&format!("b.ge {}", end_label));            // exit loop if all elements checked

                // -- load string element at index x12 (16 bytes per element) --
                emitter.instruction("lsl x13, x12, #4");                        // x13 = index * 16
                emitter.instruction("ldr x1, [x10, x13]");                      // x1 = element string pointer
                emitter.instruction("add x14, x13, #8");                        // x14 = offset to length field
                emitter.instruction("ldr x2, [x10, x14]");                      // x2 = element string length

                // -- save loop state before calling __rt_str_eq --
                emitter.instruction("stp x9, x10, [sp, #-16]!");                // push array len + data ptr
                emitter.instruction("str x12, [sp, #-16]!");                    // push loop counter

                // -- load needle and compare --
                emitter.instruction("ldp x3, x4, [sp, #32]");                   // reload needle ptr+len from stack
                emitter.instruction("bl __rt_str_eq");                          // x0 = 1 if strings are equal

                // -- restore loop state --
                emitter.instruction("ldr x12, [sp], #16");                      // pop loop counter
                emitter.instruction("ldp x9, x10, [sp], #16");                  // pop array len + data ptr

                emitter.instruction(&format!("cbnz x0, {}", found_label));      // if equal, found
                emitter.instruction("add x12, x12, #1");                        // increment loop counter
                emitter.instruction(&format!("b {}", loop_label));              // continue searching

                // -- needle found --
                emitter.label(&found_label);
                emitter.instruction("mov x0, #1");                              // return true
                emitter.instruction(&format!("b {}", done_label));              // jump to cleanup

                // -- needle not found --
                emitter.label(&end_label);
                emitter.instruction("mov x0, #0");                              // return false

                emitter.label(&done_label);
                // -- clean up stack (needle + array pointer) --
                emitter.instruction("add sp, sp, #32");                         // drop needle + array ptr
            }
            _ => {
                // -- integer/bool needle: simple comparison loop --
                emitter.instruction("mov x11, x0");                             // save needle value in x11
                emitter.instruction("ldr x0, [sp], #16");                       // pop array pointer
                emitter.instruction("ldr x9, [x0]");                            // load array length into x9
                emitter.instruction("add x10, x0, #24");                        // x10 = pointer to data (past 24-byte header)
                emitter.instruction("mov x12, #0");                             // initialize loop counter to 0

                emitter.label(&loop_label);
                // -- compare each element against needle --
                emitter.instruction("cmp x12, x9");                             // check if counter reached array length
                emitter.instruction(&format!("b.ge {}", end_label));            // exit loop if all elements checked
                emitter.instruction("ldr x13, [x10, x12, lsl #3]");             // load element at index x12 (offset = x12 * 8)
                emitter.instruction("cmp x13, x11");                            // compare element with needle
                emitter.instruction(&format!("b.eq {}", found_label));          // branch to found if element matches
                emitter.instruction("add x12, x12, #1");                        // increment loop counter
                emitter.instruction(&format!("b {}", loop_label));              // jump back to loop start

                // -- needle found --
                emitter.label(&found_label);
                emitter.instruction("mov x0, #1");                              // set return value to 1 (true)
                emitter.instruction(&format!("b {}", done_label));              // jump to done

                // -- needle not found --
                emitter.label(&end_label);
                emitter.instruction("mov x0, #0");                              // set return value to 0 (false)
                emitter.label(&done_label);
            }
        }
    }

    Some(PhpType::Int)
}
