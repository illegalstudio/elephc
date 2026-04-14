use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
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
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the associative-array hash-table pointer while evaluating the searched needle

        let needle_ty = emit_expr(&args[0], emitter, ctx, data);

        let found_label = ctx.next_label("in_array_assoc_found");
        let end_label = ctx.next_label("in_array_assoc_end");
        let loop_label = ctx.next_label("in_array_assoc_loop");
        let skip_label = ctx.next_label("in_array_assoc_skip");
        let mixed_mismatch_label = ctx.next_label("in_array_assoc_mixed_mismatch");

        match &val_ty {
            PhpType::Str => {
                // -- needle is a string in x1/x2, save it --
                abi::emit_push_reg_pair(emitter, abi::string_result_regs(emitter).0, abi::string_result_regs(emitter).1); // preserve the string needle across associative-array iteration
            }
            PhpType::Mixed if matches!(needle_ty, PhpType::Str) => {
                abi::emit_push_reg_pair(emitter, abi::string_result_regs(emitter).0, abi::string_result_regs(emitter).1); // preserve the string needle across mixed associative-array iteration
            }
            PhpType::Mixed if matches!(needle_ty, PhpType::Float) => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("fmov x0, d0");                     // move the float needle bits into an integer register for mixed-entry comparison
                    }
                    Arch::X86_64 => {
                        emitter.instruction("movq rax, xmm0");                  // move the float needle bits into the integer result register for mixed-entry comparison
                    }
                }
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // preserve the float needle bits across associative-array iteration
            }
            _ => {
                // -- needle is an integer/bool in x0, save it --
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // preserve the scalar needle across associative-array iteration
            }
        }

        // -- push iteration index onto stack --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("str xzr, [sp, #-16]!");                    // push iter_cursor = 0 (start from the associative-array header head slot)
            }
            Arch::X86_64 => {
                emitter.instruction("sub rsp, 16");                             // reserve one temporary stack slot for the associative-array iterator cursor
                emitter.instruction("mov QWORD PTR [rsp], 0");                  // initialize the associative-array iterator cursor to the hash-header head sentinel
            }
        }

        // Stack layout (top to bottom):
        //   sp+0:  iter_index (16 bytes)
        //   sp+16: needle (16 bytes)
        //   sp+32: hash_table_ptr (16 bytes)

        emitter.label(&loop_label);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [sp, #32]");                       // load the associative-array hash-table pointer for the next insertion-order iteration step
                emitter.instruction("ldr x1, [sp]");                            // load the current associative-array iterator cursor
                emitter.instruction("bl __rt_hash_iter_next");                  // advance one associative-array insertion-order entry and return its key plus payload
                emitter.instruction("cmn x0, #1");                              // has associative-array iteration reached the done sentinel?
                emitter.instruction(&format!("b.eq {}", end_label));            // stop searching once the associative-array iterator is exhausted
                emitter.instruction("str x0, [sp]");                            // save the updated associative-array iterator cursor for the next loop step

                match &val_ty {
                    PhpType::Str => {
                        emitter.instruction("mov x1, x3");                      // move the associative-array entry string pointer into the first string-compare register
                        emitter.instruction("mov x2, x4");                      // move the associative-array entry string length into the paired string-compare register
                        emitter.instruction("ldp x3, x4, [sp, #16]");          // reload the saved string needle from the associative-array search stack frame
                        emitter.instruction("bl __rt_str_eq");                  // compare the associative-array entry string value against the searched needle
                        emitter.instruction(&format!("cbnz x0, {}", found_label)); // stop once the associative-array string value matches the searched needle
                    }
                    PhpType::Mixed => {
                        let expected_tag = crate::codegen::runtime_value_tag(&needle_ty);
                        emitter.instruction(&format!("mov x6, #{}", expected_tag)); // materialize the expected mixed-entry runtime tag for the searched needle
                        emitter.instruction("cmp x5, x6");                      // does the current associative-array mixed entry match the searched needle kind?
                        emitter.instruction(&format!("b.ne {}", mixed_mismatch_label)); // skip associative-array entries whose mixed kind differs from the needle
                        match &needle_ty {
                            PhpType::Str => {
                                emitter.instruction("mov x1, x3");              // move the associative-array mixed entry string pointer into the first string-compare register
                                emitter.instruction("mov x2, x4");              // move the associative-array mixed entry string length into the paired string-compare register
                                emitter.instruction("ldp x3, x4, [sp, #16]");  // reload the saved string needle from the associative-array search stack frame
                                emitter.instruction("bl __rt_str_eq");          // compare the associative-array mixed string entry against the searched needle
                                emitter.instruction(&format!("cbnz x0, {}", found_label)); // stop once the associative-array mixed string value matches the needle
                            }
                            PhpType::Void => {
                                emitter.instruction(&format!("b {}", found_label)); // null needles match associative-array entries tagged null
                            }
                            _ => {
                                emitter.instruction("ldr x6, [sp, #16]");       // reload the saved scalar mixed needle payload from the associative-array search stack frame
                                emitter.instruction("cmp x3, x6");              // compare the associative-array mixed entry payload against the searched scalar needle
                                emitter.instruction(&format!("b.eq {}", found_label)); // stop once the associative-array mixed scalar payload matches the needle
                            }
                        }
                        emitter.label(&mixed_mismatch_label);
                    }
                    _ => {
                        emitter.instruction("ldr x5, [sp, #16]");               // reload the saved scalar needle payload from the associative-array search stack frame
                        emitter.instruction("cmp x3, x5");                      // compare the associative-array entry payload against the searched scalar needle
                        emitter.instruction(&format!("b.eq {}", found_label));  // stop once the associative-array entry payload matches the searched scalar needle
                    }
                }
                emitter.instruction(&format!("b {}", loop_label));              // continue scanning the remaining associative-array insertion-order entries

                emitter.label(&found_label);
                emitter.instruction("mov x0, #1");                              // return true once the searched needle matches an associative-array entry value
                emitter.instruction(&format!("b {}", skip_label));              // jump to the common associative-array cleanup after a match

                emitter.label(&end_label);
                emitter.instruction("mov x0, #0");                              // return false once associative-array iteration finishes without a match

                emitter.label(&skip_label);
                emitter.instruction("add sp, sp, #48");                         // drop the associative-array iterator cursor, needle, and hash-table stack slots
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");           // load the associative-array hash-table pointer for the next insertion-order iteration step
                emitter.instruction("mov rsi, QWORD PTR [rsp]");                // load the current associative-array iterator cursor
                emitter.instruction("call __rt_hash_iter_next");                // advance one associative-array insertion-order entry and return its key plus payload
                emitter.instruction("cmp rax, -1");                             // has associative-array iteration reached the done sentinel?
                emitter.instruction(&format!("je {}", end_label));              // stop searching once the associative-array iterator is exhausted
                emitter.instruction("mov QWORD PTR [rsp], rax");                // save the updated associative-array iterator cursor for the next loop step

                match &val_ty {
                    PhpType::Str => {
                        emitter.instruction("mov rdi, rcx");                    // move the associative-array entry string pointer into the first string-compare register
                        emitter.instruction("mov rsi, r8");                     // move the associative-array entry string length into the paired string-compare register
                        emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");   // reload the saved string needle pointer from the associative-array search stack frame
                        emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");   // reload the saved string needle length from the associative-array search stack frame
                        emitter.instruction("call __rt_str_eq");                // compare the associative-array entry string value against the searched needle
                        emitter.instruction("test rax, rax");                   // did the associative-array string value match the searched needle?
                        emitter.instruction(&format!("jne {}", found_label));   // stop once the associative-array string value matches the searched needle
                    }
                    PhpType::Mixed => {
                        let expected_tag = crate::codegen::runtime_value_tag(&needle_ty) as i64;
                        abi::emit_load_int_immediate(emitter, "r10", expected_tag); // materialize the expected mixed-entry runtime tag for the searched needle
                        emitter.instruction("cmp r9, r10");                     // does the current associative-array mixed entry match the searched needle kind?
                        emitter.instruction(&format!("jne {}", mixed_mismatch_label)); // skip associative-array entries whose mixed kind differs from the needle
                        match &needle_ty {
                            PhpType::Str => {
                                emitter.instruction("mov rdi, rcx");            // move the associative-array mixed entry string pointer into the first string-compare register
                                emitter.instruction("mov rsi, r8");             // move the associative-array mixed entry string length into the paired string-compare register
                                emitter.instruction("mov rdx, QWORD PTR [rsp + 16]"); // reload the saved string needle pointer from the associative-array search stack frame
                                emitter.instruction("mov rcx, QWORD PTR [rsp + 24]"); // reload the saved string needle length from the associative-array search stack frame
                                emitter.instruction("call __rt_str_eq");        // compare the associative-array mixed string entry against the searched needle
                                emitter.instruction("test rax, rax");           // did the associative-array mixed string value match the searched needle?
                                emitter.instruction(&format!("jne {}", found_label)); // stop once the associative-array mixed string value matches the needle
                            }
                            PhpType::Void => {
                                emitter.instruction(&format!("jmp {}", found_label)); // null needles match associative-array entries tagged null
                            }
                            _ => {
                                emitter.instruction("mov r10, QWORD PTR [rsp + 16]"); // reload the saved scalar mixed needle payload from the associative-array search stack frame
                                emitter.instruction("cmp rcx, r10");            // compare the associative-array mixed entry payload against the searched scalar needle
                                emitter.instruction(&format!("je {}", found_label)); // stop once the associative-array mixed scalar payload matches the needle
                            }
                        }
                        emitter.label(&mixed_mismatch_label);
                    }
                    _ => {
                        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");   // reload the saved scalar needle payload from the associative-array search stack frame
                        emitter.instruction("cmp rcx, r10");                    // compare the associative-array entry payload against the searched scalar needle
                        emitter.instruction(&format!("je {}", found_label));    // stop once the associative-array entry payload matches the searched scalar needle
                    }
                }
                emitter.instruction(&format!("jmp {}", loop_label));            // continue scanning the remaining associative-array insertion-order entries

                emitter.label(&found_label);
                emitter.instruction("mov rax, 1");                              // return true once the searched needle matches an associative-array entry value
                emitter.instruction(&format!("jmp {}", skip_label));            // jump to the common associative-array cleanup after a match

                emitter.label(&end_label);
                emitter.instruction("xor eax, eax");                            // return false once associative-array iteration finishes without a match

                emitter.label(&skip_label);
                emitter.instruction("add rsp, 48");                             // drop the associative-array iterator cursor, needle, and hash-table stack slots
            }
        }
    } else {
        // -- indexed array: linear scan --
        let elem_ty = match &arr_ty {
            PhpType::Array(t) => *t.clone(),
            _ => PhpType::Int,
        };

        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the indexed-array pointer while evaluating the searched needle
        let _needle_ty = emit_expr(&args[0], emitter, ctx, data);

        let found_label = ctx.next_label("in_array_found");
        let end_label = ctx.next_label("in_array_end");
        let done_label = ctx.next_label("in_array_done");
        let loop_label = ctx.next_label("in_array_loop");

        match &elem_ty {
            PhpType::Str => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        // -- save needle string (x1=ptr, x2=len) and set up loop --
                        emitter.instruction("stp x1, x2, [sp, #-16]!");         // push needle ptr+len
                        emitter.instruction("ldr x0, [sp, #16]");               // reload array pointer
                        emitter.instruction("ldr x9, [x0]");                    // load array length
                        emitter.instruction("add x10, x0, #24");                // x10 = pointer to data region
                        emitter.instruction("mov x12, #0");                     // initialize loop counter

                        // Stack layout:
                        //   sp+0:  needle ptr+len (16 bytes)
                        //   sp+16: array pointer (16 bytes)

                        emitter.label(&loop_label);
                        // -- check if all elements have been scanned --
                        emitter.instruction("cmp x12, x9");                     // check if counter reached array length
                        emitter.instruction(&format!("b.ge {}", end_label));    // exit loop if all elements checked

                        // -- load string element at index x12 (16 bytes per element) --
                        emitter.instruction("lsl x13, x12, #4");                // x13 = index * 16
                        emitter.instruction("ldr x1, [x10, x13]");              // x1 = element string pointer
                        emitter.instruction("add x14, x13, #8");                // x14 = offset to length field
                        emitter.instruction("ldr x2, [x10, x14]");              // x2 = element string length

                        // -- save loop state before calling __rt_str_eq --
                        emitter.instruction("stp x9, x10, [sp, #-16]!");        // push array len + data ptr
                        emitter.instruction("str x12, [sp, #-16]!");            // push loop counter

                        // -- load needle and compare --
                        emitter.instruction("ldp x3, x4, [sp, #32]");           // reload needle ptr+len from stack
                        emitter.instruction("bl __rt_str_eq");                  // x0 = 1 if strings are equal

                        // -- restore loop state --
                        emitter.instruction("ldr x12, [sp], #16");              // pop loop counter
                        emitter.instruction("ldp x9, x10, [sp], #16");          // pop array len + data ptr

                        emitter.instruction(&format!("cbnz x0, {}", found_label)); // if equal, found
                        emitter.instruction("add x12, x12, #1");                // increment loop counter
                        emitter.instruction(&format!("b {}", loop_label));      // continue searching

                        // -- needle found --
                        emitter.label(&found_label);
                        emitter.instruction("mov x0, #1");                      // return true
                        emitter.instruction(&format!("b {}", done_label));      // jump to cleanup

                        // -- needle not found --
                        emitter.label(&end_label);
                        emitter.instruction("mov x0, #0");                      // return false

                        emitter.label(&done_label);
                        emitter.instruction("add sp, sp, #32");                 // drop needle + array ptr
                    }
                    Arch::X86_64 => {
                        // -- save needle string (rax=ptr, rdx=len) and set up loop --
                        abi::emit_push_reg_pair(emitter, "rax", "rdx");         // preserve the searched string across the indexed-array scan
                        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");   // reload the indexed-array pointer from the temporary stack frame
                        emitter.instruction("mov r11, QWORD PTR [r10]");        // load the indexed-array length from the fixed header
                        emitter.instruction("lea r12, [r10 + 24]");             // compute the base address of the indexed-array string payload region
                        emitter.instruction("xor r13d, r13d");                  // initialize the indexed-array loop counter to zero

                        // Stack layout:
                        //   rsp+0:  needle ptr+len (16 bytes)
                        //   rsp+16: array pointer (16 bytes)

                        emitter.label(&loop_label);
                        emitter.instruction("cmp r13, r11");                    // have we scanned every indexed-array string element?
                        emitter.instruction(&format!("jge {}", end_label));     // stop once the loop counter reaches the indexed-array length

                        emitter.instruction("mov rcx, r13");                    // copy the loop index before scaling it to one 16-byte string slot
                        emitter.instruction("shl rcx, 4");                      // convert the indexed-array element index into a byte offset inside the string payload region
                        emitter.instruction("mov rdi, QWORD PTR [r12 + rcx]");  // load the current indexed-array string pointer into the first str_eq argument register
                        emitter.instruction("mov rsi, QWORD PTR [r12 + rcx + 8]"); // load the current indexed-array string length into the paired str_eq argument register

                        abi::emit_push_reg_pair(emitter, "r11", "r12");        // preserve the indexed-array length and payload base across the string-compare helper call
                        abi::emit_push_reg(emitter, "r13");                     // preserve the indexed-array loop counter across the string-compare helper call

                        emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");   // reload the searched string pointer from the temporary stack frame under the saved loop state
                        emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");   // reload the searched string length from the temporary stack frame under the saved loop state
                        emitter.instruction("call __rt_str_eq");                // compare the current indexed-array string element against the searched needle

                        abi::emit_pop_reg(emitter, "r13");                      // restore the indexed-array loop counter after the helper call
                        abi::emit_pop_reg_pair(emitter, "r11", "r12");          // restore the indexed-array length and payload base after the helper call

                        emitter.instruction("test rax, rax");                   // did the current indexed-array string element match the searched needle?
                        emitter.instruction(&format!("jne {}", found_label));   // return true as soon as one indexed-array string element matches
                        emitter.instruction("add r13, 1");                      // advance to the next indexed-array string element after a mismatch
                        emitter.instruction(&format!("jmp {}", loop_label));    // continue scanning the indexed-array string payloads

                        emitter.label(&found_label);
                        emitter.instruction("mov rax, 1");                      // return true once the searched string matches an indexed-array element
                        emitter.instruction(&format!("jmp {}", done_label));    // skip the not-found write and jump to the common indexed-array cleanup

                        emitter.label(&end_label);
                        emitter.instruction("xor eax, eax");                    // return false once the indexed-array scan finishes without a string match

                        emitter.label(&done_label);
                        emitter.instruction("add rsp, 32");                     // drop the saved string needle and indexed-array pointer from the temporary stack frame
                    }
                }
            }
            _ => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        // -- integer/bool needle: simple comparison loop --
                        emitter.instruction("mov x11, x0");                     // save needle value in x11
                        emitter.instruction("ldr x0, [sp], #16");               // pop array pointer
                        emitter.instruction("ldr x9, [x0]");                    // load array length into x9
                        emitter.instruction("add x10, x0, #24");                // x10 = pointer to data (past 24-byte header)
                        emitter.instruction("mov x12, #0");                     // initialize loop counter to 0

                        emitter.label(&loop_label);
                        emitter.instruction("cmp x12, x9");                     // check if counter reached array length
                        emitter.instruction(&format!("b.ge {}", end_label));    // exit loop if all elements checked
                        emitter.instruction("ldr x13, [x10, x12, lsl #3]");     // load element at index x12 (offset = x12 * 8)
                        emitter.instruction("cmp x13, x11");                    // compare element with needle
                        emitter.instruction(&format!("b.eq {}", found_label));  // branch to found if element matches
                        emitter.instruction("add x12, x12, #1");                // increment loop counter
                        emitter.instruction(&format!("b {}", loop_label));      // jump back to loop start

                        emitter.label(&found_label);
                        emitter.instruction("mov x0, #1");                      // set return value to 1 (true)
                        emitter.instruction(&format!("b {}", done_label));      // jump to done

                        emitter.label(&end_label);
                        emitter.instruction("mov x0, #0");                      // set return value to 0 (false)
                        emitter.label(&done_label);
                    }
                    Arch::X86_64 => {
                        emitter.instruction("mov rsi, rax");                    // move the searched scalar needle into the second SysV runtime-helper argument register
                        abi::emit_pop_reg(emitter, "rdi");                      // restore the indexed-array pointer into the first SysV runtime-helper argument register
                        abi::emit_call_label(emitter, "__rt_array_search");     // search the indexed-array payloads for the first matching scalar element
                        emitter.instruction("cmp rax, -1");                     // did the indexed-array search helper fail to find any matching scalar element?
                        emitter.instruction("setne al");                        // convert the found-vs-missing condition into a one-byte boolean result
                        emitter.instruction("movzx rax, al");                   // zero-extend the boolean result into the standard integer result register
                    }
                }
            }
        }
    }

    Some(PhpType::Int)
}
