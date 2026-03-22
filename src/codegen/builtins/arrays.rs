use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "count" => {
            emitter.comment("count()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- read element count from array header --
            emitter.instruction("ldr x0, [x0]");                                // load array length from first field of array struct

            Some(PhpType::Int)
        }
        // @todo: add support for array_push() with floats, booleans and other types
        "array_push" => {
            emitter.comment("array_push()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save array pointer, evaluate value to push --
            emitter.instruction("str x0, [sp, #-16]!");                         // push array pointer onto stack
            let val_ty = emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x9, [sp], #16");                           // pop saved array pointer into x9
            match &val_ty {
                PhpType::Int => {
                    // -- push integer value onto array --
                    emitter.instruction("mov x1, x0");                          // move integer value to x1 (second arg)
                    emitter.instruction("mov x0, x9");                          // move array pointer to x0 (first arg)
                    emitter.instruction("bl __rt_array_push_int");              // call runtime: append integer to array
                }
                PhpType::Str => {
                    // -- push string value onto array --
                    emitter.instruction("mov x0, x9");                          // move array pointer to x0 (first arg, x1/x2 already set)
                    emitter.instruction("bl __rt_array_push_str");              // call runtime: append string to array
                }
                _ => {}
            }

            Some(PhpType::Void)
        }
        "array_pop" => {
            emitter.comment("array_pop()");
            let arr_ty = emit_expr(&args[0], emitter, ctx, data);
            // -- decrement array length to remove last element --
            emitter.instruction("ldr x9, [x0]");                                // load current array length into x9
            emitter.instruction("sub x9, x9, #1");                              // decrement length by 1
            emitter.instruction("str x9, [x0]");                                // store decremented length back to array header
            let elem_ty = match &arr_ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            match &elem_ty {
                PhpType::Int => {
                    // -- load the popped integer element --
                    emitter.instruction("add x0, x0, #24");                     // advance past array header (24 bytes) to data area
                    emitter.instruction("ldr x0, [x0, x9, lsl #3]");            // load int at index x9 (offset = x9 * 8 bytes)
                }
                PhpType::Str => {
                    // -- load the popped string element (ptr + len) --
                    emitter.instruction("lsl x10, x9, #4");                     // multiply index by 16 (each string entry = 16 bytes)
                    emitter.instruction("add x0, x0, x10");                     // advance pointer by element offset
                    emitter.instruction("add x0, x0, #24");                     // skip past array header to data area
                    emitter.instruction("ldr x1, [x0]");                        // load string pointer from element
                    emitter.instruction("ldr x2, [x0, #8]");                    // load string length from element + 8
                }
                _ => {}
            }

            Some(elem_ty)
        }
        "in_array" => {
            emitter.comment("in_array()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- save needle, evaluate array --
            emitter.instruction("str x0, [sp, #-16]!");                         // push needle value onto stack
            emit_expr(&args[1], emitter, ctx, data);
            let found_label = ctx.next_label("in_array_found");
            let end_label = ctx.next_label("in_array_end");
            let done_label = ctx.next_label("in_array_done");
            // -- set up loop to search array for needle --
            emitter.instruction("ldr x9, [x0]");                                // load array length into x9
            emitter.instruction("add x10, x0, #24");                            // x10 = pointer to array data (past 24-byte header)
            emitter.instruction("ldr x11, [sp], #16");                          // pop needle value into x11
            emitter.instruction("mov x12, #0");                                 // initialize loop counter to 0
            let loop_label = ctx.next_label("in_array_loop");
            emitter.label(&loop_label);
            // -- compare each element against needle --
            emitter.instruction("cmp x12, x9");                                 // check if counter reached array length
            emitter.instruction(&format!("b.ge {}", end_label));                // exit loop if all elements checked
            emitter.instruction("ldr x13, [x10, x12, lsl #3]");                 // load element at index x12 (offset = x12 * 8)
            emitter.instruction("cmp x13, x11");                                // compare element with needle
            emitter.instruction(&format!("b.eq {}", found_label));              // branch to found if element matches
            emitter.instruction("add x12, x12, #1");                            // increment loop counter
            emitter.instruction(&format!("b {}", loop_label));                  // jump back to loop start
            // -- needle found --
            emitter.label(&found_label);
            emitter.instruction("mov x0, #1");                                  // set return value to 1 (true)
            emitter.instruction(&format!("b {}", done_label));                  // jump to done
            // -- needle not found --
            emitter.label(&end_label);
            emitter.instruction("mov x0, #0");                                  // set return value to 0 (false)
            emitter.label(&done_label);

            Some(PhpType::Int)
        }
        "array_keys" => {
            emitter.comment("array_keys()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- read source array length and allocate result array --
            emitter.instruction("ldr x9, [x0]");                                // load source array length into x9
            emitter.instruction("str x9, [sp, #-16]!");                         // push array length onto stack (for loop bound)
            emitter.instruction("mov x0, x9");                                  // pass length as capacity for new array
            emitter.instruction("mov x1, #8");                                  // element size = 8 bytes (integer keys)
            emitter.instruction("bl __rt_array_new");                           // call runtime: allocate new array
            emitter.instruction("str x0, [sp, #-16]!");                         // push new array pointer onto stack
            emitter.instruction("str xzr, [sp, #-16]!");                        // push loop counter (0) onto stack
            let loop_label = ctx.next_label("akeys_loop");
            let end_label = ctx.next_label("akeys_end");
            emitter.label(&loop_label);
            // -- loop: push each index as a key into result array --
            emitter.instruction("ldr x12, [sp]");                               // load current loop counter from stack
            emitter.instruction("ldr x9, [sp, #32]");                           // load array length from stack (2 slots above)
            emitter.instruction("cmp x12, x9");                                 // compare counter with array length
            emitter.instruction(&format!("b.ge {}", end_label));                // exit loop if counter >= length
            emitter.instruction("ldr x0, [sp, #16]");                           // load result array pointer from stack
            emitter.instruction("mov x1, x12");                                 // pass current index as value to push
            emitter.instruction("bl __rt_array_push_int");                      // call runtime: push index into result array
            emitter.instruction("ldr x12, [sp]");                               // reload loop counter from stack
            emitter.instruction("add x12, x12, #1");                            // increment loop counter
            emitter.instruction("str x12, [sp]");                               // store updated counter back to stack
            emitter.instruction(&format!("b {}", loop_label));                  // jump back to loop start
            emitter.label(&end_label);
            // -- clean up stack and return result array --
            emitter.instruction("add sp, sp, #16");                             // drop loop counter from stack
            emitter.instruction("ldr x0, [sp], #16");                           // pop result array pointer into x0
            emitter.instruction("add sp, sp, #16");                             // drop saved array length from stack

            Some(PhpType::Array(Box::new(PhpType::Int)))
        }
        "array_values" => {
            emitter.comment("array_values()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- array_values on a sequential array is a no-op, return same array --

            Some(PhpType::Array(Box::new(PhpType::Int)))
        }
        "sort" => {
            emitter.comment("sort()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- sort integer array in ascending order --
            emitter.instruction("bl __rt_sort_int");                            // call runtime: sort array of integers ascending

            Some(PhpType::Void)
        }
        "rsort" => {
            emitter.comment("rsort()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- sort integer array in descending order --
            emitter.instruction("bl __rt_rsort_int");                           // call runtime: sort array of integers descending

            Some(PhpType::Void)
        }
        "isset" => {
            emitter.comment("isset()");
            emit_expr(&args[0], emitter, ctx, data);
            // -- compiled variables always exist, so isset returns true --
            emitter.instruction("mov x0, #1");                                  // return 1 (true) since variable is always set

            Some(PhpType::Int)
        }
        _ => None,
    }
}
