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
    emitter.comment("array_search()");

    // -- evaluate array (second arg) first to get its type --
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);

    if let PhpType::AssocArray { value, .. } = &arr_ty {
        let val_ty = *value.clone();
        // -- save hash table pointer, evaluate needle --
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the associative-array hash-table pointer while evaluating the searched needle

        let needle_ty = emit_expr(&args[0], emitter, ctx, data);

        let found_label = ctx.next_label("asearch_assoc_found");
        let end_label = ctx.next_label("asearch_assoc_end");
        let loop_label = ctx.next_label("asearch_assoc_loop");
        let skip_label = ctx.next_label("asearch_assoc_skip");
        let mixed_mismatch_label = ctx.next_label("asearch_assoc_mixed_mismatch");

        match &val_ty {
            PhpType::Str => {
                // -- needle is a string in x1/x2, save it --
                abi::emit_push_reg_pair(emitter, abi::string_result_regs(emitter).0, abi::string_result_regs(emitter).1); // preserve the string needle across the associative-array iteration loop
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
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // preserve the float needle bits across the associative-array iteration loop
            }
            _ => {
                // -- needle is an integer/bool in x0, save it --
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // preserve the scalar needle across the associative-array iteration loop
            }
        }

        // -- push iteration index onto stack --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("str xzr, [sp, #-16]!");                    // push iter_cursor = 0 (start from hash header head)
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
                emitter.instruction("ldr x0, [sp, #32]");                       // load the associative-array hash table pointer for the next insertion-order iteration step
                emitter.instruction("ldr x1, [sp]");                            // load the current associative-array iterator cursor
                emitter.instruction("bl __rt_hash_iter_next");                  // advance one associative-array insertion-order entry and return its key plus payload
                emitter.instruction("cmn x0, #1");                              // has the associative-array iterator reached the done sentinel?
                emitter.instruction(&format!("b.eq {}", end_label));            // stop searching once associative-array iteration has completed
                emitter.instruction("str x0, [sp]");                            // save the updated associative-array iterator cursor for the next loop iteration
                abi::emit_push_reg_pair(emitter, "x1", "x2");                       // preserve the current associative-array key so it can be returned on match

                match &val_ty {
                    PhpType::Str => {
                        emitter.instruction("mov x1, x3");                      // move the associative-array entry string pointer into the first string-compare register
                        emitter.instruction("mov x2, x4");                      // move the associative-array entry string length into the paired string-compare register
                        emitter.instruction("ldp x3, x4, [sp, #32]");           // reload the saved string needle from the stack frame under the preserved key pair
                        emitter.instruction("bl __rt_str_eq");                  // compare the associative-array entry string value against the searched needle
                        emitter.instruction(&format!("cbnz x0, {}", found_label)); // stop once the searched string matches the current associative-array value
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
                                emitter.instruction("ldp x3, x4, [sp, #32]");   // reload the saved string needle under the preserved associative-array key pair
                                emitter.instruction("bl __rt_str_eq");          // compare the associative-array mixed string entry against the searched needle
                                emitter.instruction(&format!("cbnz x0, {}", found_label)); // stop once the associative-array mixed string value matches the needle
                            }
                            PhpType::Void => {
                                emitter.instruction(&format!("b {}", found_label)); // null needles match associative-array entries tagged null
                            }
                            _ => {
                                emitter.instruction("ldr x6, [sp, #32]");       // reload the saved scalar mixed needle payload under the preserved associative-array key pair
                                emitter.instruction("cmp x3, x6");              // compare the associative-array mixed entry payload against the searched scalar needle
                                emitter.instruction(&format!("b.eq {}", found_label)); // stop once the associative-array mixed scalar payload matches the needle
                            }
                        }
                        emitter.label(&mixed_mismatch_label);
                    }
                    _ => {
                        emitter.instruction("ldr x5, [sp, #32]");               // reload the saved scalar needle payload under the preserved associative-array key pair
                        emitter.instruction("cmp x3, x5");                      // compare the associative-array entry payload against the searched scalar needle
                        emitter.instruction(&format!("b.eq {}", found_label));  // stop once the associative-array entry payload matches the searched scalar needle
                    }
                }
                emitter.instruction("add sp, sp, #16");                         // drop the preserved associative-array key after a non-matching iteration step
                emitter.instruction(&format!("b {}", loop_label));              // continue scanning the remaining associative-array insertion-order entries

                emitter.label(&found_label);
                abi::emit_pop_reg_pair(emitter, "x1", "x2");                        // return the matching associative-array key as the searched string result
                emitter.instruction(&format!("b {}", skip_label));              // jump to the common associative-array cleanup once a match is found

                emitter.label(&end_label);
                emitter.instruction("mov x1, #0");                              // return an empty string pointer when associative-array search does not find the needle
                emitter.instruction("mov x2, #0");                              // return an empty string length when associative-array search does not find the needle
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");           // load the associative-array hash table pointer for the next insertion-order iteration step
                emitter.instruction("mov rsi, QWORD PTR [rsp]");                // load the current associative-array iterator cursor
                emitter.instruction("call __rt_hash_iter_next");                // advance one associative-array insertion-order entry and return its key plus payload
                emitter.instruction("cmp rax, -1");                             // has the associative-array iterator reached the done sentinel?
                emitter.instruction(&format!("je {}", end_label));              // stop searching once associative-array iteration has completed
                emitter.instruction("mov QWORD PTR [rsp], rax");                // save the updated associative-array iterator cursor for the next loop iteration
                abi::emit_push_reg_pair(emitter, "rdi", "rdx");                     // preserve the current associative-array key so it can be returned on match

                match &val_ty {
                    PhpType::Str => {
                        emitter.instruction("mov rdi, rcx");                    // move the associative-array entry string pointer into the first string-compare register
                        emitter.instruction("mov rsi, r8");                     // move the associative-array entry string length into the paired string-compare register
                        emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");   // reload the saved string needle pointer under the preserved associative-array key pair
                        emitter.instruction("mov rcx, QWORD PTR [rsp + 40]");   // reload the saved string needle length under the preserved associative-array key pair
                        emitter.instruction("call __rt_str_eq");                // compare the associative-array entry string value against the searched needle
                        emitter.instruction("test rax, rax");                   // did the associative-array string value match the searched needle?
                        emitter.instruction(&format!("jne {}", found_label));   // stop once the searched string matches the current associative-array value
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
                                emitter.instruction("mov rdx, QWORD PTR [rsp + 32]"); // reload the saved string needle pointer under the preserved associative-array key pair
                                emitter.instruction("mov rcx, QWORD PTR [rsp + 40]"); // reload the saved string needle length under the preserved associative-array key pair
                                emitter.instruction("call __rt_str_eq");        // compare the associative-array mixed string entry against the searched needle
                                emitter.instruction("test rax, rax");           // did the associative-array mixed string entry match the searched needle?
                                emitter.instruction(&format!("jne {}", found_label)); // stop once the associative-array mixed string value matches the needle
                            }
                            PhpType::Void => {
                                emitter.instruction(&format!("jmp {}", found_label)); // null needles match associative-array entries tagged null
                            }
                            _ => {
                                emitter.instruction("mov r10, QWORD PTR [rsp + 32]"); // reload the saved scalar mixed needle payload under the preserved associative-array key pair
                                emitter.instruction("cmp rcx, r10");            // compare the associative-array mixed entry payload against the searched scalar needle
                                emitter.instruction(&format!("je {}", found_label)); // stop once the associative-array mixed scalar payload matches the needle
                            }
                        }
                        emitter.label(&mixed_mismatch_label);
                    }
                    _ => {
                        emitter.instruction("mov r10, QWORD PTR [rsp + 32]");   // reload the saved scalar needle payload under the preserved associative-array key pair
                        emitter.instruction("cmp rcx, r10");                    // compare the associative-array entry payload against the searched scalar needle
                        emitter.instruction(&format!("je {}", found_label));    // stop once the associative-array entry payload matches the searched scalar needle
                    }
                }
                emitter.instruction("add rsp, 16");                             // drop the preserved associative-array key after a non-matching iteration step
                emitter.instruction(&format!("jmp {}", loop_label));            // continue scanning the remaining associative-array insertion-order entries

                emitter.label(&found_label);
                abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // return the matching associative-array key through the standard x86_64 string result registers
                emitter.instruction(&format!("jmp {}", skip_label));            // jump to the common associative-array cleanup once a match is found

                emitter.label(&end_label);
                emitter.instruction("xor eax, eax");                            // return an empty string pointer when associative-array search does not find the needle
                emitter.instruction("xor edx, edx");                            // return an empty string length when associative-array search does not find the needle
            }
        }

        emitter.label(&skip_label);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("add sp, sp, #48");                         // drop the remaining associative-array iterator cursor, needle, and hash-table stack slots
            }
            Arch::X86_64 => {
                emitter.instruction("add rsp, 48");                             // drop the remaining associative-array iterator cursor, needle, and hash-table stack slots
            }
        }

        // For assoc arrays, array_search returns a string key
        return Some(PhpType::Str);
    }

    // -- indexed array: use runtime for linear search --
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the indexed-array pointer while evaluating the searched needle
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // move the indexed-array needle into the second helper argument register
            abi::emit_pop_reg(emitter, "x0");                                   // restore the indexed-array pointer into the first helper argument register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // move the indexed-array needle into the second SysV helper argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the indexed-array pointer into the first SysV helper argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_array_search");                         // search the indexed-array values and return the first matching index or -1

    Some(PhpType::Int)
}
