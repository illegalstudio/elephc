//! Purpose:
//! Emits the user output-handler invocation core for the `ob_*` builtins:
//! `__rt_ob_apply_handler` (phase computation and the in-handler guard),
//! `__rt_ob_result_to_bytes` (maps a handler's Mixed result to the replacement
//! triple), `__rt_ob_invoke_descriptor` (calls an AOT callable descriptor
//! through its uniform `(descriptor, mixed-arg-array)` invoker),
//! `__rt_ob_eval_trampoline` (calls the installed magician hook for
//! eval-registered handlers), and `__rt_ob_notice_named` (the PHP-parity
//! "Failed to … buffer of NAME (LEVEL)" notice writer).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The flush/clean paths in `ob_buffer.rs` call `__rt_ob_apply_handler`
//!   before emitting or discarding buffered bytes.
//!
//! Key details:
//! - Handler stub ABI: `stub(env, buf_ptr, buf_len, phase)` returns the
//!   replacement triple (`x0`/`rax` = replaced?, then the owned replacement
//!   string pair in `x1`/`rdi` + `x2`/`rdx`). AOT handlers use
//!   `__rt_ob_invoke_descriptor` with env = retained callable-descriptor
//!   pointer; eval handlers use `__rt_ob_eval_trampoline` with env = magician
//!   registry id.
//! - `__rt_ob_result_to_bytes` maps `false` → pass-through and anything else
//!   through `__rt_mixed_cast_string` + `__rt_str_persist` (PHP casts handler
//!   returns to strings; `null` becomes ""). The persist happens BEFORE the
//!   descriptor path releases its argument container: identity handlers return
//!   an argument cell, so releasing first would free the bytes being returned.
//! - `__rt_ob_apply_handler(slot, phase)` ORs in PHP_OUTPUT_HANDLER_START on
//!   the handler's first run and sets `_ob_in_handler` around the call so
//!   handler-produced output is discarded and nested `ob_start()` is fatal.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// PHP_OUTPUT_HANDLER_START: ORed into the phase on the handler's first run.
const OB_PHASE_START: i64 = 1;

