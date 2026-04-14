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
    emitter.comment("array_values()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);

    if let PhpType::AssocArray { value, .. } = &arr_ty {
        let val_ty = *value.clone();
        // -- associative array: iterate hash table and collect values --
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the associative-array hash-table pointer while allocating the result values array

        // -- allocate new indexed array for values --
        let elem_size = match &val_ty {
            PhpType::Str => 16,
            _ => 8,
        };
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [x0]");                            // load the associative-array entry count to size the result values array exactly
                emitter.instruction(&format!("mov x1, #{}", elem_size));        // choose the indexed-array element size that matches the associative-array value representation
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, QWORD PTR [rax]");                // load the associative-array entry count to size the result values array exactly
                emitter.instruction(&format!("mov rsi, {}", elem_size));        // choose the indexed-array element size that matches the associative-array value representation
            }
        }
        abi::emit_call_label(emitter, "__rt_array_new");                        // allocate the result values array with exact associative-array capacity
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the result values array pointer across associative-array iteration

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

        // Stack: [iter_index(16)] [result_array(16)] [hash_ptr(16)]

        let loop_label = ctx.next_label("avals_assoc_loop");
        let end_label = ctx.next_label("avals_assoc_end");
        emitter.label(&loop_label);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [sp, #32]");                       // load the associative-array hash-table pointer for the next insertion-order iteration step
                emitter.instruction("ldr x1, [sp]");                            // load the current associative-array iterator cursor
                emitter.instruction("bl __rt_hash_iter_next");                  // advance one associative-array insertion-order entry and return its key plus payload
                emitter.instruction("cmn x0, #1");                              // has associative-array iteration reached the done sentinel?
                emitter.instruction(&format!("b.eq {}", end_label));            // stop once every associative-array value has been collected
                emitter.instruction("str x0, [sp]");                            // save the updated associative-array iterator cursor for the next loop step

                match &val_ty {
                    PhpType::Str => {
                        emitter.instruction("mov x1, x3");                      // move the associative-array string value pointer into the string-persist input register
                        emitter.instruction("mov x2, x4");                      // move the associative-array string value length into the paired string-persist input register
                        emitter.instruction("bl __rt_str_persist");             // persist the associative-array string value so the result array owns stable string storage
                        emitter.instruction("ldr x9, [sp, #16]");               // load the result values array pointer from the fixed stack layout
                        emitter.instruction("ldr x10, [x9]");                   // load the current result values array length before appending one more value
                        emitter.instruction("lsl x11, x10, #4");                // convert the result values array length into a 16-byte string-slot offset
                        emitter.instruction("add x11, x9, x11");                // advance from the result values array header to the selected string slot
                        emitter.instruction("add x11, x11, #24");               // skip the fixed indexed-array header to land on the string payload region
                        emitter.instruction("str x1, [x11]");                   // store the owned string pointer into the next result values slot
                        emitter.instruction("str x2, [x11, #8]");               // store the owned string length into the next result values slot
                        emitter.instruction("add x10, x10, #1");                // increment the result values array length after storing one more string
                        emitter.instruction("str x10, [x9]");                   // persist the updated result values array length in the header
                    }
                    PhpType::Mixed => {
                        let reuse_box = ctx.next_label("avals_assoc_reuse_mixed");
                        let store_box = ctx.next_label("avals_assoc_store_mixed");
                        emitter.instruction("cmp x5, #7");                      // does this associative-array entry already store a boxed mixed value?
                        emitter.instruction(&format!("b.eq {}", reuse_box));    // reuse existing mixed boxes instead of nesting them
                        super::super::super::emit_box_runtime_payload_as_mixed(emitter, "x5", "x3", "x4"); // box the borrowed associative-array payload into an owned mixed cell
                        emitter.instruction(&format!("b {}", store_box));       // skip the mixed-box reuse path once boxing is done
                        emitter.label(&reuse_box);
                        emitter.instruction("mov x0, x3");                      // move the existing mixed box pointer into the incref helper input register
                        emitter.instruction("bl __rt_incref");                  // retain the shared mixed box for the result values array
                        emitter.label(&store_box);
                        emitter.instruction("ldr x9, [sp, #16]");               // load the result values array pointer from the fixed stack layout
                        emitter.instruction("ldr x10, [x9]");                   // load the current result values array length before appending one more value
                        emitter.instruction("add x11, x9, #24");                // point at the result values array payload region just after the fixed header
                        emitter.instruction("str x0, [x11, x10, lsl #3]");      // store the owned mixed box pointer into the next result values slot
                        emitter.instruction("add x10, x10, #1");                // increment the result values array length after storing one more mixed box
                        emitter.instruction("str x10, [x9]");                   // persist the updated result values array length in the header
                    }
                    _ => {
                        if val_ty.is_refcounted() {
                            emitter.instruction("mov x0, x3");                  // move the borrowed heap pointer into the incref helper input register before the result array stores it
                            emitter.instruction("bl __rt_incref");              // retain the borrowed heap value for the result values array
                        }
                        emitter.instruction("ldr x9, [sp, #16]");               // load the result values array pointer from the fixed stack layout
                        emitter.instruction("ldr x10, [x9]");                   // load the current result values array length before appending one more value
                        emitter.instruction("add x11, x9, #24");                // point at the result values array payload region just after the fixed header
                        if val_ty.is_refcounted() {
                            emitter.instruction("str x0, [x11, x10, lsl #3]");  // store the retained heap pointer into the next result values slot after incref
                        } else {
                            emitter.instruction("str x3, [x11, x10, lsl #3]");  // store the associative-array value payload into the next result values slot
                        }
                        emitter.instruction("add x10, x10, #1");                // increment the result values array length after storing one more value
                        emitter.instruction("str x10, [x9]");                   // persist the updated result values array length in the header
                    }
                }
                emitter.instruction(&format!("b {}", loop_label));              // continue collecting associative-array values until iteration completes
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");           // load the associative-array hash-table pointer for the next insertion-order iteration step
                emitter.instruction("mov rsi, QWORD PTR [rsp]");                // load the current associative-array iterator cursor
                emitter.instruction("call __rt_hash_iter_next");                // advance one associative-array insertion-order entry and return its key plus payload
                emitter.instruction("cmp rax, -1");                             // has associative-array iteration reached the done sentinel?
                emitter.instruction(&format!("je {}", end_label));              // stop once every associative-array value has been collected
                emitter.instruction("mov QWORD PTR [rsp], rax");                // save the updated associative-array iterator cursor for the next loop step

                match &val_ty {
                    PhpType::Str => {
                        emitter.instruction("mov rax, rcx");                    // move the associative-array string value pointer into the x86_64 string-persist input register
                        emitter.instruction("mov rdx, r8");                     // move the associative-array string value length into the paired x86_64 string-persist input register
                        emitter.instruction("call __rt_str_persist");           // persist the associative-array string value so the result array owns stable string storage
                        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");   // load the result values array pointer from the fixed stack layout
                        emitter.instruction("mov r11, QWORD PTR [r10]");        // load the current result values array length before appending one more value
                        emitter.instruction("mov rcx, r11");                    // copy the current result values array length before scaling it into a string-slot offset
                        emitter.instruction("shl rcx, 4");                      // convert the result values array length into a 16-byte string-slot offset
                        emitter.instruction("add rcx, r10");                    // advance from the result values array header to the selected string slot
                        emitter.instruction("add rcx, 24");                     // skip the fixed indexed-array header to land on the string payload region
                        emitter.instruction("mov QWORD PTR [rcx], rax");        // store the owned string pointer into the next result values slot
                        emitter.instruction("mov QWORD PTR [rcx + 8], rdx");    // store the owned string length into the next result values slot
                        emitter.instruction("add r11, 1");                      // increment the result values array length after storing one more string
                        emitter.instruction("mov QWORD PTR [r10], r11");        // persist the updated result values array length in the header
                    }
                    PhpType::Mixed => {
                        let reuse_box = ctx.next_label("avals_assoc_reuse_mixed");
                        let store_box = ctx.next_label("avals_assoc_store_mixed");
                        emitter.instruction("cmp r9, 7");                       // does this associative-array entry already store a boxed mixed value?
                        emitter.instruction(&format!("je {}", reuse_box));      // reuse existing mixed boxes instead of nesting them
                        super::super::super::emit_box_runtime_payload_as_mixed(emitter, "r9", "rcx", "r8"); // box the borrowed associative-array payload into an owned mixed cell
                        emitter.instruction(&format!("jmp {}", store_box));     // skip the mixed-box reuse path once boxing is done
                        emitter.label(&reuse_box);
                        emitter.instruction("mov rax, rcx");                    // move the existing mixed box pointer into the incref helper input register
                        emitter.instruction("call __rt_incref");                // retain the shared mixed box for the result values array
                        emitter.label(&store_box);
                        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");   // load the result values array pointer from the fixed stack layout
                        emitter.instruction("mov r11, QWORD PTR [r10]");        // load the current result values array length before appending one more value
                        emitter.instruction("mov QWORD PTR [r10 + r11 * 8 + 24], rax"); // store the owned mixed box pointer into the next result values slot
                        emitter.instruction("add r11, 1");                      // increment the result values array length after storing one more mixed box
                        emitter.instruction("mov QWORD PTR [r10], r11");        // persist the updated result values array length in the header
                    }
                    _ => {
                        if val_ty.is_refcounted() {
                            emitter.instruction("mov rax, rcx");                // move the borrowed heap pointer into the incref helper input register before the result array stores it
                            emitter.instruction("call __rt_incref");            // retain the borrowed heap value for the result values array
                            emitter.instruction("mov rcx, rax");                // keep the retained heap pointer in the payload register used for the final store
                        }
                        emitter.instruction("mov r10, QWORD PTR [rsp + 16]");   // load the result values array pointer from the fixed stack layout
                        emitter.instruction("mov r11, QWORD PTR [r10]");        // load the current result values array length before appending one more value
                        emitter.instruction("mov QWORD PTR [r10 + r11 * 8 + 24], rcx"); // store the associative-array value payload into the next result values slot
                        emitter.instruction("add r11, 1");                      // increment the result values array length after storing one more value
                        emitter.instruction("mov QWORD PTR [r10], r11");        // persist the updated result values array length in the header
                    }
                }
                emitter.instruction(&format!("jmp {}", loop_label));            // continue collecting associative-array values until iteration completes
            }
        }

        emitter.label(&end_label);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("add sp, sp, #16");                         // drop the associative-array iterator cursor stack slot
                emitter.instruction("ldr x0, [sp], #16");                       // pop the result values array pointer into the standard integer result register
                emitter.instruction("add sp, sp, #16");                         // drop the preserved associative-array hash-table pointer stack slot
            }
            Arch::X86_64 => {
                emitter.instruction("add rsp, 16");                             // drop the associative-array iterator cursor stack slot
                emitter.instruction("mov rax, QWORD PTR [rsp]");                // move the result values array pointer into the standard integer result register
                emitter.instruction("add rsp, 16");                             // drop the preserved result values array pointer after loading it into the result register
                emitter.instruction("add rsp, 16");                             // drop the preserved associative-array hash-table pointer stack slot
            }
        }

        return Some(PhpType::Array(Box::new(val_ty)));
    }

    // -- indexed array: array_values is a no-op, but the call still returns a new alias --
    abi::emit_incref_if_refcounted(emitter, &arr_ty);                          // retain the borrowed indexed array because function-call expressions are treated as owned results by callers
    Some(arr_ty)
}
