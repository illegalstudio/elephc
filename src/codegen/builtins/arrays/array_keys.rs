//! Purpose:
//! Emits PHP `array_keys` builtin calls over associative or key-aware array data.
//! Owns key/value payload setup and runtime hash-helper invocation for array results or lookups.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Array key typing and Mixed payload tags must match the runtime hash-table representation.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `array_keys($arr)` builtin — returns all keys of an array.
///
/// For `PhpType::AssocArray` keys, iterates the hash table in insertion order,
/// persists string keys via `__rt_str_persist`, boxes mixed keys via `__rt_mixed_from_value`,
/// and returns an `Array<key_type>`. For indexed arrays, allocates `Array<Int>` with keys
/// `[0, 1, …, length-1]` via a counted loop. Both paths preserve ABI register conventions
/// per target (x86_64: rax/rdi/rsi, ARM64: x0/x1/x2) and push/pop preserved registers
/// across runtime helper calls. Stack layout (assoc path): `[iter_index(16)] [result_array(16)] [hash_ptr(16)]`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_keys()");
    let arr_ty = emit_expr(&args[0], emitter, ctx, data);
    emit_loaded_keys(&arr_ty, emitter, ctx)
}

/// Emits assembly for loaded keys.
pub(crate) fn emit_loaded_keys(
    arr_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> Option<PhpType> {
    if let PhpType::AssocArray { key, .. } = &arr_ty {
        let key_ty = *key.clone();
        let assoc_key_elem_size = if matches!(key_ty, PhpType::Str) { 16 } else { 8 };
        // -- associative array: iterate hash table and collect normalized PHP keys --
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the associative-array hash-table pointer while allocating the result array

        // -- allocate new array for keys --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [x0]");                            // load the associative-array entry count to size the result keys array exactly
                emitter.instruction(&format!("mov x1, #{}", assoc_key_elem_size)); // materialize the result key element width for associative-array keys
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, QWORD PTR [rax]");                // load the associative-array entry count to size the result keys array exactly
                emitter.instruction(&format!("mov rsi, {}", assoc_key_elem_size)); // materialize the result key element width for associative-array keys
            }
        }
        abi::emit_call_label(emitter, "__rt_array_new");                        // allocate the result keys array with exact associative-array capacity
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the result keys array pointer across associative-array iteration

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

        let loop_label = ctx.next_label("akeys_assoc_loop");
        let end_label = ctx.next_label("akeys_assoc_end");
        emitter.label(&loop_label);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [sp, #32]");                       // load the associative-array hash-table pointer for the next insertion-order iteration step
                emitter.instruction("ldr x1, [sp]");                            // load the current associative-array iterator cursor
                emitter.instruction("bl __rt_hash_iter_next");                  // advance one associative-array insertion-order entry and return its key plus payload
                emitter.instruction("cmn x0, #1");                              // has associative-array iteration reached the done sentinel?
                emitter.instruction(&format!("b.eq {}", end_label));            // stop once every associative-array key has been collected
                emitter.instruction("str x0, [sp]");                            // save the updated associative-array iterator cursor for the next loop step
                emitter.instruction("ldr x9, [sp, #16]");                       // load the result keys array pointer from the fixed stack layout
                emitter.instruction("ldr x10, [x9]");                           // load the current result keys array length before appending one more key
                match &key_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction("add x11, x9, #24");                // point at the integer-key result payload region
                        emitter.instruction("str x1, [x11, x10, lsl #3]");      // store the normalized integer key into the next result keys slot
                    }
                    PhpType::Str => {
                        emitter.instruction("stp x9, x10, [sp, #-16]!");        // preserve result array pointer and length across key persistence
                        emitter.instruction("bl __rt_str_persist");             // copy the borrowed hash key so array_keys() owns its string result
                        emitter.instruction("ldp x9, x10, [sp], #16");          // restore result array pointer and length after key persistence
                        emitter.instruction("lsl x11, x10, #4");                // convert the result keys array length into a 16-byte string-slot offset
                        emitter.instruction("add x11, x9, x11");                // advance from the result keys array header to the selected string slot
                        emitter.instruction("add x11, x11, #24");               // skip the fixed indexed-array header to land on the string payload region
                        emitter.instruction("str x1, [x11]");                   // store the owned associative-array key pointer into the next result keys slot
                        emitter.instruction("str x2, [x11, #8]");               // store the owned associative-array key length into the next result keys slot
                    }
                    _ => {
                        let key_string = ctx.next_label("akeys_assoc_key_string");
                        let key_boxed = ctx.next_label("akeys_assoc_key_boxed");
                        emitter.instruction("stp x9, x10, [sp, #-16]!");        // preserve result array pointer and length across mixed key boxing
                        emitter.instruction("cmn x2, #1");                      // check whether this associative-array key is stored as an integer
                        emitter.instruction(&format!("b.ne {}", key_string));   // string keys need string-tagged mixed boxing
                        emitter.instruction("mov x0, #0");                      // runtime tag 0 = integer key
                        emitter.instruction("mov x2, xzr");                     // integer mixed payloads do not use the high word
                        emitter.instruction("bl __rt_mixed_from_value");        // box the integer key into an owned mixed cell
                        emitter.instruction(&format!("b {}", key_boxed));       // skip the string-key boxing path
                        emitter.label(&key_string);
                        emitter.instruction("mov x0, #1");                      // runtime tag 1 = string key
                        emitter.instruction("bl __rt_mixed_from_value");        // persist and box the string key into an owned mixed cell
                        emitter.label(&key_boxed);
                        emitter.instruction("ldp x9, x10, [sp], #16");          // restore result array pointer and length after mixed key boxing
                        emitter.instruction("add x11, x9, #24");                // point at the mixed-key result payload region
                        emitter.instruction("str x0, [x11, x10, lsl #3]");      // store the boxed mixed key pointer into the next result keys slot
                    }
                }
                emitter.instruction("add x10, x10, #1");                        // increment the result keys array length after storing one more key
                emitter.instruction("str x10, [x9]");                           // persist the updated result keys array length in the header
                emitter.instruction(&format!("b {}", loop_label));              // continue collecting associative-array keys until iteration completes
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");           // load the associative-array hash-table pointer for the next insertion-order iteration step
                emitter.instruction("mov rsi, QWORD PTR [rsp]");                // load the current associative-array iterator cursor
                emitter.instruction("call __rt_hash_iter_next");                // advance one associative-array insertion-order entry and return its key plus payload
                emitter.instruction("cmp rax, -1");                             // has associative-array iteration reached the done sentinel?
                emitter.instruction(&format!("je {}", end_label));              // stop once every associative-array key has been collected
                emitter.instruction("mov QWORD PTR [rsp], rax");                // save the updated associative-array iterator cursor for the next loop step
                emitter.instruction("mov r10, QWORD PTR [rsp + 16]");           // load the result keys array pointer from the fixed stack layout
                emitter.instruction("mov r11, QWORD PTR [r10]");                // load the current result keys array length before appending one more key
                match &key_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction("mov QWORD PTR [r10 + r11 * 8 + 24], rdi"); // store the normalized integer key into the next result keys slot
                    }
                    PhpType::Str => {
                        emitter.instruction("sub rsp, 16");                     // reserve a temporary slot for result array state during key persistence
                        emitter.instruction("mov QWORD PTR [rsp], r10");        // preserve the result keys array pointer across key persistence
                        emitter.instruction("mov QWORD PTR [rsp + 8], r11");    // preserve the current result keys array length across key persistence
                        emitter.instruction("mov rax, rdi");                    // move the borrowed hash key pointer into the string-persist helper input register
                        emitter.instruction("call __rt_str_persist");           // copy the borrowed hash key so array_keys() owns its string result
                        emitter.instruction("mov r10, QWORD PTR [rsp]");        // restore the result keys array pointer after key persistence
                        emitter.instruction("mov r11, QWORD PTR [rsp + 8]");    // restore the current result keys array length after key persistence
                        emitter.instruction("add rsp, 16");                     // release the temporary result-array state slot
                        emitter.instruction("mov rcx, r11");                    // copy the current result keys array length before scaling it into a string-slot offset
                        emitter.instruction("shl rcx, 4");                      // convert the result keys array length into a 16-byte string-slot offset
                        emitter.instruction("add rcx, r10");                    // advance from the result keys array header to the selected string slot
                        emitter.instruction("add rcx, 24");                     // skip the fixed indexed-array header to land on the string payload region
                        emitter.instruction("mov QWORD PTR [rcx], rax");        // store the owned associative-array key pointer into the next result keys slot
                        emitter.instruction("mov QWORD PTR [rcx + 8], rdx");    // store the owned associative-array key length into the next result keys slot
                    }
                    _ => {
                        let key_string = ctx.next_label("akeys_assoc_key_string");
                        let key_boxed = ctx.next_label("akeys_assoc_key_boxed");
                        emitter.instruction("sub rsp, 16");                     // reserve a temporary slot for result array state during mixed key boxing
                        emitter.instruction("mov QWORD PTR [rsp], r10");        // preserve the result keys array pointer across mixed key boxing
                        emitter.instruction("mov QWORD PTR [rsp + 8], r11");    // preserve the current result keys array length across mixed key boxing
                        emitter.instruction("cmp rdx, -1");                     // check whether this associative-array key is stored as an integer
                        emitter.instruction(&format!("jne {}", key_string));    // string keys need string-tagged mixed boxing
                        emitter.instruction("xor esi, esi");                    // integer mixed payloads do not use the high word
                        emitter.instruction("mov eax, 0");                      // runtime tag 0 = integer key
                        emitter.instruction("call __rt_mixed_from_value");      // box the integer key into an owned mixed cell
                        emitter.instruction(&format!("jmp {}", key_boxed));     // skip the string-key boxing path
                        emitter.label(&key_string);
                        emitter.instruction("mov rsi, rdx");                    // move the string key length into the mixed helper high-word register
                        emitter.instruction("mov eax, 1");                      // runtime tag 1 = string key
                        emitter.instruction("call __rt_mixed_from_value");      // persist and box the string key into an owned mixed cell
                        emitter.label(&key_boxed);
                        emitter.instruction("mov r10, QWORD PTR [rsp]");        // restore the result keys array pointer after mixed key boxing
                        emitter.instruction("mov r11, QWORD PTR [rsp + 8]");    // restore the current result keys array length after mixed key boxing
                        emitter.instruction("add rsp, 16");                     // release the temporary result-array state slot
                        emitter.instruction("mov QWORD PTR [r10 + r11 * 8 + 24], rax"); // store the boxed mixed key pointer into the next result keys slot
                    }
                }
                emitter.instruction("add r11, 1");                              // increment the result keys array length after storing one more key
                emitter.instruction("mov QWORD PTR [r10], r11");                // persist the updated result keys array length in the header
                emitter.instruction(&format!("jmp {}", loop_label));            // continue collecting associative-array keys until iteration completes
            }
        }

        emitter.label(&end_label);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("add sp, sp, #16");                         // drop the associative-array iterator cursor stack slot
                emitter.instruction("ldr x0, [sp], #16");                       // pop the result keys array pointer into the standard integer result register
                emitter.instruction("add sp, sp, #16");                         // drop the preserved associative-array hash-table pointer stack slot
            }
            Arch::X86_64 => {
                emitter.instruction("add rsp, 16");                             // drop the associative-array iterator cursor stack slot
                emitter.instruction("mov rax, QWORD PTR [rsp]");                // move the result keys array pointer into the standard integer result register
                emitter.instruction("add rsp, 16");                             // drop the preserved result keys array pointer after loading it into the result register
                emitter.instruction("add rsp, 16");                             // drop the preserved associative-array hash-table pointer stack slot
            }
        }

        return Some(PhpType::Array(Box::new(key_ty)));
    }

    // -- indexed array: return [0, 1, 2, ...] --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // load the source array length so the indexed keys result can be allocated exactly
            emitter.instruction("str x9, [sp, #-16]!");                         // preserve the source array length on the stack for the loop bound and final length store
            emitter.instruction("mov x0, x9");                                  // pass the source array length as the exact result array capacity
            emitter.instruction("mov x1, #8");                                  // integer key arrays use 8-byte scalar payload slots
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // load the source array length so the indexed keys result can be allocated exactly
            emitter.instruction("sub rsp, 16");                                 // reserve one temporary stack slot for the indexed keys loop bound
            emitter.instruction("mov QWORD PTR [rsp], r10");                    // preserve the source array length on the stack for the loop bound and final length store
            emitter.instruction("mov rdi, r10");                                // pass the source array length as the exact result array capacity
            emitter.instruction("mov rsi, 8");                                  // integer key arrays use 8-byte scalar payload slots
        }
    }
    abi::emit_call_label(emitter, "__rt_array_new");                            // allocate the indexed keys result array with exact source-array capacity
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the indexed keys result array pointer across the fill loop
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str xzr, [sp, #-16]!");                        // push the initial indexed keys loop counter
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one temporary stack slot for the indexed keys loop counter
            emitter.instruction("mov QWORD PTR [rsp], 0");                      // initialize the indexed keys loop counter to zero
        }
    }
    let loop_label = ctx.next_label("akeys_loop");
    let end_label = ctx.next_label("akeys_end");
    emitter.label(&loop_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x12, [sp]");                               // load the current indexed keys loop counter from the stack
            emitter.instruction("ldr x9, [sp, #32]");                           // reload the source array length from the fixed stack layout
            emitter.instruction("cmp x12, x9");                                 // have we written every integer key from 0 to length - 1?
            emitter.instruction(&format!("b.ge {}", end_label));                // stop once the indexed keys array is fully materialized
            emitter.instruction("ldr x0, [sp, #16]");                           // load the result keys array pointer from the fixed stack layout
            emitter.instruction("add x10, x0, #24");                            // point at the indexed-array payload region just after the fixed header
            emitter.instruction("str x12, [x10, x12, lsl #3]");                 // store the current loop counter as the next integer key payload
            emitter.instruction("add x12, x12, #1");                            // increment the indexed keys loop counter after storing one more key
            emitter.instruction("str x12, [sp]");                               // persist the updated indexed keys loop counter for the next iteration
            emitter.instruction(&format!("b {}", loop_label));                  // continue filling the indexed keys result array
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // load the current indexed keys loop counter from the stack
            emitter.instruction("mov r11, QWORD PTR [rsp + 32]");               // reload the source array length from the fixed stack layout
            emitter.instruction("cmp r10, r11");                                // have we written every integer key from 0 to length - 1?
            emitter.instruction(&format!("jge {}", end_label));                 // stop once the indexed keys array is fully materialized
            emitter.instruction("mov rcx, QWORD PTR [rsp + 16]");               // load the result keys array pointer from the fixed stack layout
            emitter.instruction("mov QWORD PTR [rcx + r10 * 8 + 24], r10");     // store the current loop counter as the next integer key payload
            emitter.instruction("add r10, 1");                                  // increment the indexed keys loop counter after storing one more key
            emitter.instruction("mov QWORD PTR [rsp], r10");                    // persist the updated indexed keys loop counter for the next iteration
            emitter.instruction(&format!("jmp {}", loop_label));                // continue filling the indexed keys result array
        }
    }
    emitter.label(&end_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("add sp, sp, #16");                             // drop the indexed keys loop counter stack slot
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the result keys array pointer before finalizing its logical length
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the exact source array length from the remaining stack layout
            emitter.instruction("str x9, [x0]");                                // stamp the indexed keys result array length once the payload slots are filled
            emitter.instruction("ldr x0, [sp], #16");                           // pop the finalized result keys array pointer into the standard integer result register
            emitter.instruction("add sp, sp, #16");                             // drop the preserved source array length stack slot
        }
        Arch::X86_64 => {
            emitter.instruction("add rsp, 16");                                 // drop the indexed keys loop counter stack slot
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // reload the result keys array pointer before finalizing its logical length
            emitter.instruction("mov r10, QWORD PTR [rsp + 16]");               // reload the exact source array length from the remaining stack layout
            emitter.instruction("mov QWORD PTR [rax], r10");                    // stamp the indexed keys result array length once the payload slots are filled
            emitter.instruction("add rsp, 16");                                 // drop the preserved result keys array pointer after loading it into the result register
            emitter.instruction("add rsp, 16");                                 // drop the preserved source array length stack slot
        }
    }

    Some(PhpType::Array(Box::new(PhpType::Int)))
}