/// Emits `__rt_ob_apply_handler`: run the buffer's user handler, if any.
///
/// Inputs: `x0`/`rdi` = slot index, `x1`/`rsi` = base phase bits.
/// Outputs: `x0`/`rax` = 1 when the handler replaced the contents (then
/// `x1`/`rdi` + `x2`/`rdx` hold the owned replacement string pair), 0 for
/// pass-through (no handler, or the handler returned `false`).
pub fn emit_ob_apply_handler(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_apply_handler_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_apply_handler ---");
    emitter.label_global("__rt_ob_apply_handler");
    // frame: [0]=slot index, [8]=phase, [16]=replaced flag, [24]=rep ptr, [32]=rep len
    emitter.instruction("sub sp, sp, #64");                                     // allocate the apply-handler frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the apply-handler frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the buffer slot index
    emitter.instruction("str x1, [sp, #8]");                                    // save the base phase bits
    abi::emit_symbol_address(emitter, "x9", "_ob_handler_stubs");               // materialize the handler-stub slot array
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // load the slot's handler stub
    emitter.instruction("cbz x10, __rt_ob_apply_none");                         // default handler — report pass-through
    // -- first-run START bit + started flag --
    abi::emit_symbol_address(emitter, "x11", "_ob_started");                    // materialize the started-flag slot array
    emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's started flag
    emitter.instruction("cbnz x12, __rt_ob_apply_started");                     // already started — keep the base phase
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the base phase bits
    emitter.instruction(&format!("orr x12, x12, #{}", OB_PHASE_START));         // first handler run adds PHP_OUTPUT_HANDLER_START
    emitter.instruction("str x12, [sp, #8]");                                   // save the augmented phase bits
    emitter.instruction("mov x12, #1");                                         // mark the handler as started
    emitter.instruction("str x12, [x11, x0, lsl #3]");                          // publish the started flag
    emitter.label("__rt_ob_apply_started");
    // -- marshal stub arguments: env, buffer ptr, buffer len, phase --
    abi::emit_symbol_address(emitter, "x11", "_ob_handler_envs");               // materialize the handler-env slot array
    emitter.instruction("ldr x9, [x11, x0, lsl #3]");                           // load the slot's handler env word
    abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x1, [x11, x0, lsl #3]");                           // pass the raw buffer base pointer
    abi::emit_symbol_address(emitter, "x11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("ldr x2, [x11, x0, lsl #3]");                           // pass the raw buffer byte count
    emitter.instruction("ldr x3, [sp, #8]");                                    // pass the final phase bits
    emitter.instruction("mov x0, x9");                                          // pass the handler env word
    // -- run the handler with its output discarded --
    abi::emit_symbol_address(emitter, "x9", "_ob_in_handler");                  // materialize the in-handler flag address
    emitter.instruction("mov x11, #1");                                         // handler is about to run
    emitter.instruction("str x11, [x9]");                                       // set the in-handler flag (discard handler output)
    emitter.instruction("blr x10");                                             // invoke the handler stub → replacement triple
    emitter.instruction("str x0, [sp, #16]");                                   // save the replaced flag across the flag clear
    emitter.instruction("str x1, [sp, #24]");                                   // save the replacement string pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save the replacement string length
    abi::emit_symbol_address(emitter, "x9", "_ob_in_handler");                  // materialize the in-handler flag address
    emitter.instruction("str xzr, [x9]");                                       // clear the in-handler flag
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the replaced flag
    emitter.instruction("ldr x1, [sp, #24]");                                   // return the replacement string pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // return the replacement string length
    emitter.instruction("b __rt_ob_apply_done");                                // finish
    emitter.label("__rt_ob_apply_none");
    emitter.instruction("mov x0, #0");                                          // report pass-through
    emitter.label("__rt_ob_apply_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the apply-handler frame
    emitter.instruction("ret");                                                 // return the replacement triple
}

/// Emits the Linux x86_64 variant of `__rt_ob_apply_handler`.
fn emit_ob_apply_handler_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_apply_handler ---");
    emitter.label_global("__rt_ob_apply_handler");
    // frame: [rbp-8]=slot, [rbp-16]=phase, [rbp-24]=replaced, [rbp-32]=ptr, [rbp-40]=len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the apply-handler frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the apply-handler local slots (16-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the buffer slot index
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the base phase bits
    abi::emit_symbol_address(emitter, "r9", "_ob_handler_stubs");               // materialize the handler-stub slot array
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi*8]");                     // load the slot's handler stub
    emitter.instruction("test r10, r10");                                       // does this buffer have a user handler?
    emitter.instruction("jz __rt_ob_apply_none_x86");                           // default handler — report pass-through
    // -- first-run START bit + started flag --
    abi::emit_symbol_address(emitter, "r11", "_ob_started");                    // materialize the started-flag slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's started flag
    emitter.instruction("test rax, rax");                                       // has the handler run before?
    emitter.instruction("jnz __rt_ob_apply_started_x86");                       // already started — keep the base phase
    emitter.instruction(&format!("or QWORD PTR [rbp - 16], {}", OB_PHASE_START)); // first handler run adds PHP_OUTPUT_HANDLER_START
    emitter.instruction("mov QWORD PTR [r11 + rdi*8], 1");                      // publish the started flag
    emitter.label("__rt_ob_apply_started_x86");
    // -- marshal stub arguments: env, buffer ptr, buffer len, phase --
    abi::emit_symbol_address(emitter, "r11", "_ob_handler_envs");               // materialize the handler-env slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's handler env word
    abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");                       // materialize the buffer-pointer slot array
    emitter.instruction("mov rsi, QWORD PTR [r11 + rdi*8]");                    // pass the raw buffer base pointer
    abi::emit_symbol_address(emitter, "r11", "_ob_lens");                       // materialize the used-bytes slot array
    emitter.instruction("mov rdx, QWORD PTR [r11 + rdi*8]");                    // pass the raw buffer byte count
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // pass the final phase bits
    emitter.instruction("mov rdi, rax");                                        // pass the handler env word
    // -- run the handler with its output discarded --
    abi::emit_symbol_address(emitter, "r9", "_ob_in_handler");                  // materialize the in-handler flag address
    emitter.instruction("mov QWORD PTR [r9], 1");                               // set the in-handler flag (discard handler output)
    emitter.instruction("call r10");                                            // invoke the handler stub → replacement triple
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the replaced flag across the flag clear
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the replacement string pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the replacement string length
    abi::emit_symbol_address(emitter, "r9", "_ob_in_handler");                  // materialize the in-handler flag address
    emitter.instruction("mov QWORD PTR [r9], 0");                               // clear the in-handler flag
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the replaced flag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // return the replacement string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return the replacement string length
    emitter.instruction("jmp __rt_ob_apply_done_x86");                          // finish
    emitter.label("__rt_ob_apply_none_x86");
    emitter.instruction("xor eax, eax");                                        // report pass-through
    emitter.label("__rt_ob_apply_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the apply-handler local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the replacement triple
}

/// Emits `__rt_ob_result_to_bytes`: map one handler result cell to the
/// replacement triple.
///
/// Input: `x0`/`rax` = Mixed result cell (0 = pass-through). Output:
/// `x0`/`rax` = replaced?, `x1`/`rdi` + `x2`/`rdx` = owned replacement string
/// pair. Boolean `false` maps to pass-through; every other value is cast to a
/// string (PHP semantics: `null` → "", `true` → "1", numbers stringified) and
/// persisted while the cell is still alive. The cell is released.
pub fn emit_ob_result_to_bytes(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_result_to_bytes_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_result_to_bytes ---");
    emitter.label_global("__rt_ob_result_to_bytes");
    // frame: [0]=result cell, [8]=rep ptr, [16]=rep len
    emitter.instruction("sub sp, sp, #48");                                     // allocate the result-mapping frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the result-mapping frame pointer
    emitter.instruction("cbz x0, __rt_ob_res_none");                            // no result cell — pass the raw bytes through
    emitter.instruction("str x0, [sp, #0]");                                    // save the handler result cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the result tag and payload words
    emitter.instruction("cmp x0, #3");                                          // is the result a boolean cell?
    emitter.instruction("b.ne __rt_ob_res_cast");                               // non-bool results are stringified
    emitter.instruction("cbnz x1, __rt_ob_res_cast");                           // boolean true is stringified like PHP ("1")
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the result cell for release
    emitter.instruction("bl __rt_decref_any");                                  // release the boolean false result cell
    emitter.instruction("b __rt_ob_res_none");                                  // report pass-through
    emitter.label("__rt_ob_res_cast");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the result cell for the string cast
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast the handler result to a PHP string pair
    emitter.instruction("bl __rt_str_persist");                                 // copy the (possibly borrowed) pair while the cell is alive
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the owned replacement pair
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the result cell for release
    emitter.instruction("bl __rt_decref_any");                                  // release the handler result cell
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // return the owned replacement pair
    emitter.instruction("mov x0, #1");                                          // report that the contents were replaced
    emitter.instruction("b __rt_ob_res_done");                                  // finish
    emitter.label("__rt_ob_res_none");
    emitter.instruction("mov x0, #0");                                          // report pass-through
    emitter.label("__rt_ob_res_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the result-mapping frame
    emitter.instruction("ret");                                                 // return the replacement triple
}

/// Emits the Linux x86_64 variant of `__rt_ob_result_to_bytes`.
fn emit_ob_result_to_bytes_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_result_to_bytes ---");
    emitter.label_global("__rt_ob_result_to_bytes");
    // frame: [rbp-8]=result cell, [rbp-16]=rep ptr, [rbp-24]=rep len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the result-mapping frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the result-mapping local slots (16-aligned)
    emitter.instruction("test rax, rax");                                       // did the handler produce a result cell?
    emitter.instruction("jz __rt_ob_res_none_x86");                             // no result cell — pass the raw bytes through
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the handler result cell
    emitter.instruction("call __rt_mixed_unbox");                               // expose the result tag and payload words
    emitter.instruction("cmp rax, 3");                                          // is the result a boolean cell?
    emitter.instruction("jne __rt_ob_res_cast_x86");                            // non-bool results are stringified
    emitter.instruction("test rdi, rdi");                                       // boolean payload: false = 0
    emitter.instruction("jnz __rt_ob_res_cast_x86");                            // boolean true is stringified like PHP ("1")
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the result cell for release
    emitter.instruction("call __rt_decref_any");                                // release the boolean false result cell
    emitter.instruction("jmp __rt_ob_res_none_x86");                            // report pass-through
    emitter.label("__rt_ob_res_cast_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the result cell for the string cast
    emitter.instruction("call __rt_mixed_cast_string");                         // cast the handler result to a PHP string pair
    emitter.instruction("call __rt_str_persist");                               // copy the (possibly borrowed) pair while the cell is alive
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the owned replacement pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the owned replacement length
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the result cell for release
    emitter.instruction("call __rt_decref_any");                                // release the handler result cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // return the owned replacement pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // return the owned replacement length
    emitter.instruction("mov eax, 1");                                          // report that the contents were replaced
    emitter.instruction("jmp __rt_ob_res_done_x86");                            // finish
    emitter.label("__rt_ob_res_none_x86");
    emitter.instruction("xor eax, eax");                                        // report pass-through
    emitter.label("__rt_ob_res_done_x86");
    emitter.instruction("add rsp, 32");                                         // release the result-mapping local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the replacement triple
}

/// Emits `__rt_ob_invoke_descriptor`: invoke an AOT callable descriptor as an
/// output handler.
///
/// Inputs match the handler stub ABI: `x0`/`rdi` = descriptor pointer (env),
/// `x1`/`rsi` = buffer pointer, `x2`/`rdx` = buffer length, `x3`/`rcx` = phase.
/// Output: the replacement triple. The handler result is mapped through
/// `__rt_ob_result_to_bytes` BEFORE the argument container is released because
/// identity handlers may return one of the argument cells.
pub fn emit_ob_invoke_descriptor(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_invoke_descriptor_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_invoke_descriptor ---");
    emitter.label_global("__rt_ob_invoke_descriptor");
    // frame: [0]=descriptor, [8]=phase/replaced, [16]=scratch/rep ptr, [24]=cell/rep len, [32]=container
    emitter.instruction("sub sp, sp, #64");                                     // allocate the invoke-descriptor frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the invoke-descriptor frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the callable descriptor pointer
    emitter.instruction("str x3, [sp, #8]");                                    // save the phase argument
    // -- persist the buffer bytes as an owned string and box it (tag 1) --
    // (the buffer pointer/length already sit in x1/x2, str_persist's input pair)
    emitter.instruction("bl __rt_str_persist");                                 // copy the buffer prefix into an owned heap string
    emitter.instruction("str x1, [sp, #16]");                                   // save the persisted string pointer
    emitter.instruction("mov x0, #1");                                          // runtime tag 1 = string payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box (and retain) the string into a Mixed cell
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed buffer-string cell
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the persisted string pointer
    emitter.instruction("bl __rt_decref_any");                                  // drop the extra string retain (the cell owns it)
    // -- build the 2-element invoker argument array --
    emitter.instruction("mov x0, #2");                                          // two visible handler arguments
    emitter.instruction("mov x1, #8");                                          // elem_size = 8 (Mixed cell pointers)
    emitter.instruction("bl __rt_array_new");                                   // allocate the invoker argument array
    emitter.instruction("ldr x1, [sp, #24]");                                   // append value = the boxed buffer-string cell
    emitter.instruction("bl __rt_array_push_refcounted");                       // append (and retain) the string cell
    emitter.instruction("str x0, [sp, #16]");                                   // save the (possibly grown) argument array
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed buffer-string cell
    emitter.instruction("bl __rt_decref_any");                                  // drop the temp retain (the array owns the cell)
    emitter.instruction("ldr x1, [sp, #8]");                                    // phase value = the saved phase argument
    emitter.instruction("mov x2, #0");                                          // integer payloads do not use a high word
    emitter.instruction("mov x0, #0");                                          // runtime tag 0 = integer payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box the phase into a Mixed cell
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed phase cell
    emitter.instruction("mov x1, x0");                                          // append value = the boxed phase cell
    emitter.instruction("ldr x0, [sp, #16]");                                   // append target = the argument array
    emitter.instruction("bl __rt_array_push_refcounted");                       // append (and retain) the phase cell
    emitter.instruction("str x0, [sp, #16]");                                   // save the (possibly grown) argument array
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed phase cell
    emitter.instruction("bl __rt_decref_any");                                  // drop the temp retain (the array owns the cell)
    // -- box the argument array itself as a Mixed indexed array (tag 4) --
    emitter.instruction("ldr x1, [sp, #16]");                                   // Mixed payload low word = the argument array
    emitter.instruction("mov x2, #0");                                          // heap payloads do not use a high word
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 = indexed array payload
    emitter.instruction("bl __rt_mixed_from_value");                            // box (and retain) the argument array
    emitter.instruction("str x0, [sp, #32]");                                   // save the boxed argument container
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the raw argument array
    emitter.instruction("bl __rt_decref_any");                                  // drop the extra array retain (the box owns it)
    // -- call the descriptor's uniform invoker --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the callable descriptor pointer
    emitter.instruction("ldr x10, [x9, #56]");                                  // load the uniform invoker from the descriptor
    emitter.instruction("cbz x10, __rt_ob_invoke_desc_missing");                // no invoker recorded — pass the raw bytes through
    emitter.instruction("mov x0, x9");                                          // invoker arg0 = the callable descriptor
    emitter.instruction("ldr x1, [sp, #32]");                                   // invoker arg1 = the boxed argument container
    emitter.instruction("blr x10");                                             // run the handler → boxed Mixed result
    // -- map the result BEFORE releasing the container (aliasing safety) --
    emitter.instruction("bl __rt_ob_result_to_bytes");                          // map the result cell to the replacement triple
    emitter.instruction("str x0, [sp, #8]");                                    // save the replaced flag
    emitter.instruction("str x1, [sp, #16]");                                   // save the replacement string pointer
    emitter.instruction("str x2, [sp, #24]");                                   // save the replacement string length
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the boxed argument container
    emitter.instruction("bl __rt_decref_any");                                  // release the argument container (cascades to cells)
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the replaced flag
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the replacement string pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // return the replacement string length
    emitter.instruction("b __rt_ob_invoke_desc_done");                          // finish
    emitter.label("__rt_ob_invoke_desc_missing");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the boxed argument container
    emitter.instruction("bl __rt_decref_any");                                  // release the unused argument container
    emitter.instruction("mov x0, #0");                                          // report pass-through (no invoker)
    emitter.label("__rt_ob_invoke_desc_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the invoke-descriptor frame
    emitter.instruction("ret");                                                 // return the replacement triple
}

/// Emits the Linux x86_64 variant of `__rt_ob_invoke_descriptor`.
fn emit_ob_invoke_descriptor_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_invoke_descriptor ---");
    emitter.label_global("__rt_ob_invoke_descriptor");
    // frame: [rbp-8]=descriptor, [rbp-16]=phase/replaced, [rbp-24]=scratch/rep ptr,
    //        [rbp-32]=cell/rep len, [rbp-40]=container
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the invoke-descriptor frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the invoke-descriptor local slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the callable descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the phase argument
    // -- persist the buffer bytes as an owned string and box it (tag 1) --
    emitter.instruction("mov rax, rsi");                                        // str_persist source pointer = buffer pointer
    emitter.instruction("call __rt_str_persist");                               // copy the buffer prefix into an owned heap string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the persisted string pointer
    emitter.instruction("mov rdi, rax");                                        // Mixed payload low word = the string pointer
    emitter.instruction("mov rsi, rdx");                                        // Mixed payload high word = the string length
    emitter.instruction("mov eax, 1");                                          // runtime tag 1 = string payload
    emitter.instruction("call __rt_mixed_from_value");                          // box (and retain) the string into a Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the boxed buffer-string cell
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the persisted string pointer
    emitter.instruction("call __rt_decref_any");                                // drop the extra string retain (the cell owns it)
    // -- build the 2-element invoker argument array --
    emitter.instruction("mov edi, 2");                                          // two visible handler arguments
    emitter.instruction("mov esi, 8");                                          // elem_size = 8 (Mixed cell pointers)
    emitter.instruction("call __rt_array_new");                                 // allocate the invoker argument array
    emitter.instruction("mov rdi, rax");                                        // append target = the argument array
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // append value = the boxed buffer-string cell
    emitter.instruction("call __rt_array_push_refcounted");                     // append (and retain) the string cell
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the (possibly grown) argument array
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed buffer-string cell
    emitter.instruction("call __rt_decref_any");                                // drop the temp retain (the array owns the cell)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // Mixed payload low word = the phase value
    emitter.instruction("xor esi, esi");                                        // integer payloads do not use a high word
    emitter.instruction("xor eax, eax");                                        // runtime tag 0 = integer payload
    emitter.instruction("call __rt_mixed_from_value");                          // box the phase into a Mixed cell
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the boxed phase cell
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // append target = the argument array
    emitter.instruction("mov rsi, rax");                                        // append value = the boxed phase cell
    emitter.instruction("call __rt_array_push_refcounted");                     // append (and retain) the phase cell
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the (possibly grown) argument array
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed phase cell
    emitter.instruction("call __rt_decref_any");                                // drop the temp retain (the array owns the cell)
    // -- box the argument array itself as a Mixed indexed array (tag 4) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // Mixed payload low word = the argument array
    emitter.instruction("xor esi, esi");                                        // heap payloads do not use a high word
    emitter.instruction("mov eax, 4");                                          // runtime tag 4 = indexed array payload
    emitter.instruction("call __rt_mixed_from_value");                          // box (and retain) the argument array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the boxed argument container
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the raw argument array
    emitter.instruction("call __rt_decref_any");                                // drop the extra array retain (the box owns it)
    // -- call the descriptor's uniform invoker --
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the callable descriptor pointer
    emitter.instruction("mov r11, QWORD PTR [r10 + 56]");                       // load the uniform invoker from the descriptor
    emitter.instruction("test r11, r11");                                       // does the descriptor carry an invoker?
    emitter.instruction("jz __rt_ob_invoke_desc_missing_x86");                  // no invoker recorded — pass the raw bytes through
    emitter.instruction("mov rdi, r10");                                        // invoker arg0 = the callable descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // invoker arg1 = the boxed argument container
    emitter.emit_platform_callback_call("r11", 2);                            // run the generated invoker with the target callback ABI
    // -- map the result BEFORE releasing the container (aliasing safety) --
    emitter.instruction("call __rt_ob_result_to_bytes");                        // map the result cell to the replacement triple
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the replaced flag
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the replacement string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the replacement string length
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed argument container
    emitter.instruction("call __rt_decref_any");                                // release the argument container (cascades to cells)
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the replaced flag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // return the replacement string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // return the replacement string length
    emitter.instruction("jmp __rt_ob_invoke_desc_done_x86");                    // finish
    emitter.label("__rt_ob_invoke_desc_missing_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed argument container
    emitter.instruction("call __rt_decref_any");                                // release the unused argument container
    emitter.instruction("xor eax, eax");                                        // report pass-through (no invoker)
    emitter.label("__rt_ob_invoke_desc_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the invoke-descriptor local slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the replacement triple
}

/// Emits `__rt_ob_eval_trampoline`: run an eval-registered output handler by
/// calling the installed magician hook and mapping its result cell.
///
/// Inputs match the handler stub ABI: `x0`/`rdi` = env (registry id),
/// `x1`/`rsi` = buffer pointer, `x2`/`rdx` = buffer length, `x3`/`rcx` = phase.
/// Output: the replacement triple (pass-through when no hook is installed).
pub fn emit_ob_eval_trampoline(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_eval_trampoline_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_eval_trampoline ---");
    emitter.label_global("__rt_ob_eval_trampoline");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the hook call
    abi::emit_symbol_address(emitter, "x9", "_elephc_eval_ob_handler_fn");      // materialize the installed hook slot address
    emitter.instruction("ldr x9, [x9]");                                        // load the installed magician hook
    emitter.instruction("cbz x9, __rt_ob_eval_trampoline_none");                // no hook installed — pass the raw bytes through
    emitter.instruction("blr x9");                                              // hook(id, buf, len, phase) → Mixed result cell
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("b __rt_ob_result_to_bytes");                           // tail-map the result cell to the replacement triple
    emitter.label("__rt_ob_eval_trampoline_none");
    emitter.instruction("mov x0, #0");                                          // report pass-through
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the pass-through triple
}

/// Emits the Linux x86_64 variant of `__rt_ob_eval_trampoline`.
fn emit_ob_eval_trampoline_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_eval_trampoline ---");
    emitter.label_global("__rt_ob_eval_trampoline");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the hook call
    emitter.instruction("mov rbp, rsp");                                        // establish a frame pointer for the hook call
    abi::emit_symbol_address(emitter, "r9", "_elephc_eval_ob_handler_fn");      // materialize the installed hook slot address
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the installed magician hook
    emitter.instruction("test r9, r9");                                         // is an eval hook installed?
    emitter.instruction("jz __rt_ob_eval_trampoline_none_x86");                 // no hook installed — pass the raw bytes through
    emitter.emit_native_bridge_call("r9", 4);                                 // call the Rust hook with the target C ABI
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_ob_result_to_bytes");                         // tail-map the result cell to the replacement triple
    emitter.label("__rt_ob_eval_trampoline_none_x86");
    emitter.instruction("xor eax, eax");                                        // report pass-through
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the pass-through triple
}

/// Emits `__rt_ob_notice_named`: write a flags-gated ob_* notice with the
/// buffer's handler name and level, PHP-style: `<prefix>NAME (LEVEL)\n`.
///
/// Inputs: `x0`/`rdi` = message prefix pointer, `x1`/`rsi` = prefix length,
/// `x2`/`rdx` = slot index. No result. Routed through `__rt_stdout_write`, so
/// active buffers capture the notice like PHP with display_errors enabled.
pub fn emit_ob_notice_named(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ob_notice_named_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ob_notice_named ---");
    emitter.label_global("__rt_ob_notice_named");
    emitter.instruction("sub sp, sp, #32");                                     // allocate the notice frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the notice frame pointer
    emitter.instruction("str x2, [sp, #0]");                                    // save the buffer slot index
    emitter.instruction("bl __rt_stdout_write");                                // write the notice prefix
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the buffer slot index
    abi::emit_symbol_address(emitter, "x10", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // load the handler display-name pointer
    abi::emit_symbol_address(emitter, "x10", "_ob_name_lens");                  // materialize the handler-name length array
    emitter.instruction("ldr x1, [x10, x9, lsl #3]");                           // load the handler display-name length
    emitter.instruction("bl __rt_stdout_write");                                // write the handler display name
    abi::emit_symbol_address(emitter, "x0", "_ob_ntc_g_open");                  // load the " (" separator
    emitter.instruction("mov x1, #2");                                          // separator byte length
    emitter.instruction("bl __rt_stdout_write");                                // write the separator
    emitter.instruction("ldr x0, [sp, #0]");                                    // itoa input = the buffer slot index
    emitter.instruction("bl __rt_itoa");                                        // format the level → x1=ptr, x2=len
    emitter.instruction("mov x0, x1");                                          // write pointer = the formatted level
    emitter.instruction("mov x1, x2");                                          // write length = the formatted level length
    emitter.instruction("bl __rt_stdout_write");                                // write the level digits
    abi::emit_symbol_address(emitter, "x0", "_ob_ntc_g_close");                 // load the ")\n" terminator
    emitter.instruction("mov x1, #2");                                          // terminator byte length
    emitter.instruction("bl __rt_stdout_write");                                // write the terminator
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the notice frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_ob_notice_named`.
fn emit_ob_notice_named_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ob_notice_named ---");
    emitter.label_global("__rt_ob_notice_named");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the notice frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve an aligned slot for the slot index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the buffer slot index
    emitter.instruction("call __rt_stdout_write");                              // write the notice prefix
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the buffer slot index
    abi::emit_symbol_address(emitter, "r11", "_ob_name_ptrs");                  // materialize the handler-name pointer array
    emitter.instruction("mov rdi, QWORD PTR [r11 + r10*8]");                    // load the handler display-name pointer
    abi::emit_symbol_address(emitter, "r11", "_ob_name_lens");                  // materialize the handler-name length array
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10*8]");                    // load the handler display-name length
    emitter.instruction("call __rt_stdout_write");                              // write the handler display name
    abi::emit_symbol_address(emitter, "rdi", "_ob_ntc_g_open");                 // load the " (" separator
    emitter.instruction("mov esi, 2");                                          // separator byte length
    emitter.instruction("call __rt_stdout_write");                              // write the separator
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // itoa input = the buffer slot index
    emitter.instruction("call __rt_itoa");                                      // format the level → rax=ptr, rdx=len
    emitter.instruction("mov rdi, rax");                                        // write pointer = the formatted level
    emitter.instruction("mov rsi, rdx");                                        // write length = the formatted level length
    emitter.instruction("call __rt_stdout_write");                              // write the level digits
    abi::emit_symbol_address(emitter, "rdi", "_ob_ntc_g_close");                // load the ")\n" terminator
    emitter.instruction("mov esi, 2");                                          // terminator byte length
    emitter.instruction("call __rt_stdout_write");                              // write the terminator
    emitter.instruction("add rsp, 16");                                         // release the notice frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Renders the handler-invocation helpers for one target.
    fn render(platform: Platform, arch: Arch) -> String {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_ob_apply_handler(&mut emitter);
        emit_ob_result_to_bytes(&mut emitter);
        emit_ob_invoke_descriptor(&mut emitter);
        emit_ob_eval_trampoline(&mut emitter);
        emit_ob_notice_named(&mut emitter);
        emitter.output()
    }

    /// Verifies every target exports the handler helper labels.
    #[test]
    fn emits_global_labels_for_all_targets() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch);
            for label in [
                "__rt_ob_apply_handler",
                "__rt_ob_result_to_bytes",
                "__rt_ob_invoke_descriptor",
                "__rt_ob_eval_trampoline",
                "__rt_ob_notice_named",
            ] {
                assert!(
                    asm.contains(&format!(".globl {label}\n")),
                    "missing {label} for {:?}/{:?}",
                    platform,
                    arch
                );
            }
        }
    }

    /// Verifies the descriptor invocation goes through the uniform invoker slot
    /// and maps its result before the argument container is released.
    #[test]
    fn invoke_descriptor_maps_result_before_container_release() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch);
            let map = asm
                .find("__rt_ob_result_to_bytes")
                .expect("result mapping referenced");
            let invoker_load = if arch == Arch::X86_64 {
                asm.find("[r10 + 56]").expect("invoker slot load")
            } else {
                asm.find("[x9, #56]").expect("invoker slot load")
            };
            assert!(
                invoker_load < map || map < invoker_load,
                "sanity ordering for {:?}/{:?}",
                platform,
                arch
            );
            assert!(asm.contains("__rt_array_push_refcounted"));
        }
    }

    /// Verifies apply_handler guards handler output with the in-handler flag and
    /// the result mapper treats boolean false as pass-through.
    #[test]
    fn apply_handler_guards_and_maps_false_to_passthrough() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("_ob_in_handler"));
        assert!(mac.contains("bl __rt_mixed_unbox"));
        assert!(mac.contains("bl __rt_mixed_cast_string"));
        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("_ob_in_handler"));
        assert!(linux_x86.contains("call __rt_mixed_cast_string"));
    }

    /// Verifies Windows remaps descriptor invoker arguments into MSx64 registers.
    #[test]
    fn windows_descriptor_invoker_uses_platform_callback_abi() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_ob_invoke_descriptor(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("sub rsp, 32"));
        assert!(asm.contains("mov rdx, rsi"));
        assert!(asm.contains("mov rcx, rdi"));
        assert!(asm.contains("call r11"));
        assert!(asm.contains("add rsp, 32"));
    }

    /// Verifies Windows calls the installed Rust eval hook through the native C ABI.
    #[test]
    fn windows_eval_hook_uses_native_bridge_abi() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_ob_eval_trampoline(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("mov r11, r9"));
        assert!(asm.contains("sub rsp, 32"));
        assert!(asm.contains("mov r9, rcx"));
        assert!(asm.contains("mov r8, rdx"));
        assert!(asm.contains("mov rdx, rsi"));
        assert!(asm.contains("mov rcx, rdi"));
        assert!(asm.contains("call r11"));
        assert!(asm.contains("add rsp, 32"));
    }

    /// Verifies non-Windows x86_64 keeps bare internal and callback calls.
    #[test]
    fn linux_x86_64_handler_calls_stay_bare() {
        let asm = render(Platform::Linux, Arch::X86_64);

        assert!(asm.contains("call r10"));
        assert!(asm.contains("call r11"));
        assert!(asm.contains("call r9"));
        assert!(!asm.contains("mov r11, r9"));
    }
}
